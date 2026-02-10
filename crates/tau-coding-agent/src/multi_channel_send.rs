use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::channel_store::{ChannelLogEntry, ChannelStore};
use crate::credentials::{load_credential_store, resolve_non_empty_cli_value};
use crate::multi_channel_contract::{
    MultiChannelEventKind, MultiChannelInboundEvent, MultiChannelTransport,
};
use crate::multi_channel_outbound::{
    MultiChannelOutboundConfig, MultiChannelOutboundDeliveryError,
    MultiChannelOutboundDeliveryReceipt, MultiChannelOutboundDispatcher, MultiChannelOutboundMode,
};
use crate::{current_unix_timestamp_ms, resolve_credential_store_encryption_mode, Cli};

const MULTI_CHANNEL_SEND_REPORT_SCHEMA: &str = "multi_channel_send_report_v1";
const MULTI_CHANNEL_SEND_AUDIT_SCHEMA: &str = "multi_channel_send_audit_v1";
const MULTI_CHANNEL_SEND_MAX_TEXT_CHARS: usize = 16_000;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MultiChannelSendReport {
    pub(crate) schema: String,
    pub(crate) action: String,
    pub(crate) transport: String,
    pub(crate) target: String,
    pub(crate) mode: String,
    pub(crate) status: String,
    pub(crate) reason_code: Option<String>,
    pub(crate) text_chars: usize,
    pub(crate) chunk_count: usize,
    pub(crate) delivery_receipts: Vec<MultiChannelOutboundDeliveryReceipt>,
    pub(crate) event_key: String,
    pub(crate) channel_store_ref: String,
    pub(crate) channel_store_log_path: String,
    pub(crate) audit_artifact_relative_path: Option<String>,
    pub(crate) state_persisted: bool,
    pub(crate) updated_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSendTarget {
    normalized: String,
    conversation_id: String,
    channel_store_id: String,
    actor_id: String,
    metadata: BTreeMap<String, Value>,
}

pub(crate) fn execute_multi_channel_send_command(cli: &Cli) -> Result<()> {
    if cli.multi_channel_send.is_none() {
        return Ok(());
    }

    let report = execute_multi_channel_send_action(cli)?;
    if cli.multi_channel_send_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render multi-channel send json")?
        );
    } else {
        println!("{}", render_multi_channel_send_report(&report));
    }
    Ok(())
}

pub(crate) fn execute_multi_channel_send_action(cli: &Cli) -> Result<MultiChannelSendReport> {
    let transport: MultiChannelTransport = cli
        .multi_channel_send
        .ok_or_else(|| anyhow!("--multi-channel-send is required"))?
        .into();
    let raw_target = cli
        .multi_channel_send_target
        .as_deref()
        .ok_or_else(|| anyhow!("--multi-channel-send-target is required"))?;
    let target = parse_multi_channel_send_target(transport, raw_target)?;
    let text = resolve_multi_channel_send_text(cli)?;
    let text_chars = text.chars().count();
    if text_chars > MULTI_CHANNEL_SEND_MAX_TEXT_CHARS {
        bail!(
            "multi-channel send text too long: {} chars exceeds {}",
            text_chars,
            MULTI_CHANNEL_SEND_MAX_TEXT_CHARS
        );
    }

    let outbound_config = build_multi_channel_send_outbound_config(cli);
    if outbound_config.mode == MultiChannelOutboundMode::ChannelStore {
        bail!("--multi-channel-send requires --multi-channel-outbound-mode=dry-run or provider");
    }
    let outbound_mode = outbound_config.mode.as_str().to_string();
    let dispatcher = MultiChannelOutboundDispatcher::new(outbound_config)
        .context("failed to initialize outbound dispatcher")?;

    let now_unix_ms = current_unix_timestamp_ms();
    let event_key = format!(
        "multi-channel-send:{}:{}:{now_unix_ms}",
        transport.as_str(),
        sanitize_event_component(target.channel_store_id.as_str())
    );
    let event = MultiChannelInboundEvent {
        schema_version: 1,
        transport,
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
            vec![redact_delivery_error_as_receipt(transport, &error)],
        ),
    };

    let store = ChannelStore::open(
        &cli.multi_channel_state_dir.join("channel-store"),
        transport.as_str(),
        target.channel_store_id.as_str(),
    )
    .context("failed to open channel-store for multi-channel send")?;
    let audit_payload = json!({
        "schema": MULTI_CHANNEL_SEND_AUDIT_SCHEMA,
        "transport": transport.as_str(),
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
        transport: transport.as_str().to_string(),
        target: target.normalized,
        mode: outbound_mode,
        status,
        reason_code,
        text_chars,
        chunk_count: receipts.len(),
        delivery_receipts: receipts,
        event_key,
        channel_store_ref: format!("{}/{}", transport.as_str(), target.channel_store_id),
        channel_store_log_path: store.log_path().display().to_string(),
        audit_artifact_relative_path: Some(artifact.relative_path),
        state_persisted: true,
        updated_unix_ms: now_unix_ms,
    })
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

fn resolve_multi_channel_send_text(cli: &Cli) -> Result<String> {
    if let Some(text) = cli
        .multi_channel_send_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(text.to_string());
    }
    if let Some(path) = cli.multi_channel_send_text_file.as_ref() {
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
    bail!(
        "multi-channel send requires --multi-channel-send-text or --multi-channel-send-text-file"
    );
}

fn build_multi_channel_send_outbound_config(cli: &Cli) -> MultiChannelOutboundConfig {
    MultiChannelOutboundConfig {
        mode: cli.multi_channel_outbound_mode.into(),
        max_chars: cli.multi_channel_outbound_max_chars.max(1),
        http_timeout_ms: cli.multi_channel_outbound_http_timeout_ms.max(1),
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        whatsapp_api_base: cli.multi_channel_whatsapp_api_base.trim().to_string(),
        telegram_bot_token: resolve_multi_channel_send_secret(
            cli,
            cli.multi_channel_telegram_bot_token.as_deref(),
            "telegram-bot-token",
        ),
        discord_bot_token: resolve_multi_channel_send_secret(
            cli,
            cli.multi_channel_discord_bot_token.as_deref(),
            "discord-bot-token",
        ),
        whatsapp_access_token: resolve_multi_channel_send_secret(
            cli,
            cli.multi_channel_whatsapp_access_token.as_deref(),
            "whatsapp-access-token",
        ),
        whatsapp_phone_number_id: resolve_multi_channel_send_secret(
            cli,
            cli.multi_channel_whatsapp_phone_number_id.as_deref(),
            "whatsapp-phone-number-id",
        ),
    }
}

fn resolve_multi_channel_send_secret(
    cli: &Cli,
    direct_secret: Option<&str>,
    integration_id: &str,
) -> Option<String> {
    if let Some(value) = resolve_non_empty_cli_value(direct_secret) {
        return Some(value);
    }

    let store = load_credential_store(
        &cli.credential_store,
        resolve_credential_store_encryption_mode(cli),
        cli.credential_store_key.as_deref(),
    )
    .ok()?;
    let record = store.integrations.get(integration_id)?;
    if record.revoked {
        return None;
    }
    record
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn run_multi_channel_send_delivery(
    dispatcher: &MultiChannelOutboundDispatcher,
    event: &MultiChannelInboundEvent,
    response_text: &str,
) -> std::result::Result<
    crate::multi_channel_outbound::MultiChannelOutboundDeliveryResult,
    MultiChannelOutboundDeliveryError,
> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return tokio::task::block_in_place(|| {
            handle.block_on(dispatcher.deliver(event, response_text))
        });
    }

    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(dispatcher.deliver(event, response_text)),
        Err(error) => Err(MultiChannelOutboundDeliveryError {
            reason_code: "delivery_runtime_unavailable".to_string(),
            detail: error.to_string(),
            retryable: false,
            chunk_index: 1,
            chunk_count: 1,
            endpoint: "".to_string(),
            request_body: None,
            http_status: None,
        }),
    }
}

fn redact_delivery_receipts(
    receipts: Vec<MultiChannelOutboundDeliveryReceipt>,
) -> Vec<MultiChannelOutboundDeliveryReceipt> {
    receipts
        .into_iter()
        .map(|mut receipt| {
            if receipt.transport == MultiChannelTransport::Telegram.as_str() {
                receipt.endpoint = redact_telegram_endpoint(receipt.endpoint.as_str());
            }
            receipt
        })
        .collect()
}

fn redact_delivery_error_as_receipt(
    transport: MultiChannelTransport,
    error: &MultiChannelOutboundDeliveryError,
) -> MultiChannelOutboundDeliveryReceipt {
    MultiChannelOutboundDeliveryReceipt {
        transport: transport.as_str().to_string(),
        mode: "provider".to_string(),
        status: "failed".to_string(),
        chunk_index: error.chunk_index,
        chunk_count: error.chunk_count,
        endpoint: if transport == MultiChannelTransport::Telegram {
            redact_telegram_endpoint(error.endpoint.as_str())
        } else {
            error.endpoint.clone()
        },
        request_body: error
            .request_body
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or(Value::Null),
        reason_code: Some(error.reason_code.clone()),
        detail: Some(error.detail.clone()),
        retryable: error.retryable,
        http_status: error.http_status,
        provider_message_id: None,
    }
}

fn redact_telegram_endpoint(endpoint: &str) -> String {
    let Some((prefix, suffix)) = endpoint.split_once("/bot") else {
        return endpoint.to_string();
    };
    let Some((_token, tail)) = suffix.split_once("/sendMessage") else {
        return endpoint.to_string();
    };
    format!("{prefix}/bot<redacted>/sendMessage{tail}")
}

fn render_multi_channel_send_report(report: &MultiChannelSendReport) -> String {
    let reason_code = report.reason_code.as_deref().unwrap_or("none");
    format!(
        "multi-channel send: transport={} target={} mode={} status={} reason_code={} text_chars={} chunk_count={} event_key={} channel_store_ref={} channel_store_log_path={} audit_artifact_relative_path={}",
        report.transport,
        report.target,
        report.mode,
        report.status,
        reason_code,
        report.text_chars,
        report.chunk_count,
        report.event_key,
        report.channel_store_ref,
        report.channel_store_log_path,
        report
            .audit_artifact_relative_path
            .as_deref()
            .unwrap_or("none")
    )
}

fn sanitize_event_component(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
}

fn is_telegram_chat_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first == '-' {
        return chars.clone().next().is_some() && chars.all(|ch| ch.is_ascii_digit());
    }
    first.is_ascii_digit() && chars.all(|ch| ch.is_ascii_digit())
}

fn is_telegram_username(value: &str) -> bool {
    if !value.starts_with('@') || value.len() < 2 {
        return false;
    }
    value
        .chars()
        .skip(1)
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_discord_channel_id(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch.is_ascii_digit()) && trimmed.len() >= 5
}

fn is_e164_like(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let digits = if let Some(stripped) = trimmed.strip_prefix('+') {
        stripped
    } else {
        trimmed
    };
    if digits.is_empty() || digits.len() < 7 || digits.len() > 15 {
        return false;
    }
    digits.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::json;
    use tempfile::tempdir;

    use super::{execute_multi_channel_send_action, parse_multi_channel_send_target};
    use crate::channel_store::ChannelStore;
    use crate::multi_channel_contract::MultiChannelTransport;
    use crate::{tests::test_cli, CliMultiChannelOutboundMode, CliMultiChannelTransport};

    #[test]
    fn unit_parse_multi_channel_send_target_accepts_transport_specific_forms() {
        let telegram =
            parse_multi_channel_send_target(MultiChannelTransport::Telegram, "chat:-100123")
                .expect("telegram target");
        assert_eq!(telegram.conversation_id, "-100123");

        let discord = parse_multi_channel_send_target(
            MultiChannelTransport::Discord,
            "channel:1234567890123",
        )
        .expect("discord target");
        assert_eq!(discord.conversation_id, "1234567890123");

        let whatsapp = parse_multi_channel_send_target(
            MultiChannelTransport::Whatsapp,
            "phone:+15551230000@15551239999",
        )
        .expect("whatsapp target");
        assert_eq!(whatsapp.actor_id, "+15551230000");
        assert_eq!(
            whatsapp.metadata["whatsapp_phone_number_id"].as_str(),
            Some("15551239999")
        );
    }

    #[test]
    fn regression_parse_multi_channel_send_target_rejects_invalid_forms() {
        let telegram =
            parse_multi_channel_send_target(MultiChannelTransport::Telegram, "bad target")
                .expect_err("telegram invalid target");
        assert!(telegram.to_string().contains("invalid telegram target"));

        let discord = parse_multi_channel_send_target(MultiChannelTransport::Discord, "abc")
            .expect_err("discord invalid target");
        assert!(discord.to_string().contains("invalid discord target"));

        let whatsapp =
            parse_multi_channel_send_target(MultiChannelTransport::Whatsapp, "phone:abc")
                .expect_err("whatsapp invalid target");
        assert!(whatsapp.to_string().contains("invalid whatsapp target"));
    }

    #[test]
    fn functional_execute_multi_channel_send_action_dry_run_renders_receipts() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.multi_channel_send = Some(CliMultiChannelTransport::Discord);
        cli.multi_channel_send_target = Some("123456789012345".to_string());
        cli.multi_channel_send_text = Some("hello dry run".to_string());
        cli.multi_channel_send_json = true;
        cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
        cli.multi_channel_state_dir = temp.path().join(".tau/multi-channel");

        let report = execute_multi_channel_send_action(&cli).expect("dry-run send");
        assert_eq!(report.transport, "discord");
        assert_eq!(report.status, "sent");
        assert_eq!(report.mode, "dry_run");
        assert_eq!(report.chunk_count, 1);
        assert_eq!(
            report.delivery_receipts[0].request_body["content"],
            serde_json::Value::String("hello dry run".to_string())
        );
    }

    #[test]
    fn integration_execute_multi_channel_send_action_provider_persists_channel_store_audit() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let telegram = server.mock(|when, then| {
            when.method(POST).path("/bottest-token/sendMessage");
            then.status(200)
                .json_body(json!({"ok": true, "result": {"message_id": 99}}));
        });

        let mut cli = test_cli();
        cli.multi_channel_send = Some(CliMultiChannelTransport::Telegram);
        cli.multi_channel_send_target = Some("-100123456".to_string());
        cli.multi_channel_send_text = Some("provider send".to_string());
        cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::Provider;
        cli.multi_channel_state_dir = temp.path().join(".tau/multi-channel");
        cli.multi_channel_telegram_api_base = server.base_url();
        cli.multi_channel_telegram_bot_token = Some("test-token".to_string());

        let report = execute_multi_channel_send_action(&cli).expect("provider send");
        telegram.assert_calls(1);
        assert_eq!(report.status, "sent");
        assert_eq!(
            report.delivery_receipts[0].provider_message_id.as_deref(),
            Some("99")
        );
        assert!(report.delivery_receipts[0]
            .endpoint
            .contains("/bot<redacted>/sendMessage"));

        let store = ChannelStore::open(
            &cli.multi_channel_state_dir.join("channel-store"),
            "telegram",
            "-100123456",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert!(!logs.is_empty(), "send audit log entry should persist");
        assert_eq!(logs[0].source, "multi_channel_send");
        assert_eq!(logs[0].direction, "outbound");
        assert_eq!(
            logs[0].payload["schema"].as_str(),
            Some("multi_channel_send_audit_v1")
        );
    }

    #[test]
    fn regression_execute_multi_channel_send_action_rejects_oversized_payload() {
        let temp = tempdir().expect("tempdir");
        let mut cli = test_cli();
        cli.multi_channel_send = Some(CliMultiChannelTransport::Whatsapp);
        cli.multi_channel_send_target = Some("+15551230000".to_string());
        cli.multi_channel_send_text = Some("seed".to_string());
        cli.multi_channel_send_text = Some("a".repeat(16_001));
        cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
        cli.multi_channel_state_dir = temp.path().join(".tau/multi-channel");

        let error =
            execute_multi_channel_send_action(&cli).expect_err("oversized send should fail");
        assert!(error
            .to_string()
            .contains("multi-channel send text too long"));
    }
}
