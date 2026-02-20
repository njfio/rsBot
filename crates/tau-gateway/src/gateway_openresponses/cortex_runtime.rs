//! Cortex admin chat endpoint handlers.
//!
//! This module provides a deterministic SSE contract for the initial Cortex
//! admin chat API foundation.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_core::current_unix_timestamp_ms;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{
    append_jsonl_record, authorize_dashboard_request, parse_gateway_json_body,
    GatewayOpenResponsesServerState, OpenResponsesApiError, SseFrame,
};

const CORTEX_OBSERVER_EVENTS_FILE: &str = "cortex-observer-events.jsonl";
const CORTEX_OBSERVER_SCHEMA_VERSION: u32 = 1;
const CORTEX_OBSERVER_RECENT_EVENTS_LIMIT: usize = 32;

#[derive(Debug, Deserialize)]
struct GatewayCortexChatRequest {
    input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct GatewayCortexObserverEventRecord {
    #[serde(default = "cortex_observer_schema_version")]
    schema_version: u32,
    #[serde(default)]
    timestamp_unix_ms: u64,
    #[serde(default)]
    event_type: String,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct GatewayCortexStatusReport {
    schema_version: u32,
    generated_unix_ms: u64,
    state_present: bool,
    total_events: u64,
    invalid_events: u64,
    event_type_counts: BTreeMap<String, u64>,
    recent_events: Vec<GatewayCortexObserverEventRecord>,
    diagnostics: Vec<String>,
}

impl Default for GatewayCortexStatusReport {
    fn default() -> Self {
        Self {
            schema_version: CORTEX_OBSERVER_SCHEMA_VERSION,
            generated_unix_ms: current_unix_timestamp_ms(),
            state_present: false,
            total_events: 0,
            invalid_events: 0,
            event_type_counts: BTreeMap::new(),
            recent_events: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

fn cortex_observer_schema_version() -> u32 {
    CORTEX_OBSERVER_SCHEMA_VERSION
}

pub(super) async fn handle_cortex_chat(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }

    let request = match parse_gateway_json_body::<GatewayCortexChatRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let normalized_input = request.input.trim();
    if normalized_input.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_cortex_input",
            "input must be non-empty",
        )
        .into_response();
    }

    let response_id = format!("cortex_{}", state.next_response_id());
    let created_unix_ms = current_unix_timestamp_ms();
    let output_text = render_cortex_output_text(normalized_input, state.config.model.as_str());
    let _ = record_cortex_observer_event(
        &state.config.state_dir,
        "cortex.chat.request",
        json!({
            "response_id": response_id.clone(),
            "input_chars": normalized_input.chars().count(),
        }),
    );

    let (tx, rx) = mpsc::unbounded_channel::<SseFrame>();
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.created",
        payload: json!({
            "schema_version": 1,
            "response_id": response_id,
            "created_unix_ms": created_unix_ms,
        }),
    });
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.output_text.delta",
        payload: json!({
            "response_id": response_id,
            "delta": output_text,
        }),
    });
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.output_text.done",
        payload: json!({
            "response_id": response_id,
            "text": output_text,
        }),
    });
    let _ = tx.send(SseFrame::Done);
    drop(tx);

    let stream =
        UnboundedReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

pub(super) async fn handle_cortex_status(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }

    match load_cortex_status_report(&state.config.state_dir) {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(error) => error.into_response(),
    }
}

pub(super) fn record_cortex_observer_event(
    state_dir: &Path,
    event_type: &str,
    metadata: Value,
) -> Result<(), anyhow::Error> {
    let normalized_event_type = event_type.trim();
    if normalized_event_type.is_empty() {
        return Ok(());
    }
    let record = json!({
        "schema_version": CORTEX_OBSERVER_SCHEMA_VERSION,
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "event_type": normalized_event_type,
        "metadata": metadata,
    });
    append_jsonl_record(&gateway_cortex_observer_events_path(state_dir), &record)
}

pub(super) fn record_cortex_session_append_event(
    state_dir: &Path,
    session_key: &str,
    head_id: Option<u64>,
    entry_count: usize,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "session.append",
        json!({
            "session_key": session_key,
            "head_id": head_id,
            "entry_count": entry_count,
        }),
    );
}

pub(super) fn record_cortex_session_reset_event(state_dir: &Path, session_key: &str, reset: bool) {
    let _ = record_cortex_observer_event(
        state_dir,
        "session.reset",
        json!({"session_key": session_key, "reset": reset}),
    );
}

pub(super) fn record_cortex_external_session_opened(
    state_dir: &Path,
    session_id: &str,
    workspace_id: &str,
    status: &str,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "external_coding_agent.session_opened",
        json!({
            "session_id": session_id,
            "workspace_id": workspace_id,
            "status": status,
        }),
    );
}

pub(super) fn record_cortex_external_session_closed(
    state_dir: &Path,
    session_id: &str,
    workspace_id: &str,
    status: &str,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "external_coding_agent.session_closed",
        json!({
            "session_id": session_id,
            "workspace_id": workspace_id,
            "status": status,
        }),
    );
}

pub(super) fn record_cortex_memory_write_event(state_dir: &Path, session_key: &str, bytes: usize) {
    let _ = record_cortex_observer_event(
        state_dir,
        "memory.write",
        json!({
            "session_key": session_key,
            "bytes": bytes,
        }),
    );
}

pub(super) fn record_cortex_memory_entry_write_event(
    state_dir: &Path,
    session_key: &str,
    entry_id: &str,
    created: bool,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "memory.entry_write",
        json!({
            "session_key": session_key,
            "entry_id": entry_id,
            "created": created,
        }),
    );
}

pub(super) fn record_cortex_memory_entry_delete_event(
    state_dir: &Path,
    session_key: &str,
    entry_id: &str,
    deleted: bool,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "memory.entry_delete",
        json!({
            "session_key": session_key,
            "entry_id": entry_id,
            "deleted": deleted,
        }),
    );
}

pub(super) fn record_cortex_external_progress_event(
    state_dir: &Path,
    session_id: &str,
    sequence_id: u64,
    message: &str,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "external_coding_agent.progress",
        json!({
            "session_id": session_id,
            "sequence_id": sequence_id,
            "message": message,
        }),
    );
}

pub(super) fn record_cortex_external_followup_event(
    state_dir: &Path,
    session_id: &str,
    sequence_id: u64,
    message: &str,
) {
    let _ = record_cortex_observer_event(
        state_dir,
        "external_coding_agent.followup_queued",
        json!({
            "session_id": session_id,
            "sequence_id": sequence_id,
            "message": message,
        }),
    );
}

fn load_cortex_status_report(
    state_dir: &Path,
) -> Result<GatewayCortexStatusReport, OpenResponsesApiError> {
    let events_path = gateway_cortex_observer_events_path(state_dir);
    let mut report = GatewayCortexStatusReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        ..GatewayCortexStatusReport::default()
    };

    if !events_path.exists() {
        report.diagnostics.push(format!(
            "cortex_observer_events_missing:{}",
            events_path.display()
        ));
        return Ok(report);
    }

    report.state_present = true;
    let raw = match std::fs::read_to_string(&events_path) {
        Ok(raw) => raw,
        Err(error) => {
            report.diagnostics.push(format!(
                "cortex_observer_events_read_failed:{}:{error}",
                events_path.display()
            ));
            return Ok(report);
        }
    };

    for (line_number, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = match serde_json::from_str::<GatewayCortexObserverEventRecord>(trimmed) {
            Ok(parsed) => parsed,
            Err(_) => {
                report.invalid_events = report.invalid_events.saturating_add(1);
                report.diagnostics.push(format!(
                    "cortex_observer_event_malformed_line:{}",
                    line_number.saturating_add(1)
                ));
                continue;
            }
        };

        let normalized_event_type = parsed.event_type.trim();
        if normalized_event_type.is_empty() {
            report.invalid_events = report.invalid_events.saturating_add(1);
            report.diagnostics.push(format!(
                "cortex_observer_event_type_missing_line:{}",
                line_number.saturating_add(1)
            ));
            continue;
        }

        report.total_events = report.total_events.saturating_add(1);
        *report
            .event_type_counts
            .entry(normalized_event_type.to_string())
            .or_default() += 1;

        report.recent_events.push(GatewayCortexObserverEventRecord {
            event_type: normalized_event_type.to_string(),
            ..parsed
        });
        if report.recent_events.len() > CORTEX_OBSERVER_RECENT_EVENTS_LIMIT {
            let drop_count = report
                .recent_events
                .len()
                .saturating_sub(CORTEX_OBSERVER_RECENT_EVENTS_LIMIT);
            report.recent_events.drain(0..drop_count);
        }
    }

    if report.total_events == 0 && report.invalid_events == 0 {
        report.diagnostics.push(format!(
            "cortex_observer_events_empty:{}",
            events_path.display()
        ));
    }

    Ok(report)
}

fn gateway_cortex_observer_events_path(state_dir: &Path) -> PathBuf {
    state_dir
        .join("openresponses")
        .join(CORTEX_OBSERVER_EVENTS_FILE)
}

fn render_cortex_output_text(input: &str, model: &str) -> String {
    format!(
        "Cortex admin foundation active. Received {} characters. Active model: {}.",
        input.chars().count(),
        model
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unit_render_cortex_output_text_includes_char_count_and_model() {
        let output = render_cortex_output_text("hello", "openai/gpt-4o-mini");
        assert!(output.contains("Received 5 characters"));
        assert!(output.contains("openai/gpt-4o-mini"));
    }

    #[test]
    fn unit_load_cortex_status_report_returns_missing_artifact_fallback() {
        let temp = tempdir().expect("tempdir");
        let report = load_cortex_status_report(temp.path()).expect("load fallback report");
        assert!(!report.state_present);
        assert_eq!(report.total_events, 0);
        assert_eq!(report.invalid_events, 0);
        assert!(report.event_type_counts.is_empty());
        assert!(report.recent_events.is_empty());
        assert!(!report.diagnostics.is_empty());
    }

    #[test]
    fn unit_load_cortex_status_report_aggregates_valid_and_invalid_events() {
        let temp = tempdir().expect("tempdir");
        let events_path = gateway_cortex_observer_events_path(temp.path());
        std::fs::create_dir_all(events_path.parent().expect("events parent"))
            .expect("create events directory");
        std::fs::write(
            &events_path,
            concat!(
                "{\"schema_version\":1,\"timestamp_unix_ms\":1,\"event_type\":\"cortex.chat.request\",\"metadata\":{\"response_id\":\"r1\"}}\n",
                "not-json\n",
                "{\"schema_version\":1,\"timestamp_unix_ms\":2,\"event_type\":\"session.append\",\"metadata\":{\"session_key\":\"default\"}}\n"
            ),
        )
        .expect("write events file");

        let report = load_cortex_status_report(temp.path()).expect("load status report");
        assert!(report.state_present);
        assert_eq!(report.total_events, 2);
        assert_eq!(report.invalid_events, 1);
        assert_eq!(
            report.event_type_counts.get("cortex.chat.request").copied(),
            Some(1)
        );
        assert_eq!(
            report.event_type_counts.get("session.append").copied(),
            Some(1)
        );
        assert_eq!(report.recent_events.len(), 2);
    }
}
