use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::multi_channel_contract::{
    MultiChannelEventKind, MultiChannelInboundEvent, MultiChannelTransport,
};
use crate::multi_channel_credentials::{
    resolve_secret, MultiChannelCredentialStoreSnapshot, ResolvedSecret,
};
use crate::multi_channel_outbound::{
    MultiChannelOutboundConfig, MultiChannelOutboundDeliveryError,
    MultiChannelOutboundDeliveryReceipt, MultiChannelOutboundDeliveryResult,
    MultiChannelOutboundDispatcher, MultiChannelOutboundMode,
};
use tau_core::current_unix_timestamp_ms;
use tau_runtime::{ChannelLogEntry, ChannelStore};

const MULTI_CHANNEL_SEND_REPORT_SCHEMA: &str = "multi_channel_send_report_v1";
const MULTI_CHANNEL_SEND_AUDIT_SCHEMA: &str = "multi_channel_send_audit_v1";
const MULTI_CHANNEL_SEND_MAX_TEXT_CHARS: usize = 16_000;
const TELEGRAM_TOKEN_INTEGRATION_ID: &str = "telegram-bot-token";
const DISCORD_TOKEN_INTEGRATION_ID: &str = "discord-bot-token";
const WHATSAPP_TOKEN_INTEGRATION_ID: &str = "whatsapp-access-token";
const WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID: &str = "whatsapp-phone-number-id";

#[derive(Debug, Clone)]
/// Public struct `MultiChannelSendCommandConfig` used across Tau components.
pub struct MultiChannelSendCommandConfig {
    pub transport: MultiChannelTransport,
    pub target: String,
    pub text: String,
    pub state_dir: PathBuf,
    pub outbound_mode: MultiChannelOutboundMode,
    pub outbound_max_chars: usize,
    pub outbound_http_timeout_ms: u64,
    pub telegram_api_base: String,
    pub discord_api_base: String,
    pub whatsapp_api_base: String,
    pub credential_store: Option<MultiChannelCredentialStoreSnapshot>,
    pub credential_store_unreadable: bool,
    pub telegram_bot_token: Option<String>,
    pub discord_bot_token: Option<String>,
    pub whatsapp_access_token: Option<String>,
    pub whatsapp_phone_number_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
/// Public struct `MultiChannelSendReport` used across Tau components.
pub struct MultiChannelSendReport {
    pub schema: String,
    pub action: String,
    pub transport: String,
    pub target: String,
    pub mode: String,
    pub status: String,
    pub reason_code: Option<String>,
    pub text_chars: usize,
    pub chunk_count: usize,
    pub delivery_receipts: Vec<MultiChannelOutboundDeliveryReceipt>,
    pub event_key: String,
    pub channel_store_ref: String,
    pub channel_store_log_path: String,
    pub audit_artifact_relative_path: Option<String>,
    pub state_persisted: bool,
    pub updated_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSendTarget {
    normalized: String,
    conversation_id: String,
    channel_store_id: String,
    actor_id: String,
    metadata: BTreeMap<String, Value>,
}

pub fn execute_multi_channel_send_action(
    config: &MultiChannelSendCommandConfig,
) -> Result<MultiChannelSendReport> {
    let target = parse_multi_channel_send_target(config.transport, &config.target)?;
    let text = config.text.trim();
    if text.is_empty() {
        bail!("multi-channel send text cannot be empty");
    }
    let text = text.to_string();
    let text_chars = text.chars().count();
    if text_chars > MULTI_CHANNEL_SEND_MAX_TEXT_CHARS {
        bail!(
            "multi-channel send text too long: {} chars exceeds {}",
            text_chars,
            MULTI_CHANNEL_SEND_MAX_TEXT_CHARS
        );
    }

    let outbound_config = build_multi_channel_send_outbound_config(config);
    if outbound_config.mode == MultiChannelOutboundMode::ChannelStore {
        bail!("multi-channel send requires outbound mode dry-run or provider");
    }
    let outbound_mode = outbound_config.mode.as_str().to_string();
    let dispatcher = MultiChannelOutboundDispatcher::new(outbound_config)
        .context("failed to initialize outbound dispatcher")?;

    let now_unix_ms = current_unix_timestamp_ms();
    let event_key = format!(
        "multi-channel-send:{}:{}:{now_unix_ms}",
        config.transport.as_str(),
        sanitize_event_component(target.channel_store_id.as_str())
    );
    let event = MultiChannelInboundEvent {
        schema_version: 1,
        transport: config.transport,
        event_kind: MultiChannelEventKind::Message,
        event_id: event_key.clone(),
        conversation_id: target.conversation_id.clone(),
        thread_id: String::new(),
        actor_id: target.actor_id.clone(),
        actor_display: "Tau operator".to_string(),
        timestamp_ms: now_unix_ms,
        text: text.clone(),
        attachments: Vec::new(),
        metadata: target.metadata.clone(),
    };

    let delivery = run_multi_channel_send_delivery(&dispatcher, &event, text.as_str());
    let (status, reason_code, receipts) = match delivery {
        Ok(result) => (
            "sent".to_string(),
            None,
            redact_delivery_receipts(result.receipts),
        ),
        Err(error) => (
            "failed".to_string(),
            Some(error.reason_code.clone()),
            vec![redact_delivery_error_as_receipt(
                config.transport,
                outbound_mode.as_str(),
                &error,
            )],
        ),
    };

    let store = ChannelStore::open(
        &config.state_dir.join("channel-store"),
        config.transport.as_str(),
        target.channel_store_id.as_str(),
    )
    .context("failed to open channel-store for multi-channel send")?;
    let audit_payload = json!({
        "schema": MULTI_CHANNEL_SEND_AUDIT_SCHEMA,
        "transport": config.transport.as_str(),
        "target": target.normalized,
        "mode": outbound_mode,
        "status": status,
        "reason_code": reason_code,
        "text_chars": text_chars,
        "chunk_count": receipts.len(),
        "receipts": receipts,
    });
    store
        .append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: now_unix_ms,
            direction: "outbound".to_string(),
            event_key: Some(event_key.clone()),
            source: "multi_channel_send".to_string(),
            payload: audit_payload.clone(),
        })
        .context("failed to append multi-channel send audit log entry")?;
    let artifact_payload = serde_json::to_string_pretty(&audit_payload)
        .context("failed to serialize send audit payload")?;
    let artifact = store
        .write_text_artifact(
            event_key.as_str(),
            "multi-channel-send-receipt",
            "operator",
            Some(30),
            "json",
            artifact_payload.as_str(),
        )
        .context("failed to persist multi-channel send audit artifact")?;

    Ok(MultiChannelSendReport {
        schema: MULTI_CHANNEL_SEND_REPORT_SCHEMA.to_string(),
        action: "send".to_string(),
        transport: config.transport.as_str().to_string(),
        target: target.normalized,
        mode: outbound_mode,
        status,
        reason_code,
        text_chars,
        chunk_count: receipts.len(),
        delivery_receipts: receipts,
        event_key,
        channel_store_ref: format!("{}/{}", config.transport.as_str(), target.channel_store_id),
        channel_store_log_path: store.log_path().display().to_string(),
        audit_artifact_relative_path: Some(artifact.relative_path),
        state_persisted: true,
        updated_unix_ms: now_unix_ms,
    })
}

pub fn resolve_multi_channel_send_text(
    text: Option<&str>,
    text_file: Option<&Path>,
) -> Result<String> {
    if let Some(text) = text.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(text.to_string());
    }
    if let Some(path) = text_file {
        let raw = std::fs::read_to_string(path).with_context(|| {
            format!(
                "failed to read --multi-channel-send-text-file {}",
                path.display()
            )
        })?;
        let text = raw.trim();
        if text.is_empty() {
            bail!(
                "--multi-channel-send-text-file '{}' did not contain any non-whitespace text",
                path.display()
            );
        }
        return Ok(text.to_string());
    }
    bail!("multi-channel send requires text or text file")
}

fn build_multi_channel_send_outbound_config(
    config: &MultiChannelSendCommandConfig,
) -> MultiChannelOutboundConfig {
    let telegram_token = resolve_send_secret(
        config,
        config.telegram_bot_token.as_deref(),
        TELEGRAM_TOKEN_INTEGRATION_ID,
    );
    let discord_token = resolve_send_secret(
        config,
        config.discord_bot_token.as_deref(),
        DISCORD_TOKEN_INTEGRATION_ID,
    );
    let whatsapp_token = resolve_send_secret(
        config,
        config.whatsapp_access_token.as_deref(),
        WHATSAPP_TOKEN_INTEGRATION_ID,
    );
    let whatsapp_phone_number_id = resolve_send_secret(
        config,
        config.whatsapp_phone_number_id.as_deref(),
        WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID,
    );
    MultiChannelOutboundConfig {
        mode: config.outbound_mode,
        max_chars: config.outbound_max_chars.max(1),
        http_timeout_ms: config.outbound_http_timeout_ms.max(1),
        telegram_api_base: config.telegram_api_base.trim().to_string(),
        discord_api_base: config.discord_api_base.trim().to_string(),
        whatsapp_api_base: config.whatsapp_api_base.trim().to_string(),
        telegram_bot_token: telegram_token.value,
        discord_bot_token: discord_token.value,
        whatsapp_access_token: whatsapp_token.value,
        whatsapp_phone_number_id: whatsapp_phone_number_id.value,
    }
}

fn resolve_send_secret(
    config: &MultiChannelSendCommandConfig,
    direct_secret: Option<&str>,
    integration_id: &str,
) -> ResolvedSecret {
    resolve_secret(
        direct_secret,
        integration_id,
        config.credential_store.as_ref(),
        config.credential_store_unreadable,
    )
}

fn parse_multi_channel_send_target(
    transport: MultiChannelTransport,
    raw_target: &str,
) -> Result<ParsedSendTarget> {
    let trimmed = raw_target.trim();
    if trimmed.is_empty() {
        bail!("multi-channel send target cannot be empty");
    }

    match transport {
        MultiChannelTransport::Telegram => {
            let target = trimmed.strip_prefix("chat:").unwrap_or(trimmed).trim();
            if target.is_empty() {
                bail!(
                    "invalid telegram target '{}': expected non-empty chat id",
                    raw_target
                );
            }
            if !(is_telegram_chat_id(target) || is_telegram_username(target)) {
                bail!(
                    "invalid telegram target '{}': expected numeric chat id (for example -1001234567890) or @username",
                    raw_target
                );
            }
            Ok(ParsedSendTarget {
                normalized: target.to_string(),
                conversation_id: target.to_string(),
                channel_store_id: target.to_string(),
                actor_id: "operator:cli".to_string(),
                metadata: BTreeMap::new(),
            })
        }
        MultiChannelTransport::Discord => {
            let target = trimmed.strip_prefix("channel:").unwrap_or(trimmed).trim();
            if !is_discord_channel_id(target) {
                bail!(
                    "invalid discord target '{}': expected numeric channel id",
                    raw_target
                );
            }
            Ok(ParsedSendTarget {
                normalized: target.to_string(),
                conversation_id: target.to_string(),
                channel_store_id: target.to_string(),
                actor_id: "operator:cli".to_string(),
                metadata: BTreeMap::new(),
            })
        }
        MultiChannelTransport::Whatsapp => {
            let target = trimmed.strip_prefix("phone:").unwrap_or(trimmed).trim();
            let (recipient, phone_number_id) = match target.split_once('@') {
                Some((recipient, phone_number_id)) => {
                    (recipient.trim(), Some(phone_number_id.trim()))
                }
                None => (target, None),
            };
            if !is_e164_like(recipient) {
                bail!(
                    "invalid whatsapp target '{}': expected E.164-like recipient (for example +15551230000)",
                    raw_target
                );
            }
            if let Some(phone_number_id) = phone_number_id {
                if phone_number_id.is_empty()
                    || !phone_number_id.chars().all(|ch| ch.is_ascii_digit())
                {
                    bail!(
                        "invalid whatsapp target '{}': phone_number_id suffix after '@' must be digits",
                        raw_target
                    );
                }
            }
            let mut metadata = BTreeMap::new();
            if let Some(phone_number_id) = phone_number_id {
                metadata.insert(
                    "whatsapp_phone_number_id".to_string(),
                    Value::String(phone_number_id.to_string()),
                );
            }
            Ok(ParsedSendTarget {
                normalized: if let Some(phone_number_id) = phone_number_id {
                    format!("{recipient}@{phone_number_id}")
                } else {
                    recipient.to_string()
                },
                conversation_id: format!("whatsapp:{recipient}"),
                channel_store_id: format!("whatsapp:{recipient}"),
                actor_id: recipient.to_string(),
                metadata,
            })
        }
    }
}

fn run_multi_channel_send_delivery(
    dispatcher: &MultiChannelOutboundDispatcher,
    event: &MultiChannelInboundEvent,
    text: &str,
) -> Result<MultiChannelOutboundDeliveryResult, MultiChannelOutboundDeliveryError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| MultiChannelOutboundDeliveryError {
            reason_code: "delivery_runtime_init_failed".to_string(),
            detail: error.to_string(),
            retryable: false,
            chunk_index: 0,
            chunk_count: 0,
            endpoint: String::new(),
            request_body: None,
            http_status: None,
        })?;
    runtime.block_on(dispatcher.deliver(event, text))
}

fn redact_delivery_receipts(
    mut receipts: Vec<MultiChannelOutboundDeliveryReceipt>,
) -> Vec<MultiChannelOutboundDeliveryReceipt> {
    for receipt in &mut receipts {
        receipt.provider_message_id = receipt
            .provider_message_id
            .as_ref()
            .map(|value| redact_provider_message_id(value));
    }
    receipts
}

fn redact_delivery_error_as_receipt(
    transport: MultiChannelTransport,
    mode: &str,
    error: &MultiChannelOutboundDeliveryError,
) -> MultiChannelOutboundDeliveryReceipt {
    let request_body = match error.request_body.as_deref() {
        Some(raw) => serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string())),
        None => Value::Null,
    };
    MultiChannelOutboundDeliveryReceipt {
        transport: transport.as_str().to_string(),
        mode: mode.to_string(),
        status: "failed".to_string(),
        chunk_index: error.chunk_index,
        chunk_count: error.chunk_count,
        endpoint: error.endpoint.clone(),
        request_body,
        reason_code: Some(error.reason_code.clone()),
        detail: Some(error.detail.clone()),
        retryable: error.retryable,
        http_status: error.http_status,
        provider_message_id: None,
    }
}

fn redact_provider_message_id(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    if raw.chars().count() <= 8 {
        return "(redacted)".to_string();
    }
    let mut chars = raw.chars();
    let prefix: String = chars.by_ref().take(4).collect();
    format!("{}...", prefix)
}

fn sanitize_event_component(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
        .collect()
}

fn is_telegram_chat_id(value: &str) -> bool {
    if let Some(stripped) = value.strip_prefix('-') {
        return stripped.chars().all(|ch| ch.is_ascii_digit());
    }
    value.chars().all(|ch| ch.is_ascii_digit())
}

fn is_telegram_username(value: &str) -> bool {
    value.starts_with('@')
        && value[1..]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_discord_channel_id(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

fn is_e164_like(value: &str) -> bool {
    if !value.starts_with('+') {
        return false;
    }
    value[1..].chars().all(|ch| ch.is_ascii_digit())
}

pub fn render_multi_channel_send_report(report: &MultiChannelSendReport) -> String {
    format!(
        "multi-channel send: action={} transport={} target={} mode={} status={} reason_code={} text_chars={} chunk_count={} event_key={} channel_store_ref={} channel_store_log_path={} audit_artifact_relative_path={} state_persisted={} updated_unix_ms={}",
        report.action,
        report.transport,
        report.target,
        report.mode,
        report.status,
        report.reason_code.as_deref().unwrap_or("none"),
        report.text_chars,
        report.chunk_count,
        report.event_key,
        report.channel_store_ref,
        report.channel_store_log_path,
        report
            .audit_artifact_relative_path
            .as_deref()
            .unwrap_or("none"),
        report.state_persisted,
        report.updated_unix_ms
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_parse_multi_channel_send_target_rejects_empty() {
        let err = parse_multi_channel_send_target(MultiChannelTransport::Telegram, " ")
            .expect_err("empty should fail");
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn unit_resolve_multi_channel_send_text_prefers_inline_text() {
        let text = resolve_multi_channel_send_text(Some("hello"), None).expect("text");
        assert_eq!(text, "hello");
    }
}
