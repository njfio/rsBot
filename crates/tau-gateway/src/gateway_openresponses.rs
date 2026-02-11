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
use tau_ai::{LlmClient, Message, MessageRole, StreamDeltaHandler};
use tau_core::{current_unix_timestamp, current_unix_timestamp_ms};
use tau_runtime::TransportHealthSnapshot;
use tau_session::SessionStore;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::remote_profile::GatewayOpenResponsesAuthMode;

const OPENRESPONSES_ENDPOINT: &str = "/v1/responses";
const WEBCHAT_ENDPOINT: &str = "/webchat";
const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
const GATEWAY_WS_ENDPOINT: &str = "/gateway/ws";
const GATEWAY_AUTH_SESSION_ENDPOINT: &str = "/gateway/auth/session";
const DEFAULT_SESSION_KEY: &str = "default";
const INPUT_BODY_SIZE_MULTIPLIER: usize = 8;
const GATEWAY_WS_HEARTBEAT_REQUEST_ID: &str = "gateway-heartbeat";

pub trait GatewayToolRegistrar: Send + Sync {
    fn register(&self, agent: &mut Agent);
}

#[derive(Clone, Default)]
pub struct NoopGatewayToolRegistrar;

impl GatewayToolRegistrar for NoopGatewayToolRegistrar {
    fn register(&self, _agent: &mut Agent) {}
}

#[derive(Clone)]
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

#[derive(Debug)]
struct SessionRuntime {
    store: SessionStore,
    active_head: Option<u64>,
}

fn persist_messages(
    session_runtime: &mut Option<SessionRuntime>,
    new_messages: &[Message],
) -> Result<()> {
    let Some(runtime) = session_runtime.as_mut() else {
        return Ok(());
    };

    runtime.active_head = runtime
        .store
        .append_messages(runtime.active_head, new_messages)?;
    Ok(())
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

#[derive(Debug)]
struct OpenResponsesApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl OpenResponsesApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }

    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        )
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, "input_too_large", message)
    }

    fn timeout(message: impl Into<String>) -> Self {
        Self::new(StatusCode::REQUEST_TIMEOUT, "request_timeout", message)
    }

    fn gateway_failure(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, "gateway_runtime_error", message)
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
    }
}

impl IntoResponse for OpenResponsesApiError {
    fn into_response(self) -> Response {
        let error_type = if self.status.is_client_error() {
            "invalid_request_error"
        } else {
            "server_error"
        };
        (
            self.status,
            Json(json!({
                "error": {
                    "type": error_type,
                    "code": self.code,
                    "message": self.message,
                }
            })),
        )
            .into_response()
    }
}

#[derive(Debug, Deserialize)]
struct OpenResponsesRequest {
    #[allow(dead_code)]
    model: Option<String>,
    #[serde(default)]
    input: Value,
    #[serde(default)]
    stream: bool,
    instructions: Option<String>,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    conversation: Option<String>,
    #[serde(default, rename = "previous_response_id")]
    previous_response_id: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct GatewayAuthSessionRequest {
    password: String,
}

#[derive(Debug, Serialize)]
struct GatewayAuthSessionResponse {
    access_token: String,
    token_type: &'static str,
    expires_unix_ms: u64,
    expires_in_seconds: u64,
}

#[derive(Debug)]
struct OpenResponsesPrompt {
    prompt: String,
    session_key: String,
    ignored_fields: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct OpenResponsesUsageSummary {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Serialize)]
struct OpenResponsesOutputTextItem {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

#[derive(Debug, Serialize)]
struct OpenResponsesOutputItem {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    role: &'static str,
    content: Vec<OpenResponsesOutputTextItem>,
}

#[derive(Debug, Serialize)]
struct OpenResponsesUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Serialize)]
struct OpenResponsesResponse {
    id: String,
    object: &'static str,
    created: u64,
    status: &'static str,
    model: String,
    output: Vec<OpenResponsesOutputItem>,
    output_text: String,
    usage: OpenResponsesUsage,
    ignored_fields: Vec<String>,
}

#[derive(Debug)]
struct OpenResponsesExecutionResult {
    response: OpenResponsesResponse,
}

#[derive(Debug)]
enum SseFrame {
    Json { event: &'static str, payload: Value },
    Done,
}

impl SseFrame {
    fn into_event(self) -> Event {
        match self {
            Self::Json { event, payload } => {
                Event::default().event(event).data(payload.to_string())
            }
            Self::Done => Event::default().event("done").data("[DONE]"),
        }
    }
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
                "state_dir": state.config.state_dir.display().to_string(),
                "model": state.config.model,
            }
        })),
    )
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

fn gateway_ws_message_from_frame(
    frame: &crate::gateway_ws_protocol::GatewayWsResponseFrame,
) -> WsMessage {
    match serde_json::to_string(frame) {
        Ok(raw) => WsMessage::Text(raw.into()),
        Err(error) => {
            let fallback = crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                &frame.request_id,
                crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR,
                format!("failed to serialize gateway websocket frame: {error}").as_str(),
            );
            WsMessage::Text(
                serde_json::to_string(&fallback)
                    .unwrap_or_else(|_| {
                        "{\"schema_version\":1,\"request_id\":\"unknown-request\",\"kind\":\"error\",\"payload\":{\"code\":\"internal_error\",\"message\":\"failed to serialize gateway websocket frame\"}}".to_string()
                    })
                    .into(),
            )
        }
    }
}

fn gateway_ws_error_code_from_api_error(error: &OpenResponsesApiError) -> &'static str {
    match error.code {
        "unauthorized" => crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_UNAUTHORIZED,
        "rate_limited" => crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_RATE_LIMITED,
        "internal_error" => crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR,
        other => other,
    }
}

fn enforce_gateway_ws_principal_active(
    state: &GatewayOpenResponsesServerState,
    principal: &str,
) -> Result<(), OpenResponsesApiError> {
    let Some(session_token) = principal.strip_prefix("session:") else {
        return Ok(());
    };

    let now_unix_ms = current_unix_timestamp_ms();
    let mut auth_state = state
        .auth_runtime
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;
    prune_expired_gateway_sessions(&mut auth_state, now_unix_ms);
    if let Some(session) = auth_state.sessions.get_mut(session_token) {
        session.last_seen_unix_ms = now_unix_ms;
        session.request_count = session.request_count.saturating_add(1);
        return Ok(());
    }
    auth_state.auth_failures = auth_state.auth_failures.saturating_add(1);
    Err(OpenResponsesApiError::unauthorized())
}

fn gateway_ws_resolve_session_key(payload: &serde_json::Map<String, Value>) -> Result<String> {
    let requested = crate::gateway_ws_protocol::parse_optional_session_key(payload)?;
    Ok(sanitize_session_key(
        requested.as_deref().unwrap_or(DEFAULT_SESSION_KEY),
    ))
}

fn collect_gateway_ws_session_status_payload(
    state: &GatewayOpenResponsesServerState,
    payload: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let session_key = gateway_ws_resolve_session_key(payload)?;
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    let exists = session_path.exists();
    let (message_count, bytes) = if exists {
        let raw = std::fs::read_to_string(&session_path)
            .with_context(|| format!("failed to read {}", session_path.display()))?;
        (
            raw.lines().filter(|line| !line.trim().is_empty()).count(),
            raw.len(),
        )
    } else {
        (0, 0)
    };
    Ok(json!({
        "session_key": session_key,
        "exists": exists,
        "message_count": message_count,
        "bytes": bytes,
    }))
}

fn collect_gateway_ws_session_reset_payload(
    state: &GatewayOpenResponsesServerState,
    payload: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let session_key = gateway_ws_resolve_session_key(payload)?;
    let session_path = gateway_session_path(&state.config.state_dir, &session_key);
    let lock_path = session_path.with_extension("lock");
    let mut reset = false;

    if session_path.exists() {
        std::fs::remove_file(&session_path)
            .with_context(|| format!("failed to remove {}", session_path.display()))?;
        reset = true;
    }
    if lock_path.exists() {
        std::fs::remove_file(&lock_path)
            .with_context(|| format!("failed to remove {}", lock_path.display()))?;
    }

    Ok(json!({
        "session_key": session_key,
        "reset": reset,
    }))
}

fn collect_gateway_ws_run_lifecycle_status_payload() -> Value {
    let capabilities = crate::gateway_ws_protocol::gateway_ws_capabilities_payload();
    json!({
        "active_runs": [],
        "recent_events": [],
        "event_kinds": capabilities["contracts"]["run_lifecycle"]["event_kinds"].clone(),
    })
}

fn dispatch_gateway_ws_control_text_frame(
    state: &GatewayOpenResponsesServerState,
    principal: &str,
    raw: &str,
) -> (crate::gateway_ws_protocol::GatewayWsResponseFrame, bool) {
    if let Err(error) = enforce_gateway_ws_principal_active(state, principal) {
        let request_id = crate::gateway_ws_protocol::best_effort_gateway_ws_request_id(raw)
            .unwrap_or_else(|| "unknown-request".to_string());
        let code = gateway_ws_error_code_from_api_error(&error);
        return (
            crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                &request_id,
                code,
                error.message.as_str(),
            ),
            true,
        );
    }
    if let Err(error) = enforce_gateway_rate_limit(state, principal) {
        let request_id = crate::gateway_ws_protocol::best_effort_gateway_ws_request_id(raw)
            .unwrap_or_else(|| "unknown-request".to_string());
        let code = gateway_ws_error_code_from_api_error(&error);
        return (
            crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                &request_id,
                code,
                error.message.as_str(),
            ),
            false,
        );
    }

    let request_frame = match crate::gateway_ws_protocol::parse_gateway_ws_request_frame(raw) {
        Ok(frame) => frame,
        Err(error) => {
            let request_id = crate::gateway_ws_protocol::best_effort_gateway_ws_request_id(raw)
                .unwrap_or_else(|| "unknown-request".to_string());
            let message = error.to_string();
            let code =
                crate::gateway_ws_protocol::classify_gateway_ws_parse_error(message.as_str());
            return (
                crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                    &request_id,
                    code,
                    message.as_str(),
                ),
                false,
            );
        }
    };

    let response = match request_frame.kind {
        crate::gateway_ws_protocol::GatewayWsRequestKind::Capabilities => {
            crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                &request_frame.request_id,
                "capabilities.response",
                crate::gateway_ws_protocol::gateway_ws_capabilities_payload(),
            )
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::GatewayStatus => {
            match crate::gateway_runtime::inspect_gateway_service_mode(&state.config.state_dir) {
                Ok(service_report) => {
                    let multi_channel_report =
                        collect_gateway_multi_channel_status_report(&state.config.state_dir);
                    crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                        &request_frame.request_id,
                        "gateway.status.response",
                        json!({
                            "service": service_report,
                            "auth": collect_gateway_auth_status_report(state),
                            "multi_channel": multi_channel_report,
                            "gateway": {
                                "responses_endpoint": OPENRESPONSES_ENDPOINT,
                                "status_endpoint": GATEWAY_STATUS_ENDPOINT,
                                "webchat_endpoint": WEBCHAT_ENDPOINT,
                                "auth_session_endpoint": GATEWAY_AUTH_SESSION_ENDPOINT,
                                "ws_endpoint": GATEWAY_WS_ENDPOINT,
                                "model": state.config.model,
                                "state_dir": state.config.state_dir.display().to_string(),
                            }
                        }),
                    )
                }
                Err(error) => crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                    &request_frame.request_id,
                    crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INTERNAL_ERROR,
                    format!("failed to inspect gateway service state: {error}").as_str(),
                ),
            }
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::SessionStatus => {
            match collect_gateway_ws_session_status_payload(state, &request_frame.payload) {
                Ok(payload) => crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                    &request_frame.request_id,
                    "session.status.response",
                    payload,
                ),
                Err(error) => {
                    let message = error.to_string();
                    let code = crate::gateway_ws_protocol::classify_gateway_ws_parse_error(
                        message.as_str(),
                    );
                    crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                        &request_frame.request_id,
                        code,
                        message.as_str(),
                    )
                }
            }
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::SessionReset => {
            match collect_gateway_ws_session_reset_payload(state, &request_frame.payload) {
                Ok(payload) => crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                    &request_frame.request_id,
                    "session.reset.response",
                    payload,
                ),
                Err(error) => {
                    let message = error.to_string();
                    let code = crate::gateway_ws_protocol::classify_gateway_ws_parse_error(
                        message.as_str(),
                    );
                    crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                        &request_frame.request_id,
                        code,
                        message.as_str(),
                    )
                }
            }
        }
        crate::gateway_ws_protocol::GatewayWsRequestKind::RunLifecycleStatus => {
            crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                &request_frame.request_id,
                "run.lifecycle.status.response",
                collect_gateway_ws_run_lifecycle_status_payload(),
            )
        }
    };

    (response, false)
}

async fn run_gateway_ws_connection(
    state: Arc<GatewayOpenResponsesServerState>,
    socket: WebSocket,
    principal: String,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(
        crate::gateway_ws_protocol::GATEWAY_WS_HEARTBEAT_INTERVAL_SECONDS.max(1),
    ));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            inbound = receiver.next() => {
                let Some(inbound) = inbound else {
                    break;
                };
                let message = match inbound {
                    Ok(message) => message,
                    Err(_) => break,
                };

                match message {
                    WsMessage::Text(text) => {
                        let (response, should_close) =
                            dispatch_gateway_ws_control_text_frame(&state, principal.as_str(), text.as_str());
                        if sender
                            .send(gateway_ws_message_from_frame(&response))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        if should_close {
                            break;
                        }
                    }
                    WsMessage::Binary(bytes) => {
                        let request_id = "unknown-request";
                        let response = match String::from_utf8(bytes.to_vec()) {
                            Ok(text) => dispatch_gateway_ws_control_text_frame(
                                &state,
                                principal.as_str(),
                                text.as_str(),
                            )
                            .0,
                            Err(_) => crate::gateway_ws_protocol::build_gateway_ws_error_frame(
                                request_id,
                                crate::gateway_ws_protocol::GATEWAY_WS_ERROR_CODE_INVALID_PAYLOAD,
                                "gateway websocket binary frame must be UTF-8 encoded JSON text",
                            ),
                        };
                        if sender
                            .send(gateway_ws_message_from_frame(&response))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    WsMessage::Ping(payload) => {
                        if sender.send(WsMessage::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    WsMessage::Pong(_) => {}
                    WsMessage::Close(_) => break,
                }
            }
            _ = heartbeat.tick() => {
                if sender.send(WsMessage::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
                let heartbeat_frame = crate::gateway_ws_protocol::build_gateway_ws_response_frame(
                    GATEWAY_WS_HEARTBEAT_REQUEST_ID,
                    "gateway.heartbeat",
                    json!({
                        "ts_unix_ms": current_unix_timestamp_ms(),
                    }),
                );
                if sender
                    .send(gateway_ws_message_from_frame(&heartbeat_frame))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
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

fn translate_openresponses_request(
    request: &OpenResponsesRequest,
    max_input_chars: usize,
) -> Result<OpenResponsesPrompt, OpenResponsesApiError> {
    let mut ignored_fields = request.extra.keys().cloned().collect::<Vec<_>>();
    ignored_fields.sort();

    let mut segments = Vec::new();
    if let Some(instructions) = non_empty_trimmed(request.instructions.as_deref()) {
        segments.push(format!("System instructions:\n{instructions}"));
    }

    if let Some(previous_response_id) = non_empty_trimmed(request.previous_response_id.as_deref()) {
        segments.push(format!(
            "Continuation context (previous_response_id):\n{previous_response_id}"
        ));
    }

    let mut extracted = 0usize;
    extract_openresponses_input_segments(
        &request.input,
        &mut segments,
        &mut extracted,
        &mut ignored_fields,
    )?;

    if extracted == 0 {
        return Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input must include at least one textual message or function_call_output item",
        ));
    }

    let prompt = segments
        .iter()
        .filter_map(|segment| {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if prompt.is_empty() {
        return Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input did not contain usable text",
        ));
    }

    if prompt.chars().count() > max_input_chars {
        return Err(OpenResponsesApiError::payload_too_large(format!(
            "translated input exceeds max {} characters",
            max_input_chars
        )));
    }

    let session_seed = metadata_string(&request.metadata, "session_id")
        .or_else(|| non_empty_trimmed(request.conversation.as_deref()))
        .or_else(|| non_empty_trimmed(request.previous_response_id.as_deref()))
        .unwrap_or(DEFAULT_SESSION_KEY);

    Ok(OpenResponsesPrompt {
        prompt,
        session_key: sanitize_session_key(session_seed),
        ignored_fields,
    })
}

fn extract_openresponses_input_segments(
    input: &Value,
    segments: &mut Vec<String>,
    extracted: &mut usize,
    ignored_fields: &mut Vec<String>,
) -> Result<(), OpenResponsesApiError> {
    match input {
        Value::Null => Err(OpenResponsesApiError::bad_request(
            "missing_input",
            "input is required",
        )),
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                segments.push(format!("User:\n{text}"));
                *extracted = extracted.saturating_add(1);
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                extract_openresponses_item(item, index, segments, extracted, ignored_fields)?;
            }
            Ok(())
        }
        Value::Object(_) => {
            extract_openresponses_item(input, 0, segments, extracted, ignored_fields)
        }
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_input",
            "input must be a string, object, or array",
        )),
    }
}

fn extract_openresponses_item(
    item: &Value,
    index: usize,
    segments: &mut Vec<String>,
    extracted: &mut usize,
    ignored_fields: &mut Vec<String>,
) -> Result<(), OpenResponsesApiError> {
    match item {
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                segments.push(format!("User:\n{text}"));
                *extracted = extracted.saturating_add(1);
            }
            Ok(())
        }
        Value::Object(map) => {
            let item_type = map.get("type").and_then(Value::as_str).unwrap_or_default();
            if item_type == "function_call_output" {
                let output = stringify_output(map.get("output").unwrap_or(&Value::Null));
                if output.is_empty() {
                    return Err(OpenResponsesApiError::bad_request(
                        "invalid_function_call_output",
                        format!(
                            "input[{index}] function_call_output item requires non-empty output"
                        ),
                    ));
                }
                let call_id = map
                    .get("call_id")
                    .or_else(|| map.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("unknown");
                segments.push(format!("Function output (call_id={call_id}):\n{output}"));
                *extracted = extracted.saturating_add(1);
                return Ok(());
            }

            if item_type == "message" || map.contains_key("role") || map.contains_key("content") {
                let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
                let text = extract_message_content_text(map.get("content"));
                if !text.is_empty() {
                    segments.push(format!("{}:\n{}", role_label(role), text));
                    *extracted = extracted.saturating_add(1);
                } else {
                    ignored_fields.push(format!("input[{index}].content"));
                }
                return Ok(());
            }

            ignored_fields.push(format!("input[{index}]"));
            Ok(())
        }
        _ => {
            ignored_fields.push(format!("input[{index}]"));
            Ok(())
        }
    }
}

fn extract_message_content_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };

    match content {
        Value::String(text) => text.trim().to_string(),
        Value::Array(parts) => {
            let mut segments = Vec::new();
            for part in parts {
                if let Some(text) = extract_message_content_part(part) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        segments.push(trimmed.to_string());
                    }
                }
            }
            segments.join("\n")
        }
        Value::Object(_) => extract_message_content_part(content).unwrap_or_default(),
        _ => String::new(),
    }
}

fn extract_message_content_part(part: &Value) -> Option<String> {
    match part {
        Value::String(text) => Some(text.to_string()),
        Value::Object(map) => {
            let part_type = map.get("type").and_then(Value::as_str).unwrap_or("text");
            match part_type {
                "input_text" | "output_text" | "text" => map
                    .get("text")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                "function_call_output" => {
                    let output = stringify_output(map.get("output").unwrap_or(&Value::Null));
                    if output.trim().is_empty() {
                        return None;
                    }
                    let call_id = map
                        .get("call_id")
                        .or_else(|| map.get("id"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("unknown");
                    Some(format!("Function output (call_id={call_id}):\n{output}"))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn stringify_output(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn role_label(role: &str) -> &'static str {
    match role.trim().to_ascii_lowercase().as_str() {
        "assistant" => "Assistant context",
        "system" => "System context",
        "tool" => "Tool context",
        _ => "User",
    }
}

fn metadata_string<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .as_object()?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn non_empty_trimmed(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|value| !value.is_empty())
}

fn sanitize_session_key(raw: &str) -> String {
    let mut normalized = String::new();
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }
    let normalized = normalized.trim_matches('_').to_string();
    if normalized.is_empty() {
        DEFAULT_SESSION_KEY.to_string()
    } else {
        normalized
    }
}

fn gateway_session_path(state_dir: &Path, session_key: &str) -> PathBuf {
    state_dir
        .join("openresponses")
        .join("sessions")
        .join(format!("{session_key}.jsonl"))
}

fn initialize_gateway_session_runtime(
    session_path: &Path,
    system_prompt: &str,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
    agent: &mut Agent,
) -> Result<SessionRuntime> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    let active_head = store.ensure_initialized(system_prompt)?;
    let lineage = store.lineage_messages(active_head)?;
    if !lineage.is_empty() {
        agent.replace_messages(lineage);
    }
    Ok(SessionRuntime { store, active_head })
}

fn collect_assistant_reply(messages: &[Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this request.".to_string()
    } else {
        content
    }
}

fn render_gateway_webchat_page() -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Tau Gateway Webchat</title>
  <style>
    :root {{
      color-scheme: light;
      font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
    }}
    body {{
      margin: 0;
      background: linear-gradient(160deg, #f4f6f8 0%, #eef2f7 100%);
      color: #13232f;
    }}
    .container {{
      max-width: 980px;
      margin: 0 auto;
      padding: 1.5rem;
    }}
    h1 {{
      margin: 0 0 0.5rem 0;
      font-size: 1.5rem;
    }}
    p {{
      margin: 0.25rem 0 1rem 0;
      color: #3a4f5f;
    }}
    .grid {{
      display: grid;
      gap: 1rem;
      grid-template-columns: 1fr;
    }}
    .panel {{
      background: #ffffff;
      border: 1px solid #d2dde6;
      border-radius: 12px;
      padding: 1rem;
      box-shadow: 0 8px 20px rgba(12, 25, 38, 0.06);
    }}
    label {{
      display: block;
      font-size: 0.85rem;
      margin-bottom: 0.25rem;
      color: #375062;
    }}
    input[type="text"], textarea {{
      width: 100%;
      box-sizing: border-box;
      border: 1px solid #b8c9d6;
      border-radius: 8px;
      padding: 0.55rem 0.7rem;
      font-size: 0.95rem;
      background: #fbfdff;
      color: #13232f;
    }}
    textarea {{
      min-height: 150px;
      resize: vertical;
    }}
    .row {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
      gap: 0.8rem;
      margin-bottom: 0.8rem;
    }}
    .actions {{
      display: flex;
      gap: 0.5rem;
      flex-wrap: wrap;
      margin-top: 0.8rem;
    }}
    button {{
      border: 0;
      border-radius: 8px;
      background: #0f7d5f;
      color: #ffffff;
      padding: 0.55rem 0.9rem;
      font-weight: 600;
      cursor: pointer;
    }}
    button.secondary {{
      background: #3f5f74;
    }}
    button:disabled {{
      cursor: wait;
      opacity: 0.6;
    }}
    .checkbox {{
      display: inline-flex;
      align-items: center;
      gap: 0.4rem;
      margin-top: 0.2rem;
      color: #274355;
      font-size: 0.9rem;
    }}
    pre {{
      margin: 0;
      background: #0f1f2b;
      color: #d9ecf7;
      border-radius: 10px;
      padding: 0.8rem;
      overflow: auto;
      max-height: 300px;
      white-space: pre-wrap;
      word-break: break-word;
      font-size: 0.85rem;
    }}
    @media (min-width: 900px) {{
      .grid {{
        grid-template-columns: 1.4fr 1fr;
      }}
    }}
  </style>
</head>
<body>
  <main class="container">
    <h1>Tau Gateway Webchat</h1>
    <p>Operator webchat for the OpenResponses gateway runtime.</p>
    <div class="grid">
      <section class="panel">
        <div class="row">
          <div>
            <label for="authToken">Bearer token</label>
            <input id="authToken" type="text" autocomplete="off" placeholder="gateway auth token" />
          </div>
          <div>
            <label for="sessionKey">Session key</label>
            <input id="sessionKey" type="text" autocomplete="off" value="{default_session_key}" />
          </div>
        </div>
        <label class="checkbox">
          <input id="stream" type="checkbox" checked />
          Stream response (SSE)
        </label>
        <div style="margin-top: 0.8rem;">
          <label for="prompt">Prompt</label>
          <textarea id="prompt" placeholder="Ask Tau through the gateway..."></textarea>
        </div>
        <div class="actions">
          <button id="send">Send</button>
          <button id="refreshStatus" class="secondary">Refresh status</button>
          <button id="clearOutput" class="secondary">Clear output</button>
        </div>
      </section>
      <section class="panel">
        <h2 style="margin: 0 0 0.5rem 0; font-size: 1rem;">Gateway status</h2>
        <pre id="status">Press "Refresh status" to inspect gateway service state, multi-channel lifecycle summary, connector counters, and recent reason codes.</pre>
      </section>
    </div>
    <section class="panel" style="margin-top: 1rem;">
      <h2 style="margin: 0 0 0.5rem 0; font-size: 1rem;">Response output</h2>
      <pre id="output">No response yet.</pre>
    </section>
  </main>
  <script>
    const RESPONSES_ENDPOINT = "{responses_endpoint}";
    const STATUS_ENDPOINT = "{status_endpoint}";
    const WEBSOCKET_ENDPOINT = "{websocket_endpoint}";
    const STORAGE_TOKEN = "tau.gateway.webchat.token";
    const STORAGE_SESSION = "tau.gateway.webchat.session";
    const tokenInput = document.getElementById("authToken");
    const sessionInput = document.getElementById("sessionKey");
    const streamInput = document.getElementById("stream");
    const promptInput = document.getElementById("prompt");
    const outputPre = document.getElementById("output");
    const statusPre = document.getElementById("status");
    const sendButton = document.getElementById("send");
    const refreshButton = document.getElementById("refreshStatus");
    const clearButton = document.getElementById("clearOutput");

    function loadLocalValues() {{
      const storedToken = window.localStorage.getItem(STORAGE_TOKEN);
      const storedSession = window.localStorage.getItem(STORAGE_SESSION);
      if (storedToken) {{
        tokenInput.value = storedToken;
      }}
      if (storedSession) {{
        sessionInput.value = storedSession;
      }}
    }}

    function saveLocalValues() {{
      window.localStorage.setItem(STORAGE_TOKEN, tokenInput.value.trim());
      window.localStorage.setItem(STORAGE_SESSION, sessionInput.value.trim());
    }}

    function authHeaders() {{
      const token = tokenInput.value.trim();
      if (token.length === 0) {{
        return {{}};
      }}
      return {{
        "Authorization": "Bearer " + token
      }};
    }}

    function setOutput(text) {{
      outputPre.textContent = text;
    }}

    function appendOutput(text) {{
      if (outputPre.textContent === "No response yet.") {{
        outputPre.textContent = "";
      }}
      outputPre.textContent += text;
    }}

    function processSseFrame(frame) {{
      if (!frame || frame.trim().length === 0) {{
        return;
      }}
      let eventName = "";
      let data = "";
      const lines = frame.split(/\r?\n/);
      for (const line of lines) {{
        if (line.startsWith("event:")) {{
          eventName = line.slice("event:".length).trim();
        }} else if (line.startsWith("data:")) {{
          data += line.slice("data:".length).trim();
        }}
      }}
      if (data.length === 0 || data === "[DONE]") {{
        return;
      }}
      let payload = null;
      try {{
        payload = JSON.parse(data);
      }} catch (error) {{
        appendOutput("\n[invalid sse payload] " + data + "\n");
        return;
      }}
      if (eventName === "response.output_text.delta") {{
        appendOutput(payload.delta || "");
        return;
      }}
      if (eventName === "response.output_text.done") {{
        appendOutput("\n");
        return;
      }}
      if (eventName === "response.failed") {{
        const message = payload && payload.error ? payload.error.message : "unknown";
        appendOutput("\n[gateway error] " + message + "\n");
      }}
    }}

    async function readSseBody(response) {{
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {{
        const result = await reader.read();
        if (result.done) {{
          break;
        }}
        buffer += decoder.decode(result.value, {{ stream: true }});
        while (true) {{
          const splitIndex = buffer.indexOf("\n\n");
          if (splitIndex < 0) {{
            break;
          }}
          const frame = buffer.slice(0, splitIndex);
          buffer = buffer.slice(splitIndex + 2);
          processSseFrame(frame);
        }}
      }}
      if (buffer.trim().length > 0) {{
        processSseFrame(buffer);
      }}
    }}

    function renderMultiChannelChannelRows(connectors) {{
      if (!connectors || !connectors.channels) {{
        return "none";
      }}
      const entries = Object.entries(connectors.channels);
      if (entries.length === 0) {{
        return "none";
      }}
      entries.sort((left, right) => left[0].localeCompare(right[0]));
      return entries.map(([channel, status]) => {{
        return channel +
          ":liveness=" + (status.liveness || "unknown") +
          " breaker=" + (status.breaker_state || "unknown") +
          " ingested=" + String(status.events_ingested || 0) +
          " dup=" + String(status.duplicates_skipped || 0) +
          " retry=" + String(status.retry_attempts || 0) +
          " auth_fail=" + String(status.auth_failures || 0) +
          " parse_fail=" + String(status.parse_failures || 0) +
          " provider_fail=" + String(status.provider_failures || 0);
      }}).join("\n");
    }}

    function formatGatewayStatusSummary(payload) {{
      const service = payload && payload.service ? payload.service : {{}};
      const auth = payload && payload.auth ? payload.auth : {{}};
      const mc = payload && payload.multi_channel ? payload.multi_channel : {{}};
      const connectors = mc.connectors || {{}};
      const reasonCodes = Array.isArray(mc.last_reason_codes) && mc.last_reason_codes.length > 0
        ? mc.last_reason_codes.join(",")
        : "none";
      const diagnostics = Array.isArray(mc.diagnostics) && mc.diagnostics.length > 0
        ? mc.diagnostics.join(",")
        : "none";
      const transportCounts = mc.transport_counts ? JSON.stringify(mc.transport_counts) : "{{}}";
      return [
        "gateway_service: status=" + String(service.service_status || "unknown") +
          " rollout_gate=" + String(service.rollout_gate || "unknown") +
          " reason_code=" + String(service.rollout_reason_code || "unknown"),
        "gateway_auth: mode=" + String(auth.mode || "unknown") +
          " active_sessions=" + String(auth.active_sessions || 0) +
          " auth_failures=" + String(auth.auth_failures || 0) +
          " rate_limited=" + String(auth.rate_limited_requests || 0),
        "multi_channel_lifecycle: state_present=" + String(Boolean(mc.state_present)) +
          " health=" + String(mc.health_state || "unknown") +
          " rollout_gate=" + String(mc.rollout_gate || "hold") +
          " processed=" + String(mc.processed_event_count || 0) +
          " queue_depth=" + String(mc.queue_depth || 0) +
          " failure_streak=" + String(mc.failure_streak || 0) +
          " last_cycle_failed=" + String(mc.last_cycle_failed || 0) +
          " last_cycle_completed=" + String(mc.last_cycle_completed || 0),
        "multi_channel_reason_codes_recent: " + reasonCodes,
        "multi_channel_reason_code_counts: " + JSON.stringify(mc.reason_code_counts || {{}}),
        "multi_channel_transport_counts: " + transportCounts,
        "connectors: state_present=" + String(Boolean(connectors.state_present)) +
          " processed=" + String(connectors.processed_event_count || 0),
        "connector_channels:\n" + renderMultiChannelChannelRows(connectors),
        "multi_channel_diagnostics: " + diagnostics,
      ].join("\n");
    }}

    async function refreshStatus() {{
      statusPre.textContent = "Loading gateway status...";
      try {{
        const response = await fetch(STATUS_ENDPOINT, {{
          headers: authHeaders()
        }});
        const raw = await response.text();
        if (!response.ok) {{
          statusPre.textContent = "status " + response.status + "\n" + raw;
          return;
        }}
        const payload = JSON.parse(raw);
        const summary = formatGatewayStatusSummary(payload);
        statusPre.textContent = summary + "\n\nraw_payload:\n" + JSON.stringify(payload, null, 2);
      }} catch (error) {{
        statusPre.textContent = "status request failed: " + String(error);
      }}
    }}

    async function sendPrompt() {{
      const prompt = promptInput.value.trim();
      const sessionKey = sessionInput.value.trim() || "{default_session_key}";
      if (prompt.length === 0) {{
        setOutput("Prompt is required.");
        return;
      }}
      saveLocalValues();
      sendButton.disabled = true;
      try {{
        setOutput("");
        const payload = {{
          input: prompt,
          stream: streamInput.checked,
          metadata: {{
            session_id: sessionKey
          }}
        }};
        const response = await fetch(RESPONSES_ENDPOINT, {{
          method: "POST",
          headers: Object.assign({{
            "Content-Type": "application/json"
          }}, authHeaders()),
          body: JSON.stringify(payload)
        }});
        if (!response.ok) {{
          setOutput("request failed: status=" + response.status + "\n" + await response.text());
          return;
        }}
        if (streamInput.checked) {{
          await readSseBody(response);
        }} else {{
          const body = await response.json();
          const outputText = typeof body.output_text === "string"
            ? body.output_text
            : JSON.stringify(body, null, 2);
          setOutput(outputText);
        }}
        await refreshStatus();
      }} catch (error) {{
        setOutput("request failed: " + String(error));
      }} finally {{
        sendButton.disabled = false;
      }}
    }}

    sendButton.addEventListener("click", sendPrompt);
    refreshButton.addEventListener("click", refreshStatus);
    clearButton.addEventListener("click", () => setOutput("No response yet."));
    tokenInput.addEventListener("change", saveLocalValues);
    sessionInput.addEventListener("change", saveLocalValues);

    loadLocalValues();
  </script>
</body>
</html>
"#,
        responses_endpoint = OPENRESPONSES_ENDPOINT,
        status_endpoint = GATEWAY_STATUS_ENDPOINT,
        websocket_endpoint = GATEWAY_WS_ENDPOINT,
        default_session_key = DEFAULT_SESSION_KEY,
    )
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let header = headers.get(AUTHORIZATION)?;
    let raw = header.to_str().ok()?;
    let token = raw.strip_prefix("Bearer ")?;
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn note_gateway_auth_failure(state: &GatewayOpenResponsesServerState) {
    if let Ok(mut auth_state) = state.auth_runtime.lock() {
        auth_state.auth_failures = auth_state.auth_failures.saturating_add(1);
    }
}

fn prune_expired_gateway_sessions(auth_state: &mut GatewayAuthRuntimeState, now_unix_ms: u64) {
    auth_state
        .sessions
        .retain(|_, session| session.expires_unix_ms > now_unix_ms);
}

fn authorize_gateway_request(
    state: &GatewayOpenResponsesServerState,
    headers: &HeaderMap,
) -> Result<String, OpenResponsesApiError> {
    match state.config.auth_mode {
        GatewayOpenResponsesAuthMode::LocalhostDev => Ok("localhost-dev".to_string()),
        GatewayOpenResponsesAuthMode::Token => {
            let expected = state
                .config
                .auth_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    OpenResponsesApiError::internal("gateway token auth mode is misconfigured")
                })?;
            let Some(observed) = bearer_token_from_headers(headers) else {
                note_gateway_auth_failure(state);
                return Err(OpenResponsesApiError::unauthorized());
            };
            if observed != expected {
                note_gateway_auth_failure(state);
                return Err(OpenResponsesApiError::unauthorized());
            }
            Ok("token".to_string())
        }
        GatewayOpenResponsesAuthMode::PasswordSession => {
            let Some(session_token) = bearer_token_from_headers(headers) else {
                note_gateway_auth_failure(state);
                return Err(OpenResponsesApiError::unauthorized());
            };
            let now_unix_ms = current_unix_timestamp_ms();
            let mut auth_state = state
                .auth_runtime
                .lock()
                .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;
            prune_expired_gateway_sessions(&mut auth_state, now_unix_ms);
            if let Some(session) = auth_state.sessions.get_mut(session_token.as_str()) {
                session.last_seen_unix_ms = now_unix_ms;
                session.request_count = session.request_count.saturating_add(1);
                return Ok(format!("session:{session_token}"));
            }
            auth_state.auth_failures = auth_state.auth_failures.saturating_add(1);
            Err(OpenResponsesApiError::unauthorized())
        }
    }
}

fn enforce_gateway_rate_limit(
    state: &GatewayOpenResponsesServerState,
    principal: &str,
) -> Result<(), OpenResponsesApiError> {
    let window_ms = state
        .config
        .rate_limit_window_seconds
        .saturating_mul(1000)
        .max(1);
    let max_requests = state.config.rate_limit_max_requests.max(1);
    let now_unix_ms = current_unix_timestamp_ms();
    let mut auth_state = state
        .auth_runtime
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;

    let bucket = auth_state
        .rate_limit_buckets
        .entry(principal.to_string())
        .or_default();
    if bucket.window_started_unix_ms == 0
        || now_unix_ms.saturating_sub(bucket.window_started_unix_ms) >= window_ms
    {
        bucket.window_started_unix_ms = now_unix_ms;
        bucket.accepted_requests = 0;
        bucket.rejected_requests = 0;
    }
    if bucket.accepted_requests >= max_requests {
        bucket.rejected_requests = bucket.rejected_requests.saturating_add(1);
        auth_state.rate_limited_requests = auth_state.rate_limited_requests.saturating_add(1);
        return Err(OpenResponsesApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            format!(
                "gateway rate limit exceeded: max {} requests per {} seconds",
                max_requests, state.config.rate_limit_window_seconds
            ),
        ));
    }
    bucket.accepted_requests = bucket.accepted_requests.saturating_add(1);
    Ok(())
}

fn issue_gateway_session_token(
    state: &GatewayOpenResponsesServerState,
    password: &str,
) -> Result<GatewayAuthSessionResponse, OpenResponsesApiError> {
    let expected_password = state
        .config
        .auth_password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| OpenResponsesApiError::internal("gateway password auth is misconfigured"))?;
    if password.trim().is_empty() || password.trim() != expected_password {
        note_gateway_auth_failure(state);
        return Err(OpenResponsesApiError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "invalid gateway password",
        ));
    }

    let now_unix_ms = current_unix_timestamp_ms();
    let ttl_ms = state
        .config
        .session_ttl_seconds
        .saturating_mul(1000)
        .max(1000);
    let expires_unix_ms = now_unix_ms.saturating_add(ttl_ms);
    let access_token = format!("tau_sess_{:016x}", state.next_sequence());
    let mut auth_state = state
        .auth_runtime
        .lock()
        .map_err(|_| OpenResponsesApiError::internal("gateway auth state lock poisoned"))?;
    prune_expired_gateway_sessions(&mut auth_state, now_unix_ms);
    auth_state.sessions.insert(
        access_token.clone(),
        GatewaySessionTokenState {
            expires_unix_ms,
            last_seen_unix_ms: now_unix_ms,
            request_count: 0,
        },
    );
    auth_state.total_sessions_issued = auth_state.total_sessions_issued.saturating_add(1);
    Ok(GatewayAuthSessionResponse {
        access_token,
        token_type: "bearer",
        expires_unix_ms,
        expires_in_seconds: state.config.session_ttl_seconds,
    })
}

fn collect_gateway_auth_status_report(
    state: &GatewayOpenResponsesServerState,
) -> GatewayAuthStatusReport {
    let mut active_sessions = 0usize;
    let mut total_sessions_issued = 0u64;
    let mut auth_failures = 0u64;
    let mut rate_limited_requests = 0u64;
    if let Ok(mut auth_state) = state.auth_runtime.lock() {
        prune_expired_gateway_sessions(&mut auth_state, current_unix_timestamp_ms());
        active_sessions = auth_state.sessions.len();
        total_sessions_issued = auth_state.total_sessions_issued;
        auth_failures = auth_state.auth_failures;
        rate_limited_requests = auth_state.rate_limited_requests;
    }
    GatewayAuthStatusReport {
        mode: state.config.auth_mode.as_str().to_string(),
        session_ttl_seconds: state.config.session_ttl_seconds,
        active_sessions,
        total_sessions_issued,
        auth_failures,
        rate_limited_requests,
        rate_limit_window_seconds: state.config.rate_limit_window_seconds,
        rate_limit_max_requests: state.config.rate_limit_max_requests,
    }
}

fn collect_gateway_multi_channel_status_report(
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

#[cfg(test)]
fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::{SinkExt, StreamExt};
    use reqwest::Client;
    use serde_json::Value;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, TauAiError};
    use tempfile::tempdir;
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{
            self, client::IntoClientRequest, http::HeaderValue, Message as ClientWsMessage,
        },
    };

    #[derive(Clone, Default)]
    struct MockGatewayLlmClient {
        request_message_counts: Arc<Mutex<Vec<usize>>>,
    }

    #[async_trait]
    impl LlmClient for MockGatewayLlmClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.complete_with_stream(request, None).await
        }

        async fn complete_with_stream(
            &self,
            request: ChatRequest,
            on_delta: Option<StreamDeltaHandler>,
        ) -> Result<ChatResponse, TauAiError> {
            let message_count = request.messages.len();
            if let Ok(mut counts) = self.request_message_counts.lock() {
                counts.push(message_count);
            }
            if let Some(handler) = on_delta {
                handler("messages=".to_string());
                handler(message_count.to_string());
            }
            let reply = format!("messages={message_count}");
            Ok(ChatResponse {
                message: Message::assistant_text(reply),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: message_count as u64,
                    output_tokens: 2,
                    total_tokens: message_count as u64 + 2,
                },
            })
        }
    }

    fn test_state_with_auth(
        root: &Path,
        max_input_chars: usize,
        auth_mode: GatewayOpenResponsesAuthMode,
        token: Option<&str>,
        password: Option<&str>,
        rate_limit_window_seconds: u64,
        rate_limit_max_requests: usize,
    ) -> Arc<GatewayOpenResponsesServerState> {
        Arc::new(GatewayOpenResponsesServerState::new(
            GatewayOpenResponsesServerConfig {
                client: Arc::new(MockGatewayLlmClient::default()),
                model: "openai/gpt-4o-mini".to_string(),
                system_prompt: "You are Tau.".to_string(),
                max_turns: 4,
                tool_registrar: Arc::new(NoopGatewayToolRegistrar),
                turn_timeout_ms: 0,
                session_lock_wait_ms: 500,
                session_lock_stale_ms: 10_000,
                state_dir: root.join(".tau/gateway"),
                bind: "127.0.0.1:0".to_string(),
                auth_mode,
                auth_token: token.map(str::to_string),
                auth_password: password.map(str::to_string),
                session_ttl_seconds: 3_600,
                rate_limit_window_seconds,
                rate_limit_max_requests,
                max_input_chars,
            },
        ))
    }

    fn test_state(
        root: &Path,
        max_input_chars: usize,
        token: &str,
    ) -> Arc<GatewayOpenResponsesServerState> {
        test_state_with_auth(
            root,
            max_input_chars,
            GatewayOpenResponsesAuthMode::Token,
            Some(token),
            None,
            60,
            120,
        )
    }

    async fn spawn_test_server(
        state: Arc<GatewayOpenResponsesServerState>,
    ) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind ephemeral listener")?;
        let addr = listener.local_addr().context("resolve listener addr")?;
        let app = build_gateway_openresponses_router(state);
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        Ok((addr, handle))
    }

    fn write_multi_channel_runtime_fixture(root: &Path, with_connectors: bool) -> PathBuf {
        let multi_channel_root = root.join(".tau").join("multi-channel");
        std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel root");
        std::fs::write(
            multi_channel_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1", "discord:dc-1", "telegram:tg-2"],
  "health": {
    "updated_unix_ms": 981,
    "cycle_duration_ms": 14,
    "queue_depth": 2,
    "active_runs": 0,
    "failure_streak": 1,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 2,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write multi-channel state");
        std::fs::write(
            multi_channel_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["events_applied","connector_retry"],"health_reason":"connector retry in progress"}
invalid-json-line
{"reason_codes":["connector_retry"],"health_reason":"connector retry in progress"}
"#,
        )
        .expect("write runtime events");
        if with_connectors {
            std::fs::write(
                multi_channel_root.join("live-connectors-state.json"),
                r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1"],
  "channels": {
    "telegram": {
      "mode": "polling",
      "liveness": "open",
      "events_ingested": 6,
      "duplicates_skipped": 2,
      "retry_attempts": 3,
      "auth_failures": 1,
      "parse_failures": 0,
      "provider_failures": 2,
      "consecutive_failures": 2,
      "retry_budget_remaining": 0,
      "breaker_state": "open",
      "breaker_open_until_unix_ms": 4000,
      "breaker_last_open_reason": "provider_unavailable",
      "breaker_open_count": 1,
      "last_error_code": "provider_unavailable"
    }
  }
}
"#,
            )
            .expect("write connectors state");
        }
        multi_channel_root
    }

    async fn connect_gateway_ws(
        addr: SocketAddr,
        token: Option<&str>,
    ) -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    > {
        let uri = format!("ws://{addr}{GATEWAY_WS_ENDPOINT}");
        let mut request = uri
            .into_client_request()
            .context("failed to construct websocket request")?;
        if let Some(token) = token {
            request.headers_mut().insert(
                AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {token}").as_str())
                    .expect("valid bearer auth header"),
            );
        }
        let (socket, _) = connect_async(request)
            .await
            .context("failed to establish websocket connection")?;
        Ok(socket)
    }

    async fn recv_gateway_ws_json(
        socket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> Value {
        let message = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let Some(message) = socket.next().await else {
                    panic!("websocket closed before response frame");
                };
                let message = message.expect("read websocket frame");
                match message {
                    ClientWsMessage::Text(text) => {
                        return serde_json::from_str::<Value>(text.as_str())
                            .expect("websocket text frame should contain json");
                    }
                    ClientWsMessage::Ping(payload) => {
                        socket
                            .send(ClientWsMessage::Pong(payload))
                            .await
                            .expect("send pong");
                    }
                    ClientWsMessage::Pong(_) => continue,
                    ClientWsMessage::Binary(_) => continue,
                    ClientWsMessage::Close(_) => panic!("websocket closed before json frame"),
                    ClientWsMessage::Frame(_) => continue,
                }
            }
        })
        .await
        .expect("websocket response should arrive before timeout");
        message
    }

    #[test]
    fn unit_translate_openresponses_request_supports_item_input_and_function_call_output() {
        let request = OpenResponsesRequest {
            model: None,
            input: json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Please summarize."}]
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_123",
                    "output": "tool result"
                }
            ]),
            stream: false,
            instructions: Some("be concise".to_string()),
            metadata: json!({"session_id": "issue-42"}),
            conversation: None,
            previous_response_id: None,
            extra: BTreeMap::from([("temperature".to_string(), json!(0.0))]),
        };

        let translated =
            translate_openresponses_request(&request, 10_000).expect("translate request");
        assert!(translated.prompt.contains("System instructions"));
        assert!(translated.prompt.contains("Please summarize."));
        assert!(translated
            .prompt
            .contains("Function output (call_id=call_123):"));
        assert_eq!(translated.session_key, "issue-42");
        assert_eq!(translated.ignored_fields, vec!["temperature".to_string()]);
    }

    #[test]
    fn unit_translate_openresponses_request_rejects_invalid_input_shape() {
        let request = OpenResponsesRequest {
            model: None,
            input: json!(42),
            stream: false,
            instructions: None,
            metadata: json!({}),
            conversation: None,
            previous_response_id: None,
            extra: BTreeMap::new(),
        };

        let error =
            translate_openresponses_request(&request, 1024).expect_err("invalid input should fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "invalid_input");
    }

    #[test]
    fn unit_collect_gateway_multi_channel_status_report_composes_runtime_and_connector_fields() {
        let temp = tempdir().expect("tempdir");
        let gateway_state_dir = temp.path().join(".tau").join("gateway");
        std::fs::create_dir_all(&gateway_state_dir).expect("create gateway state dir");
        write_multi_channel_runtime_fixture(temp.path(), true);

        let report = collect_gateway_multi_channel_status_report(&gateway_state_dir);
        assert!(report.state_present);
        assert_eq!(report.health_state, "degraded");
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.health_reason, "connector retry in progress");
        assert_eq!(report.processed_event_count, 3);
        assert_eq!(report.transport_counts.get("telegram"), Some(&2));
        assert_eq!(report.transport_counts.get("discord"), Some(&1));
        assert_eq!(report.queue_depth, 2);
        assert_eq!(report.failure_streak, 1);
        assert_eq!(report.cycle_reports, 2);
        assert_eq!(report.invalid_cycle_reports, 1);
        assert_eq!(report.reason_code_counts.get("events_applied"), Some(&1));
        assert_eq!(report.reason_code_counts.get("connector_retry"), Some(&2));
        assert!(report.connectors.state_present);
        assert_eq!(report.connectors.processed_event_count, 1);
        let telegram = report
            .connectors
            .channels
            .get("telegram")
            .expect("telegram connector");
        assert_eq!(telegram.liveness, "open");
        assert_eq!(telegram.breaker_state, "open");
        assert_eq!(telegram.provider_failures, 2);
    }

    #[test]
    fn unit_render_gateway_webchat_page_includes_expected_endpoints() {
        let html = render_gateway_webchat_page();
        assert!(html.contains("Tau Gateway Webchat"));
        assert!(html.contains(OPENRESPONSES_ENDPOINT));
        assert!(html.contains(GATEWAY_STATUS_ENDPOINT));
        assert!(html.contains(GATEWAY_WS_ENDPOINT));
        assert!(html.contains(DEFAULT_SESSION_KEY));
        assert!(html.contains("multi_channel_lifecycle: state_present="));
        assert!(html.contains("connector_channels:"));
    }

    #[tokio::test]
    async fn functional_webchat_endpoint_returns_html_shell() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .get(format!("http://{addr}{WEBCHAT_ENDPOINT}"))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.contains("text/html"));
        let body = response.text().await.expect("read webchat body");
        assert!(body.contains("Tau Gateway Webchat"));
        assert!(body.contains(OPENRESPONSES_ENDPOINT));
        assert!(body.contains(GATEWAY_STATUS_ENDPOINT));
        assert!(body.contains(GATEWAY_WS_ENDPOINT));
        assert!(body.contains("multi-channel lifecycle summary"));
        assert!(body.contains("connector counters"));
        assert!(body.contains("recent reason codes"));

        handle.abort();
    }

    #[tokio::test]
    async fn functional_openresponses_endpoint_rejects_unauthorized_requests() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .json(&json!({"input":"hello"}))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        handle.abort();
    }

    #[tokio::test]
    async fn functional_openresponses_endpoint_returns_non_stream_response() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({"input":"hello"}))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response
            .json::<serde_json::Value>()
            .await
            .expect("parse response json");
        assert_eq!(payload["object"], "response");
        assert_eq!(payload["status"], "completed");
        assert!(payload["output_text"]
            .as_str()
            .unwrap_or_default()
            .contains("messages="));

        handle.abort();
    }

    #[tokio::test]
    async fn functional_openresponses_endpoint_streams_sse_for_stream_true() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({"input":"hello", "stream": true}))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(content_type.contains("text/event-stream"));

        let body = response.text().await.expect("read sse body");
        assert!(body.contains("event: response.created"));
        assert!(body.contains("event: response.output_text.delta"));
        assert!(body.contains("event: response.completed"));
        assert!(body.contains("event: done"));

        handle.abort();
    }

    #[tokio::test]
    async fn functional_gateway_auth_session_endpoint_issues_bearer_for_password_mode() {
        let temp = tempdir().expect("tempdir");
        let state = test_state_with_auth(
            temp.path(),
            10_000,
            GatewayOpenResponsesAuthMode::PasswordSession,
            None,
            Some("pw-secret"),
            60,
            120,
        );
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let issue_response = client
            .post(format!("http://{addr}{GATEWAY_AUTH_SESSION_ENDPOINT}"))
            .json(&json!({"password":"pw-secret"}))
            .send()
            .await
            .expect("send session issue request");
        assert_eq!(issue_response.status(), StatusCode::OK);
        let issue_payload = issue_response
            .json::<Value>()
            .await
            .expect("parse session payload");
        let session_token = issue_payload["access_token"]
            .as_str()
            .expect("access token present")
            .to_string();
        assert!(session_token.starts_with("tau_sess_"));

        let status_response = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth(session_token)
            .send()
            .await
            .expect("send status request");
        assert_eq!(status_response.status(), StatusCode::OK);

        handle.abort();
    }

    #[tokio::test]
    async fn functional_gateway_ws_endpoint_rejects_unauthorized_upgrade() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let error = connect_async(format!("ws://{addr}{GATEWAY_WS_ENDPOINT}"))
            .await
            .expect_err("websocket upgrade should reject missing auth");
        match error {
            tungstenite::Error::Http(response) => {
                assert_eq!(
                    response.status().as_u16(),
                    StatusCode::UNAUTHORIZED.as_u16()
                );
            }
            other => panic!("expected HTTP upgrade rejection, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn functional_gateway_ws_endpoint_supports_capabilities_and_ping_pong() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let mut socket = connect_gateway_ws(addr, Some("secret"))
            .await
            .expect("connect websocket");
        socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send capabilities frame");

        let response = recv_gateway_ws_json(&mut socket).await;
        assert_eq!(response["schema_version"], 1);
        assert_eq!(response["request_id"], "req-cap");
        assert_eq!(response["kind"], "capabilities.response");
        assert_eq!(response["payload"]["protocol_version"], "0.1.0");

        socket
            .send(ClientWsMessage::Ping(vec![7, 3, 1]))
            .await
            .expect("send ping");
        let pong = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let Some(message) = socket.next().await else {
                    panic!("websocket closed before pong");
                };
                let message = message.expect("read websocket frame");
                if let ClientWsMessage::Pong(payload) = message {
                    return payload;
                }
            }
        })
        .await
        .expect("pong should arrive before timeout");
        assert_eq!(pong.to_vec(), vec![7, 3, 1]);

        socket.close(None).await.expect("close websocket");
        handle.abort();
    }

    #[tokio::test]
    async fn integration_gateway_ws_session_status_and_reset_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let session_path = gateway_session_path(&state.config.state_dir, DEFAULT_SESSION_KEY);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session parent");
        }
        std::fs::write(
            &session_path,
            r#"{"id":"seed-1","role":"system","content":"seed"}"#,
        )
        .expect("seed session file");

        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");
        let mut socket = connect_gateway_ws(addr, Some("secret"))
            .await
            .expect("connect websocket");

        socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-status-before","kind":"session.status.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send status before");
        let before = recv_gateway_ws_json(&mut socket).await;
        assert_eq!(before["kind"], "session.status.response");
        assert_eq!(before["payload"]["session_key"], DEFAULT_SESSION_KEY);
        assert_eq!(before["payload"]["exists"], true);
        assert_eq!(before["payload"]["message_count"], 1);
        assert!(session_path.exists());

        socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-reset","kind":"session.reset.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send session reset");
        let reset = recv_gateway_ws_json(&mut socket).await;
        assert_eq!(reset["kind"], "session.reset.response");
        assert_eq!(reset["payload"]["session_key"], DEFAULT_SESSION_KEY);
        assert_eq!(reset["payload"]["reset"], true);
        assert!(!session_path.exists());

        socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-status-after","kind":"session.status.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send status after");
        let after = recv_gateway_ws_json(&mut socket).await;
        assert_eq!(after["kind"], "session.status.response");
        assert_eq!(after["payload"]["exists"], false);
        assert_eq!(after["payload"]["message_count"], 0);

        socket.close(None).await.expect("close websocket");
        handle.abort();
    }

    #[tokio::test]
    async fn regression_gateway_ws_malformed_frame_fails_closed_without_crashing_runtime() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let mut socket = connect_gateway_ws(addr, Some("secret"))
            .await
            .expect("connect websocket");
        socket
            .send(ClientWsMessage::Text("not-json".into()))
            .await
            .expect("send malformed frame");
        let malformed = recv_gateway_ws_json(&mut socket).await;
        assert_eq!(malformed["kind"], "error");
        assert_eq!(malformed["payload"]["code"], "invalid_json");

        socket
            .send(ClientWsMessage::Text(
                r#"{"schema_version":1,"request_id":"req-status","kind":"gateway.status.request","payload":{}}"#
                    .into(),
            ))
            .await
            .expect("send valid status frame");
        let status = recv_gateway_ws_json(&mut socket).await;
        assert_eq!(status["kind"], "gateway.status.response");
        assert_eq!(
            status["payload"]["gateway"]["ws_endpoint"],
            GATEWAY_WS_ENDPOINT
        );
        assert_eq!(
            status["payload"]["multi_channel"]["state_present"],
            Value::Bool(false)
        );

        socket.close(None).await.expect("close websocket");
        handle.abort();
    }

    #[tokio::test]
    async fn integration_openresponses_http_roundtrip_persists_session_state() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state.clone())
            .await
            .expect("spawn server");

        let client = Client::new();
        let response_one = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({
                "input": "first",
                "metadata": {"session_id": "http-integration"}
            }))
            .send()
            .await
            .expect("send first request")
            .json::<Value>()
            .await
            .expect("parse first response");
        let first_count = response_one["output_text"]
            .as_str()
            .unwrap_or_default()
            .trim_start_matches("messages=")
            .parse::<usize>()
            .expect("parse first count");

        let response_two = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({
                "input": "second",
                "metadata": {"session_id": "http-integration"}
            }))
            .send()
            .await
            .expect("send second request")
            .json::<Value>()
            .await
            .expect("parse second response");
        let second_count = response_two["output_text"]
            .as_str()
            .unwrap_or_default()
            .trim_start_matches("messages=")
            .parse::<usize>()
            .expect("parse second count");

        assert!(second_count > first_count);

        let session_path = gateway_session_path(
            &state.config.state_dir,
            &sanitize_session_key("http-integration"),
        );
        assert!(session_path.exists());
        let raw = std::fs::read_to_string(&session_path).expect("read session file");
        assert!(raw.lines().count() >= 4);

        handle.abort();
    }

    #[tokio::test]
    async fn integration_gateway_status_endpoint_returns_service_snapshot() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let payload = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth("secret")
            .send()
            .await
            .expect("send status request")
            .json::<Value>()
            .await
            .expect("parse status response");

        assert_eq!(
            payload["gateway"]["responses_endpoint"].as_str(),
            Some(OPENRESPONSES_ENDPOINT)
        );
        assert_eq!(
            payload["gateway"]["webchat_endpoint"].as_str(),
            Some(WEBCHAT_ENDPOINT)
        );
        assert_eq!(
            payload["gateway"]["status_endpoint"].as_str(),
            Some(GATEWAY_STATUS_ENDPOINT)
        );
        assert_eq!(
            payload["gateway"]["ws_endpoint"].as_str(),
            Some(GATEWAY_WS_ENDPOINT)
        );
        assert_eq!(
            payload["service"]["service_status"].as_str(),
            Some("running")
        );
        assert_eq!(
            payload["multi_channel"]["state_present"],
            Value::Bool(false)
        );
        assert_eq!(
            payload["multi_channel"]["health_state"],
            Value::String("unknown".to_string())
        );
        assert_eq!(
            payload["multi_channel"]["rollout_gate"],
            Value::String("hold".to_string())
        );
        assert_eq!(
            payload["multi_channel"]["connectors"]["state_present"],
            Value::Bool(false)
        );
        assert_eq!(
            payload["multi_channel"]["processed_event_count"],
            Value::Number(serde_json::Number::from(0))
        );

        handle.abort();
    }

    #[tokio::test]
    async fn integration_gateway_status_endpoint_returns_expanded_multi_channel_health_payload() {
        let temp = tempdir().expect("tempdir");
        write_multi_channel_runtime_fixture(temp.path(), true);
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let payload = Client::new()
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth("secret")
            .send()
            .await
            .expect("send status request")
            .json::<Value>()
            .await
            .expect("parse status response");

        assert_eq!(payload["multi_channel"]["state_present"], Value::Bool(true));
        assert_eq!(
            payload["multi_channel"]["health_state"],
            Value::String("degraded".to_string())
        );
        assert_eq!(
            payload["multi_channel"]["health_reason"],
            Value::String("connector retry in progress".to_string())
        );
        assert_eq!(
            payload["multi_channel"]["processed_event_count"],
            Value::Number(serde_json::Number::from(3))
        );
        assert_eq!(
            payload["multi_channel"]["transport_counts"]["telegram"],
            Value::Number(serde_json::Number::from(2))
        );
        assert_eq!(
            payload["multi_channel"]["transport_counts"]["discord"],
            Value::Number(serde_json::Number::from(1))
        );
        assert_eq!(
            payload["multi_channel"]["reason_code_counts"]["connector_retry"],
            Value::Number(serde_json::Number::from(2))
        );
        assert_eq!(
            payload["multi_channel"]["connectors"]["state_present"],
            Value::Bool(true)
        );
        assert_eq!(
            payload["multi_channel"]["connectors"]["channels"]["telegram"]["breaker_state"],
            Value::String("open".to_string())
        );
        assert_eq!(
            payload["multi_channel"]["connectors"]["channels"]["telegram"]["provider_failures"],
            Value::Number(serde_json::Number::from(2))
        );

        handle.abort();
    }

    #[test]
    fn regression_collect_gateway_multi_channel_status_report_defaults_when_state_is_missing() {
        let temp = tempdir().expect("tempdir");
        let gateway_state_dir = temp.path().join(".tau").join("gateway");
        std::fs::create_dir_all(&gateway_state_dir).expect("create gateway state dir");

        let report = collect_gateway_multi_channel_status_report(&gateway_state_dir);
        assert!(!report.state_present);
        assert_eq!(report.health_state, "unknown");
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.processed_event_count, 0);
        assert!(report.connectors.channels.is_empty());
        assert!(!report.connectors.state_present);
        assert!(report
            .diagnostics
            .iter()
            .any(|line| line.starts_with("state_missing:")));
    }

    #[tokio::test]
    async fn integration_localhost_dev_mode_allows_requests_without_bearer_token() {
        let temp = tempdir().expect("tempdir");
        let state = test_state_with_auth(
            temp.path(),
            10_000,
            GatewayOpenResponsesAuthMode::LocalhostDev,
            None,
            None,
            60,
            120,
        );
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}{OPENRESPONSES_ENDPOINT}"))
            .json(&json!({"input":"hello localhost mode"}))
            .send()
            .await
            .expect("send request");
        assert_eq!(response.status(), StatusCode::OK);

        handle.abort();
    }

    #[tokio::test]
    async fn regression_openresponses_endpoint_rejects_malformed_json_body() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .header("content-type", "application/json")
            .body("{invalid")
            .send()
            .await
            .expect("send malformed request");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        handle.abort();
    }

    #[tokio::test]
    async fn regression_openresponses_endpoint_rejects_oversized_input() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 8, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/v1/responses"))
            .bearer_auth("secret")
            .json(&json!({"input": "this request is too large"}))
            .send()
            .await
            .expect("send oversized request");

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        handle.abort();
    }

    #[tokio::test]
    async fn regression_gateway_auth_session_rejects_invalid_password() {
        let temp = tempdir().expect("tempdir");
        let state = test_state_with_auth(
            temp.path(),
            10_000,
            GatewayOpenResponsesAuthMode::PasswordSession,
            None,
            Some("pw-secret"),
            60,
            120,
        );
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}{GATEWAY_AUTH_SESSION_ENDPOINT}"))
            .json(&json!({"password":"wrong"}))
            .send()
            .await
            .expect("send request");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let payload = response.json::<Value>().await.expect("parse response");
        assert_eq!(
            payload["error"]["code"].as_str(),
            Some("invalid_credentials")
        );

        handle.abort();
    }

    #[tokio::test]
    async fn regression_gateway_password_session_token_expires_and_fails_closed() {
        let temp = tempdir().expect("tempdir");
        let state = Arc::new(GatewayOpenResponsesServerState::new(
            GatewayOpenResponsesServerConfig {
                client: Arc::new(MockGatewayLlmClient::default()),
                model: "openai/gpt-4o-mini".to_string(),
                system_prompt: "You are Tau.".to_string(),
                max_turns: 4,
                tool_registrar: Arc::new(NoopGatewayToolRegistrar),
                turn_timeout_ms: 0,
                session_lock_wait_ms: 500,
                session_lock_stale_ms: 10_000,
                state_dir: temp.path().join(".tau/gateway"),
                bind: "127.0.0.1:0".to_string(),
                auth_mode: GatewayOpenResponsesAuthMode::PasswordSession,
                auth_token: None,
                auth_password: Some("pw-secret".to_string()),
                session_ttl_seconds: 1,
                rate_limit_window_seconds: 60,
                rate_limit_max_requests: 120,
                max_input_chars: 10_000,
            },
        ));
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let issue_response = client
            .post(format!("http://{addr}{GATEWAY_AUTH_SESSION_ENDPOINT}"))
            .json(&json!({"password":"pw-secret"}))
            .send()
            .await
            .expect("issue session token");
        assert_eq!(issue_response.status(), StatusCode::OK);
        let token = issue_response
            .json::<Value>()
            .await
            .expect("parse issue response")["access_token"]
            .as_str()
            .expect("access token")
            .to_string();

        tokio::time::sleep(Duration::from_millis(1_100)).await;

        let status_response = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth(token)
            .send()
            .await
            .expect("send status request");
        assert_eq!(status_response.status(), StatusCode::UNAUTHORIZED);

        handle.abort();
    }

    #[tokio::test]
    async fn regression_gateway_rate_limit_rejects_excess_requests() {
        let temp = tempdir().expect("tempdir");
        let state = test_state_with_auth(
            temp.path(),
            10_000,
            GatewayOpenResponsesAuthMode::Token,
            Some("secret"),
            None,
            60,
            1,
        );
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let first = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth("secret")
            .send()
            .await
            .expect("first request");
        assert_eq!(first.status(), StatusCode::OK);

        let second = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .bearer_auth("secret")
            .send()
            .await
            .expect("second request");
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);

        handle.abort();
    }

    #[tokio::test]
    async fn regression_gateway_status_endpoint_rejects_unauthorized_request() {
        let temp = tempdir().expect("tempdir");
        let state = test_state(temp.path(), 10_000, "secret");
        let (addr, handle) = spawn_test_server(state).await.expect("spawn server");

        let client = Client::new();
        let response = client
            .get(format!("http://{addr}{GATEWAY_STATUS_ENDPOINT}"))
            .send()
            .await
            .expect("send request");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        handle.abort();
    }

    #[test]
    fn regression_validate_gateway_openresponses_bind_rejects_invalid_socket_address() {
        let error = validate_gateway_openresponses_bind("invalid-bind")
            .expect_err("invalid bind should fail");
        assert!(error
            .to_string()
            .contains("invalid gateway socket address 'invalid-bind'"));
    }
}
