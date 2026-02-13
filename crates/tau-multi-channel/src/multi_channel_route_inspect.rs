use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::multi_channel_contract::{
    event_contract_key, validate_multi_channel_inbound_event, MultiChannelInboundEvent,
};
use crate::multi_channel_live_ingress::parse_multi_channel_live_inbound_envelope;
use crate::multi_channel_routing::{
    load_multi_channel_route_bindings_for_state_dir, resolve_multi_channel_event_route,
};
use tau_orchestrator::multi_agent_router::{load_multi_agent_route_table, MultiAgentRouteTable};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelRouteInspectReport` used across Tau components.
pub struct MultiChannelRouteInspectReport {
    pub event_key: String,
    pub transport: String,
    pub conversation_id: String,
    pub actor_id: String,
    pub binding_id: String,
    pub binding_matched: bool,
    pub match_specificity: usize,
    pub phase: String,
    pub account_id: String,
    pub requested_category: Option<String>,
    pub selected_category: Option<String>,
    pub selected_role: String,
    pub fallback_roles: Vec<String>,
    pub attempt_roles: Vec<String>,
    pub session_key: String,
}

/// Public struct `MultiChannelRouteInspectConfig` used across Tau components.
pub struct MultiChannelRouteInspectConfig {
    pub inspect_file: PathBuf,
    pub state_dir: PathBuf,
    pub orchestrator_route_table_path: Option<PathBuf>,
}

pub fn build_multi_channel_route_inspect_report(
    config: &MultiChannelRouteInspectConfig,
) -> Result<MultiChannelRouteInspectReport> {
    let event = load_multi_channel_route_inspect_event(&config.inspect_file)?;
    let route_table = if let Some(path) = config.orchestrator_route_table_path.as_deref() {
        load_multi_agent_route_table(path)?
    } else {
        MultiAgentRouteTable::default()
    };
    let route_bindings = load_multi_channel_route_bindings_for_state_dir(&config.state_dir)?;
    let decision = resolve_multi_channel_event_route(&route_bindings, &route_table, &event);
    Ok(MultiChannelRouteInspectReport {
        event_key: event_contract_key(&event),
        transport: event.transport.as_str().to_string(),
        conversation_id: event.conversation_id.trim().to_string(),
        actor_id: event.actor_id.trim().to_string(),
        binding_id: decision.binding_id,
        binding_matched: decision.matched,
        match_specificity: decision.match_specificity,
        phase: decision.phase.as_str().to_string(),
        account_id: decision.account_id,
        requested_category: decision.requested_category,
        selected_category: decision.selected_category,
        selected_role: decision.selected_role,
        fallback_roles: decision.fallback_roles,
        attempt_roles: decision.attempt_roles,
        session_key: decision.session_key,
    })
}

pub fn render_multi_channel_route_inspect_report(
    report: &MultiChannelRouteInspectReport,
) -> String {
    let selected_category = report
        .selected_category
        .as_deref()
        .unwrap_or("none")
        .to_string();
    let requested_category = report
        .requested_category
        .as_deref()
        .unwrap_or("none")
        .to_string();
    format!(
        "multi-channel route inspect: event_key={} transport={} conversation_id={} actor_id={} binding_id={} binding_matched={} match_specificity={} phase={} account_id={} requested_category={} selected_category={} selected_role={} fallback_roles={} attempt_roles={} session_key={}",
        report.event_key,
        report.transport,
        report.conversation_id,
        report.actor_id,
        report.binding_id,
        report.binding_matched,
        report.match_specificity,
        report.phase,
        if report.account_id.is_empty() { "none" } else { report.account_id.as_str() },
        requested_category,
        selected_category,
        report.selected_role,
        if report.fallback_roles.is_empty() {
            "none".to_string()
        } else {
            report.fallback_roles.join(",")
        },
        if report.attempt_roles.is_empty() {
            "none".to_string()
        } else {
            report.attempt_roles.join(",")
        },
        report.session_key
    )
}

fn load_multi_channel_route_inspect_event(path: &Path) -> Result<MultiChannelInboundEvent> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!(
            "multi-channel route inspect file '{}' is empty",
            path.display()
        );
    }

    if let Ok(event) = serde_json::from_str::<MultiChannelInboundEvent>(trimmed) {
        validate_multi_channel_inbound_event(&event)
            .with_context(|| format!("invalid normalized event in {}", path.display()))?;
        return Ok(event);
    }

    if let Ok(event) = parse_multi_channel_live_inbound_envelope(trimmed) {
        return Ok(event);
    }

    let first_line = raw
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or_default();
    if let Ok(event) = serde_json::from_str::<MultiChannelInboundEvent>(first_line) {
        validate_multi_channel_inbound_event(&event)
            .with_context(|| format!("invalid normalized event in {}", path.display()))?;
        return Ok(event);
    }
    match parse_multi_channel_live_inbound_envelope(first_line) {
        Ok(event) => Ok(event),
        Err(error) => {
            bail!(
                "failed to parse '{}' as normalized event or live ingress envelope: {}",
                path.display(),
                error
            )
        }
    }
}
