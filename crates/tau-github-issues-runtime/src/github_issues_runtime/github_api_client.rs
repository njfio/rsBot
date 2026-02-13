use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use tau_github_issues::github_transport_helpers::{
    is_retryable_github_status, is_retryable_transport_error, parse_retry_after, retry_delay,
    truncate_for_error,
};
use tau_github_issues::issue_event_collection::{GithubIssue, GithubIssueComment};

use super::RepoRef;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct GithubCommentCreateResponse {
    pub(super) id: u64,
    pub(super) html_url: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct GithubBytesResponse {
    pub(super) bytes: Vec<u8>,
    pub(super) content_type: Option<String>,
}

#[derive(Clone)]
pub(super) struct GithubApiClient {
    http: reqwest::Client,
    api_base: String,
    repo: RepoRef,
    retry_max_attempts: usize,
    retry_base_delay_ms: u64,
}

impl GithubApiClient {
    pub(super) fn new(
        api_base: String,
        token: String,
        repo: RepoRef,
        request_timeout_ms: u64,
        retry_max_attempts: usize,
        retry_base_delay_ms: u64,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Tau-github-issues-bridge"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            reqwest::header::HeaderValue::from_static("2022-11-28"),
        );
        let auth_header = format!("Bearer {}", token.trim());
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&auth_header)
                .context("invalid github authorization header")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(request_timeout_ms.max(1)))
            .build()
            .context("failed to create github api client")?;
        Ok(Self {
            http: client,
            api_base: api_base.trim_end_matches('/').to_string(),
            repo,
            retry_max_attempts: retry_max_attempts.max(1),
            retry_base_delay_ms: retry_base_delay_ms.max(1),
        })
    }

    pub(super) async fn resolve_bot_login(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Viewer {
            login: String,
        }

        let viewer: Viewer = self
            .request_json("resolve bot login", || {
                self.http.get(format!("{}/user", self.api_base))
            })
            .await?;
        Ok(viewer.login)
    }

    pub(super) async fn list_updated_issues(
        &self,
        since: Option<&str>,
    ) -> Result<Vec<GithubIssue>> {
        let mut page = 1_u32;
        let mut rows = Vec::new();
        loop {
            let api_base = self.api_base.clone();
            let owner = self.repo.owner.clone();
            let repo = self.repo.name.clone();
            let page_value = page.to_string();
            let since_value = since.map(ToOwned::to_owned);
            let chunk: Vec<GithubIssue> = self
                .request_json("list issues", || {
                    let mut request = self
                        .http
                        .get(format!("{}/repos/{owner}/{repo}/issues", api_base));
                    request = request.query(&[
                        ("state", "open"),
                        ("sort", "updated"),
                        ("direction", "asc"),
                        ("per_page", "100"),
                        ("page", page_value.as_str()),
                    ]);
                    if let Some(since_value) = since_value.as_deref() {
                        request = request.query(&[("since", since_value)]);
                    }
                    request
                })
                .await?;
            let chunk_len = chunk.len();
            rows.extend(
                chunk
                    .into_iter()
                    .filter(|issue| issue.pull_request.is_none()),
            );
            if chunk_len < 100 {
                break;
            }
            page = page.saturating_add(1);
        }
        Ok(rows)
    }

    pub(super) async fn list_issue_comments(
        &self,
        issue_number: u64,
    ) -> Result<Vec<GithubIssueComment>> {
        let mut page = 1_u32;
        let mut rows = Vec::new();
        loop {
            let api_base = self.api_base.clone();
            let owner = self.repo.owner.clone();
            let repo = self.repo.name.clone();
            let page_value = page.to_string();
            let chunk: Vec<GithubIssueComment> = self
                .request_json("list issue comments", || {
                    self.http
                        .get(format!(
                            "{}/repos/{}/{}/issues/{}/comments",
                            api_base, owner, repo, issue_number
                        ))
                        .query(&[
                            ("sort", "created"),
                            ("direction", "asc"),
                            ("per_page", "100"),
                            ("page", page_value.as_str()),
                        ])
                })
                .await?;
            let chunk_len = chunk.len();
            rows.extend(chunk);
            if chunk_len < 100 {
                break;
            }
            page = page.saturating_add(1);
        }
        Ok(rows)
    }

    pub(super) async fn create_issue_comment(
        &self,
        issue_number: u64,
        body: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let payload = json!({ "body": body });
        self.request_json("create issue comment", || {
            self.http
                .post(format!(
                    "{}/repos/{}/{}/issues/{}/comments",
                    self.api_base, self.repo.owner, self.repo.name, issue_number
                ))
                .json(&payload)
        })
        .await
    }

    pub(super) async fn update_issue_comment(
        &self,
        comment_id: u64,
        body: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let payload = json!({ "body": body });
        self.request_json("update issue comment", || {
            self.http
                .patch(format!(
                    "{}/repos/{}/{}/issues/comments/{}",
                    self.api_base, self.repo.owner, self.repo.name, comment_id
                ))
                .json(&payload)
        })
        .await
    }

    pub(super) async fn download_url_bytes(&self, url: &str) -> Result<GithubBytesResponse> {
        let request = || self.http.get(url);
        self.request_bytes("download issue attachment", request)
            .await
    }

    async fn request_bytes<F>(
        &self,
        operation: &str,
        mut request_builder: F,
    ) -> Result<GithubBytesResponse>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = request_builder()
                .header("x-tau-retry-attempt", attempt.saturating_sub(1).to_string())
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let content_type = response
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|value| value.to_str().ok())
                            .map(|value| value.to_string());
                        let bytes = response
                            .bytes()
                            .await
                            .with_context(|| format!("failed to read github {operation} body"))?;
                        return Ok(GithubBytesResponse {
                            bytes: bytes.to_vec(),
                            content_type,
                        });
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let body = response.text().await.unwrap_or_default();
                    if attempt < self.retry_max_attempts
                        && is_retryable_github_status(status.as_u16())
                    {
                        tokio::time::sleep(retry_delay(
                            self.retry_base_delay_ms,
                            attempt,
                            retry_after,
                        ))
                        .await;
                        continue;
                    }

                    bail!(
                        "github api {operation} failed with status {}: {}",
                        status.as_u16(),
                        truncate_for_error(&body, 800)
                    );
                }
                Err(error) => {
                    if attempt < self.retry_max_attempts && is_retryable_transport_error(&error) {
                        tokio::time::sleep(retry_delay(self.retry_base_delay_ms, attempt, None))
                            .await;
                        continue;
                    }
                    return Err(error)
                        .with_context(|| format!("github api {operation} request failed"));
                }
            }
        }
    }

    async fn request_json<T, F>(&self, operation: &str, mut request_builder: F) -> Result<T>
    where
        T: DeserializeOwned,
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = request_builder()
                .header("x-tau-retry-attempt", attempt.saturating_sub(1).to_string())
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let parsed = response
                            .json::<T>()
                            .await
                            .with_context(|| format!("failed to decode github {operation}"))?;
                        return Ok(parsed);
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let body = response.text().await.unwrap_or_default();
                    if attempt < self.retry_max_attempts
                        && is_retryable_github_status(status.as_u16())
                    {
                        tokio::time::sleep(retry_delay(
                            self.retry_base_delay_ms,
                            attempt,
                            retry_after,
                        ))
                        .await;
                        continue;
                    }

                    bail!(
                        "github api {operation} failed with status {}: {}",
                        status.as_u16(),
                        truncate_for_error(&body, 800)
                    );
                }
                Err(error) => {
                    if attempt < self.retry_max_attempts && is_retryable_transport_error(&error) {
                        tokio::time::sleep(retry_delay(self.retry_base_delay_ms, attempt, None))
                            .await;
                        continue;
                    }
                    return Err(error)
                        .with_context(|| format!("github api {operation} request failed"));
                }
            }
        }
    }
}
