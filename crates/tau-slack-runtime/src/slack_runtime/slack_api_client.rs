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

#[derive(Debug, Clone)]
pub(super) struct SlackPostedMessage {
    pub(super) channel: String,
    pub(super) ts: String,
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

    pub(super) async fn download_file(&self, url: &str) -> Result<Vec<u8>> {
        let request = || self.http.get(url).bearer_auth(&self.bot_token);
        self.request_bytes("file download", request, false).await
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
