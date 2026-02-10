use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth_commands::execute_auth_command;
use crate::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use crate::diagnostics_commands::{
    execute_doctor_command, execute_doctor_command_with_options, DoctorCheckOptions,
    DoctorCommandOutputFormat,
};
use crate::multi_agent_router::{load_multi_agent_route_table, MultiAgentRouteTable};
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
use crate::pairing::{evaluate_pairing_access, pairing_policy_for_state_dir, PairingDecision};
use crate::runtime_types::{AuthCommandConfig, DoctorCommandConfig};
use crate::Cli;
use crate::{current_unix_timestamp_ms, write_text_atomic, TransportHealthSnapshot};

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

fn multi_channel_runtime_state_schema_version() -> u32 {
    MULTI_CHANNEL_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub(crate) struct MultiChannelTelemetryConfig {
    pub(crate) typing_presence_enabled: bool,
    pub(crate) usage_summary_enabled: bool,
    pub(crate) include_identifiers: bool,
    pub(crate) typing_presence_min_response_chars: usize,
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

#[derive(Debug, Clone)]
pub(crate) struct MultiChannelRuntimeConfig {
    pub(crate) fixture_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) orchestrator_route_table_path: Option<PathBuf>,
    pub(crate) queue_limit: usize,
    pub(crate) processed_event_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
    pub(crate) retry_jitter_ms: u64,
    pub(crate) outbound: MultiChannelOutboundConfig,
    pub(crate) telemetry: MultiChannelTelemetryConfig,
    pub(crate) media: MultiChannelMediaUnderstandingConfig,
    pub(crate) auth_command_config: AuthCommandConfig,
    pub(crate) doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone)]
pub(crate) struct MultiChannelLiveRuntimeConfig {
    pub(crate) ingress_dir: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) orchestrator_route_table_path: Option<PathBuf>,
    pub(crate) queue_limit: usize,
    pub(crate) processed_event_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
    pub(crate) retry_jitter_ms: u64,
    pub(crate) outbound: MultiChannelOutboundConfig,
    pub(crate) telemetry: MultiChannelTelemetryConfig,
    pub(crate) media: MultiChannelMediaUnderstandingConfig,
    pub(crate) auth_command_config: AuthCommandConfig,
    pub(crate) doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MultiChannelRuntimeSummary {
    pub(crate) discovered_events: usize,
    pub(crate) queued_events: usize,
    pub(crate) completed_events: usize,
    pub(crate) duplicate_skips: usize,
    pub(crate) transient_failures: usize,
    pub(crate) retry_attempts: usize,
    pub(crate) failed_events: usize,
    pub(crate) policy_checked_events: usize,
    pub(crate) policy_enforced_events: usize,
    pub(crate) policy_allowed_events: usize,
    pub(crate) policy_denied_events: usize,
    pub(crate) typing_events_emitted: usize,
    pub(crate) presence_events_emitted: usize,
    pub(crate) usage_summary_records: usize,
    pub(crate) usage_response_chars: usize,
    pub(crate) usage_chunks: usize,
    pub(crate) usage_estimated_cost_micros: u64,
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
    pairing_decision: PairingDecision,
    final_decision: PairingDecision,
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
    AuthStatus { provider: Option<String> },
    Doctor { online: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MultiChannelCommandExecution {
    command_line: String,
    status: String,
    reason_code: String,
    response_text: String,
}

pub(crate) async fn run_multi_channel_contract_runner(
    config: MultiChannelRuntimeConfig,
) -> Result<()> {
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

pub(crate) async fn run_multi_channel_live_runner(
    config: MultiChannelLiveRuntimeConfig,
) -> Result<()> {
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
        auth_command_config: config.auth_command_config.clone(),
        doctor_config: config.doctor_config.clone(),
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
pub(crate) struct MultiChannelRouteInspectReport {
    pub(crate) event_key: String,
    pub(crate) transport: String,
    pub(crate) conversation_id: String,
    pub(crate) actor_id: String,
    pub(crate) binding_id: String,
    pub(crate) binding_matched: bool,
    pub(crate) match_specificity: usize,
    pub(crate) phase: String,
    pub(crate) account_id: String,
    pub(crate) requested_category: Option<String>,
    pub(crate) selected_category: Option<String>,
    pub(crate) selected_role: String,
    pub(crate) fallback_roles: Vec<String>,
    pub(crate) attempt_roles: Vec<String>,
    pub(crate) session_key: String,
}

pub(crate) fn build_multi_channel_route_inspect_report(
    cli: &Cli,
) -> Result<MultiChannelRouteInspectReport> {
    let inspect_file = cli
        .multi_channel_route_inspect_file
        .as_ref()
        .ok_or_else(|| anyhow!("--multi-channel-route-inspect-file is required"))?;
    let event = load_multi_channel_route_inspect_event(inspect_file)?;
    let route_table = if let Some(path) = cli.orchestrator_route_table.as_deref() {
        load_multi_agent_route_table(path)?
    } else {
        MultiAgentRouteTable::default()
    };
    let route_bindings =
        load_multi_channel_route_bindings_for_state_dir(&cli.multi_channel_state_dir)?;
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

pub(crate) fn execute_multi_channel_route_inspect_command(cli: &Cli) -> Result<()> {
    let report = build_multi_channel_route_inspect_report(cli)?;
    if cli.multi_channel_route_inspect_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render multi-channel route inspect json")?
        );
    } else {
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
        println!(
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
        );
    }
    Ok(())
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
                            PairingDecision::Allow { .. }
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
                let denied = PairingDecision::Deny {
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
                        let allow = PairingDecision::Allow {
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
                            PairingDecision::Allow { reason_code }
                                if reason_code == "allow_allowlist"
                                    || reason_code == "allow_allowlist_and_pairing" =>
                            {
                                PairingDecision::Allow {
                                    reason_code: reason_code.clone(),
                                }
                            }
                            PairingDecision::Allow { .. } => PairingDecision::Deny {
                                reason_code: POLICY_REASON_DENY_ALLOWLIST_ONLY.to_string(),
                            },
                            PairingDecision::Deny { reason_code } => PairingDecision::Deny {
                                reason_code: reason_code.clone(),
                            },
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
    ) -> PairingDecision {
        let pairing_policy = pairing_policy_for_state_dir(&self.config.state_dir);
        match evaluate_pairing_access(
            &pairing_policy,
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
                PairingDecision::Deny {
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
                let mut args = String::from("status");
                if let Some(provider) = provider.as_deref() {
                    args.push(' ');
                    args.push_str(provider);
                }
                let output = execute_auth_command(&self.config.auth_command_config, &args);
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
                let output = if online {
                    execute_doctor_command_with_options(
                        &self.config.doctor_config,
                        DoctorCommandOutputFormat::Text,
                        DoctorCheckOptions { online: true },
                    )
                } else {
                    execute_doctor_command(
                        &self.config.doctor_config,
                        DoctorCommandOutputFormat::Text,
                    )
                };
                let command_line = if online { "doctor --online" } else { "doctor" };
                Some(build_multi_channel_command_execution(
                    command_line,
                    COMMAND_STATUS_REPORTED,
                    COMMAND_REASON_DOCTOR_REPORTED,
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

        if let PairingDecision::Deny { reason_code } = &access_decision.final_decision {
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

fn pairing_policy_channel(event: &MultiChannelInboundEvent) -> String {
    format!(
        "{}:{}",
        event.transport.as_str(),
        event.conversation_id.trim()
    )
}

fn pairing_decision_status(decision: &PairingDecision) -> &'static str {
    if matches!(decision, PairingDecision::Allow { .. }) {
        "allow"
    } else {
        "deny"
    }
}

fn pairing_decision_is_enforced(decision: &PairingDecision) -> bool {
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
        _ => Err(COMMAND_REASON_UNKNOWN.to_string()),
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
    }
}

fn render_multi_channel_tau_command_help() -> String {
    [
        "supported /tau commands:",
        "- /tau help",
        "- /tau status",
        "- /tau auth status [openai|anthropic|google]",
        "- /tau doctor [--online]",
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
        MultiChannelTauCommand::AuthStatus { .. } | MultiChannelTauCommand::Doctor { .. }
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

    use httpmock::Method::POST;
    use httpmock::MockServer;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_multi_channel_live_events, load_multi_channel_runtime_state, retry_delay_ms,
        run_multi_channel_live_runner, MultiChannelLiveRuntimeConfig, MultiChannelRuntime,
        MultiChannelRuntimeConfig, MultiChannelTelemetryConfig,
    };
    use crate::channel_store::ChannelStore;
    use crate::multi_channel_contract::{
        load_multi_channel_contract_fixture, parse_multi_channel_contract_fixture,
        MultiChannelAttachment, MultiChannelEventKind, MultiChannelInboundEvent,
        MultiChannelTransport,
    };
    use crate::multi_channel_outbound::{MultiChannelOutboundConfig, MultiChannelOutboundMode};
    use crate::runtime_types::{
        AuthCommandConfig, DoctorCommandConfig, DoctorMultiChannelReadinessConfig,
    };
    use crate::transport_health::TransportHealthState;
    use crate::{CredentialStoreEncryptionMode, ProviderAuthMethod};

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

    fn test_auth_command_config(root: &Path) -> AuthCommandConfig {
        AuthCommandConfig {
            credential_store: root.join(".tau/credentials.json"),
            credential_store_key: None,
            credential_store_encryption: CredentialStoreEncryptionMode::None,
            api_key: None,
            openai_api_key: None,
            anthropic_api_key: None,
            google_api_key: None,
            openai_auth_mode: ProviderAuthMethod::ApiKey,
            anthropic_auth_mode: ProviderAuthMethod::ApiKey,
            google_auth_mode: ProviderAuthMethod::ApiKey,
            provider_subscription_strict: false,
            openai_codex_backend: true,
            openai_codex_cli: "codex".to_string(),
            anthropic_claude_backend: true,
            anthropic_claude_cli: "claude".to_string(),
            google_gemini_backend: true,
            google_gemini_cli: "gemini".to_string(),
            google_gcloud_cli: "gcloud".to_string(),
        }
    }

    fn test_doctor_command_config(root: &Path) -> DoctorCommandConfig {
        DoctorCommandConfig {
            model: "openai/gpt-4o-mini".to_string(),
            provider_keys: Vec::new(),
            release_channel_path: root.join(".tau/release-channel.json"),
            release_lookup_cache_path: root.join(".tau/release-lookup-cache.json"),
            release_lookup_cache_ttl_ms: 3_600_000,
            browser_automation_playwright_cli: "playwright".to_string(),
            session_enabled: true,
            session_path: root.join(".tau/session.jsonl"),
            skills_dir: root.join(".tau/skills"),
            skills_lock_path: root.join(".tau/skills.lock.json"),
            trust_root_path: None,
            multi_channel_live_readiness: DoctorMultiChannelReadinessConfig {
                ingress_dir: root.join(".tau/multi-channel/live-ingress"),
                credential_store_path: root.join(".tau/credentials.json"),
                credential_store_encryption: CredentialStoreEncryptionMode::None,
                credential_store_key: None,
                telegram_bot_token: None,
                discord_bot_token: None,
                whatsapp_access_token: None,
                whatsapp_phone_number_id: None,
            },
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
            auth_command_config: test_auth_command_config(root),
            doctor_config: test_doctor_command_config(root),
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
            auth_command_config: test_auth_command_config(root),
            doctor_config: test_doctor_command_config(root),
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
