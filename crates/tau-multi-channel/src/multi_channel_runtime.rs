//! Core multi-channel runtime orchestration loop.
//!
//! The runtime composes ingress, policy, routing, dedupe, and outbound delivery
//! into a single execution pipeline. It coordinates retry cadence and structured
//! telemetry so failed deliveries can be diagnosed and replayed safely.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod ingress;
mod outbound;
mod routing;

use crate::multi_channel_contract::{
    event_contract_key, load_multi_channel_contract_fixture, MultiChannelContractFixture,
    MultiChannelEventKind, MultiChannelInboundEvent, MultiChannelTransport,
};
use crate::multi_channel_media::{
    process_media_attachments, render_media_prompt_context, MultiChannelMediaUnderstandingConfig,
};
use crate::multi_channel_outbound::{
    MultiChannelOutboundConfig, MultiChannelOutboundDeliveryError, MultiChannelOutboundDispatcher,
};
use crate::multi_channel_policy::{
    evaluate_multi_channel_channel_policy, load_multi_channel_policy_for_state_dir,
    MultiChannelAllowFrom, MultiChannelPolicyDecision, MultiChannelPolicyEvaluation,
    MultiChannelPolicyFile, MultiChannelSecureMessagingMode,
};
use crate::multi_channel_routing::{
    load_multi_channel_route_bindings_for_state_dir, resolve_multi_channel_event_route,
    route_decision_trace_payload, MultiChannelRouteBindingFile, MultiChannelRouteDecision,
};
use tau_access::{
    evaluate_signed_envelope_access, signed_envelope_policy_for_state_dir, SignedEnvelopeContext,
    SignedEnvelopeDecision,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_orchestrator::multi_agent_router::{load_multi_agent_route_table, MultiAgentRouteTable};
use tau_runtime::{ChannelContextEntry, ChannelLogEntry, ChannelStore, TransportHealthSnapshot};

use ingress::{build_user_context_text, load_multi_channel_live_events, normalize_processed_keys};
#[cfg(test)]
use outbound::retry_delay_ms;
use outbound::{
    append_delivery_failure_log, apply_retry_delay, context_contains_entry,
    log_contains_event_direction, log_contains_outbound_response, log_contains_outbound_status,
    render_response, simulated_transient_failures, DeliveryFailureLogContext,
};
use routing::{
    append_multi_channel_cycle_report, append_multi_channel_route_trace,
    build_transport_health_snapshot, cycle_reason_codes,
};

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
const SECURE_REASON_DISABLED: &str = "secure_messaging_disabled";
const SECURE_REASON_SKIPPED_POLICY_DENY: &str = "secure_messaging_skipped_channel_policy_deny";
const SECURE_REASON_DENY_MISSING_REQUIRED: &str = "deny_signed_envelope_missing";
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `MultiChannelPairingDecision` values.
pub enum MultiChannelPairingDecision {
    Allow { reason_code: String },
    Deny { reason_code: String },
}

impl MultiChannelPairingDecision {
    /// Public `fn` `reason_code` in `tau-multi-channel`.
    ///
    /// This item is part of the Wave 2 API surface for M23 documentation uplift.
    /// Callers rely on its contract and failure semantics remaining stable.
    /// Update this comment if behavior or integration expectations change.
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
    secure_messaging: MultiChannelSecureMessagingDecision,
    pairing_decision: MultiChannelPairingDecision,
    final_decision: MultiChannelPairingDecision,
    pairing_checked: bool,
    policy_enforced: bool,
}

#[derive(Debug, Clone)]
struct MultiChannelSecureMessagingDecision {
    mode: MultiChannelSecureMessagingMode,
    status: &'static str,
    reason_code: String,
    key_id: Option<String>,
    nonce: Option<String>,
    checked: bool,
    required: bool,
    legacy_fallback: bool,
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

/// Public `fn` `run_multi_channel_contract_runner` in `tau-multi-channel`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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

/// Public `fn` `run_multi_channel_live_runner` in `tau-multi-channel`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
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

pub use crate::multi_channel_incident::{
    build_multi_channel_incident_timeline_report, render_multi_channel_incident_timeline_report,
    MultiChannelIncidentOutcomeCounts, MultiChannelIncidentReplayExportSummary,
    MultiChannelIncidentTimelineEntry, MultiChannelIncidentTimelineQuery,
    MultiChannelIncidentTimelineReport,
};
pub use crate::multi_channel_route_inspect::{
    build_multi_channel_route_inspect_report, render_multi_channel_route_inspect_report,
    MultiChannelRouteInspectConfig, MultiChannelRouteInspectReport,
};

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
                let secure_mode = channel_policy_file
                    .map(|policy| policy.secure_messaging.mode)
                    .unwrap_or_default();
                MultiChannelAccessDecision {
                    policy_channel,
                    channel_policy,
                    secure_messaging: MultiChannelSecureMessagingDecision {
                        mode: secure_mode,
                        status: "skipped",
                        reason_code: SECURE_REASON_SKIPPED_POLICY_DENY.to_string(),
                        key_id: None,
                        nonce: None,
                        checked: false,
                        required: secure_mode == MultiChannelSecureMessagingMode::Required,
                        legacy_fallback: false,
                    },
                    pairing_decision: denied.clone(),
                    final_decision: denied,
                    pairing_checked: false,
                    policy_enforced: true,
                }
            }
            MultiChannelPolicyDecision::Allow { reason_code } => {
                let secure_messaging = self.evaluate_secure_messaging_decision(
                    event,
                    &policy_channel,
                    now_unix_ms,
                    channel_policy_file,
                );
                if secure_messaging.status == "deny" {
                    let denied = MultiChannelPairingDecision::Deny {
                        reason_code: secure_messaging.reason_code.clone(),
                    };
                    return MultiChannelAccessDecision {
                        policy_channel,
                        channel_policy,
                        secure_messaging,
                        pairing_decision: denied.clone(),
                        final_decision: denied,
                        pairing_checked: false,
                        policy_enforced: true,
                    };
                }
                if secure_messaging.status == "allow" {
                    let allow = MultiChannelPairingDecision::Allow {
                        reason_code: secure_messaging.reason_code.clone(),
                    };
                    return MultiChannelAccessDecision {
                        policy_channel,
                        channel_policy,
                        secure_messaging,
                        pairing_decision: allow.clone(),
                        final_decision: allow,
                        pairing_checked: false,
                        policy_enforced: true,
                    };
                }
                match channel_policy.policy.allow_from {
                    MultiChannelAllowFrom::Any => {
                        let policy_enforced = channel_policy.policy.require_mention;
                        let allow = MultiChannelPairingDecision::Allow {
                            reason_code: reason_code.clone(),
                        };
                        MultiChannelAccessDecision {
                            policy_channel,
                            channel_policy,
                            secure_messaging,
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
                            secure_messaging,
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
                            secure_messaging,
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

    fn evaluate_secure_messaging_decision(
        &self,
        event: &MultiChannelInboundEvent,
        policy_channel: &str,
        now_unix_ms: u64,
        channel_policy_file: Option<&MultiChannelPolicyFile>,
    ) -> MultiChannelSecureMessagingDecision {
        let policy = channel_policy_file
            .map(|value| value.secure_messaging.clone())
            .unwrap_or_default();
        let mode = policy.mode;
        if mode == MultiChannelSecureMessagingMode::Disabled {
            return MultiChannelSecureMessagingDecision {
                mode,
                status: "disabled",
                reason_code: SECURE_REASON_DISABLED.to_string(),
                key_id: None,
                nonce: None,
                checked: false,
                required: false,
                legacy_fallback: false,
            };
        }

        let mut secure_config = signed_envelope_policy_for_state_dir(&self.config.state_dir);
        secure_config.timestamp_skew_seconds = policy.timestamp_skew_seconds;
        secure_config.replay_window_seconds = policy.replay_window_seconds;
        let decision = evaluate_signed_envelope_access(
            &secure_config,
            &SignedEnvelopeContext {
                policy_channel,
                actor_id: event.actor_id.as_str(),
                event_id: event.event_id.as_str(),
                event_timestamp_ms: event.timestamp_ms,
                text: event.text.as_str(),
                metadata: &event.metadata,
                now_unix_ms,
            },
        );
        match decision {
            SignedEnvelopeDecision::Allow {
                reason_code,
                key_id,
                nonce,
            } => MultiChannelSecureMessagingDecision {
                mode,
                status: "allow",
                reason_code,
                key_id: Some(key_id),
                nonce: Some(nonce),
                checked: true,
                required: mode == MultiChannelSecureMessagingMode::Required,
                legacy_fallback: false,
            },
            SignedEnvelopeDecision::Missing { reason_code } => {
                if mode == MultiChannelSecureMessagingMode::Required {
                    MultiChannelSecureMessagingDecision {
                        mode,
                        status: "deny",
                        reason_code: SECURE_REASON_DENY_MISSING_REQUIRED.to_string(),
                        key_id: None,
                        nonce: None,
                        checked: true,
                        required: true,
                        legacy_fallback: false,
                    }
                } else {
                    MultiChannelSecureMessagingDecision {
                        mode,
                        status: "missing",
                        reason_code,
                        key_id: None,
                        nonce: None,
                        checked: true,
                        required: false,
                        legacy_fallback: true,
                    }
                }
            }
            SignedEnvelopeDecision::Deny { reason_code } => MultiChannelSecureMessagingDecision {
                mode,
                status: "deny",
                reason_code,
                key_id: None,
                nonce: None,
                checked: true,
                required: mode == MultiChannelSecureMessagingMode::Required,
                legacy_fallback: false,
            },
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
        let secure_messaging_payload = json!({
            "mode": access_decision.secure_messaging.mode.as_str(),
            "status": access_decision.secure_messaging.status,
            "reason_code": access_decision.secure_messaging.reason_code.clone(),
            "checked": access_decision.secure_messaging.checked,
            "required": access_decision.secure_messaging.required,
            "legacy_fallback": access_decision.secure_messaging.legacy_fallback,
            "key_id": access_decision.secure_messaging.key_id.clone(),
            "nonce": access_decision.secure_messaging.nonce.clone(),
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
            map.insert(
                "secure_messaging".to_string(),
                secure_messaging_payload.clone(),
            );
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
                        "secure_messaging": secure_messaging_payload,
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
                    secure_messaging_payload: &secure_messaging_payload,
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
                "secure_messaging": secure_messaging_payload,
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
mod tests;
