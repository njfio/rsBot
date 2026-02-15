//! Outbound delivery helpers for channel-specific transports.
//!
//! This module applies SSRF-guarded HTTP delivery with per-transport payload
//! shaping and response classification. Retryable versus terminal failures are
//! exposed through structured errors for runtime dedupe/retry coordination.

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::{redirect::Policy, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};
use tau_runtime::{SsrfGuard, SsrfProtectionConfig, SsrfViolation};

use crate::multi_channel_contract::{MultiChannelInboundEvent, MultiChannelTransport};

const TELEGRAM_SAFE_MAX_CHARS: usize = 4096;
const DISCORD_SAFE_MAX_CHARS: usize = 2000;
const WHATSAPP_SAFE_MAX_CHARS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `MultiChannelOutboundMode` values.
pub enum MultiChannelOutboundMode {
    ChannelStore,
    DryRun,
    Provider,
}

impl MultiChannelOutboundMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChannelStore => "channel_store",
            Self::DryRun => "dry_run",
            Self::Provider => "provider",
        }
    }
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelOutboundConfig` used across Tau components.
pub struct MultiChannelOutboundConfig {
    pub mode: MultiChannelOutboundMode,
    pub max_chars: usize,
    pub http_timeout_ms: u64,
    pub ssrf_protection_enabled: bool,
    pub ssrf_allow_http: bool,
    pub ssrf_allow_private_network: bool,
    pub max_redirects: usize,
    pub telegram_api_base: String,
    pub discord_api_base: String,
    pub whatsapp_api_base: String,
    pub telegram_bot_token: Option<String>,
    pub discord_bot_token: Option<String>,
    pub whatsapp_access_token: Option<String>,
    pub whatsapp_phone_number_id: Option<String>,
}

impl Default for MultiChannelOutboundConfig {
    fn default() -> Self {
        Self {
            mode: MultiChannelOutboundMode::ChannelStore,
            max_chars: 1200,
            http_timeout_ms: 5000,
            ssrf_protection_enabled: true,
            ssrf_allow_http: false,
            ssrf_allow_private_network: false,
            max_redirects: 5,
            telegram_api_base: "https://api.telegram.org".to_string(),
            discord_api_base: "https://discord.com/api/v10".to_string(),
            whatsapp_api_base: "https://graph.facebook.com/v20.0".to_string(),
            telegram_bot_token: None,
            discord_bot_token: None,
            whatsapp_access_token: None,
            whatsapp_phone_number_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelOutboundDeliveryReceipt` used across Tau components.
pub struct MultiChannelOutboundDeliveryReceipt {
    pub transport: String,
    pub mode: String,
    pub status: String,
    pub chunk_index: usize,
    pub chunk_count: usize,
    pub endpoint: String,
    pub request_body: Value,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
    pub retryable: bool,
    pub http_status: Option<u16>,
    pub provider_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelOutboundDeliveryResult` used across Tau components.
pub struct MultiChannelOutboundDeliveryResult {
    pub mode: String,
    pub chunk_count: usize,
    pub receipts: Vec<MultiChannelOutboundDeliveryReceipt>,
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelOutboundDeliveryError` used across Tau components.
pub struct MultiChannelOutboundDeliveryError {
    pub reason_code: String,
    pub detail: String,
    pub retryable: bool,
    pub chunk_index: usize,
    pub chunk_count: usize,
    pub endpoint: String,
    pub request_body: Option<String>,
    pub http_status: Option<u16>,
}

impl std::fmt::Display for MultiChannelOutboundDeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "reason_code={} retryable={} chunk={}/{} endpoint={} detail={}",
            self.reason_code,
            self.retryable,
            self.chunk_index,
            self.chunk_count,
            self.endpoint,
            self.detail
        )
    }
}

impl std::error::Error for MultiChannelOutboundDeliveryError {}

#[derive(Debug, Clone)]
struct MultiChannelOutboundRequest {
    transport: MultiChannelTransport,
    endpoint: String,
    headers: Vec<(String, String)>,
    body: Value,
    chunk_index: usize,
    chunk_count: usize,
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelOutboundDispatcher` used across Tau components.
pub struct MultiChannelOutboundDispatcher {
    config: MultiChannelOutboundConfig,
    client: Option<reqwest::Client>,
    ssrf_guard: SsrfGuard,
}

impl MultiChannelOutboundDispatcher {
    pub fn new(config: MultiChannelOutboundConfig) -> Result<Self> {
        if config.max_chars == 0 {
            return Err(anyhow!(
                "multi-channel outbound max chars must be greater than 0"
            ));
        }
        if config.mode == MultiChannelOutboundMode::Provider && config.http_timeout_ms == 0 {
            return Err(anyhow!(
                "multi-channel outbound provider mode requires http timeout > 0"
            ));
        }
        let ssrf_guard = SsrfGuard::new(SsrfProtectionConfig {
            enabled: config.ssrf_protection_enabled,
            allow_http: config.ssrf_allow_http,
            allow_private_network: config.ssrf_allow_private_network,
        });
        let client = if config.mode == MultiChannelOutboundMode::Provider {
            Some(
                reqwest::Client::builder()
                    .timeout(Duration::from_millis(config.http_timeout_ms))
                    .redirect(Policy::none())
                    .build()
                    .context("failed to build multi-channel outbound http client")?,
            )
        } else {
            None
        };
        Ok(Self {
            config,
            client,
            ssrf_guard,
        })
    }

    pub fn mode(&self) -> MultiChannelOutboundMode {
        self.config.mode
    }

    pub async fn deliver(
        &self,
        event: &MultiChannelInboundEvent,
        response_text: &str,
    ) -> Result<MultiChannelOutboundDeliveryResult, MultiChannelOutboundDeliveryError> {
        if self.config.mode == MultiChannelOutboundMode::ChannelStore {
            return Ok(MultiChannelOutboundDeliveryResult {
                mode: self.config.mode.as_str().to_string(),
                chunk_count: 0,
                receipts: Vec::new(),
            });
        }

        let requests = self.build_requests(event, response_text)?;
        if requests.is_empty() {
            return Ok(MultiChannelOutboundDeliveryResult {
                mode: self.config.mode.as_str().to_string(),
                chunk_count: 0,
                receipts: Vec::new(),
            });
        }

        let mut receipts = Vec::with_capacity(requests.len());
        for request in requests {
            match self.config.mode {
                MultiChannelOutboundMode::DryRun => {
                    receipts.push(MultiChannelOutboundDeliveryReceipt {
                        transport: request.transport.as_str().to_string(),
                        mode: self.config.mode.as_str().to_string(),
                        status: "dry_run".to_string(),
                        chunk_index: request.chunk_index,
                        chunk_count: request.chunk_count,
                        endpoint: request.endpoint.clone(),
                        request_body: request.body.clone(),
                        reason_code: None,
                        detail: None,
                        retryable: false,
                        http_status: None,
                        provider_message_id: None,
                    });
                }
                MultiChannelOutboundMode::Provider => {
                    let receipt = self.send_request(&request).await?;
                    receipts.push(receipt);
                }
                MultiChannelOutboundMode::ChannelStore => {}
            }
        }

        Ok(MultiChannelOutboundDeliveryResult {
            mode: self.config.mode.as_str().to_string(),
            chunk_count: receipts.len(),
            receipts,
        })
    }

    fn build_requests(
        &self,
        event: &MultiChannelInboundEvent,
        response_text: &str,
    ) -> Result<Vec<MultiChannelOutboundRequest>, MultiChannelOutboundDeliveryError> {
        let trimmed = response_text.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let safe_max_chars = self.safe_max_chars(event.transport);
        let chunk_max = self.config.max_chars.min(safe_max_chars).max(1);
        let chunks = chunk_text(trimmed, chunk_max);
        let chunk_count = chunks.len();
        if chunk_count == 0 {
            return Ok(Vec::new());
        }

        chunks
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| {
                let chunk_index = index + 1;
                self.build_request_for_chunk(event, chunk, chunk_index, chunk_count)
            })
            .collect::<Result<Vec<_>, _>>()
    }

    fn safe_max_chars(&self, transport: MultiChannelTransport) -> usize {
        match transport {
            MultiChannelTransport::Telegram => TELEGRAM_SAFE_MAX_CHARS,
            MultiChannelTransport::Discord => DISCORD_SAFE_MAX_CHARS,
            MultiChannelTransport::Whatsapp => WHATSAPP_SAFE_MAX_CHARS,
        }
    }

    fn build_request_for_chunk(
        &self,
        event: &MultiChannelInboundEvent,
        chunk: String,
        chunk_index: usize,
        chunk_count: usize,
    ) -> Result<MultiChannelOutboundRequest, MultiChannelOutboundDeliveryError> {
        match event.transport {
            MultiChannelTransport::Telegram => {
                let token = self
                    .config
                    .telegram_bot_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        if self.config.mode == MultiChannelOutboundMode::DryRun {
                            Some("dry-run-telegram-token".to_string())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_missing_telegram_bot_token".to_string(),
                        detail: "Telegram outbound requires TAU_TELEGRAM_BOT_TOKEN or credential-store integration id telegram-bot-token".to_string(),
                        retryable: false,
                        chunk_index,
                        chunk_count,
                        endpoint: "".to_string(),
                        request_body: None,
                        http_status: None,
                    })?;
                let endpoint = format!(
                    "{}/bot{}/sendMessage",
                    self.config.telegram_api_base.trim_end_matches('/'),
                    token
                );
                Ok(MultiChannelOutboundRequest {
                    transport: event.transport,
                    endpoint,
                    headers: Vec::new(),
                    body: json!({
                        "chat_id": event.conversation_id.trim(),
                        "text": chunk,
                        "disable_web_page_preview": true
                    }),
                    chunk_index,
                    chunk_count,
                })
            }
            MultiChannelTransport::Discord => {
                let token = self
                    .config
                    .discord_bot_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        if self.config.mode == MultiChannelOutboundMode::DryRun {
                            Some("dry-run-discord-token".to_string())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_missing_discord_bot_token".to_string(),
                        detail: "Discord outbound requires TAU_DISCORD_BOT_TOKEN or credential-store integration id discord-bot-token".to_string(),
                        retryable: false,
                        chunk_index,
                        chunk_count,
                        endpoint: "".to_string(),
                        request_body: None,
                        http_status: None,
                    })?;
                let endpoint = format!(
                    "{}/channels/{}/messages",
                    self.config.discord_api_base.trim_end_matches('/'),
                    event.conversation_id.trim()
                );
                Ok(MultiChannelOutboundRequest {
                    transport: event.transport,
                    endpoint,
                    headers: vec![("Authorization".to_string(), format!("Bot {}", token))],
                    body: json!({
                        "content": chunk
                    }),
                    chunk_index,
                    chunk_count,
                })
            }
            MultiChannelTransport::Whatsapp => {
                let access_token = self
                    .config
                    .whatsapp_access_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        if self.config.mode == MultiChannelOutboundMode::DryRun {
                            Some("dry-run-whatsapp-token".to_string())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_missing_whatsapp_access_token".to_string(),
                        detail: "WhatsApp outbound requires TAU_WHATSAPP_ACCESS_TOKEN or credential-store integration id whatsapp-access-token".to_string(),
                        retryable: false,
                        chunk_index,
                        chunk_count,
                        endpoint: "".to_string(),
                        request_body: None,
                        http_status: None,
                    })?;
                let phone_number_id = self
                    .config
                    .whatsapp_phone_number_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        event
                            .metadata
                            .get("whatsapp_phone_number_id")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                    })
                    .or_else(|| {
                        if self.config.mode == MultiChannelOutboundMode::DryRun {
                            Some("dry-run-phone-number-id")
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_missing_whatsapp_phone_number_id".to_string(),
                        detail: "WhatsApp outbound requires TAU_WHATSAPP_PHONE_NUMBER_ID, credential-store integration id whatsapp-phone-number-id, or inbound metadata.whatsapp_phone_number_id".to_string(),
                        retryable: false,
                        chunk_index,
                        chunk_count,
                        endpoint: "".to_string(),
                        request_body: None,
                        http_status: None,
                    })?;
                let recipient = event
                    .actor_id
                    .trim()
                    .split(':')
                    .next_back()
                    .unwrap_or_default()
                    .trim();
                if recipient.is_empty() && self.config.mode != MultiChannelOutboundMode::DryRun {
                    return Err(MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_missing_whatsapp_recipient".to_string(),
                        detail: "WhatsApp outbound requires a non-empty actor_id".to_string(),
                        retryable: false,
                        chunk_index,
                        chunk_count,
                        endpoint: "".to_string(),
                        request_body: None,
                        http_status: None,
                    });
                }
                let recipient = if recipient.is_empty() {
                    "dry-run-recipient"
                } else {
                    recipient
                };
                let endpoint = format!(
                    "{}/{}/messages",
                    self.config.whatsapp_api_base.trim_end_matches('/'),
                    phone_number_id
                );
                Ok(MultiChannelOutboundRequest {
                    transport: event.transport,
                    endpoint,
                    headers: vec![(
                        "Authorization".to_string(),
                        format!("Bearer {}", access_token),
                    )],
                    body: json!({
                        "messaging_product": "whatsapp",
                        "to": recipient,
                        "type": "text",
                        "text": {
                            "body": chunk
                        }
                    }),
                    chunk_index,
                    chunk_count,
                })
            }
        }
    }

    async fn send_request(
        &self,
        request: &MultiChannelOutboundRequest,
    ) -> Result<MultiChannelOutboundDeliveryReceipt, MultiChannelOutboundDeliveryError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| MultiChannelOutboundDeliveryError {
                reason_code: "delivery_provider_client_unavailable".to_string(),
                detail: "provider mode requested without initialized HTTP client".to_string(),
                retryable: false,
                chunk_index: request.chunk_index,
                chunk_count: request.chunk_count,
                endpoint: request.endpoint.clone(),
                request_body: Some(compact_request_body(&request.body)),
                http_status: None,
            })?;
        let mut endpoint = self
            .ssrf_guard
            .parse_and_validate_url(request.endpoint.as_str())
            .await
            .map_err(|violation| {
                self.map_ssrf_violation(request, request.endpoint.as_str(), violation)
            })?;
        let mut redirect_count = 0usize;

        loop {
            let mut http_request = client.post(endpoint.as_str());
            for (header, value) in &request.headers {
                http_request = http_request.header(header, value);
            }
            let response = http_request
                .json(&request.body)
                .send()
                .await
                .map_err(|error| MultiChannelOutboundDeliveryError {
                    reason_code: "delivery_transport_error".to_string(),
                    detail: error.to_string(),
                    retryable: true,
                    chunk_index: request.chunk_index,
                    chunk_count: request.chunk_count,
                    endpoint: endpoint.as_str().to_string(),
                    request_body: Some(compact_request_body(&request.body)),
                    http_status: None,
                })?;
            let status = response.status();
            if status.is_redirection() {
                if redirect_count >= self.config.max_redirects {
                    return Err(MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_redirect_limit_exceeded".to_string(),
                        detail: format!(
                            "redirect count exceeded configured max_redirects={} for endpoint '{}'",
                            self.config.max_redirects, endpoint
                        ),
                        retryable: false,
                        chunk_index: request.chunk_index,
                        chunk_count: request.chunk_count,
                        endpoint: endpoint.as_str().to_string(),
                        request_body: Some(compact_request_body(&request.body)),
                        http_status: Some(status.as_u16()),
                    });
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .ok_or_else(|| MultiChannelOutboundDeliveryError {
                        reason_code: "delivery_redirect_missing_location".to_string(),
                        detail: format!(
                            "provider returned redirect status {} without Location header",
                            status
                        ),
                        retryable: false,
                        chunk_index: request.chunk_index,
                        chunk_count: request.chunk_count,
                        endpoint: endpoint.as_str().to_string(),
                        request_body: Some(compact_request_body(&request.body)),
                        http_status: Some(status.as_u16()),
                    })?;
                let location =
                    location
                        .to_str()
                        .map_err(|error| MultiChannelOutboundDeliveryError {
                            reason_code: "delivery_redirect_invalid_location".to_string(),
                            detail: format!("provider returned invalid Location header: {error}"),
                            retryable: false,
                            chunk_index: request.chunk_index,
                            chunk_count: request.chunk_count,
                            endpoint: endpoint.as_str().to_string(),
                            request_body: Some(compact_request_body(&request.body)),
                            http_status: Some(status.as_u16()),
                        })?;
                let next_url = endpoint.join(location).map_err(|error| MultiChannelOutboundDeliveryError {
                    reason_code: "delivery_redirect_invalid_location".to_string(),
                    detail: format!(
                        "provider redirect location '{}' could not be resolved against '{}': {error}",
                        location,
                        endpoint
                    ),
                    retryable: false,
                    chunk_index: request.chunk_index,
                    chunk_count: request.chunk_count,
                    endpoint: endpoint.as_str().to_string(),
                    request_body: Some(compact_request_body(&request.body)),
                    http_status: Some(status.as_u16()),
                })?;
                self.ssrf_guard
                    .validate_url(&next_url)
                    .await
                    .map_err(|violation| {
                        self.map_ssrf_violation(request, next_url.as_str(), violation)
                    })?;
                endpoint = next_url;
                redirect_count = redirect_count.saturating_add(1);
                continue;
            }

            let endpoint_string = endpoint.as_str().to_string();
            let body_raw = response.text().await.unwrap_or_default();
            let body_json = serde_json::from_str::<Value>(&body_raw).unwrap_or(Value::Null);
            if status.is_success() {
                return Ok(MultiChannelOutboundDeliveryReceipt {
                    transport: request.transport.as_str().to_string(),
                    mode: self.config.mode.as_str().to_string(),
                    status: "sent".to_string(),
                    chunk_index: request.chunk_index,
                    chunk_count: request.chunk_count,
                    endpoint: endpoint_string,
                    request_body: request.body.clone(),
                    reason_code: None,
                    detail: None,
                    retryable: false,
                    http_status: Some(status.as_u16()),
                    provider_message_id: extract_provider_message_id(request.transport, &body_json),
                });
            }

            let (reason_code, retryable) = classify_provider_status(status);
            return Err(MultiChannelOutboundDeliveryError {
                reason_code: reason_code.to_string(),
                detail: truncate_detail(&body_raw),
                retryable,
                chunk_index: request.chunk_index,
                chunk_count: request.chunk_count,
                endpoint: endpoint_string,
                request_body: Some(compact_request_body(&request.body)),
                http_status: Some(status.as_u16()),
            });
        }
    }

    fn map_ssrf_violation(
        &self,
        request: &MultiChannelOutboundRequest,
        endpoint: &str,
        violation: SsrfViolation,
    ) -> MultiChannelOutboundDeliveryError {
        let retryable = violation.reason_code == "delivery_ssrf_dns_resolution_failed";
        MultiChannelOutboundDeliveryError {
            reason_code: violation.reason_code,
            detail: violation.detail,
            retryable,
            chunk_index: request.chunk_index,
            chunk_count: request.chunk_count,
            endpoint: endpoint.to_string(),
            request_body: Some(compact_request_body(&request.body)),
            http_status: None,
        }
    }
}

fn classify_provider_status(status: StatusCode) -> (&'static str, bool) {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ("delivery_rate_limited", true);
    }
    if status.is_server_error() {
        return ("delivery_provider_unavailable", true);
    }
    if status.is_client_error() {
        return ("delivery_request_rejected", false);
    }
    ("delivery_unknown_http_failure", true)
}

fn extract_provider_message_id(
    transport: MultiChannelTransport,
    payload: &Value,
) -> Option<String> {
    match transport {
        MultiChannelTransport::Telegram => payload
            .get("result")
            .and_then(|value| value.get("message_id"))
            .and_then(|value| value.as_i64())
            .map(|value| value.to_string()),
        MultiChannelTransport::Discord => payload
            .get("id")
            .and_then(Value::as_str)
            .map(|value| value.to_string()),
        MultiChannelTransport::Whatsapp => payload
            .get("messages")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .map(|value| value.to_string()),
    }
}

fn truncate_detail(raw: &str) -> String {
    const LIMIT: usize = 512;
    let trimmed = raw.trim();
    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }
    let mut output = String::new();
    for ch in trimmed.chars().take(LIMIT) {
        output.push(ch);
    }
    output.push_str("...");
    output
}

fn compact_request_body(value: &Value) -> String {
    const LIMIT: usize = 512;
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if serialized.chars().count() <= LIMIT {
        return serialized;
    }
    let mut output = String::new();
    for ch in serialized.chars().take(LIMIT) {
        output.push(ch);
    }
    output.push_str("...");
    output
}

fn chunk_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.is_empty() || max_chars == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for ch in text.chars() {
        current.push(ch);
        current_len = current_len.saturating_add(1);
        if current_len >= max_chars {
            chunks.push(current);
            current = String::new();
            current_len = 0;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::json;

    use super::{
        chunk_text, MultiChannelOutboundConfig, MultiChannelOutboundDispatcher,
        MultiChannelOutboundMode,
    };
    use crate::multi_channel_contract::{
        MultiChannelEventKind, MultiChannelInboundEvent, MultiChannelTransport,
    };
    use std::collections::BTreeMap;

    fn sample_event(transport: MultiChannelTransport) -> MultiChannelInboundEvent {
        MultiChannelInboundEvent {
            schema_version: 1,
            transport,
            event_kind: MultiChannelEventKind::Message,
            event_id: "event-1".to_string(),
            conversation_id: "chat-1".to_string(),
            thread_id: String::new(),
            actor_id: "15551234567".to_string(),
            actor_display: String::new(),
            timestamp_ms: 1_760_200_000_000,
            text: "hello".to_string(),
            attachments: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn unit_chunk_text_respects_max_chars() {
        let chunks = chunk_text("abcdefghijk", 4);
        assert_eq!(
            chunks,
            vec!["abcd".to_string(), "efgh".to_string(), "ijk".to_string()]
        );
    }

    #[tokio::test]
    async fn functional_dry_run_shapes_discord_payload_and_chunking() {
        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::DryRun,
            max_chars: 5,
            discord_bot_token: Some("token".to_string()),
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let mut event = sample_event(MultiChannelTransport::Discord);
        event.conversation_id = "room-88".to_string();
        let result = dispatcher
            .deliver(&event, "abcdefghijklmnopqrstuvwxyz")
            .await
            .expect("dry-run should succeed");
        assert_eq!(result.mode, "dry_run");
        assert_eq!(result.chunk_count, 6);
        assert_eq!(result.receipts[0].status, "dry_run");
        assert_eq!(result.receipts[0].request_body["content"], "abcde");
        assert_eq!(result.receipts[5].request_body["content"], "z");
        assert!(result.receipts[0]
            .endpoint
            .ends_with("/channels/room-88/messages"));
    }

    #[tokio::test]
    async fn integration_provider_mode_posts_telegram_request() {
        let server = MockServer::start();
        let sent = server.mock(|when, then| {
            when.method(POST).path("/bottest-token/sendMessage");
            then.status(200)
                .json_body(json!({"ok": true, "result": {"message_id": 55}}));
        });

        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::Provider,
            max_chars: 100,
            telegram_api_base: server.base_url(),
            telegram_bot_token: Some("test-token".to_string()),
            ssrf_allow_http: true,
            ssrf_allow_private_network: true,
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let result = dispatcher
            .deliver(
                &sample_event(MultiChannelTransport::Telegram),
                "hello telegram",
            )
            .await
            .expect("provider send should succeed");
        sent.assert_calls(1);
        assert_eq!(result.chunk_count, 1);
        assert_eq!(result.receipts[0].status, "sent");
        assert_eq!(
            result.receipts[0].provider_message_id.as_deref(),
            Some("55")
        );
    }

    #[tokio::test]
    async fn functional_provider_mode_follows_redirects_with_per_hop_validation() {
        let server = MockServer::start();
        let redirect = server.mock(|when, then| {
            when.method(POST).path("/bottest-token/sendMessage");
            then.status(307).header(
                "Location",
                format!("{}/redirected", server.base_url()).as_str(),
            );
        });
        let final_target = server.mock(|when, then| {
            when.method(POST).path("/redirected");
            then.status(200)
                .json_body(json!({"ok": true, "result": {"message_id": 77}}));
        });

        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::Provider,
            max_chars: 100,
            telegram_api_base: server.base_url(),
            telegram_bot_token: Some("test-token".to_string()),
            ssrf_allow_http: true,
            ssrf_allow_private_network: true,
            max_redirects: 3,
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let result = dispatcher
            .deliver(
                &sample_event(MultiChannelTransport::Telegram),
                "hello telegram with redirect",
            )
            .await
            .expect("provider send should follow redirect");

        redirect.assert_calls(1);
        final_target.assert_calls(1);
        assert_eq!(result.chunk_count, 1);
        assert_eq!(result.receipts[0].status, "sent");
        assert_eq!(
            result.receipts[0].provider_message_id.as_deref(),
            Some("77")
        );
        assert!(result.receipts[0].endpoint.ends_with("/redirected"));
    }

    #[tokio::test]
    async fn regression_provider_mode_blocks_http_without_override() {
        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::Provider,
            max_chars: 100,
            telegram_api_base: "http://example.com".to_string(),
            telegram_bot_token: Some("test-token".to_string()),
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let error = dispatcher
            .deliver(&sample_event(MultiChannelTransport::Telegram), "hello")
            .await
            .expect_err("http should be blocked");
        assert_eq!(error.reason_code, "delivery_ssrf_blocked_scheme");
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn regression_provider_mode_blocks_metadata_redirect_even_with_private_override() {
        let server = MockServer::start();
        let redirect = server.mock(|when, then| {
            when.method(POST).path("/bottest-token/sendMessage");
            then.status(302)
                .header("Location", "http://169.254.169.254/latest/meta-data");
        });

        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::Provider,
            max_chars: 100,
            telegram_api_base: server.base_url(),
            telegram_bot_token: Some("test-token".to_string()),
            ssrf_allow_http: true,
            ssrf_allow_private_network: true,
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let error = dispatcher
            .deliver(&sample_event(MultiChannelTransport::Telegram), "hello")
            .await
            .expect_err("metadata redirect should be blocked");
        redirect.assert_calls(1);
        assert_eq!(error.reason_code, "delivery_ssrf_blocked_metadata_endpoint");
    }

    #[tokio::test]
    async fn regression_provider_mode_enforces_redirect_limit() {
        let server = MockServer::start();
        let redirect = server.mock(|when, then| {
            when.method(POST).path("/bottest-token/sendMessage");
            then.status(302).header(
                "Location",
                format!("{}/bottest-token/sendMessage", server.base_url()).as_str(),
            );
        });

        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::Provider,
            max_chars: 100,
            telegram_api_base: server.base_url(),
            telegram_bot_token: Some("test-token".to_string()),
            ssrf_allow_http: true,
            ssrf_allow_private_network: true,
            max_redirects: 0,
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let error = dispatcher
            .deliver(&sample_event(MultiChannelTransport::Telegram), "hello")
            .await
            .expect_err("redirects should stop at configured limit");
        redirect.assert_calls(1);
        assert_eq!(error.reason_code, "delivery_redirect_limit_exceeded");
    }

    #[tokio::test]
    async fn regression_provider_mode_returns_stable_reason_for_missing_token() {
        let dispatcher = MultiChannelOutboundDispatcher::new(MultiChannelOutboundConfig {
            mode: MultiChannelOutboundMode::Provider,
            max_chars: 100,
            telegram_bot_token: None,
            ..MultiChannelOutboundConfig::default()
        })
        .expect("dispatcher");
        let error = dispatcher
            .deliver(&sample_event(MultiChannelTransport::Telegram), "hello")
            .await
            .expect_err("missing token should fail");
        assert_eq!(error.reason_code, "delivery_missing_telegram_bot_token");
        assert!(!error.retryable);
    }
}
