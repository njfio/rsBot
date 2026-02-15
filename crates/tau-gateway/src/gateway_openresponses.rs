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
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, StreamDeltaHandler};
use tau_core::{current_unix_timestamp, current_unix_timestamp_ms};
use tau_runtime::{
    inspect_runtime_heartbeat, start_runtime_heartbeat_scheduler, RuntimeHeartbeatSchedulerConfig,
    TransportHealthSnapshot,
};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::remote_profile::GatewayOpenResponsesAuthMode;

mod auth_runtime;
mod dashboard_status;
mod multi_channel_status;
mod openai_compat;
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
use openai_compat::{
    build_chat_completions_payload, build_chat_completions_stream_chunks,
    build_completions_payload, build_completions_stream_chunks, build_models_payload,
    translate_chat_completions_request, translate_completions_request,
    OpenAiChatCompletionsRequest, OpenAiCompletionsRequest,
};
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
const OPENAI_CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";
const OPENAI_COMPLETIONS_ENDPOINT: &str = "/v1/completions";
const OPENAI_MODELS_ENDPOINT: &str = "/v1/models";
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
const GATEWAY_EVENTS_INSPECT_QUEUE_LIMIT: usize = 64;
const GATEWAY_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS: u64 = 86_400;
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
    pub runtime_heartbeat: RuntimeHeartbeatSchedulerConfig,
}

#[derive(Clone)]
struct GatewayOpenResponsesServerState {
    config: GatewayOpenResponsesServerConfig,
    response_sequence: Arc<AtomicU64>,
    auth_runtime: Arc<Mutex<GatewayAuthRuntimeState>>,
    compat_runtime: Arc<Mutex<GatewayOpenAiCompatRuntimeState>>,
}

impl GatewayOpenResponsesServerState {
    fn new(config: GatewayOpenResponsesServerConfig) -> Self {
        Self {
            config,
            response_sequence: Arc::new(AtomicU64::new(0)),
            auth_runtime: Arc::new(Mutex::new(GatewayAuthRuntimeState::default())),
            compat_runtime: Arc::new(Mutex::new(GatewayOpenAiCompatRuntimeState::default())),
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

    fn record_openai_compat_request(&self, surface: GatewayOpenAiCompatSurface, stream: bool) {
        if let Ok(mut runtime) = self.compat_runtime.lock() {
            runtime.total_requests = runtime.total_requests.saturating_add(1);
            if stream {
                runtime.stream_requests = runtime.stream_requests.saturating_add(1);
            }
            match surface {
                GatewayOpenAiCompatSurface::ChatCompletions => {
                    runtime.chat_completions_requests =
                        runtime.chat_completions_requests.saturating_add(1);
                }
                GatewayOpenAiCompatSurface::Completions => {
                    runtime.completions_requests = runtime.completions_requests.saturating_add(1);
                }
                GatewayOpenAiCompatSurface::Models => {
                    runtime.models_requests = runtime.models_requests.saturating_add(1);
                }
            }
        }
    }

    fn record_openai_compat_reason(&self, reason_code: &str) {
        if reason_code.trim().is_empty() {
            return;
        }
        if let Ok(mut runtime) = self.compat_runtime.lock() {
            *runtime
                .reason_code_counts
                .entry(reason_code.to_string())
                .or_default() += 1;
            runtime.last_reason_codes.push(reason_code.to_string());
            if runtime.last_reason_codes.len() > 16 {
                let drop_count = runtime.last_reason_codes.len().saturating_sub(16);
                runtime.last_reason_codes.drain(0..drop_count);
            }
        }
    }

    fn record_openai_compat_ignored_fields(&self, fields: &[String]) {
        if fields.is_empty() {
            return;
        }
        if let Ok(mut runtime) = self.compat_runtime.lock() {
            for field in fields {
                if field.trim().is_empty() {
                    continue;
                }
                *runtime
                    .ignored_field_counts
                    .entry(field.clone())
                    .or_default() += 1;
            }
        }
    }

    fn collect_openai_compat_status_report(&self) -> GatewayOpenAiCompatStatusReport {
        if let Ok(runtime) = self.compat_runtime.lock() {
            return GatewayOpenAiCompatStatusReport {
                total_requests: runtime.total_requests,
                chat_completions_requests: runtime.chat_completions_requests,
                completions_requests: runtime.completions_requests,
                models_requests: runtime.models_requests,
                stream_requests: runtime.stream_requests,
                translation_failures: runtime.translation_failures,
                execution_failures: runtime.execution_failures,
                reason_code_counts: runtime.reason_code_counts.clone(),
                ignored_field_counts: runtime.ignored_field_counts.clone(),
                last_reason_codes: runtime.last_reason_codes.clone(),
            };
        }

        GatewayOpenAiCompatStatusReport::default()
    }

    fn increment_openai_compat_translation_failures(&self) {
        if let Ok(mut runtime) = self.compat_runtime.lock() {
            runtime.translation_failures = runtime.translation_failures.saturating_add(1);
        }
    }

    fn increment_openai_compat_execution_failures(&self) {
        if let Ok(mut runtime) = self.compat_runtime.lock() {
            runtime.execution_failures = runtime.execution_failures.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum GatewayOpenAiCompatSurface {
    ChatCompletions,
    Completions,
    Models,
}

#[derive(Debug, Clone, Default)]
struct GatewayOpenAiCompatRuntimeState {
    total_requests: u64,
    chat_completions_requests: u64,
    completions_requests: u64,
    models_requests: u64,
    stream_requests: u64,
    translation_failures: u64,
    execution_failures: u64,
    reason_code_counts: BTreeMap<String, u64>,
    ignored_field_counts: BTreeMap<String, u64>,
    last_reason_codes: Vec<String>,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
struct GatewayOpenAiCompatStatusReport {
    total_requests: u64,
    chat_completions_requests: u64,
    completions_requests: u64,
    models_requests: u64,
    stream_requests: u64,
    translation_failures: u64,
    execution_failures: u64,
    reason_code_counts: BTreeMap<String, u64>,
    ignored_field_counts: BTreeMap<String, u64>,
    last_reason_codes: Vec<String>,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct GatewayEventsStatusReport {
    state_present: bool,
    events_dir: String,
    state_path: String,
    health_state: String,
    rollout_gate: String,
    reason_code: String,
    health_reason: String,
    discovered_events: usize,
    enabled_events: usize,
    due_now_events: usize,
    queued_now_events: usize,
    not_due_events: usize,
    stale_immediate_events: usize,
    malformed_events: usize,
    due_eval_failed_events: usize,
    execution_history_entries: usize,
    executed_history_entries: usize,
    failed_history_entries: usize,
    skipped_history_entries: usize,
    last_execution_unix_ms: Option<u64>,
    last_execution_reason_code: Option<String>,
    diagnostics: Vec<String>,
}

impl Default for GatewayEventsStatusReport {
    fn default() -> Self {
        Self {
            state_present: false,
            events_dir: String::new(),
            state_path: String::new(),
            health_state: "unknown".to_string(),
            rollout_gate: "hold".to_string(),
            reason_code: "events_status_unavailable".to_string(),
            health_reason: "events scheduler status is unavailable".to_string(),
            discovered_events: 0,
            enabled_events: 0,
            due_now_events: 0,
            queued_now_events: 0,
            not_due_events: 0,
            stale_immediate_events: 0,
            malformed_events: 0,
            due_eval_failed_events: 0,
            execution_history_entries: 0,
            executed_history_entries: 0,
            failed_history_entries: 0,
            skipped_history_entries: 0,
            last_execution_unix_ms: None,
            last_execution_reason_code: None,
            diagnostics: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GatewayEventDefinition {
    id: String,
    channel: String,
    schedule: GatewayEventSchedule,
    #[serde(default = "default_gateway_event_enabled")]
    enabled: bool,
    #[serde(default)]
    created_unix_ms: Option<u64>,
}

fn default_gateway_event_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum GatewayEventSchedule {
    Immediate,
    At { at_unix_ms: u64 },
    Periodic { cron: String, timezone: String },
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayEventsStateFile {
    #[serde(default)]
    recent_executions: Vec<GatewayEventExecutionRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct GatewayEventExecutionRecord {
    timestamp_unix_ms: u64,
    outcome: String,
    reason_code: String,
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
    let mut runtime_heartbeat_handle =
        start_runtime_heartbeat_scheduler(config.runtime_heartbeat.clone())?;

    println!(
        "gateway openresponses server listening: endpoint={} addr={} state_dir={}",
        OPENRESPONSES_ENDPOINT,
        local_addr,
        config.state_dir.display()
    );

    let state_dir = config.state_dir.clone();
    let state = Arc::new(GatewayOpenResponsesServerState::new(config));
    let app = build_gateway_openresponses_router(state);
    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    runtime_heartbeat_handle.shutdown().await;
    serve_result.context("gateway openresponses server exited unexpectedly")?;

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
            OPENAI_CHAT_COMPLETIONS_ENDPOINT,
            post(handle_openai_chat_completions),
        )
        .route(OPENAI_COMPLETIONS_ENDPOINT, post(handle_openai_completions))
        .route(OPENAI_MODELS_ENDPOINT, get(handle_openai_models))
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
    let events_report = collect_gateway_events_status_report(&state.config.state_dir);
    let dashboard_snapshot = collect_gateway_dashboard_snapshot(&state.config.state_dir);
    let runtime_heartbeat = inspect_runtime_heartbeat(&state.config.runtime_heartbeat.state_path);

    (
        StatusCode::OK,
        Json(json!({
            "service": service_report,
            "auth": collect_gateway_auth_status_report(&state),
            "multi_channel": multi_channel_report,
            "events": events_report,
            "training": dashboard_snapshot.training,
            "runtime_heartbeat": runtime_heartbeat,
            "gateway": {
                "responses_endpoint": OPENRESPONSES_ENDPOINT,
                "openai_compat": {
                    "chat_completions_endpoint": OPENAI_CHAT_COMPLETIONS_ENDPOINT,
                    "completions_endpoint": OPENAI_COMPLETIONS_ENDPOINT,
                    "models_endpoint": OPENAI_MODELS_ENDPOINT,
                    "runtime": state.collect_openai_compat_status_report(),
                },
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

fn collect_gateway_events_status_report(gateway_state_dir: &Path) -> GatewayEventsStatusReport {
    let tau_root = gateway_state_dir.parent().unwrap_or(gateway_state_dir);
    let events_dir = tau_root.join("events");
    let state_path = events_dir.join("state.json");
    let events_dir_exists = events_dir.is_dir();
    let state_present = state_path.is_file();

    if !events_dir_exists && !state_present {
        return GatewayEventsStatusReport {
            state_present: false,
            events_dir: events_dir.display().to_string(),
            state_path: state_path.display().to_string(),
            health_state: "healthy".to_string(),
            rollout_gate: "pass".to_string(),
            reason_code: "events_not_configured".to_string(),
            health_reason: "events scheduler is not configured".to_string(),
            diagnostics: vec![
                "create event definitions under events_dir to enable routine scheduling"
                    .to_string(),
            ],
            ..GatewayEventsStatusReport::default()
        };
    }

    let state = if state_present {
        match std::fs::read_to_string(&state_path) {
            Ok(payload) => match serde_json::from_str::<GatewayEventsStateFile>(&payload) {
                Ok(parsed) => Some(parsed),
                Err(error) => {
                    return GatewayEventsStatusReport {
                        state_present,
                        events_dir: events_dir.display().to_string(),
                        state_path: state_path.display().to_string(),
                        health_state: "failing".to_string(),
                        rollout_gate: "hold".to_string(),
                        reason_code: "events_state_parse_failed".to_string(),
                        health_reason: "failed to parse events state payload".to_string(),
                        diagnostics: vec![error.to_string()],
                        ..GatewayEventsStatusReport::default()
                    };
                }
            },
            Err(error) => {
                return GatewayEventsStatusReport {
                    state_present,
                    events_dir: events_dir.display().to_string(),
                    state_path: state_path.display().to_string(),
                    health_state: "failing".to_string(),
                    rollout_gate: "hold".to_string(),
                    reason_code: "events_state_read_failed".to_string(),
                    health_reason: "failed to read events state payload".to_string(),
                    diagnostics: vec![error.to_string()],
                    ..GatewayEventsStatusReport::default()
                };
            }
        }
    } else {
        None
    };

    let mut discovered_events = 0usize;
    let mut enabled_events = 0usize;
    let mut due_now_events = 0usize;
    let mut not_due_events = 0usize;
    let mut stale_immediate_events = 0usize;
    let mut malformed_events = 0usize;
    let due_eval_failed_events = 0usize;
    let now_unix_ms = current_unix_timestamp_ms();

    if events_dir_exists {
        let entries = match std::fs::read_dir(&events_dir) {
            Ok(entries) => entries,
            Err(error) => {
                return GatewayEventsStatusReport {
                    state_present,
                    events_dir: events_dir.display().to_string(),
                    state_path: state_path.display().to_string(),
                    health_state: "failing".to_string(),
                    rollout_gate: "hold".to_string(),
                    reason_code: "events_dir_read_failed".to_string(),
                    health_reason: "failed to read events definitions directory".to_string(),
                    diagnostics: vec![error.to_string()],
                    ..GatewayEventsStatusReport::default()
                };
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => {
                    malformed_events = malformed_events.saturating_add(1);
                    continue;
                }
            };
            let path = entry.path();
            if path == state_path {
                continue;
            }
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let payload = match std::fs::read_to_string(&path) {
                Ok(payload) => payload,
                Err(_) => {
                    malformed_events = malformed_events.saturating_add(1);
                    continue;
                }
            };
            let definition = match serde_json::from_str::<GatewayEventDefinition>(&payload) {
                Ok(definition) => definition,
                Err(_) => {
                    malformed_events = malformed_events.saturating_add(1);
                    continue;
                }
            };
            let _ = (&definition.id, &definition.channel);
            discovered_events = discovered_events.saturating_add(1);
            if definition.enabled {
                enabled_events = enabled_events.saturating_add(1);
            } else {
                not_due_events = not_due_events.saturating_add(1);
                continue;
            }

            match definition.schedule {
                GatewayEventSchedule::Immediate => {
                    let created = definition.created_unix_ms.unwrap_or(now_unix_ms);
                    let max_age_ms =
                        GATEWAY_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS.saturating_mul(1_000);
                    if GATEWAY_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS > 0
                        && now_unix_ms.saturating_sub(created) > max_age_ms
                    {
                        stale_immediate_events = stale_immediate_events.saturating_add(1);
                    } else {
                        due_now_events = due_now_events.saturating_add(1);
                    }
                }
                GatewayEventSchedule::At { at_unix_ms } => {
                    if now_unix_ms >= at_unix_ms {
                        due_now_events = due_now_events.saturating_add(1);
                    } else {
                        not_due_events = not_due_events.saturating_add(1);
                    }
                }
                GatewayEventSchedule::Periodic { cron, timezone } => {
                    let _ = (cron, timezone);
                    not_due_events = not_due_events.saturating_add(1);
                }
            }
        }
    }

    let queued_now_events = due_now_events.min(GATEWAY_EVENTS_INSPECT_QUEUE_LIMIT.max(1));
    let executions = state
        .as_ref()
        .map(|value| value.recent_executions.clone())
        .unwrap_or_default();
    let execution_history_entries = executions.len();
    let executed_history_entries = executions
        .iter()
        .filter(|entry| entry.outcome == "executed")
        .count();
    let failed_history_entries = executions
        .iter()
        .filter(|entry| entry.outcome == "failed")
        .count();
    let skipped_history_entries = executions
        .iter()
        .filter(|entry| entry.outcome == "skipped")
        .count();
    let last_execution_unix_ms = executions.last().map(|entry| entry.timestamp_unix_ms);
    let last_execution_reason_code = executions.last().map(|entry| entry.reason_code.clone());

    let mut health_state = "healthy".to_string();
    let mut rollout_gate = "pass".to_string();
    let mut reason_code = "events_ready".to_string();
    let mut health_reason = "events scheduler diagnostics are healthy".to_string();
    let mut diagnostics = Vec::new();

    if discovered_events == 0 {
        reason_code = "events_none_discovered".to_string();
        health_reason = "events directory is configured but contains no definitions".to_string();
        diagnostics.push("add event definition files to enable scheduled routines".to_string());
    }
    if malformed_events > 0 {
        health_state = "degraded".to_string();
        rollout_gate = "hold".to_string();
        reason_code = "events_malformed_definitions".to_string();
        health_reason = format!(
            "events inspect found {} malformed definition files",
            malformed_events
        );
        diagnostics
            .push("run --events-validate to repair malformed event definition files".to_string());
    }
    if failed_history_entries > 0 {
        health_state = "degraded".to_string();
        rollout_gate = "hold".to_string();
        reason_code = "events_recent_failures".to_string();
        health_reason = format!(
            "events execution history includes {} failed runs",
            failed_history_entries
        );
        diagnostics.push(
            "inspect channel-store logs and recent execution history for failing routines"
                .to_string(),
        );
    }

    GatewayEventsStatusReport {
        state_present,
        events_dir: events_dir.display().to_string(),
        state_path: state_path.display().to_string(),
        health_state,
        rollout_gate,
        reason_code,
        health_reason,
        discovered_events,
        enabled_events,
        due_now_events,
        queued_now_events,
        not_due_events,
        stale_immediate_events,
        malformed_events,
        due_eval_failed_events,
        execution_history_entries,
        executed_history_entries,
        failed_history_entries,
        skipped_history_entries,
        last_execution_unix_ms,
        last_execution_reason_code,
        diagnostics,
    }
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
            "training": snapshot.training,
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
            "training": snapshot.training,
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
            "training": snapshot.training,
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
            "training": snapshot.training,
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

async fn handle_openai_chat_completions(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    state.record_openai_compat_reason("openai_chat_completions_request_received");

    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        state.increment_openai_compat_translation_failures();
        state.record_openai_compat_reason("openai_chat_completions_body_too_large");
        return error.into_response();
    }

    let request = match parse_gateway_json_body::<OpenAiChatCompletionsRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            state.increment_openai_compat_translation_failures();
            state.record_openai_compat_reason("openai_chat_completions_malformed_json");
            return error.into_response();
        }
    };

    let translated = match translate_chat_completions_request(request) {
        Ok(translated) => translated,
        Err(error) => {
            state.increment_openai_compat_translation_failures();
            state.record_openai_compat_reason("openai_chat_completions_translation_failed");
            return error.into_response();
        }
    };

    state.record_openai_compat_request(
        GatewayOpenAiCompatSurface::ChatCompletions,
        translated.stream,
    );

    if translated
        .requested_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        state.record_openai_compat_reason("openai_chat_completions_model_override_ignored");
    }
    state.record_openai_compat_ignored_fields(&translated.ignored_fields);

    if translated.stream {
        return stream_openai_chat_completions(
            state,
            translated.request,
            translated.ignored_fields,
        )
        .await;
    }

    match execute_openresponses_request(state.clone(), translated.request, None).await {
        Ok(result) => {
            let mut ignored_fields = translated.ignored_fields;
            ignored_fields.extend(result.response.ignored_fields.clone());
            if !ignored_fields.is_empty() {
                state.record_openai_compat_reason("openai_chat_completions_ignored_fields");
            }
            state.record_openai_compat_ignored_fields(&ignored_fields);
            state.record_openai_compat_reason("openai_chat_completions_succeeded");
            (
                StatusCode::OK,
                Json(build_chat_completions_payload(&result.response)),
            )
                .into_response()
        }
        Err(error) => {
            state.increment_openai_compat_execution_failures();
            state.record_openai_compat_reason("openai_chat_completions_execution_failed");
            error.into_response()
        }
    }
}

async fn handle_openai_completions(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    state.record_openai_compat_reason("openai_completions_request_received");

    if let Err(error) = validate_gateway_request_body_size(&state, &body) {
        state.increment_openai_compat_translation_failures();
        state.record_openai_compat_reason("openai_completions_body_too_large");
        return error.into_response();
    }

    let request = match parse_gateway_json_body::<OpenAiCompletionsRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            state.increment_openai_compat_translation_failures();
            state.record_openai_compat_reason("openai_completions_malformed_json");
            return error.into_response();
        }
    };

    let translated = match translate_completions_request(request) {
        Ok(translated) => translated,
        Err(error) => {
            state.increment_openai_compat_translation_failures();
            state.record_openai_compat_reason("openai_completions_translation_failed");
            return error.into_response();
        }
    };

    state.record_openai_compat_request(GatewayOpenAiCompatSurface::Completions, translated.stream);

    if translated
        .requested_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        state.record_openai_compat_reason("openai_completions_model_override_ignored");
    }
    state.record_openai_compat_ignored_fields(&translated.ignored_fields);

    if translated.stream {
        return stream_openai_completions(state, translated.request, translated.ignored_fields)
            .await;
    }

    match execute_openresponses_request(state.clone(), translated.request, None).await {
        Ok(result) => {
            let mut ignored_fields = translated.ignored_fields;
            ignored_fields.extend(result.response.ignored_fields.clone());
            if !ignored_fields.is_empty() {
                state.record_openai_compat_reason("openai_completions_ignored_fields");
            }
            state.record_openai_compat_ignored_fields(&ignored_fields);
            state.record_openai_compat_reason("openai_completions_succeeded");
            (
                StatusCode::OK,
                Json(build_completions_payload(&result.response)),
            )
                .into_response()
        }
        Err(error) => {
            state.increment_openai_compat_execution_failures();
            state.record_openai_compat_reason("openai_completions_execution_failed");
            error.into_response()
        }
    }
}

async fn handle_openai_models(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    state.record_openai_compat_request(GatewayOpenAiCompatSurface::Models, false);
    state.record_openai_compat_reason("openai_models_listed");

    let payload = build_models_payload(&state.config.model, current_unix_timestamp());
    (StatusCode::OK, Json(payload)).into_response()
}

fn authorize_and_enforce_gateway_limits(
    state: &Arc<GatewayOpenResponsesServerState>,
    headers: &HeaderMap,
) -> Result<String, OpenResponsesApiError> {
    let principal = authorize_gateway_request(state, headers)?;
    enforce_gateway_rate_limit(state, principal.as_str())?;
    Ok(principal)
}

fn validate_gateway_request_body_size(
    state: &Arc<GatewayOpenResponsesServerState>,
    body: &Bytes,
) -> Result<(), OpenResponsesApiError> {
    let body_limit = state
        .config
        .max_input_chars
        .saturating_mul(INPUT_BODY_SIZE_MULTIPLIER)
        .max(state.config.max_input_chars);
    if body.len() > body_limit {
        return Err(OpenResponsesApiError::payload_too_large(format!(
            "request body exceeds max size of {} bytes",
            body_limit
        )));
    }
    Ok(())
}

fn parse_gateway_json_body<T: DeserializeOwned>(body: &Bytes) -> Result<T, OpenResponsesApiError> {
    serde_json::from_slice::<T>(body).map_err(|error| {
        OpenResponsesApiError::bad_request(
            "malformed_json",
            format!("failed to parse request body: {error}"),
        )
    })
}

async fn stream_openai_chat_completions(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
    compat_ignored_fields: Vec<String>,
) -> Response {
    let (tx, rx) = mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        match execute_openresponses_request(state.clone(), request, None).await {
            Ok(result) => {
                let mut ignored_fields = compat_ignored_fields;
                ignored_fields.extend(result.response.ignored_fields.clone());
                if !ignored_fields.is_empty() {
                    state.record_openai_compat_reason(
                        "openai_chat_completions_stream_ignored_fields",
                    );
                }
                state.record_openai_compat_ignored_fields(&ignored_fields);
                for chunk in build_chat_completions_stream_chunks(&result.response) {
                    let _ = tx.send(Event::default().data(chunk.to_string()));
                }
                let _ = tx.send(Event::default().data("[DONE]"));
                state.record_openai_compat_reason("openai_chat_completions_stream_succeeded");
            }
            Err(error) => {
                state.increment_openai_compat_execution_failures();
                state.record_openai_compat_reason("openai_chat_completions_stream_failed");
                let _ = tx.send(
                    Event::default().data(
                        json!({
                            "error": {
                                "type": "server_error",
                                "code": error.code,
                                "message": error.message,
                            }
                        })
                        .to_string(),
                    ),
                );
                let _ = tx.send(Event::default().data("[DONE]"));
            }
        }
    });

    let stream = UnboundedReceiverStream::new(rx).map(Ok::<Event, Infallible>);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn stream_openai_completions(
    state: Arc<GatewayOpenResponsesServerState>,
    request: OpenResponsesRequest,
    compat_ignored_fields: Vec<String>,
) -> Response {
    let (tx, rx) = mpsc::unbounded_channel::<Event>();
    tokio::spawn(async move {
        match execute_openresponses_request(state.clone(), request, None).await {
            Ok(result) => {
                let mut ignored_fields = compat_ignored_fields;
                ignored_fields.extend(result.response.ignored_fields.clone());
                if !ignored_fields.is_empty() {
                    state.record_openai_compat_reason("openai_completions_stream_ignored_fields");
                }
                state.record_openai_compat_ignored_fields(&ignored_fields);
                for chunk in build_completions_stream_chunks(&result.response) {
                    let _ = tx.send(Event::default().data(chunk.to_string()));
                }
                let _ = tx.send(Event::default().data("[DONE]"));
                state.record_openai_compat_reason("openai_completions_stream_succeeded");
            }
            Err(error) => {
                state.increment_openai_compat_execution_failures();
                state.record_openai_compat_reason("openai_completions_stream_failed");
                let _ = tx.send(
                    Event::default().data(
                        json!({
                            "error": {
                                "type": "server_error",
                                "code": error.code,
                                "message": error.message,
                            }
                        })
                        .to_string(),
                    ),
                );
                let _ = tx.send(Event::default().data("[DONE]"));
            }
        }
    });

    let stream = UnboundedReceiverStream::new(rx).map(Ok::<Event, Infallible>);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
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
