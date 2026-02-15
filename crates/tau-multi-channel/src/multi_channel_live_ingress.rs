//! Ingress envelope parsing and normalization for live multi-channel events.
//!
//! Incoming payloads are converted into canonical event envelopes with stable
//! metadata/session fields before routing. Parsing and normalization failures are
//! surfaced with contextual diagnostics so operators can trace malformed ingress.

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::OpenOptions;
use std::io::Write;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::multi_channel_contract::{
    validate_multi_channel_inbound_event, MultiChannelAttachment, MultiChannelEventKind,
    MultiChannelInboundEvent, MultiChannelTransport, MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelLiveIngressReasonCode` values.
pub enum MultiChannelLiveIngressReasonCode {
    InvalidJson,
    UnsupportedSchemaVersion,
    MissingTransport,
    UnsupportedTransport,
    MissingPayload,
    MissingField,
    InvalidFieldType,
    InvalidTimestamp,
    EmptyContent,
    InvalidNormalizedEvent,
}

impl MultiChannelLiveIngressReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidJson => "invalid_json",
            Self::UnsupportedSchemaVersion => "unsupported_schema_version",
            Self::MissingTransport => "missing_transport",
            Self::UnsupportedTransport => "unsupported_transport",
            Self::MissingPayload => "missing_payload",
            Self::MissingField => "missing_field",
            Self::InvalidFieldType => "invalid_field_type",
            Self::InvalidTimestamp => "invalid_timestamp",
            Self::EmptyContent => "empty_content",
            Self::InvalidNormalizedEvent => "invalid_normalized_event",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `MultiChannelLiveIngressParseError` used across Tau components.
pub struct MultiChannelLiveIngressParseError {
    pub code: MultiChannelLiveIngressReasonCode,
    pub message: String,
}

impl Display for MultiChannelLiveIngressParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for MultiChannelLiveIngressParseError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `MultiChannelLiveInboundEnvelope` used across Tau components.
pub struct MultiChannelLiveInboundEnvelope {
    #[serde(default = "multi_channel_live_ingress_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub payload: Value,
}

fn multi_channel_live_ingress_schema_version() -> u32 {
    MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelLivePayloadIngestConfig` used across Tau components.
pub struct MultiChannelLivePayloadIngestConfig {
    pub ingress_dir: PathBuf,
    pub payload_file: PathBuf,
    pub transport: MultiChannelTransport,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `MultiChannelLivePayloadIngestReport` used across Tau components.
pub struct MultiChannelLivePayloadIngestReport {
    pub ingress_path: PathBuf,
    pub transport: MultiChannelTransport,
    pub provider: String,
    pub event_id: String,
    pub conversation_id: String,
}

pub fn default_multi_channel_live_provider_label(transport: MultiChannelTransport) -> &'static str {
    match transport {
        MultiChannelTransport::Telegram => "telegram-bot-api",
        MultiChannelTransport::Discord => "discord-gateway",
        MultiChannelTransport::Whatsapp => "whatsapp-cloud-api",
    }
}

pub fn build_multi_channel_live_envelope_from_raw_payload(
    transport: MultiChannelTransport,
    provider: &str,
    raw_payload: &str,
) -> Result<MultiChannelLiveInboundEnvelope, MultiChannelLiveIngressParseError> {
    let payload = serde_json::from_str::<Value>(raw_payload).map_err(|error| {
        parse_error(
            MultiChannelLiveIngressReasonCode::InvalidJson,
            error.to_string(),
        )
    })?;
    if !payload.is_object() {
        return Err(parse_error(
            MultiChannelLiveIngressReasonCode::InvalidFieldType,
            "provider payload must be a JSON object",
        ));
    }

    let provider = if provider.trim().is_empty() {
        default_multi_channel_live_provider_label(transport).to_string()
    } else {
        provider.trim().to_string()
    };

    let envelope = MultiChannelLiveInboundEnvelope {
        schema_version: MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION,
        transport: transport.as_str().to_string(),
        provider,
        payload,
    };
    // Validate and normalize before persisting to ingress NDJSON.
    parse_multi_channel_live_inbound_envelope_value(&envelope)?;
    Ok(envelope)
}

pub fn ingest_multi_channel_live_raw_payload(
    config: &MultiChannelLivePayloadIngestConfig,
) -> Result<MultiChannelLivePayloadIngestReport> {
    let raw_payload = std::fs::read_to_string(&config.payload_file).with_context(|| {
        format!(
            "failed to read multi-channel live ingest payload file {}",
            config.payload_file.display()
        )
    })?;
    let envelope = build_multi_channel_live_envelope_from_raw_payload(
        config.transport,
        &config.provider,
        &raw_payload,
    )
    .map_err(|error| {
        anyhow!(
            "multi-channel live ingest parse failure: reason_code={} detail={}",
            error.code.as_str(),
            error.message
        )
    })?;
    let event = parse_multi_channel_live_inbound_envelope_value(&envelope).map_err(|error| {
        anyhow!(
            "multi-channel live ingest validation failure: reason_code={} detail={}",
            error.code.as_str(),
            error.message
        )
    })?;

    std::fs::create_dir_all(&config.ingress_dir).with_context(|| {
        format!(
            "failed to create multi-channel live ingest directory {}",
            config.ingress_dir.display()
        )
    })?;
    let ingress_path = config
        .ingress_dir
        .join(format!("{}.ndjson", config.transport.as_str()));
    let encoded =
        serde_json::to_string(&envelope).context("failed to encode live ingress envelope")?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&ingress_path)
        .with_context(|| format!("failed to open {}", ingress_path.display()))?;
    file.write_all(encoded.as_bytes())
        .with_context(|| format!("failed to append {}", ingress_path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to append newline {}", ingress_path.display()))?;

    Ok(MultiChannelLivePayloadIngestReport {
        ingress_path,
        transport: config.transport,
        provider: envelope.provider,
        event_id: event.event_id,
        conversation_id: event.conversation_id,
    })
}

pub fn parse_multi_channel_live_inbound_envelope(
    raw: &str,
) -> Result<MultiChannelInboundEvent, MultiChannelLiveIngressParseError> {
    let envelope =
        serde_json::from_str::<MultiChannelLiveInboundEnvelope>(raw).map_err(|error| {
            parse_error(
                MultiChannelLiveIngressReasonCode::InvalidJson,
                error.to_string(),
            )
        })?;
    parse_multi_channel_live_inbound_envelope_value(&envelope)
}

pub fn parse_multi_channel_live_inbound_envelope_value(
    envelope: &MultiChannelLiveInboundEnvelope,
) -> Result<MultiChannelInboundEvent, MultiChannelLiveIngressParseError> {
    if envelope.schema_version != MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION {
        return Err(parse_error(
            MultiChannelLiveIngressReasonCode::UnsupportedSchemaVersion,
            format!(
                "expected schema_version {} but found {}",
                MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION, envelope.schema_version
            ),
        ));
    }
    let transport = parse_transport(&envelope.transport)?;
    let event = match transport {
        MultiChannelTransport::Telegram => parse_telegram_event(envelope)?,
        MultiChannelTransport::Discord => parse_discord_event(envelope)?,
        MultiChannelTransport::Whatsapp => parse_whatsapp_event(envelope)?,
    };

    if event.text.trim().is_empty() && event.attachments.is_empty() {
        return Err(parse_error(
            MultiChannelLiveIngressReasonCode::EmptyContent,
            "normalized event must include non-empty text or at least one attachment",
        ));
    }

    validate_multi_channel_inbound_event(&event).map_err(|error| {
        parse_error(
            MultiChannelLiveIngressReasonCode::InvalidNormalizedEvent,
            error.to_string(),
        )
    })?;

    Ok(event)
}

#[cfg(test)]
pub fn load_multi_channel_live_inbound_envelope_fixture(
    path: &Path,
) -> Result<MultiChannelInboundEvent, MultiChannelLiveIngressParseError> {
    let raw = std::fs::read_to_string(path).map_err(|error| {
        parse_error(
            MultiChannelLiveIngressReasonCode::InvalidJson,
            format!("failed to read {} ({error})", path.display()),
        )
    })?;
    parse_multi_channel_live_inbound_envelope(&raw)
}

fn parse_transport(
    raw_transport: &str,
) -> Result<MultiChannelTransport, MultiChannelLiveIngressParseError> {
    let normalized = raw_transport.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(parse_error(
            MultiChannelLiveIngressReasonCode::MissingTransport,
            "transport cannot be empty",
        ));
    }
    match normalized.as_str() {
        "telegram" => Ok(MultiChannelTransport::Telegram),
        "discord" => Ok(MultiChannelTransport::Discord),
        "whatsapp" => Ok(MultiChannelTransport::Whatsapp),
        unsupported => Err(parse_error(
            MultiChannelLiveIngressReasonCode::UnsupportedTransport,
            format!(
                "unsupported transport '{}' (expected telegram, discord, or whatsapp)",
                unsupported
            ),
        )),
    }
}

fn parse_telegram_event(
    envelope: &MultiChannelLiveInboundEnvelope,
) -> Result<MultiChannelInboundEvent, MultiChannelLiveIngressParseError> {
    let payload = as_object(
        &envelope.payload,
        MultiChannelLiveIngressReasonCode::MissingPayload,
        "payload must be a JSON object",
    )?;
    let message = object_field(
        payload,
        "message",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.message",
    )?;
    let chat = object_field(
        message,
        "chat",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.message.chat",
    )?;
    let from = object_field(
        message,
        "from",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.message.from",
    )?;

    let event_id = optional_string_field(message, "message_id")
        .or_else(|| optional_string_field(payload, "update_id"))
        .ok_or_else(|| {
            parse_error(
                MultiChannelLiveIngressReasonCode::MissingField,
                "payload.message.message_id or payload.update_id is required",
            )
        })?;

    let timestamp_secs = required_u64_field(
        message,
        "date",
        MultiChannelLiveIngressReasonCode::InvalidTimestamp,
        "payload.message.date",
    )?;

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "ingress_provider".to_string(),
        Value::String(envelope.provider.trim().to_string()),
    );
    if let Some(update_id) = optional_string_field(payload, "update_id") {
        metadata.insert("telegram_update_id".to_string(), Value::String(update_id));
    }

    Ok(MultiChannelInboundEvent {
        schema_version: MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION,
        transport: MultiChannelTransport::Telegram,
        event_kind: detect_event_kind(optional_string_field(message, "text").as_deref()),
        event_id,
        conversation_id: required_string_field(
            chat,
            "id",
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.message.chat.id",
        )?,
        thread_id: optional_string_field(message, "message_thread_id").unwrap_or_default(),
        actor_id: required_string_field(
            from,
            "id",
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.message.from.id",
        )?,
        actor_display: optional_string_field(from, "username")
            .or_else(|| optional_string_field(from, "first_name"))
            .unwrap_or_default(),
        timestamp_ms: timestamp_secs.saturating_mul(1000),
        text: optional_string_field(message, "text").unwrap_or_default(),
        attachments: parse_attachments(message.get("attachments"))?,
        metadata,
    })
}

fn parse_discord_event(
    envelope: &MultiChannelLiveInboundEnvelope,
) -> Result<MultiChannelInboundEvent, MultiChannelLiveIngressParseError> {
    let payload = as_object(
        &envelope.payload,
        MultiChannelLiveIngressReasonCode::MissingPayload,
        "payload must be a JSON object",
    )?;
    let author = object_field(
        payload,
        "author",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.author",
    )?;

    let timestamp_raw = required_string_field(
        payload,
        "timestamp",
        MultiChannelLiveIngressReasonCode::InvalidTimestamp,
        "payload.timestamp",
    )?;
    let timestamp_ms = parse_rfc3339_to_unix_ms(&timestamp_raw).ok_or_else(|| {
        parse_error(
            MultiChannelLiveIngressReasonCode::InvalidTimestamp,
            format!("payload.timestamp '{}' is not valid RFC3339", timestamp_raw),
        )
    })?;

    let thread_id = optional_object_field(payload, "thread")
        .and_then(|thread| optional_string_field(thread, "id"))
        .or_else(|| optional_string_field(payload, "thread_id"))
        .unwrap_or_default();

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "ingress_provider".to_string(),
        Value::String(envelope.provider.trim().to_string()),
    );

    Ok(MultiChannelInboundEvent {
        schema_version: MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION,
        transport: MultiChannelTransport::Discord,
        event_kind: detect_event_kind(optional_string_field(payload, "content").as_deref()),
        event_id: required_string_field(
            payload,
            "id",
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.id",
        )?,
        conversation_id: required_string_field(
            payload,
            "channel_id",
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.channel_id",
        )?,
        thread_id,
        actor_id: required_string_field(
            author,
            "id",
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.author.id",
        )?,
        actor_display: optional_string_field(author, "username").unwrap_or_default(),
        timestamp_ms,
        text: optional_string_field(payload, "content").unwrap_or_default(),
        attachments: parse_attachments(payload.get("attachments"))?,
        metadata,
    })
}

fn parse_whatsapp_event(
    envelope: &MultiChannelLiveInboundEnvelope,
) -> Result<MultiChannelInboundEvent, MultiChannelLiveIngressParseError> {
    let payload = as_object(
        &envelope.payload,
        MultiChannelLiveIngressReasonCode::MissingPayload,
        "payload must be a JSON object",
    )?;
    let messages = array_field(
        payload,
        "messages",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.messages",
    )?;
    let first = messages.first().ok_or_else(|| {
        parse_error(
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.messages must include at least one message object",
        )
    })?;
    let message = as_object(
        first,
        MultiChannelLiveIngressReasonCode::InvalidFieldType,
        "payload.messages[0] must be an object",
    )?;
    let metadata_object = object_field(
        payload,
        "metadata",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.metadata",
    )?;
    let phone_number_id = required_string_field(
        metadata_object,
        "phone_number_id",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.metadata.phone_number_id",
    )?;

    let timestamp_secs = required_u64_field(
        message,
        "timestamp",
        MultiChannelLiveIngressReasonCode::InvalidTimestamp,
        "payload.messages[0].timestamp",
    )?;
    let actor_id = required_string_field(
        message,
        "from",
        MultiChannelLiveIngressReasonCode::MissingField,
        "payload.messages[0].from",
    )?;

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "ingress_provider".to_string(),
        Value::String(envelope.provider.trim().to_string()),
    );
    metadata.insert(
        "whatsapp_phone_number_id".to_string(),
        Value::String(phone_number_id.clone()),
    );

    let text = message
        .get("text")
        .and_then(|raw| {
            as_object(raw, MultiChannelLiveIngressReasonCode::InvalidFieldType, "").ok()
        })
        .and_then(|raw| optional_string_field(raw, "body"))
        .unwrap_or_default();

    Ok(MultiChannelInboundEvent {
        schema_version: MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION,
        transport: MultiChannelTransport::Whatsapp,
        event_kind: detect_event_kind(Some(text.as_str())),
        event_id: required_string_field(
            message,
            "id",
            MultiChannelLiveIngressReasonCode::MissingField,
            "payload.messages[0].id",
        )?,
        conversation_id: format!("{phone_number_id}:{actor_id}"),
        thread_id: String::new(),
        actor_id,
        actor_display: String::new(),
        timestamp_ms: timestamp_secs.saturating_mul(1000),
        text,
        attachments: parse_attachments(message.get("attachments"))?,
        metadata,
    })
}

fn parse_attachments(
    raw_value: Option<&Value>,
) -> Result<Vec<MultiChannelAttachment>, MultiChannelLiveIngressParseError> {
    let Some(value) = raw_value else {
        return Ok(Vec::new());
    };
    let rows = value.as_array().ok_or_else(|| {
        parse_error(
            MultiChannelLiveIngressReasonCode::InvalidFieldType,
            "attachments must be an array",
        )
    })?;
    let mut attachments = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let row = as_object(
            row,
            MultiChannelLiveIngressReasonCode::InvalidFieldType,
            &format!("attachments[{index}] must be an object"),
        )?;
        attachments.push(MultiChannelAttachment {
            attachment_id: required_string_value(
                row.get("attachment_id").or_else(|| row.get("id")),
                MultiChannelLiveIngressReasonCode::MissingField,
                &format!("attachments[{index}].attachment_id"),
            )?,
            url: required_string_value(
                row.get("url"),
                MultiChannelLiveIngressReasonCode::MissingField,
                &format!("attachments[{index}].url"),
            )?,
            content_type: optional_string_value(row.get("content_type")).unwrap_or_default(),
            file_name: optional_string_value(row.get("file_name").or_else(|| row.get("name")))
                .unwrap_or_default(),
            size_bytes: optional_u64_value(row.get("size_bytes")).unwrap_or(0),
        });
    }
    Ok(attachments)
}

fn detect_event_kind(text: Option<&str>) -> MultiChannelEventKind {
    if text
        .map(str::trim)
        .map(|value| value.starts_with('/'))
        .unwrap_or(false)
    {
        MultiChannelEventKind::Command
    } else {
        MultiChannelEventKind::Message
    }
}

fn as_object<'a>(
    value: &'a Value,
    code: MultiChannelLiveIngressReasonCode,
    detail: &str,
) -> Result<&'a Map<String, Value>, MultiChannelLiveIngressParseError> {
    value.as_object().ok_or_else(|| parse_error(code, detail))
}

fn object_field<'a>(
    parent: &'a Map<String, Value>,
    key: &str,
    code: MultiChannelLiveIngressReasonCode,
    field_name: &str,
) -> Result<&'a Map<String, Value>, MultiChannelLiveIngressParseError> {
    let value = parent
        .get(key)
        .ok_or_else(|| parse_error(code, format!("{field_name} is required")))?;
    as_object(
        value,
        MultiChannelLiveIngressReasonCode::InvalidFieldType,
        &format!("{field_name} must be an object"),
    )
}

fn optional_object_field<'a>(
    parent: &'a Map<String, Value>,
    key: &str,
) -> Option<&'a Map<String, Value>> {
    parent.get(key).and_then(Value::as_object)
}

fn array_field<'a>(
    parent: &'a Map<String, Value>,
    key: &str,
    code: MultiChannelLiveIngressReasonCode,
    field_name: &str,
) -> Result<&'a Vec<Value>, MultiChannelLiveIngressParseError> {
    parent
        .get(key)
        .ok_or_else(|| parse_error(code, format!("{field_name} is required")))?
        .as_array()
        .ok_or_else(|| {
            parse_error(
                MultiChannelLiveIngressReasonCode::InvalidFieldType,
                format!("{field_name} must be an array"),
            )
        })
}

fn required_string_field(
    object: &Map<String, Value>,
    key: &str,
    code: MultiChannelLiveIngressReasonCode,
    field_name: &str,
) -> Result<String, MultiChannelLiveIngressParseError> {
    required_string_value(object.get(key), code, field_name)
}

fn required_u64_field(
    object: &Map<String, Value>,
    key: &str,
    code: MultiChannelLiveIngressReasonCode,
    field_name: &str,
) -> Result<u64, MultiChannelLiveIngressParseError> {
    required_u64_value(object.get(key), code, field_name)
}

fn required_string_value(
    value: Option<&Value>,
    code: MultiChannelLiveIngressReasonCode,
    field_name: &str,
) -> Result<String, MultiChannelLiveIngressParseError> {
    let parsed = optional_string_value(value);
    let Some(parsed) = parsed else {
        return Err(parse_error(code, format!("{field_name} is required")));
    };
    if parsed.trim().is_empty() {
        return Err(parse_error(code, format!("{field_name} cannot be empty")));
    }
    Ok(parsed)
}

fn required_u64_value(
    value: Option<&Value>,
    code: MultiChannelLiveIngressReasonCode,
    field_name: &str,
) -> Result<u64, MultiChannelLiveIngressParseError> {
    let parsed = optional_u64_value(value);
    let Some(parsed) = parsed else {
        return Err(parse_error(code, format!("{field_name} is required")));
    };
    if parsed == 0 {
        return Err(parse_error(
            code,
            format!("{field_name} must be greater than 0"),
        ));
    }
    Ok(parsed)
}

fn optional_string_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    optional_string_value(object.get(key))
}

fn optional_string_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    match value {
        Value::String(raw) => Some(raw.trim().to_string()),
        Value::Number(raw) => Some(raw.to_string()),
        _ => None,
    }
}

fn optional_u64_value(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    match value {
        Value::Number(raw) => raw.as_u64(),
        Value::String(raw) => raw.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn parse_rfc3339_to_unix_ms(raw: &str) -> Option<u64> {
    let parsed = DateTime::parse_from_rfc3339(raw).ok()?;
    u64::try_from(parsed.timestamp_millis()).ok()
}

fn parse_error(
    code: MultiChannelLiveIngressReasonCode,
    message: impl Into<String>,
) -> MultiChannelLiveIngressParseError {
    MultiChannelLiveIngressParseError {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::multi_channel_contract::MultiChannelTransport;

    use super::{
        build_multi_channel_live_envelope_from_raw_payload,
        default_multi_channel_live_provider_label, ingest_multi_channel_live_raw_payload,
        load_multi_channel_live_inbound_envelope_fixture,
        parse_multi_channel_live_inbound_envelope, MultiChannelLiveIngressReasonCode,
        MultiChannelLivePayloadIngestConfig,
    };

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-channel-live-ingress")
            .join(name)
    }

    fn fixture_raw(name: &str) -> String {
        std::fs::read_to_string(fixture_path(name)).expect("fixture should load")
    }

    fn fixture_payload(name: &str) -> String {
        let value: serde_json::Value =
            serde_json::from_str(&fixture_raw(name)).expect("fixture json should parse");
        serde_json::to_string_pretty(value.get("payload").expect("payload field should exist"))
            .expect("payload json should encode")
    }

    #[test]
    fn unit_parse_telegram_envelope_maps_expected_fields() {
        let event = parse_multi_channel_live_inbound_envelope(&fixture_raw("telegram-valid.json"))
            .expect("telegram fixture should parse");
        assert_eq!(event.transport, MultiChannelTransport::Telegram);
        assert_eq!(event.event_id, "42");
        assert_eq!(event.conversation_id, "chat-100");
        assert_eq!(event.actor_id, "user-7");
        assert_eq!(event.text, "hello from telegram");
        assert_eq!(event.timestamp_ms, 1_760_100_000_000);
    }

    #[test]
    fn unit_parse_discord_envelope_maps_expected_fields() {
        let event = parse_multi_channel_live_inbound_envelope(&fixture_raw("discord-valid.json"))
            .expect("discord fixture should parse");
        assert_eq!(event.transport, MultiChannelTransport::Discord);
        assert_eq!(event.event_id, "discord-msg-1");
        assert_eq!(event.conversation_id, "discord-channel-88");
        assert_eq!(event.actor_id, "discord-user-3");
        assert_eq!(event.text, "/status");
        assert!(event.timestamp_ms > 0);
    }

    #[test]
    fn unit_parse_whatsapp_envelope_maps_expected_fields() {
        let event = parse_multi_channel_live_inbound_envelope(&fixture_raw("whatsapp-valid.json"))
            .expect("whatsapp fixture should parse");
        assert_eq!(event.transport, MultiChannelTransport::Whatsapp);
        assert_eq!(event.event_id, "wamid.HBg123");
        assert_eq!(event.conversation_id, "phone-55:15551234567");
        assert_eq!(event.actor_id, "15551234567");
        assert_eq!(event.text, "hello from whatsapp");
        assert_eq!(event.timestamp_ms, 1_760_300_000_000);
    }

    #[test]
    fn functional_live_ingress_fixtures_cover_all_supported_transports() {
        let parsed = [
            load_multi_channel_live_inbound_envelope_fixture(&fixture_path("telegram-valid.json"))
                .expect("telegram fixture should parse"),
            load_multi_channel_live_inbound_envelope_fixture(&fixture_path("discord-valid.json"))
                .expect("discord fixture should parse"),
            load_multi_channel_live_inbound_envelope_fixture(&fixture_path("whatsapp-valid.json"))
                .expect("whatsapp fixture should parse"),
        ];
        let transports = parsed
            .iter()
            .map(|event| event.transport)
            .collect::<HashSet<_>>();
        assert_eq!(transports.len(), 3);
        assert!(transports.contains(&MultiChannelTransport::Telegram));
        assert!(transports.contains(&MultiChannelTransport::Discord));
        assert!(transports.contains(&MultiChannelTransport::Whatsapp));
    }

    #[test]
    fn integration_live_ingress_parser_is_deterministic_across_reloads() {
        let raw = fixture_raw("telegram-valid.json");
        let first =
            parse_multi_channel_live_inbound_envelope(&raw).expect("first parse should pass");
        let second =
            parse_multi_channel_live_inbound_envelope(&raw).expect("second parse should pass");
        assert_eq!(first, second);
    }

    #[test]
    fn regression_rejects_unsupported_transport_with_reason_code() {
        let error = parse_multi_channel_live_inbound_envelope(&fixture_raw(
            "invalid-unsupported-transport.json",
        ))
        .expect_err("unsupported transport should fail");
        assert_eq!(
            error.code,
            MultiChannelLiveIngressReasonCode::UnsupportedTransport
        );
    }

    #[test]
    fn regression_rejects_missing_discord_author_with_reason_code() {
        let error = parse_multi_channel_live_inbound_envelope(&fixture_raw(
            "invalid-discord-missing-author.json",
        ))
        .expect_err("missing discord author should fail");
        assert_eq!(error.code, MultiChannelLiveIngressReasonCode::MissingField);
        assert!(error.message.contains("payload.author"));
    }

    #[test]
    fn unit_build_envelope_from_raw_payload_maps_transport_and_defaults_provider() {
        let raw_payload = fixture_payload("telegram-valid.json");
        let envelope = build_multi_channel_live_envelope_from_raw_payload(
            MultiChannelTransport::Telegram,
            "",
            &raw_payload,
        )
        .expect("raw telegram payload should normalize");
        assert_eq!(envelope.transport, "telegram");
        assert_eq!(
            envelope.provider,
            default_multi_channel_live_provider_label(MultiChannelTransport::Telegram)
        );
        let event = super::parse_multi_channel_live_inbound_envelope_value(&envelope)
            .expect("normalized envelope should parse");
        assert_eq!(event.event_id, "42");
        assert_eq!(event.conversation_id, "chat-100");
    }

    #[test]
    fn functional_build_envelope_from_raw_payload_supports_three_transports() {
        let telegram = build_multi_channel_live_envelope_from_raw_payload(
            MultiChannelTransport::Telegram,
            "telegram-bot-api",
            &fixture_payload("telegram-valid.json"),
        )
        .expect("telegram payload should parse");
        let discord = build_multi_channel_live_envelope_from_raw_payload(
            MultiChannelTransport::Discord,
            "discord-gateway",
            &fixture_payload("discord-valid.json"),
        )
        .expect("discord payload should parse");
        let whatsapp = build_multi_channel_live_envelope_from_raw_payload(
            MultiChannelTransport::Whatsapp,
            "whatsapp-cloud-api",
            &fixture_payload("whatsapp-valid.json"),
        )
        .expect("whatsapp payload should parse");
        assert_eq!(telegram.transport, "telegram");
        assert_eq!(discord.transport, "discord");
        assert_eq!(whatsapp.transport, "whatsapp");
    }

    #[test]
    fn integration_ingest_multi_channel_live_raw_payload_appends_ndjson_row() {
        let temp = tempdir().expect("tempdir");
        let ingress_dir = temp.path().join("ingress");
        let payload_file = temp.path().join("telegram-raw.json");
        std::fs::write(&payload_file, fixture_payload("telegram-valid.json")).expect("write raw");

        let report = ingest_multi_channel_live_raw_payload(&MultiChannelLivePayloadIngestConfig {
            ingress_dir: ingress_dir.clone(),
            payload_file: payload_file.clone(),
            transport: MultiChannelTransport::Telegram,
            provider: "telegram-bot-api".to_string(),
        })
        .expect("ingest should succeed");
        assert_eq!(report.transport, MultiChannelTransport::Telegram);
        assert_eq!(report.event_id, "42");
        assert!(report.ingress_path.ends_with("telegram.ndjson"));

        let lines = std::fs::read_to_string(ingress_dir.join("telegram.ndjson"))
            .expect("read ingress file")
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let replay =
            parse_multi_channel_live_inbound_envelope(lines[0].as_str()).expect("parse line");
        assert_eq!(replay.event_id, "42");
    }

    #[test]
    fn regression_build_envelope_from_raw_payload_rejects_non_object_payload() {
        let error = build_multi_channel_live_envelope_from_raw_payload(
            MultiChannelTransport::Discord,
            "discord-gateway",
            "\"not-an-object\"",
        )
        .expect_err("payload should fail");
        assert_eq!(
            error.code,
            MultiChannelLiveIngressReasonCode::InvalidFieldType
        );
        assert!(error.message.contains("provider payload"));
    }
}
