use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::multi_channel_contract::{
    event_contract_key, load_multi_channel_contract_fixture, validate_multi_channel_inbound_event,
    MultiChannelContractFixture, MultiChannelEventKind, MultiChannelInboundEvent,
    MultiChannelTransport,
};
use crate::multi_channel_live_ingress::parse_multi_channel_live_inbound_envelope;
use crate::multi_channel_media::{
    process_media_attachments, render_media_prompt_context, MultiChannelMediaUnderstandingConfig,
};
use crate::multi_channel_outbound::{
    MultiChannelOutboundConfig, MultiChannelOutboundDeliveryError, MultiChannelOutboundDispatcher,
};
use crate::multi_channel_policy::{
    evaluate_multi_channel_channel_policy, load_multi_channel_policy_for_state_dir,
    MultiChannelAllowFrom, MultiChannelPolicyDecision, MultiChannelPolicyEvaluation,
    MultiChannelPolicyFile,
};
use crate::multi_channel_routing::{
    load_multi_channel_route_bindings_for_state_dir, resolve_multi_channel_event_route,
    route_decision_trace_payload, MultiChannelRouteBindingFile, MultiChannelRouteDecision,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_orchestrator::multi_agent_router::{load_multi_agent_route_table, MultiAgentRouteTable};
use tau_runtime::{ChannelContextEntry, ChannelLogEntry, ChannelStore, TransportHealthSnapshot};

const MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const MULTI_CHANNEL_ROUTE_TRACES_LOG_FILE: &str = "route-traces.jsonl";
const MULTI_CHANNEL_LIVE_INGRESS_SOURCES: [(&str, &str); 3] = [
    ("telegram", "telegram.ndjson"),
    ("discord", "discord.ndjson"),
    ("whatsapp", "whatsapp.ndjson"),
];
const PAIRING_REASON_ALLOW_PERMISSIVE_MODE: &str = "allow_permissive_mode";
const PAIRING_REASON_DENY_POLICY_EVALUATION_ERROR: &str = "deny_policy_evaluation_error";
const POLICY_REASON_DENY_CHANNEL_POLICY_LOAD_ERROR: &str = "deny_channel_policy_load_error";
const POLICY_REASON_DENY_ALLOWLIST_ONLY: &str = "deny_channel_policy_allow_from_allowlist_only";
const TELEMETRY_STATUS_TYPING_STARTED: &str = "typing_started";
const TELEMETRY_STATUS_TYPING_STOPPED: &str = "typing_stopped";
const TELEMETRY_STATUS_PRESENCE_ACTIVE: &str = "presence_active";
const TELEMETRY_STATUS_PRESENCE_IDLE: &str = "presence_idle";
const COMMAND_STATUS_REPORTED: &str = "reported";
const COMMAND_STATUS_FAILED: &str = "failed";
const COMMAND_REASON_UNKNOWN: &str = "command_unknown";
const COMMAND_REASON_INVALID_ARGS: &str = "command_invalid_args";
const COMMAND_REASON_RBAC_DENIED: &str = "command_rbac_denied";
const COMMAND_REASON_HELP_REPORTED: &str = "command_help_reported";
const COMMAND_REASON_STATUS_REPORTED: &str = "command_status_reported";
const COMMAND_REASON_AUTH_STATUS_REPORTED: &str = "command_auth_status_reported";
const COMMAND_REASON_AUTH_STATUS_FAILED: &str = "command_auth_status_failed";
const COMMAND_REASON_DOCTOR_REPORTED: &str = "command_doctor_reported";
const COMMAND_REASON_APPROVALS_LIST_REPORTED: &str = "command_approvals_list_reported";
const COMMAND_REASON_APPROVALS_APPROVED: &str = "command_approvals_approved";
const COMMAND_REASON_APPROVALS_REJECTED: &str = "command_approvals_rejected";
const COMMAND_REASON_APPROVALS_FAILED: &str = "command_approvals_failed";
const COMMAND_REASON_APPROVALS_UNKNOWN_REQUEST: &str = "command_approvals_unknown_request";
const COMMAND_REASON_APPROVALS_STALE_REQUEST: &str = "command_approvals_stale_request";
const COMMAND_REASON_APPROVALS_ACTOR_MAPPING_FAILED: &str =
    "command_approvals_actor_mapping_failed";
const MULTI_CHANNEL_INCIDENT_REPLAY_EXPORT_SCHEMA_VERSION: u32 = 1;
const MULTI_CHANNEL_INCIDENT_DIAGNOSTIC_CAP: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `MultiChannelPairingDecision` values.
pub enum MultiChannelPairingDecision {
    Allow { reason_code: String },
    Deny { reason_code: String },
}

impl MultiChannelPairingDecision {
    pub fn reason_code(&self) -> &str {
        match self {
            Self::Allow { reason_code } | Self::Deny { reason_code } => reason_code,
        }
    }
}

/// Trait contract for `MultiChannelPairingEvaluator` behavior.
pub trait MultiChannelPairingEvaluator: Send + Sync {
    fn evaluate_pairing(
        &self,
        state_dir: &Path,
        policy_channel: &str,
        actor_id: &str,
        now_unix_ms: u64,
    ) -> Result<MultiChannelPairingDecision>;
}

/// Trait contract for `MultiChannelAuthCommandExecutor` behavior.
pub trait MultiChannelAuthCommandExecutor: Send + Sync {
    fn execute_auth_status(&self, provider: Option<&str>) -> String;
}

/// Trait contract for `MultiChannelDoctorCommandExecutor` behavior.
pub trait MultiChannelDoctorCommandExecutor: Send + Sync {
    fn execute_doctor(&self, online: bool) -> String;
}

/// Trait contract for `MultiChannelApprovalsCommandExecutor` behavior.
pub trait MultiChannelApprovalsCommandExecutor: Send + Sync {
    fn execute_approvals(
        &self,
        state_dir: &Path,
        args: &str,
        decision_actor: Option<&str>,
    ) -> String;
}

#[derive(Clone, Default)]
/// Public struct `MultiChannelCommandHandlers` used across Tau components.
pub struct MultiChannelCommandHandlers {
    pub auth: Option<Arc<dyn MultiChannelAuthCommandExecutor>>,
    pub doctor: Option<Arc<dyn MultiChannelDoctorCommandExecutor>>,
    pub approvals: Option<Arc<dyn MultiChannelApprovalsCommandExecutor>>,
}

fn multi_channel_runtime_state_schema_version() -> u32 {
    MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelTelemetryConfig` used across Tau components.
pub struct MultiChannelTelemetryConfig {
    pub typing_presence_enabled: bool,
    pub usage_summary_enabled: bool,
    pub include_identifiers: bool,
    pub typing_presence_min_response_chars: usize,
}

impl Default for MultiChannelTelemetryConfig {
    fn default() -> Self {
        Self {
            typing_presence_enabled: true,
            usage_summary_enabled: true,
            include_identifiers: false,
            typing_presence_min_response_chars: 120,
        }
    }
}

#[derive(Clone)]
/// Public struct `MultiChannelRuntimeConfig` used across Tau components.
pub struct MultiChannelRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub orchestrator_route_table_path: Option<PathBuf>,
    pub queue_limit: usize,
    pub processed_event_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub retry_jitter_ms: u64,
    pub outbound: MultiChannelOutboundConfig,
    pub telemetry: MultiChannelTelemetryConfig,
    pub media: MultiChannelMediaUnderstandingConfig,
    pub command_handlers: MultiChannelCommandHandlers,
    pub pairing_evaluator: Arc<dyn MultiChannelPairingEvaluator>,
}

#[derive(Clone)]
/// Public struct `MultiChannelLiveRuntimeConfig` used across Tau components.
pub struct MultiChannelLiveRuntimeConfig {
    pub ingress_dir: PathBuf,
    pub state_dir: PathBuf,
    pub orchestrator_route_table_path: Option<PathBuf>,
    pub queue_limit: usize,
    pub processed_event_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub retry_jitter_ms: u64,
    pub outbound: MultiChannelOutboundConfig,
    pub telemetry: MultiChannelTelemetryConfig,
    pub media: MultiChannelMediaUnderstandingConfig,
    pub command_handlers: MultiChannelCommandHandlers,
    pub pairing_evaluator: Arc<dyn MultiChannelPairingEvaluator>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `MultiChannelRuntimeSummary` used across Tau components.
pub struct MultiChannelRuntimeSummary {
    pub discovered_events: usize,
    pub queued_events: usize,
    pub completed_events: usize,
    pub duplicate_skips: usize,
    pub transient_failures: usize,
    pub retry_attempts: usize,
    pub failed_events: usize,
    pub policy_checked_events: usize,
    pub policy_enforced_events: usize,
    pub policy_allowed_events: usize,
    pub policy_denied_events: usize,
    pub typing_events_emitted: usize,
    pub presence_events_emitted: usize,
    pub usage_summary_records: usize,
    pub usage_response_chars: usize,
    pub usage_chunks: usize,
    pub usage_estimated_cost_micros: u64,
}

#[derive(Debug, Clone, Serialize)]
struct MultiChannelRuntimeCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    discovered_events: usize,
    queued_events: usize,
    completed_events: usize,
    duplicate_skips: usize,
    transient_failures: usize,
    retry_attempts: usize,
    failed_events: usize,
    policy_checked_events: usize,
    policy_enforced_events: usize,
    policy_allowed_events: usize,
    policy_denied_events: usize,
    typing_events_emitted: usize,
    presence_events_emitted: usize,
    usage_summary_records: usize,
    usage_response_chars: usize,
    usage_chunks: usize,
    usage_estimated_cost_micros: u64,
    backlog_events: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MultiChannelRuntimeTelemetryCounters {
    #[serde(default)]
    typing_events_emitted: usize,
    #[serde(default)]
    presence_events_emitted: usize,
    #[serde(default)]
    usage_summary_records: usize,
    #[serde(default)]
    usage_response_chars: usize,
    #[serde(default)]
    usage_chunks: usize,
    #[serde(default)]
    usage_estimated_cost_micros: u64,
    #[serde(default)]
    typing_events_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    presence_events_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_summary_records_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_response_chars_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_chunks_by_transport: BTreeMap<String, usize>,
    #[serde(default)]
    usage_estimated_cost_micros_by_transport: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelRuntimeTelemetryPolicyState {
    #[serde(default = "multi_channel_telemetry_typing_presence_default")]
    typing_presence_enabled: bool,
    #[serde(default = "multi_channel_telemetry_usage_summary_default")]
    usage_summary_enabled: bool,
    #[serde(default)]
    include_identifiers: bool,
    #[serde(default = "multi_channel_telemetry_min_response_chars_default")]
    typing_presence_min_response_chars: usize,
}

impl Default for MultiChannelRuntimeTelemetryPolicyState {
    fn default() -> Self {
        Self {
            typing_presence_enabled: multi_channel_telemetry_typing_presence_default(),
            usage_summary_enabled: multi_channel_telemetry_usage_summary_default(),
            include_identifiers: false,
            typing_presence_min_response_chars: multi_channel_telemetry_min_response_chars_default(
            ),
        }
    }
}

fn multi_channel_telemetry_typing_presence_default() -> bool {
    true
}

fn multi_channel_telemetry_usage_summary_default() -> bool {
    true
}

fn multi_channel_telemetry_min_response_chars_default() -> usize {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelRuntimeState {
    #[serde(default = "multi_channel_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    health: TransportHealthSnapshot,
    #[serde(default)]
    telemetry: MultiChannelRuntimeTelemetryCounters,
    #[serde(default)]
    telemetry_policy: MultiChannelRuntimeTelemetryPolicyState,
}

impl Default for MultiChannelRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION,
            processed_event_keys: Vec::new(),
            health: TransportHealthSnapshot::default(),
            telemetry: MultiChannelRuntimeTelemetryCounters::default(),
            telemetry_policy: MultiChannelRuntimeTelemetryPolicyState::default(),
        }
    }
}

#[derive(Debug, Clone)]
struct MultiChannelAccessDecision {
    policy_channel: String,
    channel_policy: MultiChannelPolicyEvaluation,
    pairing_decision: MultiChannelPairingDecision,
    final_decision: MultiChannelPairingDecision,
    pairing_checked: bool,
    policy_enforced: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PersistEventOutcome {
    typing_events_emitted: usize,
    presence_events_emitted: usize,
    usage_summary_records: usize,
    usage_response_chars: usize,
    usage_chunks: usize,
    usage_estimated_cost_micros: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MultiChannelTauCommand {
    Help,
    Status,
    AuthStatus {
        provider: Option<String>,
    },
    Doctor {
        online: bool,
    },
    Approvals {
        action: MultiChannelTauApprovalsAction,
        args: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MultiChannelTauApprovalsAction {
    List,
    Approve,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MultiChannelCommandExecution {
    command_line: String,
    status: String,
    reason_code: String,
    response_text: String,
}

pub async fn run_multi_channel_contract_runner(config: MultiChannelRuntimeConfig) -> Result<()> {
    let fixture = load_multi_channel_contract_fixture(&config.fixture_path)?;
    let mut runtime = MultiChannelRuntime::new(config)?;
    let summary = runtime.run_once_fixture(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();
    println!(
        "multi-channel runner summary: discovered={} queued={} completed={} duplicate_skips={} retries={} transient_failures={} failed={} policy_checked={} policy_enforced={} policy_allowed={} policy_denied={} typing_events={} presence_events={} usage_records={} usage_chars={} usage_chunks={} usage_cost_micros={}",
        summary.discovered_events,
        summary.queued_events,
        summary.completed_events,
        summary.duplicate_skips,
        summary.retry_attempts,
        summary.transient_failures,
        summary.failed_events,
        summary.policy_checked_events,
        summary.policy_enforced_events,
        summary.policy_allowed_events,
        summary.policy_denied_events,
        summary.typing_events_emitted,
        summary.presence_events_emitted,
        summary.usage_summary_records,
        summary.usage_response_chars,
        summary.usage_chunks,
        summary.usage_estimated_cost_micros
    );
    println!(
        "multi-channel runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );
    Ok(())
}

pub async fn run_multi_channel_live_runner(config: MultiChannelLiveRuntimeConfig) -> Result<()> {
    let live_events = load_multi_channel_live_events(&config.ingress_dir)?;
    let mut runtime = MultiChannelRuntime::new(MultiChannelRuntimeConfig {
        fixture_path: config.ingress_dir.join("live-ingress.ndjson"),
        state_dir: config.state_dir.clone(),
        orchestrator_route_table_path: config.orchestrator_route_table_path.clone(),
        queue_limit: config.queue_limit,
        processed_event_cap: config.processed_event_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
        retry_jitter_ms: config.retry_jitter_ms,
        outbound: config.outbound.clone(),
        telemetry: config.telemetry.clone(),
        media: config.media.clone(),
        command_handlers: config.command_handlers.clone(),
        pairing_evaluator: Arc::clone(&config.pairing_evaluator),
    })?;
    let summary = runtime.run_once_events(&live_events).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();
    println!(
        "multi-channel live runner summary: discovered={} queued={} completed={} duplicate_skips={} retries={} transient_failures={} failed={} policy_checked={} policy_enforced={} policy_allowed={} policy_denied={} typing_events={} presence_events={} usage_records={} usage_chars={} usage_chunks={} usage_cost_micros={}",
        summary.discovered_events,
        summary.queued_events,
        summary.completed_events,
        summary.duplicate_skips,
        summary.retry_attempts,
        summary.transient_failures,
        summary.failed_events,
        summary.policy_checked_events,
        summary.policy_enforced_events,
        summary.policy_allowed_events,
        summary.policy_denied_events,
        summary.typing_events_emitted,
        summary.presence_events_emitted,
        summary.usage_summary_records,
        summary.usage_response_chars,
        summary.usage_chunks,
        summary.usage_estimated_cost_micros
    );
    println!(
        "multi-channel live runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelRouteInspectReport` used across Tau components.
pub struct MultiChannelRouteInspectReport {
    pub event_key: String,
    pub transport: String,
    pub conversation_id: String,
    pub actor_id: String,
    pub binding_id: String,
    pub binding_matched: bool,
    pub match_specificity: usize,
    pub phase: String,
    pub account_id: String,
    pub requested_category: Option<String>,
    pub selected_category: Option<String>,
    pub selected_role: String,
    pub fallback_roles: Vec<String>,
    pub attempt_roles: Vec<String>,
    pub session_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `MultiChannelIncidentOutcomeCounts` used across Tau components.
pub struct MultiChannelIncidentOutcomeCounts {
    pub allowed: usize,
    pub denied: usize,
    pub retried: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelIncidentTimelineEntry` used across Tau components.
pub struct MultiChannelIncidentTimelineEntry {
    pub event_key: String,
    pub transport: String,
    pub conversation_id: String,
    pub route_session_key: String,
    pub route_binding_id: String,
    pub route_reason_code: String,
    pub policy_reason_code: String,
    pub delivery_reason_code: String,
    pub outcome: String,
    pub first_timestamp_unix_ms: u64,
    pub last_timestamp_unix_ms: u64,
    pub delivery_failed_attempts: usize,
    pub retryable_failures: usize,
    pub status_history: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelIncidentReplayExportSummary` used across Tau components.
pub struct MultiChannelIncidentReplayExportSummary {
    pub path: String,
    pub event_count: usize,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelIncidentTimelineReport` used across Tau components.
pub struct MultiChannelIncidentTimelineReport {
    pub generated_unix_ms: u64,
    pub state_dir: String,
    pub channel_store_root: String,
    pub window_start_unix_ms: Option<u64>,
    pub window_end_unix_ms: Option<u64>,
    pub event_limit: usize,
    pub scanned_channel_count: usize,
    pub scanned_log_file_count: usize,
    pub scanned_line_count: usize,
    pub invalid_line_count: usize,
    pub total_events_before_limit: usize,
    pub truncated_event_count: usize,
    pub outcomes: MultiChannelIncidentOutcomeCounts,
    pub route_reason_code_counts: BTreeMap<String, usize>,
    pub route_binding_counts: BTreeMap<String, usize>,
    pub policy_reason_code_counts: BTreeMap<String, usize>,
    pub delivery_reason_code_counts: BTreeMap<String, usize>,
    pub diagnostics: Vec<String>,
    pub timeline: Vec<MultiChannelIncidentTimelineEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_export: Option<MultiChannelIncidentReplayExportSummary>,
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelIncidentTimelineQuery` used across Tau components.
pub struct MultiChannelIncidentTimelineQuery {
    pub state_dir: PathBuf,
    pub window_start_unix_ms: Option<u64>,
    pub window_end_unix_ms: Option<u64>,
    pub event_limit: usize,
    pub replay_export_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
struct MultiChannelIncidentEventAggregate {
    first_timestamp_unix_ms: u64,
    last_timestamp_unix_ms: u64,
    transport: String,
    conversation_id: String,
    route_session_key: String,
    route_binding_id: String,
    route_binding_matched: Option<bool>,
    policy_reason_code: String,
    delivery_reason_code: String,
    denied: bool,
    has_response: bool,
    delivery_failed_attempts: usize,
    retryable_failures: usize,
    status_history: Vec<String>,
    records: Vec<ChannelLogEntry>,
}

#[derive(Debug, Clone)]
struct MultiChannelIncidentEventWithEntry {
    entry: MultiChannelIncidentTimelineEntry,
    records: Vec<ChannelLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelIncidentReplayExportFile {
    schema_version: u32,
    generated_unix_ms: u64,
    state_dir: String,
    channel_store_root: String,
    window_start_unix_ms: Option<u64>,
    window_end_unix_ms: Option<u64>,
    outcomes: MultiChannelIncidentOutcomeCounts,
    route_reason_code_counts: BTreeMap<String, usize>,
    route_binding_counts: BTreeMap<String, usize>,
    policy_reason_code_counts: BTreeMap<String, usize>,
    delivery_reason_code_counts: BTreeMap<String, usize>,
    diagnostics: Vec<String>,
    timeline: Vec<MultiChannelIncidentTimelineEntry>,
    events: Vec<MultiChannelIncidentReplayEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiChannelIncidentReplayEvent {
    event_key: String,
    transport: String,
    conversation_id: String,
    route_session_key: String,
    outcome: String,
    records: Vec<ChannelLogEntry>,
}

pub fn build_multi_channel_incident_timeline_report(
    query: &MultiChannelIncidentTimelineQuery,
) -> Result<MultiChannelIncidentTimelineReport> {
    collect_multi_channel_incident_timeline_report(query)
}

fn collect_multi_channel_incident_timeline_report(
    query: &MultiChannelIncidentTimelineQuery,
) -> Result<MultiChannelIncidentTimelineReport> {
    if let (Some(start_unix_ms), Some(end_unix_ms)) =
        (query.window_start_unix_ms, query.window_end_unix_ms)
    {
        if end_unix_ms < start_unix_ms {
            bail!(
                "incident timeline window is invalid: end {} is less than start {}",
                end_unix_ms,
                start_unix_ms
            );
        }
    }

    let mut diagnostics = Vec::new();
    let event_limit = query.event_limit.max(1);
    let channel_store_root = query.state_dir.join("channel-store");
    let channels_root = channel_store_root.join("channels");
    let log_paths = collect_multi_channel_incident_log_paths(&channels_root, &mut diagnostics)?;
    let scanned_channel_count = log_paths.len();
    let scanned_log_file_count = log_paths.len();

    let mut scanned_line_count = 0usize;
    let mut invalid_line_count = 0usize;
    let mut aggregates: BTreeMap<String, MultiChannelIncidentEventAggregate> = BTreeMap::new();

    for (transport, channel_id, log_path) in log_paths {
        let raw = std::fs::read_to_string(&log_path)
            .with_context(|| format!("failed to read {}", log_path.display()))?;
        for (line_index, raw_line) in raw.lines().enumerate() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            scanned_line_count = scanned_line_count.saturating_add(1);
            let parsed_entry = match serde_json::from_str::<ChannelLogEntry>(trimmed) {
                Ok(entry) => entry,
                Err(error) => {
                    invalid_line_count = invalid_line_count.saturating_add(1);
                    push_multi_channel_incident_diagnostic(
                        &mut diagnostics,
                        format!(
                            "{}:{} invalid channel-store log line: {error}",
                            log_path.display(),
                            line_index + 1
                        ),
                    );
                    continue;
                }
            };
            if !multi_channel_incident_timestamp_in_window(
                parsed_entry.timestamp_unix_ms,
                query.window_start_unix_ms,
                query.window_end_unix_ms,
            ) {
                continue;
            }
            let Some(event_key) = multi_channel_incident_event_key(&parsed_entry) else {
                invalid_line_count = invalid_line_count.saturating_add(1);
                push_multi_channel_incident_diagnostic(
                    &mut diagnostics,
                    format!(
                        "{}:{} skipped unkeyed channel-store record",
                        log_path.display(),
                        line_index + 1
                    ),
                );
                continue;
            };
            let aggregate = aggregates.entry(event_key).or_default();
            merge_multi_channel_incident_log_entry(
                aggregate,
                &parsed_entry,
                transport.as_str(),
                channel_id.as_str(),
            );
        }
    }

    let mut events = aggregates
        .into_iter()
        .map(
            |(event_key, aggregate)| MultiChannelIncidentEventWithEntry {
                entry: build_multi_channel_incident_timeline_entry(event_key, &aggregate),
                records: aggregate.records,
            },
        )
        .collect::<Vec<_>>();

    events.sort_by(|left, right| {
        right
            .entry
            .last_timestamp_unix_ms
            .cmp(&left.entry.last_timestamp_unix_ms)
            .then_with(|| {
                right
                    .entry
                    .first_timestamp_unix_ms
                    .cmp(&left.entry.first_timestamp_unix_ms)
            })
            .then_with(|| left.entry.event_key.cmp(&right.entry.event_key))
    });

    let total_events_before_limit = events.len();
    if events.len() > event_limit {
        events.truncate(event_limit);
    }
    let truncated_event_count = total_events_before_limit.saturating_sub(events.len());

    let mut outcomes = MultiChannelIncidentOutcomeCounts::default();
    let mut route_reason_code_counts = BTreeMap::new();
    let mut route_binding_counts = BTreeMap::new();
    let mut policy_reason_code_counts = BTreeMap::new();
    let mut delivery_reason_code_counts = BTreeMap::new();
    for event in &events {
        match event.entry.outcome.as_str() {
            "denied" => outcomes.denied = outcomes.denied.saturating_add(1),
            "retried" => outcomes.retried = outcomes.retried.saturating_add(1),
            "failed" => outcomes.failed = outcomes.failed.saturating_add(1),
            _ => outcomes.allowed = outcomes.allowed.saturating_add(1),
        }
        increment_counter_map(
            &mut route_reason_code_counts,
            event.entry.route_reason_code.as_str(),
            1,
        );
        increment_counter_map(
            &mut route_binding_counts,
            event.entry.route_binding_id.as_str(),
            1,
        );
        increment_counter_map(
            &mut policy_reason_code_counts,
            event.entry.policy_reason_code.as_str(),
            1,
        );
        increment_counter_map(
            &mut delivery_reason_code_counts,
            event.entry.delivery_reason_code.as_str(),
            1,
        );
    }

    let mut report = MultiChannelIncidentTimelineReport {
        generated_unix_ms: current_unix_timestamp_ms(),
        state_dir: query.state_dir.display().to_string(),
        channel_store_root: channel_store_root.display().to_string(),
        window_start_unix_ms: query.window_start_unix_ms,
        window_end_unix_ms: query.window_end_unix_ms,
        event_limit,
        scanned_channel_count,
        scanned_log_file_count,
        scanned_line_count,
        invalid_line_count,
        total_events_before_limit,
        truncated_event_count,
        outcomes,
        route_reason_code_counts,
        route_binding_counts,
        policy_reason_code_counts,
        delivery_reason_code_counts,
        diagnostics,
        timeline: events.iter().map(|event| event.entry.clone()).collect(),
        replay_export: None,
    };

    if let Some(path) = query.replay_export_path.as_ref() {
        let replay_export = write_multi_channel_incident_replay_export(path, &report, &events)?;
        report.replay_export = Some(replay_export);
    }

    Ok(report)
}

fn collect_multi_channel_incident_log_paths(
    channels_root: &Path,
    diagnostics: &mut Vec<String>,
) -> Result<Vec<(String, String, PathBuf)>> {
    if !channels_root.exists() {
        push_multi_channel_incident_diagnostic(
            diagnostics,
            format!(
                "channel-store channels directory is not present: {}",
                channels_root.display()
            ),
        );
        return Ok(Vec::new());
    }
    if !channels_root.is_dir() {
        bail!(
            "channel-store channels path '{}' must be a directory",
            channels_root.display()
        );
    }

    let mut transport_entries = std::fs::read_dir(channels_root)
        .with_context(|| format!("failed to read {}", channels_root.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read {}", channels_root.display()))?;
    transport_entries.sort_by_key(|entry| entry.file_name());

    let mut paths = Vec::new();
    for transport_entry in transport_entries {
        let transport_path = transport_entry.path();
        if !transport_path.is_dir() {
            continue;
        }
        let transport = transport_entry.file_name().to_string_lossy().to_string();
        let mut channel_entries = std::fs::read_dir(&transport_path)
            .with_context(|| format!("failed to read {}", transport_path.display()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read {}", transport_path.display()))?;
        channel_entries.sort_by_key(|entry| entry.file_name());
        for channel_entry in channel_entries {
            let channel_path = channel_entry.path();
            if !channel_path.is_dir() {
                continue;
            }
            let channel_id = channel_entry.file_name().to_string_lossy().to_string();
            let log_path = channel_path.join("log.jsonl");
            if log_path.is_file() {
                paths.push((transport.clone(), channel_id, log_path));
            }
        }
    }

    Ok(paths)
}

fn merge_multi_channel_incident_log_entry(
    aggregate: &mut MultiChannelIncidentEventAggregate,
    entry: &ChannelLogEntry,
    transport_hint: &str,
    channel_id_hint: &str,
) {
    if aggregate.first_timestamp_unix_ms == 0
        || entry.timestamp_unix_ms < aggregate.first_timestamp_unix_ms
    {
        aggregate.first_timestamp_unix_ms = entry.timestamp_unix_ms;
    }
    if entry.timestamp_unix_ms > aggregate.last_timestamp_unix_ms {
        aggregate.last_timestamp_unix_ms = entry.timestamp_unix_ms;
    }
    if aggregate.transport.is_empty() {
        aggregate.transport =
            extract_multi_channel_incident_payload_text(&entry.payload, "transport")
                .unwrap_or_else(|| transport_hint.to_string());
    }
    if aggregate.conversation_id.is_empty() {
        aggregate.conversation_id =
            extract_multi_channel_incident_payload_text(&entry.payload, "conversation_id")
                .unwrap_or_default();
    }
    if aggregate.route_session_key.is_empty() {
        aggregate.route_session_key =
            extract_multi_channel_incident_payload_text(&entry.payload, "route_session_key")
                .unwrap_or_else(|| channel_id_hint.to_string());
    }
    if aggregate.route_binding_id.is_empty() {
        if let Some(binding_id) = extract_multi_channel_incident_nested_payload_text(
            &entry.payload,
            &["route", "binding_id"],
        ) {
            aggregate.route_binding_id = binding_id;
        }
    }
    if aggregate.route_binding_matched.is_none() {
        aggregate.route_binding_matched = entry
            .payload
            .get("route")
            .and_then(|value| value.get("binding_matched"))
            .and_then(Value::as_bool);
    }
    if let Some(policy_reason_code) = extract_multi_channel_incident_nested_payload_text(
        &entry.payload,
        &["channel_policy", "reason_code"],
    ) {
        aggregate.policy_reason_code = policy_reason_code;
    }
    if let Some(delivery_reason_code) =
        extract_multi_channel_incident_delivery_reason_code(&entry.payload)
    {
        aggregate.delivery_reason_code = delivery_reason_code;
    }

    if entry.direction == "inbound" {
        push_multi_channel_incident_status(&mut aggregate.status_history, "inbound");
    }
    if entry.direction == "outbound" {
        if let Some(status) = extract_multi_channel_incident_payload_text(&entry.payload, "status")
        {
            push_multi_channel_incident_status(&mut aggregate.status_history, status.as_str());
            if status == "denied" {
                aggregate.denied = true;
            }
            if status == "delivery_failed" {
                aggregate.delivery_failed_attempts =
                    aggregate.delivery_failed_attempts.saturating_add(1);
                if entry
                    .payload
                    .get("retryable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    aggregate.retryable_failures = aggregate.retryable_failures.saturating_add(1);
                }
            }
        }
        if entry.payload.get("response").is_some() {
            aggregate.has_response = true;
            push_multi_channel_incident_status(&mut aggregate.status_history, "delivered");
        }
    }

    aggregate.records.push(entry.clone());
}

fn build_multi_channel_incident_timeline_entry(
    event_key: String,
    aggregate: &MultiChannelIncidentEventAggregate,
) -> MultiChannelIncidentTimelineEntry {
    let outcome = multi_channel_incident_outcome(aggregate).to_string();
    let route_reason_code =
        multi_channel_incident_route_reason_code(aggregate.route_binding_matched).to_string();
    let policy_reason_code = if aggregate.policy_reason_code.trim().is_empty() {
        "policy_reason_unknown".to_string()
    } else {
        aggregate.policy_reason_code.clone()
    };
    let delivery_reason_code = if aggregate.delivery_reason_code.trim().is_empty() {
        match outcome.as_str() {
            "denied" => "delivery_denied".to_string(),
            "failed" => "delivery_failed".to_string(),
            "retried" => "delivery_retried".to_string(),
            _ => "delivery_success".to_string(),
        }
    } else {
        aggregate.delivery_reason_code.clone()
    };
    let route_binding_id = if aggregate.route_binding_id.trim().is_empty() {
        "default".to_string()
    } else {
        aggregate.route_binding_id.clone()
    };
    let route_session_key = if aggregate.route_session_key.trim().is_empty() {
        "unknown".to_string()
    } else {
        aggregate.route_session_key.clone()
    };
    MultiChannelIncidentTimelineEntry {
        event_key,
        transport: aggregate.transport.clone(),
        conversation_id: aggregate.conversation_id.clone(),
        route_session_key,
        route_binding_id,
        route_reason_code,
        policy_reason_code,
        delivery_reason_code,
        outcome,
        first_timestamp_unix_ms: aggregate.first_timestamp_unix_ms,
        last_timestamp_unix_ms: aggregate.last_timestamp_unix_ms,
        delivery_failed_attempts: aggregate.delivery_failed_attempts,
        retryable_failures: aggregate.retryable_failures,
        status_history: aggregate.status_history.clone(),
    }
}

fn multi_channel_incident_event_key(entry: &ChannelLogEntry) -> Option<String> {
    entry
        .event_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| extract_multi_channel_incident_payload_text(&entry.payload, "event_key"))
}

fn extract_multi_channel_incident_payload_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

fn extract_multi_channel_incident_nested_payload_text(
    payload: &Value,
    path: &[&str],
) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut value = payload;
    for key in path {
        value = value.get(*key)?;
    }
    value
        .as_str()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
}

fn extract_multi_channel_incident_delivery_reason_code(payload: &Value) -> Option<String> {
    if let Some(reason_code) = extract_multi_channel_incident_payload_text(payload, "reason_code") {
        return Some(reason_code);
    }
    payload
        .get("delivery")
        .and_then(|value| value.get("receipts"))
        .and_then(Value::as_array)
        .and_then(|receipts| {
            receipts.iter().find_map(|receipt| {
                receipt
                    .get("reason_code")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string())
            })
        })
}

fn multi_channel_incident_timestamp_in_window(
    timestamp_unix_ms: u64,
    window_start_unix_ms: Option<u64>,
    window_end_unix_ms: Option<u64>,
) -> bool {
    if let Some(start_unix_ms) = window_start_unix_ms {
        if timestamp_unix_ms < start_unix_ms {
            return false;
        }
    }
    if let Some(end_unix_ms) = window_end_unix_ms {
        if timestamp_unix_ms > end_unix_ms {
            return false;
        }
    }
    true
}

fn multi_channel_incident_route_reason_code(binding_matched: Option<bool>) -> &'static str {
    match binding_matched {
        Some(true) => "route_binding_matched",
        Some(false) => "route_binding_default",
        None => "route_binding_unknown",
    }
}

fn multi_channel_incident_outcome(aggregate: &MultiChannelIncidentEventAggregate) -> &'static str {
    if aggregate.denied {
        return "denied";
    }
    if aggregate.has_response {
        if aggregate.delivery_failed_attempts > 0 {
            return "retried";
        }
        return "allowed";
    }
    if aggregate.delivery_failed_attempts > 0 {
        return "failed";
    }
    "allowed"
}

fn push_multi_channel_incident_status(status_history: &mut Vec<String>, status: &str) {
    let normalized = status.trim();
    if normalized.is_empty() {
        return;
    }
    if status_history.last().is_some_and(|last| last == normalized) {
        return;
    }
    status_history.push(normalized.to_string());
    if status_history.len() > 12 {
        status_history.remove(0);
    }
}

fn push_multi_channel_incident_diagnostic(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() >= MULTI_CHANNEL_INCIDENT_DIAGNOSTIC_CAP {
        return;
    }
    diagnostics.push(message);
}

pub fn render_multi_channel_incident_timeline_report(
    report: &MultiChannelIncidentTimelineReport,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "multi-channel incident timeline: state_dir={} channel_store_root={} window_start_unix_ms={} window_end_unix_ms={} event_limit={} events={} truncated={} scanned_channels={} scanned_logs={} scanned_lines={} invalid_lines={} outcomes=allowed:{}|denied:{}|retried:{}|failed:{} route_reason_code_counts={} policy_reason_code_counts={} delivery_reason_code_counts={} replay_export={}",
        report.state_dir,
        report.channel_store_root,
        report
            .window_start_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report
            .window_end_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        report.event_limit,
        report.timeline.len(),
        report.truncated_event_count,
        report.scanned_channel_count,
        report.scanned_log_file_count,
        report.scanned_line_count,
        report.invalid_line_count,
        report.outcomes.allowed,
        report.outcomes.denied,
        report.outcomes.retried,
        report.outcomes.failed,
        render_multi_channel_incident_counter_map(&report.route_reason_code_counts),
        render_multi_channel_incident_counter_map(&report.policy_reason_code_counts),
        render_multi_channel_incident_counter_map(&report.delivery_reason_code_counts),
        report
            .replay_export
            .as_ref()
            .map(|summary| summary.path.as_str())
            .unwrap_or("none"),
    ));
    for entry in &report.timeline {
        lines.push(format!(
            "multi-channel incident event: event_key={} outcome={} transport={} conversation_id={} route_session_key={} route_binding_id={} route_reason_code={} policy_reason_code={} delivery_reason_code={} first_timestamp_unix_ms={} last_timestamp_unix_ms={} delivery_failed_attempts={} retryable_failures={} status_history={}",
            entry.event_key,
            entry.outcome,
            entry.transport,
            if entry.conversation_id.is_empty() {
                "unknown"
            } else {
                entry.conversation_id.as_str()
            },
            entry.route_session_key,
            entry.route_binding_id,
            entry.route_reason_code,
            entry.policy_reason_code,
            entry.delivery_reason_code,
            entry.first_timestamp_unix_ms,
            entry.last_timestamp_unix_ms,
            entry.delivery_failed_attempts,
            entry.retryable_failures,
            if entry.status_history.is_empty() {
                "none".to_string()
            } else {
                entry.status_history.join(",")
            }
        ));
    }
    if !report.diagnostics.is_empty() {
        lines.push(format!(
            "multi-channel incident diagnostics: count={} sample={}",
            report.diagnostics.len(),
            report.diagnostics.join(" | ")
        ));
    }
    lines.join("\n")
}

fn render_multi_channel_incident_counter_map(counts: &BTreeMap<String, usize>) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn write_multi_channel_incident_replay_export(
    path: &Path,
    report: &MultiChannelIncidentTimelineReport,
    events: &[MultiChannelIncidentEventWithEntry],
) -> Result<MultiChannelIncidentReplayExportSummary> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let mut replay_events = events
        .iter()
        .map(|event| {
            let mut records = event.records.clone();
            records.sort_by(|left, right| {
                left.timestamp_unix_ms
                    .cmp(&right.timestamp_unix_ms)
                    .then_with(|| left.direction.cmp(&right.direction))
                    .then_with(|| left.source.cmp(&right.source))
            });
            MultiChannelIncidentReplayEvent {
                event_key: event.entry.event_key.clone(),
                transport: event.entry.transport.clone(),
                conversation_id: event.entry.conversation_id.clone(),
                route_session_key: event.entry.route_session_key.clone(),
                outcome: event.entry.outcome.clone(),
                records,
            }
        })
        .collect::<Vec<_>>();
    replay_events.sort_by(|left, right| left.event_key.cmp(&right.event_key));

    let payload = MultiChannelIncidentReplayExportFile {
        schema_version: MULTI_CHANNEL_INCIDENT_REPLAY_EXPORT_SCHEMA_VERSION,
        generated_unix_ms: report.generated_unix_ms,
        state_dir: report.state_dir.clone(),
        channel_store_root: report.channel_store_root.clone(),
        window_start_unix_ms: report.window_start_unix_ms,
        window_end_unix_ms: report.window_end_unix_ms,
        outcomes: report.outcomes.clone(),
        route_reason_code_counts: report.route_reason_code_counts.clone(),
        route_binding_counts: report.route_binding_counts.clone(),
        policy_reason_code_counts: report.policy_reason_code_counts.clone(),
        delivery_reason_code_counts: report.delivery_reason_code_counts.clone(),
        diagnostics: report.diagnostics.clone(),
        timeline: report.timeline.clone(),
        events: replay_events,
    };
    let mut rendered = serde_json::to_string_pretty(&payload)
        .context("failed to render multi-channel incident replay export json")?;
    rendered.push('\n');
    write_text_atomic(path, &rendered)
        .with_context(|| format!("failed to write {}", path.display()))?;
    let checksum = format!("{:x}", Sha256::digest(rendered.as_bytes()));
    Ok(MultiChannelIncidentReplayExportSummary {
        path: path.display().to_string(),
        event_count: payload.events.len(),
        checksum_sha256: checksum,
    })
}

/// Public struct `MultiChannelRouteInspectConfig` used across Tau components.
pub struct MultiChannelRouteInspectConfig {
    pub inspect_file: PathBuf,
    pub state_dir: PathBuf,
    pub orchestrator_route_table_path: Option<PathBuf>,
}

pub fn build_multi_channel_route_inspect_report(
    config: &MultiChannelRouteInspectConfig,
) -> Result<MultiChannelRouteInspectReport> {
    let event = load_multi_channel_route_inspect_event(&config.inspect_file)?;
    let route_table = if let Some(path) = config.orchestrator_route_table_path.as_deref() {
        load_multi_agent_route_table(path)?
    } else {
        MultiAgentRouteTable::default()
    };
    let route_bindings = load_multi_channel_route_bindings_for_state_dir(&config.state_dir)?;
    let decision = resolve_multi_channel_event_route(&route_bindings, &route_table, &event);
    Ok(MultiChannelRouteInspectReport {
        event_key: event_contract_key(&event),
        transport: event.transport.as_str().to_string(),
        conversation_id: event.conversation_id.trim().to_string(),
        actor_id: event.actor_id.trim().to_string(),
        binding_id: decision.binding_id,
        binding_matched: decision.matched,
        match_specificity: decision.match_specificity,
        phase: decision.phase.as_str().to_string(),
        account_id: decision.account_id,
        requested_category: decision.requested_category,
        selected_category: decision.selected_category,
        selected_role: decision.selected_role,
        fallback_roles: decision.fallback_roles,
        attempt_roles: decision.attempt_roles,
        session_key: decision.session_key,
    })
}

pub fn render_multi_channel_route_inspect_report(
    report: &MultiChannelRouteInspectReport,
) -> String {
    let selected_category = report
        .selected_category
        .as_deref()
        .unwrap_or("none")
        .to_string();
    let requested_category = report
        .requested_category
        .as_deref()
        .unwrap_or("none")
        .to_string();
    format!(
        "multi-channel route inspect: event_key={} transport={} conversation_id={} actor_id={} binding_id={} binding_matched={} match_specificity={} phase={} account_id={} requested_category={} selected_category={} selected_role={} fallback_roles={} attempt_roles={} session_key={}",
        report.event_key,
        report.transport,
        report.conversation_id,
        report.actor_id,
        report.binding_id,
        report.binding_matched,
        report.match_specificity,
        report.phase,
        if report.account_id.is_empty() { "none" } else { report.account_id.as_str() },
        requested_category,
        selected_category,
        report.selected_role,
        if report.fallback_roles.is_empty() {
            "none".to_string()
        } else {
            report.fallback_roles.join(",")
        },
        if report.attempt_roles.is_empty() {
            "none".to_string()
        } else {
            report.attempt_roles.join(",")
        },
        report.session_key
    )
}

fn load_multi_channel_route_inspect_event(path: &Path) -> Result<MultiChannelInboundEvent> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!(
            "multi-channel route inspect file '{}' is empty",
            path.display()
        );
    }

    if let Ok(event) = serde_json::from_str::<MultiChannelInboundEvent>(trimmed) {
        validate_multi_channel_inbound_event(&event)
            .with_context(|| format!("invalid normalized event in {}", path.display()))?;
        return Ok(event);
    }

    if let Ok(event) = parse_multi_channel_live_inbound_envelope(trimmed) {
        return Ok(event);
    }

    let first_line = raw
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or_default();
    if let Ok(event) = serde_json::from_str::<MultiChannelInboundEvent>(first_line) {
        validate_multi_channel_inbound_event(&event)
            .with_context(|| format!("invalid normalized event in {}", path.display()))?;
        return Ok(event);
    }
    match parse_multi_channel_live_inbound_envelope(first_line) {
        Ok(event) => Ok(event),
        Err(error) => {
            bail!(
                "failed to parse '{}' as normalized event or live ingress envelope: {}",
                path.display(),
                error
            )
        }
    }
}

struct MultiChannelRuntime {
    config: MultiChannelRuntimeConfig,
    state: MultiChannelRuntimeState,
    processed_event_keys: HashSet<String>,
    route_table: MultiAgentRouteTable,
    route_bindings: MultiChannelRouteBindingFile,
    outbound_dispatcher: MultiChannelOutboundDispatcher,
}

impl MultiChannelRuntime {
    fn new(config: MultiChannelRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let route_table = if let Some(path) = config.orchestrator_route_table_path.as_deref() {
            load_multi_agent_route_table(path)?
        } else {
            MultiAgentRouteTable::default()
        };
        let route_bindings =
            match load_multi_channel_route_bindings_for_state_dir(&config.state_dir) {
                Ok(bindings) => bindings,
                Err(error) => {
                    eprintln!(
                        "multi-channel route bindings load failed: state_dir={} error={error}",
                        config.state_dir.display()
                    );
                    MultiChannelRouteBindingFile::default()
                }
            };
        let mut state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_event_keys =
            normalize_processed_keys(&state.processed_event_keys, config.processed_event_cap);
        let processed_event_keys = state.processed_event_keys.iter().cloned().collect();
        let outbound_dispatcher = MultiChannelOutboundDispatcher::new(config.outbound.clone())
            .context("failed to initialize multi-channel outbound dispatcher")?;
        Ok(Self {
            config,
            state,
            processed_event_keys,
            route_table,
            route_bindings,
            outbound_dispatcher,
        })
    }

    fn state_path(&self) -> PathBuf {
        self.config.state_dir.join("state.json")
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    async fn run_once_fixture(
        &mut self,
        fixture: &MultiChannelContractFixture,
    ) -> Result<MultiChannelRuntimeSummary> {
        self.run_once_events(&fixture.events).await
    }

    async fn run_once_events(
        &mut self,
        source_events: &[MultiChannelInboundEvent],
    ) -> Result<MultiChannelRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = MultiChannelRuntimeSummary {
            discovered_events: source_events.len(),
            ..MultiChannelRuntimeSummary::default()
        };

        let mut queued_events = source_events.to_vec();
        queued_events.sort_by(|left, right| {
            left.timestamp_ms
                .cmp(&right.timestamp_ms)
                .then_with(|| event_contract_key(left).cmp(&event_contract_key(right)))
        });
        queued_events.truncate(self.config.queue_limit);
        summary.queued_events = queued_events.len();
        let channel_policy = match load_multi_channel_policy_for_state_dir(&self.config.state_dir) {
            Ok(policy) => Some(policy),
            Err(error) => {
                eprintln!(
                    "multi-channel channel policy load failed: state_dir={} error={error}",
                    self.config.state_dir.display()
                );
                None
            }
        };

        for event in queued_events {
            let event_key = event_contract_key(&event);
            if self.processed_event_keys.contains(&event_key) {
                summary.duplicate_skips = summary.duplicate_skips.saturating_add(1);
                continue;
            }
            let now_unix_ms = current_unix_timestamp_ms();
            let route_decision =
                resolve_multi_channel_event_route(&self.route_bindings, &self.route_table, &event);
            let access_decision =
                self.evaluate_access_decision(&event, now_unix_ms, channel_policy.as_ref());
            summary.policy_checked_events = summary.policy_checked_events.saturating_add(1);
            if access_decision.policy_enforced {
                summary.policy_enforced_events = summary.policy_enforced_events.saturating_add(1);
            }

            let simulated_transient_failures = simulated_transient_failures(&event);
            let mut attempt = 1usize;
            loop {
                if attempt <= simulated_transient_failures {
                    summary.transient_failures = summary.transient_failures.saturating_add(1);
                    if attempt >= self.config.retry_max_attempts {
                        summary.failed_events = summary.failed_events.saturating_add(1);
                        break;
                    }
                    summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                    apply_retry_delay(
                        self.config.retry_base_delay_ms,
                        self.config.retry_jitter_ms,
                        attempt,
                        &event_key,
                    )
                    .await;
                    attempt = attempt.saturating_add(1);
                    continue;
                }

                match self
                    .persist_event(&event, &event_key, &access_decision, &route_decision)
                    .await
                {
                    Ok(outcome) => {
                        self.record_processed_event(&event_key);
                        summary.completed_events = summary.completed_events.saturating_add(1);
                        summary.typing_events_emitted = summary
                            .typing_events_emitted
                            .saturating_add(outcome.typing_events_emitted);
                        summary.presence_events_emitted = summary
                            .presence_events_emitted
                            .saturating_add(outcome.presence_events_emitted);
                        summary.usage_summary_records = summary
                            .usage_summary_records
                            .saturating_add(outcome.usage_summary_records);
                        summary.usage_response_chars = summary
                            .usage_response_chars
                            .saturating_add(outcome.usage_response_chars);
                        summary.usage_chunks =
                            summary.usage_chunks.saturating_add(outcome.usage_chunks);
                        summary.usage_estimated_cost_micros = summary
                            .usage_estimated_cost_micros
                            .saturating_add(outcome.usage_estimated_cost_micros);
                        if matches!(
                            access_decision.final_decision,
                            MultiChannelPairingDecision::Allow { .. }
                        ) {
                            summary.policy_allowed_events =
                                summary.policy_allowed_events.saturating_add(1);
                        } else {
                            summary.policy_denied_events =
                                summary.policy_denied_events.saturating_add(1);
                        }
                        break;
                    }
                    Err(error) => {
                        if attempt >= self.config.retry_max_attempts {
                            eprintln!(
                                "multi-channel runner event failed: key={} transport={} error={error}",
                                event_key,
                                event.transport.as_str()
                            );
                            summary.failed_events = summary.failed_events.saturating_add(1);
                            break;
                        }
                        summary.transient_failures = summary.transient_failures.saturating_add(1);
                        summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                        apply_retry_delay(
                            self.config.retry_base_delay_ms,
                            self.config.retry_jitter_ms,
                            attempt,
                            &event_key,
                        )
                        .await;
                        attempt = attempt.saturating_add(1);
                    }
                }
            }
        }

        let cycle_duration_ms =
            u64::try_from(cycle_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let health = build_transport_health_snapshot(
            &summary,
            cycle_duration_ms,
            self.state.health.failure_streak,
        );
        let classification = health.classify();
        let reason_codes = cycle_reason_codes(&summary);
        self.state.health = health.clone();
        self.state.telemetry_policy = MultiChannelRuntimeTelemetryPolicyState {
            typing_presence_enabled: self.config.telemetry.typing_presence_enabled,
            usage_summary_enabled: self.config.telemetry.usage_summary_enabled,
            include_identifiers: self.config.telemetry.include_identifiers,
            typing_presence_min_response_chars: self
                .config
                .telemetry
                .typing_presence_min_response_chars,
        };

        save_multi_channel_runtime_state(&self.state_path(), &self.state)?;
        append_multi_channel_cycle_report(
            &self
                .config
                .state_dir
                .join(MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;
        Ok(summary)
    }

    fn evaluate_access_decision(
        &self,
        event: &MultiChannelInboundEvent,
        now_unix_ms: u64,
        channel_policy_file: Option<&MultiChannelPolicyFile>,
    ) -> MultiChannelAccessDecision {
        let policy_channel = pairing_policy_channel(event);
        let channel_policy = channel_policy_file
            .map(|policy_file| evaluate_multi_channel_channel_policy(policy_file, event))
            .unwrap_or_else(|| {
                let mut fallback = evaluate_multi_channel_channel_policy(
                    &MultiChannelPolicyFile::default(),
                    event,
                );
                fallback.matched_policy_key = "policy_load_error".to_string();
                fallback.decision = MultiChannelPolicyDecision::Deny {
                    reason_code: POLICY_REASON_DENY_CHANNEL_POLICY_LOAD_ERROR.to_string(),
                };
                fallback
            });

        match &channel_policy.decision {
            MultiChannelPolicyDecision::Deny { reason_code } => {
                let denied = MultiChannelPairingDecision::Deny {
                    reason_code: reason_code.clone(),
                };
                MultiChannelAccessDecision {
                    policy_channel,
                    channel_policy,
                    pairing_decision: denied.clone(),
                    final_decision: denied,
                    pairing_checked: false,
                    policy_enforced: true,
                }
            }
            MultiChannelPolicyDecision::Allow { reason_code } => {
                match channel_policy.policy.allow_from {
                    MultiChannelAllowFrom::Any => {
                        let policy_enforced = channel_policy.policy.require_mention;
                        let allow = MultiChannelPairingDecision::Allow {
                            reason_code: reason_code.clone(),
                        };
                        MultiChannelAccessDecision {
                            policy_channel,
                            channel_policy,
                            pairing_decision: allow.clone(),
                            final_decision: allow,
                            pairing_checked: false,
                            policy_enforced,
                        }
                    }
                    MultiChannelAllowFrom::AllowlistOrPairing => {
                        let pairing_decision =
                            self.evaluate_pairing_decision(event, &policy_channel, now_unix_ms);
                        let policy_enforced = channel_policy.policy.require_mention
                            || pairing_decision_is_enforced(&pairing_decision);
                        MultiChannelAccessDecision {
                            policy_channel,
                            channel_policy,
                            pairing_decision: pairing_decision.clone(),
                            final_decision: pairing_decision,
                            pairing_checked: true,
                            policy_enforced,
                        }
                    }
                    MultiChannelAllowFrom::AllowlistOnly => {
                        let pairing_decision =
                            self.evaluate_pairing_decision(event, &policy_channel, now_unix_ms);
                        let final_decision = match &pairing_decision {
                            MultiChannelPairingDecision::Allow { reason_code }
                                if reason_code == "allow_allowlist"
                                    || reason_code == "allow_allowlist_and_pairing" =>
                            {
                                MultiChannelPairingDecision::Allow {
                                    reason_code: reason_code.clone(),
                                }
                            }
                            MultiChannelPairingDecision::Allow { .. } => {
                                MultiChannelPairingDecision::Deny {
                                    reason_code: POLICY_REASON_DENY_ALLOWLIST_ONLY.to_string(),
                                }
                            }
                            MultiChannelPairingDecision::Deny { reason_code } => {
                                MultiChannelPairingDecision::Deny {
                                    reason_code: reason_code.clone(),
                                }
                            }
                        };
                        MultiChannelAccessDecision {
                            policy_channel,
                            channel_policy,
                            pairing_decision,
                            final_decision,
                            pairing_checked: true,
                            policy_enforced: true,
                        }
                    }
                }
            }
        }
    }

    fn evaluate_pairing_decision(
        &self,
        event: &MultiChannelInboundEvent,
        policy_channel: &str,
        now_unix_ms: u64,
    ) -> MultiChannelPairingDecision {
        match self.config.pairing_evaluator.evaluate_pairing(
            &self.config.state_dir,
            policy_channel,
            event.actor_id.as_str(),
            now_unix_ms,
        ) {
            Ok(decision) => decision,
            Err(error) => {
                eprintln!(
                    "multi-channel pairing policy evaluation failed: transport={} event_id={} actor_id={} policy_channel={} error={error}",
                    event.transport.as_str(),
                    event.event_id.trim(),
                    event.actor_id.trim(),
                    policy_channel
                );
                MultiChannelPairingDecision::Deny {
                    reason_code: PAIRING_REASON_DENY_POLICY_EVALUATION_ERROR.to_string(),
                }
            }
        }
    }

    fn execute_tau_command_if_requested(
        &self,
        event: &MultiChannelInboundEvent,
        access_decision: &MultiChannelAccessDecision,
    ) -> Option<MultiChannelCommandExecution> {
        let parsed = match parse_multi_channel_tau_command(event.text.as_str()) {
            Ok(parsed) => parsed,
            Err(reason_code) => {
                let response = format!(
                    "invalid `/tau` command.\n\n{}",
                    render_multi_channel_tau_command_help()
                );
                return Some(build_multi_channel_command_execution(
                    "invalid",
                    COMMAND_STATUS_FAILED,
                    reason_code.as_str(),
                    response.as_str(),
                ));
            }
        };
        let command = parsed?;
        if command_requires_operator_scope(&command)
            && !multi_channel_command_operator_allowed(access_decision)
        {
            let response =
                "command denied: this `/tau` command requires allowlisted operator scope.";
            let command_line = render_multi_channel_tau_command_line(&command);
            return Some(build_multi_channel_command_execution(
                command_line.as_str(),
                COMMAND_STATUS_FAILED,
                COMMAND_REASON_RBAC_DENIED,
                response,
            ));
        }

        match command {
            MultiChannelTauCommand::Help => Some(build_multi_channel_command_execution(
                "help",
                COMMAND_STATUS_REPORTED,
                COMMAND_REASON_HELP_REPORTED,
                render_multi_channel_tau_command_help().as_str(),
            )),
            MultiChannelTauCommand::Status => Some(build_multi_channel_command_execution(
                "status",
                COMMAND_STATUS_REPORTED,
                COMMAND_REASON_STATUS_REPORTED,
                self.render_multi_channel_status_command_report().as_str(),
            )),
            MultiChannelTauCommand::AuthStatus { provider } => {
                let Some(auth_handler) = &self.config.command_handlers.auth else {
                    let response = "command unavailable: auth status handler is not configured.";
                    return Some(build_multi_channel_command_execution(
                        "auth status",
                        COMMAND_STATUS_FAILED,
                        COMMAND_REASON_AUTH_STATUS_FAILED,
                        response,
                    ));
                };
                let output = auth_handler.execute_auth_status(provider.as_deref());
                let failed = output.trim_start().starts_with("auth error:");
                let reason_code = if failed {
                    COMMAND_REASON_AUTH_STATUS_FAILED
                } else {
                    COMMAND_REASON_AUTH_STATUS_REPORTED
                };
                let status = if failed {
                    COMMAND_STATUS_FAILED
                } else {
                    COMMAND_STATUS_REPORTED
                };
                let command_line = if let Some(provider) = provider.as_deref() {
                    format!("auth status {provider}")
                } else {
                    "auth status".to_string()
                };
                Some(build_multi_channel_command_execution(
                    command_line.as_str(),
                    status,
                    reason_code,
                    output.as_str(),
                ))
            }
            MultiChannelTauCommand::Doctor { online } => {
                let Some(doctor_handler) = &self.config.command_handlers.doctor else {
                    let response = "command unavailable: doctor handler is not configured.";
                    return Some(build_multi_channel_command_execution(
                        "doctor",
                        COMMAND_STATUS_FAILED,
                        COMMAND_REASON_DOCTOR_REPORTED,
                        response,
                    ));
                };
                let output = doctor_handler.execute_doctor(online);
                let command_line = if online { "doctor --online" } else { "doctor" };
                Some(build_multi_channel_command_execution(
                    command_line,
                    COMMAND_STATUS_REPORTED,
                    COMMAND_REASON_DOCTOR_REPORTED,
                    output.as_str(),
                ))
            }
            MultiChannelTauCommand::Approvals { action, args } => {
                let Some(approvals_handler) = &self.config.command_handlers.approvals else {
                    let response = "command unavailable: approvals handler is not configured.";
                    return Some(build_multi_channel_command_execution(
                        "approvals",
                        COMMAND_STATUS_FAILED,
                        COMMAND_REASON_APPROVALS_FAILED,
                        response,
                    ));
                };
                let decision_actor = if matches!(
                    action,
                    MultiChannelTauApprovalsAction::Approve
                        | MultiChannelTauApprovalsAction::Reject
                ) {
                    match build_multi_channel_approver_actor(event) {
                        Some(actor) => Some(actor),
                        None => {
                            let response =
                                "command denied: missing transport actor mapping for approval decision.";
                            return Some(build_multi_channel_command_execution(
                                "approvals",
                                COMMAND_STATUS_FAILED,
                                COMMAND_REASON_APPROVALS_ACTOR_MAPPING_FAILED,
                                response,
                            ));
                        }
                    }
                } else {
                    None
                };

                let output = approvals_handler.execute_approvals(
                    &self.config.state_dir,
                    args.as_str(),
                    decision_actor.as_deref(),
                );
                let failed = output.trim_start().starts_with("approvals error:");
                let (status, reason_code) = if failed {
                    (
                        COMMAND_STATUS_FAILED,
                        approvals_failure_reason_code(&action, output.as_str()),
                    )
                } else {
                    (
                        COMMAND_STATUS_REPORTED,
                        approvals_success_reason_code(&action),
                    )
                };
                let command_line = format!("approvals {}", args.trim());
                Some(build_multi_channel_command_execution(
                    command_line.as_str(),
                    status,
                    reason_code,
                    output.as_str(),
                ))
            }
        }
    }

    fn render_multi_channel_status_command_report(&self) -> String {
        let classification = self.state.health.classify();
        format!(
            "multi-channel status: health_state={} health_reason={} failure_streak={} queue_depth={} processed_event_keys={} typing_events={} presence_events={} usage_records={} usage_chars={} usage_chunks={} usage_cost_micros={}",
            classification.state.as_str(),
            classification.reason,
            self.state.health.failure_streak,
            self.state.health.queue_depth,
            self.state.processed_event_keys.len(),
            self.state.telemetry.typing_events_emitted,
            self.state.telemetry.presence_events_emitted,
            self.state.telemetry.usage_summary_records,
            self.state.telemetry.usage_response_chars,
            self.state.telemetry.usage_chunks,
            self.state.telemetry.usage_estimated_cost_micros
        )
    }

    async fn persist_event(
        &mut self,
        event: &MultiChannelInboundEvent,
        event_key: &str,
        access_decision: &MultiChannelAccessDecision,
        route_decision: &MultiChannelRouteDecision,
    ) -> Result<PersistEventOutcome> {
        let mut outcome = PersistEventOutcome::default();
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            event.transport.as_str(),
            &route_decision.session_key,
        )?;
        let existing_logs = store.load_log_entries()?;
        let existing_context = store.load_context_entries()?;
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let media_report = process_media_attachments(event, &self.config.media);
        let media_prompt_context = render_media_prompt_context(&media_report);
        let media_payload =
            serde_json::to_value(&media_report).context("serialize media understanding payload")?;
        let user_context_text = build_user_context_text(event, media_prompt_context.as_deref());
        let pairing_status = pairing_decision_status(&access_decision.pairing_decision);
        let pairing_reason_code = access_decision.pairing_decision.reason_code().to_string();
        let route_payload = route_decision_trace_payload(event, event_key, route_decision);
        let pairing_payload = json!({
            "decision": pairing_status,
            "reason_code": pairing_reason_code,
            "channel": access_decision.policy_channel,
            "actor_id": event.actor_id.trim(),
            "checked": access_decision.pairing_checked,
        });
        let channel_policy_payload = json!({
            "decision": access_decision.channel_policy.decision.as_str(),
            "reason_code": access_decision.channel_policy.decision.reason_code(),
            "channel": access_decision.channel_policy.policy_channel,
            "matched_policy_key": access_decision.channel_policy.matched_policy_key,
            "conversation_kind": access_decision.channel_policy.conversation_kind.as_str(),
            "dm_policy": access_decision.channel_policy.policy.dm_policy.as_str(),
            "allow_from": access_decision.channel_policy.policy.allow_from.as_str(),
            "group_policy": access_decision.channel_policy.policy.group_policy.as_str(),
            "require_mention": access_decision.channel_policy.policy.require_mention,
            "mention_present": access_decision.channel_policy.mention_present,
        });
        let mut inbound_payload =
            serde_json::to_value(event).context("serialize inbound event payload")?;
        if let Value::Object(map) = &mut inbound_payload {
            map.insert("pairing".to_string(), pairing_payload.clone());
            map.insert("channel_policy".to_string(), channel_policy_payload.clone());
            map.insert("route".to_string(), route_payload.clone());
            map.insert("media_understanding".to_string(), media_payload.clone());
            map.insert(
                "route_session_key".to_string(),
                Value::String(route_decision.session_key.clone()),
            );
        }

        if !log_contains_event_direction(&existing_logs, event_key, "inbound") {
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "inbound".to_string(),
                event_key: Some(event_key.to_string()),
                source: event.transport.as_str().to_string(),
                payload: inbound_payload,
            })?;
            append_multi_channel_route_trace(
                &self
                    .config
                    .state_dir
                    .join(MULTI_CHANNEL_ROUTE_TRACES_LOG_FILE),
                &route_payload,
            )?;
        }

        if let MultiChannelPairingDecision::Deny { reason_code } = &access_decision.final_decision {
            if !log_contains_outbound_status(&existing_logs, event_key, "denied") {
                store.append_log_entry(&ChannelLogEntry {
                    timestamp_unix_ms: current_unix_timestamp_ms(),
                    direction: "outbound".to_string(),
                    event_key: Some(event_key.to_string()),
                    source: "tau-multi-channel-runner".to_string(),
                    payload: json!({
                        "status": "denied",
                        "reason_code": reason_code,
                        "policy_channel": access_decision.policy_channel,
                        "actor_id": event.actor_id.trim(),
                        "event_key": event_key,
                        "transport": event.transport.as_str(),
                        "conversation_id": event.conversation_id.trim(),
                        "route_session_key": route_decision.session_key.as_str(),
                        "route": route_payload.clone(),
                        "channel_policy": channel_policy_payload,
                        "pairing": pairing_payload,
                        "media_understanding": media_payload,
                    }),
                })?;
            }
            return Ok(outcome);
        }

        let command_execution = self.execute_tau_command_if_requested(event, access_decision);
        let command_payload = command_execution
            .as_ref()
            .map(multi_channel_command_payload);
        let response_text = command_execution
            .as_ref()
            .map(|execution| execution.response_text.clone())
            .unwrap_or_else(|| render_response(event));
        let response_chars = response_text.chars().count();
        let emit_lifecycle =
            should_emit_typing_presence_lifecycle(event, &response_text, &self.config.telemetry);
        if emit_lifecycle {
            if !log_contains_outbound_status(
                &existing_logs,
                event_key,
                TELEMETRY_STATUS_TYPING_STARTED,
            ) {
                store.append_log_entry(&ChannelLogEntry {
                    timestamp_unix_ms: current_unix_timestamp_ms(),
                    direction: "outbound".to_string(),
                    event_key: Some(event_key.to_string()),
                    source: "tau-multi-channel-runner".to_string(),
                    payload: build_telemetry_lifecycle_payload(&TelemetryLifecyclePayloadContext {
                        status: TELEMETRY_STATUS_TYPING_STARTED,
                        telemetry_kind: "typing",
                        telemetry_state: "started",
                        signal: channel_typing_signal(event.transport),
                        event,
                        event_key,
                        route_decision,
                        include_identifiers: self.config.telemetry.include_identifiers,
                    }),
                })?;
                self.record_typing_telemetry(event.transport.as_str());
                outcome.typing_events_emitted = outcome.typing_events_emitted.saturating_add(1);
            }
            if !log_contains_outbound_status(
                &existing_logs,
                event_key,
                TELEMETRY_STATUS_PRESENCE_ACTIVE,
            ) {
                store.append_log_entry(&ChannelLogEntry {
                    timestamp_unix_ms: current_unix_timestamp_ms(),
                    direction: "outbound".to_string(),
                    event_key: Some(event_key.to_string()),
                    source: "tau-multi-channel-runner".to_string(),
                    payload: build_telemetry_lifecycle_payload(&TelemetryLifecyclePayloadContext {
                        status: TELEMETRY_STATUS_PRESENCE_ACTIVE,
                        telemetry_kind: "presence",
                        telemetry_state: "active",
                        signal: channel_presence_signal(event.transport, true),
                        event,
                        event_key,
                        route_decision,
                        include_identifiers: self.config.telemetry.include_identifiers,
                    }),
                })?;
                self.record_presence_telemetry(event.transport.as_str());
                outcome.presence_events_emitted = outcome.presence_events_emitted.saturating_add(1);
            }
        }

        let delivery_result = match self
            .outbound_dispatcher
            .deliver(event, &response_text)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let failure_context = DeliveryFailureLogContext {
                    event,
                    event_key,
                    route_decision,
                    route_payload: &route_payload,
                    pairing_payload: &pairing_payload,
                    channel_policy_payload: &channel_policy_payload,
                    delivery_mode: self.outbound_dispatcher.mode().as_str(),
                    command_payload: command_payload.as_ref(),
                };
                append_delivery_failure_log(&store, &failure_context, &error)?;
                return Err(anyhow!(
                    "multi-channel outbound delivery failed: reason_code={} retryable={} chunk={}/{} detail={}",
                    error.reason_code,
                    error.retryable,
                    error.chunk_index,
                    error.chunk_count,
                    error.detail
                ));
            }
        };

        if emit_lifecycle {
            if !log_contains_outbound_status(
                &existing_logs,
                event_key,
                TELEMETRY_STATUS_TYPING_STOPPED,
            ) {
                store.append_log_entry(&ChannelLogEntry {
                    timestamp_unix_ms: current_unix_timestamp_ms(),
                    direction: "outbound".to_string(),
                    event_key: Some(event_key.to_string()),
                    source: "tau-multi-channel-runner".to_string(),
                    payload: build_telemetry_lifecycle_payload(&TelemetryLifecyclePayloadContext {
                        status: TELEMETRY_STATUS_TYPING_STOPPED,
                        telemetry_kind: "typing",
                        telemetry_state: "stopped",
                        signal: channel_typing_signal(event.transport),
                        event,
                        event_key,
                        route_decision,
                        include_identifiers: self.config.telemetry.include_identifiers,
                    }),
                })?;
                self.record_typing_telemetry(event.transport.as_str());
                outcome.typing_events_emitted = outcome.typing_events_emitted.saturating_add(1);
            }
            if !log_contains_outbound_status(
                &existing_logs,
                event_key,
                TELEMETRY_STATUS_PRESENCE_IDLE,
            ) {
                store.append_log_entry(&ChannelLogEntry {
                    timestamp_unix_ms: current_unix_timestamp_ms(),
                    direction: "outbound".to_string(),
                    event_key: Some(event_key.to_string()),
                    source: "tau-multi-channel-runner".to_string(),
                    payload: build_telemetry_lifecycle_payload(&TelemetryLifecyclePayloadContext {
                        status: TELEMETRY_STATUS_PRESENCE_IDLE,
                        telemetry_kind: "presence",
                        telemetry_state: "idle",
                        signal: channel_presence_signal(event.transport, false),
                        event,
                        event_key,
                        route_decision,
                        include_identifiers: self.config.telemetry.include_identifiers,
                    }),
                })?;
                self.record_presence_telemetry(event.transport.as_str());
                outcome.presence_events_emitted = outcome.presence_events_emitted.saturating_add(1);
            }
        }

        if let Some(user_context_text) = user_context_text {
            if !context_contains_entry(&existing_context, "user", &user_context_text) {
                store.append_context_entry(&ChannelContextEntry {
                    timestamp_unix_ms,
                    role: "user".to_string(),
                    text: user_context_text,
                })?;
            }
        }

        let delivery_payload =
            serde_json::to_value(&delivery_result).context("serialize delivery payload")?;
        if !log_contains_outbound_response(&existing_logs, event_key, &response_text) {
            let mut payload = json!({
                "response": response_text,
                "event_key": event_key,
                "transport": event.transport.as_str(),
                "conversation_id": event.conversation_id.trim(),
                "route_session_key": route_decision.session_key.as_str(),
                "route": route_payload,
                "pairing": pairing_payload,
                "channel_policy": channel_policy_payload,
                "media_understanding": media_payload,
                "delivery": delivery_payload,
            });
            if let Some(command_payload) = command_payload.as_ref() {
                if let Value::Object(map) = &mut payload {
                    map.insert("command".to_string(), command_payload.clone());
                }
            }
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms: current_unix_timestamp_ms(),
                direction: "outbound".to_string(),
                event_key: Some(event_key.to_string()),
                source: "tau-multi-channel-runner".to_string(),
                payload,
            })?;
        }
        if !context_contains_entry(&existing_context, "assistant", &response_text) {
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms: current_unix_timestamp_ms(),
                role: "assistant".to_string(),
                text: response_text,
            })?;
        }

        if self.config.telemetry.usage_summary_enabled {
            let usage_cost_micros = extract_usage_estimated_cost_micros(event).unwrap_or(0);
            self.record_usage_summary_telemetry(
                event.transport.as_str(),
                response_chars,
                delivery_result.chunk_count,
                usage_cost_micros,
            );
            outcome.usage_summary_records = outcome.usage_summary_records.saturating_add(1);
            outcome.usage_response_chars =
                outcome.usage_response_chars.saturating_add(response_chars);
            outcome.usage_chunks = outcome
                .usage_chunks
                .saturating_add(delivery_result.chunk_count);
            outcome.usage_estimated_cost_micros = outcome
                .usage_estimated_cost_micros
                .saturating_add(usage_cost_micros);
        }

        Ok(outcome)
    }

    fn record_typing_telemetry(&mut self, transport: &str) {
        self.state.telemetry.typing_events_emitted =
            self.state.telemetry.typing_events_emitted.saturating_add(1);
        increment_counter_map(
            &mut self.state.telemetry.typing_events_by_transport,
            transport,
            1,
        );
    }

    fn record_presence_telemetry(&mut self, transport: &str) {
        self.state.telemetry.presence_events_emitted = self
            .state
            .telemetry
            .presence_events_emitted
            .saturating_add(1);
        increment_counter_map(
            &mut self.state.telemetry.presence_events_by_transport,
            transport,
            1,
        );
    }

    fn record_usage_summary_telemetry(
        &mut self,
        transport: &str,
        response_chars: usize,
        usage_chunks: usize,
        usage_cost_micros: u64,
    ) {
        self.state.telemetry.usage_summary_records =
            self.state.telemetry.usage_summary_records.saturating_add(1);
        self.state.telemetry.usage_response_chars = self
            .state
            .telemetry
            .usage_response_chars
            .saturating_add(response_chars);
        self.state.telemetry.usage_chunks = self
            .state
            .telemetry
            .usage_chunks
            .saturating_add(usage_chunks);
        self.state.telemetry.usage_estimated_cost_micros = self
            .state
            .telemetry
            .usage_estimated_cost_micros
            .saturating_add(usage_cost_micros);
        increment_counter_map(
            &mut self.state.telemetry.usage_summary_records_by_transport,
            transport,
            1,
        );
        increment_counter_map(
            &mut self.state.telemetry.usage_response_chars_by_transport,
            transport,
            response_chars,
        );
        increment_counter_map(
            &mut self.state.telemetry.usage_chunks_by_transport,
            transport,
            usage_chunks,
        );
        increment_counter_u64_map(
            &mut self
                .state
                .telemetry
                .usage_estimated_cost_micros_by_transport,
            transport,
            usage_cost_micros,
        );
    }

    fn record_processed_event(&mut self, event_key: &str) {
        if self.processed_event_keys.contains(event_key) {
            return;
        }
        self.state.processed_event_keys.push(event_key.to_string());
        self.processed_event_keys.insert(event_key.to_string());
        if self.state.processed_event_keys.len() > self.config.processed_event_cap {
            let overflow = self
                .state
                .processed_event_keys
                .len()
                .saturating_sub(self.config.processed_event_cap);
            let removed = self.state.processed_event_keys.drain(0..overflow);
            for key in removed {
                self.processed_event_keys.remove(&key);
            }
        }
    }
}

fn approvals_success_reason_code(action: &MultiChannelTauApprovalsAction) -> &'static str {
    match action {
        MultiChannelTauApprovalsAction::List => COMMAND_REASON_APPROVALS_LIST_REPORTED,
        MultiChannelTauApprovalsAction::Approve => COMMAND_REASON_APPROVALS_APPROVED,
        MultiChannelTauApprovalsAction::Reject => COMMAND_REASON_APPROVALS_REJECTED,
    }
}

fn approvals_failure_reason_code(
    action: &MultiChannelTauApprovalsAction,
    output: &str,
) -> &'static str {
    if matches!(
        action,
        MultiChannelTauApprovalsAction::Approve | MultiChannelTauApprovalsAction::Reject
    ) {
        if output.contains("not found") {
            return COMMAND_REASON_APPROVALS_UNKNOWN_REQUEST;
        }
        if output.contains("is not pending") {
            return COMMAND_REASON_APPROVALS_STALE_REQUEST;
        }
    }
    COMMAND_REASON_APPROVALS_FAILED
}

fn build_multi_channel_approver_actor(event: &MultiChannelInboundEvent) -> Option<String> {
    let conversation_id = event.conversation_id.trim();
    let actor_id = event.actor_id.trim();
    if conversation_id.is_empty() || actor_id.is_empty() {
        return None;
    }
    Some(format!(
        "{}:{}:{}",
        event.transport.as_str(),
        conversation_id,
        actor_id
    ))
}

fn pairing_policy_channel(event: &MultiChannelInboundEvent) -> String {
    format!(
        "{}:{}",
        event.transport.as_str(),
        event.conversation_id.trim()
    )
}

fn pairing_decision_status(decision: &MultiChannelPairingDecision) -> &'static str {
    if matches!(decision, MultiChannelPairingDecision::Allow { .. }) {
        "allow"
    } else {
        "deny"
    }
}

fn pairing_decision_is_enforced(decision: &MultiChannelPairingDecision) -> bool {
    decision.reason_code() != PAIRING_REASON_ALLOW_PERMISSIVE_MODE
}

fn should_emit_typing_presence_lifecycle(
    event: &MultiChannelInboundEvent,
    response_text: &str,
    config: &MultiChannelTelemetryConfig,
) -> bool {
    if !config.typing_presence_enabled {
        return false;
    }
    if event
        .metadata
        .get("telemetry_force_typing_presence")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return true;
    }
    response_text.chars().count() >= config.typing_presence_min_response_chars
}

fn channel_typing_signal(transport: MultiChannelTransport) -> &'static str {
    match transport {
        MultiChannelTransport::Telegram => "telegram:typing",
        MultiChannelTransport::Discord => "discord:typing",
        MultiChannelTransport::Whatsapp => "whatsapp:typing",
    }
}

fn channel_presence_signal(transport: MultiChannelTransport, active: bool) -> &'static str {
    match (transport, active) {
        (MultiChannelTransport::Telegram, true) => "telegram:online",
        (MultiChannelTransport::Telegram, false) => "telegram:idle",
        (MultiChannelTransport::Discord, true) => "discord:online",
        (MultiChannelTransport::Discord, false) => "discord:idle",
        (MultiChannelTransport::Whatsapp, true) => "whatsapp:available",
        (MultiChannelTransport::Whatsapp, false) => "whatsapp:idle",
    }
}

struct TelemetryLifecyclePayloadContext<'a> {
    status: &'a str,
    telemetry_kind: &'a str,
    telemetry_state: &'a str,
    signal: &'a str,
    event: &'a MultiChannelInboundEvent,
    event_key: &'a str,
    route_decision: &'a MultiChannelRouteDecision,
    include_identifiers: bool,
}

fn build_telemetry_lifecycle_payload(context: &TelemetryLifecyclePayloadContext<'_>) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "status".to_string(),
        Value::String(context.status.to_string()),
    );
    payload.insert(
        "record_type".to_string(),
        Value::String("multi_channel_telemetry_lifecycle_v1".to_string()),
    );
    payload.insert(
        "reason_code".to_string(),
        Value::String("telemetry_lifecycle_emitted".to_string()),
    );
    payload.insert(
        "telemetry_kind".to_string(),
        Value::String(context.telemetry_kind.to_string()),
    );
    payload.insert(
        "telemetry_state".to_string(),
        Value::String(context.telemetry_state.to_string()),
    );
    payload.insert(
        "signal".to_string(),
        Value::String(context.signal.to_string()),
    );
    payload.insert(
        "transport".to_string(),
        Value::String(context.event.transport.as_str().to_string()),
    );
    payload.insert(
        "event_key".to_string(),
        Value::String(context.event_key.to_string()),
    );
    if context.include_identifiers {
        payload.insert(
            "conversation_id".to_string(),
            Value::String(context.event.conversation_id.trim().to_string()),
        );
        payload.insert(
            "actor_id".to_string(),
            Value::String(context.event.actor_id.trim().to_string()),
        );
        payload.insert(
            "route_session_key".to_string(),
            Value::String(context.route_decision.session_key.clone()),
        );
    }
    Value::Object(payload)
}

fn extract_usage_estimated_cost_micros(event: &MultiChannelInboundEvent) -> Option<u64> {
    if let Some(value) = event
        .metadata
        .get("usage_cost_micros")
        .and_then(serde_json::Value::as_u64)
    {
        return Some(value);
    }
    let usd = event
        .metadata
        .get("usage_cost_usd")
        .and_then(serde_json::Value::as_f64)?;
    if !usd.is_finite() || usd.is_sign_negative() {
        return None;
    }
    Some((usd * 1_000_000.0).round() as u64)
}

fn increment_counter_map(map: &mut BTreeMap<String, usize>, key: &str, delta: usize) {
    let normalized = key.trim();
    if normalized.is_empty() || delta == 0 {
        return;
    }
    let entry = map.entry(normalized.to_string()).or_insert(0);
    *entry = entry.saturating_add(delta);
}

fn increment_counter_u64_map(map: &mut BTreeMap<String, u64>, key: &str, delta: u64) {
    let normalized = key.trim();
    if normalized.is_empty() || delta == 0 {
        return;
    }
    let entry = map.entry(normalized.to_string()).or_insert(0);
    *entry = entry.saturating_add(delta);
}

fn log_contains_event_direction(
    logs: &[ChannelLogEntry],
    event_key: &str,
    direction: &str,
) -> bool {
    logs.iter()
        .any(|entry| entry.direction == direction && entry.event_key.as_deref() == Some(event_key))
}

fn log_contains_outbound_status(logs: &[ChannelLogEntry], event_key: &str, status: &str) -> bool {
    logs.iter().any(|entry| {
        entry.direction == "outbound"
            && entry.event_key.as_deref() == Some(event_key)
            && entry.payload.get("status").and_then(Value::as_str) == Some(status)
    })
}

fn log_contains_outbound_response(
    logs: &[ChannelLogEntry],
    event_key: &str,
    response: &str,
) -> bool {
    logs.iter().any(|entry| {
        entry.direction == "outbound"
            && entry.event_key.as_deref() == Some(event_key)
            && entry.payload.get("response").and_then(Value::as_str) == Some(response)
    })
}

fn context_contains_entry(entries: &[ChannelContextEntry], role: &str, text: &str) -> bool {
    entries
        .iter()
        .any(|entry| entry.role == role && entry.text == text)
}

struct DeliveryFailureLogContext<'a> {
    event: &'a MultiChannelInboundEvent,
    event_key: &'a str,
    route_decision: &'a MultiChannelRouteDecision,
    route_payload: &'a Value,
    pairing_payload: &'a Value,
    channel_policy_payload: &'a Value,
    delivery_mode: &'a str,
    command_payload: Option<&'a Value>,
}

fn append_delivery_failure_log(
    store: &ChannelStore,
    context: &DeliveryFailureLogContext<'_>,
    error: &MultiChannelOutboundDeliveryError,
) -> Result<()> {
    let mut payload = json!({
        "status": "delivery_failed",
        "reason_code": error.reason_code,
        "detail": error.detail,
        "retryable": error.retryable,
        "chunk_index": error.chunk_index,
        "chunk_count": error.chunk_count,
        "endpoint": error.endpoint,
        "http_status": error.http_status,
        "request_body": error.request_body,
        "delivery_mode": context.delivery_mode,
        "event_key": context.event_key,
        "transport": context.event.transport.as_str(),
        "conversation_id": context.event.conversation_id.trim(),
        "route_session_key": context.route_decision.session_key.as_str(),
        "route": context.route_payload,
        "pairing": context.pairing_payload,
        "channel_policy": context.channel_policy_payload,
    });
    if let Some(command_payload) = context.command_payload {
        if let Value::Object(map) = &mut payload {
            map.insert("command".to_string(), command_payload.clone());
        }
    }
    store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        direction: "outbound".to_string(),
        event_key: Some(context.event_key.to_string()),
        source: "tau-multi-channel-runner".to_string(),
        payload,
    })
}

fn load_multi_channel_live_events(ingress_dir: &Path) -> Result<Vec<MultiChannelInboundEvent>> {
    std::fs::create_dir_all(ingress_dir)
        .with_context(|| format!("failed to create {}", ingress_dir.display()))?;
    let mut events = Vec::new();
    for (transport, file_name) in MULTI_CHANNEL_LIVE_INGRESS_SOURCES {
        let path = ingress_dir.join(file_name);
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for (index, line) in raw.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match parse_multi_channel_live_inbound_envelope(trimmed) {
                Ok(event) => {
                    if event.transport.as_str() != transport {
                        eprintln!(
                            "multi-channel live ingress skipped event: file={} line={} reason=transport_mismatch expected={} actual={}",
                            path.display(),
                            index + 1,
                            transport,
                            event.transport.as_str()
                        );
                        continue;
                    }
                    events.push(event);
                }
                Err(error) => {
                    eprintln!(
                        "multi-channel live ingress parse failure: file={} line={} reason_code={} detail={}",
                        path.display(),
                        index + 1,
                        error.code.as_str(),
                        error.message
                    );
                }
            }
        }
    }
    Ok(events)
}

fn build_transport_health_snapshot(
    summary: &MultiChannelRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_events = summary
        .discovered_events
        .saturating_sub(summary.queued_events);
    let failure_streak = if summary.failed_events > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };
    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_events,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_events,
        last_cycle_processed: summary
            .completed_events
            .saturating_add(summary.failed_events)
            .saturating_add(summary.duplicate_skips),
        last_cycle_completed: summary.completed_events,
        last_cycle_failed: summary.failed_events,
        last_cycle_duplicates: summary.duplicate_skips,
    }
}

fn cycle_reason_codes(summary: &MultiChannelRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    let mut operational_issue_detected = false;
    if summary.discovered_events > summary.queued_events {
        operational_issue_detected = true;
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        operational_issue_detected = true;
        codes.push("duplicate_events_skipped".to_string());
    }
    if summary.retry_attempts > 0 {
        operational_issue_detected = true;
        codes.push("retry_attempted".to_string());
    }
    if summary.transient_failures > 0 {
        operational_issue_detected = true;
        codes.push("transient_failures_observed".to_string());
    }
    if summary.failed_events > 0 {
        operational_issue_detected = true;
        codes.push("event_processing_failed".to_string());
    }
    if !operational_issue_detected {
        codes.push("healthy_cycle".to_string());
    }
    if summary.policy_checked_events > 0 {
        if summary.policy_enforced_events > 0 {
            codes.push("pairing_policy_enforced".to_string());
        } else {
            codes.push("pairing_policy_permissive".to_string());
        }
    }
    if summary.policy_denied_events > 0 {
        codes.push("pairing_policy_denied_events".to_string());
    }
    if summary.typing_events_emitted > 0 || summary.presence_events_emitted > 0 {
        codes.push("telemetry_lifecycle_emitted".to_string());
    }
    if summary.usage_summary_records > 0 {
        codes.push("telemetry_usage_summary_emitted".to_string());
    }
    codes
}

fn append_multi_channel_cycle_report(
    path: &Path,
    summary: &MultiChannelRuntimeSummary,
    health: &TransportHealthSnapshot,
    health_reason: &str,
    reason_codes: &[String],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = MultiChannelRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        discovered_events: summary.discovered_events,
        queued_events: summary.queued_events,
        completed_events: summary.completed_events,
        duplicate_skips: summary.duplicate_skips,
        transient_failures: summary.transient_failures,
        retry_attempts: summary.retry_attempts,
        failed_events: summary.failed_events,
        policy_checked_events: summary.policy_checked_events,
        policy_enforced_events: summary.policy_enforced_events,
        policy_allowed_events: summary.policy_allowed_events,
        policy_denied_events: summary.policy_denied_events,
        typing_events_emitted: summary.typing_events_emitted,
        presence_events_emitted: summary.presence_events_emitted,
        usage_summary_records: summary.usage_summary_records,
        usage_response_chars: summary.usage_response_chars,
        usage_chunks: summary.usage_chunks,
        usage_estimated_cost_micros: summary.usage_estimated_cost_micros,
        backlog_events: summary
            .discovered_events
            .saturating_sub(summary.queued_events),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize multi-channel runtime report")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn append_multi_channel_route_trace(path: &Path, payload: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let line = serde_json::to_string(payload).context("serialize multi-channel route trace")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn build_user_context_text(
    event: &MultiChannelInboundEvent,
    media_prompt_context: Option<&str>,
) -> Option<String> {
    let text = event.text.trim();
    let media = media_prompt_context.map(str::trim).unwrap_or_default();
    if text.is_empty() && media.is_empty() {
        return None;
    }
    if media.is_empty() {
        return Some(text.to_string());
    }
    if text.is_empty() {
        return Some(media.to_string());
    }
    Some(format!("{text}\n\n{media}"))
}

fn parse_multi_channel_tau_command(
    command_text: &str,
) -> std::result::Result<Option<MultiChannelTauCommand>, String> {
    let trimmed = command_text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut tokens = trimmed.split_whitespace();
    let Some(command_prefix) = tokens.next() else {
        return Ok(None);
    };
    let is_tau_prefix = command_prefix == "/tau" || command_prefix.starts_with("/tau@");
    if !is_tau_prefix {
        return Ok(None);
    }

    let Some(subcommand) = tokens.next() else {
        return Ok(Some(MultiChannelTauCommand::Help));
    };
    match subcommand {
        "help" => {
            if tokens.next().is_some() {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            }
            Ok(Some(MultiChannelTauCommand::Help))
        }
        "status" => {
            if tokens.next().is_some() {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            }
            Ok(Some(MultiChannelTauCommand::Status))
        }
        "auth" => {
            let Some(action) = tokens.next() else {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            };
            if action != "status" {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            }
            let provider = tokens.next().map(|value| value.trim().to_ascii_lowercase());
            if let Some(provider_token) = provider.as_deref() {
                if !matches!(provider_token, "openai" | "anthropic" | "google") {
                    return Err(COMMAND_REASON_INVALID_ARGS.to_string());
                }
            }
            if tokens.next().is_some() {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            }
            Ok(Some(MultiChannelTauCommand::AuthStatus { provider }))
        }
        "doctor" => {
            let mut online = false;
            if let Some(option) = tokens.next() {
                if option == "--online" {
                    online = true;
                } else {
                    return Err(COMMAND_REASON_INVALID_ARGS.to_string());
                }
            }
            if tokens.next().is_some() {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            }
            Ok(Some(MultiChannelTauCommand::Doctor { online }))
        }
        "approvals" => {
            let remaining = tokens.collect::<Vec<_>>();
            parse_multi_channel_tau_approvals_command(&remaining)
        }
        _ => Err(COMMAND_REASON_UNKNOWN.to_string()),
    }
}

fn parse_multi_channel_tau_approvals_command(
    tokens: &[&str],
) -> std::result::Result<Option<MultiChannelTauCommand>, String> {
    let Some(action) = tokens.first().copied() else {
        return Err(COMMAND_REASON_INVALID_ARGS.to_string());
    };

    match action {
        "list" => {
            let mut emit_json = false;
            let mut status_filter = None;
            let mut index = 1usize;
            while index < tokens.len() {
                match tokens[index] {
                    "--json" => {
                        if emit_json {
                            return Err(COMMAND_REASON_INVALID_ARGS.to_string());
                        }
                        emit_json = true;
                        index = index.saturating_add(1);
                    }
                    "--status" => {
                        let Some(raw_status) = tokens.get(index.saturating_add(1)).copied() else {
                            return Err(COMMAND_REASON_INVALID_ARGS.to_string());
                        };
                        let normalized_status = raw_status.trim().to_ascii_lowercase();
                        if !matches!(
                            normalized_status.as_str(),
                            "pending" | "approved" | "rejected" | "expired" | "consumed"
                        ) {
                            return Err(COMMAND_REASON_INVALID_ARGS.to_string());
                        }
                        status_filter = Some(normalized_status);
                        index = index.saturating_add(2);
                    }
                    _ => return Err(COMMAND_REASON_INVALID_ARGS.to_string()),
                }
            }
            let mut args = vec!["list".to_string()];
            if emit_json {
                args.push("--json".to_string());
            }
            if let Some(status) = status_filter {
                args.push("--status".to_string());
                args.push(status);
            }
            Ok(Some(MultiChannelTauCommand::Approvals {
                action: MultiChannelTauApprovalsAction::List,
                args: args.join(" "),
            }))
        }
        "approve" | "reject" => {
            let Some(request_id) = tokens.get(1).map(|value| value.trim()) else {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            };
            if request_id.is_empty() {
                return Err(COMMAND_REASON_INVALID_ARGS.to_string());
            }
            let reason = tokens
                .iter()
                .skip(2)
                .filter(|token| !token.trim().is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            let mut args = format!("{action} {request_id}");
            if !reason.is_empty() {
                args.push(' ');
                args.push_str(reason.as_str());
            }
            let action = if action == "approve" {
                MultiChannelTauApprovalsAction::Approve
            } else {
                MultiChannelTauApprovalsAction::Reject
            };
            Ok(Some(MultiChannelTauCommand::Approvals { action, args }))
        }
        _ => Err(COMMAND_REASON_INVALID_ARGS.to_string()),
    }
}

fn render_multi_channel_tau_command_line(command: &MultiChannelTauCommand) -> String {
    match command {
        MultiChannelTauCommand::Help => "help".to_string(),
        MultiChannelTauCommand::Status => "status".to_string(),
        MultiChannelTauCommand::AuthStatus { provider } => {
            if let Some(provider) = provider.as_deref() {
                format!("auth status {provider}")
            } else {
                "auth status".to_string()
            }
        }
        MultiChannelTauCommand::Doctor { online } => {
            if *online {
                "doctor --online".to_string()
            } else {
                "doctor".to_string()
            }
        }
        MultiChannelTauCommand::Approvals { args, .. } => format!("approvals {args}"),
    }
}

fn render_multi_channel_tau_command_help() -> String {
    [
        "supported /tau commands:",
        "- /tau help",
        "- /tau status",
        "- /tau auth status [openai|anthropic|google]",
        "- /tau doctor [--online]",
        "- /tau approvals list [--json] [--status pending|approved|rejected|expired|consumed]",
        "- /tau approvals approve <request_id> [reason]",
        "- /tau approvals reject <request_id> [reason]",
    ]
    .join("\n")
}

fn render_multi_channel_command_response(
    command_line: &str,
    status: &str,
    reason_code: &str,
    content: &str,
) -> String {
    let body = if content.trim().is_empty() {
        "Tau command response."
    } else {
        content.trim()
    };
    format!(
        "{body}\n\nTau command `/tau {command_line}` | status `{status}` | reason_code `{reason_code}`"
    )
}

fn build_multi_channel_command_execution(
    command_line: &str,
    status: &str,
    reason_code: &str,
    content: &str,
) -> MultiChannelCommandExecution {
    MultiChannelCommandExecution {
        command_line: command_line.to_string(),
        status: status.to_string(),
        reason_code: reason_code.to_string(),
        response_text: render_multi_channel_command_response(
            command_line,
            status,
            reason_code,
            content,
        ),
    }
}

fn command_requires_operator_scope(command: &MultiChannelTauCommand) -> bool {
    matches!(
        command,
        MultiChannelTauCommand::AuthStatus { .. }
            | MultiChannelTauCommand::Doctor { .. }
            | MultiChannelTauCommand::Approvals { .. }
    )
}

fn multi_channel_command_operator_allowed(access_decision: &MultiChannelAccessDecision) -> bool {
    let reason_code = access_decision.final_decision.reason_code();
    reason_code == "allow_allowlist" || reason_code == "allow_allowlist_and_pairing"
}

fn multi_channel_command_payload(execution: &MultiChannelCommandExecution) -> Value {
    json!({
        "schema": "multi_channel_tau_command_v1",
        "command": execution.command_line,
        "status": execution.status,
        "reason_code": execution.reason_code,
    })
}

fn render_response(event: &MultiChannelInboundEvent) -> String {
    let transport = event.transport.as_str();
    let event_id = event.event_id.trim();
    if matches!(event.event_kind, MultiChannelEventKind::Command)
        || event.text.trim().starts_with('/')
    {
        return format!(
            "command acknowledged: transport={} event_id={} conversation={}",
            transport, event_id, event.conversation_id
        );
    }
    format!(
        "message processed: transport={} event_id={} text_chars={}",
        transport,
        event_id,
        event.text.chars().count()
    )
}

fn normalize_processed_keys(raw: &[String], cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for key in raw {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let owned = trimmed.to_string();
        if seen.insert(owned.clone()) {
            normalized.push(owned);
        }
    }
    if cap == 0 {
        return Vec::new();
    }
    if normalized.len() > cap {
        normalized.drain(0..normalized.len().saturating_sub(cap));
    }
    normalized
}

fn simulated_transient_failures(event: &MultiChannelInboundEvent) -> usize {
    event
        .metadata
        .get("simulate_transient_failures")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

fn retry_delay_ms(base_delay_ms: u64, jitter_ms: u64, attempt: usize, jitter_seed: &str) -> u64 {
    if base_delay_ms == 0 {
        return 0;
    }
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    let base_delay = base_delay_ms.saturating_mul(1_u64 << exponent);
    if jitter_ms == 0 {
        return base_delay;
    }
    let mut hasher = Sha256::new();
    hasher.update(jitter_seed.as_bytes());
    hasher.update(attempt.to_le_bytes());
    let digest = hasher.finalize();
    let mut seed_bytes = [0_u8; 8];
    seed_bytes.copy_from_slice(&digest[..8]);
    let deterministic_jitter = u64::from_le_bytes(seed_bytes) % jitter_ms.saturating_add(1);
    base_delay.saturating_add(deterministic_jitter)
}

async fn apply_retry_delay(base_delay_ms: u64, jitter_ms: u64, attempt: usize, jitter_seed: &str) {
    let delay_ms = retry_delay_ms(base_delay_ms, jitter_ms, attempt, jitter_seed);
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

fn load_multi_channel_runtime_state(path: &Path) -> Result<MultiChannelRuntimeState> {
    if !path.exists() {
        return Ok(MultiChannelRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<MultiChannelRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "multi-channel runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(MultiChannelRuntimeState::default());
        }
    };
    if parsed.schema_version != MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "multi-channel runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(MultiChannelRuntimeState::default());
    }
    Ok(parsed)
}

fn save_multi_channel_runtime_state(path: &Path, state: &MultiChannelRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize multi-channel state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use anyhow::{bail, Context, Result};
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde::Deserialize;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        current_unix_timestamp_ms, load_multi_channel_live_events,
        load_multi_channel_runtime_state, retry_delay_ms, run_multi_channel_live_runner,
        MultiChannelApprovalsCommandExecutor, MultiChannelAuthCommandExecutor,
        MultiChannelCommandHandlers, MultiChannelDoctorCommandExecutor,
        MultiChannelLiveRuntimeConfig, MultiChannelPairingDecision, MultiChannelPairingEvaluator,
        MultiChannelRuntime, MultiChannelRuntimeConfig, MultiChannelTelemetryConfig,
        PAIRING_REASON_ALLOW_PERMISSIVE_MODE,
    };
    use crate::multi_channel_contract::{
        load_multi_channel_contract_fixture, parse_multi_channel_contract_fixture,
        MultiChannelAttachment, MultiChannelEventKind, MultiChannelInboundEvent,
        MultiChannelTransport,
    };
    use crate::multi_channel_outbound::{MultiChannelOutboundConfig, MultiChannelOutboundMode};
    use tau_runtime::{ChannelStore, TransportHealthState};

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-channel-contract")
            .join(name)
    }

    fn live_fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-channel-live-ingress")
            .join(name)
    }

    #[derive(Clone, Default)]
    struct TestAuthHandler;

    impl MultiChannelAuthCommandExecutor for TestAuthHandler {
        fn execute_auth_status(&self, provider: Option<&str>) -> String {
            if let Some(provider) = provider {
                format!("auth status {provider}: ok")
            } else {
                "auth status ok".to_string()
            }
        }
    }

    #[derive(Clone, Default)]
    struct TestDoctorHandler;

    impl MultiChannelDoctorCommandExecutor for TestDoctorHandler {
        fn execute_doctor(&self, online: bool) -> String {
            if online {
                "doctor online ok".to_string()
            } else {
                "doctor ok".to_string()
            }
        }
    }

    #[derive(Clone, Default)]
    struct TestApprovalsHandler;

    impl MultiChannelApprovalsCommandExecutor for TestApprovalsHandler {
        fn execute_approvals(
            &self,
            state_dir: &Path,
            args: &str,
            decision_actor: Option<&str>,
        ) -> String {
            match execute_test_approvals_command(state_dir, args, decision_actor) {
                Ok(output) => output,
                Err(error) => format!("approvals error: {error}"),
            }
        }
    }

    #[derive(Debug, Clone, Deserialize)]
    struct PairingRecord {
        channel: String,
        actor_id: String,
        #[serde(default)]
        expires_unix_ms: Option<u64>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct PairingRegistryFile {
        schema_version: u32,
        #[serde(default)]
        pairings: Vec<PairingRecord>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct PairingAllowlistFile {
        schema_version: u32,
        #[serde(default)]
        strict: bool,
        #[serde(default)]
        channels: BTreeMap<String, Vec<String>>,
    }

    #[derive(Clone, Default)]
    struct FilePairingEvaluator;

    impl MultiChannelPairingEvaluator for FilePairingEvaluator {
        fn evaluate_pairing(
            &self,
            state_dir: &Path,
            policy_channel: &str,
            actor_id: &str,
            now_unix_ms: u64,
        ) -> Result<MultiChannelPairingDecision> {
            const ALLOW_ALLOWLIST_AND_PAIRING: &str = "allow_allowlist_and_pairing";
            const ALLOW_ALLOWLIST: &str = "allow_allowlist";
            const ALLOW_PAIRING: &str = "allow_pairing";
            const DENY_ACTOR_ID_MISSING: &str = "deny_actor_id_missing";
            const DENY_ACTOR_NOT_PAIRED_OR_ALLOWLISTED: &str =
                "deny_actor_not_paired_or_allowlisted";

            let actor_id = actor_id.trim();
            let (allowlist_path, registry_path, strict_mode) =
                pairing_paths_for_state_dir(state_dir);
            let allowlist = load_pairing_allowlist(&allowlist_path)?;
            let registry = load_pairing_registry(&registry_path)?;
            let candidates = channel_candidates(policy_channel);
            let strict_effective = strict_mode
                || allowlist.strict
                || channel_has_pairing_rules(&allowlist, &registry, &candidates);

            if !strict_effective {
                return Ok(MultiChannelPairingDecision::Allow {
                    reason_code: PAIRING_REASON_ALLOW_PERMISSIVE_MODE.to_string(),
                });
            }
            if actor_id.is_empty() {
                return Ok(MultiChannelPairingDecision::Deny {
                    reason_code: DENY_ACTOR_ID_MISSING.to_string(),
                });
            }

            let allowed_by_allowlist = allowlist_actor_allowed(&allowlist, &candidates, actor_id);
            let allowed_by_pairing =
                pairing_actor_allowed(&registry, &candidates, actor_id, now_unix_ms);

            if allowed_by_allowlist && allowed_by_pairing {
                return Ok(MultiChannelPairingDecision::Allow {
                    reason_code: ALLOW_ALLOWLIST_AND_PAIRING.to_string(),
                });
            }
            if allowed_by_allowlist {
                return Ok(MultiChannelPairingDecision::Allow {
                    reason_code: ALLOW_ALLOWLIST.to_string(),
                });
            }
            if allowed_by_pairing {
                return Ok(MultiChannelPairingDecision::Allow {
                    reason_code: ALLOW_PAIRING.to_string(),
                });
            }
            Ok(MultiChannelPairingDecision::Deny {
                reason_code: DENY_ACTOR_NOT_PAIRED_OR_ALLOWLISTED.to_string(),
            })
        }
    }

    fn pairing_paths_for_state_dir(state_dir: &Path) -> (PathBuf, PathBuf, bool) {
        let state_name = state_dir.file_name().and_then(|value| value.to_str());
        let tau_root = match state_name {
            Some("github")
            | Some("slack")
            | Some("events")
            | Some("channel-store")
            | Some("multi-channel") => state_dir
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(state_dir),
            _ => state_dir,
        };
        let security_dir = tau_root.join("security");
        (
            security_dir.join("allowlist.json"),
            security_dir.join("pairings.json"),
            false,
        )
    }

    fn load_pairing_allowlist(path: &Path) -> Result<PairingAllowlistFile> {
        if !path.exists() {
            return Ok(PairingAllowlistFile {
                schema_version: 1,
                strict: false,
                channels: BTreeMap::new(),
            });
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let parsed = serde_json::from_str::<PairingAllowlistFile>(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        if parsed.schema_version != 1 {
            bail!(
                "unsupported allowlist schema_version {} in {} (expected 1)",
                parsed.schema_version,
                path.display()
            );
        }
        Ok(parsed)
    }

    fn load_pairing_registry(path: &Path) -> Result<PairingRegistryFile> {
        if !path.exists() {
            return Ok(PairingRegistryFile {
                schema_version: 1,
                pairings: Vec::new(),
            });
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let parsed = serde_json::from_str::<PairingRegistryFile>(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        if parsed.schema_version != 1 {
            bail!(
                "unsupported pairing schema_version {} in {} (expected 1)",
                parsed.schema_version,
                path.display()
            );
        }
        Ok(parsed)
    }

    fn channel_candidates(channel: &str) -> Vec<String> {
        let trimmed = channel.trim();
        if trimmed.is_empty() {
            return vec!["*".to_string()];
        }
        let mut candidates = vec![trimmed.to_string()];
        if let Some((prefix, _)) = trimmed.split_once(':') {
            if !prefix.is_empty() {
                candidates.push(prefix.to_string());
            }
        }
        candidates.push("*".to_string());
        candidates
    }

    fn channel_has_pairing_rules(
        allowlist: &PairingAllowlistFile,
        registry: &PairingRegistryFile,
        candidates: &[String],
    ) -> bool {
        let allowlist_has_entries = candidates.iter().any(|candidate| {
            allowlist
                .channels
                .get(candidate)
                .is_some_and(|actors| !actors.is_empty())
        });
        if allowlist_has_entries {
            return true;
        }
        registry
            .pairings
            .iter()
            .any(|entry| candidates.contains(&entry.channel))
    }

    fn allowlist_actor_allowed(
        allowlist: &PairingAllowlistFile,
        candidates: &[String],
        actor_id: &str,
    ) -> bool {
        candidates.iter().any(|candidate| {
            allowlist.channels.get(candidate).is_some_and(|actors| {
                actors
                    .iter()
                    .any(|actor| actor.trim().eq_ignore_ascii_case(actor_id))
            })
        })
    }

    fn pairing_actor_allowed(
        registry: &PairingRegistryFile,
        candidates: &[String],
        actor_id: &str,
        now_unix_ms: u64,
    ) -> bool {
        registry.pairings.iter().any(|entry| {
            candidates.contains(&entry.channel)
                && entry.actor_id.eq_ignore_ascii_case(actor_id)
                && !is_pairing_expired(entry, now_unix_ms)
        })
    }

    fn is_pairing_expired(entry: &PairingRecord, now_unix_ms: u64) -> bool {
        entry
            .expires_unix_ms
            .is_some_and(|expires| expires <= now_unix_ms)
    }

    fn approvals_root_for_state_dir(state_dir: &Path) -> PathBuf {
        let state_name = state_dir.file_name().and_then(|value| value.to_str());
        let tau_root = match state_name {
            Some("github")
            | Some("slack")
            | Some("events")
            | Some("channel-store")
            | Some("multi-channel") => state_dir
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or(state_dir),
            _ => state_dir,
        };
        tau_root.join("approvals")
    }

    fn execute_test_approvals_command(
        state_dir: &Path,
        args: &str,
        decision_actor: Option<&str>,
    ) -> Result<String> {
        let approvals_root = approvals_root_for_state_dir(state_dir);
        std::fs::create_dir_all(&approvals_root)
            .with_context(|| format!("create {}", approvals_root.display()))?;
        let store_path = approvals_root.join("requests.json");
        let mut store = if store_path.exists() {
            let raw = std::fs::read_to_string(&store_path)
                .with_context(|| format!("read {}", store_path.display()))?;
            serde_json::from_str::<Value>(&raw)
                .with_context(|| format!("parse {}", store_path.display()))?
        } else {
            serde_json::json!({
                "schema_version": 1,
                "next_request_id": 1,
                "requests": []
            })
        };

        let tokens = args.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            bail!("missing approvals action");
        }
        let action = tokens[0];
        let requests = store
            .get_mut("requests")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| anyhow::anyhow!("approvals store missing requests array"))?;

        match action {
            "list" => {
                let mut status_filter: Option<&str> = None;
                let mut index = 1;
                while index < tokens.len() {
                    if tokens[index] == "--status" {
                        status_filter = tokens.get(index + 1).copied();
                        index += 2;
                        continue;
                    }
                    index += 1;
                }
                let mut total = 0;
                let mut pending = 0;
                let mut approved = 0;
                let mut rejected = 0;
                let mut expired = 0;
                let mut consumed = 0;
                let mut lines = Vec::new();
                for request in requests.iter() {
                    let status = request
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    total += 1;
                    match status {
                        "pending" => pending += 1,
                        "approved" => approved += 1,
                        "rejected" => rejected += 1,
                        "expired" => expired += 1,
                        "consumed" => consumed += 1,
                        _ => {}
                    }
                    if status_filter.map(|filter| filter == status).unwrap_or(true) {
                        let id = request
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown");
                        lines.push(format!("request id={} status={}", id, status));
                    }
                }
                let mut output = format!(
                    "approvals summary: total={} pending={} approved={} rejected={} expired={} consumed={}",
                    total, pending, approved, rejected, expired, consumed
                );
                if !lines.is_empty() {
                    output.push('\n');
                    output.push_str(&lines.join("\n"));
                }
                Ok(output)
            }
            "approve" | "reject" => {
                let request_id = tokens
                    .get(1)
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("missing approvals request id"))?;
                let reason = if tokens.len() > 2 {
                    Some(tokens[2..].join(" "))
                } else {
                    None
                };
                let mut target = None;
                for entry in requests.iter_mut() {
                    if entry.get("id").and_then(Value::as_str) == Some(request_id) {
                        target = Some(entry);
                        break;
                    }
                }
                let Some(entry) = target else {
                    bail!("request {request_id} not found");
                };
                let status = entry
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if status != "pending" {
                    bail!("request {request_id} is not pending");
                }
                let new_status = if action == "approve" {
                    "approved"
                } else {
                    "rejected"
                };
                entry["status"] = Value::String(new_status.to_string());
                entry["decision_actor"] =
                    Value::String(decision_actor.unwrap_or("local-command").to_string());
                entry["decision_reason"] = reason
                    .as_deref()
                    .map(|value| Value::String(value.to_string()))
                    .unwrap_or(Value::Null);
                entry["decision_at_ms"] = Value::Number(current_unix_timestamp_ms().into());
                let payload =
                    serde_json::to_string_pretty(&store).context("serialize approval store")?;
                std::fs::write(&store_path, payload)
                    .with_context(|| format!("write {}", store_path.display()))?;
                Ok(format!(
                    "approvals {action}: request {request_id} {new_status}"
                ))
            }
            _ => bail!("unknown approvals action '{action}'"),
        }
    }

    fn build_config(root: &Path) -> MultiChannelRuntimeConfig {
        MultiChannelRuntimeConfig {
            fixture_path: fixture_path("baseline-three-channel.json"),
            state_dir: root.join(".tau/multi-channel"),
            orchestrator_route_table_path: None,
            queue_limit: 64,
            processed_event_cap: 10_000,
            retry_max_attempts: 3,
            retry_base_delay_ms: 0,
            retry_jitter_ms: 0,
            outbound: MultiChannelOutboundConfig::default(),
            telemetry: MultiChannelTelemetryConfig::default(),
            media: crate::multi_channel_media::MultiChannelMediaUnderstandingConfig::default(),
            command_handlers: MultiChannelCommandHandlers {
                auth: Some(Arc::new(TestAuthHandler)),
                doctor: Some(Arc::new(TestDoctorHandler)),
                approvals: Some(Arc::new(TestApprovalsHandler)),
            },
            pairing_evaluator: Arc::new(FilePairingEvaluator),
        }
    }

    fn build_live_config(root: &Path) -> MultiChannelLiveRuntimeConfig {
        MultiChannelLiveRuntimeConfig {
            ingress_dir: root.join(".tau/multi-channel/live-ingress"),
            state_dir: root.join(".tau/multi-channel"),
            orchestrator_route_table_path: None,
            queue_limit: 64,
            processed_event_cap: 10_000,
            retry_max_attempts: 3,
            retry_base_delay_ms: 0,
            retry_jitter_ms: 0,
            outbound: MultiChannelOutboundConfig::default(),
            telemetry: MultiChannelTelemetryConfig::default(),
            media: crate::multi_channel_media::MultiChannelMediaUnderstandingConfig::default(),
            command_handlers: MultiChannelCommandHandlers {
                auth: Some(Arc::new(TestAuthHandler)),
                doctor: Some(Arc::new(TestDoctorHandler)),
                approvals: Some(Arc::new(TestApprovalsHandler)),
            },
            pairing_evaluator: Arc::new(FilePairingEvaluator),
        }
    }

    fn write_live_ingress_file(ingress_dir: &Path, transport: &str, fixture_name: &str) {
        std::fs::create_dir_all(ingress_dir).expect("create ingress directory");
        let file_name = format!("{transport}.ndjson");
        let fixture_raw =
            std::fs::read_to_string(live_fixture_path(fixture_name)).expect("read live fixture");
        let fixture_json: Value = serde_json::from_str(&fixture_raw).expect("parse fixture json");
        let fixture_line = serde_json::to_string(&fixture_json).expect("serialize fixture line");
        std::fs::write(ingress_dir.join(file_name), format!("{fixture_line}\n"))
            .expect("write ingress file");
    }

    fn write_pairing_allowlist(root: &Path, payload: &str) {
        let security_dir = root.join(".tau/security");
        std::fs::create_dir_all(&security_dir).expect("create security dir");
        std::fs::write(security_dir.join("allowlist.json"), payload).expect("write allowlist");
    }

    fn write_pairing_registry(root: &Path, payload: &str) {
        let security_dir = root.join(".tau/security");
        std::fs::create_dir_all(&security_dir).expect("create security dir");
        std::fs::write(security_dir.join("pairings.json"), payload).expect("write pairings");
    }

    fn write_channel_policy(root: &Path, payload: &str) {
        let security_dir = root.join(".tau/security");
        std::fs::create_dir_all(&security_dir).expect("create security dir");
        std::fs::write(
            security_dir.join(crate::multi_channel_policy::MULTI_CHANNEL_POLICY_FILE_NAME),
            payload,
        )
        .expect("write channel policy");
    }

    fn write_approval_policy(root: &Path, payload: &str) {
        let approvals_dir = root.join(".tau/approvals");
        std::fs::create_dir_all(&approvals_dir).expect("create approvals dir");
        std::fs::write(approvals_dir.join("policy.json"), payload).expect("write approval policy");
    }

    fn write_approval_store(root: &Path, payload: &str) {
        let approvals_dir = root.join(".tau/approvals");
        std::fs::create_dir_all(&approvals_dir).expect("create approvals dir");
        std::fs::write(approvals_dir.join("requests.json"), payload).expect("write approval store");
    }

    fn write_multi_channel_route_bindings(root: &Path, payload: &str) {
        let security_dir = root.join(".tau/multi-channel/security");
        std::fs::create_dir_all(&security_dir).expect("create multi-channel security dir");
        std::fs::write(
            security_dir.join(crate::multi_channel_routing::MULTI_CHANNEL_ROUTE_BINDINGS_FILE_NAME),
            payload,
        )
        .expect("write multi-channel route bindings");
    }

    fn write_orchestrator_route_table(path: &Path, payload: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create orchestrator route table parent");
        }
        std::fs::write(path, payload).expect("write orchestrator route table");
    }

    fn sample_event(
        transport: MultiChannelTransport,
        event_id: &str,
        conversation_id: &str,
        actor_id: &str,
        text: &str,
    ) -> MultiChannelInboundEvent {
        MultiChannelInboundEvent {
            schema_version: 1,
            transport,
            event_kind: MultiChannelEventKind::Message,
            event_id: event_id.to_string(),
            conversation_id: conversation_id.to_string(),
            thread_id: String::new(),
            actor_id: actor_id.to_string(),
            actor_display: String::new(),
            timestamp_ms: 1_760_200_000_000,
            text: text.to_string(),
            attachments: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn unit_retry_delay_ms_scales_with_attempt_number() {
        assert_eq!(retry_delay_ms(0, 0, 1, "seed"), 0);
        assert_eq!(retry_delay_ms(10, 0, 1, "seed"), 10);
        assert_eq!(retry_delay_ms(10, 0, 2, "seed"), 20);
        assert_eq!(retry_delay_ms(10, 0, 3, "seed"), 40);
    }

    #[test]
    fn unit_retry_delay_ms_jitter_is_deterministic_for_seed() {
        let first = retry_delay_ms(20, 15, 2, "event-1");
        let second = retry_delay_ms(20, 15, 2, "event-1");
        assert_eq!(first, second);
        assert!(first >= 40);
        assert!(first <= 55);
    }

    #[test]
    fn unit_pairing_policy_channel_includes_transport_prefix() {
        let event = sample_event(
            MultiChannelTransport::Discord,
            "evt-1",
            "ops-room",
            "discord-user-1",
            "hello",
        );
        assert_eq!(super::pairing_policy_channel(&event), "discord:ops-room");
    }

    #[test]
    fn unit_should_emit_typing_presence_lifecycle_respects_threshold_and_force_flag() {
        let mut event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-telemetry-1",
            "chat-telemetry",
            "telegram-user-1",
            "hello",
        );
        let config = MultiChannelTelemetryConfig {
            typing_presence_min_response_chars: 200,
            ..MultiChannelTelemetryConfig::default()
        };
        assert!(!super::should_emit_typing_presence_lifecycle(
            &event,
            "short response",
            &config,
        ));

        event.metadata.insert(
            "telemetry_force_typing_presence".to_string(),
            Value::Bool(true),
        );
        assert!(super::should_emit_typing_presence_lifecycle(
            &event,
            "short response",
            &config,
        ));
    }

    #[test]
    fn unit_extract_usage_estimated_cost_micros_parses_supported_metadata_fields() {
        let mut event = sample_event(
            MultiChannelTransport::Discord,
            "dc-cost-1",
            "ops-room",
            "discord-user-1",
            "cost metadata",
        );
        event
            .metadata
            .insert("usage_cost_usd".to_string(), Value::from(0.00125_f64));
        assert_eq!(
            super::extract_usage_estimated_cost_micros(&event),
            Some(1250)
        );

        event
            .metadata
            .insert("usage_cost_micros".to_string(), Value::from(777_u64));
        assert_eq!(
            super::extract_usage_estimated_cost_micros(&event),
            Some(777)
        );
    }

    #[test]
    fn unit_parse_multi_channel_tau_command_supports_initial_command_set() {
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau").expect("parse"),
            Some(super::MultiChannelTauCommand::Help)
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau help").expect("parse"),
            Some(super::MultiChannelTauCommand::Help)
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau status").expect("parse"),
            Some(super::MultiChannelTauCommand::Status)
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau auth status openai").expect("parse"),
            Some(super::MultiChannelTauCommand::AuthStatus {
                provider: Some("openai".to_string())
            })
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau doctor --online").expect("parse"),
            Some(super::MultiChannelTauCommand::Doctor { online: true })
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau approvals list --json --status pending")
                .expect("parse"),
            Some(super::MultiChannelTauCommand::Approvals {
                action: super::MultiChannelTauApprovalsAction::List,
                args: "list --json --status pending".to_string(),
            })
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau approvals approve req-7 looks_safe")
                .expect("parse"),
            Some(super::MultiChannelTauCommand::Approvals {
                action: super::MultiChannelTauApprovalsAction::Approve,
                args: "approve req-7 looks_safe".to_string(),
            })
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau approvals reject req-8 blocked")
                .expect("parse"),
            Some(super::MultiChannelTauCommand::Approvals {
                action: super::MultiChannelTauApprovalsAction::Reject,
                args: "reject req-8 blocked".to_string(),
            })
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("plain text").expect("parse"),
            None
        );
    }

    #[test]
    fn regression_parse_multi_channel_tau_command_rejects_invalid_forms() {
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau auth").expect_err("invalid args"),
            "command_invalid_args"
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau auth login").expect_err("invalid args"),
            "command_invalid_args"
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau auth status mystery")
                .expect_err("invalid provider"),
            "command_invalid_args"
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau unknown").expect_err("unknown command"),
            "command_unknown"
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau approvals").expect_err("missing action"),
            "command_invalid_args"
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau approvals list --status maybe")
                .expect_err("invalid status"),
            "command_invalid_args"
        );
        assert_eq!(
            super::parse_multi_channel_tau_command("/tau approvals approve")
                .expect_err("missing request id"),
            "command_invalid_args"
        );
    }

    #[tokio::test]
    async fn functional_runner_executes_tau_status_command_and_persists_command_metadata() {
        let temp = tempdir().expect("tempdir");
        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-command-status-1",
            "telegram-command-room",
            "telegram-user-1",
            "/tau status",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "telegram-command-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["schema"].as_str(),
            Some("multi_channel_tau_command_v1")
        );
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("reported")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_status_reported")
        );
        let response = command_entry.payload["response"]
            .as_str()
            .expect("response string");
        assert!(response.contains("Tau command `/tau status`"));
        assert!(response.contains("reason_code `command_status_reported`"));
    }

    #[tokio::test]
    async fn integration_runner_tau_doctor_requires_allowlisted_operator_scope() {
        let temp = tempdir().expect("tempdir");
        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Discord,
            "dc-command-doctor-1",
            "discord-command-room",
            "discord-user-1",
            "/tau doctor",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_allowed_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "discord",
            "discord-command-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("failed")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_rbac_denied")
        );
        let response = command_entry.payload["response"]
            .as_str()
            .expect("response");
        assert!(response.contains("command denied"));
    }

    #[tokio::test]
    async fn integration_runner_executes_tau_doctor_for_allowlisted_operator() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "discord:discord-command-room": ["discord-allowed-user"]
  }
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Discord,
            "dc-command-doctor-allow-1",
            "discord-command-room",
            "discord-allowed-user",
            "/tau doctor",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_allowed_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "discord",
            "discord-command-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("reported")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_doctor_reported")
        );
    }

    #[tokio::test]
    async fn functional_runner_executes_tau_approvals_list_for_allowlisted_operator() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
        );
        write_approval_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": []
}
"#,
        );
        write_approval_store(
            temp.path(),
            r#"{
  "schema_version": 1,
  "next_request_id": 2,
  "requests": [
    {
      "id": "req-1",
      "rule_id": "command-review",
      "action_kind": "command",
      "action_summary": "command name=/danger args='now'",
      "fingerprint": "seed",
      "status": "pending",
      "created_at_ms": 1,
      "expires_at_ms": 9999999999999,
      "decision_at_ms": null,
      "decision_reason": null,
      "decision_actor": null,
      "consumed_at_ms": null
    }
  ]
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-command-approvals-list-1",
            "approval-room",
            "telegram-operator",
            "/tau approvals list --status pending",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "approval-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("reported")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_approvals_list_reported")
        );
        let response = command_entry.payload["response"]
            .as_str()
            .expect("response");
        assert!(response.contains("approvals summary:"));
        assert!(response.contains("req-1"));
    }

    #[tokio::test]
    async fn integration_runner_executes_tau_approvals_approve_and_persists_actor_mapping() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
        );
        write_approval_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": [
    {
      "id": "command-review",
      "action": "command",
      "command_names": ["/danger"]
    }
  ]
}
"#,
        );
        write_approval_store(
            temp.path(),
            r#"{
  "schema_version": 1,
  "next_request_id": 2,
  "requests": [
    {
      "id": "req-42",
      "rule_id": "command-review",
      "action_kind": "command",
      "action_summary": "command name=/danger args='now'",
      "fingerprint": "seed",
      "status": "pending",
      "created_at_ms": 1,
      "expires_at_ms": 9999999999999,
      "decision_at_ms": null,
      "decision_reason": null,
      "decision_actor": null,
      "consumed_at_ms": null
    }
  ]
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-command-approvals-approve-1",
            "approval-room",
            "telegram-operator",
            "/tau approvals approve req-42 approved_from_channel",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "approval-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("reported")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_approvals_approved")
        );

        let raw_store = std::fs::read_to_string(temp.path().join(".tau/approvals/requests.json"))
            .expect("read approval store");
        let store_json: Value = serde_json::from_str(&raw_store).expect("parse approval store");
        let request = store_json["requests"]
            .as_array()
            .expect("requests array")
            .iter()
            .find(|entry| entry["id"].as_str() == Some("req-42"))
            .expect("approval request");
        assert_eq!(request["status"].as_str(), Some("approved"));
        assert_eq!(
            request["decision_actor"].as_str(),
            Some("telegram:approval-room:telegram-operator")
        );
    }

    #[tokio::test]
    async fn regression_runner_tau_approvals_approve_unknown_request_fails_closed() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
        );
        write_approval_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": []
}
"#,
        );
        write_approval_store(
            temp.path(),
            r#"{
  "schema_version": 1,
  "next_request_id": 1,
  "requests": []
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-command-approvals-unknown-1",
            "approval-room",
            "telegram-operator",
            "/tau approvals approve req-404 blocked",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "approval-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("failed")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_approvals_unknown_request")
        );
        let response = command_entry.payload["response"]
            .as_str()
            .expect("response");
        assert!(response.contains("approvals error:"));
    }

    #[tokio::test]
    async fn regression_runner_tau_approvals_reject_stale_request_fails_closed() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:approval-room": ["telegram-operator"]
  }
}
"#,
        );
        write_approval_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "enabled": true,
  "strict_mode": true,
  "timeout_seconds": 3600,
  "rules": []
}
"#,
        );
        write_approval_store(
            temp.path(),
            r#"{
  "schema_version": 1,
  "next_request_id": 2,
  "requests": [
    {
      "id": "req-9",
      "rule_id": "command-review",
      "action_kind": "command",
      "action_summary": "command name=/danger args='now'",
      "fingerprint": "seed",
      "status": "approved",
      "created_at_ms": 1,
      "expires_at_ms": 9999999999999,
      "decision_at_ms": 10,
      "decision_reason": "already approved",
      "decision_actor": "local-command",
      "consumed_at_ms": null
    }
  ]
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-command-approvals-stale-1",
            "approval-room",
            "telegram-operator",
            "/tau approvals reject req-9 blocked",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "approval-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("failed")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_approvals_stale_request")
        );
    }

    #[tokio::test]
    async fn regression_runner_reports_unknown_tau_command_with_failure_reason_code() {
        let temp = tempdir().expect("tempdir");
        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Whatsapp,
            "wa-command-unknown-1",
            "whatsapp-command-room",
            "whatsapp-user-1",
            "/tau unknown",
        );

        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "whatsapp",
            "whatsapp-command-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let command_entry = logs
            .iter()
            .find(|entry| {
                entry.direction == "outbound"
                    && entry
                        .payload
                        .get("command")
                        .and_then(Value::as_object)
                        .is_some()
            })
            .expect("command outbound log entry");
        assert_eq!(
            command_entry.payload["command"]["status"].as_str(),
            Some("failed")
        );
        assert_eq!(
            command_entry.payload["command"]["reason_code"].as_str(),
            Some("command_unknown")
        );
        let response = command_entry.payload["response"]
            .as_str()
            .expect("response");
        assert!(response.contains("/tau help"));
    }

    #[tokio::test]
    async fn functional_runner_processes_fixture_and_persists_channel_store_entries() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_events, 3);
        assert_eq!(summary.queued_events, 3);
        assert_eq!(summary.completed_events, 3);
        assert_eq!(summary.duplicate_skips, 0);
        assert_eq!(summary.failed_events, 0);
        assert_eq!(summary.policy_checked_events, 3);
        assert_eq!(summary.policy_enforced_events, 0);
        assert_eq!(summary.policy_allowed_events, 3);
        assert_eq!(summary.policy_denied_events, 0);

        let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_completed, 3);
        assert_eq!(state.health.last_cycle_failed, 0);
        assert_eq!(state.health.failure_streak, 0);
        assert_eq!(state.health.classify().state, TransportHealthState::Healthy);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events log");
        assert!(events_log.contains("healthy_cycle"));
        assert!(events_log.contains("\"health_state\":\"healthy\""));
        assert!(events_log.contains("pairing_policy_permissive"));

        for event in &fixture.events {
            let store = ChannelStore::open(
                &config.state_dir.join("channel-store"),
                event.transport.as_str(),
                &event.conversation_id,
            )
            .expect("open store");
            let logs = store.load_log_entries().expect("load logs");
            let context = store.load_context_entries().expect("load context");
            assert_eq!(logs.len(), 2);
            assert!(context.len() >= 2);
            assert_eq!(
                logs[0].payload["pairing"]["decision"].as_str(),
                Some("allow")
            );
            assert_eq!(
                logs[0].payload["pairing"]["reason_code"].as_str(),
                Some("allow_permissive_mode")
            );
        }
    }

    #[tokio::test]
    async fn integration_runner_media_understanding_enriches_context_and_logs_reason_codes() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.outbound.mode = MultiChannelOutboundMode::DryRun;
        config.media.max_attachments_per_event = 3;
        config.media.max_summary_chars = 96;

        let mut event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-media-1",
            "chat-media",
            "telegram-user-1",
            "please inspect media",
        );
        event.attachments = vec![
            MultiChannelAttachment {
                attachment_id: "img-1".to_string(),
                url: "https://example.com/image.png".to_string(),
                content_type: "image/png".to_string(),
                file_name: "image.png".to_string(),
                size_bytes: 128,
            },
            MultiChannelAttachment {
                attachment_id: "aud-1".to_string(),
                url: "https://example.com/voice.wav".to_string(),
                content_type: "audio/wav".to_string(),
                file_name: "voice.wav".to_string(),
                size_bytes: 256,
            },
            MultiChannelAttachment {
                attachment_id: "doc-1".to_string(),
                url: "https://example.com/readme.txt".to_string(),
                content_type: "text/plain".to_string(),
                file_name: "readme.txt".to_string(),
                size_bytes: 32,
            },
        ];

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "telegram",
            "chat-media",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].payload["media_understanding"]["processed"], 2);
        assert_eq!(logs[0].payload["media_understanding"]["skipped"], 1);
        assert_eq!(
            logs[0].payload["media_understanding"]["reason_code_counts"]
                ["media_unsupported_attachment_type"],
            1
        );

        let context = store.load_context_entries().expect("load context");
        assert_eq!(context[0].role, "user");
        assert!(context[0].text.contains("Media understanding outcomes:"));
        assert!(context[0].text.contains("attachment_id=img-1"));
        assert!(context[0]
            .text
            .contains("reason_code=media_image_described"));
    }

    #[tokio::test]
    async fn regression_runner_media_understanding_is_bounded_and_duplicate_event_idempotent() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.outbound.mode = MultiChannelOutboundMode::DryRun;
        config.media.max_attachments_per_event = 1;

        let mut event = sample_event(
            MultiChannelTransport::Discord,
            "dc-media-bounded-1",
            "discord-media-room",
            "discord-user-1",
            "bounded media processing",
        );
        event.attachments = vec![
            MultiChannelAttachment {
                attachment_id: "img-1".to_string(),
                url: "https://example.com/image.png".to_string(),
                content_type: "image/png".to_string(),
                file_name: "image.png".to_string(),
                size_bytes: 128,
            },
            MultiChannelAttachment {
                attachment_id: "vid-1".to_string(),
                url: "https://example.com/video.mp4".to_string(),
                content_type: "video/mp4".to_string(),
                file_name: "video.mp4".to_string(),
                size_bytes: 2048,
            },
        ];

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let first = runtime
            .run_once_events(std::slice::from_ref(&event))
            .await
            .expect("first run");
        let second = runtime
            .run_once_events(std::slice::from_ref(&event))
            .await
            .expect("second run");

        assert_eq!(first.completed_events, 1);
        assert_eq!(second.duplicate_skips, 1);

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "discord",
            "discord-media-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].payload["media_understanding"]["processed"], 1);
        assert_eq!(logs[0].payload["media_understanding"]["skipped"], 1);
        assert_eq!(
            logs[0].payload["media_understanding"]["reason_code_counts"]
                ["media_attachment_limit_exceeded"],
            1
        );
    }

    #[tokio::test]
    async fn functional_runner_allows_allowlisted_actor_in_strict_mode() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "telegram:chat-allow": ["telegram-allowed-user"]
  }
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let events = vec![sample_event(
            MultiChannelTransport::Telegram,
            "tg-allow-1",
            "chat-allow",
            "telegram-allowed-user",
            "allowlist actor",
        )];
        let summary = runtime.run_once_events(&events).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);
        assert_eq!(summary.policy_checked_events, 1);
        assert_eq!(summary.policy_enforced_events, 1);
        assert_eq!(summary.policy_allowed_events, 1);
        assert_eq!(summary.policy_denied_events, 0);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "chat-allow",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let context = store.load_context_entries().expect("load context");
        assert_eq!(logs.len(), 2);
        assert_eq!(context.len(), 2);
        assert_eq!(
            logs[0].payload["pairing"]["reason_code"].as_str(),
            Some("allow_allowlist")
        );

        let events_log = std::fs::read_to_string(
            temp.path()
                .join(".tau/multi-channel")
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read events log");
        assert!(events_log.contains("pairing_policy_enforced"));
    }

    #[tokio::test]
    async fn integration_runner_denies_unpaired_actor_in_strict_mode() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {}
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let events = vec![sample_event(
            MultiChannelTransport::Discord,
            "dc-deny-1",
            "discord-room-deny",
            "discord-unknown-user",
            "restricted actor",
        )];
        let summary = runtime.run_once_events(&events).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);
        assert_eq!(summary.policy_checked_events, 1);
        assert_eq!(summary.policy_enforced_events, 1);
        assert_eq!(summary.policy_allowed_events, 0);
        assert_eq!(summary.policy_denied_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "discord",
            "discord-room-deny",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let context = store.load_context_entries().expect("load context");
        assert_eq!(logs.len(), 2);
        assert_eq!(context.len(), 0);
        assert_eq!(logs[1].payload["status"].as_str(), Some("denied"));
        assert_eq!(
            logs[1].payload["reason_code"].as_str(),
            Some("deny_actor_not_paired_or_allowlisted")
        );

        let events_log = std::fs::read_to_string(
            temp.path()
                .join(".tau/multi-channel")
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read events log");
        assert!(events_log.contains("pairing_policy_denied_events"));
    }

    #[tokio::test]
    async fn integration_runner_denies_group_message_when_mention_required() {
        let temp = tempdir().expect("tempdir");
        write_channel_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "discord:ops-room": {
      "dmPolicy": "allow",
      "allowFrom": "any",
      "groupPolicy": "allow",
      "requireMention": true
    }
  }
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let mut event = sample_event(
            MultiChannelTransport::Discord,
            "dc-mention-1",
            "ops-room",
            "discord-user-1",
            "hello team",
        );
        event
            .metadata
            .insert("guild_id".to_string(), Value::String("guild-1".to_string()));
        let summary = runtime.run_once_events(&[event]).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_denied_events, 1);
        assert_eq!(summary.policy_enforced_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "discord",
            "ops-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(
            logs[1].payload["reason_code"].as_str(),
            Some("deny_channel_policy_mention_required")
        );
    }

    #[tokio::test]
    async fn integration_runner_allows_group_message_when_mention_present_and_allow_from_any() {
        let temp = tempdir().expect("tempdir");
        write_channel_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "discord:ops-room": {
      "dmPolicy": "allow",
      "allowFrom": "any",
      "groupPolicy": "allow",
      "requireMention": true
    }
  }
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let mut event = sample_event(
            MultiChannelTransport::Discord,
            "dc-mention-2",
            "ops-room",
            "discord-user-1",
            "@tau deploy status",
        );
        event
            .metadata
            .insert("guild_id".to_string(), Value::String("guild-1".to_string()));
        let summary = runtime.run_once_events(&[event]).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_allowed_events, 1);
        assert_eq!(summary.policy_denied_events, 0);
        assert_eq!(summary.policy_enforced_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "discord",
            "ops-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(logs[0].payload["pairing"]["checked"].as_bool(), Some(false));
        assert_eq!(
            logs[0].payload["channel_policy"]["reason_code"].as_str(),
            Some("allow_channel_policy_allow_from_any")
        );
    }

    #[tokio::test]
    async fn integration_runner_retries_transient_failure_then_recovers() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 4;
        let fixture_raw = r#"{
  "schema_version": 1,
  "name": "transient-retry",
  "events": [
    {
      "schema_version": 1,
      "transport": "telegram",
      "event_kind": "message",
      "event_id": "tg-transient-1",
      "conversation_id": "telegram-chat-transient",
      "actor_id": "telegram-user-1",
      "timestamp_ms": 1760100000000,
      "text": "hello",
      "metadata": { "simulate_transient_failures": 1 }
    }
  ]
}"#;
        let fixture = parse_multi_channel_contract_fixture(fixture_raw).expect("parse fixture");
        let mut runtime = MultiChannelRuntime::new(config).expect("runtime");
        let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.transient_failures, 1);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_events, 0);
        assert_eq!(summary.policy_checked_events, 1);
        assert_eq!(summary.policy_allowed_events, 1);
    }

    #[tokio::test]
    async fn functional_runner_dry_run_outbound_records_delivery_receipts() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.outbound.mode = MultiChannelOutboundMode::DryRun;
        config.outbound.max_chars = 12;

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-dry-run-1",
            "chat-dry-run",
            "telegram-user-1",
            "hello with dry run",
        );
        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "telegram",
            "chat-dry-run",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[1].payload["delivery"]["mode"], "dry_run");
        let receipts = logs[1].payload["delivery"]["receipts"]
            .as_array()
            .expect("delivery receipts");
        assert!(!receipts.is_empty());
        assert_eq!(receipts[0]["status"], "dry_run");
    }

    #[tokio::test]
    async fn functional_runner_emits_typing_presence_telemetry_for_long_replies_in_dry_run_mode() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.outbound.mode = MultiChannelOutboundMode::DryRun;
        config.telemetry.typing_presence_min_response_chars = 1;

        let events = vec![
            sample_event(
                MultiChannelTransport::Telegram,
                "tg-typing-1",
                "chat-typing-telegram",
                "telegram-user-1",
                "hello",
            ),
            sample_event(
                MultiChannelTransport::Discord,
                "dc-typing-1",
                "chat-typing-discord",
                "discord-user-1",
                "hello",
            ),
            sample_event(
                MultiChannelTransport::Whatsapp,
                "wa-typing-1",
                "chat-typing-whatsapp",
                "15550001111",
                "hello",
            ),
        ];

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_events(&events).await.expect("run once");
        assert_eq!(summary.completed_events, 3);
        assert_eq!(summary.typing_events_emitted, 6);
        assert_eq!(summary.presence_events_emitted, 6);
        assert_eq!(summary.usage_summary_records, 3);

        let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.telemetry.typing_events_emitted, 6);
        assert_eq!(state.telemetry.presence_events_emitted, 6);
        assert_eq!(state.telemetry.usage_summary_records, 3);
        assert_eq!(
            state.telemetry.typing_events_by_transport.get("telegram"),
            Some(&2)
        );
        assert_eq!(
            state.telemetry.typing_events_by_transport.get("discord"),
            Some(&2)
        );
        assert_eq!(
            state.telemetry.typing_events_by_transport.get("whatsapp"),
            Some(&2)
        );

        for event in &events {
            let store = ChannelStore::open(
                &config.state_dir.join("channel-store"),
                event.transport.as_str(),
                event.conversation_id.as_str(),
            )
            .expect("open channel store");
            let logs = store.load_log_entries().expect("load logs");
            assert!(logs.iter().any(|entry| {
                entry.payload.get("status").and_then(Value::as_str) == Some("typing_started")
            }));
            assert!(logs.iter().any(|entry| {
                entry.payload.get("status").and_then(Value::as_str) == Some("typing_stopped")
            }));
            assert!(logs.iter().any(|entry| {
                entry.payload.get("status").and_then(Value::as_str) == Some("presence_active")
            }));
            assert!(logs.iter().any(|entry| {
                entry.payload.get("status").and_then(Value::as_str) == Some("presence_idle")
            }));
        }
    }

    #[tokio::test]
    async fn integration_runner_provider_outbound_posts_per_transport_adapter() {
        struct Scenario<'a> {
            transport: MultiChannelTransport,
            event_id: &'a str,
            conversation_id: &'a str,
            actor_id: &'a str,
            expected_path: &'a str,
            response_body: &'a str,
        }

        let scenarios = vec![
            Scenario {
                transport: MultiChannelTransport::Telegram,
                event_id: "tg-provider-1",
                conversation_id: "chat-200",
                actor_id: "telegram-user-1",
                expected_path: "/bottelegram-token/sendMessage",
                response_body: r#"{"ok":true,"result":{"message_id":42}}"#,
            },
            Scenario {
                transport: MultiChannelTransport::Discord,
                event_id: "dc-provider-1",
                conversation_id: "discord-room-1",
                actor_id: "discord-user-1",
                expected_path: "/channels/discord-room-1/messages",
                response_body: r#"{"id":"msg-22"}"#,
            },
            Scenario {
                transport: MultiChannelTransport::Whatsapp,
                event_id: "wa-provider-1",
                conversation_id: "whatsapp-room-1",
                actor_id: "15550001111",
                expected_path: "/15551234567/messages",
                response_body: r#"{"messages":[{"id":"wamid.1"}]}"#,
            },
        ];

        for scenario in scenarios {
            let server = MockServer::start();
            let sent = server.mock(|when, then| {
                when.method(POST).path(scenario.expected_path);
                then.status(200)
                    .header("content-type", "application/json")
                    .body(scenario.response_body);
            });

            let temp = tempdir().expect("tempdir");
            let mut config = build_config(temp.path());
            config.outbound.mode = MultiChannelOutboundMode::Provider;
            config.outbound.http_timeout_ms = 3_000;
            match scenario.transport {
                MultiChannelTransport::Telegram => {
                    config.outbound.telegram_api_base = server.base_url();
                    config.outbound.telegram_bot_token = Some("telegram-token".to_string());
                }
                MultiChannelTransport::Discord => {
                    config.outbound.discord_api_base = server.base_url();
                    config.outbound.discord_bot_token = Some("discord-token".to_string());
                }
                MultiChannelTransport::Whatsapp => {
                    config.outbound.whatsapp_api_base = server.base_url();
                    config.outbound.whatsapp_access_token = Some("whatsapp-token".to_string());
                    config.outbound.whatsapp_phone_number_id = Some("15551234567".to_string());
                }
            }
            let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
            let event = sample_event(
                scenario.transport,
                scenario.event_id,
                scenario.conversation_id,
                scenario.actor_id,
                "provider integration event",
            );
            let summary = runtime.run_once_events(&[event]).await.expect("run once");
            assert_eq!(summary.completed_events, 1);
            assert_eq!(summary.failed_events, 0);
            assert_eq!(summary.usage_summary_records, 1);
            sent.assert_calls(1);

            let store = ChannelStore::open(
                &config.state_dir.join("channel-store"),
                scenario.transport.as_str(),
                scenario.conversation_id,
            )
            .expect("open store");
            let logs = store.load_log_entries().expect("load logs");
            assert_eq!(logs[1].payload["delivery"]["mode"], "provider");
            assert_eq!(logs[1].payload["delivery"]["receipts"][0]["status"], "sent");

            let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("load state");
            assert_eq!(state.telemetry.usage_summary_records, 1);
            assert_eq!(
                state
                    .telemetry
                    .usage_summary_records_by_transport
                    .get(scenario.transport.as_str()),
                Some(&1)
            );
        }
    }

    #[tokio::test]
    async fn regression_runner_provider_outbound_duplicate_event_suppresses_second_send() {
        let server = MockServer::start();
        let sent = server.mock(|when, then| {
            when.method(POST).path("/bottelegram-token/sendMessage");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"ok":true,"result":{"message_id":42}}"#);
        });

        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.outbound.mode = MultiChannelOutboundMode::Provider;
        config.outbound.telegram_api_base = server.base_url();
        config.outbound.telegram_bot_token = Some("telegram-token".to_string());
        config.telemetry.typing_presence_min_response_chars = 1;
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-dup-provider-1",
            "chat-dup-provider",
            "telegram-user-1",
            "duplicate suppression",
        );

        let mut runtime = MultiChannelRuntime::new(config).expect("runtime");
        let first = runtime
            .run_once_events(std::slice::from_ref(&event))
            .await
            .expect("first run");
        let second = runtime
            .run_once_events(std::slice::from_ref(&event))
            .await
            .expect("second run");

        assert_eq!(first.completed_events, 1);
        assert_eq!(first.typing_events_emitted, 2);
        assert_eq!(first.presence_events_emitted, 2);
        assert_eq!(first.usage_summary_records, 1);
        assert_eq!(second.duplicate_skips, 1);
        assert_eq!(second.typing_events_emitted, 0);
        assert_eq!(second.presence_events_emitted, 0);
        assert_eq!(second.usage_summary_records, 0);
        sent.assert_calls(1);

        let state =
            load_multi_channel_runtime_state(&temp.path().join(".tau/multi-channel/state.json"))
                .expect("load state");
        assert_eq!(state.telemetry.typing_events_emitted, 2);
        assert_eq!(state.telemetry.presence_events_emitted, 2);
        assert_eq!(state.telemetry.usage_summary_records, 1);
    }

    #[tokio::test]
    async fn regression_runner_provider_outbound_retry_exhaustion_surfaces_reason_code() {
        let server = MockServer::start();
        let failed = server.mock(|when, then| {
            when.method(POST).path("/bottelegram-token/sendMessage");
            then.status(503)
                .header("content-type", "application/json")
                .body(r#"{"error":"unavailable"}"#);
        });

        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 2;
        config.outbound.mode = MultiChannelOutboundMode::Provider;
        config.outbound.telegram_api_base = server.base_url();
        config.outbound.telegram_bot_token = Some("telegram-token".to_string());
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-retry-exhaust-1",
            "chat-retry-exhaust",
            "telegram-user-1",
            "should fail delivery",
        );

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.failed_events, 1);
        assert_eq!(summary.retry_attempts, 1);
        failed.assert_calls(2);

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "telegram",
            "chat-retry-exhaust",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert!(logs.iter().any(|entry| {
            entry.payload.get("status").and_then(Value::as_str) == Some("delivery_failed")
                && entry.payload.get("reason_code").and_then(Value::as_str)
                    == Some("delivery_provider_unavailable")
        }));
    }

    #[tokio::test]
    async fn integration_runner_routes_event_to_bound_session_and_emits_route_trace() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        let route_table_path = temp.path().join("route-table.json");
        write_orchestrator_route_table(
            &route_table_path,
            r#"{
  "schema_version": 1,
  "roles": {
    "triage": {},
    "default": {}
  },
  "planner": { "role": "default" },
  "delegated": { "role": "default" },
  "delegated_categories": {
    "incident": { "role": "triage" }
  },
  "review": { "role": "default" }
}"#,
        );
        config.orchestrator_route_table_path = Some(route_table_path);

        write_multi_channel_route_bindings(
            temp.path(),
            r#"{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "discord-ops",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "*",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "session-{role}"
    }
  ]
}"#,
        );

        let mut event = sample_event(
            MultiChannelTransport::Discord,
            "dc-route-1",
            "ops-room",
            "discord-user-1",
            "please check latest incident",
        );
        event.metadata.insert(
            "account_id".to_string(),
            Value::String("discord-main".to_string()),
        );

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_events(&[event]).await.expect("run once");
        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.failed_events, 0);

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "discord",
            "session-triage",
        )
        .expect("open routed store");
        let logs = store.load_log_entries().expect("load routed logs");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].payload["route"]["binding_id"], "discord-ops");
        assert_eq!(logs[0].payload["route"]["selected_role"], "triage");
        assert_eq!(logs[0].payload["route_session_key"], "session-triage");

        let route_traces = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_ROUTE_TRACES_LOG_FILE),
        )
        .expect("read route traces");
        assert!(route_traces.contains("\"record_type\":\"multi_channel_route_trace_v1\""));
        assert!(route_traces.contains("\"binding_id\":\"discord-ops\""));
        assert!(route_traces.contains("\"selected_role\":\"triage\""));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once_fixture(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_events, 3);
        assert_eq!(summary.queued_events, 2);
        assert_eq!(summary.completed_events, 2);
        assert_eq!(summary.policy_checked_events, 2);
        assert_eq!(summary.policy_allowed_events, 2);
        let state = load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.processed_event_keys.len(), 2);
    }

    #[tokio::test]
    async fn regression_runner_skips_duplicate_events_from_persisted_state() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_channel_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = MultiChannelRuntime::new(config.clone()).expect("first runtime");
        let first_summary = first_runtime
            .run_once_fixture(&fixture)
            .await
            .expect("first run");
        assert_eq!(first_summary.completed_events, 3);

        let mut second_runtime = MultiChannelRuntime::new(config).expect("second runtime");
        let second_summary = second_runtime
            .run_once_fixture(&fixture)
            .await
            .expect("second run");
        assert_eq!(second_summary.completed_events, 0);
        assert_eq!(second_summary.duplicate_skips, 3);
        assert_eq!(second_summary.policy_checked_events, 0);
    }

    #[tokio::test]
    async fn regression_runner_prefers_specific_route_binding_over_wildcard() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        let route_table_path = temp.path().join("route-table.json");
        write_orchestrator_route_table(
            &route_table_path,
            r#"{
  "schema_version": 1,
  "roles": {
    "specific": {},
    "fallback": {},
    "default": {}
  },
  "planner": { "role": "default" },
  "delegated": { "role": "fallback" },
  "delegated_categories": {
    "incident": { "role": "specific" }
  },
  "review": { "role": "default" }
}"#,
        );
        config.orchestrator_route_table_path = Some(route_table_path);

        write_multi_channel_route_bindings(
            temp.path(),
            r#"{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "wildcard",
      "transport": "discord",
      "account_id": "*",
      "conversation_id": "*",
      "actor_id": "*",
      "phase": "delegated_step",
      "session_key_template": "wildcard"
    },
    {
      "binding_id": "specific",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "discord-user-1",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "specific-{role}"
    }
  ]
}"#,
        );

        let mut event = sample_event(
            MultiChannelTransport::Discord,
            "dc-specific-1",
            "ops-room",
            "discord-user-1",
            "incident triage please",
        );
        event.metadata.insert(
            "account_id".to_string(),
            Value::String("discord-main".to_string()),
        );

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        runtime.run_once_events(&[event]).await.expect("run once");

        let specific_store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "discord",
            "specific-specific",
        )
        .expect("open specific store");
        let specific_logs = specific_store.load_log_entries().expect("specific logs");
        assert_eq!(specific_logs.len(), 2);
        assert_eq!(specific_logs[0].payload["route"]["binding_id"], "specific");
        assert_eq!(
            specific_logs[0].payload["route"]["selected_role"],
            "specific"
        );
    }

    #[tokio::test]
    async fn regression_runner_denies_expired_pairing_in_strict_evaluation() {
        let temp = tempdir().expect("tempdir");
        write_pairing_registry(
            temp.path(),
            r#"{
  "schema_version": 1,
  "pairings": [
    {
      "channel": "whatsapp:incident-room",
      "actor_id": "15551234567",
      "paired_by": "ops",
      "issued_unix_ms": 1,
      "expires_unix_ms": 2
    }
  ]
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let events = vec![sample_event(
            MultiChannelTransport::Whatsapp,
            "wa-expired-1",
            "incident-room",
            "15551234567",
            "expired pairing should fail",
        )];
        let summary = runtime.run_once_events(&events).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_enforced_events, 1);
        assert_eq!(summary.policy_denied_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "whatsapp",
            "incident-room",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(
            logs[1].payload["reason_code"].as_str(),
            Some("deny_actor_not_paired_or_allowlisted")
        );
    }

    #[tokio::test]
    async fn regression_runner_allowlist_only_denies_pairing_only_actor() {
        let temp = tempdir().expect("tempdir");
        write_pairing_registry(
            temp.path(),
            r#"{
  "schema_version": 1,
  "pairings": [
    {
      "channel": "telegram:chat-allowlist-only",
      "actor_id": "telegram-paired-user",
      "paired_by": "ops",
      "issued_unix_ms": 1000,
      "expires_unix_ms": null
    }
  ]
}
"#,
        );
        write_channel_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "allow",
    "allowFrom": "allowlist_or_pairing",
    "groupPolicy": "allow",
    "requireMention": false
  },
  "channels": {
    "telegram:chat-allowlist-only": {
      "dmPolicy": "allow",
      "allowFrom": "allowlist_only",
      "groupPolicy": "allow",
      "requireMention": false
    }
  }
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let event = sample_event(
            MultiChannelTransport::Telegram,
            "tg-allowlist-only-1",
            "chat-allowlist-only",
            "telegram-paired-user",
            "paired actor should be denied by allowlist_only",
        );
        let summary = runtime.run_once_events(&[event]).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_denied_events, 1);
        assert_eq!(summary.policy_enforced_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "chat-allowlist-only",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(
            logs[1].payload["reason_code"].as_str(),
            Some("deny_channel_policy_allow_from_allowlist_only")
        );
    }

    #[tokio::test]
    async fn regression_runner_explicit_dm_deny_blocks_allow_from_any() {
        let temp = tempdir().expect("tempdir");
        write_channel_policy(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strictMode": false,
  "defaultPolicy": {
    "dmPolicy": "deny",
    "allowFrom": "any",
    "groupPolicy": "allow",
    "requireMention": false
  }
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let mut event = sample_event(
            MultiChannelTransport::Whatsapp,
            "wa-dm-deny-1",
            "15551234567:15550001111",
            "15550001111",
            "dm should be denied",
        );
        event.metadata.insert(
            "conversation_mode".to_string(),
            Value::String("dm".to_string()),
        );
        let summary = runtime.run_once_events(&[event]).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_denied_events, 1);
        assert_eq!(summary.policy_allowed_events, 0);
        assert_eq!(summary.policy_enforced_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "whatsapp",
            "15551234567:15550001111",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(
            logs[1].payload["reason_code"].as_str(),
            Some("deny_channel_policy_dm")
        );
    }

    #[tokio::test]
    async fn regression_runner_denies_empty_actor_id_fail_closed_in_strict_mode() {
        let temp = tempdir().expect("tempdir");
        write_pairing_allowlist(
            temp.path(),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {}
}
"#,
        );

        let mut runtime = MultiChannelRuntime::new(build_config(temp.path())).expect("runtime");
        let events = vec![sample_event(
            MultiChannelTransport::Telegram,
            "tg-empty-actor-1",
            "chat-empty-actor",
            "   ",
            "actor missing test",
        )];
        let summary = runtime.run_once_events(&events).await.expect("run once");

        assert_eq!(summary.completed_events, 1);
        assert_eq!(summary.policy_enforced_events, 1);
        assert_eq!(summary.policy_denied_events, 1);

        let store = ChannelStore::open(
            &temp.path().join(".tau/multi-channel/channel-store"),
            "telegram",
            "chat-empty-actor",
        )
        .expect("open store");
        let logs = store.load_log_entries().expect("load logs");
        let context = store.load_context_entries().expect("load context");
        assert_eq!(context.len(), 0);
        assert_eq!(
            logs[1].payload["reason_code"].as_str(),
            Some("deny_actor_id_missing")
        );
    }

    #[tokio::test]
    async fn integration_runner_failure_streak_increments_and_resets_on_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 2;

        let failing_fixture_raw = r#"{
  "schema_version": 1,
  "name": "persistent-failure",
  "events": [
    {
      "schema_version": 1,
      "transport": "discord",
      "event_kind": "message",
      "event_id": "discord-failing-1",
      "conversation_id": "discord-channel-failing",
      "actor_id": "discord-user-1",
      "timestamp_ms": 1760200000000,
      "text": "retry me",
      "metadata": { "simulate_transient_failures": 5 }
    }
  ]
}"#;
        let failing_fixture = parse_multi_channel_contract_fixture(failing_fixture_raw)
            .expect("parse failing fixture");
        let success_fixture = load_multi_channel_contract_fixture(&config.fixture_path)
            .expect("load success fixture");

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let first_failed = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("first failed cycle");
        assert_eq!(first_failed.failed_events, 1);
        let state_after_first =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state first");
        assert_eq!(state_after_first.health.failure_streak, 1);
        assert_eq!(
            state_after_first.health.classify().state,
            TransportHealthState::Degraded
        );

        let second_failed = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("second failed cycle");
        assert_eq!(second_failed.failed_events, 1);
        let state_after_second =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state second");
        assert_eq!(state_after_second.health.failure_streak, 2);

        let third_failed = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("third failed cycle");
        assert_eq!(third_failed.failed_events, 1);
        let state_after_third =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state third");
        assert_eq!(state_after_third.health.failure_streak, 3);
        assert_eq!(
            state_after_third.health.classify().state,
            TransportHealthState::Failing
        );

        let success = runtime
            .run_once_fixture(&success_fixture)
            .await
            .expect("successful cycle");
        assert_eq!(success.failed_events, 0);
        assert_eq!(success.completed_events, 3);
        let state_after_success =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json"))
                .expect("state success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }

    #[tokio::test]
    async fn regression_runner_emits_reason_codes_for_failed_cycles() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 2;
        let failing_fixture_raw = r#"{
  "schema_version": 1,
  "name": "failed-cycle-reasons",
  "events": [
    {
      "schema_version": 1,
      "transport": "whatsapp",
      "event_kind": "message",
      "event_id": "whatsapp-failing-1",
      "conversation_id": "whatsapp-chat-failing",
      "actor_id": "whatsapp-user-1",
      "timestamp_ms": 1760300000000,
      "text": "retries",
      "metadata": { "simulate_transient_failures": 5 }
    }
  ]
}"#;
        let failing_fixture =
            parse_multi_channel_contract_fixture(failing_fixture_raw).expect("parse fixture");

        let mut runtime = MultiChannelRuntime::new(config.clone()).expect("runtime");
        let summary = runtime
            .run_once_fixture(&failing_fixture)
            .await
            .expect("run once");
        assert_eq!(summary.failed_events, 1);
        assert_eq!(summary.retry_attempts, 1);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events log");
        let first_line = events_log.lines().next().expect("at least one report line");
        let report: Value = serde_json::from_str(first_line).expect("parse report");
        assert_eq!(report["health_state"], "degraded");
        let reason_codes = report["reason_codes"]
            .as_array()
            .expect("reason code array");
        let reason_codes_set = reason_codes
            .iter()
            .filter_map(|value| value.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(reason_codes_set.contains("retry_attempted"));
        assert!(reason_codes_set.contains("transient_failures_observed"));
        assert!(reason_codes_set.contains("event_processing_failed"));
        assert!(reason_codes_set.contains("pairing_policy_permissive"));
    }

    #[test]
    fn unit_live_ingress_loader_skips_invalid_rows_without_failing() {
        let temp = tempdir().expect("tempdir");
        let ingress_dir = temp.path().join("live");
        std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
        let telegram_raw =
            std::fs::read_to_string(live_fixture_path("telegram-valid.json")).expect("fixture");
        let telegram_json: Value = serde_json::from_str(&telegram_raw).expect("parse fixture");
        let telegram_line = serde_json::to_string(&telegram_json).expect("serialize fixture");
        std::fs::write(
            ingress_dir.join("telegram.ndjson"),
            format!("{telegram_line}\n{{\"transport\":\"slack\"}}\n"),
        )
        .expect("write telegram ingress");

        let events = load_multi_channel_live_events(&ingress_dir).expect("load live events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].transport.as_str(), "telegram");
    }

    #[tokio::test]
    async fn functional_live_runner_processes_ingress_files_and_persists_state() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path());
        write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
        write_live_ingress_file(&config.ingress_dir, "discord", "discord-valid.json");
        write_live_ingress_file(&config.ingress_dir, "whatsapp", "whatsapp-valid.json");

        run_multi_channel_live_runner(config.clone())
            .await
            .expect("live runner should succeed");

        let state =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_completed, 3);
        assert_eq!(state.health.last_cycle_failed, 0);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(super::MULTI_CHANNEL_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events log");
        assert!(events_log.contains("\"health_state\":\"healthy\""));
    }

    #[tokio::test]
    async fn integration_live_runner_is_idempotent_across_repeated_cycles() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path());
        write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
        write_live_ingress_file(&config.ingress_dir, "discord", "discord-valid.json");

        run_multi_channel_live_runner(config.clone())
            .await
            .expect("first live run should succeed");
        run_multi_channel_live_runner(config.clone())
            .await
            .expect("second live run should succeed");

        let state =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.processed_event_keys.len(), 2);

        let channel_store_root = config.state_dir.join("channel-store");
        let telegram_store = ChannelStore::open(&channel_store_root, "telegram", "chat-100")
            .expect("open telegram store");
        let telegram_logs = telegram_store.load_log_entries().expect("telegram logs");
        assert_eq!(telegram_logs.len(), 2);
    }

    #[tokio::test]
    async fn regression_live_runner_handles_invalid_transport_file_contents() {
        let temp = tempdir().expect("tempdir");
        let config = build_live_config(temp.path());
        write_live_ingress_file(&config.ingress_dir, "telegram", "telegram-valid.json");
        std::fs::write(
            config.ingress_dir.join("discord.ndjson"),
            "{\"schema_version\":1,\"transport\":\"telegram\",\"provider\":\"telegram-bot-api\",\"payload\":{}}\n",
        )
        .expect("write mismatched ingress");

        run_multi_channel_live_runner(config.clone())
            .await
            .expect("live runner should continue despite mismatch");

        let state =
            load_multi_channel_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.health.last_cycle_discovered, 1);
        assert_eq!(state.health.last_cycle_completed, 1);
    }
}
