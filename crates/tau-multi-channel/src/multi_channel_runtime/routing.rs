use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use super::{
    current_unix_timestamp_ms, MultiChannelRuntimeCycleReport, MultiChannelRuntimeSummary,
    TransportHealthSnapshot,
};

pub(super) fn build_transport_health_snapshot(
    summary: &MultiChannelRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_events = summary
        .discovered_events
        .saturating_sub(summary.queued_events);
    let failure_streak = if summary.failed_events > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };
    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_events,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_events,
        last_cycle_processed: summary
            .completed_events
            .saturating_add(summary.failed_events)
            .saturating_add(summary.duplicate_skips),
        last_cycle_completed: summary.completed_events,
        last_cycle_failed: summary.failed_events,
        last_cycle_duplicates: summary.duplicate_skips,
    }
}

pub(super) fn cycle_reason_codes(summary: &MultiChannelRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    let mut operational_issue_detected = false;
    if summary.discovered_events > summary.queued_events {
        operational_issue_detected = true;
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        operational_issue_detected = true;
        codes.push("duplicate_events_skipped".to_string());
    }
    if summary.retry_attempts > 0 {
        operational_issue_detected = true;
        codes.push("retry_attempted".to_string());
    }
    if summary.transient_failures > 0 {
        operational_issue_detected = true;
        codes.push("transient_failures_observed".to_string());
    }
    if summary.failed_events > 0 {
        operational_issue_detected = true;
        codes.push("event_processing_failed".to_string());
    }
    if !operational_issue_detected {
        codes.push("healthy_cycle".to_string());
    }
    if summary.policy_checked_events > 0 {
        if summary.policy_enforced_events > 0 {
            codes.push("pairing_policy_enforced".to_string());
        } else {
            codes.push("pairing_policy_permissive".to_string());
        }
    }
    if summary.policy_denied_events > 0 {
        codes.push("pairing_policy_denied_events".to_string());
    }
    if summary.typing_events_emitted > 0 || summary.presence_events_emitted > 0 {
        codes.push("telemetry_lifecycle_emitted".to_string());
    }
    if summary.usage_summary_records > 0 {
        codes.push("telemetry_usage_summary_emitted".to_string());
    }
    codes
}

pub(super) fn append_multi_channel_cycle_report(
    path: &Path,
    summary: &MultiChannelRuntimeSummary,
    health: &TransportHealthSnapshot,
    health_reason: &str,
    reason_codes: &[String],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = MultiChannelRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        discovered_events: summary.discovered_events,
        queued_events: summary.queued_events,
        completed_events: summary.completed_events,
        duplicate_skips: summary.duplicate_skips,
        transient_failures: summary.transient_failures,
        retry_attempts: summary.retry_attempts,
        failed_events: summary.failed_events,
        policy_checked_events: summary.policy_checked_events,
        policy_enforced_events: summary.policy_enforced_events,
        policy_allowed_events: summary.policy_allowed_events,
        policy_denied_events: summary.policy_denied_events,
        typing_events_emitted: summary.typing_events_emitted,
        presence_events_emitted: summary.presence_events_emitted,
        usage_summary_records: summary.usage_summary_records,
        usage_response_chars: summary.usage_response_chars,
        usage_chunks: summary.usage_chunks,
        usage_estimated_cost_micros: summary.usage_estimated_cost_micros,
        backlog_events: summary
            .discovered_events
            .saturating_sub(summary.queued_events),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize multi-channel runtime report")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

pub(super) fn append_multi_channel_route_trace(path: &Path, payload: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let line = serde_json::to_string(payload).context("serialize multi-channel route trace")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}
