//! Gateway jobs list/cancel endpoint handlers.
//!
//! This module maps external coding-agent bridge sessions to dashboard-facing
//! jobs contracts used by the Tau ops dashboard.

use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::json;
use tau_core::current_unix_timestamp_ms;
use tau_runtime::{ExternalCodingAgentBridgeError, ExternalCodingAgentSessionStatus};

use super::{authorize_dashboard_request, GatewayOpenResponsesServerState, OpenResponsesApiError};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayJobView {
    job_id: String,
    workspace_id: String,
    status: String,
    started_unix_ms: u64,
    last_activity_unix_ms: u64,
    queued_followups: usize,
}

pub(super) async fn handle_gateway_jobs_list(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }

    let jobs = state
        .external_coding_agent_bridge
        .list_sessions()
        .into_iter()
        .map(|snapshot| GatewayJobView {
            job_id: snapshot.session_id,
            workspace_id: snapshot.workspace_id,
            status: job_status_label(snapshot.status).to_string(),
            started_unix_ms: snapshot.started_unix_ms,
            last_activity_unix_ms: snapshot.last_activity_unix_ms,
            queued_followups: snapshot.queued_followups,
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "generated_unix_ms": current_unix_timestamp_ms(),
            "total_jobs": jobs.len(),
            "jobs": jobs,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_job_cancel(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(job_id): AxumPath<String>,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let normalized_job_id = job_id.trim();
    if normalized_job_id.is_empty() {
        return OpenResponsesApiError::bad_request("invalid_job_id", "job_id must be non-empty")
            .into_response();
    }

    let snapshot = match state
        .external_coding_agent_bridge
        .close_session(normalized_job_id)
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return map_job_cancel_error(error, normalized_job_id).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "job_id": snapshot.session_id,
            "status": "cancelled",
            "cancelled_unix_ms": current_unix_timestamp_ms(),
        })),
    )
        .into_response()
}

fn map_job_cancel_error(
    error: ExternalCodingAgentBridgeError,
    job_id: &str,
) -> OpenResponsesApiError {
    match error {
        ExternalCodingAgentBridgeError::SessionNotFound(_) => OpenResponsesApiError::not_found(
            "job_not_found",
            format!("job '{job_id}' was not found"),
        ),
        ExternalCodingAgentBridgeError::InvalidWorkspaceId => {
            OpenResponsesApiError::bad_request("invalid_job_id", "job_id must be non-empty")
        }
        ExternalCodingAgentBridgeError::InvalidMessage => {
            OpenResponsesApiError::internal("unexpected bridge message validation failure")
        }
        ExternalCodingAgentBridgeError::InvalidSubprocessConfig(message) => {
            OpenResponsesApiError::internal(format!(
                "external coding-agent subprocess configuration is invalid: {message}"
            ))
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

fn job_status_label(status: ExternalCodingAgentSessionStatus) -> &'static str {
    match status {
        ExternalCodingAgentSessionStatus::Running => "running",
        ExternalCodingAgentSessionStatus::Completed => "completed",
        ExternalCodingAgentSessionStatus::Failed => "failed",
        ExternalCodingAgentSessionStatus::TimedOut => "timed_out",
        ExternalCodingAgentSessionStatus::Closed => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_job_status_label_maps_all_runtime_statuses() {
        assert_eq!(
            job_status_label(ExternalCodingAgentSessionStatus::Running),
            "running"
        );
        assert_eq!(
            job_status_label(ExternalCodingAgentSessionStatus::Completed),
            "completed"
        );
        assert_eq!(
            job_status_label(ExternalCodingAgentSessionStatus::Failed),
            "failed"
        );
        assert_eq!(
            job_status_label(ExternalCodingAgentSessionStatus::TimedOut),
            "timed_out"
        );
        assert_eq!(
            job_status_label(ExternalCodingAgentSessionStatus::Closed),
            "cancelled"
        );
    }
}
