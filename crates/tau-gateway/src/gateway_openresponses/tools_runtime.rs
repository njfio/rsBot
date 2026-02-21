//! Gateway tools inventory/stats endpoint handlers.
//!
//! This module exposes dashboard-oriented contracts for tool inventory and
//! usage diagnostics aggregated from gateway UI telemetry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentConfig};
use tau_core::current_unix_timestamp_ms;

use super::channel_telemetry_runtime::gateway_ui_telemetry_path;
use super::{authorize_dashboard_request, GatewayOpenResponsesServerState, OpenResponsesApiError};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayToolInventoryItem {
    name: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayToolStatsItem {
    tool_name: String,
    registered: bool,
    event_count: u64,
    action_counts: BTreeMap<String, u64>,
    reason_code_counts: BTreeMap<String, u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_event_unix_ms: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct GatewayUiTelemetryRecord {
    #[serde(default)]
    timestamp_unix_ms: u64,
    #[serde(default)]
    view: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    reason_code: String,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Default)]
struct GatewayToolStatsLoad {
    total_events: u64,
    invalid_records: u64,
    stats: BTreeMap<String, GatewayToolStatsItem>,
    diagnostics: Vec<String>,
}

pub(super) async fn handle_gateway_tools_inventory(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let tools = collect_tool_inventory_items(&state);
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "generated_unix_ms": current_unix_timestamp_ms(),
            "total_tools": tools.len(),
            "tools": tools,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_tools_stats(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }

    let inventory = collect_tool_inventory_items(&state);
    let load = match load_tool_stats(&state.config.state_dir, &inventory) {
        Ok(load) => load,
        Err(error) => return error.into_response(),
    };
    let stats = load.stats.into_values().collect::<Vec<_>>();

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "generated_unix_ms": current_unix_timestamp_ms(),
            "total_tools": inventory.len(),
            "total_events": load.total_events,
            "invalid_records": load.invalid_records,
            "stats": stats,
            "diagnostics": load.diagnostics,
        })),
    )
        .into_response()
}

fn collect_tool_inventory_items(
    state: &GatewayOpenResponsesServerState,
) -> Vec<GatewayToolInventoryItem> {
    let mut agent = Agent::new(
        state.config.client.clone(),
        AgentConfig {
            model: state.config.model.clone(),
            system_prompt: state.config.system_prompt.clone(),
            max_turns: state.config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            ..AgentConfig::default()
        },
    );
    state.config.tool_registrar.register(&mut agent);
    agent
        .registered_tool_names()
        .into_iter()
        .map(|name| GatewayToolInventoryItem {
            name,
            enabled: true,
        })
        .collect()
}

fn load_tool_stats(
    state_dir: &Path,
    inventory: &[GatewayToolInventoryItem],
) -> Result<GatewayToolStatsLoad, OpenResponsesApiError> {
    let mut stats_by_tool = BTreeMap::<String, GatewayToolStatsItem>::new();
    for tool in inventory {
        stats_by_tool.insert(
            tool.name.clone(),
            GatewayToolStatsItem {
                tool_name: tool.name.clone(),
                registered: true,
                event_count: 0,
                action_counts: BTreeMap::new(),
                reason_code_counts: BTreeMap::new(),
                last_event_unix_ms: None,
            },
        );
    }

    let path = gateway_ui_telemetry_path(state_dir);
    if !path.exists() {
        return Ok(GatewayToolStatsLoad {
            total_events: 0,
            invalid_records: 0,
            stats: stats_by_tool,
            diagnostics: vec![format!("tools_telemetry_missing:{}", path.display())],
        });
    }

    let raw = std::fs::read_to_string(&path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read tools telemetry '{}': {error}",
            path.display()
        ))
    })?;

    let registered_names = inventory
        .iter()
        .map(|item| item.name.clone())
        .collect::<BTreeSet<_>>();
    let mut load = GatewayToolStatsLoad {
        stats: stats_by_tool,
        ..GatewayToolStatsLoad::default()
    };
    for (line_number, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = match serde_json::from_str::<GatewayUiTelemetryRecord>(trimmed) {
            Ok(parsed) => parsed,
            Err(_) => {
                load.invalid_records = load.invalid_records.saturating_add(1);
                load.diagnostics.push(format!(
                    "tools_telemetry_malformed_line:{}",
                    line_number.saturating_add(1)
                ));
                continue;
            }
        };
        if !parsed.view.eq_ignore_ascii_case("tools") {
            continue;
        }
        let Some(tool_name) = parsed
            .metadata
            .get("tool_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        let entry = load
            .stats
            .entry(tool_name.clone())
            .or_insert_with(|| GatewayToolStatsItem {
                tool_name: tool_name.clone(),
                registered: registered_names.contains(&tool_name),
                event_count: 0,
                action_counts: BTreeMap::new(),
                reason_code_counts: BTreeMap::new(),
                last_event_unix_ms: None,
            });
        entry.event_count = entry.event_count.saturating_add(1);
        load.total_events = load.total_events.saturating_add(1);
        increment_u64_counter(
            &mut entry.action_counts,
            normalize_non_empty(parsed.action.as_str(), "unknown"),
        );
        increment_u64_counter(
            &mut entry.reason_code_counts,
            normalize_non_empty(parsed.reason_code.as_str(), "unknown"),
        );
        entry.last_event_unix_ms = Some(
            entry
                .last_event_unix_ms
                .unwrap_or(0)
                .max(parsed.timestamp_unix_ms),
        );
    }

    Ok(load)
}

fn normalize_non_empty(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn increment_u64_counter(counters: &mut BTreeMap<String, u64>, key: String) {
    *counters.entry(key).or_insert(0) += 1;
}
