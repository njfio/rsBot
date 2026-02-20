//! OpenResponses-compatible gateway server and request flow handlers.
//!
//! This module defines HTTP/WebSocket serving boundaries, auth handling, and
//! response streaming behavior for gateway mode. Failure paths retain structured
//! context to support operator diagnostics and incident replay.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::io::Write;
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
use tau_ai::{LlmClient, Message, MessageRole, StreamDeltaHandler};
use tau_core::{current_unix_timestamp, current_unix_timestamp_ms, write_text_atomic};
use tau_memory::memory_contract::{MemoryEntry, MemoryScope};
use tau_memory::runtime::{
    FileMemoryStore, MemoryRelationInput, MemoryScopeFilter, MemorySearchMatch,
    MemorySearchOptions, MemoryType, RuntimeMemoryRecord,
};
use tau_multi_channel::multi_channel_contract::MultiChannelTransport;
use tau_multi_channel::multi_channel_lifecycle::{
    default_probe_max_attempts, default_probe_retry_delay_ms, default_probe_timeout_ms,
    execute_multi_channel_lifecycle_action, MultiChannelLifecycleAction,
    MultiChannelLifecycleCommandConfig,
};
use tau_runtime::{
    inspect_runtime_heartbeat, start_runtime_heartbeat_scheduler, ExternalCodingAgentBridge,
    ExternalCodingAgentBridgeConfig, ExternalCodingAgentBridgeError,
    ExternalCodingAgentSessionSnapshot, ExternalCodingAgentSessionStatus,
    RuntimeHeartbeatSchedulerConfig, TransportHealthSnapshot,
};
use tau_session::SessionStore;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::remote_profile::GatewayOpenResponsesAuthMode;

mod audit_runtime;
mod auth_runtime;
mod cortex_bulletin_runtime;
mod cortex_runtime;
mod dashboard_status;
mod deploy_runtime;
mod jobs_runtime;
mod multi_channel_status;
mod openai_compat;
mod request_translation;
mod safety_runtime;
mod session_runtime;
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
use cortex_bulletin_runtime::start_cortex_bulletin_runtime;
use cortex_runtime::{
    handle_cortex_chat, handle_cortex_status, record_cortex_external_followup_event,
    record_cortex_external_progress_event, record_cortex_external_session_closed,
    record_cortex_external_session_opened, record_cortex_memory_entry_delete_event,
    record_cortex_memory_entry_write_event, record_cortex_memory_write_event,
    record_cortex_session_append_event, record_cortex_session_reset_event,
};
use dashboard_status::{
    apply_gateway_dashboard_action, collect_gateway_dashboard_snapshot,
    GatewayDashboardActionRequest,
};
use deploy_runtime::{handle_gateway_agent_stop, handle_gateway_deploy};
use jobs_runtime::{handle_gateway_job_cancel, handle_gateway_jobs_list};
use multi_channel_status::collect_gateway_multi_channel_status_report;
use openai_compat::{
    build_chat_completions_payload, build_chat_completions_stream_chunks,
    build_completions_payload, build_completions_stream_chunks, build_models_payload,
    translate_chat_completions_request, translate_completions_request,
    OpenAiChatCompletionsRequest, OpenAiCompletionsRequest,
};
use request_translation::{sanitize_session_key, translate_openresponses_request};
use safety_runtime::{
    handle_gateway_safety_policy_get, handle_gateway_safety_policy_put,
    handle_gateway_safety_rules_get, handle_gateway_safety_rules_put, handle_gateway_safety_test,
};
use session_runtime::{
    collect_assistant_reply, gateway_session_path, initialize_gateway_session_runtime,
    persist_messages, persist_session_usage_delta,
};
use tools_runtime::{handle_gateway_tools_inventory, handle_gateway_tools_stats};
use training_runtime::{handle_gateway_training_config_patch, handle_gateway_training_rollouts};
use types::{
    GatewayAuthSessionRequest, GatewayAuthSessionResponse, GatewayChannelLifecycleRequest,
    GatewayConfigPatchRequest, GatewayExternalCodingAgentFollowupsDrainRequest,
    GatewayExternalCodingAgentMessageRequest, GatewayExternalCodingAgentReapRequest,
    GatewayExternalCodingAgentSessionOpenRequest, GatewayExternalCodingAgentStreamQuery,
    GatewayMemoryEntryDeleteRequest, GatewayMemoryEntryUpsertRequest, GatewayMemoryGraphEdge,
    GatewayMemoryGraphFilterSummary, GatewayMemoryGraphNode, GatewayMemoryGraphQuery,
    GatewayMemoryGraphResponse, GatewayMemoryReadQuery, GatewayMemoryUpdateRequest,
    GatewaySafetyPolicyUpdateRequest, GatewaySafetyRulesUpdateRequest, GatewaySafetyTestRequest,
    GatewaySessionAppendRequest, GatewaySessionResetRequest, GatewayUiTelemetryRequest,
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
const WEBCHAT_ENDPOINT: &str = "/webchat";
const GATEWAY_STATUS_ENDPOINT: &str = "/gateway/status";
const GATEWAY_WS_ENDPOINT: &str = "/gateway/ws";
const GATEWAY_AUTH_SESSION_ENDPOINT: &str = "/gateway/auth/session";
const GATEWAY_SESSIONS_ENDPOINT: &str = "/gateway/sessions";
const GATEWAY_SESSION_DETAIL_ENDPOINT: &str = "/gateway/sessions/{session_key}";
const GATEWAY_SESSION_APPEND_ENDPOINT: &str = "/gateway/sessions/{session_key}/append";
const GATEWAY_SESSION_RESET_ENDPOINT: &str = "/gateway/sessions/{session_key}/reset";
const GATEWAY_MEMORY_ENDPOINT: &str = "/gateway/memory/{session_key}";
const GATEWAY_MEMORY_ENTRY_ENDPOINT: &str = "/gateway/memory/{session_key}/{entry_id}";
const GATEWAY_MEMORY_GRAPH_ENDPOINT: &str = "/gateway/memory-graph/{session_key}";
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
const GATEWAY_EVENTS_INSPECT_QUEUE_LIMIT: usize = 64;
const GATEWAY_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS: u64 = 86_400;
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

    fn record_ui_telemetry_event(&self, view: &str, action: &str, reason_code: &str) {
        if let Ok(mut runtime) = self.ui_telemetry_runtime.lock() {
            runtime.total_events = runtime.total_events.saturating_add(1);
            runtime.last_event_unix_ms = Some(current_unix_timestamp_ms());

            if !view.trim().is_empty() {
                *runtime
                    .view_counts
                    .entry(view.trim().to_string())
                    .or_default() += 1;
            }
            if !action.trim().is_empty() {
                *runtime
                    .action_counts
                    .entry(action.trim().to_string())
                    .or_default() += 1;
            }
            if !reason_code.trim().is_empty() {
                *runtime
                    .reason_code_counts
                    .entry(reason_code.trim().to_string())
                    .or_default() += 1;
            }
        }
    }

    fn collect_ui_telemetry_status_report(&self) -> GatewayUiTelemetryStatusReport {
        if let Ok(runtime) = self.ui_telemetry_runtime.lock() {
            return GatewayUiTelemetryStatusReport {
                total_events: runtime.total_events,
                last_event_unix_ms: runtime.last_event_unix_ms,
                view_counts: runtime.view_counts.clone(),
                action_counts: runtime.action_counts.clone(),
                reason_code_counts: runtime.reason_code_counts.clone(),
            };
        }
        GatewayUiTelemetryStatusReport::default()
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
struct GatewayUiTelemetryRuntimeState {
    total_events: u64,
    last_event_unix_ms: Option<u64>,
    view_counts: BTreeMap<String, u64>,
    action_counts: BTreeMap<String, u64>,
    reason_code_counts: BTreeMap<String, u64>,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
struct GatewayUiTelemetryStatusReport {
    total_events: u64,
    last_event_unix_ms: Option<u64>,
    view_counts: BTreeMap<String, u64>,
    action_counts: BTreeMap<String, u64>,
    reason_code_counts: BTreeMap<String, u64>,
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

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewaySessionsListQuery {
    #[serde(default)]
    limit: Option<usize>,
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
                "web_ui": {
                    "sessions_endpoint": GATEWAY_SESSIONS_ENDPOINT,
                    "session_detail_endpoint": GATEWAY_SESSION_DETAIL_ENDPOINT,
                    "session_append_endpoint": GATEWAY_SESSION_APPEND_ENDPOINT,
                    "session_reset_endpoint": GATEWAY_SESSION_RESET_ENDPOINT,
                    "memory_endpoint": GATEWAY_MEMORY_ENDPOINT,
                    "memory_entry_endpoint": GATEWAY_MEMORY_ENTRY_ENDPOINT,
                    "memory_graph_endpoint": GATEWAY_MEMORY_GRAPH_ENDPOINT,
                    "channel_lifecycle_endpoint": GATEWAY_CHANNEL_LIFECYCLE_ENDPOINT,
                    "config_endpoint": GATEWAY_CONFIG_ENDPOINT,
                    "safety_policy_endpoint": GATEWAY_SAFETY_POLICY_ENDPOINT,
                    "safety_rules_endpoint": GATEWAY_SAFETY_RULES_ENDPOINT,
                    "safety_test_endpoint": GATEWAY_SAFETY_TEST_ENDPOINT,
                    "audit_summary_endpoint": GATEWAY_AUDIT_SUMMARY_ENDPOINT,
                    "audit_log_endpoint": GATEWAY_AUDIT_LOG_ENDPOINT,
                    "training_status_endpoint": GATEWAY_TRAINING_STATUS_ENDPOINT,
                    "training_rollouts_endpoint": GATEWAY_TRAINING_ROLLOUTS_ENDPOINT,
                    "training_config_endpoint": GATEWAY_TRAINING_CONFIG_ENDPOINT,
                    "tools_endpoint": GATEWAY_TOOLS_ENDPOINT,
                    "tool_stats_endpoint": GATEWAY_TOOLS_STATS_ENDPOINT,
                    "jobs_endpoint": GATEWAY_JOBS_ENDPOINT,
                    "job_cancel_endpoint_template": GATEWAY_JOB_CANCEL_ENDPOINT_TEMPLATE,
                    "deploy_endpoint": GATEWAY_DEPLOY_ENDPOINT,
                    "agent_stop_endpoint_template": GATEWAY_AGENT_STOP_ENDPOINT_TEMPLATE,
                    "ui_telemetry_endpoint": GATEWAY_UI_TELEMETRY_ENDPOINT,
                    "cortex_chat_endpoint": CORTEX_CHAT_ENDPOINT,
                    "cortex_status_endpoint": CORTEX_STATUS_ENDPOINT,
                    "policy_gates": {
                        "session_write": SESSION_WRITE_POLICY_GATE,
                        "memory_write": MEMORY_WRITE_POLICY_GATE,
                    },
                    "telemetry_runtime": state.collect_ui_telemetry_status_report(),
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
                "external_coding_agent": {
                    "sessions_endpoint": EXTERNAL_CODING_AGENT_SESSIONS_ENDPOINT,
                    "session_detail_endpoint": EXTERNAL_CODING_AGENT_SESSION_DETAIL_ENDPOINT,
                    "progress_endpoint": EXTERNAL_CODING_AGENT_SESSION_PROGRESS_ENDPOINT,
                    "followups_endpoint": EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_ENDPOINT,
                    "followups_drain_endpoint": EXTERNAL_CODING_AGENT_SESSION_FOLLOWUPS_DRAIN_ENDPOINT,
                    "stream_endpoint": EXTERNAL_CODING_AGENT_SESSION_STREAM_ENDPOINT,
                    "close_endpoint": EXTERNAL_CODING_AGENT_SESSION_CLOSE_ENDPOINT,
                    "reap_endpoint": EXTERNAL_CODING_AGENT_REAP_ENDPOINT,
                    "runtime": {
                        "active_sessions": state.external_coding_agent_bridge.active_session_count(),
                        "inactivity_timeout_ms": state.config.external_coding_agent_bridge.inactivity_timeout_ms,
                        "max_active_sessions": state.config.external_coding_agent_bridge.max_active_sessions,
                        "max_events_per_session": state.config.external_coding_agent_bridge.max_events_per_session,
                    }
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

async fn handle_gateway_training_status(
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

async fn handle_external_coding_agent_open_session(
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

async fn handle_external_coding_agent_session_detail(
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

async fn handle_external_coding_agent_session_progress(
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

async fn handle_external_coding_agent_session_followup(
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

async fn handle_external_coding_agent_session_followups_drain(
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

async fn handle_external_coding_agent_session_stream(
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

async fn handle_external_coding_agent_session_close(
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

async fn handle_external_coding_agent_reap(
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

async fn handle_gateway_sessions_list(
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

async fn handle_gateway_session_detail(
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

async fn handle_gateway_session_append(
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

async fn handle_gateway_session_reset(
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

async fn handle_gateway_memory_read(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    Query(query): Query<GatewayMemoryReadQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let session_key = sanitize_session_key(session_key.as_str());

    let search_query = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(search_query) = search_query {
        let memory_type_filter = match parse_gateway_memory_type(query.memory_type.as_deref()) {
            Ok(memory_type) => memory_type,
            Err(error) => return error.into_response(),
        };
        let options = MemorySearchOptions {
            limit: query.limit.unwrap_or(25).clamp(1, 200),
            scope: MemoryScopeFilter {
                workspace_id: normalize_optional_text(query.workspace_id),
                channel_id: normalize_optional_text(query.channel_id),
                actor_id: normalize_optional_text(query.actor_id),
            },
            ..MemorySearchOptions::default()
        };

        let store = gateway_memory_store(&state.config.state_dir, &session_key);
        let search_result = match store.search(search_query.as_str(), &options) {
            Ok(result) => result,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to search memory entries for session '{session_key}': {error}"
                ))
                .into_response();
            }
        };

        let mut matches = search_result
            .matches
            .iter()
            .filter(|entry| {
                memory_type_filter
                    .map(|expected| entry.memory_type == expected)
                    .unwrap_or(true)
            })
            .map(memory_search_match_json)
            .collect::<Vec<_>>();
        matches.truncate(options.limit);

        state.record_ui_telemetry_event("memory", "search", "memory_search_requested");
        return (
            StatusCode::OK,
            Json(json!({
                "mode": "search",
                "session_key": session_key,
                "query": search_query,
                "limit": options.limit,
                "memory_type_filter": memory_type_filter.map(|kind| kind.as_str()),
                "scope_filter": {
                    "workspace_id": options.scope.workspace_id,
                    "channel_id": options.scope.channel_id,
                    "actor_id": options.scope.actor_id,
                },
                "scanned": search_result.scanned,
                "returned": matches.len(),
                "retrieval_backend": search_result.retrieval_backend,
                "retrieval_reason_code": search_result.retrieval_reason_code,
                "embedding_backend": search_result.embedding_backend,
                "embedding_reason_code": search_result.embedding_reason_code,
                "matches": matches,
                "storage_backend": store.storage_backend_label(),
                "storage_reason_code": store.storage_backend_reason_code(),
                "store_root": gateway_memory_store_root(&state.config.state_dir, &session_key).display().to_string(),
            })),
        )
            .into_response();
    }

    let path = gateway_memory_path(&state.config.state_dir, &session_key);
    let exists = path.exists();
    let content = if exists {
        match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to read memory '{}': {error}",
                    path.display()
                ))
                .into_response();
            }
        }
    } else {
        String::new()
    };

    state.record_ui_telemetry_event("memory", "read", "memory_read_requested");
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "path": path.display().to_string(),
            "exists": exists,
            "bytes": content.len(),
            "content": content,
        })),
    )
        .into_response()
}

async fn handle_gateway_memory_write(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayMemoryUpdateRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), MEMORY_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event("memory", "write", "memory_write_policy_gate_blocked");
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let memory_path = gateway_memory_path(&state.config.state_dir, &session_key);
    if let Some(parent) = memory_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return OpenResponsesApiError::internal(format!(
                "failed to create memory directory '{}': {error}",
                parent.display()
            ))
            .into_response();
        }
    }
    let mut content = request.content;
    if !content.ends_with('\n') {
        content.push('\n');
    }
    if let Err(error) = std::fs::write(&memory_path, content.as_bytes()) {
        return OpenResponsesApiError::internal(format!(
            "failed to write memory '{}': {error}",
            memory_path.display()
        ))
        .into_response();
    }

    state.record_ui_telemetry_event("memory", "write", "memory_write_applied");
    record_cortex_memory_write_event(&state.config.state_dir, session_key.as_str(), content.len());
    (
        StatusCode::OK,
        Json(json!({
            "session_key": session_key,
            "path": memory_path.display().to_string(),
            "bytes": content.len(),
            "updated_unix_ms": current_unix_timestamp_ms(),
        })),
    )
        .into_response()
}

async fn handle_gateway_memory_entry_read(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath((session_key, entry_id)): AxumPath<(String, String)>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let session_key = sanitize_session_key(session_key.as_str());
    let entry_id = entry_id.trim().to_string();
    if entry_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_memory_entry_id",
            "entry_id must be non-empty",
        )
        .into_response();
    }

    let store = gateway_memory_store(&state.config.state_dir, &session_key);
    match store.read_entry(entry_id.as_str(), None) {
        Ok(Some(record)) => {
            state.record_ui_telemetry_event("memory", "entry_read", "memory_entry_read_requested");
            (
                StatusCode::OK,
                Json(json!({
                    "session_key": session_key,
                    "entry": memory_record_json(&record),
                    "storage_backend": store.storage_backend_label(),
                    "storage_reason_code": store.storage_backend_reason_code(),
                })),
            )
                .into_response()
        }
        Ok(None) => OpenResponsesApiError::not_found(
            "memory_entry_not_found",
            format!(
                "memory entry '{}' was not found for session '{}'",
                entry_id, session_key
            ),
        )
        .into_response(),
        Err(error) => OpenResponsesApiError::internal(format!(
            "failed to read memory entry '{}' for session '{}': {error}",
            entry_id, session_key
        ))
        .into_response(),
    }
}

async fn handle_gateway_memory_entry_write(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath((session_key, entry_id)): AxumPath<(String, String)>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayMemoryEntryUpsertRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), MEMORY_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event(
            "memory",
            "entry_write",
            "memory_entry_write_policy_gate_blocked",
        );
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let entry_id = entry_id.trim().to_string();
    if entry_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_memory_entry_id",
            "entry_id must be non-empty",
        )
        .into_response();
    }
    let summary = request.summary.trim().to_string();
    if summary.is_empty() {
        return OpenResponsesApiError::bad_request("invalid_summary", "summary must be non-empty")
            .into_response();
    }
    let memory_type = match parse_gateway_memory_type(request.memory_type.as_deref()) {
        Ok(memory_type) => memory_type,
        Err(error) => return error.into_response(),
    };

    let scope = MemoryScope {
        workspace_id: normalize_optional_text(request.workspace_id)
            .unwrap_or_else(|| session_key.clone()),
        channel_id: normalize_optional_text(request.channel_id)
            .unwrap_or_else(|| "gateway".to_string()),
        actor_id: normalize_optional_text(request.actor_id)
            .unwrap_or_else(|| "operator".to_string()),
    };
    let entry = MemoryEntry {
        memory_id: entry_id.clone(),
        summary,
        tags: request.tags,
        facts: request.facts,
        source_event_key: request.source_event_key,
        recency_weight_bps: 0,
        confidence_bps: 1000,
    };
    let relation_inputs = request
        .relations
        .into_iter()
        .map(|relation| MemoryRelationInput {
            target_id: relation.target_id,
            relation_type: relation.relation_type,
            weight: relation.weight,
        })
        .collect::<Vec<_>>();

    let store = gateway_memory_store(&state.config.state_dir, &session_key);
    let write_result = match store.write_entry_with_metadata_and_relations(
        &scope,
        entry,
        memory_type,
        request.importance,
        relation_inputs.as_slice(),
    ) {
        Ok(result) => result,
        Err(error) => {
            return OpenResponsesApiError::internal(format!(
                "failed to write memory entry '{}' for session '{}': {error}",
                entry_id, session_key
            ))
            .into_response();
        }
    };

    state.record_ui_telemetry_event("memory", "entry_write", "memory_entry_write_applied");
    record_cortex_memory_entry_write_event(
        &state.config.state_dir,
        session_key.as_str(),
        entry_id.as_str(),
        write_result.created,
    );
    (
        if write_result.created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "session_key": session_key,
            "created": write_result.created,
            "entry": memory_record_json(&write_result.record),
            "storage_backend": store.storage_backend_label(),
            "storage_reason_code": store.storage_backend_reason_code(),
        })),
    )
        .into_response()
}

async fn handle_gateway_memory_entry_delete(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath((session_key, entry_id)): AxumPath<(String, String)>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayMemoryEntryDeleteRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if let Err(error) =
        enforce_policy_gate(request.policy_gate.as_deref(), MEMORY_WRITE_POLICY_GATE)
    {
        state.record_ui_telemetry_event(
            "memory",
            "entry_delete",
            "memory_entry_delete_policy_gate_blocked",
        );
        return error.into_response();
    }

    let session_key = sanitize_session_key(session_key.as_str());
    let entry_id = entry_id.trim().to_string();
    if entry_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_memory_entry_id",
            "entry_id must be non-empty",
        )
        .into_response();
    }

    let store = gateway_memory_store(&state.config.state_dir, &session_key);
    match store.soft_delete_entry(entry_id.as_str(), None) {
        Ok(Some(record)) => {
            state.record_ui_telemetry_event(
                "memory",
                "entry_delete",
                "memory_entry_delete_applied",
            );
            record_cortex_memory_entry_delete_event(
                &state.config.state_dir,
                session_key.as_str(),
                entry_id.as_str(),
                true,
            );
            (
                StatusCode::OK,
                Json(json!({
                    "session_key": session_key,
                    "deleted": true,
                    "entry": memory_record_json(&record),
                    "storage_backend": store.storage_backend_label(),
                    "storage_reason_code": store.storage_backend_reason_code(),
                })),
            )
                .into_response()
        }
        Ok(None) => OpenResponsesApiError::not_found(
            "memory_entry_not_found",
            format!(
                "memory entry '{}' was not found for session '{}'",
                entry_id, session_key
            ),
        )
        .into_response(),
        Err(error) => OpenResponsesApiError::internal(format!(
            "failed to delete memory entry '{}' for session '{}': {error}",
            entry_id, session_key
        ))
        .into_response(),
    }
}

async fn handle_gateway_memory_graph(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(session_key): AxumPath<String>,
    Query(query): Query<GatewayMemoryGraphQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let session_key = sanitize_session_key(session_key.as_str());
    let memory_path = gateway_memory_path(&state.config.state_dir, &session_key);
    let exists = memory_path.exists();
    let content = if exists {
        match std::fs::read_to_string(&memory_path) {
            Ok(content) => content,
            Err(error) => {
                return OpenResponsesApiError::internal(format!(
                    "failed to read memory '{}': {error}",
                    memory_path.display()
                ))
                .into_response();
            }
        }
    } else {
        String::new()
    };

    let max_nodes = query.max_nodes.unwrap_or(24).clamp(1, 256);
    let min_edge_weight = query.min_edge_weight.unwrap_or(1.0).max(0.0);
    let relation_types = normalize_memory_graph_relation_types(query.relation_types.as_deref());
    let nodes = build_memory_graph_nodes(&content, max_nodes);
    let edges = build_memory_graph_edges(&nodes, &relation_types, min_edge_weight);

    state.record_ui_telemetry_event("memory", "graph", "memory_graph_requested");
    (
        StatusCode::OK,
        Json(GatewayMemoryGraphResponse {
            session_key,
            path: memory_path.display().to_string(),
            exists,
            bytes: content.len(),
            node_count: nodes.len(),
            edge_count: edges.len(),
            nodes,
            edges,
            filters: GatewayMemoryGraphFilterSummary {
                max_nodes,
                min_edge_weight,
                relation_types,
            },
        }),
    )
        .into_response()
}

async fn handle_gateway_config_get(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let overrides_path = gateway_config_overrides_path(&state.config.state_dir);
    let pending_overrides = match read_gateway_config_pending_overrides(&overrides_path) {
        Ok(overrides) => overrides,
        Err(error) => return error.into_response(),
    };
    let heartbeat_interval_ms =
        match u64::try_from(state.config.runtime_heartbeat.interval.as_millis()) {
            Ok(value) => value,
            Err(_) => u64::MAX,
        };
    let heartbeat_policy_path =
        gateway_runtime_heartbeat_policy_path(&state.config.runtime_heartbeat.state_path);
    let heartbeat_policy_exists = heartbeat_policy_path.is_file();

    state.record_ui_telemetry_event("configuration", "config_get", "config_get_requested");
    (
        StatusCode::OK,
        Json(json!({
            "active": {
                "model": state.config.model.clone(),
                "system_prompt": state.config.system_prompt.clone(),
                "max_turns": state.config.max_turns,
                "max_input_chars": state.config.max_input_chars,
                "turn_timeout_ms": state.config.turn_timeout_ms,
                "session_lock_wait_ms": state.config.session_lock_wait_ms,
                "session_lock_stale_ms": state.config.session_lock_stale_ms,
                "auth_mode": state.config.auth_mode.as_str(),
                "rate_limit_window_seconds": state.config.rate_limit_window_seconds,
                "rate_limit_max_requests": state.config.rate_limit_max_requests,
                "runtime_heartbeat_enabled": state.config.runtime_heartbeat.enabled,
                "runtime_heartbeat_interval_ms": heartbeat_interval_ms,
            },
            "pending_overrides": pending_overrides,
            "overrides_path": overrides_path.display().to_string(),
            "hot_reload_capabilities": {
                "runtime_heartbeat_interval_ms": {
                    "mode": "hot_reload",
                    "policy_path": heartbeat_policy_path.display().to_string(),
                    "policy_exists": heartbeat_policy_exists,
                },
                "model": { "mode": "restart_required" },
                "system_prompt": { "mode": "restart_required" },
                "max_turns": { "mode": "restart_required" },
                "max_input_chars": { "mode": "restart_required" },
            }
        })),
    )
        .into_response()
}

async fn handle_gateway_config_patch(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayConfigPatchRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };

    let mut pending_overrides = match read_gateway_config_pending_overrides(
        &gateway_config_overrides_path(&state.config.state_dir),
    ) {
        Ok(overrides) => overrides,
        Err(error) => return error.into_response(),
    };
    let mut accepted = serde_json::Map::<String, Value>::new();
    let mut applied = serde_json::Map::<String, Value>::new();
    let mut restart_required_fields = BTreeSet::<String>::new();

    if let Some(model) = request.model {
        let trimmed = model.trim().to_string();
        if trimmed.is_empty() {
            return OpenResponsesApiError::bad_request("invalid_model", "model must be non-empty")
                .into_response();
        }
        accepted.insert("model".to_string(), json!(trimmed));
        pending_overrides.insert("model".to_string(), json!(trimmed));
        restart_required_fields.insert("model".to_string());
    }

    if let Some(system_prompt) = request.system_prompt {
        let trimmed = system_prompt.trim().to_string();
        if trimmed.is_empty() {
            return OpenResponsesApiError::bad_request(
                "invalid_system_prompt",
                "system_prompt must be non-empty",
            )
            .into_response();
        }
        accepted.insert("system_prompt".to_string(), json!(trimmed));
        pending_overrides.insert("system_prompt".to_string(), json!(trimmed));
        restart_required_fields.insert("system_prompt".to_string());
    }

    if let Some(max_turns) = request.max_turns {
        if max_turns == 0 {
            return OpenResponsesApiError::bad_request(
                "invalid_max_turns",
                "max_turns must be greater than zero",
            )
            .into_response();
        }
        accepted.insert("max_turns".to_string(), json!(max_turns));
        pending_overrides.insert("max_turns".to_string(), json!(max_turns));
        restart_required_fields.insert("max_turns".to_string());
    }

    if let Some(max_input_chars) = request.max_input_chars {
        if max_input_chars == 0 {
            return OpenResponsesApiError::bad_request(
                "invalid_max_input_chars",
                "max_input_chars must be greater than zero",
            )
            .into_response();
        }
        accepted.insert("max_input_chars".to_string(), json!(max_input_chars));
        pending_overrides.insert("max_input_chars".to_string(), json!(max_input_chars));
        restart_required_fields.insert("max_input_chars".to_string());
    }

    if let Some(runtime_heartbeat_interval_ms) = request.runtime_heartbeat_interval_ms {
        if runtime_heartbeat_interval_ms == 0 {
            return OpenResponsesApiError::bad_request(
                "invalid_runtime_heartbeat_interval_ms",
                "runtime_heartbeat_interval_ms must be greater than zero",
            )
            .into_response();
        }
        let clamped_interval_ms = runtime_heartbeat_interval_ms.clamp(100, 60_000);
        let policy_path =
            gateway_runtime_heartbeat_policy_path(&state.config.runtime_heartbeat.state_path);
        let policy_payload = format!("interval_ms = {clamped_interval_ms}\n");
        if let Some(parent) = policy_path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    return OpenResponsesApiError::internal(format!(
                        "failed to create runtime heartbeat policy dir '{}': {error}",
                        parent.display()
                    ))
                    .into_response();
                }
            }
        }
        if let Err(error) = std::fs::write(&policy_path, policy_payload.as_bytes()) {
            return OpenResponsesApiError::internal(format!(
                "failed to write runtime heartbeat policy '{}': {error}",
                policy_path.display()
            ))
            .into_response();
        }

        accepted.insert(
            "runtime_heartbeat_interval_ms".to_string(),
            json!(clamped_interval_ms),
        );
        pending_overrides.insert(
            "runtime_heartbeat_interval_ms".to_string(),
            json!(clamped_interval_ms),
        );
        applied.insert(
            "runtime_heartbeat_interval_ms".to_string(),
            json!({
                "mode": "hot_reload",
                "value": clamped_interval_ms,
                "policy_path": policy_path.display().to_string(),
            }),
        );
    }

    if accepted.is_empty() {
        return OpenResponsesApiError::bad_request(
            "no_config_changes",
            "patch payload did not include any supported config fields",
        )
        .into_response();
    }

    let overrides_path = gateway_config_overrides_path(&state.config.state_dir);
    let updated_unix_ms = current_unix_timestamp_ms();
    let overrides_payload = json!({
        "schema_version": 1,
        "updated_unix_ms": updated_unix_ms,
        "pending_overrides": pending_overrides,
    });
    if let Some(parent) = overrides_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return OpenResponsesApiError::internal(format!(
                    "failed to create config override directory '{}': {error}",
                    parent.display()
                ))
                .into_response();
            }
        }
    }
    if let Err(error) = std::fs::write(&overrides_path, format!("{overrides_payload}\n").as_bytes())
    {
        return OpenResponsesApiError::internal(format!(
            "failed to write config overrides '{}': {error}",
            overrides_path.display()
        ))
        .into_response();
    }

    state.record_ui_telemetry_event("configuration", "config_patch", "config_patch_applied");
    (
        StatusCode::OK,
        Json(json!({
            "accepted": accepted,
            "applied": applied,
            "restart_required_fields": restart_required_fields.into_iter().collect::<Vec<_>>(),
            "pending_overrides": overrides_payload["pending_overrides"],
            "overrides_path": overrides_path.display().to_string(),
            "updated_unix_ms": updated_unix_ms,
        })),
    )
        .into_response()
}

async fn handle_gateway_channel_lifecycle_action(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(channel): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayChannelLifecycleRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let channel = match parse_gateway_channel_transport(channel.as_str()) {
        Ok(channel) => channel,
        Err(error) => return error.into_response(),
    };
    let action_label = request.action.trim().to_ascii_lowercase();
    let action = match parse_gateway_channel_lifecycle_action(action_label.as_str()) {
        Ok(action) => action,
        Err(error) => return error.into_response(),
    };

    let command_config =
        build_gateway_multi_channel_lifecycle_command_config(&state.config.state_dir, &request);
    match execute_multi_channel_lifecycle_action(&command_config, action, channel) {
        Ok(report) => {
            let reason_code = format!("channel_lifecycle_action_{}_applied", action_label);
            state.record_ui_telemetry_event("channels", "lifecycle_action", reason_code.as_str());
            (
                StatusCode::OK,
                Json(json!({
                    "channel": channel.as_str(),
                    "action": action_label,
                    "report": report,
                })),
            )
                .into_response()
        }
        Err(error) => OpenResponsesApiError::internal(format!(
            "failed to execute channel lifecycle action: {error}"
        ))
        .into_response(),
    }
}

async fn handle_gateway_ui_telemetry(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let principal = match authorize_and_enforce_gateway_limits(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };

    let request = match parse_gateway_json_body::<GatewayUiTelemetryRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let view = request.view.trim();
    let action = request.action.trim();
    if view.is_empty() || action.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_telemetry",
            "view and action must be non-empty",
        )
        .into_response();
    }
    let reason_code = request
        .reason_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("ui_event");
    let session_key = request
        .session_key
        .as_deref()
        .map(sanitize_session_key)
        .unwrap_or_else(|| DEFAULT_SESSION_KEY.to_string());

    let event = json!({
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "view": view,
        "action": action,
        "reason_code": reason_code,
        "session_key": session_key,
        "principal": principal,
        "metadata": request.metadata,
    });
    let telemetry_path = gateway_ui_telemetry_path(&state.config.state_dir);
    if let Err(error) = append_jsonl_record(&telemetry_path, &event) {
        return OpenResponsesApiError::internal(format!(
            "failed to append ui telemetry '{}': {error}",
            telemetry_path.display()
        ))
        .into_response();
    }

    state.record_ui_telemetry_event(view, action, reason_code);
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "reason_code": reason_code,
        })),
    )
        .into_response()
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

fn normalize_memory_graph_relation_types(raw: Option<&str>) -> Vec<String> {
    let mut relation_types = raw
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|value| value == "contains" || value == "keyword_overlap")
        .collect::<BTreeSet<_>>();
    if relation_types.is_empty() {
        relation_types.insert("contains".to_string());
        relation_types.insert("keyword_overlap".to_string());
    }
    relation_types.into_iter().collect()
}

fn parse_gateway_channel_transport(
    raw: &str,
) -> Result<MultiChannelTransport, OpenResponsesApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "telegram" => Ok(MultiChannelTransport::Telegram),
        "discord" => Ok(MultiChannelTransport::Discord),
        "whatsapp" => Ok(MultiChannelTransport::Whatsapp),
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_channel",
            "channel must be one of: telegram, discord, whatsapp",
        )),
    }
}

fn parse_gateway_channel_lifecycle_action(
    raw: &str,
) -> Result<MultiChannelLifecycleAction, OpenResponsesApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "status" => Ok(MultiChannelLifecycleAction::Status),
        "login" => Ok(MultiChannelLifecycleAction::Login),
        "logout" => Ok(MultiChannelLifecycleAction::Logout),
        "probe" => Ok(MultiChannelLifecycleAction::Probe),
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_lifecycle_action",
            "action must be one of: status, login, logout, probe",
        )),
    }
}

fn build_gateway_multi_channel_lifecycle_command_config(
    gateway_state_dir: &Path,
    request: &GatewayChannelLifecycleRequest,
) -> MultiChannelLifecycleCommandConfig {
    let tau_root = gateway_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| gateway_state_dir.to_path_buf());
    let probe_online = request.probe_online.unwrap_or(false);
    let probe_online_timeout_ms = request
        .probe_online_timeout_ms
        .unwrap_or(default_probe_timeout_ms())
        .clamp(100, 30_000);
    let probe_online_max_attempts = request
        .probe_online_max_attempts
        .unwrap_or(default_probe_max_attempts())
        .clamp(1, 5);
    let probe_online_retry_delay_ms = request
        .probe_online_retry_delay_ms
        .unwrap_or(default_probe_retry_delay_ms())
        .clamp(25, 5_000);

    MultiChannelLifecycleCommandConfig {
        state_dir: tau_root.join("multi-channel"),
        ingress_dir: tau_root.join("channel-store"),
        telegram_api_base: "https://api.telegram.org".to_string(),
        discord_api_base: "https://discord.com/api/v10".to_string(),
        whatsapp_api_base: "https://graph.facebook.com/v20.0".to_string(),
        credential_store: None,
        credential_store_unreadable: false,
        telegram_bot_token: None,
        discord_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
        probe_online,
        probe_online_timeout_ms,
        probe_online_max_attempts,
        probe_online_retry_delay_ms,
    }
}

fn build_memory_graph_nodes(content: &str, max_nodes: usize) -> Vec<GatewayMemoryGraphNode> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(max_nodes)
        .enumerate()
        .map(|(index, line)| {
            let term_count = memory_graph_terms(line).len().max(1);
            GatewayMemoryGraphNode {
                id: format!("line:{}", index + 1),
                label: line.to_string(),
                category: "memory_line".to_string(),
                weight: term_count as f64,
                size: 12.0 + (term_count.min(8) as f64 * 2.0),
            }
        })
        .collect()
}

fn build_memory_graph_edges(
    nodes: &[GatewayMemoryGraphNode],
    relation_types: &[String],
    min_edge_weight: f64,
) -> Vec<GatewayMemoryGraphEdge> {
    let relation_filter = relation_types
        .iter()
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let normalized_labels = nodes
        .iter()
        .map(|node| node.label.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let terms = nodes
        .iter()
        .map(|node| memory_graph_terms(&node.label))
        .collect::<Vec<_>>();
    let mut edges = Vec::new();

    for left_index in 0..nodes.len() {
        for right_index in (left_index + 1)..nodes.len() {
            let left = &nodes[left_index];
            let right = &nodes[right_index];
            let left_label = normalized_labels[left_index].as_str();
            let right_label = normalized_labels[right_index].as_str();

            if relation_filter.contains("contains")
                && left_label != right_label
                && !left_label.is_empty()
                && !right_label.is_empty()
            {
                let relation_direction = if left_label.contains(right_label) {
                    Some((left.id.as_str(), right.id.as_str()))
                } else if right_label.contains(left_label) {
                    Some((right.id.as_str(), left.id.as_str()))
                } else {
                    None
                };
                if let Some((source, target)) = relation_direction {
                    let weight = 1.0;
                    if weight >= min_edge_weight {
                        edges.push(GatewayMemoryGraphEdge {
                            id: format!("edge:contains:{source}:{target}"),
                            source: source.to_string(),
                            target: target.to_string(),
                            relation_type: "contains".to_string(),
                            weight,
                        });
                    }
                }
            }

            if relation_filter.contains("keyword_overlap") {
                let overlap = terms[left_index].intersection(&terms[right_index]).count();
                if overlap > 0 {
                    let weight = overlap as f64;
                    if weight >= min_edge_weight {
                        edges.push(GatewayMemoryGraphEdge {
                            id: format!("edge:keyword_overlap:{}:{}", left.id, right.id),
                            source: left.id.clone(),
                            target: right.id.clone(),
                            relation_type: "keyword_overlap".to_string(),
                            weight,
                        });
                    }
                }
            }
        }
    }

    edges.sort_by(|left, right| {
        (
            left.relation_type.as_str(),
            left.source.as_str(),
            left.target.as_str(),
            left.id.as_str(),
        )
            .cmp(&(
                right.relation_type.as_str(),
                right.source.as_str(),
                right.target.as_str(),
                right.id.as_str(),
            ))
    });
    edges
}

fn memory_graph_terms(value: &str) -> BTreeSet<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_lowercase)
        .filter(|segment| segment.len() >= 3)
        .collect()
}

fn gateway_memory_path(state_dir: &Path, session_key: &str) -> PathBuf {
    state_dir
        .join("openresponses")
        .join("memory")
        .join(format!("{session_key}.md"))
}

fn gateway_memory_store_root(state_dir: &Path, session_key: &str) -> PathBuf {
    gateway_memory_stores_root(state_dir).join(session_key)
}

fn gateway_memory_store(state_dir: &Path, session_key: &str) -> FileMemoryStore {
    FileMemoryStore::new(gateway_memory_store_root(state_dir, session_key))
}

fn gateway_memory_stores_root(state_dir: &Path) -> PathBuf {
    state_dir.join("openresponses").join("memory-store")
}

fn normalize_optional_text(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_gateway_memory_type(
    raw: Option<&str>,
) -> Result<Option<MemoryType>, OpenResponsesApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    MemoryType::parse(raw).map(Some).ok_or_else(|| {
        OpenResponsesApiError::bad_request(
            "invalid_memory_type",
            "memory_type must be one of: identity, goal, decision, todo, preference, fact, event, observation",
        )
    })
}

fn memory_record_json(record: &RuntimeMemoryRecord) -> Value {
    json!({
        "memory_id": record.entry.memory_id.as_str(),
        "summary": record.entry.summary.as_str(),
        "tags": &record.entry.tags,
        "facts": &record.entry.facts,
        "source_event_key": record.entry.source_event_key.as_str(),
        "scope": {
            "workspace_id": record.scope.workspace_id.as_str(),
            "channel_id": record.scope.channel_id.as_str(),
            "actor_id": record.scope.actor_id.as_str(),
        },
        "memory_type": record.memory_type.as_str(),
        "importance": record.importance,
        "relations": &record.relations,
        "embedding_source": record.embedding_source.as_str(),
        "embedding_model": &record.embedding_model,
        "embedding_vector_dim": record.embedding_vector.len(),
        "embedding_reason_code": record.embedding_reason_code.as_str(),
        "updated_unix_ms": record.updated_unix_ms,
        "last_accessed_at_unix_ms": record.last_accessed_at_unix_ms,
        "access_count": record.access_count,
        "forgotten": record.forgotten,
    })
}

fn memory_search_match_json(entry: &MemorySearchMatch) -> Value {
    json!({
        "memory_id": entry.memory_id.as_str(),
        "score": entry.score,
        "vector_score": entry.vector_score,
        "lexical_score": entry.lexical_score,
        "fused_score": entry.fused_score,
        "graph_score": entry.graph_score,
        "scope": {
            "workspace_id": entry.scope.workspace_id.as_str(),
            "channel_id": entry.scope.channel_id.as_str(),
            "actor_id": entry.scope.actor_id.as_str(),
        },
        "summary": entry.summary.as_str(),
        "memory_type": entry.memory_type.as_str(),
        "importance": entry.importance,
        "tags": &entry.tags,
        "facts": &entry.facts,
        "source_event_key": entry.source_event_key.as_str(),
        "embedding_source": entry.embedding_source.as_str(),
        "embedding_model": &entry.embedding_model,
        "relations": &entry.relations,
    })
}

fn gateway_config_overrides_path(state_dir: &Path) -> PathBuf {
    state_dir
        .join("openresponses")
        .join("config-overrides.json")
}

fn gateway_runtime_heartbeat_policy_path(state_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.policy.toml", state_path.display()))
}

fn read_gateway_config_pending_overrides(
    path: &Path,
) -> Result<serde_json::Map<String, Value>, OpenResponsesApiError> {
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let raw = std::fs::read_to_string(path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read config overrides '{}': {error}",
            path.display()
        ))
    })?;
    let parsed = serde_json::from_str::<Value>(raw.as_str()).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to parse config overrides '{}': {error}",
            path.display()
        ))
    })?;
    if let Some(overrides) = parsed.get("pending_overrides").and_then(Value::as_object) {
        return Ok(overrides.clone());
    }
    if let Some(overrides) = parsed.as_object() {
        return Ok(overrides.clone());
    }
    Ok(serde_json::Map::new())
}

fn gateway_ui_telemetry_path(state_dir: &Path) -> PathBuf {
    state_dir.join("openresponses").join("ui-telemetry.jsonl")
}

fn append_jsonl_record(path: &Path, record: &Value) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(record.to_string().as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to write newline {}", path.display()))?;
    Ok(())
}

fn system_time_to_unix_ms(time: std::time::SystemTime) -> Option<u64> {
    let duration = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(duration.as_millis()).ok()
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
            // Fail closed on preflight limits: keep input compaction disabled so
            // over-budget requests are rejected instead of compacted away.
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
