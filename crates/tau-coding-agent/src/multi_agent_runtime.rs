use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use crate::{current_unix_timestamp_ms, write_text_atomic, TransportHealthSnapshot};
use tau_orchestrator::multi_agent_contract::{
    evaluate_multi_agent_case, load_multi_agent_contract_fixture,
    validate_multi_agent_case_result_against_contract, MultiAgentContractCase,
    MultiAgentContractFixture, MultiAgentReplayStep,
};

const MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn multi_agent_runtime_state_schema_version() -> u32 {
    MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub(crate) struct MultiAgentRuntimeConfig {
    pub(crate) fixture_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) queue_limit: usize,
    pub(crate) processed_case_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MultiAgentRuntimeSummary {
    pub(crate) discovered_cases: usize,
    pub(crate) queued_cases: usize,
    pub(crate) applied_cases: usize,
    pub(crate) duplicate_skips: usize,
    pub(crate) malformed_cases: usize,
    pub(crate) retryable_failures: usize,
    pub(crate) retry_attempts: usize,
    pub(crate) failed_cases: usize,
    pub(crate) routed_cases_upserted: usize,
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
    backlog_cases: usize,
    failure_streak: usize,
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
    health: TransportHealthSnapshot,
}

impl Default for MultiAgentRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: MULTI_AGENT_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            routed_cases: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub(crate) async fn run_multi_agent_contract_runner(config: MultiAgentRuntimeConfig) -> Result<()> {
    let fixture = load_multi_agent_contract_fixture(&config.fixture_path)?;
    let mut runtime = MultiAgentRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "multi-agent runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} routed_cases_upserted={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.routed_cases_upserted
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

            let mut attempt = 1usize;
            loop {
                let result = evaluate_multi_agent_case(&case);
                validate_multi_agent_case_result_against_contract(&case, &result)?;
                match result.step {
                    MultiAgentReplayStep::Success => {
                        let upserted = self.persist_success_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.routed_cases_upserted =
                            summary.routed_cases_upserted.saturating_add(upserted);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    MultiAgentReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(&case, &case_key, &result)?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    MultiAgentReplayStep::RetryableFailure => {
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

    fn persist_success_result(
        &mut self,
        case: &MultiAgentContractCase,
        case_key: &str,
        result: &tau_orchestrator::multi_agent_contract::MultiAgentReplayResult,
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
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "multi-agent case {} applied with selected_role={} phase={}",
                case.case_id,
                result.selected_role,
                case.phase.as_str()
            ),
        })?;
        store.write_memory(&render_multi_agent_route_snapshot(&self.state.routed_cases))?;
        Ok(1)
    }

    fn persist_non_success_result(
        &self,
        case: &MultiAgentContractCase,
        case_key: &str,
        result: &tau_orchestrator::multi_agent_contract::MultiAgentReplayResult,
    ) -> Result<()> {
        let store = self.channel_store()?;
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let outcome = match result.step {
            MultiAgentReplayStep::Success => "success",
            MultiAgentReplayStep::MalformedInput => "malformed_input",
            MultiAgentReplayStep::RetryableFailure => "retryable_failure",
        };
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
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "multi-agent case {} outcome={} error_code={}",
                case.case_id,
                outcome,
                result.error_code.clone().unwrap_or_default()
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
        load_multi_agent_runtime_state, retry_delay_ms, MultiAgentRuntime, MultiAgentRuntimeConfig,
        MULTI_AGENT_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::channel_store::ChannelStore;
    use crate::transport_health::TransportHealthState;
    use tau_orchestrator::multi_agent_contract::{
        load_multi_agent_contract_fixture, parse_multi_agent_contract_fixture,
    };

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

        let state =
            load_multi_agent_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.routed_cases.len(), 1);
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
