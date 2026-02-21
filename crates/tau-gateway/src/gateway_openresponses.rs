//! OpenResponses-compatible gateway server and request flow handlers.

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
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{
    default_safety_rule_set, scan_safety_rules, validate_safety_rule_set, Agent, AgentConfig,
    AgentEvent, Cortex, CortexConfig, SafetyMode, SafetyPolicy, SafetyRuleSet,
};
#[cfg(test)]
use tau_ai::MessageRole;
use tau_ai::{LlmClient, StreamDeltaHandler};
use tau_core::{current_unix_timestamp, current_unix_timestamp_ms, write_text_atomic};
use tau_dashboard_ui::TauOpsDashboardRoute;
use tau_runtime::{
    inspect_runtime_heartbeat, start_runtime_heartbeat_scheduler, ExternalCodingAgentBridge,
    ExternalCodingAgentBridgeConfig, RuntimeHeartbeatSchedulerConfig, TransportHealthSnapshot,
};
#[cfg(test)]
use tau_session::SessionStore;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::remote_profile::GatewayOpenResponsesAuthMode;

mod audit_runtime;
mod auth_runtime;
mod channel_telemetry_runtime;
mod compat_state_runtime;
mod config_runtime;
mod cortex_bulletin_runtime;
mod cortex_runtime;
mod dashboard_runtime;
mod dashboard_shell_page;
mod dashboard_status;
mod deploy_runtime;
mod events_status;
mod external_agent_runtime;
mod jobs_runtime;
mod memory_runtime;
mod multi_channel_status;
mod openai_compat;
mod openai_compat_runtime;
mod ops_dashboard_shell;
mod ops_shell_controls;
mod request_translation;
mod safety_runtime;
mod session_api_runtime;
mod session_runtime;
mod status_runtime;
#[cfg(test)]
mod tests;
mod tools_runtime;
mod training_runtime;
mod types;
mod webchat_page;
mod websocket;

use audit_runtime::{handle_gateway_audit_log, handle_gateway_audit_summary};
use auth_runtime::{
    authorize_gateway_request, collect_gateway_auth_status_report, enforce_gateway_rate_limit,
    issue_gateway_session_token,
};
use channel_telemetry_runtime::{
    handle_gateway_channel_lifecycle_action, handle_gateway_ui_telemetry,
};
use compat_state_runtime::{
    GatewayOpenAiCompatRuntimeState, GatewayOpenAiCompatSurface, GatewayUiTelemetryRuntimeState,
};
use config_runtime::{handle_gateway_config_get, handle_gateway_config_patch};
use cortex_bulletin_runtime::start_cortex_bulletin_runtime;
use cortex_runtime::{
    handle_cortex_chat, handle_cortex_status, record_cortex_external_followup_event,
    record_cortex_external_progress_event, record_cortex_external_session_closed,
    record_cortex_external_session_opened, record_cortex_memory_entry_delete_event,
    record_cortex_memory_entry_write_event, record_cortex_memory_write_event,
    record_cortex_session_append_event, record_cortex_session_reset_event,
};
use dashboard_runtime::{
    authorize_dashboard_request, handle_dashboard_action, handle_dashboard_alerts,
    handle_dashboard_health, handle_dashboard_queue_timeline, handle_dashboard_stream,
    handle_dashboard_widgets, handle_gateway_training_status,
};
use dashboard_shell_page::render_gateway_dashboard_shell_page;
use dashboard_status::{
    apply_gateway_dashboard_action, collect_gateway_dashboard_snapshot,
    collect_tau_ops_dashboard_command_center_snapshot, GatewayDashboardActionRequest,
};
use deploy_runtime::{handle_gateway_agent_stop, handle_gateway_deploy};
use events_status::collect_gateway_events_status_report;
use external_agent_runtime::{
    handle_external_coding_agent_open_session, handle_external_coding_agent_reap,
    handle_external_coding_agent_session_close, handle_external_coding_agent_session_detail,
    handle_external_coding_agent_session_followup,
    handle_external_coding_agent_session_followups_drain,
    handle_external_coding_agent_session_progress, handle_external_coding_agent_session_stream,
};
use jobs_runtime::{handle_gateway_job_cancel, handle_gateway_jobs_list};
use memory_runtime::{
    gateway_memory_store, gateway_memory_stores_root, handle_api_memories_graph,
    handle_gateway_memory_entry_delete, handle_gateway_memory_entry_read,
    handle_gateway_memory_entry_write, handle_gateway_memory_graph, handle_gateway_memory_read,
    handle_gateway_memory_write,
};
use multi_channel_status::collect_gateway_multi_channel_status_report;
#[cfg(test)]
use openai_compat::{translate_chat_completions_request, OpenAiChatCompletionsRequest};
use openai_compat_runtime::{
    handle_openai_chat_completions, handle_openai_completions, handle_openai_models,
};
use ops_dashboard_shell::{
    handle_ops_dashboard_chat_new, handle_ops_dashboard_chat_send,
    handle_ops_dashboard_memory_create, handle_ops_dashboard_session_detail_reset,
    handle_ops_dashboard_sessions_branch, render_tau_ops_dashboard_shell_for_route,
    resolve_tau_ops_dashboard_auth_mode,
};
use ops_shell_controls::OpsShellControlsQuery;
use request_translation::{sanitize_session_key, translate_openresponses_request};
use safety_runtime::{
    handle_gateway_safety_policy_get, handle_gateway_safety_policy_put,
    handle_gateway_safety_rules_get, handle_gateway_safety_rules_put, handle_gateway_safety_test,
};
use session_api_runtime::{
    handle_gateway_session_append, handle_gateway_session_detail, handle_gateway_session_reset,
    handle_gateway_sessions_list,
};
use session_runtime::{
    collect_assistant_reply, gateway_session_path, initialize_gateway_session_runtime,
    persist_messages, persist_session_usage_delta,
};
use status_runtime::handle_gateway_status;
use tools_runtime::{handle_gateway_tools_inventory, handle_gateway_tools_stats};
use training_runtime::{handle_gateway_training_config_patch, handle_gateway_training_rollouts};
use types::{
    GatewayAuthBootstrapResponse, GatewayAuthSessionRequest, GatewayAuthSessionResponse,
    GatewayMemoryEntryDeleteRequest, GatewayMemoryEntryUpsertRequest, GatewayMemoryGraphEdge,
    GatewayMemoryGraphFilterSummary, GatewayMemoryGraphNode, GatewayMemoryGraphQuery,
    GatewayMemoryGraphResponse, GatewayMemoryReadQuery, GatewayMemoryUpdateRequest,
    GatewaySafetyPolicyUpdateRequest, GatewaySafetyRulesUpdateRequest, GatewaySafetyTestRequest,
    OpenResponsesApiError, OpenResponsesExecutionResult, OpenResponsesOutputItem,
    OpenResponsesOutputTextItem, OpenResponsesPrompt, OpenResponsesRequest, OpenResponsesResponse,
    OpenResponsesUsage, OpenResponsesUsageSummary, SseFrame,
};
use webchat_page::render_gateway_webchat_page;
use websocket::run_gateway_ws_connection;

const OPENRESPONSES_ENDPOINT: &str = "/v1/responses";
const OPENAI_CHAT_COMPLETIONS_ENDPOINT: &str = "/v1/chat/completions";
const OPENAI_COMPLETIONS_ENDPOINT: &str = "/v1/completions";
const OPENAI_MODELS_ENDPOINT: &str = "/v1/models";
const OPS_DASHBOARD_ENDPOINT: &str = "/ops";
const OPS_DASHBOARD_AGENTS_ENDPOINT: &str = "/ops/agents";
const OPS_DASHBOARD_AGENT_DETAIL_ENDPOINT: &str = "/ops/agents/{agent_id}";
const OPS_DASHBOARD_CHAT_ENDPOINT: &str = "/ops/chat";
const OPS_DASHBOARD_CHAT_NEW_ENDPOINT: &str = "/ops/chat/new";
const OPS_DASHBOARD_CHAT_SEND_ENDPOINT: &str = "/ops/chat/send";
const OPS_DASHBOARD_SESSIONS_ENDPOINT: &str = "/ops/sessions";
const OPS_DASHBOARD_SESSION_DETAIL_ENDPOINT: &str = "/ops/sessions/{session_key}";
const OPS_DASHBOARD_MEMORY_ENDPOINT: &str = "/ops/memory";
const OPS_DASHBOARD_MEMORY_GRAPH_ENDPOINT: &str = "/ops/memory-graph";
const OPS_DASHBOARD_TOOLS_JOBS_ENDPOINT: &str = "/ops/tools-jobs";
const OPS_DASHBOARD_CHANNELS_ENDPOINT: &str = "/ops/channels";
const OPS_DASHBOARD_CONFIG_ENDPOINT: &str = "/ops/config";
const OPS_DASHBOARD_TRAINING_ENDPOINT: &str = "/ops/training";
const OPS_DASHBOARD_SAFETY_ENDPOINT: &str = "/ops/safety";
const OPS_DASHBOARD_DIAGNOSTICS_ENDPOINT: &str = "/ops/diagnostics";
const OPS_DASHBOARD_DEPLOY_ENDPOINT: &str = "/ops/deploy";
const OPS_DASHBOARD_LOGIN_ENDPOINT: &str = "/ops/login";
const DASHBOARD_SHELL_ENDPOINT: &str = "/dashboard";
const WEBCHAT_ENDPOINT: &str = "/webchat";
const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
const GATEWAY_WS_ENDPOINT: &str = "/gateway/ws";
const GATEWAY_AUTH_BOOTSTRAP_ENDPOINT: &str = "/gateway/auth/bootstrap";
const GATEWAY_AUTH_SESSION_ENDPOINT: &str = "/gateway/auth/session";
const GATEWAY_SESSIONS_ENDPOINT: &str = "/gateway/sessions";
const GATEWAY_SESSION_DETAIL_ENDPOINT: &str = "/gateway/sessions/{session_key}";
const GATEWAY_SESSION_APPEND_ENDPOINT: &str = "/gateway/sessions/{session_key}/append";
const GATEWAY_SESSION_RESET_ENDPOINT: &str = "/gateway/sessions/{session_key}/reset";
const GATEWAY_MEMORY_ENDPOINT: &str = "/gateway/memory/{session_key}";
const GATEWAY_MEMORY_ENTRY_ENDPOINT: &str = "/gateway/memory/{session_key}/{entry_id}";
const GATEWAY_MEMORY_GRAPH_ENDPOINT: &str = "/gateway/memory-graph/{session_key}";
const API_MEMORIES_GRAPH_ENDPOINT: &str = "/api/memories/graph";
const GATEWAY_CHANNEL_LIFECYCLE_ENDPOINT: &str = "/gateway/channels/{channel}/lifecycle";
const GATEWAY_CONFIG_ENDPOINT: &str = "/gateway/config";
const GATEWAY_SAFETY_POLICY_ENDPOINT: &str = "/gateway/safety/policy";
const GATEWAY_SAFETY_RULES_ENDPOINT: &str = "/gateway/safety/rules";
const GATEWAY_SAFETY_TEST_ENDPOINT: &str = "/gateway/safety/test";
const GATEWAY_AUDIT_SUMMARY_ENDPOINT: &str = "/gateway/audit/summary";
const GATEWAY_AUDIT_LOG_ENDPOINT: &str = "/gateway/audit/log";
const GATEWAY_TRAINING_STATUS_ENDPOINT: &str = "/gateway/training/status";
const GATEWAY_TRAINING_ROLLOUTS_ENDPOINT: &str = "/gateway/training/rollouts";
const GATEWAY_TRAINING_CONFIG_ENDPOINT: &str = "/gateway/training/config";
const GATEWAY_TOOLS_ENDPOINT: &str = "/gateway/tools";
const GATEWAY_TOOLS_STATS_ENDPOINT: &str = "/gateway/tools/stats";
const GATEWAY_JOBS_ENDPOINT: &str = "/gateway/jobs";
const GATEWAY_JOB_CANCEL_ENDPOINT_TEMPLATE: &str = "/gateway/jobs/{job_id}/cancel";
const GATEWAY_DEPLOY_ENDPOINT: &str = "/gateway/deploy";
const GATEWAY_AGENT_STOP_ENDPOINT_TEMPLATE: &str = "/gateway/agents/{agent_id}/stop";
const GATEWAY_UI_TELEMETRY_ENDPOINT: &str = "/gateway/ui/telemetry";
const CORTEX_CHAT_ENDPOINT: &str = "/cortex/chat";
const CORTEX_STATUS_ENDPOINT: &str = "/cortex/status";
const DASHBOARD_HEALTH_ENDPOINT: &str = "/dashboard/health";
const DASHBOARD_WIDGETS_ENDPOINT: &str = "/dashboard/widgets";
const DASHBOARD_QUEUE_TIMELINE_ENDPOINT: &str = "/dashboard/queue-timeline";
const DASHBOARD_ALERTS_ENDPOINT: &str = "/dashboard/alerts";
const DASHBOARD_ACTIONS_ENDPOINT: &str = "/dashboard/actions";
const DASHBOARD_STREAM_ENDPOINT: &str = "/dashboard/stream";
const EXTERNAL_CODING_AGENT_SESSIONS_ENDPOINT: &str = "/gateway/external-coding-agent/sessions";
const EXTERNAL_CODING_AGENT_SESSION_DETAIL_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}";
const EXTERNAL_CODING_AGENT_SESSION_PROGRESS_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/progress";
const EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/followups";
const EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_DRAIN_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/followups/drain";
const EXTERNAL_CODING_AGENT_SESSION_STREAM_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/stream";
const EXTERNAL_CODING_AGENT_SESSION_CLOSE_ENDPOINT: &str =
    "/gateway/external-coding-agent/sessions/{session_id}/close";
const EXTERNAL_CODING_AGENT_REAP_ENDPOINT: &str = "/gateway/external-coding-agent/reap";
const DEFAULT_SESSION_KEY: &str = "default";
const INPUT_BODY_SIZE_MULTIPLIER: usize = 8;
const GATEWAY_WS_HEARTBEAT_REQUEST_ID: &str = "gateway-heartbeat";
const SESSION_WRITE_POLICY_GATE: &str = "allow_session_write";
const MEMORY_WRITE_POLICY_GATE: &str = "allow_memory_write";

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
    /// Public `fn` `new` in `tau-gateway`.
    ///
    /// This item is part of the Wave 2 API surface for M23 documentation uplift.
    /// Callers rely on its contract and failure semantics remaining stable.
    /// Update this comment if behavior or integration expectations change.
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
    pub model_input_cost_per_million: Option<f64>,
    pub model_cached_input_cost_per_million: Option<f64>,
    pub model_output_cost_per_million: Option<f64>,
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
    pub external_coding_agent_bridge: ExternalCodingAgentBridgeConfig,
}

#[derive(Clone)]
struct GatewayOpenResponsesServerState {
    config: GatewayOpenResponsesServerConfig,
    response_sequence: Arc<AtomicU64>,
    auth_runtime: Arc<Mutex<GatewayAuthRuntimeState>>,
    compat_runtime: Arc<Mutex<GatewayOpenAiCompatRuntimeState>>,
    ui_telemetry_runtime: Arc<Mutex<GatewayUiTelemetryRuntimeState>>,
    external_coding_agent_bridge: Arc<ExternalCodingAgentBridge>,
    cortex: Arc<Cortex>,
}

impl GatewayOpenResponsesServerState {
    fn new(config: GatewayOpenResponsesServerConfig) -> Self {
        let external_coding_agent_bridge = Arc::new(ExternalCodingAgentBridge::new(
            config.external_coding_agent_bridge.clone(),
        ));
        let cortex = Arc::new(Cortex::new(CortexConfig::new(gateway_memory_stores_root(
            &config.state_dir,
        ))));
        Self {
            config,
            response_sequence: Arc::new(AtomicU64::new(0)),
            auth_runtime: Arc::new(Mutex::new(GatewayAuthRuntimeState::default())),
            compat_runtime: Arc::new(Mutex::new(GatewayOpenAiCompatRuntimeState::default())),
            ui_telemetry_runtime: Arc::new(Mutex::new(GatewayUiTelemetryRuntimeState::default())),
            external_coding_agent_bridge,
            cortex,
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

    fn resolved_system_prompt(&self) -> String {
        self.cortex
            .compose_system_prompt(self.config.system_prompt.as_str())
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

/// Public `fn` `run_gateway_openresponses_server` in `tau-gateway`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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
    let mut cortex_bulletin_runtime = start_cortex_bulletin_runtime(
        Arc::clone(&state.cortex),
        state.config.client.clone(),
        state.config.model.clone(),
        state.config.runtime_heartbeat.enabled,
        state.config.runtime_heartbeat.interval,
    );
    let app = build_gateway_openresponses_router(state);
    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    cortex_bulletin_runtime.shutdown().await;
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
        .route(GATEWAY_SESSIONS_ENDPOINT, get(handle_gateway_sessions_list))
        .route(
            GATEWAY_SESSION_DETAIL_ENDPOINT,
            get(handle_gateway_session_detail),
        )
        .route(
            GATEWAY_SESSION_APPEND_ENDPOINT,
            post(handle_gateway_session_append),
        )
        .route(
            GATEWAY_SESSION_RESET_ENDPOINT,
            post(handle_gateway_session_reset),
        )
        .route(
            GATEWAY_MEMORY_ENDPOINT,
            get(handle_gateway_memory_read).put(handle_gateway_memory_write),
        )
        .route(
            GATEWAY_MEMORY_ENTRY_ENDPOINT,
            get(handle_gateway_memory_entry_read)
                .put(handle_gateway_memory_entry_write)
                .delete(handle_gateway_memory_entry_delete),
        )
        .route(
            GATEWAY_MEMORY_GRAPH_ENDPOINT,
            get(handle_gateway_memory_graph),
        )
        .route(API_MEMORIES_GRAPH_ENDPOINT, get(handle_api_memories_graph))
        .route(
            GATEWAY_CHANNEL_LIFECYCLE_ENDPOINT,
            post(handle_gateway_channel_lifecycle_action),
        )
        .route(
            GATEWAY_CONFIG_ENDPOINT,
            get(handle_gateway_config_get).patch(handle_gateway_config_patch),
        )
        .route(
            GATEWAY_SAFETY_POLICY_ENDPOINT,
            get(handle_gateway_safety_policy_get).put(handle_gateway_safety_policy_put),
        )
        .route(
            GATEWAY_SAFETY_RULES_ENDPOINT,
            get(handle_gateway_safety_rules_get).put(handle_gateway_safety_rules_put),
        )
        .route(
            GATEWAY_SAFETY_TEST_ENDPOINT,
            post(handle_gateway_safety_test),
        )
        .route(
            GATEWAY_AUDIT_SUMMARY_ENDPOINT,
            get(handle_gateway_audit_summary),
        )
        .route(GATEWAY_AUDIT_LOG_ENDPOINT, get(handle_gateway_audit_log))
        .route(
            GATEWAY_TRAINING_STATUS_ENDPOINT,
            get(handle_gateway_training_status),
        )
        .route(
            GATEWAY_TRAINING_ROLLOUTS_ENDPOINT,
            get(handle_gateway_training_rollouts),
        )
        .route(
            GATEWAY_TRAINING_CONFIG_ENDPOINT,
            patch(handle_gateway_training_config_patch),
        )
        .route(GATEWAY_TOOLS_ENDPOINT, get(handle_gateway_tools_inventory))
        .route(
            GATEWAY_TOOLS_STATS_ENDPOINT,
            get(handle_gateway_tools_stats),
        )
        .route(GATEWAY_JOBS_ENDPOINT, get(handle_gateway_jobs_list))
        .route(
            GATEWAY_JOB_CANCEL_ENDPOINT_TEMPLATE,
            post(handle_gateway_job_cancel),
        )
        .route(GATEWAY_DEPLOY_ENDPOINT, post(handle_gateway_deploy))
        .route(
            GATEWAY_AGENT_STOP_ENDPOINT_TEMPLATE,
            post(handle_gateway_agent_stop),
        )
        .route(
            GATEWAY_UI_TELEMETRY_ENDPOINT,
            post(handle_gateway_ui_telemetry),
        )
        .route(CORTEX_CHAT_ENDPOINT, post(handle_cortex_chat))
        .route(CORTEX_STATUS_ENDPOINT, get(handle_cortex_status))
        .route(
            EXTERNAL_CODING_AGENT_SESSIONS_ENDPOINT,
            post(handle_external_coding_agent_open_session),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_DETAIL_ENDPOINT,
            get(handle_external_coding_agent_session_detail),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_PROGRESS_ENDPOINT,
            post(handle_external_coding_agent_session_progress),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_ENDPOINT,
            post(handle_external_coding_agent_session_followup),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_DRAIN_ENDPOINT,
            post(handle_external_coding_agent_session_followups_drain),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_STREAM_ENDPOINT,
            get(handle_external_coding_agent_session_stream),
        )
        .route(
            EXTERNAL_CODING_AGENT_SESSION_CLOSE_ENDPOINT,
            post(handle_external_coding_agent_session_close),
        )
        .route(
            EXTERNAL_CODING_AGENT_REAP_ENDPOINT,
            post(handle_external_coding_agent_reap),
        )
        .route(OPS_DASHBOARD_ENDPOINT, get(handle_ops_dashboard_shell_page))
        .route(
            OPS_DASHBOARD_AGENTS_ENDPOINT,
            get(handle_ops_dashboard_agents_shell_page),
        )
        .route(
            OPS_DASHBOARD_AGENT_DETAIL_ENDPOINT,
            get(handle_ops_dashboard_agent_detail_shell_page),
        )
        .route(
            OPS_DASHBOARD_CHAT_ENDPOINT,
            get(handle_ops_dashboard_chat_shell_page),
        )
        .route(
            OPS_DASHBOARD_CHAT_NEW_ENDPOINT,
            post(handle_ops_dashboard_chat_new),
        )
        .route(
            OPS_DASHBOARD_CHAT_SEND_ENDPOINT,
            post(handle_ops_dashboard_chat_send),
        )
        .route(
            OPS_DASHBOARD_SESSIONS_ENDPOINT,
            get(handle_ops_dashboard_sessions_shell_page),
        )
        .route(
            "/ops/sessions/branch",
            post(handle_ops_dashboard_sessions_branch),
        )
        .route(
            OPS_DASHBOARD_SESSION_DETAIL_ENDPOINT,
            get(handle_ops_dashboard_session_detail_shell_page)
                .post(handle_ops_dashboard_session_detail_reset),
        )
        .route(
            OPS_DASHBOARD_MEMORY_ENDPOINT,
            get(handle_ops_dashboard_memory_shell_page).post(handle_ops_dashboard_memory_create),
        )
        .route(
            OPS_DASHBOARD_MEMORY_GRAPH_ENDPOINT,
            get(handle_ops_dashboard_memory_graph_shell_page),
        )
        .route(
            OPS_DASHBOARD_TOOLS_JOBS_ENDPOINT,
            get(handle_ops_dashboard_tools_jobs_shell_page),
        )
        .route(
            OPS_DASHBOARD_CHANNELS_ENDPOINT,
            get(handle_ops_dashboard_channels_shell_page),
        )
        .route(
            OPS_DASHBOARD_CONFIG_ENDPOINT,
            get(handle_ops_dashboard_config_shell_page),
        )
        .route(
            OPS_DASHBOARD_TRAINING_ENDPOINT,
            get(handle_ops_dashboard_training_shell_page),
        )
        .route(
            OPS_DASHBOARD_SAFETY_ENDPOINT,
            get(handle_ops_dashboard_safety_shell_page),
        )
        .route(
            OPS_DASHBOARD_DIAGNOSTICS_ENDPOINT,
            get(handle_ops_dashboard_diagnostics_shell_page),
        )
        .route(
            OPS_DASHBOARD_DEPLOY_ENDPOINT,
            get(handle_ops_dashboard_deploy_shell_page),
        )
        .route(
            OPS_DASHBOARD_LOGIN_ENDPOINT,
            get(handle_ops_dashboard_login_shell_page),
        )
        .route(DASHBOARD_SHELL_ENDPOINT, get(handle_dashboard_shell_page))
        .route(WEBCHAT_ENDPOINT, get(handle_webchat_page))
        .route(
            GATEWAY_AUTH_BOOTSTRAP_ENDPOINT,
            get(handle_gateway_auth_bootstrap),
        )
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

macro_rules! define_ops_shell_handler {
    ($handler_name:ident, $route:expr) => {
        async fn $handler_name(
            State(state): State<Arc<GatewayOpenResponsesServerState>>,
            Query(controls): Query<OpsShellControlsQuery>,
        ) -> Html<String> {
            render_tau_ops_dashboard_shell_for_route(&state, $route, controls, None)
        }
    };
}

define_ops_shell_handler!(handle_ops_dashboard_shell_page, TauOpsDashboardRoute::Ops);
define_ops_shell_handler!(
    handle_ops_dashboard_agents_shell_page,
    TauOpsDashboardRoute::Agents
);
define_ops_shell_handler!(
    handle_ops_dashboard_chat_shell_page,
    TauOpsDashboardRoute::Chat
);
define_ops_shell_handler!(
    handle_ops_dashboard_sessions_shell_page,
    TauOpsDashboardRoute::Sessions
);
define_ops_shell_handler!(
    handle_ops_dashboard_memory_shell_page,
    TauOpsDashboardRoute::Memory
);
define_ops_shell_handler!(
    handle_ops_dashboard_memory_graph_shell_page,
    TauOpsDashboardRoute::MemoryGraph
);
define_ops_shell_handler!(
    handle_ops_dashboard_tools_jobs_shell_page,
    TauOpsDashboardRoute::ToolsJobs
);
define_ops_shell_handler!(
    handle_ops_dashboard_channels_shell_page,
    TauOpsDashboardRoute::Channels
);
define_ops_shell_handler!(
    handle_ops_dashboard_config_shell_page,
    TauOpsDashboardRoute::Config
);
define_ops_shell_handler!(
    handle_ops_dashboard_training_shell_page,
    TauOpsDashboardRoute::Training
);
define_ops_shell_handler!(
    handle_ops_dashboard_safety_shell_page,
    TauOpsDashboardRoute::Safety
);
define_ops_shell_handler!(
    handle_ops_dashboard_diagnostics_shell_page,
    TauOpsDashboardRoute::Diagnostics
);
define_ops_shell_handler!(
    handle_ops_dashboard_deploy_shell_page,
    TauOpsDashboardRoute::Deploy
);
define_ops_shell_handler!(
    handle_ops_dashboard_login_shell_page,
    TauOpsDashboardRoute::Login
);

async fn handle_ops_dashboard_agent_detail_shell_page(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    AxumPath(_agent_id): AxumPath<String>,
    Query(controls): Query<OpsShellControlsQuery>,
) -> Html<String> {
    render_tau_ops_dashboard_shell_for_route(
        &state,
        TauOpsDashboardRoute::AgentDetail,
        controls,
        None,
    )
}

async fn handle_ops_dashboard_session_detail_shell_page(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    AxumPath(session_key): AxumPath<String>,
    Query(controls): Query<OpsShellControlsQuery>,
) -> Html<String> {
    render_tau_ops_dashboard_shell_for_route(
        &state,
        TauOpsDashboardRoute::Sessions,
        controls,
        Some(session_key.as_str()),
    )
}

async fn handle_dashboard_shell_page() -> Html<String> {
    Html(render_gateway_dashboard_shell_page())
}

async fn handle_gateway_auth_bootstrap(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
) -> Response {
    if let Err(error) = enforce_gateway_rate_limit(&state, "gateway_auth_bootstrap") {
        return error.into_response();
    }

    let auth_mode = resolve_tau_ops_dashboard_auth_mode(state.config.auth_mode);
    (
        StatusCode::OK,
        Json(GatewayAuthBootstrapResponse {
            auth_mode: state.config.auth_mode.as_str().to_string(),
            ui_auth_mode: auth_mode.as_str().to_string(),
            requires_authentication: auth_mode.requires_authentication(),
            ops_endpoint: OPS_DASHBOARD_ENDPOINT,
            ops_login_endpoint: OPS_DASHBOARD_LOGIN_ENDPOINT,
            auth_session_endpoint: GATEWAY_AUTH_SESSION_ENDPOINT,
        }),
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

fn enforce_policy_gate(
    provided: Option<&str>,
    required: &'static str,
) -> Result<(), OpenResponsesApiError> {
    let Some(gate) = provided.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(OpenResponsesApiError::forbidden(
            "policy_gate_required",
            format!("set policy_gate='{required}' to perform this operation"),
        ));
    };
    if gate != required {
        return Err(OpenResponsesApiError::forbidden(
            "policy_gate_mismatch",
            format!("policy_gate must equal '{required}'"),
        ));
    }
    Ok(())
}

fn system_time_to_unix_ms(time: std::time::SystemTime) -> Option<u64> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_millis()).ok()
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

    let preflight_input_tokens = derive_gateway_preflight_token_limit(state.config.max_input_chars);
    let resolved_system_prompt = state.resolved_system_prompt();
    let mut agent = Agent::new(
        state.config.client.clone(),
        AgentConfig {
            model: state.config.model.clone(),
            model_input_cost_per_million: state.config.model_input_cost_per_million,
            model_cached_input_cost_per_million: state.config.model_cached_input_cost_per_million,
            model_output_cost_per_million: state.config.model_output_cost_per_million,
            system_prompt: resolved_system_prompt.clone(),
            max_turns: state.config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            // Fail closed on preflight limits: reject over-budget requests instead of compacting them.
            max_estimated_input_tokens: None,
            max_estimated_total_tokens: preflight_input_tokens,
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
            &resolved_system_prompt,
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

    let pre_prompt_cost = agent.cost_snapshot();
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
    let post_prompt_cost = agent.cost_snapshot();
    persist_session_usage_delta(&mut session_runtime, &pre_prompt_cost, &post_prompt_cost)
        .map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to persist gateway session usage summary: {error}"
            ))
        })?;

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

fn derive_gateway_preflight_token_limit(max_input_chars: usize) -> Option<u32> {
    if max_input_chars == 0 {
        return None;
    }
    let chars = u32::try_from(max_input_chars).unwrap_or(u32::MAX);
    Some(chars.saturating_add(3) / 4)
}

#[cfg(test)]
fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}
