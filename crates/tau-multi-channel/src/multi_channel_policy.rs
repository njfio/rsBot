use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::multi_channel_contract::{MultiChannelEventKind, MultiChannelInboundEvent};

pub const MULTI_CHANNEL_POLICY_SCHEMA_VERSION: u32 = 1;
pub const MULTI_CHANNEL_POLICY_FILE_NAME: &str = "channel-policy.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelDmPolicy` values.
pub enum MultiChannelDmPolicy {
    #[default]
    Allow,
    Deny,
}

impl MultiChannelDmPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelAllowFrom` values.
pub enum MultiChannelAllowFrom {
    Any,
    #[default]
    AllowlistOrPairing,
    AllowlistOnly,
}

impl MultiChannelAllowFrom {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::AllowlistOrPairing => "allowlist_or_pairing",
            Self::AllowlistOnly => "allowlist_only",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelGroupPolicy` values.
pub enum MultiChannelGroupPolicy {
    #[default]
    Allow,
    Deny,
}

impl MultiChannelGroupPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `MultiChannelChannelPolicy` used across Tau components.
pub struct MultiChannelChannelPolicy {
    #[serde(default, rename = "dmPolicy")]
    pub dm_policy: MultiChannelDmPolicy,
    #[serde(default, rename = "allowFrom")]
    pub allow_from: MultiChannelAllowFrom,
    #[serde(default, rename = "groupPolicy")]
    pub group_policy: MultiChannelGroupPolicy,
    #[serde(default, rename = "requireMention")]
    pub require_mention: bool,
}

impl MultiChannelChannelPolicy {
    fn is_open_dm_combo(&self) -> bool {
        self.dm_policy == MultiChannelDmPolicy::Allow
            && self.allow_from == MultiChannelAllowFrom::Any
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelPolicyFile` used across Tau components.
pub struct MultiChannelPolicyFile {
    pub schema_version: u32,
    #[serde(default, rename = "strictMode")]
    pub strict_mode: bool,
    #[serde(default, rename = "defaultPolicy")]
    pub default_policy: MultiChannelChannelPolicy,
    #[serde(default)]
    pub channels: BTreeMap<String, MultiChannelChannelPolicy>,
}

impl Default for MultiChannelPolicyFile {
    fn default() -> Self {
        Self {
            schema_version: MULTI_CHANNEL_POLICY_SCHEMA_VERSION,
            strict_mode: false,
            default_policy: MultiChannelChannelPolicy::default(),
            channels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelConversationKind` values.
pub enum MultiChannelConversationKind {
    Dm,
    Group,
}

impl MultiChannelConversationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dm => "dm",
            Self::Group => "group",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Enumerates supported `MultiChannelPolicyDecision` values.
pub enum MultiChannelPolicyDecision {
    Allow { reason_code: String },
    Deny { reason_code: String },
}

impl MultiChannelPolicyDecision {
    pub fn reason_code(&self) -> &str {
        match self {
            Self::Allow { reason_code } | Self::Deny { reason_code } => reason_code,
        }
    }

    pub fn as_str(&self) -> &'static str {
        if matches!(self, Self::Allow { .. }) {
            "allow"
        } else {
            "deny"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Public struct `MultiChannelPolicyEvaluation` used across Tau components.
pub struct MultiChannelPolicyEvaluation {
    pub policy_channel: String,
    pub matched_policy_key: String,
    pub policy: MultiChannelChannelPolicy,
    pub conversation_kind: MultiChannelConversationKind,
    pub mention_present: bool,
    pub decision: MultiChannelPolicyDecision,
}

pub fn channel_policy_path_for_state_dir(state_dir: &Path) -> PathBuf {
    let state_name = state_dir.file_name().and_then(|value| value.to_str());
    let tau_root = match state_name {
        Some("github")
        | Some("slack")
        | Some("events")
        | Some("channel-store")
        | Some("multi-channel") => state_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(state_dir),
        _ => state_dir,
    };
    tau_root
        .join("security")
        .join(MULTI_CHANNEL_POLICY_FILE_NAME)
}

pub fn load_multi_channel_policy_for_state_dir(state_dir: &Path) -> Result<MultiChannelPolicyFile> {
    let path = channel_policy_path_for_state_dir(state_dir);
    load_multi_channel_policy_file(&path)
        .with_context(|| format!("failed to load multi-channel policy {}", path.display()))
}

pub fn load_multi_channel_policy_file(path: &Path) -> Result<MultiChannelPolicyFile> {
    if !path.exists() {
        return Ok(MultiChannelPolicyFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read multi-channel policy {}", path.display()))?;
    let parsed = serde_json::from_str::<MultiChannelPolicyFile>(&raw)
        .with_context(|| format!("failed to parse multi-channel policy {}", path.display()))?;
    validate_multi_channel_policy(&parsed)?;
    Ok(parsed)
}

fn validate_multi_channel_policy(policy: &MultiChannelPolicyFile) -> Result<()> {
    if policy.schema_version != MULTI_CHANNEL_POLICY_SCHEMA_VERSION {
        bail!(
            "unsupported multi-channel policy schema_version {} (expected {})",
            policy.schema_version,
            MULTI_CHANNEL_POLICY_SCHEMA_VERSION
        );
    }
    for key in policy.channels.keys() {
        if key.trim().is_empty() {
            bail!("multi-channel policy channel key must not be empty");
        }
    }
    Ok(())
}

pub fn evaluate_multi_channel_channel_policy(
    policy_file: &MultiChannelPolicyFile,
    event: &MultiChannelInboundEvent,
) -> MultiChannelPolicyEvaluation {
    let policy_channel = format!(
        "{}:{}",
        event.transport.as_str(),
        event.conversation_id.trim()
    );
    let transport_wildcard = format!("{}:*", event.transport.as_str());
    let (matched_policy_key, policy) =
        if let Some(channel_policy) = policy_file.channels.get(&policy_channel) {
            (policy_channel.clone(), channel_policy.clone())
        } else if let Some(channel_policy) = policy_file.channels.get(&transport_wildcard) {
            (transport_wildcard, channel_policy.clone())
        } else if let Some(channel_policy) = policy_file.channels.get("*") {
            ("*".to_string(), channel_policy.clone())
        } else {
            ("default".to_string(), policy_file.default_policy.clone())
        };

    let conversation_kind = detect_conversation_kind(event);
    let mention_present = detect_mention_present(event);

    let decision = match conversation_kind {
        MultiChannelConversationKind::Dm => {
            if policy.dm_policy == MultiChannelDmPolicy::Deny {
                MultiChannelPolicyDecision::Deny {
                    reason_code: "deny_channel_policy_dm".to_string(),
                }
            } else {
                allow_decision_for_allow_from(policy.allow_from)
            }
        }
        MultiChannelConversationKind::Group => {
            if policy.group_policy == MultiChannelGroupPolicy::Deny {
                MultiChannelPolicyDecision::Deny {
                    reason_code: "deny_channel_policy_group".to_string(),
                }
            } else if policy.require_mention && !mention_present {
                MultiChannelPolicyDecision::Deny {
                    reason_code: "deny_channel_policy_mention_required".to_string(),
                }
            } else {
                allow_decision_for_allow_from(policy.allow_from)
            }
        }
    };

    MultiChannelPolicyEvaluation {
        policy_channel,
        matched_policy_key,
        policy,
        conversation_kind,
        mention_present,
        decision,
    }
}

pub fn collect_open_dm_risk_channels(policy_file: &MultiChannelPolicyFile) -> Vec<String> {
    let mut channels = Vec::new();
    if policy_file.default_policy.is_open_dm_combo() {
        channels.push("default".to_string());
    }
    for (key, policy) in &policy_file.channels {
        if policy.is_open_dm_combo() {
            channels.push(key.to_string());
        }
    }
    channels.sort();
    channels.dedup();
    channels
}

fn allow_decision_for_allow_from(allow_from: MultiChannelAllowFrom) -> MultiChannelPolicyDecision {
    let reason_code = match allow_from {
        MultiChannelAllowFrom::Any => "allow_channel_policy_allow_from_any",
        MultiChannelAllowFrom::AllowlistOrPairing => {
            "allow_channel_policy_allow_from_allowlist_or_pairing"
        }
        MultiChannelAllowFrom::AllowlistOnly => "allow_channel_policy_allow_from_allowlist_only",
    };
    MultiChannelPolicyDecision::Allow {
        reason_code: reason_code.to_string(),
    }
}

fn detect_conversation_kind(event: &MultiChannelInboundEvent) -> MultiChannelConversationKind {
    if event.transport.as_str() == "whatsapp" {
        return MultiChannelConversationKind::Dm;
    }

    if metadata_string_matches(event, "conversation_mode", &["dm"]) {
        return MultiChannelConversationKind::Dm;
    }
    if metadata_string_matches(event, "chat_type", &["private", "dm", "direct"]) {
        return MultiChannelConversationKind::Dm;
    }
    if metadata_string_matches(event, "channel_type", &["dm", "private", "direct"]) {
        return MultiChannelConversationKind::Dm;
    }
    if metadata_bool(event, "is_dm") {
        return MultiChannelConversationKind::Dm;
    }

    if metadata_string_matches(event, "conversation_mode", &["group"]) {
        return MultiChannelConversationKind::Group;
    }
    if metadata_string_matches(event, "chat_type", &["group", "supergroup", "channel"]) {
        return MultiChannelConversationKind::Group;
    }
    if metadata_string_matches(
        event,
        "channel_type",
        &["group", "guild", "public_thread", "private_thread", "text"],
    ) {
        return MultiChannelConversationKind::Group;
    }
    if event
        .metadata
        .get("guild_id")
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return MultiChannelConversationKind::Group;
    }
    if !event.thread_id.trim().is_empty() {
        return MultiChannelConversationKind::Group;
    }
    MultiChannelConversationKind::Group
}

fn detect_mention_present(event: &MultiChannelInboundEvent) -> bool {
    if matches!(event.event_kind, MultiChannelEventKind::Command) {
        return true;
    }
    if metadata_bool(event, "mentions_bot")
        || metadata_bool(event, "mentioned")
        || metadata_bool(event, "mention")
    {
        return true;
    }
    if event
        .metadata
        .get("mention_count")
        .and_then(Value::as_u64)
        .map(|value| value > 0)
        .unwrap_or(false)
    {
        return true;
    }
    if event
        .metadata
        .get("mentions")
        .and_then(Value::as_array)
        .map(|rows| !rows.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    let text = event.text.to_ascii_lowercase();
    text.contains("@tau") || text.contains("<@") || text.contains("/tau")
}

fn metadata_bool(event: &MultiChannelInboundEvent, key: &str) -> bool {
    event
        .metadata
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn metadata_string_matches(event: &MultiChannelInboundEvent, key: &str, accepted: &[&str]) -> bool {
    let Some(value) = event.metadata.get(key).and_then(Value::as_str) else {
        return false;
    };
    let normalized = value.trim().to_ascii_lowercase();
    accepted.iter().any(|candidate| normalized == *candidate)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        channel_policy_path_for_state_dir, collect_open_dm_risk_channels,
        evaluate_multi_channel_channel_policy, load_multi_channel_policy_file,
        load_multi_channel_policy_for_state_dir, MultiChannelAllowFrom, MultiChannelChannelPolicy,
        MultiChannelConversationKind, MultiChannelDmPolicy, MultiChannelPolicyDecision,
        MultiChannelPolicyFile,
    };
    use crate::multi_channel_contract::{
        MultiChannelEventKind, MultiChannelInboundEvent, MultiChannelTransport,
    };

    fn sample_event(
        transport: MultiChannelTransport,
        conversation_id: &str,
        text: &str,
    ) -> MultiChannelInboundEvent {
        MultiChannelInboundEvent {
            schema_version: 1,
            transport,
            event_kind: MultiChannelEventKind::Message,
            event_id: "evt-1".to_string(),
            conversation_id: conversation_id.to_string(),
            thread_id: String::new(),
            actor_id: "user-1".to_string(),
            actor_display: String::new(),
            timestamp_ms: 1,
            text: text.to_string(),
            attachments: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn unit_load_policy_rejects_unsupported_schema() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("channel-policy.json");
        std::fs::write(
            &path,
            r#"{
  "schema_version": 99,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  }
}
"#,
        )
        .expect("write policy");
        let error = load_multi_channel_policy_file(&path).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported multi-channel policy schema_version"));
    }

    #[test]
    fn functional_evaluate_policy_applies_group_mention_gate() {
        let policy = MultiChannelPolicyFile {
            channels: BTreeMap::from([(
                "discord:ops-room".to_string(),
                MultiChannelChannelPolicy {
                    require_mention: true,
                    ..MultiChannelChannelPolicy::default()
                },
            )]),
            ..MultiChannelPolicyFile::default()
        };
        let mut event = sample_event(MultiChannelTransport::Discord, "ops-room", "hello");
        event
            .metadata
            .insert("guild_id".to_string(), Value::String("g1".to_string()));
        let denied = evaluate_multi_channel_channel_policy(&policy, &event);
        assert_eq!(
            denied.conversation_kind,
            MultiChannelConversationKind::Group
        );
        assert_eq!(
            denied.decision,
            MultiChannelPolicyDecision::Deny {
                reason_code: "deny_channel_policy_mention_required".to_string(),
            }
        );

        event.text = "@tau hello".to_string();
        let allowed = evaluate_multi_channel_channel_policy(&policy, &event);
        assert!(matches!(
            allowed.decision,
            MultiChannelPolicyDecision::Allow { .. }
        ));
    }

    #[test]
    fn integration_load_policy_for_state_dir_uses_tau_security_root_for_multi_channel() {
        let temp = tempdir().expect("tempdir");
        let state_dir = temp.path().join(".tau/multi-channel");
        let policy_path = channel_policy_path_for_state_dir(&state_dir);
        std::fs::create_dir_all(policy_path.parent().expect("policy parent"))
            .expect("create policy dir");
        std::fs::write(
            &policy_path,
            r#"{
  "schema_version": 1,
  "strictMode": true,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  }
}
"#,
        )
        .expect("write policy");
        let loaded = load_multi_channel_policy_for_state_dir(&state_dir).expect("load policy");
        assert!(loaded.strict_mode);
    }

    #[test]
    fn regression_permissive_allow_from_any_never_bypasses_explicit_dm_deny() {
        let policy = MultiChannelPolicyFile {
            default_policy: MultiChannelChannelPolicy {
                dm_policy: MultiChannelDmPolicy::Deny,
                allow_from: MultiChannelAllowFrom::Any,
                ..MultiChannelChannelPolicy::default()
            },
            ..MultiChannelPolicyFile::default()
        };
        let mut event = sample_event(MultiChannelTransport::Whatsapp, "room-1", "hello");
        event.metadata.insert(
            "conversation_mode".to_string(),
            Value::String("dm".to_string()),
        );
        let evaluation = evaluate_multi_channel_channel_policy(&policy, &event);
        assert_eq!(
            evaluation.decision,
            MultiChannelPolicyDecision::Deny {
                reason_code: "deny_channel_policy_dm".to_string(),
            }
        );
    }

    #[test]
    fn regression_collect_open_dm_risk_channels_reports_default_and_overrides() {
        let policy = MultiChannelPolicyFile {
            default_policy: MultiChannelChannelPolicy {
                allow_from: MultiChannelAllowFrom::Any,
                ..MultiChannelChannelPolicy::default()
            },
            channels: BTreeMap::from([(
                "telegram:chat-100".to_string(),
                MultiChannelChannelPolicy {
                    allow_from: MultiChannelAllowFrom::Any,
                    ..MultiChannelChannelPolicy::default()
                },
            )]),
            ..MultiChannelPolicyFile::default()
        };
        let risky = collect_open_dm_risk_channels(&policy);
        assert_eq!(
            risky,
            vec!["default".to_string(), "telegram:chat-100".to_string()]
        );
    }
}
