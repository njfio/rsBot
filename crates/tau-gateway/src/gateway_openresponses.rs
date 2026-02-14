use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::body::Bytes;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, StreamDeltaHandler};
use tau_core::{current_unix_timestamp, current_unix_timestamp_ms};
use tau_runtime::TransportHealthSnapshot;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::remote_profile::GatewayOpenResponsesAuthMode;

mod auth_runtime;
mod dashboard_status;
mod multi_channel_status;
mod request_translation;
mod session_runtime;
#[cfg(test)]
mod tests;
mod types;
mod webchat_page;
mod websocket;

use auth_runtime::{
    authorize_gateway_request, collect_gateway_auth_status_report, enforce_gateway_rate_limit,
    issue_gateway_session_token,
};
use dashboard_status::{
    apply_gateway_dashboard_action, collect_gateway_dashboard_snapshot,
    GatewayDashboardActionRequest,
};
use multi_channel_status::collect_gateway_multi_channel_status_report;
use request_translation::{sanitize_session_key, translate_openresponses_request};
use session_runtime::{
    collect_assistant_reply, gateway_session_path, initialize_gateway_session_runtime,
    persist_messages,
};
use types::{
    GatewayAuthSessionRequest, GatewayAuthSessionResponse, OpenResponsesApiError,
    OpenResponsesExecutionResult, OpenResponsesOutputItem, OpenResponsesOutputTextItem,
    OpenResponsesPrompt, OpenResponsesRequest, OpenResponsesResponse, OpenResponsesUsage,
    OpenResponsesUsageSummary, SseFrame,
};
use webchat_page::render_gateway_webchat_page;
use websocket::run_gateway_ws_connection;

const OPENRESPONSES_ENDPOINT: &str = "/v1/responses";
const WEBCHAT_ENDPOINT: &str = "/webchat";
const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
const GATEWAY_WS_ENDPOINT: &str = "/gateway/ws";
const GATEWAY_AUTH_SESSION_ENDPOINT: &str = "/gateway/auth/session";
const DASHBOARD_HEALTH_ENDPOINT: &str = "/dashboard/health";
const DASHBOARD_WIDGETS_ENDPOINT: &str = "/dashboard/widgets";
const DASHBOARD_QUEUE_TIMELINE_ENDPOINT: &str = "/dashboard/queue-timeline";
const DASHBOARD_ALERTS_ENDPOINT: &str = "/dashboard/alerts";
const DASHBOARD_ACTIONS_ENDPOINT: &str = "/dashboard/actions";
const DASHBOARD_STREAM_ENDPOINT: &str = "/dashboard/stream";
const DEFAULT_SESSION_KEY: &str = "default";
const INPUT_BODY_SIZE_MULTIPLIER: usize = 8;
const GATEWAY_WS_HEARTBEAT_REQUEST_ID: &str = "gateway-heartbeat";

/// Trait contract for `GatewayToolRegistrar` behavior.
pub trait GatewayToolRegistrar: Send + Sync {
    fn register(&self, agent: &mut Agent);
}

#[derive(Clone, Default)]
/// Public struct `NoopGatewayToolRegistrar` used across Tau components.
pub struct NoopGatewayToolRegistrar;

impl GatewayToolRegistrar for NoopGatewayToolRegistrar {
    fn register(&self, _agent: &mut Agent) {}
}

#[derive(Clone)]
/// Public struct `GatewayToolRegistrarFn` used across Tau components.
pub struct GatewayToolRegistrarFn {
    inner: Arc<dyn Fn(&mut Agent) + Send + Sync>,
}

impl GatewayToolRegistrarFn {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(&mut Agent) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(handler),
        }
    }
}

impl GatewayToolRegistrar for GatewayToolRegistrarFn {
    fn register(&self, agent: &mut Agent) {
        (self.inner)(agent);
    }
}

#[derive(Clone)]
/// Public struct `GatewayOpenResponsesServerConfig` used across Tau components.
pub struct GatewayOpenResponsesServerConfig {
    pub client: Arc<dyn LlmClient>,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub tool_registrar: Arc<dyn GatewayToolRegistrar>,
    pub turn_timeout_ms: u64,
    pub session_lock_wait_ms: u64,
    pub session_lock_stale_ms: u64,
    pub state_dir: PathBuf,
    pub bind: String,
    pub auth_mode: GatewayOpenResponsesAuthMode,
    pub auth_token: Option<String>,
    pub auth_password: Option<String>,
    pub session_ttl_seconds: u64,
    pub rate_limit_window_seconds: u64,
    pub rate_limit_max_requests: usize,
    pub max_input_chars: usize,
}

#[derive(Clone)]
struct GatewayOpenResponsesServerState {
    config: GatewayOpenResponsesServerConfig,
    response_sequence: Arc<AtomicU64>,
    auth_runtime: Arc<Mutex<GatewayAuthRuntimeState>>,
}

impl GatewayOpenResponsesServerState {
    fn new(config: GatewayOpenResponsesServerConfig) -> Self {
        Self {
            config,
            response_sequence: Arc::new(AtomicU64::new(0)),
            auth_runtime: Arc::new(Mutex::new(GatewayAuthRuntimeState::default())),
        }
    }

    fn next_sequence(&self) -> u64 {
        self.response_sequence.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn next_response_id(&self) -> String {
        format!("resp_{:016x}", self.next_sequence())
    }

    fn next_output_message_id(&self) -> String {
        format!("msg_{:016x}", self.next_sequence())
    }
}

#[derive(Debug, Clone, Default)]
struct GatewayAuthRuntimeState {
    sessions: BTreeMap<String, GatewaySessionTokenState>,
    total_sessions_issued: u64,
    auth_failures: u64,
    rate_limited_requests: u64,
    rate_limit_buckets: BTreeMap<String, GatewayRateLimitBucket>,
}

#[derive(Debug, Clone)]
struct GatewaySessionTokenState {
    expires_unix_ms: u64,
    last_seen_unix_ms: u64,
    request_count: u64,
}

#[derive(Debug, Clone, Default)]
struct GatewayRateLimitBucket {
    window_started_unix_ms: u64,
    accepted_requests: usize,
    rejected_requests: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayAuthStatusReport {
    mode: String,
    session_ttl_seconds: u64,
    active_sessions: usize,
    total_sessions_issued: u64,
    auth_failures: u64,
    rate_limited_requests: u64,
    rate_limit_window_seconds: u64,
    rate_limit_max_requests: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayMultiChannelStatusReport {
    state_present: bool,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    processed_event_count: usize,
    transport_counts: BTreeMap<String, usize>,
    queue_depth: usize,
    failure_streak: usize,
    last_cycle_failed: usize,
    last_cycle_completed: usize,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    connectors: GatewayMultiChannelConnectorsStatusReport,
    diagnostics: Vec<String>,
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
struct GatewayMultiChannelConnectorsStatusReport {
    state_present: bool,
    processed_event_count: usize,
    channels: BTreeMap<String, GatewayMultiChannelConnectorChannelSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
struct GatewayMultiChannelConnectorChannelSummary {
    mode: String,
    liveness: String,
    breaker_state: String,
    events_ingested: u64,
    duplicates_skipped: u64,
    retry_attempts: u64,
    auth_failures: u64,
    parse_failures: u64,
    provider_failures: u64,
    consecutive_failures: u64,
    retry_budget_remaining: u64,
    breaker_open_until_unix_ms: u64,
    breaker_last_open_reason: String,
    breaker_open_count: u64,
    last_error_code: String,
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

pub async fn run_gateway_openresponses_server(
    config: GatewayOpenResponsesServerConfig,
) -> Result<()> {
    std::fs::create_dir_all(&config.state_dir)
        .with_context(|| format!("failed to create {}", config.state_dir.display()))?;

    let bind_addr = config
        .bind
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid --gateway-openresponses-bind '{}'", config.bind))?;

    let service_report = crate::gateway_runtime::start_gateway_service_mode(&config.state_dir)?;
    println!(
        "{}",
        crate::gateway_runtime::render_gateway_service_status_report(&service_report)
    );

    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind gateway openresponses server on {bind_addr}"))?;
    let local_addr = listener
        .local_addr()
        .context("failed to resolve bound openresponses server address")?;

    println!(
        "gateway openresponses server listening: endpoint={} addr={} state_dir={}",
        OPENRESPONSES_ENDPOINT,
        local_addr,
        config.state_dir.display()
    );

    let state_dir = config.state_dir.clone();
    let state = Arc::new(GatewayOpenResponsesServerState::new(config));
    let app = build_gateway_openresponses_router(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("gateway openresponses server exited unexpectedly")?;

    let stop_report = crate::gateway_runtime::stop_gateway_service_mode(
        &state_dir,
        Some("openresponses_server_shutdown"),
    );
    if let Ok(report) = stop_report {
        println!(
            "{}",
            crate::gateway_runtime::render_gateway_service_status_report(&report)
        );
    }

    Ok(())
}

fn build_gateway_openresponses_router(state: Arc<GatewayOpenResponsesServerState>) -> Router {
    Router::new()
        .route(OPENRESPONSES_ENDPOINT, post(handle_openresponses))
        .route(
            GATEWAY_AUTH_SESSION_ENDPOINT,
            post(handle_gateway_auth_session),
        )
        .route(WEBCHAT_ENDPOINT, get(handle_webchat_page))
        .route(GATEWAY_STATUS_ENDPOINT, get(handle_gateway_status))
        .route(DASHBOARD_HEALTH_ENDPOINT, get(handle_dashboard_health))
        .route(DASHBOARD_WIDGETS_ENDPOINT, get(handle_dashboard_widgets))
        .route(
            DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
            get(handle_dashboard_queue_timeline),
        )
        .route(DASHBOARD_ALERTS_ENDPOINT, get(handle_dashboard_alerts))
        .route(DASHBOARD_ACTIONS_ENDPOINT, post(handle_dashboard_action))
        .route(DASHBOARD_STREAM_ENDPOINT, get(handle_dashboard_stream))
        .route(GATEWAY_WS_ENDPOINT, get(handle_gateway_ws_upgrade))
        .with_state(state)
}

async fn handle_webchat_page() -> Html<String> {
    Html(render_gateway_webchat_page())
}

async fn handle_gateway_status(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    let principal = match authorize_gateway_request(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = enforce_gateway_rate_limit(&state, principal.as_str()) {
        return error.into_response();
    }

    let service_report =
        match crate::gateway_runtime::inspect_gateway_service_mode(&state.config.state_dir) {
            Ok(report) => report,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to inspect gateway service state: {error}"
                ))
                .into_response();
            }
        };
    let multi_channel_report = collect_gateway_multi_channel_status_report(&state.config.state_dir);

    (
        StatusCode::OK,
        Json(json!({
            "service": service_report,
            "auth": collect_gateway_auth_status_report(&state),
            "multi_channel": multi_channel_report,
            "gateway": {
                "responses_endpoint": OPENRESPONSES_ENDPOINT,
                "webchat_endpoint": WEBCHAT_ENDPOINT,
                "auth_session_endpoint": GATEWAY_AUTH_SESSION_ENDPOINT,
                "status_endpoint": GATEWAY_STATUS_ENDPOINT,
                "ws_endpoint": GATEWAY_WS_ENDPOINT,
                "dashboard": {
                    "health_endpoint": DASHBOARD_HEALTH_ENDPOINT,
                    "widgets_endpoint": DASHBOARD_WIDGETS_ENDPOINT,
                    "queue_timeline_endpoint": DASHBOARD_QUEUE_TIMELINE_ENDPOINT,
                    "alerts_endpoint": DASHBOARD_ALERTS_ENDPOINT,
                    "actions_endpoint": DASHBOARD_ACTIONS_ENDPOINT,
                    "stream_endpoint": DASHBOARD_STREAM_ENDPOINT,
                },
                "state_dir": state.config.state_dir.display().to_string(),
                "model": state.config.model,
            }
        })),
    )
        .into_response()
}

fn authorize_dashboard_request(
    state: &Arc<GatewayOpenResponsesServerState>,
    headers: &HeaderMap,
) -> Result<String, OpenResponsesApiError> {
    let principal = authorize_gateway_request(state, headers)?;
    enforce_gateway_rate_limit(state, principal.as_str())?;
    Ok(principal)
}

async fn handle_dashboard_health(
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
            "control": snapshot.control,
            "state": snapshot.state,
        })),
    )
        .into_response()
}

async fn handle_dashboard_widgets(
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
            "state": snapshot.state,
        })),
    )
        .into_response()
}

async fn handle_dashboard_queue_timeline(
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
            "state": snapshot.state,
        })),
    )
        .into_response()
}

async fn handle_dashboard_alerts(
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
            "state": snapshot.state,
        })),
    )
        .into_response()
}

async fn handle_dashboard_action(
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

async fn handle_dashboard_stream(
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

async fn handle_openresponses(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let principal = match authorize_gateway_request(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = enforce_gateway_rate_limit(&state, principal.as_str()) {
        return error.into_response();
    }

    let body_limit = state
        .config
        .max_input_chars
        .saturating_mul(INPUT_BODY_SIZE_MULTIPLIER)
        .max(state.config.max_input_chars);
    if body.len() > body_limit {
        return OpenResponsesApiError::payload_too_large(format!(
            "request body exceeds max size of {} bytes",
            body_limit
        ))
        .into_response();
    }

    let request = match serde_json::from_slice::<OpenResponsesRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return OpenResponsesApiError::bad_request(
                "malformed_json",
                format!("failed to parse request body: {error}"),
            )
            .into_response();
        }
    };

    if request.stream {
        return stream_openresponses(state, request).await;
    }

    match execute_openresponses_request(state, request, None).await {
        Ok(result) => (StatusCode::OK, Json(result.response)).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_gateway_auth_session(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    body: Bytes,
) -> Response {
    if state.config.auth_mode != GatewayOpenResponsesAuthMode::PasswordSession {
        return OpenResponsesApiError::bad_request(
            "auth_mode_mismatch",
            "gateway auth session endpoint requires --gateway-openresponses-auth-mode=password-session",
        )
        .into_response();
    }
    if let Err(error) = enforce_gateway_rate_limit(&state, "auth_session_issue") {
        return error.into_response();
    }
    let request = match serde_json::from_slice::<GatewayAuthSessionRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return OpenResponsesApiError::bad_request(
                "malformed_json",
                format!("failed to parse request body: {error}"),
            )
            .into_response();
        }
    };

    match issue_gateway_session_token(&state, request.password.as_str()) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_gateway_ws_upgrade(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    websocket: WebSocketUpgrade,
) -> Response {
    let principal = match authorize_gateway_request(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    if let Err(error) = enforce_gateway_rate_limit(&state, principal.as_str()) {
        return error.into_response();
    }

    websocket
        .on_upgrade(move |socket| run_gateway_ws_connection(state, socket, principal))
        .into_response()
}

async fn run_dashboard_stream_loop(
    state: Arc<GatewayOpenResponsesServerState>,
    sender: mpsc::UnboundedSender<Event>,
    reconnect_event_id: Option<String>,
) {
    if let Some(last_event_id) = reconnect_event_id {
        let reset_payload = json!({
            "schema_version": 1,
            "reset": true,
            "last_event_id": last_event_id,
            "reason": "history_not_retained_request_full_snapshot",
        });
        let reset = Event::default()
            .id(format!("dashboard-{}", state.next_sequence()))
            .event("dashboard.reset")
            .data(reset_payload.to_string());
        if sender.send(reset).is_err() {
            return;
        }
    }

    let mut last_snapshot_payload = String::new();
    loop {
        let snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
        let payload_value = match serde_json::to_value(&snapshot) {
            Ok(payload) => payload,
            Err(error) => json!({
                "schema_version": 1,
                "generated_unix_ms": current_unix_timestamp_ms(),
                "error": "dashboard_snapshot_serialize_failed",
                "message": error.to_string(),
            }),
        };
        let payload_string = payload_value.to_string();
        if payload_string != last_snapshot_payload {
            let snapshot_event = Event::default()
                .id(format!("dashboard-{}", state.next_sequence()))
                .event("dashboard.snapshot")
                .data(payload_string.clone());
            if sender.send(snapshot_event).is_err() {
                return;
            }
            last_snapshot_payload = payload_string;
        }
        tokio::time::sleep(Duration::from_millis(750)).await;
    }
}

async fn stream_openresponses(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
) -> Response {
    let (tx, rx) = mpsc::unbounded_channel::<SseFrame>();
    tokio::spawn(async move {
        match execute_openresponses_request(state, request, Some(tx.clone())).await {
            Ok(result) => {
                let response = result.response;
                let _ = tx.send(SseFrame::Json {
                    event: "response.output_text.done",
                    payload: json!({
                        "type": "response.output_text.done",
                        "response_id": response.id,
                        "text": response.output_text,
                    }),
                });
                let _ = tx.send(SseFrame::Json {
                    event: "response.completed",
                    payload: json!({
                        "type": "response.completed",
                        "response": response,
                    }),
                });
                let _ = tx.send(SseFrame::Done);
            }
            Err(error) => {
                let _ = tx.send(SseFrame::Json {
                    event: "response.failed",
                    payload: json!({
                        "type": "response.failed",
                        "error": {
                            "code": error.code,
                            "message": error.message,
                        }
                    }),
                });
                let _ = tx.send(SseFrame::Done);
            }
        }
    });

    let stream =
        UnboundedReceiverStream::new(rx).map(|frame| Ok::<Event, Infallible>(frame.into_event()));
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn execute_openresponses_request(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
    stream_sender: Option<mpsc::UnboundedSender<SseFrame>>,
) -> Result<OpenResponsesExecutionResult, OpenResponsesApiError> {
    let mut translated = translate_openresponses_request(&request, state.config.max_input_chars)?;
    if request.model.is_some() {
        translated.ignored_fields.push("model".to_string());
    }

    let response_id = state.next_response_id();
    let created = current_unix_timestamp();

    if let Some(sender) = &stream_sender {
        let _ = sender.send(SseFrame::Json {
            event: "response.created",
            payload: json!({
                "type": "response.created",
                "response": {
                    "id": response_id,
                    "object": "response",
                    "status": "in_progress",
                    "model": state.config.model,
                    "created": created,
                }
            }),
        });
    }

    let mut agent = Agent::new(
        state.config.client.clone(),
        AgentConfig {
            model: state.config.model.clone(),
            system_prompt: state.config.system_prompt.clone(),
            max_turns: state.config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            ..AgentConfig::default()
        },
    );
    state.config.tool_registrar.register(&mut agent);

    let usage = Arc::new(Mutex::new(OpenResponsesUsageSummary::default()));
    agent.subscribe({
        let usage = usage.clone();
        move |event| {
            if let AgentEvent::TurnEnd {
                usage: turn_usage, ..
            } = event
            {
                if let Ok(mut guard) = usage.lock() {
                    guard.input_tokens = guard.input_tokens.saturating_add(turn_usage.input_tokens);
                    guard.output_tokens =
                        guard.output_tokens.saturating_add(turn_usage.output_tokens);
                    guard.total_tokens = guard.total_tokens.saturating_add(turn_usage.total_tokens);
                }
            }
        }
    });

    let session_path = gateway_session_path(&state.config.state_dir, &translated.session_key);
    let mut session_runtime = Some(
        initialize_gateway_session_runtime(
            &session_path,
            &state.config.system_prompt,
            state.config.session_lock_wait_ms,
            state.config.session_lock_stale_ms,
            &mut agent,
        )
        .map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to initialize gateway session runtime: {error}"
            ))
        })?,
    );

    let start_index = agent.messages().len();
    let stream_handler = stream_sender.as_ref().map(|sender| {
        let sender = sender.clone();
        let response_id = response_id.clone();
        Arc::new(move |delta: String| {
            if delta.is_empty() {
                return;
            }
            let _ = sender.send(SseFrame::Json {
                event: "response.output_text.delta",
                payload: json!({
                    "type": "response.output_text.delta",
                    "response_id": response_id,
                    "delta": delta,
                }),
            });
        }) as StreamDeltaHandler
    });

    let prompt_result = if state.config.turn_timeout_ms == 0 {
        agent
            .prompt_with_stream(&translated.prompt, stream_handler)
            .await
    } else {
        match tokio::time::timeout(
            Duration::from_millis(state.config.turn_timeout_ms),
            agent.prompt_with_stream(&translated.prompt, stream_handler),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                return Err(OpenResponsesApiError::timeout(
                    "response generation timed out before completion",
                ));
            }
        }
    };

    let new_messages = prompt_result.map_err(|error| {
        OpenResponsesApiError::gateway_failure(format!("gateway runtime failed: {error}"))
    })?;
    persist_messages(&mut session_runtime, &new_messages).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to persist gateway session messages: {error}"
        ))
    })?;

    let output_text = collect_assistant_reply(&agent.messages()[start_index..]);
    let usage = usage
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("prompt usage lock is poisoned"))?
        .clone();

    let mut ignored = BTreeSet::new();
    for field in translated.ignored_fields {
        if !field.trim().is_empty() {
            ignored.insert(field);
        }
    }

    let response = OpenResponsesResponse {
        id: response_id,
        object: "response",
        created,
        status: "completed",
        model: state.config.model.clone(),
        output: vec![OpenResponsesOutputItem {
            id: state.next_output_message_id(),
            kind: "message",
            role: "assistant",
            content: vec![OpenResponsesOutputTextItem {
                kind: "output_text",
                text: output_text.clone(),
            }],
        }],
        output_text,
        usage: OpenResponsesUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        },
        ignored_fields: ignored.into_iter().collect(),
    };

    Ok(OpenResponsesExecutionResult { response })
}

#[cfg(test)]
fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}
