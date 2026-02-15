//! Routing and session-key selection for multi-channel events.
//!
//! Routing resolves each inbound envelope to a deterministic session key and
//! target execution route (including multi-agent selection when configured).
//! Invariants here keep dedupe/session continuity stable across retries.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::multi_channel_contract::{MultiChannelEventKind, MultiChannelInboundEvent};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_core::current_unix_timestamp_ms;
use tau_orchestrator::multi_agent_router::{
    select_multi_agent_route_with_trust, MultiAgentRoutePhase, MultiAgentRouteSelection,
    MultiAgentRouteTable, MultiAgentRouteTrustInput,
};

pub const MULTI_CHANNEL_ROUTE_BINDINGS_FILE_NAME: &str = "multi-channel-route-bindings.json";
const MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION: u32 = 1;
const WILDCARD_SELECTOR: &str = "*";
const TRUST_SCORE_KEY: &str = "trust_score";
const TRUST_SCORES_KEY: &str = "trust_scores";
const TRUST_UPDATED_UNIX_MS_KEY: &str = "trust_updated_unix_ms";

fn multi_channel_route_bindings_schema_version() -> u32 {
    MULTI_CHANNEL_ROUTE_BINDINGS_SCHEMA_VERSION
}

fn default_route_binding_selector() -> String {
    WILDCARD_SELECTOR.to_string()
}

fn default_trust_score_source() -> String {
    TRUST_SCORE_KEY.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelRouteBindingFile` used across Tau components.
pub struct MultiChannelRouteBindingFile {
    #[serde(default = "multi_channel_route_bindings_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub bindings: Vec<MultiChannelRouteBinding>,
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
/// Public struct `MultiChannelRouteBinding` used across Tau components.
pub struct MultiChannelRouteBinding {
    pub binding_id: String,
    #[serde(default = "default_route_binding_selector")]
    pub transport: String,
    #[serde(default = "default_route_binding_selector")]
    pub account_id: String,
    #[serde(default = "default_route_binding_selector")]
    pub conversation_id: String,
    #[serde(default = "default_route_binding_selector")]
    pub actor_id: String,
    #[serde(default)]
    pub phase: Option<MultiAgentRoutePhase>,
    #[serde(default)]
    pub category_hint: String,
    #[serde(default)]
    pub session_key_template: String,
    #[serde(default = "default_trust_score_source")]
    pub trust_score_source: String,
    #[serde(default)]
    pub trust_score_threshold: Option<u8>,
    #[serde(default)]
    pub trust_stale_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelRouteDecision` used across Tau components.
pub struct MultiChannelRouteDecision {
    pub binding_id: String,
    pub matched: bool,
    pub match_specificity: usize,
    pub phase: MultiAgentRoutePhase,
    pub account_id: String,
    pub requested_category: Option<String>,
    pub selected_role: String,
    pub fallback_roles: Vec<String>,
    pub attempt_roles: Vec<String>,
    pub selected_category: Option<String>,
    pub session_key: String,
    pub trust_status: String,
    pub trust_score: Option<u8>,
    pub trust_threshold: Option<u8>,
    pub trust_stale: bool,
    pub trust_score_source: Option<String>,
    pub trust_input_source: Option<String>,
}

pub fn load_multi_channel_route_bindings_for_state_dir(
    state_dir: &Path,
) -> Result<MultiChannelRouteBindingFile> {
    load_multi_channel_route_bindings(
        &state_dir
            .join("security")
            .join(MULTI_CHANNEL_ROUTE_BINDINGS_FILE_NAME),
    )
}

pub fn load_multi_channel_route_bindings(path: &Path) -> Result<MultiChannelRouteBindingFile> {
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

pub fn parse_multi_channel_route_bindings(raw: &str) -> Result<MultiChannelRouteBindingFile> {
    let mut parsed = serde_json::from_str::<MultiChannelRouteBindingFile>(raw)
        .context("failed to parse multi-channel route bindings")?;
    normalize_multi_channel_route_bindings(&mut parsed)?;
    Ok(parsed)
}

pub fn resolve_multi_channel_event_route(
    bindings: &MultiChannelRouteBindingFile,
    route_table: &MultiAgentRouteTable,
    event: &MultiChannelInboundEvent,
) -> MultiChannelRouteDecision {
    let account_id = resolve_multi_channel_account_id(event);
    let default_phase = default_phase_for_event(event.event_kind);
    let matched_binding = select_best_binding(bindings, event, &account_id);
    let matched = matched_binding.is_some();
    let (
        binding_id,
        requested_category,
        phase,
        specificity,
        session_key_template,
        trust_score_source_key,
        trust_score_threshold,
        trust_stale_after_seconds,
    ) = if let Some((binding, specificity)) = matched_binding {
        (
            binding.binding_id.clone(),
            normalize_optional_text(Some(binding.category_hint.as_str())),
            binding.phase.unwrap_or(default_phase),
            specificity,
            normalize_optional_text(Some(binding.session_key_template.as_str()))
                .unwrap_or_default(),
            normalize_optional_text(Some(binding.trust_score_source.as_str())),
            binding.trust_score_threshold,
            binding.trust_stale_after_seconds,
        )
    } else {
        (
            "default".to_string(),
            None,
            default_phase,
            0,
            String::new(),
            None,
            None,
            None,
        )
    };

    let event_text_category = normalize_optional_text(Some(event.text.as_str()));
    let category_lookup = if matches!(phase, MultiAgentRoutePhase::DelegatedStep) {
        requested_category
            .as_deref()
            .or(event_text_category.as_deref())
    } else {
        None
    };
    let (trust_input, trust_input_source) = build_route_trust_input(
        event,
        trust_score_source_key.as_deref(),
        trust_score_threshold,
        trust_stale_after_seconds,
    );
    let selection = select_multi_agent_route_with_trust(
        route_table,
        phase,
        category_lookup,
        trust_input.as_ref(),
    );
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
        trust_status: selection.trust_status,
        trust_score: selection.trust_score,
        trust_threshold: selection.trust_threshold,
        trust_stale: selection.trust_stale,
        trust_score_source: selection.trust_score_source,
        trust_input_source,
    }
}

pub fn resolve_multi_channel_account_id(event: &MultiChannelInboundEvent) -> String {
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

pub fn route_decision_trace_payload(
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
        "trust_status": decision.trust_status,
        "trust_score": decision.trust_score,
        "trust_threshold": decision.trust_threshold,
        "trust_stale": decision.trust_stale,
        "trust_score_source": decision.trust_score_source,
        "trust_input_source": decision.trust_input_source,
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
        binding.trust_score_source =
            normalize_optional_text(Some(binding.trust_score_source.as_str()))
                .unwrap_or_else(default_trust_score_source);
        if let Some(threshold) = binding.trust_score_threshold {
            if threshold > 100 {
                bail!(
                    "binding '{}' trust_score_threshold {} exceeds 100",
                    binding.binding_id,
                    threshold
                );
            }
        }
        if let Some(stale_after) = binding.trust_stale_after_seconds {
            if stale_after == 0 {
                bail!(
                    "binding '{}' trust_stale_after_seconds must be greater than 0",
                    binding.binding_id
                );
            }
        }
    }
    Ok(())
}

fn build_route_trust_input(
    event: &MultiChannelInboundEvent,
    preferred_source_key: Option<&str>,
    minimum_score: Option<u8>,
    stale_after_seconds: Option<u64>,
) -> (Option<MultiAgentRouteTrustInput>, Option<String>) {
    let mut role_scores = BTreeMap::new();
    let mut global_score = None;
    let mut source = None;

    if let Some(source_key) = preferred_source_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(value) = event.metadata.get(source_key) {
            source = Some(source_key.to_string());
            if let Some(score) = parse_trust_score_u8(value) {
                global_score = Some(score);
            } else if let Some(parsed_map) = parse_trust_score_map(value) {
                role_scores = parsed_map;
            }
        }
    }

    if role_scores.is_empty() {
        if let Some(value) = event.metadata.get(TRUST_SCORES_KEY) {
            if let Some(parsed_map) = parse_trust_score_map(value) {
                if !parsed_map.is_empty() {
                    source.get_or_insert_with(|| TRUST_SCORES_KEY.to_string());
                    role_scores = parsed_map;
                }
            }
        }
    }
    if global_score.is_none() {
        if let Some(value) = event.metadata.get(TRUST_SCORE_KEY) {
            if let Some(score) = parse_trust_score_u8(value) {
                source.get_or_insert_with(|| TRUST_SCORE_KEY.to_string());
                global_score = Some(score);
            }
        }
    }

    let updated_unix_ms = event
        .metadata
        .get(TRUST_UPDATED_UNIX_MS_KEY)
        .and_then(Value::as_u64);

    if role_scores.is_empty()
        && global_score.is_none()
        && minimum_score.is_none()
        && stale_after_seconds.is_none()
        && updated_unix_ms.is_none()
    {
        return (None, source);
    }

    (
        Some(MultiAgentRouteTrustInput {
            global_score,
            role_scores,
            minimum_score,
            updated_unix_ms,
            now_unix_ms: current_unix_timestamp_ms(),
            stale_after_seconds,
        }),
        source,
    )
}

fn parse_trust_score_u8(value: &Value) -> Option<u8> {
    let parsed = value.as_u64()?;
    if parsed > 100 {
        return None;
    }
    u8::try_from(parsed).ok()
}

fn parse_trust_score_map(value: &Value) -> Option<BTreeMap<String, u8>> {
    let object = value.as_object()?;
    let mut scores = BTreeMap::new();
    for (role, score) in object {
        let role = role.trim();
        if role.is_empty() {
            continue;
        }
        if let Some(parsed_score) = parse_trust_score_u8(score) {
            scores.insert(role.to_string(), parsed_score);
        }
    }
    Some(scores)
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
    use tau_orchestrator::multi_agent_router::MultiAgentRoleProfile;

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

    fn trust_weighted_route_table(
        primary_weight: Option<u16>,
        fallback_weight: Option<u16>,
    ) -> MultiAgentRouteTable {
        let mut table = MultiAgentRouteTable::default();
        table.roles = BTreeMap::from([
            (
                "primary".to_string(),
                MultiAgentRoleProfile {
                    trust_weight: primary_weight,
                    ..MultiAgentRoleProfile::default()
                },
            ),
            (
                "fallback".to_string(),
                MultiAgentRoleProfile {
                    trust_weight: fallback_weight,
                    ..MultiAgentRoleProfile::default()
                },
            ),
        ]);
        table.planner.role = "primary".to_string();
        table.planner.fallback_roles = vec!["fallback".to_string()];
        table.delegated.role = "primary".to_string();
        table.delegated.fallback_roles = vec!["fallback".to_string()];
        table.review.role = "primary".to_string();
        table.review.fallback_roles = vec!["fallback".to_string()];
        table
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
    fn unit_build_route_trust_input_prefers_binding_source_key() {
        let mut event = sample_event();
        event.metadata.insert(
            "provider_scores".to_string(),
            serde_json::json!({
                "primary": 92,
                "fallback": 31,
                "ignored": 250
            }),
        );
        event.metadata.insert(
            "trust_scores".to_string(),
            serde_json::json!({
                "primary": 5
            }),
        );
        let (trust_input, source) =
            build_route_trust_input(&event, Some("provider_scores"), Some(40), Some(300));
        let trust_input = trust_input.expect("trust input");
        assert_eq!(source.as_deref(), Some("provider_scores"));
        assert_eq!(trust_input.minimum_score, Some(40));
        assert_eq!(trust_input.stale_after_seconds, Some(300));
        assert_eq!(trust_input.role_scores.get("primary"), Some(&92));
        assert_eq!(trust_input.role_scores.get("fallback"), Some(&31));
        assert!(!trust_input.role_scores.contains_key("ignored"));
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
        assert_eq!(payload["trust_status"], "disabled");
    }

    #[test]
    fn functional_route_binding_applies_trust_weighted_selection_from_role_scores() {
        let bindings = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "ops", "transport": "discord", "account_id": "discord-main", "conversation_id": "*", "actor_id": "*", "phase": "planner", "trust_score_source": "trust_scores", "trust_score_threshold": 50 }
  ]
}"#,
        )
        .expect("parse bindings");
        let mut event = sample_event();
        event.metadata.insert(
            "trust_scores".to_string(),
            serde_json::json!({
                "primary": 90,
                "fallback": 60
            }),
        );
        let route_table = trust_weighted_route_table(Some(100), Some(180));
        let decision = resolve_multi_channel_event_route(&bindings, &route_table, &event);
        assert_eq!(decision.binding_id, "ops");
        assert_eq!(decision.selected_role, "fallback");
        assert_eq!(decision.attempt_roles, vec!["fallback", "primary"]);
        assert_eq!(decision.trust_status, "trust_weighted");
        assert_eq!(decision.trust_score, Some(60));
        assert_eq!(decision.trust_threshold, Some(50));
        assert_eq!(decision.trust_score_source.as_deref(), Some("role_scores"));
        assert_eq!(decision.trust_input_source.as_deref(), Some("trust_scores"));
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
    fn integration_route_binding_supports_custom_trust_provider_score_key() {
        let bindings = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "provider", "transport": "discord", "account_id": "discord-main", "conversation_id": "*", "actor_id": "*", "phase": "planner", "trust_score_source": "provider_trust", "trust_score_threshold": 70 }
  ]
}"#,
        )
        .expect("parse bindings");
        let mut event = sample_event();
        event
            .metadata
            .insert("provider_trust".to_string(), Value::from(88_u64));
        let decision =
            resolve_multi_channel_event_route(&bindings, &MultiAgentRouteTable::default(), &event);
        assert_eq!(decision.binding_id, "provider");
        assert_eq!(decision.selected_role, "default");
        assert_eq!(decision.trust_status, "trust_weighted");
        assert_eq!(decision.trust_score, Some(88));
        assert_eq!(decision.trust_threshold, Some(70));
        assert_eq!(decision.trust_score_source.as_deref(), Some("global_score"));
        assert_eq!(
            decision.trust_input_source.as_deref(),
            Some("provider_trust")
        );
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
    fn regression_route_binding_low_trust_keeps_original_fallback_order() {
        let bindings = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "ops", "transport": "discord", "account_id": "discord-main", "conversation_id": "*", "actor_id": "*", "phase": "planner", "trust_score_threshold": 95 }
  ]
}"#,
        )
        .expect("parse bindings");
        let mut event = sample_event();
        event
            .metadata
            .insert("trust_score".to_string(), Value::from(40_u64));
        let route_table = trust_weighted_route_table(Some(100), Some(150));
        let decision = resolve_multi_channel_event_route(&bindings, &route_table, &event);
        assert_eq!(decision.selected_role, "primary");
        assert_eq!(decision.attempt_roles, vec!["primary", "fallback"]);
        assert_eq!(decision.trust_status, "fallback_low_trust");
        assert_eq!(decision.trust_score, Some(40));
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

    #[test]
    fn regression_parse_route_bindings_rejects_invalid_trust_threshold() {
        let error = parse_multi_channel_route_bindings(
            r#"{
  "schema_version": 1,
  "bindings": [
    { "binding_id": "invalid", "transport": "*", "account_id": "*", "conversation_id": "*", "actor_id": "*", "trust_score_threshold": 120 }
  ]
}"#,
        )
        .expect_err("threshold above 100 should fail");
        assert!(error
            .to_string()
            .contains("trust_score_threshold 120 exceeds 100"));
    }
}
