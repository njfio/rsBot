use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use tau_ai::{Message, MessageRole};
use tau_session::SessionStore;

use super::types::{GatewaySessionAppendRequest, GatewaySessionResetRequest};
use super::{
    authorize_and_enforce_gateway_limits, enforce_policy_gate, gateway_session_path,
    parse_gateway_json_body, record_cortex_session_append_event, record_cortex_session_reset_event,
    sanitize_session_key, system_time_to_unix_ms, GatewayOpenResponsesServerState,
    OpenResponsesApiError, SESSION_WRITE_POLICY_GATE,
};

#[derive(Debug, Clone, Deserialize, Default)]
pub(super) struct GatewaySessionsListQuery {
    #[serde(default)]
    limit: Option<usize>,
}

pub(super) async fn handle_gateway_sessions_list(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    Query(query): Query<GatewaySessionsListQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let sessions_root = state
        .config
        .state_dir
        .join("openresponses")
        .join("sessions");
    let mut entries = Vec::<(u64, Value)>::new();

    if sessions_root.is_dir() {
        let dir_entries = match std::fs::read_dir(&sessions_root) {
            Ok(entries) => entries,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to list sessions directory {}: {error}",
                    sessions_root.display()
                ))
                .into_response();
            }
        };

        for dir_entry in dir_entries.flatten() {
            let path = dir_entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(file_stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let session_key = sanitize_session_key(file_stem);
            let metadata = match std::fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            let modified_unix_ms = metadata
                .modified()
                .ok()
                .and_then(system_time_to_unix_ms)
                .unwrap_or(0);
            let bytes = metadata.len();
            let message_count = std::fs::read_to_string(&path)
                .ok()
                .map(|payload| {
                    payload
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .count()
                })
                .unwrap_or(0);
            entries.push((
                modified_unix_ms,
                json!({
                    "session_key": session_key,
                    "path": path.display().to_string(),
                    "modified_unix_ms": modified_unix_ms,
                    "bytes": bytes,
                    "message_count": message_count,
                }),
            ));
        }
    }

    entries.sort_by(|left, right| right.0.cmp(&left.0));
    entries.truncate(limit);
    let sessions = entries.into_iter().map(|entry| entry.1).collect::<Vec<_>>();

    state.record_ui_telemetry_event("sessions", "list", "session_list_requested");
    (
        StatusCode::OK,
        Json(json!({
            "sessions": sessions,
            "limit": limit,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_session_detail(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    if !session_path.exists() {
        return OpenResponsesApiError::not_found(
            "session_not_found",
            format!("session '{session_key}' does not exist"),
        )
        .into_response();
    }

    let store = match SessionStore::load(&session_path) {
        Ok(store) => store,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to load session '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
    };
    let entries = store
        .entries()
        .iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "parent_id": entry.parent_id,
                "role": entry.message.role,
                "text": entry.message.text_content(),
                "message": entry.message,
            })
        })
        .collect::<Vec<_>>();

    state.record_ui_telemetry_event("sessions", "detail", "session_detail_requested");
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "path": session_path.display().to_string(),
            "entry_count": entries.len(),
            "head_id": store.head_id(),
            "entries": entries,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_session_append(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewaySessionAppendRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), SESSION_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event("sessions", "append", "session_append_policy_gate_blocked");
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let content = request.content.trim();
    if content.is_empty() {
        return OpenResponsesApiError::bad_request("invalid_content", "content must be non-empty")
            .into_response();
    }
    let role = match parse_message_role(request.role.as_str()) {
        Ok(role) => role,
        Err(error) => return error.into_response(),
    };

    let message = build_manual_session_message(role, content);
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    let mut store = match SessionStore::load(&session_path) {
        Ok(store) => store,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to load session '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
    };
    store.set_lock_policy(
        state.config.session_lock_wait_ms,
        state.config.session_lock_stale_ms,
    );
    let resolved_system_prompt = state.resolved_system_prompt();
    if let Err(error) = store.ensure_initialized(&resolved_system_prompt) {
        return OpenResponsesApiError::internal(format!(
            "failed to initialize session '{}': {error}",
            session_path.display()
        ))
        .into_response();
    }
    let parent_id = store.head_id();
    let new_head = match store.append_messages(parent_id, &[message]) {
        Ok(head) => head,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to append session message '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
    };

    state.record_ui_telemetry_event("sessions", "append", "session_message_appended");
    record_cortex_session_append_event(
        &state.config.state_dir,
        session_key.as_str(),
        new_head,
        store.entries().len(),
    );
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "path": session_path.display().to_string(),
            "entry_count": store.entries().len(),
            "head_id": new_head,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_session_reset(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewaySessionResetRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), SESSION_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event("sessions", "reset", "session_reset_policy_gate_blocked");
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    let lock_path = session_path.with_extension("lock");
    let mut reset = false;

    if session_path.exists() {
        if let Err(error) = std::fs::remove_file(&session_path) {
            return OpenResponsesApiError::internal(format!(
                "failed to remove session '{}': {error}",
                session_path.display()
            ))
            .into_response();
        }
        reset = true;
    }
    if lock_path.exists() {
        let _ = std::fs::remove_file(&lock_path);
    }

    state.record_ui_telemetry_event("sessions", "reset", "session_reset_applied");
    record_cortex_session_reset_event(&state.config.state_dir, session_key.as_str(), reset);
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "reset": reset,
        })),
    )
        .into_response()
}

fn parse_message_role(raw: &str) -> Result<MessageRole, OpenResponsesApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "system" => Ok(MessageRole::System),
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "tool" => Ok(MessageRole::Tool),
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_role",
            "role must be one of: system, user, assistant, tool",
        )),
    }
}

fn build_manual_session_message(role: MessageRole, content: &str) -> Message {
    match role {
        MessageRole::System => Message::system(content),
        MessageRole::User => Message::user(content),
        MessageRole::Assistant => Message::assistant_text(content),
        MessageRole::Tool => Message::tool_result("manual", "manual", content, false),
    }
}
