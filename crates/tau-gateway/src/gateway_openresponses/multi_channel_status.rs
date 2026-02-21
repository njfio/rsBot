//! Multi-channel status projection helpers for gateway status surfaces.
use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayMultiChannelStatusReport {
    pub(super) state_present: bool,
    pub(super) health_state: String,
    pub(super) health_reason: String,
    pub(super) rollout_gate: String,
    pub(super) processed_event_count: usize,
    pub(super) transport_counts: BTreeMap<String, usize>,
    pub(super) queue_depth: usize,
    pub(super) failure_streak: usize,
    pub(super) last_cycle_failed: usize,
    pub(super) last_cycle_completed: usize,
    pub(super) cycle_reports: usize,
    pub(super) invalid_cycle_reports: usize,
    pub(super) last_reason_codes: Vec<String>,
    pub(super) reason_code_counts: BTreeMap<String, usize>,
    pub(super) connectors: GatewayMultiChannelConnectorsStatusReport,
    pub(super) diagnostics: Vec<String>,
}

impl Default for GatewayMultiChannelStatusReport {
    fn default() -> Self {
        Self {
            state_present: false,
            health_state: "unknown".to_string(),
            health_reason: "multi-channel runtime state is unavailable".to_string(),
            rollout_gate: "hold".to_string(),
            processed_event_count: 0,
            transport_counts: BTreeMap::new(),
            queue_depth: 0,
            failure_streak: 0,
            last_cycle_failed: 0,
            last_cycle_completed: 0,
            cycle_reports: 0,
            invalid_cycle_reports: 0,
            last_reason_codes: Vec::new(),
            reason_code_counts: BTreeMap::new(),
            connectors: GatewayMultiChannelConnectorsStatusReport::default(),
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub(super) struct GatewayMultiChannelConnectorsStatusReport {
    pub(super) state_present: bool,
    pub(super) processed_event_count: usize,
    pub(super) channels: BTreeMap<String, GatewayMultiChannelConnectorChannelSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub(super) struct GatewayMultiChannelConnectorChannelSummary {
    pub(super) mode: String,
    pub(super) liveness: String,
    pub(super) breaker_state: String,
    pub(super) events_ingested: u64,
    pub(super) duplicates_skipped: u64,
    pub(super) retry_attempts: u64,
    pub(super) auth_failures: u64,
    pub(super) parse_failures: u64,
    pub(super) provider_failures: u64,
    pub(super) consecutive_failures: u64,
    pub(super) retry_budget_remaining: u64,
    pub(super) breaker_open_until_unix_ms: u64,
    pub(super) breaker_last_open_reason: String,
    pub(super) breaker_open_count: u64,
    pub(super) last_error_code: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayMultiChannelRuntimeStateFile {
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayMultiChannelCycleReportLine {
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    health_reason: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayMultiChannelConnectorsStateFile {
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    channels: BTreeMap<
        String,
        tau_multi_channel::multi_channel_live_connectors::MultiChannelLiveConnectorChannelState,
    >,
}

pub(super) fn collect_gateway_multi_channel_status_report(
    gateway_state_dir: &Path,
) -> GatewayMultiChannelStatusReport {
    let tau_root = gateway_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| gateway_state_dir.to_path_buf());
    let multi_channel_root = tau_root.join("multi-channel");
    let state_path = multi_channel_root.join("state.json");
    let events_path = multi_channel_root.join("runtime-events.jsonl");
    let connectors_path = multi_channel_root.join("live-connectors-state.json");

    let mut report = GatewayMultiChannelStatusReport::default();
    report.connectors = load_gateway_multi_channel_connectors_status_report(
        &connectors_path,
        &mut report.diagnostics,
    );

    if !state_path.exists() {
        report
            .diagnostics
            .push(format!("state_missing:{}", state_path.display()));
        return report;
    }
    report.state_present = true;

    let raw_state = match std::fs::read_to_string(&state_path) {
        Ok(raw) => raw,
        Err(error) => {
            report.diagnostics.push(format!(
                "state_read_failed:{}:{error}",
                state_path.display()
            ));
            return report;
        }
    };
    let state = match serde_json::from_str::<GatewayMultiChannelRuntimeStateFile>(&raw_state) {
        Ok(state) => state,
        Err(error) => {
            report.diagnostics.push(format!(
                "state_parse_failed:{}:{error}",
                state_path.display()
            ));
            return report;
        }
    };

    report.processed_event_count = state.processed_event_keys.len();
    for event_key in &state.processed_event_keys {
        let Some((transport, _)) = event_key.split_once(':') else {
            continue;
        };
        increment_gateway_multi_channel_counter(
            &mut report.transport_counts,
            &transport.to_ascii_lowercase(),
        );
    }

    let classification = state.health.classify();
    report.health_state = classification.state.as_str().to_string();
    report.health_reason = classification.reason;
    report.rollout_gate = if report.health_state == "healthy" {
        "pass".to_string()
    } else {
        "hold".to_string()
    };
    report.queue_depth = state.health.queue_depth;
    report.failure_streak = state.health.failure_streak;
    report.last_cycle_failed = state.health.last_cycle_failed;
    report.last_cycle_completed = state.health.last_cycle_completed;

    if !events_path.exists() {
        report
            .diagnostics
            .push(format!("events_log_missing:{}", events_path.display()));
        return report;
    }
    let raw_events = match std::fs::read_to_string(&events_path) {
        Ok(raw) => raw,
        Err(error) => {
            report.diagnostics.push(format!(
                "events_log_read_failed:{}:{error}",
                events_path.display()
            ));
            return report;
        }
    };
    for line in raw_events.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayMultiChannelCycleReportLine>(trimmed) {
            Ok(event) => {
                report.cycle_reports = report.cycle_reports.saturating_add(1);
                report.last_reason_codes = event.reason_codes.clone();
                for reason_code in &event.reason_codes {
                    increment_gateway_multi_channel_counter(
                        &mut report.reason_code_counts,
                        reason_code,
                    );
                }
                if !event.health_reason.trim().is_empty() {
                    report.health_reason = event.health_reason;
                }
            }
            Err(_) => {
                report.invalid_cycle_reports = report.invalid_cycle_reports.saturating_add(1);
            }
        }
    }

    report
}

fn load_gateway_multi_channel_connectors_status_report(
    path: &Path,
    diagnostics: &mut Vec<String>,
) -> GatewayMultiChannelConnectorsStatusReport {
    let mut report = GatewayMultiChannelConnectorsStatusReport::default();
    if !path.exists() {
        diagnostics.push(format!("connectors_state_missing:{}", path.display()));
        return report;
    }
    report.state_present = true;

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(format!(
                "connectors_state_read_failed:{}:{error}",
                path.display()
            ));
            return report;
        }
    };
    let parsed = match serde_json::from_str::<GatewayMultiChannelConnectorsStateFile>(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            diagnostics.push(format!(
                "connectors_state_parse_failed:{}:{error}",
                path.display()
            ));
            return report;
        }
    };

    report.processed_event_count = parsed.processed_event_keys.len();
    for (channel, state) in parsed.channels {
        report
            .channels
            .insert(channel, normalize_gateway_connector_channel_summary(&state));
    }
    report
}

fn normalize_gateway_connector_channel_summary(
    state: &tau_multi_channel::multi_channel_live_connectors::MultiChannelLiveConnectorChannelState,
) -> GatewayMultiChannelConnectorChannelSummary {
    GatewayMultiChannelConnectorChannelSummary {
        mode: normalize_non_empty_string(&state.mode, "unknown"),
        liveness: normalize_non_empty_string(&state.liveness, "unknown"),
        breaker_state: normalize_non_empty_string(&state.breaker_state, "unknown"),
        events_ingested: state.events_ingested,
        duplicates_skipped: state.duplicates_skipped,
        retry_attempts: state.retry_attempts,
        auth_failures: state.auth_failures,
        parse_failures: state.parse_failures,
        provider_failures: state.provider_failures,
        consecutive_failures: state.consecutive_failures,
        retry_budget_remaining: state.retry_budget_remaining,
        breaker_open_until_unix_ms: state.breaker_open_until_unix_ms,
        breaker_last_open_reason: normalize_non_empty_string(
            &state.breaker_last_open_reason,
            "none",
        ),
        breaker_open_count: state.breaker_open_count,
        last_error_code: normalize_non_empty_string(&state.last_error_code, "none"),
    }
}

fn increment_gateway_multi_channel_counter(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_insert(0) += 1;
}

fn normalize_non_empty_string(raw: &str, fallback: &str) -> String {
    if raw.trim().is_empty() {
        fallback.to_string()
    } else {
        raw.trim().to_string()
    }
}
