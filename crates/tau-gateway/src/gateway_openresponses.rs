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

mod auth_runtime;
mod multi_channel_status;
#[cfg(test)]
mod tests;
mod webchat_page;
mod websocket;

use auth_runtime::{
    authorize_gateway_request, collect_gateway_auth_status_report, enforce_gateway_rate_limit,
    issue_gateway_session_token,
};
use multi_channel_status::collect_gateway_multi_channel_status_report;
use webchat_page::render_gateway_webchat_page;
use websocket::run_gateway_ws_connection;

const OPENRESPONSES_ENDPOINT: &str = "/v1/responses";
const WEBCHAT_ENDPOINT: &str = "/webchat";
const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
const GATEWAY_WS_ENDPOINT: &str = "/gateway/ws";
const GATEWAY_AUTH_SESSION_ENDPOINT: &str = "/gateway/auth/session";
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

#[cfg(test)]
fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}
