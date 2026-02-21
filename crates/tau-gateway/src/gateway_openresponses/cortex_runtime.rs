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
use tau_ai::{ChatRequest, Message, PromptCacheConfig};
use tau_core::current_unix_timestamp_ms;
use tau_memory::runtime::FileMemoryStore;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::channel_telemetry_runtime::append_jsonl_record;
use super::{
    authorize_dashboard_request, parse_gateway_json_body, GatewayOpenResponsesServerState,
    OpenResponsesApiError, SseFrame,
};

const CORTEX_OBSERVER_EVENTS_FILE: &str = "cortex-observer-events.jsonl";
const CORTEX_OBSERVER_SCHEMA_VERSION: u32 = 1;
const CORTEX_OBSERVER_RECENT_EVENTS_LIMIT: usize = 32;
const CORTEX_READINESS_STALE_MAX_AGE_SECONDS: u64 = 21_600;
const CORTEX_REQUIRED_CHAT_EVENT_TYPE: &str = "cortex.chat.request";
const CORTEX_CHAT_MAX_PROMPT_CHARS: usize = 8_000;
const CORTEX_CHAT_MAX_OUTPUT_CHARS: usize = 4_000;
const CORTEX_CHAT_MAX_BULLETIN_CHARS: usize = 1_200;
const CORTEX_CHAT_MAX_OBSERVER_DIAGNOSTICS: usize = 3;
const CORTEX_CHAT_MAX_MEMORY_DIAGNOSTICS: usize = 3;
const CORTEX_CHAT_MEMORY_MAX_SESSIONS: usize = 8;
const CORTEX_CHAT_MEMORY_MAX_RECORDS_PER_SESSION: usize = 8;
const CORTEX_CHAT_SYSTEM_PROMPT: &str = "You are Tau Cortex admin copilot. Provide concise,\
 actionable operator guidance grounded in supplied runtime context. Respond with plain text only.";

#[derive(Debug, Clone, PartialEq, Eq)]
struct CortexChatOutput {
    output_text: String,
    reason_code: &'static str,
    fallback: bool,
}

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
    health_state: String,
    rollout_gate: String,
    reason_code: String,
    health_reason: String,
    total_events: u64,
    invalid_events: u64,
    last_event_unix_ms: Option<u64>,
    last_event_age_seconds: Option<u64>,
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
            health_state: "unknown".to_string(),
            rollout_gate: "hold".to_string(),
            reason_code: "cortex_status_uninitialized".to_string(),
            health_reason: "cortex readiness status not yet evaluated".to_string(),
            total_events: 0,
            invalid_events: 0,
            last_event_unix_ms: None,
            last_event_age_seconds: None,
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
    let output = complete_cortex_chat(&state, normalized_input).await;
    let _ = record_cortex_observer_event(
        &state.config.state_dir,
        "cortex.chat.request",
        json!({
            "response_id": response_id.clone(),
            "input_chars": normalized_input.chars().count(),
            "output_chars": output.output_text.chars().count(),
            "reason_code": output.reason_code,
            "fallback": output.fallback,
        }),
    );
    let output_text = output.output_text.clone();

    let (tx, rx) = mpsc::unbounded_channel::<SseFrame>();
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.created",
        payload: json!({
            "schema_version": 1,
            "response_id": response_id.clone(),
            "created_unix_ms": created_unix_ms,
            "reason_code": output.reason_code,
            "fallback": output.fallback,
        }),
    });
    let _ = tx.send(SseFrame::Json {
        event: "cortex.response.output_text.delta",
        payload: json!({
            "response_id": response_id.clone(),
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
        apply_cortex_readiness_classification(&mut report, &events_path);
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
            apply_cortex_readiness_classification(&mut report, &events_path);
            return Ok(report);
        }
    };

    let mut last_event_unix_ms = None::<u64>;

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
        last_event_unix_ms = Some(
            last_event_unix_ms
                .map(|existing| existing.max(parsed.timestamp_unix_ms))
                .unwrap_or(parsed.timestamp_unix_ms),
        );

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

    report.last_event_unix_ms = last_event_unix_ms;
    report.last_event_age_seconds = report.last_event_unix_ms.map(|last_seen| {
        report
            .generated_unix_ms
            .saturating_sub(last_seen)
            .saturating_div(1000)
    });
    apply_cortex_readiness_classification(&mut report, &events_path);

    Ok(report)
}

fn apply_cortex_readiness_classification(
    report: &mut GatewayCortexStatusReport,
    events_path: &Path,
) {
    report.rollout_gate = "hold".to_string();

    if !report.state_present {
        report.health_state = "failing".to_string();
        report.reason_code = "cortex_observer_events_missing".to_string();
        report.health_reason = format!(
            "cortex observer events artifact is missing at {}",
            events_path.display()
        );
        return;
    }

    if report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.starts_with("cortex_observer_events_read_failed:"))
    {
        report.health_state = "failing".to_string();
        report.reason_code = "cortex_observer_events_read_failed".to_string();
        report.health_reason = "failed to read cortex observer event history".to_string();
        return;
    }

    if report.total_events == 0 && report.invalid_events > 0 {
        report.health_state = "failing".to_string();
        report.reason_code = "cortex_observer_events_malformed".to_string();
        report.health_reason = format!(
            "cortex observer history contains {} malformed event lines and no valid events",
            report.invalid_events
        );
        return;
    }

    if report.total_events == 0 {
        report.health_state = "degraded".to_string();
        report.reason_code = "cortex_observer_events_empty".to_string();
        report.health_reason = "cortex observer history has no valid readiness events".to_string();
        return;
    }

    if report.invalid_events > 0 {
        report.health_state = "degraded".to_string();
        report.reason_code = "cortex_observer_events_malformed".to_string();
        report.health_reason = format!(
            "cortex observer history includes {} malformed event lines",
            report.invalid_events
        );
        return;
    }

    let cortex_chat_count = report
        .event_type_counts
        .get(CORTEX_REQUIRED_CHAT_EVENT_TYPE)
        .copied()
        .unwrap_or(0);
    if cortex_chat_count == 0 {
        report.health_state = "degraded".to_string();
        report.reason_code = "cortex_chat_activity_missing".to_string();
        report.health_reason =
            "no cortex.chat.request event has been observed for readiness validation".to_string();
        return;
    }

    let Some(last_event_age_seconds) = report.last_event_age_seconds else {
        report.health_state = "degraded".to_string();
        report.reason_code = "cortex_observer_last_event_unknown".to_string();
        report.health_reason = "latest cortex observer event timestamp is unavailable".to_string();
        return;
    };

    if last_event_age_seconds > CORTEX_READINESS_STALE_MAX_AGE_SECONDS {
        report.health_state = "degraded".to_string();
        report.reason_code = "cortex_observer_events_stale".to_string();
        report.health_reason = format!(
            "latest cortex observer event is stale (age={}s max={}s)",
            last_event_age_seconds, CORTEX_READINESS_STALE_MAX_AGE_SECONDS
        );
        return;
    }

    report.health_state = "healthy".to_string();
    report.rollout_gate = "pass".to_string();
    report.reason_code = "cortex_ready".to_string();
    report.health_reason =
        "cortex observer history is fresh and includes validated cortex chat activity".to_string();
}

fn gateway_cortex_observer_events_path(state_dir: &Path) -> PathBuf {
    state_dir
        .join("openresponses")
        .join(CORTEX_OBSERVER_EVENTS_FILE)
}

async fn complete_cortex_chat(
    state: &Arc<GatewayOpenResponsesServerState>,
    input: &str,
) -> CortexChatOutput {
    let observer_summary = match load_cortex_status_report(&state.config.state_dir) {
        Ok(report) => render_cortex_observer_summary(&report),
        Err(error) => format!(
            "health_state=unavailable rollout_gate=hold reason_code={} diagnostics={}",
            error.code,
            truncate_chars(collapse_whitespace(error.message.as_str()).as_str(), 160)
        ),
    };
    let bulletin_summary =
        render_cortex_bulletin_context(state.cortex.bulletin_snapshot().as_str());
    let memory_graph_summary = render_cortex_memory_graph_summary(&state.config.state_dir);
    let user_prompt = truncate_chars(
        render_cortex_chat_user_prompt(
            input,
            observer_summary.as_str(),
            bulletin_summary.as_str(),
            memory_graph_summary.as_str(),
        )
        .as_str(),
        CORTEX_CHAT_MAX_PROMPT_CHARS,
    );
    let request = ChatRequest {
        model: state.config.model.clone(),
        messages: vec![
            Message::system(CORTEX_CHAT_SYSTEM_PROMPT),
            Message::user(user_prompt),
        ],
        tools: Vec::new(),
        tool_choice: None,
        json_mode: false,
        max_tokens: Some(384),
        temperature: Some(0.0),
        prompt_cache: PromptCacheConfig::default(),
    };

    match state.config.client.complete(request).await {
        Ok(response) => {
            let output_text = truncate_chars(
                collapse_whitespace(response.message.text_content().as_str()).as_str(),
                CORTEX_CHAT_MAX_OUTPUT_CHARS,
            );
            if output_text.is_empty() {
                let reason_code = "cortex_chat_llm_empty_fallback";
                return CortexChatOutput {
                    output_text: render_cortex_fallback_output(
                        input,
                        state.config.model.as_str(),
                        reason_code,
                    ),
                    reason_code,
                    fallback: true,
                };
            }
            CortexChatOutput {
                output_text,
                reason_code: "cortex_chat_llm_applied",
                fallback: false,
            }
        }
        Err(_) => {
            let reason_code = "cortex_chat_llm_error_fallback";
            CortexChatOutput {
                output_text: render_cortex_fallback_output(
                    input,
                    state.config.model.as_str(),
                    reason_code,
                ),
                reason_code,
                fallback: true,
            }
        }
    }
}

fn render_cortex_chat_user_prompt(
    input: &str,
    observer_summary: &str,
    bulletin_summary: &str,
    memory_graph_summary: &str,
) -> String {
    format!(
        "[operator_query]\n{}\n\n[observer_status]\n{}\n\n[cortex_bulletin]\n{}\n\n[memory_graph]\n{}\n\n[response_style]\nReturn concise operator guidance with immediate next checks and explicit risks.",
        input.trim(),
        observer_summary.trim(),
        bulletin_summary.trim(),
        memory_graph_summary.trim(),
    )
}

fn render_cortex_observer_summary(report: &GatewayCortexStatusReport) -> String {
    let mut event_type_counts = report
        .event_type_counts
        .iter()
        .map(|(event_type, count)| (event_type.clone(), *count))
        .collect::<Vec<_>>();
    event_type_counts
        .sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    event_type_counts.truncate(5);
    let event_summary = if event_type_counts.is_empty() {
        "none".to_string()
    } else {
        event_type_counts
            .iter()
            .map(|(event_type, count)| format!("{event_type}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    let diagnostics = format_bounded_diagnostics(
        report.diagnostics.as_slice(),
        CORTEX_CHAT_MAX_OBSERVER_DIAGNOSTICS,
    );
    format!(
        "health_state={} rollout_gate={} reason_code={} total_events={} invalid_events={} last_event_age_seconds={} top_event_types={} diagnostics={}",
        report.health_state,
        report.rollout_gate,
        report.reason_code,
        report.total_events,
        report.invalid_events,
        report
            .last_event_age_seconds
            .map(|age| age.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        event_summary,
        diagnostics,
    )
}

fn render_cortex_bulletin_context(bulletin_snapshot: &str) -> String {
    let bulletin = truncate_chars(
        collapse_whitespace(bulletin_snapshot).as_str(),
        CORTEX_CHAT_MAX_BULLETIN_CHARS,
    );
    if bulletin.is_empty() {
        "bulletin_unavailable: cortex bulletin snapshot is empty".to_string()
    } else {
        bulletin
    }
}

fn render_cortex_memory_graph_summary(state_dir: &Path) -> String {
    let memory_store_root = state_dir.join("openresponses").join("memory-store");
    if !memory_store_root.exists() {
        return "reason_code=memory_graph_store_missing sessions_scanned=0 records_total=0 relation_edges_total=0 memory_types=none diagnostics=store_missing".to_string();
    }
    if !memory_store_root.is_dir() {
        return format!(
            "reason_code=memory_graph_store_not_directory sessions_scanned=0 records_total=0 relation_edges_total=0 memory_types=none diagnostics=root_not_directory:{}",
            memory_store_root.display()
        );
    }

    let mut diagnostics = Vec::<String>::new();
    let dir_entries = match std::fs::read_dir(&memory_store_root) {
        Ok(entries) => entries,
        Err(error) => {
            return format!(
                "reason_code=memory_graph_store_unreadable sessions_scanned=0 records_total=0 relation_edges_total=0 memory_types=none diagnostics={}",
                truncate_chars(
                    format!("read_dir_failed:{error}").as_str(),
                    160
                )
            );
        }
    };

    let mut session_paths = dir_entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    session_paths.sort();
    if session_paths.len() > CORTEX_CHAT_MEMORY_MAX_SESSIONS {
        diagnostics.push(format!(
            "sessions_truncated:discovered={} max_sessions={}",
            session_paths.len(),
            CORTEX_CHAT_MEMORY_MAX_SESSIONS
        ));
    }

    let mut sessions_scanned = 0u64;
    let mut records_total = 0u64;
    let mut relation_edges_total = 0u64;
    let mut memory_type_counts = BTreeMap::<String, u64>::new();
    for session_path in session_paths
        .into_iter()
        .take(CORTEX_CHAT_MEMORY_MAX_SESSIONS)
    {
        let store = FileMemoryStore::new(&session_path);
        match store.list_latest_records(None, CORTEX_CHAT_MEMORY_MAX_RECORDS_PER_SESSION) {
            Ok(records) => {
                sessions_scanned = sessions_scanned.saturating_add(1);
                records_total = records_total.saturating_add(records.len() as u64);
                for record in records {
                    relation_edges_total =
                        relation_edges_total.saturating_add(record.relations.len() as u64);
                    *memory_type_counts
                        .entry(record.memory_type.as_str().to_string())
                        .or_default() += 1;
                }
            }
            Err(error) => diagnostics.push(format!(
                "session_read_failed:{}:{}",
                session_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unknown"),
                truncate_chars(error.to_string().as_str(), 120)
            )),
        }
    }

    let reason_code = if records_total == 0 {
        "memory_graph_records_unavailable"
    } else if diagnostics.is_empty() {
        "memory_graph_summary_ok"
    } else {
        "memory_graph_summary_partial"
    };
    let memory_types = if memory_type_counts.is_empty() {
        "none".to_string()
    } else {
        memory_type_counts
            .iter()
            .map(|(memory_type, count)| format!("{memory_type}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };

    format!(
        "reason_code={} sessions_scanned={} records_total={} relation_edges_total={} memory_types={} diagnostics={}",
        reason_code,
        sessions_scanned,
        records_total,
        relation_edges_total,
        memory_types,
        format_bounded_diagnostics(
            diagnostics.as_slice(),
            CORTEX_CHAT_MAX_MEMORY_DIAGNOSTICS
        ),
    )
}

fn render_cortex_fallback_output(input: &str, model: &str, reason_code: &str) -> String {
    format!(
        "Cortex fallback response engaged. reason_code={reason_code} model={} input_chars={}. Review observer status, bulletin context, and memory graph diagnostics before acting.",
        model.trim(),
        input.chars().count()
    )
}

fn format_bounded_diagnostics(values: &[String], limit: usize) -> String {
    let mut diagnostics = values
        .iter()
        .take(limit)
        .map(|value| truncate_chars(collapse_whitespace(value.as_str()).as_str(), 120))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.len() > limit {
        diagnostics.push(format!("+{} more", values.len().saturating_sub(limit)));
    }
    if diagnostics.is_empty() {
        "none".to_string()
    } else {
        diagnostics.join(";")
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn unit_render_cortex_chat_user_prompt_includes_required_context_markers() {
        let prompt = render_cortex_chat_user_prompt(
            "summarize risks",
            "health_state=healthy",
            "## Cortex Memory Bulletin - release stabilization",
            "reason_code=memory_graph_summary_ok",
        );
        assert!(prompt.contains("[operator_query]"));
        assert!(prompt.contains("[observer_status]"));
        assert!(prompt.contains("[cortex_bulletin]"));
        assert!(prompt.contains("[memory_graph]"));
    }

    #[test]
    fn unit_render_cortex_fallback_output_includes_reason_and_model() {
        let output = render_cortex_fallback_output(
            "hello",
            "openai/gpt-4o-mini",
            "cortex_chat_llm_error_fallback",
        );
        assert!(output.contains("Cortex fallback response engaged"));
        assert!(output.contains("cortex_chat_llm_error_fallback"));
        assert!(output.contains("openai/gpt-4o-mini"));
        assert!(output.contains("input_chars=5"));
    }

    #[test]
    fn unit_render_cortex_memory_graph_summary_reports_missing_store() {
        let temp = tempdir().expect("tempdir");
        let summary = render_cortex_memory_graph_summary(temp.path());
        assert!(summary.contains("reason_code=memory_graph_store_missing"));
        assert!(summary.contains("records_total=0"));
    }

    #[test]
    fn unit_load_cortex_status_report_returns_missing_artifact_fallback() {
        let temp = tempdir().expect("tempdir");
        let report = load_cortex_status_report(temp.path()).expect("load fallback report");
        assert!(!report.state_present);
        assert_eq!(report.health_state, "failing");
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.reason_code, "cortex_observer_events_missing");
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
        assert_eq!(report.health_state, "degraded");
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.reason_code, "cortex_observer_events_malformed");
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

    #[test]
    fn unit_load_cortex_status_report_marks_healthy_for_fresh_chat_activity() {
        let temp = tempdir().expect("tempdir");
        let events_path = gateway_cortex_observer_events_path(temp.path());
        std::fs::create_dir_all(events_path.parent().expect("events parent"))
            .expect("create events directory");
        let now = current_unix_timestamp_ms();
        std::fs::write(
            &events_path,
            format!(
                "{{\"schema_version\":1,\"timestamp_unix_ms\":{now},\"event_type\":\"cortex.chat.request\",\"metadata\":{{\"response_id\":\"r1\"}}}}\n"
            ),
        )
        .expect("write events file");

        let report = load_cortex_status_report(temp.path()).expect("load status report");
        assert_eq!(report.health_state, "healthy");
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.reason_code, "cortex_ready");
        assert_eq!(
            report.event_type_counts.get("cortex.chat.request").copied(),
            Some(1)
        );
        assert_eq!(report.last_event_unix_ms, Some(now));
        assert!(report
            .last_event_age_seconds
            .map(|value| value <= CORTEX_READINESS_STALE_MAX_AGE_SECONDS)
            .unwrap_or(false));
    }

    #[test]
    fn unit_load_cortex_status_report_flags_stale_or_missing_chat_readiness() {
        let temp = tempdir().expect("tempdir");
        let events_path = gateway_cortex_observer_events_path(temp.path());
        std::fs::create_dir_all(events_path.parent().expect("events parent"))
            .expect("create events directory");
        let stale_timestamp_ms = current_unix_timestamp_ms().saturating_sub(
            (CORTEX_READINESS_STALE_MAX_AGE_SECONDS.saturating_add(60)).saturating_mul(1000),
        );
        std::fs::write(
            &events_path,
            format!(
                "{{\"schema_version\":1,\"timestamp_unix_ms\":{stale_timestamp_ms},\"event_type\":\"cortex.chat.request\",\"metadata\":{{\"response_id\":\"r1\"}}}}\n"
            ),
        )
        .expect("write stale events file");

        let stale_report =
            load_cortex_status_report(temp.path()).expect("load stale status report");
        assert_eq!(stale_report.health_state, "degraded");
        assert_eq!(stale_report.reason_code, "cortex_observer_events_stale");

        std::fs::write(
            &events_path,
            format!(
                "{{\"schema_version\":1,\"timestamp_unix_ms\":{},\"event_type\":\"session.append\",\"metadata\":{{\"session_key\":\"default\"}}}}\n",
                current_unix_timestamp_ms()
            ),
        )
        .expect("write non-chat events file");
        let missing_chat_report =
            load_cortex_status_report(temp.path()).expect("load missing chat status report");
        assert_eq!(missing_chat_report.health_state, "degraded");
        assert_eq!(
            missing_chat_report.reason_code,
            "cortex_chat_activity_missing"
        );
    }
}
