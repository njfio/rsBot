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
}

impl Default for GatewayRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: GATEWAY_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            requests: Vec::new(),
            health: TransportHealthSnapshot::default(),
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
        self.state.health = health.clone();

        save_gateway_runtime_state(&self.state_path(), &self.state)?;
        append_gateway_cycle_report(
            &self.config.state_dir.join(GATEWAY_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
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
        load_gateway_runtime_state, retry_delay_ms, GatewayRuntime, GatewayRuntimeConfig,
        GATEWAY_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::channel_store::ChannelStore;
    use crate::gateway_contract::{load_gateway_contract_fixture, parse_gateway_contract_fixture};
    use crate::transport_health::TransportHealthState;

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
        }
    }

    #[test]
    fn unit_retry_delay_ms_scales_with_attempt_number() {
        assert_eq!(retry_delay_ms(0, 1), 0);
        assert_eq!(retry_delay_ms(10, 1), 10);
        assert_eq!(retry_delay_ms(10, 2), 20);
        assert_eq!(retry_delay_ms(10, 3), 40);
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
