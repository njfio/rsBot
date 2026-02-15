//! Multi-channel contract schema and fixture parsing.
//!
//! This module defines transport contract types and validation helpers used by
//! runtime and bridge entrypoints. It enforces schema/header checks so ingress
//! and routing code only consumes well-formed contract fixtures.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tau_contract::{
    load_fixture_from_path, parse_fixture_with_validation,
    validate_fixture_header_with_empty_message,
};

pub const MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION: u32 = 1;

fn multi_channel_contract_schema_version() -> u32 {
    MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelTransport` values.
pub enum MultiChannelTransport {
    Telegram,
    Discord,
    Whatsapp,
}

impl MultiChannelTransport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
            Self::Whatsapp => "whatsapp",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelEventKind` values.
pub enum MultiChannelEventKind {
    Message,
    Edit,
    Command,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelAttachment` used across Tau components.
pub struct MultiChannelAttachment {
    pub attachment_id: String,
    pub url: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `MultiChannelInboundEvent` used across Tau components.
pub struct MultiChannelInboundEvent {
    #[serde(default = "multi_channel_contract_schema_version")]
    pub schema_version: u32,
    pub transport: MultiChannelTransport,
    pub event_kind: MultiChannelEventKind,
    pub event_id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub thread_id: String,
    pub actor_id: String,
    #[serde(default)]
    pub actor_display: String,
    pub timestamp_ms: u64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<MultiChannelAttachment>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `MultiChannelContractFixture` used across Tau components.
pub struct MultiChannelContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub events: Vec<MultiChannelInboundEvent>,
}

pub fn parse_multi_channel_contract_fixture(raw: &str) -> Result<MultiChannelContractFixture> {
    parse_fixture_with_validation(
        raw,
        "failed to parse multi-channel contract fixture",
        validate_multi_channel_contract_fixture,
    )
}

pub fn load_multi_channel_contract_fixture(path: &Path) -> Result<MultiChannelContractFixture> {
    load_fixture_from_path(path, parse_multi_channel_contract_fixture)
}

pub fn validate_multi_channel_contract_fixture(
    fixture: &MultiChannelContractFixture,
) -> Result<()> {
    validate_fixture_header_with_empty_message(
        "multi-channel",
        fixture.schema_version,
        MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION,
        &fixture.name,
        fixture.events.len(),
        "fixture must include at least one event",
    )?;

    let mut event_keys = HashSet::new();
    for (index, event) in fixture.events.iter().enumerate() {
        validate_multi_channel_event_with_label(event, &format!("fixture event index {}", index))?;
        let key = event_contract_key(event);
        if !event_keys.insert(key.clone()) {
            bail!("fixture contains duplicate transport event key '{key}'");
        }
    }

    Ok(())
}

pub fn validate_multi_channel_inbound_event(event: &MultiChannelInboundEvent) -> Result<()> {
    validate_multi_channel_event_with_label(event, "live ingress event")
}

fn validate_multi_channel_event_with_label(
    event: &MultiChannelInboundEvent,
    label: &str,
) -> Result<()> {
    if event.schema_version != MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION {
        bail!(
            "{label} has unsupported schema_version {} (expected {})",
            event.schema_version,
            MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION
        );
    }
    if event.event_id.trim().is_empty() {
        bail!("{label} has empty event_id");
    }
    if event.conversation_id.trim().is_empty() {
        bail!("{label} has empty conversation_id");
    }
    if event.actor_id.trim().is_empty() {
        bail!("{label} has empty actor_id");
    }
    if event.timestamp_ms == 0 {
        bail!("{label} has zero timestamp_ms");
    }
    if event.text.trim().is_empty() && event.attachments.is_empty() {
        bail!("{label} must include non-empty text or at least one attachment");
    }
    if event.metadata.keys().any(|key| key.trim().is_empty()) {
        bail!("{label} includes empty metadata key");
    }

    let mut attachment_ids = HashSet::new();
    for attachment in &event.attachments {
        validate_attachment(attachment, label)?;
        let trimmed_id = attachment.attachment_id.trim().to_string();
        if !attachment_ids.insert(trimmed_id.clone()) {
            bail!("{label} includes duplicate attachment_id '{}'", trimmed_id);
        }
    }

    Ok(())
}

fn validate_attachment(attachment: &MultiChannelAttachment, label: &str) -> Result<()> {
    if attachment.attachment_id.trim().is_empty() {
        bail!("{label} has attachment with empty attachment_id");
    }
    let url = attachment.url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://localhost")) {
        bail!("{label} has invalid attachment url '{}'", attachment.url);
    }
    if !attachment.content_type.trim().is_empty() && !attachment.content_type.contains('/') {
        bail!(
            "{label} has invalid content_type '{}'",
            attachment.content_type
        );
    }
    Ok(())
}

pub fn event_contract_key(event: &MultiChannelInboundEvent) -> String {
    format!("{}:{}", event.transport.as_str(), event.event_id.trim())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};
    use std::path::{Path, PathBuf};

    use super::{
        event_contract_key, load_multi_channel_contract_fixture,
        parse_multi_channel_contract_fixture, MultiChannelTransport,
    };

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-channel-contract")
            .join(name)
    }

    #[test]
    fn unit_parse_multi_channel_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "unsupported",
  "events": [
    {
      "schema_version": 1,
      "transport": "telegram",
      "event_kind": "message",
      "event_id": "evt-1",
      "conversation_id": "chat-1",
      "actor_id": "user-1",
      "timestamp_ms": 1,
      "text": "hello"
    }
  ]
}"#;
        let error = parse_multi_channel_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported multi-channel contract schema version"));
    }

    #[test]
    fn unit_validate_multi_channel_event_rejects_empty_identifier() {
        let raw = r#"{
  "schema_version": 1,
  "name": "bad-event-id",
  "events": [
    {
      "schema_version": 1,
      "transport": "discord",
      "event_kind": "message",
      "event_id": " ",
      "conversation_id": "channel-1",
      "actor_id": "user-1",
      "timestamp_ms": 1,
      "text": "hello"
    }
  ]
}"#;
        let error =
            parse_multi_channel_contract_fixture(raw).expect_err("empty event id should fail");
        assert!(error
            .to_string()
            .contains("fixture event index 0 has empty event_id"));
    }

    #[test]
    fn unit_validate_multi_channel_inbound_event_rejects_empty_actor() {
        use super::{
            validate_multi_channel_inbound_event, MultiChannelEventKind, MultiChannelInboundEvent,
        };

        let event = MultiChannelInboundEvent {
            schema_version: 1,
            transport: MultiChannelTransport::Telegram,
            event_kind: MultiChannelEventKind::Message,
            event_id: "evt-1".to_string(),
            conversation_id: "chat-1".to_string(),
            thread_id: String::new(),
            actor_id: " ".to_string(),
            actor_display: String::new(),
            timestamp_ms: 1,
            text: "hello".to_string(),
            attachments: Vec::new(),
            metadata: BTreeMap::new(),
        };

        let error = validate_multi_channel_inbound_event(&event)
            .expect_err("empty actor id should be rejected");
        assert!(error
            .to_string()
            .contains("live ingress event has empty actor_id"));
    }

    #[test]
    fn functional_fixture_baseline_supports_telegram_discord_whatsapp() {
        let fixture =
            load_multi_channel_contract_fixture(&fixture_path("baseline-three-channel.json"))
                .expect("fixture should load");
        let transports = fixture
            .events
            .iter()
            .map(|event| event.transport)
            .collect::<HashSet<MultiChannelTransport>>();
        assert_eq!(fixture.events.len(), 3);
        assert!(transports.contains(&MultiChannelTransport::Telegram));
        assert!(transports.contains(&MultiChannelTransport::Discord));
        assert!(transports.contains(&MultiChannelTransport::Whatsapp));
    }

    #[test]
    fn integration_fixture_loader_is_deterministic_across_reloads() {
        let path = fixture_path("baseline-three-channel.json");
        let first = load_multi_channel_contract_fixture(&path).expect("first load");
        let second = load_multi_channel_contract_fixture(&path).expect("second load");
        assert_eq!(first, second);
    }

    #[test]
    fn integration_fixture_roundtrip_preserves_event_contract_keys() {
        let fixture =
            load_multi_channel_contract_fixture(&fixture_path("baseline-three-channel.json"))
                .expect("fixture should load");
        let first_keys = fixture
            .events
            .iter()
            .map(event_contract_key)
            .collect::<Vec<String>>();
        let serialized = serde_json::to_string(&fixture).expect("serialize");
        let roundtrip = parse_multi_channel_contract_fixture(&serialized).expect("roundtrip parse");
        let second_keys = roundtrip
            .events
            .iter()
            .map(event_contract_key)
            .collect::<Vec<String>>();
        assert_eq!(first_keys, second_keys);
    }

    #[test]
    fn regression_fixture_rejects_duplicate_transport_event_key() {
        let error = load_multi_channel_contract_fixture(&fixture_path("duplicate-event-key.json"))
            .expect_err("duplicate key fixture should fail");
        let message = format!("{error:#}");
        assert!(message.contains("duplicate transport event key"));
    }

    #[test]
    fn regression_fixture_rejects_attachment_without_https_url() {
        let error =
            load_multi_channel_contract_fixture(&fixture_path("invalid-attachment-url.json"))
                .expect_err("invalid attachment fixture should fail");
        let message = format!("{error:#}");
        assert!(message.contains("invalid attachment url"));
    }
}
