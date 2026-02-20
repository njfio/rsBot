use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{
    apply_gateway_dashboard_action, authorize_gateway_request, collect_gateway_dashboard_snapshot,
    enforce_gateway_rate_limit, run_dashboard_stream_loop, GatewayDashboardActionRequest,
    GatewayOpenResponsesServerState, OpenResponsesApiError,
};

pub(super) fn authorize_dashboard_request(
    state: &Arc<GatewayOpenResponsesServerState>,
    headers: &HeaderMap,
) -> Result<String, OpenResponsesApiError> {
    let principal = authorize_gateway_request(state, headers)?;
    enforce_gateway_rate_limit(state, principal.as_str())?;
    Ok(principal)
}

pub(super) async fn handle_dashboard_health(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": snapshot.schema_version,
            "generated_unix_ms": snapshot.generated_unix_ms,
            "health": snapshot.health,
            "training": snapshot.training,
            "control": snapshot.control,
            "state": snapshot.state,
        })),
    )
        .into_response()
}

pub(super) async fn handle_dashboard_widgets(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": snapshot.schema_version,
            "generated_unix_ms": snapshot.generated_unix_ms,
            "widgets": snapshot.widgets,
            "training": snapshot.training,
            "state": snapshot.state,
        })),
    )
        .into_response()
}

pub(super) async fn handle_dashboard_queue_timeline(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": snapshot.schema_version,
            "generated_unix_ms": snapshot.generated_unix_ms,
            "queue_timeline": snapshot.queue_timeline,
            "health": snapshot.health,
            "training": snapshot.training,
            "state": snapshot.state,
        })),
    )
        .into_response()
}

pub(super) async fn handle_dashboard_alerts(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": snapshot.schema_version,
            "generated_unix_ms": snapshot.generated_unix_ms,
            "alerts": snapshot.alerts,
            "health": snapshot.health,
            "training": snapshot.training,
            "state": snapshot.state,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_training_status(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    (
        StatusCode::OK,
        Json(json!({
            "schema_version": snapshot.schema_version,
            "generated_unix_ms": snapshot.generated_unix_ms,
            "training": snapshot.training,
        })),
    )
        .into_response()
}

pub(super) async fn handle_dashboard_action(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let principal = match authorize_dashboard_request(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };

    let request = match serde_json::from_slice::<GatewayDashboardActionRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return OpenResponsesApiError::bad_request(
                "malformed_json",
                format!("failed to parse request body: {error}"),
            )
            .into_response();
        }
    };

    match apply_gateway_dashboard_action(&state.config.state_dir, principal.as_str(), request) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(error) => error.into_response(),
    }
}

pub(super) async fn handle_dashboard_stream(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let reconnect_event_id = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let (tx, rx) = mpsc::unbounded_channel::<Event>();
    tokio::spawn(run_dashboard_stream_loop(state, tx, reconnect_event_id));
    let stream = UnboundedReceiverStream::new(rx).map(Ok::<Event, Infallible>);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
