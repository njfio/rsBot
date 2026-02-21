use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde_json::{json, Value};
use tau_core::current_unix_timestamp_ms;
use tau_runtime::{
    ExternalCodingAgentBridgeError, ExternalCodingAgentSessionSnapshot,
    ExternalCodingAgentSessionStatus,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::types::{
    GatewayExternalCodingAgentFollowupsDrainRequest, GatewayExternalCodingAgentMessageRequest,
    GatewayExternalCodingAgentReapRequest, GatewayExternalCodingAgentSessionOpenRequest,
    GatewayExternalCodingAgentStreamQuery,
};
use super::{
    authorize_and_enforce_gateway_limits, parse_gateway_json_body,
    record_cortex_external_followup_event, record_cortex_external_progress_event,
    record_cortex_external_session_closed, record_cortex_external_session_opened,
    validate_gateway_request_body_size, GatewayOpenResponsesServerState, OpenResponsesApiError,
    SseFrame,
};

pub(super) async fn handle_external_coding_agent_open_session(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        return error.into_response();
    }
    let request =
        match parse_gateway_json_body::<GatewayExternalCodingAgentSessionOpenRequest>(&body) {
            Ok(request) => request,
            Err(error) => return error.into_response(),
        };
    state
        .external_coding_agent_bridge
        .reap_inactive_sessions(current_unix_timestamp_ms());
    let snapshot = match state
        .external_coding_agent_bridge
        .open_or_reuse_session(request.workspace_id.as_str())
    {
        Ok(snapshot) => snapshot,
        Err(error) => return map_external_coding_agent_bridge_error(error).into_response(),
    };
    record_cortex_external_session_opened(
        &state.config.state_dir,
        snapshot.session_id.as_str(),
        snapshot.workspace_id.as_str(),
        external_coding_agent_status_label(snapshot.status),
    );
    (
        StatusCode::OK,
        Json(json!({
            "session": external_coding_agent_session_json(&snapshot),
        })),
    )
        .into_response()
}

pub(super) async fn handle_external_coding_agent_session_detail(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    state
        .external_coding_agent_bridge
        .reap_inactive_sessions(current_unix_timestamp_ms());
    let Some(snapshot) = state
        .external_coding_agent_bridge
        .snapshot(session_id.as_str())
    else {
        return OpenResponsesApiError::not_found(
            "external_coding_agent_session_not_found",
            format!("session '{session_id}' was not found"),
        )
        .into_response();
    };
    (
        StatusCode::OK,
        Json(json!({ "session": external_coding_agent_session_json(&snapshot) })),
    )
        .into_response()
}

pub(super) async fn handle_external_coding_agent_session_progress(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayExternalCodingAgentMessageRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let event = match state
        .external_coding_agent_bridge
        .append_progress(session_id.as_str(), request.message.as_str())
    {
        Ok(event) => event,
        Err(error) => return map_external_coding_agent_bridge_error(error).into_response(),
    };
    record_cortex_external_progress_event(
        &state.config.state_dir,
        session_id.as_str(),
        event.sequence_id,
        event.message.as_str(),
    );
    let session = state
        .external_coding_agent_bridge
        .snapshot(session_id.as_str())
        .map(|snapshot| external_coding_agent_session_json(&snapshot))
        .unwrap_or_else(|| Value::Null);
    (
        StatusCode::OK,
        Json(json!({
            "event": external_coding_agent_event_json(&event),
            "session": session,
        })),
    )
        .into_response()
}

pub(super) async fn handle_external_coding_agent_session_followup(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayExternalCodingAgentMessageRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let event = match state
        .external_coding_agent_bridge
        .queue_followup(session_id.as_str(), request.message.as_str())
    {
        Ok(event) => event,
        Err(error) => return map_external_coding_agent_bridge_error(error).into_response(),
    };
    record_cortex_external_followup_event(
        &state.config.state_dir,
        session_id.as_str(),
        event.sequence_id,
        event.message.as_str(),
    );
    let session = state
        .external_coding_agent_bridge
        .snapshot(session_id.as_str())
        .map(|snapshot| external_coding_agent_session_json(&snapshot))
        .unwrap_or_else(|| Value::Null);
    (
        StatusCode::OK,
        Json(json!({
            "event": external_coding_agent_event_json(&event),
            "session": session,
        })),
    )
        .into_response()
}

pub(super) async fn handle_external_coding_agent_session_followups_drain(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        return error.into_response();
    }
    let request = if body.is_empty() {
        GatewayExternalCodingAgentFollowupsDrainRequest::default()
    } else {
        match parse_gateway_json_body::<GatewayExternalCodingAgentFollowupsDrainRequest>(&body) {
            Ok(request) => request,
            Err(error) => return error.into_response(),
        }
    };
    let limit = request.limit.unwrap_or(64).max(1);
    let followups = match state
        .external_coding_agent_bridge
        .take_followups(session_id.as_str(), limit)
    {
        Ok(followups) => followups,
        Err(error) => return map_external_coding_agent_bridge_error(error).into_response(),
    };
    let session = state
        .external_coding_agent_bridge
        .snapshot(session_id.as_str())
        .map(|snapshot| external_coding_agent_session_json(&snapshot))
        .unwrap_or_else(|| Value::Null);
    (
        StatusCode::OK,
        Json(json!({
            "session_id": session_id,
            "drained_count": followups.len(),
            "followups": followups,
            "session": session,
        })),
    )
        .into_response()
}

pub(super) async fn handle_external_coding_agent_session_stream(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<GatewayExternalCodingAgentStreamQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    state
        .external_coding_agent_bridge
        .reap_inactive_sessions(current_unix_timestamp_ms());
    let Some(snapshot) = state
        .external_coding_agent_bridge
        .snapshot(session_id.as_str())
    else {
        return OpenResponsesApiError::not_found(
            "external_coding_agent_session_not_found",
            format!("session '{session_id}' was not found"),
        )
        .into_response();
    };
    let replay_from_header = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok());
    let after_sequence_id = query.after_sequence_id.or(replay_from_header);
    let limit = query
        .limit
        .unwrap_or(
            state
                .config
                .external_coding_agent_bridge
                .max_events_per_session,
        )
        .max(1);
    let events = match state.external_coding_agent_bridge.poll_events(
        session_id.as_str(),
        after_sequence_id,
        limit,
    ) {
        Ok(events) => events,
        Err(error) => return map_external_coding_agent_bridge_error(error).into_response(),
    };

    let (tx, rx) = mpsc::unbounded_channel::<SseFrame>();
    let _ = tx.send(SseFrame::Json {
        event: "external_coding_agent.snapshot",
        payload: json!({
            "session": external_coding_agent_session_json(&snapshot),
            "replay_after_sequence_id": after_sequence_id,
        }),
    });
    for event in events {
        let _ = tx.send(SseFrame::Json {
            event: "external_coding_agent.progress",
            payload: external_coding_agent_event_json(&event),
        });
    }
    let _ = tx.send(SseFrame::Done);
    drop(tx);

    let stream =
        UnboundedReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

pub(super) async fn handle_external_coding_agent_session_close(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let snapshot = match state
        .external_coding_agent_bridge
        .close_session(session_id.as_str())
    {
        Ok(snapshot) => snapshot,
        Err(error) => return map_external_coding_agent_bridge_error(error).into_response(),
    };
    record_cortex_external_session_closed(
        &state.config.state_dir,
        snapshot.session_id.as_str(),
        snapshot.workspace_id.as_str(),
        external_coding_agent_status_label(snapshot.status),
    );
    (
        StatusCode::OK,
        Json(json!({
            "session": external_coding_agent_session_json(&snapshot),
        })),
    )
        .into_response()
}

pub(super) async fn handle_external_coding_agent_reap(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        return error.into_response();
    }
    let request = if body.is_empty() {
        GatewayExternalCodingAgentReapRequest::default()
    } else {
        match parse_gateway_json_body::<GatewayExternalCodingAgentReapRequest>(&body) {
            Ok(request) => request,
            Err(error) => return error.into_response(),
        }
    };
    let now_unix_ms = request
        .now_unix_ms
        .unwrap_or_else(current_unix_timestamp_ms);
    let sessions = state
        .external_coding_agent_bridge
        .reap_inactive_sessions(now_unix_ms)
        .into_iter()
        .map(|snapshot| external_coding_agent_session_json(&snapshot))
        .collect::<Vec<_>>();
    (
        StatusCode::OK,
        Json(json!({
            "reaped_count": sessions.len(),
            "sessions": sessions,
            "runtime": {
                "active_sessions": state.external_coding_agent_bridge.active_session_count(),
                "inactivity_timeout_ms": state.config.external_coding_agent_bridge.inactivity_timeout_ms,
                "max_active_sessions": state.config.external_coding_agent_bridge.max_active_sessions,
                "max_events_per_session": state.config.external_coding_agent_bridge.max_events_per_session,
            }
        })),
    )
        .into_response()
}

fn map_external_coding_agent_bridge_error(
    error: ExternalCodingAgentBridgeError,
) -> OpenResponsesApiError {
    match error {
        ExternalCodingAgentBridgeError::InvalidWorkspaceId => OpenResponsesApiError::bad_request(
            "invalid_workspace_id",
            "workspace_id must be non-empty",
        ),
        ExternalCodingAgentBridgeError::InvalidMessage => {
            OpenResponsesApiError::bad_request("invalid_message", "message must be non-empty")
        }
        ExternalCodingAgentBridgeError::InvalidSubprocessConfig(message) => {
            OpenResponsesApiError::internal(format!(
                "external coding-agent subprocess configuration is invalid: {message}"
            ))
        }
        ExternalCodingAgentBridgeError::SessionNotFound(session_id) => {
            OpenResponsesApiError::not_found(
                "external_coding_agent_session_not_found",
                format!("session '{session_id}' was not found"),
            )
        }
        ExternalCodingAgentBridgeError::SessionLimitReached { limit } => {
            OpenResponsesApiError::new(
                StatusCode::CONFLICT,
                "external_coding_agent_session_limit_reached",
                format!("max active sessions limit reached ({limit})"),
            )
        }
        ExternalCodingAgentBridgeError::SubprocessSpawnFailed {
            workspace_id,
            error,
        } => OpenResponsesApiError::gateway_failure(format!(
            "failed to start external coding-agent worker for workspace '{workspace_id}': {error}"
        )),
        ExternalCodingAgentBridgeError::SubprocessIoError { session_id, error } => {
            OpenResponsesApiError::gateway_failure(format!(
                "external coding-agent worker I/O failed for session '{session_id}': {error}"
            ))
        }
    }
}

fn external_coding_agent_status_label(status: ExternalCodingAgentSessionStatus) -> &'static str {
    match status {
        ExternalCodingAgentSessionStatus::Running => "running",
        ExternalCodingAgentSessionStatus::Completed => "completed",
        ExternalCodingAgentSessionStatus::Failed => "failed",
        ExternalCodingAgentSessionStatus::TimedOut => "timed_out",
        ExternalCodingAgentSessionStatus::Closed => "closed",
    }
}

fn external_coding_agent_session_json(snapshot: &ExternalCodingAgentSessionSnapshot) -> Value {
    json!({
        "session_id": snapshot.session_id,
        "workspace_id": snapshot.workspace_id,
        "status": external_coding_agent_status_label(snapshot.status),
        "started_unix_ms": snapshot.started_unix_ms,
        "last_activity_unix_ms": snapshot.last_activity_unix_ms,
        "queued_followups": snapshot.queued_followups,
    })
}

fn external_coding_agent_event_json(
    event: &tau_runtime::ExternalCodingAgentProgressEvent,
) -> Value {
    json!({
        "sequence_id": event.sequence_id,
        "event_type": event.event_type,
        "message": event.message,
        "timestamp_unix_ms": event.timestamp_unix_ms,
    })
}
