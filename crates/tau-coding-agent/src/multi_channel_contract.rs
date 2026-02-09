use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION: u32 = 1;

fn multi_channel_contract_schema_version() -> u32 {
    MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MultiChannelTransport {
    Telegram,
    Discord,
    Whatsapp,
}

impl MultiChannelTransport {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
            Self::Whatsapp => "whatsapp",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MultiChannelEventKind {
    Message,
    Edit,
    Command,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MultiChannelAttachment {
    pub(crate) attachment_id: String,
    pub(crate) url: String,
    #[serde(default)]
    pub(crate) content_type: String,
    #[serde(default)]
    pub(crate) file_name: String,
    #[serde(default)]
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct MultiChannelInboundEvent {
    #[serde(default = "multi_channel_contract_schema_version")]
    pub(crate) schema_version: u32,
    pub(crate) transport: MultiChannelTransport,
    pub(crate) event_kind: MultiChannelEventKind,
    pub(crate) event_id: String,
    pub(crate) conversation_id: String,
    #[serde(default)]
    pub(crate) thread_id: String,
    pub(crate) actor_id: String,
    #[serde(default)]
    pub(crate) actor_display: String,
    pub(crate) timestamp_ms: u64,
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) attachments: Vec<MultiChannelAttachment>,
    #[serde(default)]
    pub(crate) metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct MultiChannelContractFixture {
    pub(crate) schema_version: u32,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) events: Vec<MultiChannelInboundEvent>,
}

pub(crate) fn parse_multi_channel_contract_fixture(
    raw: &str,
) -> Result<MultiChannelContractFixture> {
    let fixture = serde_json::from_str::<MultiChannelContractFixture>(raw)
        .context("failed to parse multi-channel contract fixture")?;
    validate_multi_channel_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub(crate) fn load_multi_channel_contract_fixture(
    path: &Path,
) -> Result<MultiChannelContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_multi_channel_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub(crate) fn validate_multi_channel_contract_fixture(
    fixture: &MultiChannelContractFixture,
) -> Result<()> {
    if fixture.schema_version != MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported multi-channel contract schema version {} (expected {})",
            fixture.schema_version,
            MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION
        );
    }
    if fixture.name.trim().is_empty() {
        bail!("fixture name cannot be empty");
    }
    if fixture.events.is_empty() {
        bail!("fixture must include at least one event");
    }

    let mut event_keys = HashSet::new();
    for (index, event) in fixture.events.iter().enumerate() {
        validate_multi_channel_event(event, index)?;
        let key = event_contract_key(event);
        if !event_keys.insert(key.clone()) {
            bail!("fixture contains duplicate transport event key '{key}'");
        }
    }

    Ok(())
}

fn validate_multi_channel_event(event: &MultiChannelInboundEvent, index: usize) -> Result<()> {
    if event.schema_version != MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture event index {} has unsupported schema_version {} (expected {})",
            index,
            event.schema_version,
            MULTI_CHANNEL_CONTRACT_SCHEMA_VERSION
        );
    }
    if event.event_id.trim().is_empty() {
        bail!("fixture event index {} has empty event_id", index);
    }
    if event.conversation_id.trim().is_empty() {
        bail!("fixture event index {} has empty conversation_id", index);
    }
    if event.actor_id.trim().is_empty() {
        bail!("fixture event index {} has empty actor_id", index);
    }
    if event.timestamp_ms == 0 {
        bail!("fixture event index {} has zero timestamp_ms", index);
    }
    if event.text.trim().is_empty() && event.attachments.is_empty() {
        bail!(
            "fixture event index {} must include non-empty text or at least one attachment",
            index
        );
    }
    if event.metadata.keys().any(|key| key.trim().is_empty()) {
        bail!("fixture event index {} includes empty metadata key", index);
    }

    let mut attachment_ids = HashSet::new();
    for attachment in &event.attachments {
        validate_attachment(attachment, index)?;
        let trimmed_id = attachment.attachment_id.trim().to_string();
        if !attachment_ids.insert(trimmed_id.clone()) {
            bail!(
                "fixture event index {} includes duplicate attachment_id '{}'",
                index,
                trimmed_id
            );
        }
    }

    Ok(())
}

fn validate_attachment(attachment: &MultiChannelAttachment, event_index: usize) -> Result<()> {
    if attachment.attachment_id.trim().is_empty() {
        bail!(
            "fixture event index {} has attachment with empty attachment_id",
            event_index
        );
    }
    let url = attachment.url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://localhost")) {
        bail!(
            "fixture event index {} has invalid attachment url '{}'",
            event_index,
            attachment.url
        );
    }
    if !attachment.content_type.trim().is_empty() && !attachment.content_type.contains('/') {
        bail!(
            "fixture event index {} has invalid content_type '{}'",
            event_index,
            attachment.content_type
        );
    }
    Ok(())
}

pub(crate) fn event_contract_key(event: &MultiChannelInboundEvent) -> String {
    format!("{}:{}", event.transport.as_str(), event.event_id.trim())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
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
