use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use crate::gateway_contract::{
    evaluate_gateway_case, load_gateway_contract_fixture,
    validate_gateway_case_result_against_contract, GatewayContractCase, GatewayContractFixture,
    GatewayReplayResult, GatewayReplayStep,
};
use crate::{current_unix_timestamp_ms, write_text_atomic, TransportHealthSnapshot};

const GATEWAY_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const GATEWAY_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const DEFAULT_GATEWAY_GUARDRAIL_FAILURE_STREAK_THRESHOLD: usize = 2;
const DEFAULT_GATEWAY_GUARDRAIL_RETRYABLE_FAILURES_THRESHOLD: usize = 2;
const GATEWAY_SERVICE_STATUS_RUNNING: &str = "running";
const GATEWAY_SERVICE_STATUS_STOPPED: &str = "stopped";
const DEFAULT_GATEWAY_SERVICE_STOP_REASON: &str = "operator_requested";

fn gateway_runtime_state_schema_version() -> u32 {
    GATEWAY_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub(crate) struct GatewayRuntimeConfig {
    pub(crate) fixture_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) queue_limit: usize,
    pub(crate) processed_case_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
    pub(crate) guardrail_failure_streak_threshold: usize,
    pub(crate) guardrail_retryable_failures_threshold: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GatewayRuntimeSummary {
    pub(crate) discovered_cases: usize,
    pub(crate) queued_cases: usize,
    pub(crate) applied_cases: usize,
    pub(crate) duplicate_skips: usize,
    pub(crate) malformed_cases: usize,
    pub(crate) retryable_failures: usize,
    pub(crate) retry_attempts: usize,
    pub(crate) failed_cases: usize,
    pub(crate) upserted_requests: usize,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayRuntimeCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    rollout_gate: String,
    rollout_reason_code: String,
    guardrail_failure_streak_threshold: usize,
    guardrail_retryable_failures_threshold: usize,
    reason_codes: Vec<String>,
    discovered_cases: usize,
    queued_cases: usize,
    applied_cases: usize,
    duplicate_skips: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    retry_attempts: usize,
    failed_cases: usize,
    upserted_requests: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GatewayRolloutGuardrailState {
    gate: String,
    reason_code: String,
    failure_streak_threshold: usize,
    retryable_failures_threshold: usize,
    failure_streak: usize,
    last_failed_cases: usize,
    last_retryable_failures: usize,
    updated_unix_ms: u64,
}

impl Default for GatewayRolloutGuardrailState {
    fn default() -> Self {
        Self {
            gate: "pass".to_string(),
            reason_code: "guardrail_checks_passing".to_string(),
            failure_streak_threshold: DEFAULT_GATEWAY_GUARDRAIL_FAILURE_STREAK_THRESHOLD,
            retryable_failures_threshold: DEFAULT_GATEWAY_GUARDRAIL_RETRYABLE_FAILURES_THRESHOLD,
            failure_streak: 0,
            last_failed_cases: 0,
            last_retryable_failures: 0,
            updated_unix_ms: current_unix_timestamp_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GatewayServiceLifecycleState {
    status: String,
    startup_attempts: u64,
    startup_failure_streak: usize,
    last_startup_error: String,
    last_started_unix_ms: u64,
    last_stopped_unix_ms: u64,
    last_transition_unix_ms: u64,
    last_stop_reason: String,
}

impl Default for GatewayServiceLifecycleState {
    fn default() -> Self {
        let now = current_unix_timestamp_ms();
        Self {
            status: GATEWAY_SERVICE_STATUS_RUNNING.to_string(),
            startup_attempts: 0,
            startup_failure_streak: 0,
            last_startup_error: String::new(),
            last_started_unix_ms: now,
            last_stopped_unix_ms: 0,
            last_transition_unix_ms: now,
            last_stop_reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct GatewayServiceStatusReport {
    pub(crate) state_path: String,
    pub(crate) service_status: String,
    pub(crate) rollout_gate: String,
    pub(crate) rollout_reason_code: String,
    pub(crate) guardrail_gate: String,
    pub(crate) guardrail_reason_code: String,
    pub(crate) service_startup_attempts: u64,
    pub(crate) service_startup_failure_streak: usize,
    pub(crate) service_last_startup_error: String,
    pub(crate) service_last_started_unix_ms: u64,
    pub(crate) service_last_stopped_unix_ms: u64,
    pub(crate) service_last_transition_unix_ms: u64,
    pub(crate) service_last_stop_reason: String,
    pub(crate) health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct GatewayRequestRecord {
    case_key: String,
    case_id: String,
    method: String,
    endpoint: String,
    actor_id: String,
    status_code: u16,
    outcome: String,
    error_code: String,
    response_body: serde_json::Value,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayRuntimeState {
    #[serde(default = "gateway_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    requests: Vec<GatewayRequestRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
    #[serde(default)]
    guardrail: GatewayRolloutGuardrailState,
    #[serde(default)]
    service: GatewayServiceLifecycleState,
}

impl Default for GatewayRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: GATEWAY_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            requests: Vec::new(),
            health: TransportHealthSnapshot::default(),
            guardrail: GatewayRolloutGuardrailState::default(),
            service: GatewayServiceLifecycleState::default(),
        }
    }
}

pub(crate) async fn run_gateway_contract_runner(config: GatewayRuntimeConfig) -> Result<()> {
    let fixture = load_gateway_contract_fixture(&config.fixture_path)?;
    let mut runtime = GatewayRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "gateway runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} upserted_requests={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.upserted_requests
    );
    println!(
        "gateway runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

pub(crate) fn start_gateway_service_mode(state_dir: &Path) -> Result<GatewayServiceStatusReport> {
    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    let state_path = gateway_state_path(state_dir);
    let mut state = load_gateway_runtime_state(&state_path)?;
    normalize_gateway_service_state(&mut state.service);

    let now = current_unix_timestamp_ms();
    state.service.status = GATEWAY_SERVICE_STATUS_RUNNING.to_string();
    state.service.startup_attempts = state.service.startup_attempts.saturating_add(1);
    state.service.startup_failure_streak = 0;
    state.service.last_startup_error.clear();
    state.service.last_started_unix_ms = now;
    state.service.last_transition_unix_ms = now;

    save_gateway_runtime_state(&state_path, &state)?;
    Ok(build_gateway_service_status_report(&state_path, &state))
}

pub(crate) fn stop_gateway_service_mode(
    state_dir: &Path,
    stop_reason: Option<&str>,
) -> Result<GatewayServiceStatusReport> {
    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    let state_path = gateway_state_path(state_dir);
    let mut state = load_gateway_runtime_state(&state_path)?;
    normalize_gateway_service_state(&mut state.service);

    let now = current_unix_timestamp_ms();
    state.service.status = GATEWAY_SERVICE_STATUS_STOPPED.to_string();
    state.service.last_stopped_unix_ms = now;
    state.service.last_transition_unix_ms = now;
    state.service.last_stop_reason = normalized_gateway_stop_reason(stop_reason);

    save_gateway_runtime_state(&state_path, &state)?;
    Ok(build_gateway_service_status_report(&state_path, &state))
}

pub(crate) fn inspect_gateway_service_mode(state_dir: &Path) -> Result<GatewayServiceStatusReport> {
    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    let state_path = gateway_state_path(state_dir);
    let mut state = load_gateway_runtime_state(&state_path)?;
    normalize_gateway_service_state(&mut state.service);
    Ok(build_gateway_service_status_report(&state_path, &state))
}

pub(crate) fn render_gateway_service_status_report(report: &GatewayServiceStatusReport) -> String {
    format!(
        "gateway service status: state_path={} service_status={} rollout_gate={} rollout_reason_code={} guardrail_gate={} guardrail_reason_code={} startup_attempts={} startup_failure_streak={} last_startup_error={} last_started_unix_ms={} last_stopped_unix_ms={} last_transition_unix_ms={} last_stop_reason={} queue_depth={} failure_streak={} last_cycle_failed={} last_cycle_completed={}",
        report.state_path,
        report.service_status,
        report.rollout_gate,
        report.rollout_reason_code,
        report.guardrail_gate,
        report.guardrail_reason_code,
        report.service_startup_attempts,
        report.service_startup_failure_streak,
        if report.service_last_startup_error.trim().is_empty() {
            "none".to_string()
        } else {
            report.service_last_startup_error.clone()
        },
        report.service_last_started_unix_ms,
        report.service_last_stopped_unix_ms,
        report.service_last_transition_unix_ms,
        if report.service_last_stop_reason.trim().is_empty() {
            "none".to_string()
        } else {
            report.service_last_stop_reason.clone()
        },
        report.health.queue_depth,
        report.health.failure_streak,
        report.health.last_cycle_failed,
        report.health.last_cycle_completed
    )
}

struct GatewayRuntime {
    config: GatewayRuntimeConfig,
    state: GatewayRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl GatewayRuntime {
    fn new(config: GatewayRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_gateway_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .requests
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));
        normalize_gateway_service_state(&mut state.service);

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
        fixture: &GatewayContractFixture,
    ) -> Result<GatewayRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = GatewayRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..GatewayRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.method.cmp(&right.method))
                .then_with(|| left.endpoint.cmp(&right.endpoint))
        });
        queued_cases.truncate(self.config.queue_limit);
        summary.queued_cases = queued_cases.len();

        for case in queued_cases {
            let case_key = case_runtime_key(&case);
            if self.processed_case_keys.contains(&case_key) {
                summary.duplicate_skips = summary.duplicate_skips.saturating_add(1);
                continue;
            }

            let mut attempt = 1usize;
            loop {
                let result = evaluate_gateway_case(&case);
                validate_gateway_case_result_against_contract(&case, &result)?;
                match result.step {
                    GatewayReplayStep::Success => {
                        let upserted = self.persist_success_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.upserted_requests =
                            summary.upserted_requests.saturating_add(upserted);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    GatewayReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(&case, &case_key, &result)?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    GatewayReplayStep::RetryableFailure => {
                        summary.retryable_failures = summary.retryable_failures.saturating_add(1);
                        if attempt >= self.config.retry_max_attempts {
                            summary.failed_cases = summary.failed_cases.saturating_add(1);
                            self.persist_non_success_result(&case, &case_key, &result)?;
                            break;
                        }
                        summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                        apply_retry_delay(self.config.retry_base_delay_ms, attempt).await;
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
        let guardrail = evaluate_gateway_rollout_guardrail(
            &summary,
            &health,
            self.config.guardrail_failure_streak_threshold,
            self.config.guardrail_retryable_failures_threshold,
        );
        self.state.health = health.clone();
        self.state.guardrail = guardrail.clone();

        save_gateway_runtime_state(&self.state_path(), &self.state)?;
        append_gateway_cycle_report(
            &self.config.state_dir.join(GATEWAY_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &guardrail,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn persist_success_result(
        &mut self,
        case: &GatewayContractCase,
        case_key: &str,
        result: &GatewayReplayResult,
    ) -> Result<usize> {
        let record = GatewayRequestRecord {
            case_key: case_key.to_string(),
            case_id: case.case_id.clone(),
            method: case.method.trim().to_ascii_uppercase(),
            endpoint: case.endpoint.trim().to_string(),
            actor_id: case.actor_id.trim().to_string(),
            status_code: result.status_code,
            outcome: outcome_name(result.step).to_string(),
            error_code: String::new(),
            response_body: result.response_body.clone(),
            updated_unix_ms: current_unix_timestamp_ms(),
        };

        if let Some(existing) = self
            .state
            .requests
            .iter_mut()
            .find(|existing| existing.case_key == record.case_key)
        {
            *existing = record;
        } else {
            self.state.requests.push(record);
        }
        self.state
            .requests
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));

        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-gateway-runner".to_string(),
                payload: json!({
                    "outcome": "success",
                    "case_id": case.case_id,
                    "method": case.method.trim().to_ascii_uppercase(),
                    "endpoint": case.endpoint.trim(),
                    "status_code": result.status_code,
                    "response_body": result.response_body,
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "gateway case {} applied method={} endpoint={} status={}",
                    case.case_id,
                    case.method.trim().to_ascii_uppercase(),
                    case.endpoint.trim(),
                    result.status_code
                ),
            })?;
            store.write_memory(&render_gateway_snapshot(
                &self.state.requests,
                case.actor_id.trim(),
            ))?;
        }

        Ok(1)
    }

    fn persist_non_success_result(
        &self,
        case: &GatewayContractCase,
        case_key: &str,
        result: &GatewayReplayResult,
    ) -> Result<()> {
        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            let outcome = outcome_name(result.step);
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-gateway-runner".to_string(),
                payload: json!({
                    "outcome": outcome,
                    "case_id": case.case_id,
                    "method": case.method.trim().to_ascii_uppercase(),
                    "endpoint": case.endpoint.trim(),
                    "status_code": result.status_code,
                    "error_code": result.error_code.clone().unwrap_or_default(),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "gateway case {} outcome={} error_code={} status={}",
                    case.case_id,
                    outcome,
                    result.error_code.clone().unwrap_or_default(),
                    result.status_code
                ),
            })?;
        }
        Ok(())
    }

    fn scope_channel_store(&self, case: &GatewayContractCase) -> Result<Option<ChannelStore>> {
        let actor_id = case.actor_id.trim();
        if actor_id.is_empty() {
            return Ok(None);
        }
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "gateway",
            actor_id,
        )?;
        Ok(Some(store))
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

fn gateway_state_path(state_dir: &Path) -> PathBuf {
    state_dir.join("state.json")
}

fn normalized_gateway_stop_reason(raw: Option<&str>) -> String {
    let trimmed = raw.unwrap_or_default().trim();
    if trimmed.is_empty() {
        DEFAULT_GATEWAY_SERVICE_STOP_REASON.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalized_gateway_service_status(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        GATEWAY_SERVICE_STATUS_STOPPED => GATEWAY_SERVICE_STATUS_STOPPED,
        _ => GATEWAY_SERVICE_STATUS_RUNNING,
    }
}

fn normalize_gateway_service_state(service: &mut GatewayServiceLifecycleState) {
    let normalized = normalized_gateway_service_status(&service.status);
    service.status = normalized.to_string();
    if normalized == GATEWAY_SERVICE_STATUS_RUNNING {
        service.startup_failure_streak = 0;
        service.last_startup_error = service.last_startup_error.trim().to_string();
    }
    if service.last_transition_unix_ms == 0 {
        service.last_transition_unix_ms = current_unix_timestamp_ms();
    }
    if normalized == GATEWAY_SERVICE_STATUS_RUNNING && service.last_started_unix_ms == 0 {
        service.last_started_unix_ms = service.last_transition_unix_ms;
    }
}

fn build_gateway_service_status_report(
    state_path: &Path,
    state: &GatewayRuntimeState,
) -> GatewayServiceStatusReport {
    let guardrail_gate = match state.guardrail.gate.trim().to_ascii_lowercase().as_str() {
        "hold" => "hold".to_string(),
        "pass" => "pass".to_string(),
        _ if state.health.classify().state.as_str() == "healthy" => "pass".to_string(),
        _ => "hold".to_string(),
    };
    let guardrail_reason_code = if !state.guardrail.reason_code.trim().is_empty() {
        state.guardrail.reason_code.trim().to_string()
    } else if guardrail_gate == "pass" {
        "guardrail_checks_passing".to_string()
    } else {
        "health_state_not_healthy".to_string()
    };

    let service_status = normalized_gateway_service_status(&state.service.status).to_string();
    let (rollout_gate, rollout_reason_code) = if service_status == GATEWAY_SERVICE_STATUS_STOPPED {
        ("hold".to_string(), "service_stopped".to_string())
    } else {
        (guardrail_gate.clone(), guardrail_reason_code.clone())
    };

    GatewayServiceStatusReport {
        state_path: state_path.display().to_string(),
        service_status,
        rollout_gate,
        rollout_reason_code,
        guardrail_gate,
        guardrail_reason_code,
        service_startup_attempts: state.service.startup_attempts,
        service_startup_failure_streak: state.service.startup_failure_streak,
        service_last_startup_error: state.service.last_startup_error.clone(),
        service_last_started_unix_ms: state.service.last_started_unix_ms,
        service_last_stopped_unix_ms: state.service.last_stopped_unix_ms,
        service_last_transition_unix_ms: state.service.last_transition_unix_ms,
        service_last_stop_reason: state.service.last_stop_reason.clone(),
        health: state.health.clone(),
    }
}

fn case_runtime_key(case: &GatewayContractCase) -> String {
    format!(
        "{}:{}:{}",
        case.method.trim().to_ascii_uppercase(),
        case.endpoint.trim(),
        case.case_id.trim()
    )
}

fn outcome_name(step: GatewayReplayStep) -> &'static str {
    match step {
        GatewayReplayStep::Success => "success",
        GatewayReplayStep::MalformedInput => "malformed_input",
        GatewayReplayStep::RetryableFailure => "retryable_failure",
    }
}

fn build_transport_health_snapshot(
    summary: &GatewayRuntimeSummary,
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

fn cycle_reason_codes(summary: &GatewayRuntimeSummary) -> Vec<String> {
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
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_gateway_cycle_report(
    path: &Path,
    summary: &GatewayRuntimeSummary,
    health: &TransportHealthSnapshot,
    guardrail: &GatewayRolloutGuardrailState,
    health_reason: &str,
    reason_codes: &[String],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = GatewayRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        rollout_gate: guardrail.gate.clone(),
        rollout_reason_code: guardrail.reason_code.clone(),
        guardrail_failure_streak_threshold: guardrail.failure_streak_threshold,
        guardrail_retryable_failures_threshold: guardrail.retryable_failures_threshold,
        reason_codes: reason_codes.to_vec(),
        discovered_cases: summary.discovered_cases,
        queued_cases: summary.queued_cases,
        applied_cases: summary.applied_cases,
        duplicate_skips: summary.duplicate_skips,
        malformed_cases: summary.malformed_cases,
        retryable_failures: summary.retryable_failures,
        retry_attempts: summary.retry_attempts,
        failed_cases: summary.failed_cases,
        upserted_requests: summary.upserted_requests,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize gateway runtime report")?;
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

fn evaluate_gateway_rollout_guardrail(
    summary: &GatewayRuntimeSummary,
    health: &TransportHealthSnapshot,
    failure_streak_threshold: usize,
    retryable_failures_threshold: usize,
) -> GatewayRolloutGuardrailState {
    let failure_streak_threshold = failure_streak_threshold.max(1);
    let retryable_failures_threshold = retryable_failures_threshold.max(1);
    let (gate, reason_code) =
        if health.failure_streak >= failure_streak_threshold && summary.failed_cases > 0 {
            ("hold", "failure_streak_threshold_exceeded")
        } else if summary.retryable_failures >= retryable_failures_threshold {
            ("hold", "retryable_failures_threshold_exceeded")
        } else if summary.malformed_cases > 0 {
            ("hold", "malformed_input_observed")
        } else {
            ("pass", "guardrail_checks_passing")
        };

    GatewayRolloutGuardrailState {
        gate: gate.to_string(),
        reason_code: reason_code.to_string(),
        failure_streak_threshold,
        retryable_failures_threshold,
        failure_streak: health.failure_streak,
        last_failed_cases: summary.failed_cases,
        last_retryable_failures: summary.retryable_failures,
        updated_unix_ms: current_unix_timestamp_ms(),
    }
}

fn render_gateway_snapshot(records: &[GatewayRequestRecord], actor_id: &str) -> String {
    let filtered = records
        .iter()
        .filter(|record| record.actor_id == actor_id)
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return format!("# Tau Gateway Snapshot ({actor_id})\n\n- No persisted requests");
    }

    let mut lines = vec![
        format!("# Tau Gateway Snapshot ({actor_id})"),
        String::new(),
    ];
    for record in filtered {
        lines.push(format!(
            "- {} {} {} status={} outcome={}",
            record.case_id, record.method, record.endpoint, record.status_code, record.outcome
        ));
    }
    lines.join("\n")
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
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    base_delay_ms.saturating_mul(1_u64 << exponent)
}

async fn apply_retry_delay(base_delay_ms: u64, attempt: usize) {
    let delay_ms = retry_delay_ms(base_delay_ms, attempt);
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

fn load_gateway_runtime_state(path: &Path) -> Result<GatewayRuntimeState> {
    if !path.exists() {
        return Ok(GatewayRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<GatewayRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "gateway runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(GatewayRuntimeState::default());
        }
    };
    if parsed.schema_version != GATEWAY_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "gateway runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(GatewayRuntimeState::default());
    }
    Ok(parsed)
}

fn save_gateway_runtime_state(path: &Path, state: &GatewayRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize gateway state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        evaluate_gateway_rollout_guardrail, inspect_gateway_service_mode,
        load_gateway_runtime_state, render_gateway_service_status_report, retry_delay_ms,
        start_gateway_service_mode, stop_gateway_service_mode, GatewayRuntime,
        GatewayRuntimeConfig, GatewayRuntimeSummary, GATEWAY_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::channel_store::ChannelStore;
    use crate::gateway_contract::{load_gateway_contract_fixture, parse_gateway_contract_fixture};
    use crate::transport_health::TransportHealthState;
    use crate::TransportHealthSnapshot;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("gateway-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> GatewayRuntimeConfig {
        GatewayRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/gateway"),
            queue_limit: 64,
            processed_case_cap: 10_000,
            retry_max_attempts: 2,
            retry_base_delay_ms: 0,
            guardrail_failure_streak_threshold: 2,
            guardrail_retryable_failures_threshold: 2,
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
    fn unit_guardrail_evaluator_respects_failure_and_retry_thresholds() {
        let summary = GatewayRuntimeSummary {
            retryable_failures: 1,
            failed_cases: 0,
            ..GatewayRuntimeSummary::default()
        };
        let health = TransportHealthSnapshot::default();
        let pass = evaluate_gateway_rollout_guardrail(&summary, &health, 2, 2);
        assert_eq!(pass.gate, "pass");
        assert_eq!(pass.reason_code, "guardrail_checks_passing");

        let hold_retryable = evaluate_gateway_rollout_guardrail(&summary, &health, 2, 1);
        assert_eq!(hold_retryable.gate, "hold");
        assert_eq!(
            hold_retryable.reason_code,
            "retryable_failures_threshold_exceeded"
        );

        let failed_summary = GatewayRuntimeSummary {
            failed_cases: 1,
            ..GatewayRuntimeSummary::default()
        };
        let failure_health = TransportHealthSnapshot {
            failure_streak: 2,
            ..TransportHealthSnapshot::default()
        };
        let hold_failure =
            evaluate_gateway_rollout_guardrail(&failed_summary, &failure_health, 2, 2);
        assert_eq!(hold_failure.gate, "hold");
        assert_eq!(
            hold_failure.reason_code,
            "failure_streak_threshold_exceeded"
        );
    }

    #[test]
    fn unit_service_mode_start_and_stop_emit_deterministic_reports() {
        let temp = tempdir().expect("tempdir");
        let state_dir = temp.path().join(".tau/gateway-service");

        let started = start_gateway_service_mode(&state_dir).expect("start service");
        assert_eq!(started.service_status, "running");
        assert_eq!(started.rollout_gate, "pass");
        assert_eq!(started.guardrail_gate, "pass");
        assert!(started.service_startup_attempts >= 1);

        let stopped = stop_gateway_service_mode(&state_dir, Some("maintenance_window"))
            .expect("stop service");
        assert_eq!(stopped.service_status, "stopped");
        assert_eq!(stopped.rollout_gate, "hold");
        assert_eq!(stopped.rollout_reason_code, "service_stopped");
        assert_eq!(stopped.service_last_stop_reason, "maintenance_window");
    }

    #[test]
    fn functional_service_mode_status_roundtrip_persists_lifecycle_state() {
        let temp = tempdir().expect("tempdir");
        let state_dir = temp.path().join(".tau/gateway-service");

        start_gateway_service_mode(&state_dir).expect("start service");
        stop_gateway_service_mode(&state_dir, Some("operator_requested")).expect("stop service");

        let report = inspect_gateway_service_mode(&state_dir).expect("inspect service");
        assert_eq!(report.service_status, "stopped");
        assert_eq!(report.rollout_gate, "hold");
        assert_eq!(report.service_last_stop_reason, "operator_requested");

        let rendered = render_gateway_service_status_report(&report);
        assert!(rendered.contains("gateway service status:"));
        assert!(rendered.contains("service_status=stopped"));
    }

    #[test]
    fn regression_service_mode_status_normalizes_legacy_state_without_service_block() {
        let temp = tempdir().expect("tempdir");
        let state_dir = temp.path().join(".tau/gateway-service");
        std::fs::create_dir_all(&state_dir).expect("create state dir");
        std::fs::write(
            state_dir.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 910,
    "cycle_duration_ms": 9,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 0,
    "last_cycle_processed": 0,
    "last_cycle_completed": 0,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write legacy state");

        let report = inspect_gateway_service_mode(&state_dir).expect("inspect service");
        assert_eq!(report.service_status, "running");
        assert_eq!(report.rollout_gate, "pass");
        assert_eq!(report.rollout_reason_code, "guardrail_checks_passing");
    }

    #[tokio::test]
    async fn functional_runner_processes_fixture_and_persists_gateway_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_gateway_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = GatewayRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.upserted_requests, 1);
        assert_eq!(summary.duplicate_skips, 0);

        let state =
            load_gateway_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.requests.len(), 1);
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );
        assert_eq!(state.guardrail.gate, "hold");
        assert_eq!(
            state.guardrail.reason_code,
            "retryable_failures_threshold_exceeded"
        );
        assert_eq!(state.guardrail.failure_streak_threshold, 2);
        assert_eq!(state.guardrail.retryable_failures_threshold, 2);
        assert_eq!(state.guardrail.last_failed_cases, 1);
        assert_eq!(state.guardrail.last_retryable_failures, 2);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(GATEWAY_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("case_processing_failed"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "gateway",
            "ops-bot",
        )
        .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Gateway Snapshot (ops-bot)"));
        assert!(memory.contains("gateway-success"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_gateway_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = GatewayRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.failed_cases, 1);
        let state =
            load_gateway_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert!(state.requests.is_empty());
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_gateway_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = GatewayRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);

        let mut second_runtime = GatewayRuntime::new(config).expect("second runtime");
        let second = second_runtime.run_once(&fixture).await.expect("second run");
        assert_eq!(second.duplicate_skips, 2);
        assert_eq!(second.applied_cases, 0);
        assert_eq!(second.malformed_cases, 0);
        assert_eq!(second.failed_cases, 1);
    }

    #[tokio::test]
    async fn regression_runner_rejects_contract_drift_between_expected_and_runtime_result() {
        let temp = tempdir().expect("tempdir");
        let mut fixture =
            load_gateway_contract_fixture(&fixture_path("mixed-outcomes.json")).expect("fixture");
        let success_case = fixture
            .cases
            .iter_mut()
            .find(|case| case.case_id == "gateway-success")
            .expect("success case");
        success_case.expected.response_body = json!({
            "status":"accepted",
            "method":"POST",
            "endpoint":"/v1/tasks",
            "actor_id":"incorrect"
        });
        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let mut runtime = GatewayRuntime::new(config).expect("runtime");
        let drift_fixture = load_gateway_contract_fixture(&runtime.config.fixture_path)
            .expect("fixture should load");
        let error = runtime
            .run_once(&drift_fixture)
            .await
            .expect_err("drift should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[tokio::test]
    async fn regression_runner_failure_streak_resets_after_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 1;
        config.guardrail_failure_streak_threshold = 1;

        let failing_fixture = parse_gateway_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "retry-only-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "gateway-retry-only",
      "method": "GET",
      "endpoint": "/v1/health",
      "actor_id": "ops-bot",
      "body": {},
      "simulate_retryable_failure": true,
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "gateway_backend_unavailable",
        "response_body": {"status":"retryable","reason":"backend_unavailable"}
      }
    }
  ]
}"#,
        )
        .expect("parse failing fixture");
        let success_fixture = parse_gateway_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "gateway-success-only",
      "method": "GET",
      "endpoint": "/v1/health",
      "actor_id": "ops-bot",
      "body": {},
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status":"accepted",
          "method":"GET",
          "endpoint":"/v1/health",
          "actor_id":"ops-bot"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse success fixture");

        let mut runtime = GatewayRuntime::new(config.clone()).expect("runtime");
        let failed = runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail = load_gateway_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state after fail");
        assert_eq!(state_after_fail.health.failure_streak, 1);
        assert_eq!(state_after_fail.guardrail.gate, "hold");
        assert_eq!(
            state_after_fail.guardrail.reason_code,
            "failure_streak_threshold_exceeded"
        );

        let success = runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 1);
        let state_after_success = load_gateway_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state after success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
        assert_eq!(state_after_success.guardrail.gate, "pass");
        assert_eq!(
            state_after_success.guardrail.reason_code,
            "guardrail_checks_passing"
        );
    }

    #[tokio::test]
    async fn regression_runner_events_log_contains_reason_codes_for_healthy_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        let fixture = parse_gateway_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "gateway-success-only",
      "method": "GET",
      "endpoint": "/v1/health",
      "actor_id": "ops-bot",
      "body": {},
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status":"accepted",
          "method":"GET",
          "endpoint":"/v1/health",
          "actor_id":"ops-bot"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");

        config.fixture_path = temp.path().join("single-success.json");
        std::fs::write(
            &config.fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let loaded_fixture =
            load_gateway_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = GatewayRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&loaded_fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 0);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(GATEWAY_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        let parsed = events_log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["rollout_gate"].as_str(), Some("pass"));
        assert_eq!(
            parsed[0]["rollout_reason_code"].as_str(),
            Some("guardrail_checks_passing")
        );
        let reason_codes = parsed[0]["reason_codes"]
            .as_array()
            .expect("reason codes array");
        assert!(reason_codes
            .iter()
            .any(|value| value.as_str() == Some("healthy_cycle")));
    }

    #[tokio::test]
    async fn regression_runner_events_log_emits_degraded_state_for_failed_cycle() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_gateway_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = GatewayRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 1);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(GATEWAY_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        let parsed = events_log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["health_state"].as_str(), Some("degraded"));
        assert_eq!(parsed[0]["rollout_gate"].as_str(), Some("hold"));
        assert_eq!(
            parsed[0]["rollout_reason_code"].as_str(),
            Some("retryable_failures_threshold_exceeded")
        );
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
