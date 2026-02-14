//! Incident timeline execution and reporting helpers for multi-channel runtime artifacts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::ChannelLogEntry;

const MULTI_CHANNEL_INCIDENT_REPLAY_EXPORT_SCHEMA_VERSION: u32 = 1;
const MULTI_CHANNEL_INCIDENT_DIAGNOSTIC_CAP: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `MultiChannelIncidentOutcomeCounts` used across Tau components.
pub struct MultiChannelIncidentOutcomeCounts {
    pub allowed: usize,
    pub denied: usize,
    pub retried: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelIncidentTimelineEntry` used across Tau components.
pub struct MultiChannelIncidentTimelineEntry {
    pub event_key: String,
    pub transport: String,
    pub conversation_id: String,
    pub route_session_key: String,
    pub route_binding_id: String,
    pub route_reason_code: String,
    pub policy_reason_code: String,
    pub delivery_reason_code: String,
    pub outcome: String,
    pub first_timestamp_unix_ms: u64,
    pub last_timestamp_unix_ms: u64,
    pub delivery_failed_attempts: usize,
    pub retryable_failures: usize,
    pub status_history: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelIncidentReplayExportSummary` used across Tau components.
pub struct MultiChannelIncidentReplayExportSummary {
    pub path: String,
    pub event_count: usize,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelIncidentTimelineReport` used across Tau components.
pub struct MultiChannelIncidentTimelineReport {
    pub generated_unix_ms: u64,
    pub state_dir: String,
    pub channel_store_root: String,
    pub window_start_unix_ms: Option<u64>,
    pub window_end_unix_ms: Option<u64>,
    pub event_limit: usize,
    pub scanned_channel_count: usize,
    pub scanned_log_file_count: usize,
    pub scanned_line_count: usize,
    pub invalid_line_count: usize,
    pub total_events_before_limit: usize,
    pub truncated_event_count: usize,
    pub outcomes: MultiChannelIncidentOutcomeCounts,
    pub route_reason_code_counts: BTreeMap<String, usize>,
    pub route_binding_counts: BTreeMap<String, usize>,
    pub policy_reason_code_counts: BTreeMap<String, usize>,
    pub delivery_reason_code_counts: BTreeMap<String, usize>,
    pub diagnostics: Vec<String>,
    pub timeline: Vec<MultiChannelIncidentTimelineEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_export: Option<MultiChannelIncidentReplayExportSummary>,
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelIncidentTimelineQuery` used across Tau components.
pub struct MultiChannelIncidentTimelineQuery {
    pub state_dir: PathBuf,
    pub window_start_unix_ms: Option<u64>,
    pub window_end_unix_ms: Option<u64>,
    pub event_limit: usize,
    pub replay_export_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
struct MultiChannelIncidentEventAggregate {
    first_timestamp_unix_ms: u64,
    last_timestamp_unix_ms: u64,
    transport: String,
    conversation_id: String,
    route_session_key: String,
    route_binding_id: String,
    route_binding_matched: Option<bool>,
    policy_reason_code: String,
    delivery_reason_code: String,
    denied: bool,
    has_response: bool,
    delivery_failed_attempts: usize,
    retryable_failures: usize,
    status_history: Vec<String>,
    records: Vec<ChannelLogEntry>,
}

#[derive(Debug, Clone)]
struct MultiChannelIncidentEventWithEntry {
    entry: MultiChannelIncidentTimelineEntry,
    records: Vec<ChannelLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelIncidentReplayExportFile {
    schema_version: u32,
    generated_unix_ms: u64,
    state_dir: String,
    channel_store_root: String,
    window_start_unix_ms: Option<u64>,
    window_end_unix_ms: Option<u64>,
    outcomes: MultiChannelIncidentOutcomeCounts,
    route_reason_code_counts: BTreeMap<String, usize>,
    route_binding_counts: BTreeMap<String, usize>,
    policy_reason_code_counts: BTreeMap<String, usize>,
    delivery_reason_code_counts: BTreeMap<String, usize>,
    diagnostics: Vec<String>,
    timeline: Vec<MultiChannelIncidentTimelineEntry>,
    events: Vec<MultiChannelIncidentReplayEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelIncidentReplayEvent {
    event_key: String,
    transport: String,
    conversation_id: String,
    route_session_key: String,
    outcome: String,
    records: Vec<ChannelLogEntry>,
}

pub fn build_multi_channel_incident_timeline_report(
    query: &MultiChannelIncidentTimelineQuery,
) -> Result<MultiChannelIncidentTimelineReport> {
    collect_multi_channel_incident_timeline_report(query)
}

fn collect_multi_channel_incident_timeline_report(
    query: &MultiChannelIncidentTimelineQuery,
) -> Result<MultiChannelIncidentTimelineReport> {
    if let (Some(start_unix_ms), Some(end_unix_ms)) =
        (query.window_start_unix_ms, query.window_end_unix_ms)
    {
        if end_unix_ms < start_unix_ms {
            bail!(
                "incident timeline window is invalid: end {} is less than start {}",
                end_unix_ms,
                start_unix_ms
            );
        }
    }

    let mut diagnostics = Vec::new();
    let event_limit = query.event_limit.max(1);
    let channel_store_root = query.state_dir.join("channel-store");
    let channels_root = channel_store_root.join("channels");
    let log_paths = collect_multi_channel_incident_log_paths(&channels_root, &mut diagnostics)?;
    let scanned_channel_count = log_paths.len();
    let scanned_log_file_count = log_paths.len();

    let mut scanned_line_count = 0usize;
    let mut invalid_line_count = 0usize;
    let mut aggregates: BTreeMap<String, MultiChannelIncidentEventAggregate> = BTreeMap::new();

    for (transport, channel_id, log_path) in log_paths {
        let raw = std::fs::read_to_string(&log_path)
            .with_context(|| format!("failed to read {}", log_path.display()))?;
        for (line_index, raw_line) in raw.lines().enumerate() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            scanned_line_count = scanned_line_count.saturating_add(1);
            let parsed_entry = match serde_json::from_str::<ChannelLogEntry>(trimmed) {
                Ok(entry) => entry,
                Err(error) => {
                    invalid_line_count = invalid_line_count.saturating_add(1);
                    push_multi_channel_incident_diagnostic(
                        &mut diagnostics,
                        format!(
                            "{}:{} invalid channel-store log line: {error}",
                            log_path.display(),
                            line_index + 1
                        ),
                    );
                    continue;
                }
            };
            if !multi_channel_incident_timestamp_in_window(
                parsed_entry.timestamp_unix_ms,
                query.window_start_unix_ms,
                query.window_end_unix_ms,
            ) {
                continue;
            }
            let Some(event_key) = multi_channel_incident_event_key(&parsed_entry) else {
                invalid_line_count = invalid_line_count.saturating_add(1);
                push_multi_channel_incident_diagnostic(
                    &mut diagnostics,
                    format!(
                        "{}:{} skipped unkeyed channel-store record",
                        log_path.display(),
                        line_index + 1
                    ),
                );
                continue;
            };
            let aggregate = aggregates.entry(event_key).or_default();
            merge_multi_channel_incident_log_entry(
                aggregate,
                &parsed_entry,
                transport.as_str(),
                channel_id.as_str(),
            );
        }
    }

    let mut events = aggregates
        .into_iter()
        .map(
            |(event_key, aggregate)| MultiChannelIncidentEventWithEntry {
                entry: build_multi_channel_incident_timeline_entry(event_key, &aggregate),
                records: aggregate.records,
            },
        )
        .collect::<Vec<_>>();

    events.sort_by(|left, right| {
        right
            .entry
            .last_timestamp_unix_ms
            .cmp(&left.entry.last_timestamp_unix_ms)
            .then_with(|| {
                right
                    .entry
                    .first_timestamp_unix_ms
                    .cmp(&left.entry.first_timestamp_unix_ms)
            })
            .then_with(|| left.entry.event_key.cmp(&right.entry.event_key))
    });

    let total_events_before_limit = events.len();
    if events.len() > event_limit {
        events.truncate(event_limit);
    }
    let truncated_event_count = total_events_before_limit.saturating_sub(events.len());

    let mut outcomes = MultiChannelIncidentOutcomeCounts::default();
    let mut route_reason_code_counts = BTreeMap::new();
    let mut route_binding_counts = BTreeMap::new();
    let mut policy_reason_code_counts = BTreeMap::new();
    let mut delivery_reason_code_counts = BTreeMap::new();
    for event in &events {
        match event.entry.outcome.as_str() {
            "denied" => outcomes.denied = outcomes.denied.saturating_add(1),
            "retried" => outcomes.retried = outcomes.retried.saturating_add(1),
            "failed" => outcomes.failed = outcomes.failed.saturating_add(1),
            _ => outcomes.allowed = outcomes.allowed.saturating_add(1),
        }
        increment_counter_map(
            &mut route_reason_code_counts,
            event.entry.route_reason_code.as_str(),
            1,
        );
        increment_counter_map(
            &mut route_binding_counts,
            event.entry.route_binding_id.as_str(),
            1,
        );
        increment_counter_map(
            &mut policy_reason_code_counts,
            event.entry.policy_reason_code.as_str(),
            1,
        );
        increment_counter_map(
            &mut delivery_reason_code_counts,
            event.entry.delivery_reason_code.as_str(),
            1,
        );
    }

    let mut report = MultiChannelIncidentTimelineReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        state_dir: query.state_dir.display().to_string(),
        channel_store_root: channel_store_root.display().to_string(),
        window_start_unix_ms: query.window_start_unix_ms,
        window_end_unix_ms: query.window_end_unix_ms,
        event_limit,
        scanned_channel_count,
        scanned_log_file_count,
        scanned_line_count,
        invalid_line_count,
        total_events_before_limit,
        truncated_event_count,
        outcomes,
        route_reason_code_counts,
        route_binding_counts,
        policy_reason_code_counts,
        delivery_reason_code_counts,
        diagnostics,
        timeline: events.iter().map(|event| event.entry.clone()).collect(),
        replay_export: None,
    };

    if let Some(path) = query.replay_export_path.as_ref() {
        let replay_export = write_multi_channel_incident_replay_export(path, &report, &events)?;
        report.replay_export = Some(replay_export);
    }

    Ok(report)
}

fn collect_multi_channel_incident_log_paths(
    channels_root: &Path,
    diagnostics: &mut Vec<String>,
) -> Result<Vec<(String, String, PathBuf)>> {
    if !channels_root.exists() {
        push_multi_channel_incident_diagnostic(
            diagnostics,
            format!(
                "channel-store channels directory is not present: {}",
                channels_root.display()
            ),
        );
        return Ok(Vec::new());
    }
    if !channels_root.is_dir() {
        bail!(
            "channel-store channels path '{}' must be a directory",
            channels_root.display()
        );
    }

    let mut transport_entries = std::fs::read_dir(channels_root)
        .with_context(|| format!("failed to read {}", channels_root.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read {}", channels_root.display()))?;
    transport_entries.sort_by_key(|entry| entry.file_name());

    let mut paths = Vec::new();
    for transport_entry in transport_entries {
        let transport_path = transport_entry.path();
        if !transport_path.is_dir() {
            continue;
        }
        let transport = transport_entry.file_name().to_string_lossy().to_string();
        let mut channel_entries = std::fs::read_dir(&transport_path)
            .with_context(|| format!("failed to read {}", transport_path.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read {}", transport_path.display()))?;
        channel_entries.sort_by_key(|entry| entry.file_name());
        for channel_entry in channel_entries {
            let channel_path = channel_entry.path();
            if !channel_path.is_dir() {
                continue;
            }
            let channel_id = channel_entry.file_name().to_string_lossy().to_string();
            let log_path = channel_path.join("log.jsonl");
            if log_path.is_file() {
                paths.push((transport.clone(), channel_id, log_path));
            }
        }
    }

    Ok(paths)
}

fn merge_multi_channel_incident_log_entry(
    aggregate: &mut MultiChannelIncidentEventAggregate,
    entry: &ChannelLogEntry,
    transport_hint: &str,
    channel_id_hint: &str,
) {
    if aggregate.first_timestamp_unix_ms == 0
        || entry.timestamp_unix_ms < aggregate.first_timestamp_unix_ms
    {
        aggregate.first_timestamp_unix_ms = entry.timestamp_unix_ms;
    }
    if entry.timestamp_unix_ms > aggregate.last_timestamp_unix_ms {
        aggregate.last_timestamp_unix_ms = entry.timestamp_unix_ms;
    }
    if aggregate.transport.is_empty() {
        aggregate.transport =
            extract_multi_channel_incident_payload_text(&entry.payload, "transport")
                .unwrap_or_else(|| transport_hint.to_string());
    }
    if aggregate.conversation_id.is_empty() {
        aggregate.conversation_id =
            extract_multi_channel_incident_payload_text(&entry.payload, "conversation_id")
                .unwrap_or_default();
    }
    if aggregate.route_session_key.is_empty() {
        aggregate.route_session_key =
            extract_multi_channel_incident_payload_text(&entry.payload, "route_session_key")
                .unwrap_or_else(|| channel_id_hint.to_string());
    }
    if aggregate.route_binding_id.is_empty() {
        if let Some(binding_id) = extract_multi_channel_incident_nested_payload_text(
            &entry.payload,
            &["route", "binding_id"],
        ) {
            aggregate.route_binding_id = binding_id;
        }
    }
    if aggregate.route_binding_matched.is_none() {
        aggregate.route_binding_matched = entry
            .payload
            .get("route")
            .and_then(|value| value.get("binding_matched"))
            .and_then(Value::as_bool);
    }
    if let Some(policy_reason_code) = extract_multi_channel_incident_nested_payload_text(
        &entry.payload,
        &["channel_policy", "reason_code"],
    ) {
        aggregate.policy_reason_code = policy_reason_code;
    }
    if let Some(delivery_reason_code) =
        extract_multi_channel_incident_delivery_reason_code(&entry.payload)
    {
        aggregate.delivery_reason_code = delivery_reason_code;
    }

    if entry.direction == "inbound" {
        push_multi_channel_incident_status(&mut aggregate.status_history, "inbound");
    }
    if entry.direction == "outbound" {
        if let Some(status) = extract_multi_channel_incident_payload_text(&entry.payload, "status")
        {
            push_multi_channel_incident_status(&mut aggregate.status_history, status.as_str());
            if status == "denied" {
                aggregate.denied = true;
            }
            if status == "delivery_failed" {
                aggregate.delivery_failed_attempts =
                    aggregate.delivery_failed_attempts.saturating_add(1);
                if entry
                    .payload
                    .get("retryable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    aggregate.retryable_failures = aggregate.retryable_failures.saturating_add(1);
                }
            }
        }
        if entry.payload.get("response").is_some() {
            aggregate.has_response = true;
            push_multi_channel_incident_status(&mut aggregate.status_history, "delivered");
        }
    }

    aggregate.records.push(entry.clone());
}

fn build_multi_channel_incident_timeline_entry(
    event_key: String,
    aggregate: &MultiChannelIncidentEventAggregate,
) -> MultiChannelIncidentTimelineEntry {
    let outcome = multi_channel_incident_outcome(aggregate).to_string();
    let route_reason_code =
        multi_channel_incident_route_reason_code(aggregate.route_binding_matched).to_string();
    let policy_reason_code = if aggregate.policy_reason_code.trim().is_empty() {
        "policy_reason_unknown".to_string()
    } else {
        aggregate.policy_reason_code.clone()
    };
    let delivery_reason_code = if aggregate.delivery_reason_code.trim().is_empty() {
        match outcome.as_str() {
            "denied" => "delivery_denied".to_string(),
            "failed" => "delivery_failed".to_string(),
            "retried" => "delivery_retried".to_string(),
            _ => "delivery_success".to_string(),
        }
    } else {
        aggregate.delivery_reason_code.clone()
    };
    let route_binding_id = if aggregate.route_binding_id.trim().is_empty() {
        "default".to_string()
    } else {
        aggregate.route_binding_id.clone()
    };
    let route_session_key = if aggregate.route_session_key.trim().is_empty() {
        "unknown".to_string()
    } else {
        aggregate.route_session_key.clone()
    };
    MultiChannelIncidentTimelineEntry {
        event_key,
        transport: aggregate.transport.clone(),
        conversation_id: aggregate.conversation_id.clone(),
        route_session_key,
        route_binding_id,
        route_reason_code,
        policy_reason_code,
        delivery_reason_code,
        outcome,
        first_timestamp_unix_ms: aggregate.first_timestamp_unix_ms,
        last_timestamp_unix_ms: aggregate.last_timestamp_unix_ms,
        delivery_failed_attempts: aggregate.delivery_failed_attempts,
        retryable_failures: aggregate.retryable_failures,
        status_history: aggregate.status_history.clone(),
    }
}

fn multi_channel_incident_event_key(entry: &ChannelLogEntry) -> Option<String> {
    entry
        .event_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| extract_multi_channel_incident_payload_text(&entry.payload, "event_key"))
}

fn extract_multi_channel_incident_payload_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn extract_multi_channel_incident_nested_payload_text(
    payload: &Value,
    path: &[&str],
) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut value = payload;
    for key in path {
        value = value.get(*key)?;
    }
    value
        .as_str()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
}

fn extract_multi_channel_incident_delivery_reason_code(payload: &Value) -> Option<String> {
    if let Some(reason_code) = extract_multi_channel_incident_payload_text(payload, "reason_code") {
        return Some(reason_code);
    }
    payload
        .get("delivery")
        .and_then(|value| value.get("receipts"))
        .and_then(Value::as_array)
        .and_then(|receipts| {
            receipts.iter().find_map(|receipt| {
                receipt
                    .get("reason_code")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string())
            })
        })
}

fn multi_channel_incident_timestamp_in_window(
    timestamp_unix_ms: u64,
    window_start_unix_ms: Option<u64>,
    window_end_unix_ms: Option<u64>,
) -> bool {
    if let Some(start_unix_ms) = window_start_unix_ms {
        if timestamp_unix_ms < start_unix_ms {
            return false;
        }
    }
    if let Some(end_unix_ms) = window_end_unix_ms {
        if timestamp_unix_ms > end_unix_ms {
            return false;
        }
    }
    true
}

fn multi_channel_incident_route_reason_code(binding_matched: Option<bool>) -> &'static str {
    match binding_matched {
        Some(true) => "route_binding_matched",
        Some(false) => "route_binding_default",
        None => "route_binding_unknown",
    }
}

fn multi_channel_incident_outcome(aggregate: &MultiChannelIncidentEventAggregate) -> &'static str {
    if aggregate.denied {
        return "denied";
    }
    if aggregate.has_response {
        if aggregate.delivery_failed_attempts > 0 {
            return "retried";
        }
        return "allowed";
    }
    if aggregate.delivery_failed_attempts > 0 {
        return "failed";
    }
    "allowed"
}

fn push_multi_channel_incident_status(status_history: &mut Vec<String>, status: &str) {
    let normalized = status.trim();
    if normalized.is_empty() {
        return;
    }
    if status_history.last().is_some_and(|last| last == normalized) {
        return;
    }
    status_history.push(normalized.to_string());
    if status_history.len() > 12 {
        status_history.remove(0);
    }
}

fn push_multi_channel_incident_diagnostic(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() >= MULTI_CHANNEL_INCIDENT_DIAGNOSTIC_CAP {
        return;
    }
    diagnostics.push(message);
}

pub fn render_multi_channel_incident_timeline_report(
    report: &MultiChannelIncidentTimelineReport,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "multi-channel incident timeline: state_dir={} channel_store_root={} window_start_unix_ms={} window_end_unix_ms={} event_limit={} events={} truncated={} scanned_channels={} scanned_logs={} scanned_lines={} invalid_lines={} outcomes=allowed:{}|denied:{}|retried:{}|failed:{} route_reason_code_counts={} policy_reason_code_counts={} delivery_reason_code_counts={} replay_export={}",
        report.state_dir,
        report.channel_store_root,
        report
            .window_start_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .window_end_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report.event_limit,
        report.timeline.len(),
        report.truncated_event_count,
        report.scanned_channel_count,
        report.scanned_log_file_count,
        report.scanned_line_count,
        report.invalid_line_count,
        report.outcomes.allowed,
        report.outcomes.denied,
        report.outcomes.retried,
        report.outcomes.failed,
        render_multi_channel_incident_counter_map(&report.route_reason_code_counts),
        render_multi_channel_incident_counter_map(&report.policy_reason_code_counts),
        render_multi_channel_incident_counter_map(&report.delivery_reason_code_counts),
        report
            .replay_export
            .as_ref()
            .map(|summary| summary.path.as_str())
            .unwrap_or("none"),
    ));
    for entry in &report.timeline {
        lines.push(format!(
            "multi-channel incident event: event_key={} outcome={} transport={} conversation_id={} route_session_key={} route_binding_id={} route_reason_code={} policy_reason_code={} delivery_reason_code={} first_timestamp_unix_ms={} last_timestamp_unix_ms={} delivery_failed_attempts={} retryable_failures={} status_history={}",
            entry.event_key,
            entry.outcome,
            entry.transport,
            if entry.conversation_id.is_empty() {
                "unknown"
            } else {
                entry.conversation_id.as_str()
            },
            entry.route_session_key,
            entry.route_binding_id,
            entry.route_reason_code,
            entry.policy_reason_code,
            entry.delivery_reason_code,
            entry.first_timestamp_unix_ms,
            entry.last_timestamp_unix_ms,
            entry.delivery_failed_attempts,
            entry.retryable_failures,
            if entry.status_history.is_empty() {
                "none".to_string()
            } else {
                entry.status_history.join(",")
            }
        ));
    }
    if !report.diagnostics.is_empty() {
        lines.push(format!(
            "multi-channel incident diagnostics: count={} sample={}",
            report.diagnostics.len(),
            report.diagnostics.join(" | ")
        ));
    }
    lines.join("\n")
}

fn render_multi_channel_incident_counter_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn write_multi_channel_incident_replay_export(
    path: &Path,
    report: &MultiChannelIncidentTimelineReport,
    events: &[MultiChannelIncidentEventWithEntry],
) -> Result<MultiChannelIncidentReplayExportSummary> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let mut replay_events = events
        .iter()
        .map(|event| {
            let mut records = event.records.clone();
            records.sort_by(|left, right| {
                left.timestamp_unix_ms
                    .cmp(&right.timestamp_unix_ms)
                    .then_with(|| left.direction.cmp(&right.direction))
                    .then_with(|| left.source.cmp(&right.source))
            });
            MultiChannelIncidentReplayEvent {
                event_key: event.entry.event_key.clone(),
                transport: event.entry.transport.clone(),
                conversation_id: event.entry.conversation_id.clone(),
                route_session_key: event.entry.route_session_key.clone(),
                outcome: event.entry.outcome.clone(),
                records,
            }
        })
        .collect::<Vec<_>>();
    replay_events.sort_by(|left, right| left.event_key.cmp(&right.event_key));

    let payload = MultiChannelIncidentReplayExportFile {
        schema_version: MULTI_CHANNEL_INCIDENT_REPLAY_EXPORT_SCHEMA_VERSION,
        generated_unix_ms: report.generated_unix_ms,
        state_dir: report.state_dir.clone(),
        channel_store_root: report.channel_store_root.clone(),
        window_start_unix_ms: report.window_start_unix_ms,
        window_end_unix_ms: report.window_end_unix_ms,
        outcomes: report.outcomes.clone(),
        route_reason_code_counts: report.route_reason_code_counts.clone(),
        route_binding_counts: report.route_binding_counts.clone(),
        policy_reason_code_counts: report.policy_reason_code_counts.clone(),
        delivery_reason_code_counts: report.delivery_reason_code_counts.clone(),
        diagnostics: report.diagnostics.clone(),
        timeline: report.timeline.clone(),
        events: replay_events,
    };
    let mut rendered = serde_json::to_string_pretty(&payload)
        .context("failed to render multi-channel incident replay export json")?;
    rendered.push('\n');
    write_text_atomic(path, &rendered)
        .with_context(|| format!("failed to write {}", path.display()))?;
    let checksum = format!("{:x}", Sha256::digest(rendered.as_bytes()));
    Ok(MultiChannelIncidentReplayExportSummary {
        path: path.display().to_string(),
        event_count: payload.events.len(),
        checksum_sha256: checksum,
    })
}

fn increment_counter_map(map: &mut BTreeMap<String, usize>, key: &str, delta: usize) {
    if key.trim().is_empty() || delta == 0 {
        return;
    }
    let entry = map.entry(key.to_string()).or_default();
    *entry = entry.saturating_add(delta);
}
