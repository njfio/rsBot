use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::multi_agent_contract::{
    evaluate_multi_agent_case, load_multi_agent_contract_fixture,
    validate_multi_agent_case_result_against_contract, MultiAgentContractCase,
    MultiAgentContractFixture, MultiAgentReplayStep,
};
use tau_core::atomic_io::write_text_atomic;
use tau_core::time_utils::current_unix_timestamp_ms;
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use tau_runtime::transport_health::TransportHealthSnapshot;

const MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const MULTI_AGENT_RUNTIME_SETTLEMENT_AUDIT_LOG_FILE: &str = "settlement-audit.jsonl";
const MULTI_AGENT_RUNTIME_SETTLEMENT_DIAGNOSTICS_FILE: &str = "settlement-diagnostics.json";
const DEFAULT_ESCROW_RESERVE_MICROS: u64 = 10_000;
const DEFAULT_EXECUTION_COST_MICROS: u64 = 2_000;
const ESCROW_TIMEOUT_MS: u64 = 300_000;

fn multi_agent_runtime_state_schema_version() -> u32 {
    MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
/// Public struct `MultiAgentRuntimeConfig` used across Tau components.
pub struct MultiAgentRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `MultiAgentRuntimeSummary` used across Tau components.
pub struct MultiAgentRuntimeSummary {
    pub discovered_cases: usize,
    pub queued_cases: usize,
    pub applied_cases: usize,
    pub duplicate_skips: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub retry_attempts: usize,
    pub failed_cases: usize,
    pub routed_cases_upserted: usize,
    pub escrow_reserved_cases: usize,
    pub escrow_settled_cases: usize,
    pub escrow_refunded_cases: usize,
    pub settlement_idempotent_skips: usize,
    pub settlement_timeout_refunds: usize,
    pub settlement_audit_events: usize,
}

#[derive(Debug, Clone, Serialize)]
struct MultiAgentRuntimeCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    discovered_cases: usize,
    queued_cases: usize,
    applied_cases: usize,
    duplicate_skips: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    retry_attempts: usize,
    failed_cases: usize,
    routed_cases_upserted: usize,
    escrow_reserved_cases: usize,
    escrow_settled_cases: usize,
    escrow_refunded_cases: usize,
    settlement_idempotent_skips: usize,
    settlement_timeout_refunds: usize,
    settlement_audit_events: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MultiAgentEscrowState {
    Reserved,
    Settled,
    Refunded,
}

impl MultiAgentEscrowState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Settled => "settled",
            Self::Refunded => "refunded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MultiAgentSettlementArtifact {
    artifact_type: String,
    artifact_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MultiAgentSettlementRecord {
    case_key: String,
    case_id: String,
    phase: String,
    settlement_reference: String,
    escrow_state: MultiAgentEscrowState,
    reserved_micros: u64,
    released_micros: u64,
    refunded_micros: u64,
    execution_cost_micros: u64,
    finalized_error_code: String,
    reserved_unix_ms: u64,
    finalized_unix_ms: Option<u64>,
    updated_unix_ms: u64,
    #[serde(default)]
    artifacts: Vec<MultiAgentSettlementArtifact>,
}

#[derive(Debug, Clone, Serialize)]
struct MultiAgentSettlementAuditEvent {
    timestamp_unix_ms: u64,
    case_key: String,
    case_id: String,
    phase: String,
    settlement_reference: String,
    event_kind: String,
    escrow_state_before: String,
    escrow_state_after: String,
    reserved_micros: u64,
    released_micros: u64,
    refunded_micros: u64,
    execution_cost_micros: u64,
    error_code: String,
    reason_code: String,
}

#[derive(Debug, Clone, Serialize)]
struct MultiAgentSettlementDiagnostics {
    updated_unix_ms: u64,
    total_records: usize,
    state_counts: BTreeMap<String, usize>,
    total_reserved_micros: u64,
    total_released_micros: u64,
    total_refunded_micros: u64,
    total_execution_cost_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MultiAgentRoutedCase {
    case_key: String,
    case_id: String,
    phase: String,
    selected_role: String,
    attempted_roles: Vec<String>,
    category: String,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultiAgentRuntimeState {
    #[serde(default = "multi_agent_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    routed_cases: Vec<MultiAgentRoutedCase>,
    #[serde(default)]
    settlement_records: Vec<MultiAgentSettlementRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for MultiAgentRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            routed_cases: Vec::new(),
            settlement_records: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub async fn run_multi_agent_contract_runner(config: MultiAgentRuntimeConfig) -> Result<()> {
    let fixture = load_multi_agent_contract_fixture(&config.fixture_path)?;
    let mut runtime = MultiAgentRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "multi-agent runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} routed_cases_upserted={} escrow_reserved={} escrow_settled={} escrow_refunded={} settlement_idempotent_skips={} settlement_timeout_refunds={} settlement_audit_events={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.routed_cases_upserted,
        summary.escrow_reserved_cases,
        summary.escrow_settled_cases,
        summary.escrow_refunded_cases,
        summary.settlement_idempotent_skips,
        summary.settlement_timeout_refunds,
        summary.settlement_audit_events,
    );
    println!(
        "multi-agent runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct MultiAgentRuntime {
    config: MultiAgentRuntimeConfig,
    state: MultiAgentRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl MultiAgentRuntime {
    fn new(config: MultiAgentRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_multi_agent_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .routed_cases
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));
        state
            .settlement_records
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));

        let processed_case_keys = state.processed_case_keys.iter().cloned().collect();
        Ok(Self {
            config,
            state,
            processed_case_keys,
        })
    }

    fn state_path(&self) -> PathBuf {
        self.config.state_dir.join("state.json")
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    async fn run_once(
        &mut self,
        fixture: &MultiAgentContractFixture,
    ) -> Result<MultiAgentRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = MultiAgentRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..MultiAgentRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.phase.as_str().cmp(right.phase.as_str()))
        });
        queued_cases.truncate(self.config.queue_limit);
        summary.queued_cases = queued_cases.len();

        for case in queued_cases {
            let case_key = case_runtime_key(&case);
            if self.processed_case_keys.contains(&case_key) {
                summary.duplicate_skips = summary.duplicate_skips.saturating_add(1);
                continue;
            }
            self.ensure_case_escrow_reservation(&case, &case_key, &mut summary)?;

            let mut attempt = 1usize;
            loop {
                let result = evaluate_multi_agent_case(&case);
                validate_multi_agent_case_result_against_contract(&case, &result)?;
                match result.step {
                    MultiAgentReplayStep::Success => {
                        let settlement = self.finalize_success_settlement(
                            &case,
                            &case_key,
                            &result,
                            &mut summary,
                        )?;
                        let upserted = self.persist_success_result(
                            &case,
                            &case_key,
                            &result,
                            settlement.as_ref(),
                        )?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.routed_cases_upserted =
                            summary.routed_cases_upserted.saturating_add(upserted);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    MultiAgentReplayStep::MalformedInput => {
                        let settlement = self.finalize_failure_settlement(
                            &case,
                            &case_key,
                            result.error_code.as_deref(),
                            "malformed_input",
                            &mut summary,
                        )?;
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(
                            &case,
                            &case_key,
                            &result,
                            settlement.as_ref(),
                        )?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    MultiAgentReplayStep::RetryableFailure => {
                        summary.retryable_failures = summary.retryable_failures.saturating_add(1);
                        if attempt >= self.config.retry_max_attempts {
                            let settlement = self.finalize_failure_settlement(
                                &case,
                                &case_key,
                                result.error_code.as_deref(),
                                "retry_attempts_exhausted",
                                &mut summary,
                            )?;
                            summary.failed_cases = summary.failed_cases.saturating_add(1);
                            self.persist_non_success_result(
                                &case,
                                &case_key,
                                &result,
                                settlement.as_ref(),
                            )?;
                            break;
                        }
                        summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                        apply_retry_delay(self.config.retry_base_delay_ms, attempt).await;
                        attempt = attempt.saturating_add(1);
                    }
                }
            }
        }
        self.reconcile_settlement_timeouts(&mut summary)?;
        self.persist_runtime_snapshot_memory()?;
        write_multi_agent_settlement_diagnostics(
            &self
                .config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_SETTLEMENT_DIAGNOSTICS_FILE),
            &self.state.settlement_records,
        )?;

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

        save_multi_agent_runtime_state(&self.state_path(), &self.state)?;
        append_multi_agent_cycle_report(
            &self
                .config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn ensure_case_escrow_reservation(
        &mut self,
        case: &MultiAgentContractCase,
        case_key: &str,
        summary: &mut MultiAgentRuntimeSummary,
    ) -> Result<()> {
        if self
            .state
            .settlement_records
            .iter()
            .any(|record| record.case_key == case_key)
        {
            summary.settlement_idempotent_skips =
                summary.settlement_idempotent_skips.saturating_add(1);
            return Ok(());
        }

        let timestamp_unix_ms = current_unix_timestamp_ms();
        let reserve_micros = case
            .economics
            .escrow_reserve_micros
            .unwrap_or(DEFAULT_ESCROW_RESERVE_MICROS);
        let settlement_reference = normalize_settlement_reference(case);

        let record = MultiAgentSettlementRecord {
            case_key: case_key.to_string(),
            case_id: case.case_id.clone(),
            phase: case.phase.as_str().to_string(),
            settlement_reference: settlement_reference.clone(),
            escrow_state: MultiAgentEscrowState::Reserved,
            reserved_micros: reserve_micros,
            released_micros: 0,
            refunded_micros: 0,
            execution_cost_micros: 0,
            finalized_error_code: String::new(),
            reserved_unix_ms: timestamp_unix_ms,
            finalized_unix_ms: None,
            updated_unix_ms: timestamp_unix_ms,
            artifacts: Vec::new(),
        };
        self.state.settlement_records.push(record.clone());
        self.state
            .settlement_records
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));

        self.append_settlement_audit_event(&MultiAgentSettlementAuditEvent {
            timestamp_unix_ms,
            case_key: case_key.to_string(),
            case_id: case.case_id.clone(),
            phase: case.phase.as_str().to_string(),
            settlement_reference,
            event_kind: "reserve".to_string(),
            escrow_state_before: "none".to_string(),
            escrow_state_after: record.escrow_state.as_str().to_string(),
            reserved_micros: record.reserved_micros,
            released_micros: record.released_micros,
            refunded_micros: record.refunded_micros,
            execution_cost_micros: record.execution_cost_micros,
            error_code: String::new(),
            reason_code: "escrow_reserved".to_string(),
        })?;
        summary.escrow_reserved_cases = summary.escrow_reserved_cases.saturating_add(1);
        summary.settlement_audit_events = summary.settlement_audit_events.saturating_add(1);
        Ok(())
    }

    fn finalize_success_settlement(
        &mut self,
        case: &MultiAgentContractCase,
        case_key: &str,
        result: &crate::multi_agent_contract::MultiAgentReplayResult,
        summary: &mut MultiAgentRuntimeSummary,
    ) -> Result<Option<MultiAgentSettlementRecord>> {
        let Some(index) = self
            .state
            .settlement_records
            .iter()
            .position(|record| record.case_key == case_key)
        else {
            summary.settlement_idempotent_skips =
                summary.settlement_idempotent_skips.saturating_add(1);
            return Ok(None);
        };

        let timestamp_unix_ms = current_unix_timestamp_ms();
        let (event, snapshot, idempotent_skip, refunded_micros) = {
            let record = &mut self.state.settlement_records[index];
            if record.escrow_state != MultiAgentEscrowState::Reserved {
                (
                    MultiAgentSettlementAuditEvent {
                        timestamp_unix_ms,
                        case_key: record.case_key.clone(),
                        case_id: record.case_id.clone(),
                        phase: record.phase.clone(),
                        settlement_reference: record.settlement_reference.clone(),
                        event_kind: "settle_success_skip".to_string(),
                        escrow_state_before: record.escrow_state.as_str().to_string(),
                        escrow_state_after: record.escrow_state.as_str().to_string(),
                        reserved_micros: record.reserved_micros,
                        released_micros: record.released_micros,
                        refunded_micros: record.refunded_micros,
                        execution_cost_micros: record.execution_cost_micros,
                        error_code: String::new(),
                        reason_code: "settlement_idempotent_skip".to_string(),
                    },
                    record.clone(),
                    true,
                    record.refunded_micros,
                )
            } else {
                let attempted_roles = u64::try_from(result.attempted_roles.len()).unwrap_or(1);
                let baseline_cost = attempted_roles
                    .max(1)
                    .saturating_mul(DEFAULT_EXECUTION_COST_MICROS);
                let requested_cost = case
                    .economics
                    .execution_cost_micros
                    .unwrap_or(baseline_cost);
                let cost_micros = requested_cost.min(record.reserved_micros);
                let refunded_micros = record.reserved_micros.saturating_sub(cost_micros);

                record.escrow_state = MultiAgentEscrowState::Settled;
                record.execution_cost_micros = cost_micros;
                record.released_micros = cost_micros;
                record.refunded_micros = refunded_micros;
                record.finalized_error_code.clear();
                record.finalized_unix_ms = Some(timestamp_unix_ms);
                record.updated_unix_ms = timestamp_unix_ms;
                upsert_settlement_artifact(&mut record.artifacts, "route_case", case_key);
                upsert_settlement_artifact(
                    &mut record.artifacts,
                    "selected_role",
                    result.selected_role.as_str(),
                );

                (
                    MultiAgentSettlementAuditEvent {
                        timestamp_unix_ms,
                        case_key: record.case_key.clone(),
                        case_id: record.case_id.clone(),
                        phase: record.phase.clone(),
                        settlement_reference: record.settlement_reference.clone(),
                        event_kind: "settle_success".to_string(),
                        escrow_state_before: MultiAgentEscrowState::Reserved.as_str().to_string(),
                        escrow_state_after: record.escrow_state.as_str().to_string(),
                        reserved_micros: record.reserved_micros,
                        released_micros: record.released_micros,
                        refunded_micros: record.refunded_micros,
                        execution_cost_micros: record.execution_cost_micros,
                        error_code: String::new(),
                        reason_code: "settlement_success".to_string(),
                    },
                    record.clone(),
                    false,
                    refunded_micros,
                )
            }
        };

        self.append_settlement_audit_event(&event)?;
        if idempotent_skip {
            summary.settlement_idempotent_skips =
                summary.settlement_idempotent_skips.saturating_add(1);
        } else {
            summary.escrow_settled_cases = summary.escrow_settled_cases.saturating_add(1);
            if refunded_micros > 0 {
                summary.escrow_refunded_cases = summary.escrow_refunded_cases.saturating_add(1);
            }
        }
        summary.settlement_audit_events = summary.settlement_audit_events.saturating_add(1);
        Ok(Some(snapshot))
    }

    fn finalize_failure_settlement(
        &mut self,
        _case: &MultiAgentContractCase,
        case_key: &str,
        error_code: Option<&str>,
        reason_code: &str,
        summary: &mut MultiAgentRuntimeSummary,
    ) -> Result<Option<MultiAgentSettlementRecord>> {
        let Some(index) = self
            .state
            .settlement_records
            .iter()
            .position(|record| record.case_key == case_key)
        else {
            summary.settlement_idempotent_skips =
                summary.settlement_idempotent_skips.saturating_add(1);
            return Ok(None);
        };

        let timestamp_unix_ms = current_unix_timestamp_ms();
        let (event, snapshot, idempotent_skip) = {
            let record = &mut self.state.settlement_records[index];
            if record.escrow_state != MultiAgentEscrowState::Reserved {
                (
                    MultiAgentSettlementAuditEvent {
                        timestamp_unix_ms,
                        case_key: record.case_key.clone(),
                        case_id: record.case_id.clone(),
                        phase: record.phase.clone(),
                        settlement_reference: record.settlement_reference.clone(),
                        event_kind: "settle_failure_skip".to_string(),
                        escrow_state_before: record.escrow_state.as_str().to_string(),
                        escrow_state_after: record.escrow_state.as_str().to_string(),
                        reserved_micros: record.reserved_micros,
                        released_micros: record.released_micros,
                        refunded_micros: record.refunded_micros,
                        execution_cost_micros: record.execution_cost_micros,
                        error_code: error_code.unwrap_or_default().to_string(),
                        reason_code: "settlement_idempotent_skip".to_string(),
                    },
                    record.clone(),
                    true,
                )
            } else {
                record.escrow_state = MultiAgentEscrowState::Refunded;
                record.execution_cost_micros = 0;
                record.refunded_micros = record
                    .reserved_micros
                    .saturating_sub(record.released_micros);
                record.finalized_error_code = error_code.unwrap_or_default().to_string();
                record.finalized_unix_ms = Some(timestamp_unix_ms);
                record.updated_unix_ms = timestamp_unix_ms;
                upsert_settlement_artifact(&mut record.artifacts, "failure_reason", reason_code);

                (
                    MultiAgentSettlementAuditEvent {
                        timestamp_unix_ms,
                        case_key: record.case_key.clone(),
                        case_id: record.case_id.clone(),
                        phase: record.phase.clone(),
                        settlement_reference: record.settlement_reference.clone(),
                        event_kind: "settle_failure".to_string(),
                        escrow_state_before: MultiAgentEscrowState::Reserved.as_str().to_string(),
                        escrow_state_after: record.escrow_state.as_str().to_string(),
                        reserved_micros: record.reserved_micros,
                        released_micros: record.released_micros,
                        refunded_micros: record.refunded_micros,
                        execution_cost_micros: record.execution_cost_micros,
                        error_code: error_code.unwrap_or_default().to_string(),
                        reason_code: reason_code.to_string(),
                    },
                    record.clone(),
                    false,
                )
            }
        };

        self.append_settlement_audit_event(&event)?;
        if idempotent_skip {
            summary.settlement_idempotent_skips =
                summary.settlement_idempotent_skips.saturating_add(1);
        } else {
            summary.escrow_refunded_cases = summary.escrow_refunded_cases.saturating_add(1);
        }
        summary.settlement_audit_events = summary.settlement_audit_events.saturating_add(1);
        Ok(Some(snapshot))
    }

    fn reconcile_settlement_timeouts(
        &mut self,
        summary: &mut MultiAgentRuntimeSummary,
    ) -> Result<()> {
        let now_unix_ms = current_unix_timestamp_ms();
        let mut timeout_events = Vec::new();
        for record in &mut self.state.settlement_records {
            if record.escrow_state != MultiAgentEscrowState::Reserved {
                continue;
            }
            if now_unix_ms.saturating_sub(record.reserved_unix_ms) <= ESCROW_TIMEOUT_MS {
                continue;
            }
            record.escrow_state = MultiAgentEscrowState::Refunded;
            record.execution_cost_micros = 0;
            record.refunded_micros = record
                .reserved_micros
                .saturating_sub(record.released_micros);
            record.finalized_error_code = "escrow_timeout".to_string();
            record.finalized_unix_ms = Some(now_unix_ms);
            record.updated_unix_ms = now_unix_ms;
            upsert_settlement_artifact(&mut record.artifacts, "failure_reason", "escrow_timeout");

            timeout_events.push(MultiAgentSettlementAuditEvent {
                timestamp_unix_ms: now_unix_ms,
                case_key: record.case_key.clone(),
                case_id: record.case_id.clone(),
                phase: record.phase.clone(),
                settlement_reference: record.settlement_reference.clone(),
                event_kind: "timeout_refund".to_string(),
                escrow_state_before: MultiAgentEscrowState::Reserved.as_str().to_string(),
                escrow_state_after: record.escrow_state.as_str().to_string(),
                reserved_micros: record.reserved_micros,
                released_micros: record.released_micros,
                refunded_micros: record.refunded_micros,
                execution_cost_micros: record.execution_cost_micros,
                error_code: "escrow_timeout".to_string(),
                reason_code: "escrow_timeout_refund".to_string(),
            });
            summary.escrow_refunded_cases = summary.escrow_refunded_cases.saturating_add(1);
            summary.settlement_timeout_refunds =
                summary.settlement_timeout_refunds.saturating_add(1);
        }

        for event in timeout_events {
            self.append_settlement_audit_event(&event)?;
            summary.settlement_audit_events = summary.settlement_audit_events.saturating_add(1);
        }
        Ok(())
    }

    fn append_settlement_audit_event(&self, event: &MultiAgentSettlementAuditEvent) -> Result<()> {
        append_ndjson_event(
            &self
                .config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_SETTLEMENT_AUDIT_LOG_FILE),
            event,
            "serialize settlement audit event",
        )
    }

    fn persist_runtime_snapshot_memory(&self) -> Result<()> {
        let store = self.channel_store()?;
        store.write_memory(&render_multi_agent_runtime_snapshot(
            &self.state.routed_cases,
            &self.state.settlement_records,
        ))
    }

    fn persist_success_result(
        &mut self,
        case: &MultiAgentContractCase,
        case_key: &str,
        result: &crate::multi_agent_contract::MultiAgentReplayResult,
        settlement: Option<&MultiAgentSettlementRecord>,
    ) -> Result<usize> {
        let routed_case = MultiAgentRoutedCase {
            case_key: case_key.to_string(),
            case_id: case.case_id.clone(),
            phase: case.phase.as_str().to_string(),
            selected_role: result.selected_role.clone(),
            attempted_roles: result.attempted_roles.clone(),
            category: result.category.clone(),
            updated_unix_ms: current_unix_timestamp_ms(),
        };

        if let Some(existing) = self
            .state
            .routed_cases
            .iter_mut()
            .find(|existing| existing.case_key == routed_case.case_key)
        {
            *existing = routed_case;
        } else {
            self.state.routed_cases.push(routed_case);
        }
        self.state
            .routed_cases
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));

        let store = self.channel_store()?;
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let settlement_state = settlement
            .map(|record| record.escrow_state.as_str())
            .unwrap_or("none");
        let settlement_reference = settlement
            .map(|record| record.settlement_reference.as_str())
            .unwrap_or("");
        let reserved_micros = settlement.map(|record| record.reserved_micros).unwrap_or(0);
        let released_micros = settlement.map(|record| record.released_micros).unwrap_or(0);
        let refunded_micros = settlement.map(|record| record.refunded_micros).unwrap_or(0);
        let cost_micros = settlement
            .map(|record| record.execution_cost_micros)
            .unwrap_or(0);
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms,
            direction: "system".to_string(),
            event_key: Some(case_key.to_string()),
            source: "tau-multi-agent-runner".to_string(),
            payload: json!({
                "outcome": "success",
                "phase": case.phase.as_str(),
                "case_id": case.case_id,
                "selected_role": result.selected_role,
                "attempted_roles": result.attempted_roles,
                "category": result.category,
                "settlement_reference": settlement_reference,
                "escrow_state": settlement_state,
                "reserved_micros": reserved_micros,
                "released_micros": released_micros,
                "refunded_micros": refunded_micros,
                "execution_cost_micros": cost_micros,
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "multi-agent case {} applied with selected_role={} phase={} escrow_state={} cost_micros={}",
                case.case_id,
                result.selected_role,
                case.phase.as_str(),
                settlement_state,
                cost_micros
            ),
        })?;
        Ok(1)
    }

    fn persist_non_success_result(
        &self,
        case: &MultiAgentContractCase,
        case_key: &str,
        result: &crate::multi_agent_contract::MultiAgentReplayResult,
        settlement: Option<&MultiAgentSettlementRecord>,
    ) -> Result<()> {
        let store = self.channel_store()?;
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let outcome = match result.step {
            MultiAgentReplayStep::Success => "success",
            MultiAgentReplayStep::MalformedInput => "malformed_input",
            MultiAgentReplayStep::RetryableFailure => "retryable_failure",
        };
        let settlement_state = settlement
            .map(|record| record.escrow_state.as_str())
            .unwrap_or("none");
        let settlement_reference = settlement
            .map(|record| record.settlement_reference.as_str())
            .unwrap_or("");
        let reserved_micros = settlement.map(|record| record.reserved_micros).unwrap_or(0);
        let released_micros = settlement.map(|record| record.released_micros).unwrap_or(0);
        let refunded_micros = settlement.map(|record| record.refunded_micros).unwrap_or(0);
        let cost_micros = settlement
            .map(|record| record.execution_cost_micros)
            .unwrap_or(0);
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms,
            direction: "system".to_string(),
            event_key: Some(case_key.to_string()),
            source: "tau-multi-agent-runner".to_string(),
            payload: json!({
                "outcome": outcome,
                "phase": case.phase.as_str(),
                "case_id": case.case_id,
                "error_code": result.error_code.clone().unwrap_or_default(),
                "settlement_reference": settlement_reference,
                "escrow_state": settlement_state,
                "reserved_micros": reserved_micros,
                "released_micros": released_micros,
                "refunded_micros": refunded_micros,
                "execution_cost_micros": cost_micros,
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "multi-agent case {} outcome={} error_code={} escrow_state={}",
                case.case_id,
                outcome,
                result.error_code.clone().unwrap_or_default(),
                settlement_state,
            ),
        })?;
        Ok(())
    }

    fn channel_store(&self) -> Result<ChannelStore> {
        ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "multi-agent",
            "orchestrator-router",
        )
    }

    fn record_processed_case(&mut self, case_key: &str) {
        if self.processed_case_keys.contains(case_key) {
            return;
        }
        self.state.processed_case_keys.push(case_key.to_string());
        self.processed_case_keys.insert(case_key.to_string());
        if self.state.processed_case_keys.len() > self.config.processed_case_cap {
            let overflow = self
                .state
                .processed_case_keys
                .len()
                .saturating_sub(self.config.processed_case_cap);
            let removed = self.state.processed_case_keys.drain(0..overflow);
            for key in removed {
                self.processed_case_keys.remove(&key);
            }
        }
    }
}

fn case_runtime_key(case: &MultiAgentContractCase) -> String {
    format!("{}:{}", case.phase.as_str(), case.case_id.trim())
}

fn normalize_settlement_reference(case: &MultiAgentContractCase) -> String {
    let trimmed = case.economics.settlement_reference.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    format!("kamn://settlement/{}", case_runtime_key(case))
}

fn upsert_settlement_artifact(
    artifacts: &mut Vec<MultiAgentSettlementArtifact>,
    artifact_type: &str,
    artifact_ref: &str,
) {
    let artifact_type = artifact_type.trim();
    let artifact_ref = artifact_ref.trim();
    if artifact_type.is_empty() || artifact_ref.is_empty() {
        return;
    }
    if artifacts.iter().any(|artifact| {
        artifact.artifact_type == artifact_type && artifact.artifact_ref == artifact_ref
    }) {
        return;
    }
    artifacts.push(MultiAgentSettlementArtifact {
        artifact_type: artifact_type.to_string(),
        artifact_ref: artifact_ref.to_string(),
    });
}

fn build_transport_health_snapshot(
    summary: &MultiAgentRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_cases = summary
        .discovered_cases
        .saturating_sub(summary.queued_cases);
    let failure_streak = if summary.failed_cases > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };
    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_cases,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_cases,
        last_cycle_processed: summary
            .applied_cases
            .saturating_add(summary.malformed_cases)
            .saturating_add(summary.failed_cases)
            .saturating_add(summary.duplicate_skips),
        last_cycle_completed: summary
            .applied_cases
            .saturating_add(summary.malformed_cases),
        last_cycle_failed: summary.failed_cases,
        last_cycle_duplicates: summary.duplicate_skips,
    }
}

fn cycle_reason_codes(summary: &MultiAgentRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.discovered_cases > summary.queued_cases {
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        codes.push("duplicate_cases_skipped".to_string());
    }
    if summary.malformed_cases > 0 {
        codes.push("malformed_inputs_observed".to_string());
    }
    if summary.retry_attempts > 0 {
        codes.push("retry_attempted".to_string());
    }
    if summary.retryable_failures > 0 {
        codes.push("retryable_failures_observed".to_string());
    }
    if summary.failed_cases > 0 {
        codes.push("case_processing_failed".to_string());
    }
    if summary.routed_cases_upserted > 0 {
        codes.push("routed_cases_updated".to_string());
    }
    if summary.escrow_reserved_cases > 0 {
        codes.push("escrow_reserved".to_string());
    }
    if summary.escrow_settled_cases > 0 {
        codes.push("escrow_settled".to_string());
    }
    if summary.escrow_refunded_cases > 0 {
        codes.push("escrow_refunded".to_string());
    }
    if summary.settlement_idempotent_skips > 0 {
        codes.push("settlement_idempotent_skip".to_string());
    }
    if summary.settlement_timeout_refunds > 0 {
        codes.push("escrow_timeout_refund".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_multi_agent_cycle_report(
    path: &Path,
    summary: &MultiAgentRuntimeSummary,
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
    let payload = MultiAgentRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        discovered_cases: summary.discovered_cases,
        queued_cases: summary.queued_cases,
        applied_cases: summary.applied_cases,
        duplicate_skips: summary.duplicate_skips,
        malformed_cases: summary.malformed_cases,
        retryable_failures: summary.retryable_failures,
        retry_attempts: summary.retry_attempts,
        failed_cases: summary.failed_cases,
        routed_cases_upserted: summary.routed_cases_upserted,
        escrow_reserved_cases: summary.escrow_reserved_cases,
        escrow_settled_cases: summary.escrow_settled_cases,
        escrow_refunded_cases: summary.escrow_refunded_cases,
        settlement_idempotent_skips: summary.settlement_idempotent_skips,
        settlement_timeout_refunds: summary.settlement_timeout_refunds,
        settlement_audit_events: summary.settlement_audit_events,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize multi-agent runtime report")?;
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

fn append_ndjson_event<T: Serialize>(
    path: &Path,
    payload: &T,
    serialization_context: &str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let line = serde_json::to_string(payload).with_context(|| serialization_context.to_string())?;
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

fn write_multi_agent_settlement_diagnostics(
    path: &Path,
    settlement_records: &[MultiAgentSettlementRecord],
) -> Result<()> {
    let mut state_counts = BTreeMap::new();
    let mut total_reserved_micros = 0_u64;
    let mut total_released_micros = 0_u64;
    let mut total_refunded_micros = 0_u64;
    let mut total_execution_cost_micros = 0_u64;
    for record in settlement_records {
        *state_counts
            .entry(record.escrow_state.as_str().to_string())
            .or_insert(0) += 1;
        total_reserved_micros = total_reserved_micros.saturating_add(record.reserved_micros);
        total_released_micros = total_released_micros.saturating_add(record.released_micros);
        total_refunded_micros = total_refunded_micros.saturating_add(record.refunded_micros);
        total_execution_cost_micros =
            total_execution_cost_micros.saturating_add(record.execution_cost_micros);
    }

    let payload = MultiAgentSettlementDiagnostics {
        updated_unix_ms: current_unix_timestamp_ms(),
        total_records: settlement_records.len(),
        state_counts,
        total_reserved_micros,
        total_released_micros,
        total_refunded_micros,
        total_execution_cost_micros,
    };
    let rendered =
        serde_json::to_string_pretty(&payload).context("serialize settlement diagnostics")?;
    write_text_atomic(path, &rendered)
}

fn normalize_processed_case_keys(raw: &[String], cap: usize) -> Vec<String> {
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

fn retry_delay_ms(base_delay_ms: u64, attempt: usize) -> u64 {
    if base_delay_ms == 0 {
        return 0;
    }
    let shift = u32::try_from(attempt.saturating_sub(1)).unwrap_or(u32::MAX);
    let multiplier = 1_u64.checked_shl(shift).unwrap_or(u64::MAX);
    base_delay_ms.saturating_mul(multiplier)
}

async fn apply_retry_delay(base_delay_ms: u64, attempt: usize) {
    let delay = retry_delay_ms(base_delay_ms, attempt);
    if delay == 0 {
        return;
    }
    tokio::time::sleep(Duration::from_millis(delay)).await;
}

fn render_multi_agent_runtime_snapshot(
    routed_cases: &[MultiAgentRoutedCase],
    settlement_records: &[MultiAgentSettlementRecord],
) -> String {
    let mut lines = vec![
        render_multi_agent_route_snapshot(routed_cases),
        String::new(),
        "## Settlement Snapshot".to_string(),
    ];
    lines.extend(render_multi_agent_settlement_snapshot_lines(
        settlement_records,
    ));
    lines.join("\n")
}

fn render_multi_agent_route_snapshot(routed_cases: &[MultiAgentRoutedCase]) -> String {
    if routed_cases.is_empty() {
        return "# Tau Multi-agent Route Snapshot\n\n- No routed cases yet".to_string();
    }

    let mut lines = vec![
        "# Tau Multi-agent Route Snapshot".to_string(),
        String::new(),
    ];
    for routed in routed_cases {
        let category = if routed.category.trim().is_empty() {
            "none".to_string()
        } else {
            routed.category.clone()
        };
        lines.push(format!(
            "- {} phase={} selected_role={} category={}",
            routed.case_id, routed.phase, routed.selected_role, category
        ));
    }
    lines.join("\n")
}

fn render_multi_agent_settlement_snapshot_lines(
    settlement_records: &[MultiAgentSettlementRecord],
) -> Vec<String> {
    if settlement_records.is_empty() {
        return vec!["- No settlement records yet".to_string()];
    }

    settlement_records
        .iter()
        .map(|record| {
            format!(
                "- {} phase={} escrow_state={} reserved_micros={} released_micros={} refunded_micros={} execution_cost_micros={} settlement_reference={}",
                record.case_id,
                record.phase,
                record.escrow_state.as_str(),
                record.reserved_micros,
                record.released_micros,
                record.refunded_micros,
                record.execution_cost_micros,
                record.settlement_reference
            )
        })
        .collect()
}

fn load_multi_agent_runtime_state(path: &Path) -> Result<MultiAgentRuntimeState> {
    if !path.exists() {
        return Ok(MultiAgentRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let state = serde_json::from_str::<MultiAgentRuntimeState>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if state.schema_version != MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION {
        bail!(
            "unsupported multi-agent runtime state schema_version {} (expected {})",
            state.schema_version,
            MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION
        );
    }
    Ok(state)
}

fn save_multi_agent_runtime_state(path: &Path, state: &MultiAgentRuntimeState) -> Result<()> {
    let rendered = serde_json::to_string_pretty(state).context("serialize multi-agent state")?;
    write_text_atomic(path, &rendered)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_multi_agent_runtime_state, normalize_settlement_reference, retry_delay_ms,
        save_multi_agent_runtime_state, MultiAgentEscrowState, MultiAgentRuntime,
        MultiAgentRuntimeConfig, MultiAgentRuntimeState, MultiAgentRuntimeSummary,
        MultiAgentSettlementRecord, MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE,
        MULTI_AGENT_RUNTIME_SETTLEMENT_AUDIT_LOG_FILE,
        MULTI_AGENT_RUNTIME_SETTLEMENT_DIAGNOSTICS_FILE,
    };
    use crate::multi_agent_contract::{
        evaluate_multi_agent_case, load_multi_agent_contract_fixture,
        parse_multi_agent_contract_fixture,
    };
    use tau_runtime::channel_store::ChannelStore;
    use tau_runtime::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-agent-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> MultiAgentRuntimeConfig {
        MultiAgentRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/multi-agent"),
            queue_limit: 64,
            processed_case_cap: 10_000,
            retry_max_attempts: 2,
            retry_base_delay_ms: 0,
        }
    }

    #[test]
    fn unit_retry_delay_ms_scales_with_attempt_number() {
        assert_eq!(retry_delay_ms(0, 1), 0);
        assert_eq!(retry_delay_ms(10, 1), 10);
        assert_eq!(retry_delay_ms(10, 2), 20);
        assert_eq!(retry_delay_ms(10, 3), 40);
    }

    #[test]
    fn unit_normalize_settlement_reference_defaults_to_case_key() {
        let fixture = parse_multi_agent_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "planner-case",
      "phase": "planner",
      "route_table": {
        "schema_version": 1,
        "roles": { "planner": {} },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "planner" }
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#,
        )
        .expect("fixture");
        let reference = normalize_settlement_reference(&fixture.cases[0]);
        assert_eq!(reference, "kamn://settlement/planner:planner-case");
    }

    #[test]
    fn unit_finalize_success_settlement_transitions_reserved_to_settled() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = parse_multi_agent_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "planner-case",
      "phase": "planner",
      "route_table": {
        "schema_version": 1,
        "roles": { "planner": {} },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "planner" }
      },
      "economics": {
        "escrow_reserve_micros": 9000,
        "execution_cost_micros": 3000
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#,
        )
        .expect("fixture");
        let case = fixture.cases[0].clone();
        let case_key = format!("{}:{}", case.phase.as_str(), case.case_id.trim());

        let mut runtime = MultiAgentRuntime::new(config).expect("runtime");
        let mut summary = MultiAgentRuntimeSummary::default();
        runtime
            .ensure_case_escrow_reservation(&case, &case_key, &mut summary)
            .expect("reserve");
        let result = evaluate_multi_agent_case(&case);
        let settlement = runtime
            .finalize_success_settlement(&case, &case_key, &result, &mut summary)
            .expect("settle success")
            .expect("settlement record");
        assert_eq!(settlement.escrow_state, MultiAgentEscrowState::Settled);
        assert_eq!(settlement.reserved_micros, 9000);
        assert_eq!(settlement.released_micros, 3000);
        assert_eq!(settlement.refunded_micros, 6000);
    }

    #[tokio::test]
    async fn functional_runner_processes_fixture_and_persists_route_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.routed_cases_upserted, 1);
        assert_eq!(summary.duplicate_skips, 0);
        assert_eq!(summary.escrow_reserved_cases, 3);
        assert_eq!(summary.escrow_settled_cases, 1);
        assert_eq!(summary.escrow_refunded_cases, 3);
        assert_eq!(summary.settlement_timeout_refunds, 0);
        assert!(summary.settlement_audit_events >= 3);

        let state =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.routed_cases.len(), 1);
        assert_eq!(state.settlement_records.len(), 3);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log =
            std::fs::read_to_string(config.state_dir.join(MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE))
                .expect("events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("case_processing_failed"));
        assert!(events_log.contains("escrow_reserved"));
        assert!(events_log.contains("escrow_refunded"));

        let settlement_audit = std::fs::read_to_string(
            config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_SETTLEMENT_AUDIT_LOG_FILE),
        )
        .expect("settlement audit");
        assert!(settlement_audit.contains("\"event_kind\":\"reserve\""));
        assert!(settlement_audit.contains("\"event_kind\":\"settle_success\""));
        assert!(settlement_audit.contains("\"event_kind\":\"settle_failure\""));

        let settlement_diagnostics = std::fs::read_to_string(
            config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_SETTLEMENT_DIAGNOSTICS_FILE),
        )
        .expect("settlement diagnostics");
        assert!(settlement_diagnostics.contains("\"total_records\": 3"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "multi-agent",
            "orchestrator-router",
        )
        .expect("channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Multi-agent Route Snapshot"));
        assert!(memory.contains("Settlement Snapshot"));
        assert!(memory.contains("planner-success"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.failed_cases, 1);
        let state =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.health.queue_depth, 1);
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = MultiAgentRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);

        let mut second_runtime = MultiAgentRuntime::new(config).expect("second runtime");
        let second = second_runtime.run_once(&fixture).await.expect("second run");
        assert_eq!(second.duplicate_skips, 2);
        assert_eq!(second.applied_cases, 0);
        assert_eq!(second.malformed_cases, 0);
        assert_eq!(second.failed_cases, 1);
    }

    #[tokio::test]
    async fn integration_runner_persists_kamn_settlement_fixture_artifacts() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path("kamn-settlement.json");
        config.retry_max_attempts = 1;

        let fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.discovered_cases, 2);
        assert_eq!(summary.escrow_reserved_cases, 2);
        assert_eq!(summary.escrow_settled_cases, 1);
        assert_eq!(summary.escrow_refunded_cases, 2);

        let state =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.settlement_records.len(), 2);
        let planner = state
            .settlement_records
            .iter()
            .find(|record| record.case_id == "kamn-planner-success")
            .expect("planner settlement");
        assert_eq!(planner.escrow_state, MultiAgentEscrowState::Settled);
        assert_eq!(
            planner.settlement_reference,
            "kamn://escrow/root-alpha/planner-success"
        );
        assert_eq!(planner.reserved_micros, 12_000);
        assert_eq!(planner.execution_cost_micros, 7_000);
        assert_eq!(planner.refunded_micros, 5_000);

        let retryable = state
            .settlement_records
            .iter()
            .find(|record| record.case_id == "kamn-delegated-retryable")
            .expect("retryable settlement");
        assert_eq!(retryable.escrow_state, MultiAgentEscrowState::Refunded);
        assert_eq!(
            retryable.settlement_reference,
            "kamn://escrow/root-alpha/delegated-retryable"
        );
        assert_eq!(retryable.refunded_micros, 9_000);

        let settlement_audit = std::fs::read_to_string(
            config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_SETTLEMENT_AUDIT_LOG_FILE),
        )
        .expect("read settlement audit");
        assert!(settlement_audit.contains("kamn://escrow/root-alpha/planner-success"));
        assert!(settlement_audit.contains("kamn://escrow/root-alpha/delegated-retryable"));
    }

    #[tokio::test]
    async fn regression_runner_rejects_contract_drift_between_expected_and_runtime_result() {
        let temp = tempdir().expect("tempdir");
        let mut fixture = load_multi_agent_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture");
        fixture.cases[0].expected.selected_role = "reviewer".to_string();
        fixture.cases[0].expected.attempted_roles =
            vec!["reviewer".to_string(), "planner-fallback".to_string()];

        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize fixture"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let error =
            load_multi_agent_contract_fixture(&config.fixture_path).expect_err("drift should fail");
        assert!(format!("{error:#}").contains("expected.selected_role"));
    }

    #[tokio::test]
    async fn regression_runner_failure_streak_resets_after_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 1;

        let failing_fixture = parse_multi_agent_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "retry-only-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "delegated-retryable",
      "phase": "delegated_step",
      "route_table": {
        "schema_version": 1,
        "roles": {
          "planner": {},
          "executor": {},
          "reviewer": {}
        },
        "planner": { "role": "planner" },
        "delegated": { "role": "executor", "fallback_roles": ["reviewer"] },
        "review": { "role": "reviewer" }
      },
      "step_text": "triage hotfix failures",
      "simulate_retryable_failure": true,
      "expected": {
        "outcome": "retryable_failure",
        "error_code": "multi_agent_role_unavailable"
      }
    }
  ]
}"#,
        )
        .expect("parse failing fixture");
        let success_fixture = parse_multi_agent_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "planner-success",
      "phase": "planner",
      "route_table": {
        "schema_version": 1,
        "roles": {
          "planner": {},
          "reviewer": {}
        },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "reviewer" }
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#,
        )
        .expect("parse success fixture");

        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let failed = runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let success = runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 1);
        let state_after_success =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }

    #[tokio::test]
    async fn regression_runner_prevents_double_settlement_for_repeated_failure_case() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path("kamn-settlement.json");
        config.retry_max_attempts = 1;
        let fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = MultiAgentRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.failed_cases, 1);

        let mut second_runtime = MultiAgentRuntime::new(config.clone()).expect("second runtime");
        let second = second_runtime.run_once(&fixture).await.expect("second run");
        assert_eq!(second.failed_cases, 1);
        assert!(second.settlement_idempotent_skips >= 1);

        let state =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        let retryable = state
            .settlement_records
            .iter()
            .find(|record| record.case_id == "kamn-delegated-retryable")
            .expect("retryable settlement");
        assert_eq!(retryable.escrow_state, MultiAgentEscrowState::Refunded);
        assert_eq!(retryable.refunded_micros, 9_000);

        let settlement_audit = std::fs::read_to_string(
            config
                .state_dir
                .join(MULTI_AGENT_RUNTIME_SETTLEMENT_AUDIT_LOG_FILE),
        )
        .expect("read settlement audit");
        let settle_failure_count = settlement_audit
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid json"))
            .filter(|line| {
                line["case_id"].as_str() == Some("kamn-delegated-retryable")
                    && line["event_kind"].as_str() == Some("settle_failure")
            })
            .count();
        assert_eq!(settle_failure_count, 1);
    }

    #[tokio::test]
    async fn regression_runner_refunds_timed_out_reserved_escrow_records() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        let fixture = parse_multi_agent_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success-timeout-check",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "planner-success",
      "phase": "planner",
      "route_table": {
        "schema_version": 1,
        "roles": { "planner": {} },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "planner" }
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");
        config.fixture_path = temp.path().join("timeout-fixture.json");
        std::fs::write(
            &config.fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize fixture"),
        )
        .expect("write fixture");

        let stale_timestamp = 1;
        let seeded_state = MultiAgentRuntimeState {
            processed_case_keys: vec!["planner:planner-success".to_string()],
            settlement_records: vec![MultiAgentSettlementRecord {
                case_key: "planner:planner-success".to_string(),
                case_id: "planner-success".to_string(),
                phase: "planner".to_string(),
                settlement_reference: "kamn://escrow/stale/planner-success".to_string(),
                escrow_state: MultiAgentEscrowState::Reserved,
                reserved_micros: 10_000,
                released_micros: 0,
                refunded_micros: 0,
                execution_cost_micros: 0,
                finalized_error_code: String::new(),
                reserved_unix_ms: stale_timestamp,
                finalized_unix_ms: None,
                updated_unix_ms: stale_timestamp,
                artifacts: Vec::new(),
            }],
            ..MultiAgentRuntimeState::default()
        };
        save_multi_agent_runtime_state(&config.state_dir.join("state.json"), &seeded_state)
            .expect("seed state");

        let loaded_fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&loaded_fixture).await.expect("run once");
        assert_eq!(summary.settlement_timeout_refunds, 1);

        let state =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        let timed_out = state
            .settlement_records
            .iter()
            .find(|record| record.case_id == "planner-success")
            .expect("timed out settlement");
        assert_eq!(timed_out.escrow_state, MultiAgentEscrowState::Refunded);
        assert_eq!(timed_out.finalized_error_code, "escrow_timeout");
    }

    #[tokio::test]
    async fn regression_runner_events_log_contains_reason_codes_for_success_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        let fixture = parse_multi_agent_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "review-success",
      "phase": "review",
      "route_table": {
        "schema_version": 1,
        "roles": {
          "planner": {},
          "reviewer": {}
        },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "reviewer" }
      },
      "expected": {
        "outcome": "success",
        "selected_role": "reviewer",
        "attempted_roles": ["reviewer"]
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");
        config.fixture_path = temp.path().join("success-fixture.json");
        std::fs::write(
            &config.fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize fixture"),
        )
        .expect("write fixture");

        let loaded_fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&loaded_fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 0);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE))
                .expect("read events");
        let parsed = events_log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 1);
        let reason_codes = parsed[0]["reason_codes"]
            .as_array()
            .expect("reason codes array");
        assert!(reason_codes
            .iter()
            .any(|value| value.as_str() == Some("routed_cases_updated")));
    }

    #[tokio::test]
    async fn regression_runner_events_log_emits_degraded_state_for_failed_cycle() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_multi_agent_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MultiAgentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 1);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE))
                .expect("read events");
        let parsed = events_log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["health_state"].as_str(), Some("degraded"));
        assert!(parsed[0]["health_reason"]
            .as_str()
            .unwrap_or_default()
            .contains("recent transport failures observed"));
        let reason_codes = parsed[0]["reason_codes"]
            .as_array()
            .expect("reason codes array");
        assert!(reason_codes
            .iter()
            .any(|value| value.as_str() == Some("retryable_failures_observed")));
        assert!(reason_codes
            .iter()
            .any(|value| value.as_str() == Some("case_processing_failed")));
    }
}
