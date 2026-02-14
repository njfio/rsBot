//! Multi-channel status projection helpers for gateway status surfaces.
use super::*;

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
