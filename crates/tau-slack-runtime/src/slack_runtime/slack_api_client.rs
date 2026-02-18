//! Slack Web API client helpers used by bridge polling and posting flows.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    is_retryable_slack_status, is_retryable_transport_error, parse_retry_after, retry_delay,
    truncate_for_error,
};

#[derive(Debug, Clone, Deserialize)]
struct SlackAuthTestResponse {
    ok: bool,
    user_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackOpenSocketResponse {
    ok: bool,
    url: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackChatMessageResponse {
    ok: bool,
    ts: Option<String>,
    channel: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackGetUploadUrlExternalResponse {
    ok: bool,
    upload_url: Option<String>,
    file_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackCompleteUploadExternalResponse {
    ok: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct SlackPostedMessage {
    pub(super) channel: String,
    pub(super) ts: String,
}

#[derive(Debug, Clone)]
pub(super) struct SlackUploadedFile {
    pub(super) file_id: String,
}

#[derive(Clone)]
pub(super) struct SlackApiClient {
    http: reqwest::Client,
    api_base: String,
    app_token: String,
    bot_token: String,
    retry_max_attempts: usize,
    retry_base_delay_ms: u64,
}

impl SlackApiClient {
    pub(super) fn new(
        api_base: String,
        app_token: String,
        bot_token: String,
        request_timeout_ms: u64,
        retry_max_attempts: usize,
        retry_base_delay_ms: u64,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Tau-slack-bridge"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(request_timeout_ms.max(1)))
            .build()
            .context("failed to create slack api client")?;

        Ok(Self {
            http,
            api_base: api_base.trim_end_matches('/').to_string(),
            app_token: app_token.trim().to_string(),
            bot_token: bot_token.trim().to_string(),
            retry_max_attempts: retry_max_attempts.max(1),
            retry_base_delay_ms: retry_base_delay_ms.max(1),
        })
    }

    pub(super) async fn resolve_bot_user_id(&self) -> Result<String> {
        let response: SlackAuthTestResponse = self
            .request_json(
                "auth.test",
                || {
                    self.http
                        .post(format!("{}/auth.test", self.api_base))
                        .bearer_auth(&self.bot_token)
                },
                true,
            )
            .await?;

        if !response.ok {
            bail!(
                "slack auth.test failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .user_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("slack auth.test did not return user_id"))
    }

    pub(super) async fn open_socket_connection(&self) -> Result<String> {
        let response: SlackOpenSocketResponse = self
            .request_json(
                "apps.connections.open",
                || {
                    self.http
                        .post(format!("{}/apps.connections.open", self.api_base))
                        .bearer_auth(&self.app_token)
                },
                true,
            )
            .await?;
        if !response.ok {
            bail!(
                "slack apps.connections.open failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }
        response
            .url
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("slack apps.connections.open did not return url"))
    }

    pub(super) async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<SlackPostedMessage> {
        let mut payload = json!({
            "channel": channel,
            "text": text,
            "mrkdwn": false,
            "unfurl_links": false,
            "unfurl_media": false,
        });
        if let Some(thread_ts) = thread_ts {
            payload["thread_ts"] = Value::String(thread_ts.to_string());
        }

        let response: SlackChatMessageResponse = self
            .request_json(
                "chat.postMessage",
                || {
                    self.http
                        .post(format!("{}/chat.postMessage", self.api_base))
                        .bearer_auth(&self.bot_token)
                        .json(&payload)
                },
                true,
            )
            .await?;

        if !response.ok {
            bail!(
                "slack chat.postMessage failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(SlackPostedMessage {
            channel: response.channel.unwrap_or_else(|| channel.to_string()),
            ts: response
                .ts
                .ok_or_else(|| anyhow!("slack chat.postMessage response missing ts"))?,
        })
    }

    pub(super) async fn update_message(
        &self,
        channel: &str,
        ts: &str,
        text: &str,
    ) -> Result<SlackPostedMessage> {
        let payload = json!({
            "channel": channel,
            "ts": ts,
            "text": text,
            "mrkdwn": false,
        });
        let response: SlackChatMessageResponse = self
            .request_json(
                "chat.update",
                || {
                    self.http
                        .post(format!("{}/chat.update", self.api_base))
                        .bearer_auth(&self.bot_token)
                        .json(&payload)
                },
                true,
            )
            .await?;
        if !response.ok {
            bail!(
                "slack chat.update failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }
        Ok(SlackPostedMessage {
            channel: response.channel.unwrap_or_else(|| channel.to_string()),
            ts: response.ts.unwrap_or_else(|| ts.to_string()),
        })
    }

    pub(super) async fn upload_file_v2(
        &self,
        channel: &str,
        thread_ts: Option<&str>,
        filename: &str,
        bytes: &[u8],
        initial_comment: Option<&str>,
    ) -> Result<SlackUploadedFile> {
        if filename.trim().is_empty() {
            bail!("slack files upload requires non-empty filename");
        }
        let file_size = bytes.len();
        if file_size == 0 {
            bail!("slack files upload requires non-empty payload");
        }

        let get_upload: SlackGetUploadUrlExternalResponse = self
            .request_json(
                "files.getUploadURLExternal",
                || {
                    self.http
                        .post(format!("{}/files.getUploadURLExternal", self.api_base))
                        .bearer_auth(&self.bot_token)
                        .json(&json!({
                            "filename": filename,
                            "length": file_size,
                        }))
                },
                true,
            )
            .await?;
        if !get_upload.ok {
            bail!(
                "slack files.getUploadURLExternal failed: {}",
                get_upload
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }
        let upload_url = get_upload
            .upload_url
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("slack files.getUploadURLExternal missing upload_url"))?;
        let file_id = get_upload
            .file_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("slack files.getUploadURLExternal missing file_id"))?;

        let upload_response = self
            .http
            .post(upload_url)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await
            .context("failed to upload file payload to slack external upload URL")?;
        if !upload_response.status().is_success() {
            let status = upload_response.status();
            let body = upload_response.text().await.unwrap_or_default();
            bail!(
                "slack external upload failed: status={} body={}",
                status,
                truncate_for_error(&body, 320)
            );
        }

        let mut complete_payload = json!({
            "files": [{ "id": file_id.clone(), "title": filename }],
            "channel_id": channel,
        });
        if let Some(thread_ts) = thread_ts.map(str::trim).filter(|value| !value.is_empty()) {
            complete_payload["thread_ts"] = Value::String(thread_ts.to_string());
        }
        if let Some(initial_comment) = initial_comment
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            complete_payload["initial_comment"] = Value::String(initial_comment.to_string());
        }

        let complete: SlackCompleteUploadExternalResponse = self
            .request_json(
                "files.completeUploadExternal",
                || {
                    self.http
                        .post(format!("{}/files.completeUploadExternal", self.api_base))
                        .bearer_auth(&self.bot_token)
                        .json(&complete_payload)
                },
                true,
            )
            .await?;
        if !complete.ok {
            bail!(
                "slack files.completeUploadExternal failed: {}",
                complete
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(SlackUploadedFile { file_id })
    }

    pub(super) async fn download_file(&self, url: &str) -> Result<Vec<u8>> {
        let request = || self.http.get(url).bearer_auth(&self.bot_token);
        self.request_bytes("file download", request, false).await
    }

    pub(super) async fn download_public_file(&self, url: &str) -> Result<Vec<u8>> {
        let request = || self.http.get(url);
        self.request_bytes("public file download", request, false)
            .await
    }

    async fn request_json<T, F>(
        &self,
        operation: &str,
        mut builder: F,
        decode_error_body: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = builder()
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
                            .with_context(|| format!("failed to decode slack {operation}"))?;
                        return Ok(parsed);
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let body = if decode_error_body {
                        response.text().await.unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if attempt < self.retry_max_attempts
                        && is_retryable_slack_status(status.as_u16())
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
                        "slack api {operation} failed with status {}: {}",
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
                        .with_context(|| format!("slack api {operation} request failed"));
                }
            }
        }
    }

    async fn request_bytes<F>(
        &self,
        operation: &str,
        mut builder: F,
        decode_error_body: bool,
    ) -> Result<Vec<u8>>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = builder()
                .header("x-tau-retry-attempt", attempt.saturating_sub(1).to_string())
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response.bytes().await?.to_vec());
                    }
                    let retry_after = parse_retry_after(response.headers());
                    let body = if decode_error_body {
                        response.text().await.unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if attempt < self.retry_max_attempts
                        && is_retryable_slack_status(status.as_u16())
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
                        "slack api {operation} failed with status {}: {}",
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
                        .with_context(|| format!("slack api {operation} request failed"));
                }
            }
        }
    }
}
