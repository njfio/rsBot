use super::*;
use std::collections::HashSet;

use crate::multi_agent_router::{
    select_multi_agent_route, MultiAgentRoutePhase, MultiAgentRouteSelection, MultiAgentRouteTable,
};
use crate::multi_channel_contract::{MultiChannelEventKind, MultiChannelInboundEvent};
use serde_json::{json, Value};

pub(crate) const MULTI_CHANNEL_ROUTE_BINDINGS_FILE_NAME: &str = "multi-channel-route-bindings.json";
const MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION: u32 = 1;
const WILDCARD_SELECTOR: &str = "*";

fn multi_channel_route_bindings_schema_version() -> u32 {
    MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION
}

fn default_route_binding_selector() -> String {
    WILDCARD_SELECTOR.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MultiChannelRouteBindingFile {
    #[serde(default = "multi_channel_route_bindings_schema_version")]
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub(crate) bindings: Vec<MultiChannelRouteBinding>,
}

impl Default for MultiChannelRouteBindingFile {
    fn default() -> Self {
        Self {
            schema_version: MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION,
            bindings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MultiChannelRouteBinding {
    pub(crate) binding_id: String,
    #[serde(default = "default_route_binding_selector")]
    pub(crate) transport: String,
    #[serde(default = "default_route_binding_selector")]
    pub(crate) account_id: String,
    #[serde(default = "default_route_binding_selector")]
    pub(crate) conversation_id: String,
    #[serde(default = "default_route_binding_selector")]
    pub(crate) actor_id: String,
    #[serde(default)]
    pub(crate) phase: Option<MultiAgentRoutePhase>,
    #[serde(default)]
    pub(crate) category_hint: String,
    #[serde(default)]
    pub(crate) session_key_template: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct MultiChannelRouteDecision {
    pub(crate) binding_id: String,
    pub(crate) matched: bool,
    pub(crate) match_specificity: usize,
    pub(crate) phase: MultiAgentRoutePhase,
    pub(crate) account_id: String,
    pub(crate) requested_category: Option<String>,
    pub(crate) selected_role: String,
    pub(crate) fallback_roles: Vec<String>,
    pub(crate) attempt_roles: Vec<String>,
    pub(crate) selected_category: Option<String>,
    pub(crate) session_key: String,
}

pub(crate) fn load_multi_channel_route_bindings_for_state_dir(
    state_dir: &Path,
) -> Result<MultiChannelRouteBindingFile> {
    load_multi_channel_route_bindings(
        &state_dir
            .join("security")
            .join(MULTI_CHANNEL_ROUTE_BINDINGS_FILE_NAME),
    )
}

pub(crate) fn load_multi_channel_route_bindings(
    path: &Path,
) -> Result<MultiChannelRouteBindingFile> {
    if !path.exists() {
        return Ok(MultiChannelRouteBindingFile::default());
    }
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read multi-channel route bindings {}",
            path.display()
        )
    })?;
    parse_multi_channel_route_bindings(&raw)
        .with_context(|| format!("invalid multi-channel route bindings {}", path.display()))
}

pub(crate) fn parse_multi_channel_route_bindings(
    raw: &str,
) -> Result<MultiChannelRouteBindingFile> {
    let mut parsed = serde_json::from_str::<MultiChannelRouteBindingFile>(raw)
        .context("failed to parse multi-channel route bindings")?;
    normalize_multi_channel_route_bindings(&mut parsed)?;
    Ok(parsed)
}

pub(crate) fn resolve_multi_channel_event_route(
    bindings: &MultiChannelRouteBindingFile,
    route_table: &MultiAgentRouteTable,
    event: &MultiChannelInboundEvent,
) -> MultiChannelRouteDecision {
    let account_id = resolve_multi_channel_account_id(event);
    let default_phase = default_phase_for_event(event.event_kind);
    let matched_binding = select_best_binding(bindings, event, &account_id);
    let matched = matched_binding.is_some();
    let (binding_id, requested_category, phase, specificity, session_key_template) =
        if let Some((binding, specificity)) = matched_binding {
            (
                binding.binding_id.clone(),
                normalize_optional_text(Some(binding.category_hint.as_str())),
                binding.phase.unwrap_or(default_phase),
                specificity,
                normalize_optional_text(Some(binding.session_key_template.as_str()))
                    .unwrap_or_default(),
            )
        } else {
            ("default".to_string(), None, default_phase, 0, String::new())
        };

    let event_text_category = normalize_optional_text(Some(event.text.as_str()));
    let category_lookup = if matches!(phase, MultiAgentRoutePhase::DelegatedStep) {
        requested_category
            .as_deref()
            .or(event_text_category.as_deref())
    } else {
        None
    };
    let selection = select_multi_agent_route(route_table, phase, category_lookup);
    let selected_category = selection
        .category
        .clone()
        .or_else(|| requested_category.clone());
    let session_key = if session_key_template.is_empty() {
        normalize_default_session_key(event)
    } else {
        render_session_key_template(
            &session_key_template,
            event,
            &account_id,
            &selection,
            selected_category.as_deref(),
        )
    };

    MultiChannelRouteDecision {
        binding_id,
        matched,
        match_specificity: specificity,
        phase,
        account_id,
        requested_category,
        selected_role: selection.primary_role,
        fallback_roles: selection.fallback_roles,
        attempt_roles: selection.attempt_roles,
        selected_category,
        session_key,
    }
}

pub(crate) fn resolve_multi_channel_account_id(event: &MultiChannelInboundEvent) -> String {
    for key in [
        "account_id",
        "telegram_bot_id",
        "discord_bot_id",
        "discord_application_id",
        "whatsapp_business_account_id",
        "whatsapp_phone_number_id",
    ] {
        if let Some(value) = event.metadata.get(key).and_then(Value::as_str) {
            if let Some(normalized) = normalize_optional_text(Some(value)) {
                return normalized;
            }
        }
    }
    String::new()
}

pub(crate) fn route_decision_trace_payload(
    event: &MultiChannelInboundEvent,
    event_key: &str,
    decision: &MultiChannelRouteDecision,
) -> Value {
    json!({
        "record_type": "multi_channel_route_trace_v1",
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "event_key": event_key,
        "transport": event.transport.as_str(),
        "conversation_id": event.conversation_id.trim(),
        "actor_id": event.actor_id.trim(),
        "binding_id": decision.binding_id,
        "binding_matched": decision.matched,
        "match_specificity": decision.match_specificity,
        "phase": decision.phase.as_str(),
        "account_id": decision.account_id,
        "requested_category": decision.requested_category,
        "selected_category": decision.selected_category,
        "selected_role": decision.selected_role,
        "fallback_roles": decision.fallback_roles,
        "attempt_roles": decision.attempt_roles,
        "session_key": decision.session_key,
    })
}

fn normalize_multi_channel_route_bindings(
    bindings: &mut MultiChannelRouteBindingFile,
) -> Result<()> {
    if bindings.schema_version != MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION {
        bail!(
            "unsupported multi-channel route bindings schema_version {} (expected {})",
            bindings.schema_version,
            MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION
        );
    }

    let mut seen_ids = HashSet::new();
    for binding in &mut bindings.bindings {
        let binding_id = normalize_optional_text(Some(binding.binding_id.as_str()))
            .ok_or_else(|| anyhow!("binding_id cannot be empty"))?;
        if !seen_ids.insert(binding_id.clone()) {
            bail!("duplicate binding_id '{}'", binding_id);
        }
        binding.binding_id = binding_id;
        binding.transport = normalize_selector(binding.transport.as_str(), true)?;
        binding.account_id = normalize_selector(binding.account_id.as_str(), false)?;
        binding.conversation_id = normalize_selector(binding.conversation_id.as_str(), false)?;
        binding.actor_id = normalize_selector(binding.actor_id.as_str(), false)?;
        binding.category_hint =
            normalize_optional_text(Some(binding.category_hint.as_str())).unwrap_or_default();
        binding.session_key_template =
            normalize_optional_text(Some(binding.session_key_template.as_str()))
                .unwrap_or_default();
    }
    Ok(())
}

fn normalize_selector(raw: &str, lowercase: bool) -> Result<String> {
    let normalized =
        normalize_optional_text(Some(raw)).unwrap_or_else(|| WILDCARD_SELECTOR.to_string());
    if normalized == WILDCARD_SELECTOR {
        return Ok(normalized);
    }
    if normalized.contains('*') {
        bail!(
            "selector '{}' is invalid; only '*' wildcard is supported",
            normalized
        );
    }
    if lowercase {
        Ok(normalized.to_ascii_lowercase())
    } else {
        Ok(normalized)
    }
}

fn select_best_binding<'a>(
    bindings: &'a MultiChannelRouteBindingFile,
    event: &MultiChannelInboundEvent,
    account_id: &str,
) -> Option<(&'a MultiChannelRouteBinding, usize)> {
    let mut best: Option<(&MultiChannelRouteBinding, usize)> = None;
    for binding in &bindings.bindings {
        let Some(score) = binding_match_score(binding, event, account_id) else {
            continue;
        };
        match best {
            Some((_, best_score)) if best_score >= score => {}
            _ => {
                best = Some((binding, score));
            }
        }
    }
    best
}

fn binding_match_score(
    binding: &MultiChannelRouteBinding,
    event: &MultiChannelInboundEvent,
    account_id: &str,
) -> Option<usize> {
    let mut score = 0usize;
    score = score.saturating_add(selector_score(
        binding.transport.as_str(),
        event.transport.as_str(),
    )?);
    score = score.saturating_add(selector_score(binding.account_id.as_str(), account_id)?);
    score = score.saturating_add(selector_score(
        binding.conversation_id.as_str(),
        event.conversation_id.trim(),
    )?);
    score = score.saturating_add(selector_score(
        binding.actor_id.as_str(),
        event.actor_id.trim(),
    )?);
    Some(score)
}

fn selector_score(selector: &str, value: &str) -> Option<usize> {
    if selector == WILDCARD_SELECTOR {
        return Some(0);
    }
    if selector == value {
        return Some(1);
    }
    None
}

fn default_phase_for_event(event_kind: MultiChannelEventKind) -> MultiAgentRoutePhase {
    match event_kind {
        MultiChannelEventKind::Command => MultiAgentRoutePhase::Planner,
        MultiChannelEventKind::System => MultiAgentRoutePhase::Review,
        MultiChannelEventKind::Message | MultiChannelEventKind::Edit => {
            MultiAgentRoutePhase::DelegatedStep
        }
    }
}

fn render_session_key_template(
    template: &str,
    event: &MultiChannelInboundEvent,
    account_id: &str,
    selection: &MultiAgentRouteSelection,
    category: Option<&str>,
) -> String {
    let mut rendered = template.to_string();
    let replacements = [
        ("transport", event.transport.as_str()),
        ("account_id", account_id),
        ("conversation_id", event.conversation_id.trim()),
        ("actor_id", event.actor_id.trim()),
        ("role", selection.primary_role.as_str()),
        ("phase", selection.phase.as_str()),
        ("category", category.unwrap_or("")),
    ];
    for (key, value) in replacements {
        rendered = rendered.replace(
            &format!("{{{key}}}"),
            sanitize_session_segment(value).as_str(),
        );
    }
    let normalized = sanitize_session_segment(rendered.as_str());
    if normalized.is_empty() {
        normalize_default_session_key(event)
    } else {
        normalized
    }
}

fn normalize_default_session_key(event: &MultiChannelInboundEvent) -> String {
    let normalized = sanitize_session_segment(event.conversation_id.trim());
    if normalized.is_empty() {
        "default".to_string()
    } else {
        normalized
    }
}

fn sanitize_session_segment(raw: &str) -> String {
    let mut normalized = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ':' || ch == '.' {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }
    normalized.trim_matches('_').to_string()
}

fn normalize_optional_text(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::multi_channel_contract::{MultiChannelEventKind, MultiChannelTransport};

    fn sample_event() -> MultiChannelInboundEvent {
        MultiChannelInboundEvent {
            schema_version: 1,
            transport: MultiChannelTransport::Discord,
            event_kind: MultiChannelEventKind::Message,
            event_id: "evt-1".to_string(),
            conversation_id: "ops-room".to_string(),
            thread_id: String::new(),
            actor_id: "user-42".to_string(),
            actor_display: String::new(),
            timestamp_ms: 1_760_200_000_000,
            text: "please investigate incident".to_string(),
            attachments: Vec::new(),
            metadata: BTreeMap::from([(
                "account_id".to_string(),
                Value::String("discord-main".to_string()),
            )]),
        }
    }

    #[test]
    fn unit_route_binding_resolver_prefers_highest_specificity() {
        let bindings = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "wildcard", "transport": "discord", "account_id": "*", "conversation_id": "*", "actor_id": "*", "phase": "delegated_step", "session_key_template": "wildcard" },
    { "binding_id": "specific", "transport": "discord", "account_id": "discord-main", "conversation_id": "ops-room", "actor_id": "user-42", "phase": "planner", "session_key_template": "incident-{role}" }
  ]
}"#,
        )
        .expect("parse bindings");
        let decision = resolve_multi_channel_event_route(
            &bindings,
            &MultiAgentRouteTable::default(),
            &sample_event(),
        );
        assert_eq!(decision.binding_id, "specific");
        assert_eq!(decision.phase, MultiAgentRoutePhase::Planner);
        assert_eq!(decision.selected_role, "default");
        assert_eq!(decision.session_key, "incident-default");
        assert_eq!(decision.match_specificity, 4);
    }

    #[test]
    fn functional_route_binding_trace_payload_includes_routing_context() {
        let bindings = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "ops", "transport": "discord", "account_id": "discord-main", "conversation_id": "ops-room", "actor_id": "*", "phase": "delegated_step", "category_hint": "incident", "session_key_template": "{transport}:{conversation_id}:{role}" }
  ]
}"#,
        )
        .expect("parse bindings");
        let event = sample_event();
        let event_key = "discord:evt-1";
        let decision =
            resolve_multi_channel_event_route(&bindings, &MultiAgentRouteTable::default(), &event);
        let payload = route_decision_trace_payload(&event, event_key, &decision);
        assert_eq!(payload["record_type"], "multi_channel_route_trace_v1");
        assert_eq!(payload["binding_id"], "ops");
        assert_eq!(payload["session_key"], "discord:ops-room:default");
        assert_eq!(payload["event_key"], event_key);
    }

    #[test]
    fn integration_route_binding_defaults_to_conversation_session_when_no_match() {
        let bindings = MultiChannelRouteBindingFile::default();
        let event = sample_event();
        let decision =
            resolve_multi_channel_event_route(&bindings, &MultiAgentRouteTable::default(), &event);
        assert_eq!(decision.binding_id, "default");
        assert_eq!(decision.session_key, "ops-room");
        assert_eq!(decision.selected_role, "default");
    }

    #[test]
    fn regression_route_binding_precedence_prefers_first_binding_on_equal_specificity() {
        let bindings = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "first", "transport": "discord", "account_id": "*", "conversation_id": "ops-room", "actor_id": "*", "session_key_template": "first" },
    { "binding_id": "second", "transport": "discord", "account_id": "*", "conversation_id": "ops-room", "actor_id": "*", "session_key_template": "second" }
  ]
}"#,
        )
        .expect("parse bindings");
        let decision = resolve_multi_channel_event_route(
            &bindings,
            &MultiAgentRouteTable::default(),
            &sample_event(),
        );
        assert_eq!(decision.binding_id, "first");
        assert_eq!(decision.session_key, "first");
    }

    #[test]
    fn regression_parse_route_bindings_rejects_duplicate_binding_ids() {
        let error = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "dup", "transport": "*", "account_id": "*", "conversation_id": "*", "actor_id": "*" },
    { "binding_id": "dup", "transport": "discord", "account_id": "*", "conversation_id": "*", "actor_id": "*" }
  ]
}"#,
        )
        .expect_err("duplicate binding ids should fail");
        assert!(error.to_string().contains("duplicate binding_id 'dup'"));
    }
}
