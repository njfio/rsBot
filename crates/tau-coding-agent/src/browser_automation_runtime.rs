use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::browser_automation_contract::{
    assert_browser_automation_result_matches_expectation, evaluate_browser_automation_case,
    load_browser_automation_contract_fixture, BrowserAutomationContractCase,
    BrowserAutomationContractFixture, BrowserAutomationReplayResult, BrowserAutomationReplayStep,
    BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED, BROWSER_AUTOMATION_ERROR_TIMEOUT,
    BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION,
};
use crate::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use crate::{current_unix_timestamp_ms, write_text_atomic, TransportHealthSnapshot};

const BROWSER_AUTOMATION_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn browser_automation_runtime_state_schema_version() -> u32 {
    BROWSER_AUTOMATION_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub(crate) struct BrowserAutomationRuntimeConfig {
    pub(crate) fixture_path: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) queue_limit: usize,
    pub(crate) processed_case_cap: usize,
    pub(crate) retry_max_attempts: usize,
    pub(crate) retry_base_delay_ms: u64,
    pub(crate) action_timeout_ms: u64,
    pub(crate) max_actions_per_case: usize,
    pub(crate) allow_unsafe_actions: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BrowserAutomationRuntimeSummary {
    pub(crate) discovered_cases: usize,
    pub(crate) queued_cases: usize,
    pub(crate) applied_cases: usize,
    pub(crate) duplicate_skips: usize,
    pub(crate) malformed_cases: usize,
    pub(crate) retryable_failures: usize,
    pub(crate) retry_attempts: usize,
    pub(crate) failed_cases: usize,
    pub(crate) timeout_failures: usize,
    pub(crate) denied_unsafe_actions: usize,
    pub(crate) denied_action_limit: usize,
    pub(crate) persisted_results: usize,
}

#[derive(Debug, Clone, Serialize)]
struct BrowserAutomationRuntimeCycleReport {
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
    timeout_failures: usize,
    denied_unsafe_actions: usize,
    denied_action_limit: usize,
    persisted_results: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BrowserAutomationResultRecord {
    case_key: String,
    operation: String,
    status_code: u16,
    error_code: String,
    response_body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrowserAutomationRuntimeState {
    #[serde(default = "browser_automation_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    results: Vec<BrowserAutomationResultRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for BrowserAutomationRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: BROWSER_AUTOMATION_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            results: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub(crate) async fn run_browser_automation_contract_runner(
    config: BrowserAutomationRuntimeConfig,
) -> Result<()> {
    let fixture = load_browser_automation_contract_fixture(&config.fixture_path)?;
    let mut runtime = BrowserAutomationRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "browser automation runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} timeout_failures={} denied_unsafe_actions={} denied_action_limit={} persisted_results={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.timeout_failures,
        summary.denied_unsafe_actions,
        summary.denied_action_limit,
        summary.persisted_results,
    );
    println!(
        "browser automation runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct BrowserAutomationRuntime {
    config: BrowserAutomationRuntimeConfig,
    state: BrowserAutomationRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl BrowserAutomationRuntime {
    fn new(config: BrowserAutomationRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state =
            load_browser_automation_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .results
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
        fixture: &BrowserAutomationContractFixture,
    ) -> Result<BrowserAutomationRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = BrowserAutomationRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..BrowserAutomationRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.operation.cmp(&right.operation))
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
                let result = self.evaluate_case_with_policy(&case, &mut summary);
                assert_browser_automation_result_matches_expectation(&case, &result)?;
                match result.step {
                    BrowserAutomationReplayStep::Success => {
                        self.persist_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.persisted_results = summary.persisted_results.saturating_add(1);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    BrowserAutomationReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_result(&case, &case_key, &result)?;
                        summary.persisted_results = summary.persisted_results.saturating_add(1);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    BrowserAutomationReplayStep::RetryableFailure => {
                        summary.retryable_failures = summary.retryable_failures.saturating_add(1);
                        if result.error_code.as_deref() == Some(BROWSER_AUTOMATION_ERROR_TIMEOUT) {
                            summary.timeout_failures = summary.timeout_failures.saturating_add(1);
                        }
                        if attempt >= self.config.retry_max_attempts {
                            summary.failed_cases = summary.failed_cases.saturating_add(1);
                            self.persist_result(&case, &case_key, &result)?;
                            summary.persisted_results = summary.persisted_results.saturating_add(1);
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

        save_browser_automation_runtime_state(&self.state_path(), &self.state)?;
        append_browser_automation_cycle_report(
            &self
                .config
                .state_dir
                .join(BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn evaluate_case_with_policy(
        &self,
        case: &BrowserAutomationContractCase,
        summary: &mut BrowserAutomationRuntimeSummary,
    ) -> BrowserAutomationReplayResult {
        let operation = case.operation.trim().to_ascii_lowercase();
        if operation == "action" && case.action_repeat_count > self.config.max_actions_per_case {
            summary.denied_action_limit = summary.denied_action_limit.saturating_add(1);
            return BrowserAutomationReplayResult {
                step: BrowserAutomationReplayStep::MalformedInput,
                status_code: 429,
                error_code: Some(BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED.to_string()),
                response_body: json!({"status":"rejected","reason":"action_limit_exceeded"}),
            };
        }

        if !self.config.allow_unsafe_actions && case.unsafe_operation {
            summary.denied_unsafe_actions = summary.denied_unsafe_actions.saturating_add(1);
            return BrowserAutomationReplayResult {
                step: BrowserAutomationReplayStep::MalformedInput,
                status_code: 403,
                error_code: Some(BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION.to_string()),
                response_body: json!({"status":"rejected","reason":"unsafe_operation"}),
            };
        }

        let mut effective_case = case.clone();
        if self.config.allow_unsafe_actions {
            // The runtime policy can explicitly allow otherwise blocked unsafe actions.
            effective_case.unsafe_operation = false;
        }

        let result = evaluate_browser_automation_case(&effective_case);
        if operation == "action"
            && result.step == BrowserAutomationReplayStep::Success
            && effective_case.timeout_ms > self.config.action_timeout_ms
        {
            return BrowserAutomationReplayResult {
                step: BrowserAutomationReplayStep::RetryableFailure,
                status_code: 504,
                error_code: Some(BROWSER_AUTOMATION_ERROR_TIMEOUT.to_string()),
                response_body: json!({"status":"retryable","reason":"timeout"}),
            };
        }

        result
    }

    fn persist_result(
        &mut self,
        case: &BrowserAutomationContractCase,
        case_key: &str,
        result: &BrowserAutomationReplayResult,
    ) -> Result<()> {
        let record = BrowserAutomationResultRecord {
            case_key: case_key.to_string(),
            operation: case.operation.trim().to_ascii_lowercase(),
            status_code: result.status_code,
            error_code: result.error_code.clone().unwrap_or_default(),
            response_body: result.response_body.clone(),
        };

        if let Some(existing) = self
            .state
            .results
            .iter_mut()
            .find(|existing| existing.case_key == record.case_key)
        {
            *existing = record;
        } else {
            self.state.results.push(record);
        }
        self.state
            .results
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));

        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "browser-automation",
            "fixtures",
        )?;
        let timestamp_unix_ms = current_unix_timestamp_ms();
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms,
            direction: "system".to_string(),
            event_key: Some(case_key.to_string()),
            source: "tau-browser-automation-runner".to_string(),
            payload: json!({
                "case_id": case.case_id.trim(),
                "operation": case.operation.trim().to_ascii_lowercase(),
                "status_code": result.status_code,
                "error_code": result.error_code.clone().unwrap_or_default(),
                "response_body": result.response_body,
            }),
        })?;
        store.append_context_entry(&ChannelContextEntry {
            timestamp_unix_ms,
            role: "system".to_string(),
            text: format!(
                "browser automation case {} operation={} status={} error_code={}",
                case.case_id.trim(),
                case.operation.trim().to_ascii_lowercase(),
                result.status_code,
                result.error_code.as_deref().unwrap_or_default()
            ),
        })?;
        store.write_memory(&render_browser_automation_snapshot(&self.state.results))?;
        Ok(())
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

fn case_runtime_key(case: &BrowserAutomationContractCase) -> String {
    format!(
        "{}:{}:{}",
        case.operation.trim().to_ascii_lowercase(),
        case.action.trim().to_ascii_lowercase(),
        case.case_id.trim()
    )
}

fn build_transport_health_snapshot(
    summary: &BrowserAutomationRuntimeSummary,
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

fn cycle_reason_codes(summary: &BrowserAutomationRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.discovered_cases > summary.queued_cases {
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        codes.push("duplicate_cases_skipped".to_string());
    }
    if summary.denied_action_limit > 0 {
        codes.push("action_limit_guardrail_denied".to_string());
    }
    if summary.denied_unsafe_actions > 0 {
        codes.push("unsafe_actions_denied".to_string());
    }
    if summary.timeout_failures > 0 {
        codes.push("timeout_failures_observed".to_string());
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

fn append_browser_automation_cycle_report(
    path: &Path,
    summary: &BrowserAutomationRuntimeSummary,
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
    let payload = BrowserAutomationRuntimeCycleReport {
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
        timeout_failures: summary.timeout_failures,
        denied_unsafe_actions: summary.denied_unsafe_actions,
        denied_action_limit: summary.denied_action_limit,
        persisted_results: summary.persisted_results,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line =
        serde_json::to_string(&payload).context("serialize browser automation runtime report")?;
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

fn render_browser_automation_snapshot(records: &[BrowserAutomationResultRecord]) -> String {
    if records.is_empty() {
        return "# Tau Browser Automation Snapshot\n\n- No persisted results".to_string();
    }

    let mut lines = vec![
        "# Tau Browser Automation Snapshot".to_string(),
        String::new(),
    ];
    for record in records {
        lines.push(format!(
            "- {} op={} status={} error_code={}",
            record.case_key,
            record.operation,
            record.status_code,
            if record.error_code.is_empty() {
                "none"
            } else {
                record.error_code.as_str()
            }
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

fn load_browser_automation_runtime_state(path: &Path) -> Result<BrowserAutomationRuntimeState> {
    if !path.exists() {
        return Ok(BrowserAutomationRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<BrowserAutomationRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "browser automation runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(BrowserAutomationRuntimeState::default());
        }
    };
    if parsed.schema_version != BROWSER_AUTOMATION_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "browser automation runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(BrowserAutomationRuntimeState::default());
    }
    Ok(parsed)
}

fn save_browser_automation_runtime_state(
    path: &Path,
    state: &BrowserAutomationRuntimeState,
) -> Result<()> {
    let payload =
        serde_json::to_string_pretty(state).context("serialize browser automation state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_browser_automation_runtime_state, retry_delay_ms, BrowserAutomationRuntime,
        BrowserAutomationRuntimeConfig, BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::browser_automation_contract::{
        load_browser_automation_contract_fixture, parse_browser_automation_contract_fixture,
    };
    use crate::channel_store::ChannelStore;
    use crate::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("browser-automation-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> BrowserAutomationRuntimeConfig {
        BrowserAutomationRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/browser-automation"),
            queue_limit: 64,
            processed_case_cap: 10_000,
            retry_max_attempts: 2,
            retry_base_delay_ms: 0,
            action_timeout_ms: 4_000,
            max_actions_per_case: 4,
            allow_unsafe_actions: false,
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
    async fn functional_runner_processes_fixture_and_persists_browser_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = load_browser_automation_contract_fixture(&config.fixture_path)
            .expect("fixture should load");
        let mut runtime = BrowserAutomationRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.timeout_failures, 2);
        assert_eq!(summary.duplicate_skips, 0);

        let state = load_browser_automation_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.results.len(), 3);
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("timeout_failures_observed"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "browser-automation",
            "fixtures",
        )
        .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Browser Automation Snapshot"));
        assert!(memory.contains("navigate::navigate_home"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture = load_browser_automation_contract_fixture(&config.fixture_path)
            .expect("fixture should load");
        let mut runtime = BrowserAutomationRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.failed_cases, 1);

        let state = load_browser_automation_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.results.len(), 2);
        assert_eq!(state.health.queue_depth, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = load_browser_automation_contract_fixture(&config.fixture_path)
            .expect("fixture should load");

        let mut first_runtime =
            BrowserAutomationRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);
        assert_eq!(first.failed_cases, 1);

        let mut second_runtime = BrowserAutomationRuntime::new(config).expect("second runtime");
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
            load_browser_automation_contract_fixture(&fixture_path("mixed-outcomes.json"))
                .expect("fixture should load");
        let success_case = fixture
            .cases
            .iter_mut()
            .find(|case| case.case_id == "navigate_home")
            .expect("success case");
        success_case.expected.response_body = serde_json::json!({
            "status": "ok",
            "operation": "navigate",
            "url": "https://unexpected.example"
        });
        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let mut runtime = BrowserAutomationRuntime::new(config).expect("runtime");
        let drift_fixture = load_browser_automation_contract_fixture(&runtime.config.fixture_path)
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

        let failing_fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "retry-only-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "action-timeout-failure",
      "operation": "action",
      "action": "wait",
      "selector": "#ready",
      "timeout_ms": 1000,
      "simulate_timeout": true,
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 504,
        "error_code": "browser_automation_timeout",
        "response_body": {"status":"retryable","reason":"timeout"}
      }
    }
  ]
}"##,
        )
        .expect("parse failing fixture");
        let success_fixture = parse_browser_automation_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "navigate-only",
      "operation": "navigate",
      "url": "https://example.com",
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "navigate",
          "url": "https://example.com",
          "title": "Fixture page for navigate-only",
          "dom_nodes": 96
        }
      }
    }
  ]
}"#,
        )
        .expect("parse success fixture");

        let mut runtime = BrowserAutomationRuntime::new(config.clone()).expect("runtime");
        let failed = runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail =
            load_browser_automation_runtime_state(&config.state_dir.join("state.json"))
                .expect("load state after fail");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let success = runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 1);
        let state_after_success =
            load_browser_automation_runtime_state(&config.state_dir.join("state.json"))
                .expect("load state after success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }

    #[tokio::test]
    async fn regression_runner_denies_action_limit_with_reason_code() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.max_actions_per_case = 1;
        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "action-limit",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "repeat-excessive",
      "operation": "action",
      "action": "click",
      "selector": "#submit",
      "action_repeat_count": 2,
      "expected": {
        "outcome": "malformed_input",
        "status_code": 429,
        "error_code": "browser_automation_action_limit_exceeded",
        "response_body": {"status":"rejected","reason":"action_limit_exceeded"}
      }
    }
  ]
}"##,
        )
        .expect("fixture should parse");

        let mut runtime = BrowserAutomationRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.denied_action_limit, 1);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("action_limit_guardrail_denied"));
    }

    #[tokio::test]
    async fn regression_runner_timeout_policy_surfaces_retryable_failure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.action_timeout_ms = 10;
        config.retry_max_attempts = 1;
        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "timeout-policy",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "wait-too-long",
      "operation": "action",
      "action": "wait",
      "selector": "#ready",
      "timeout_ms": 20,
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 504,
        "error_code": "browser_automation_timeout",
        "response_body": {"status":"retryable","reason":"timeout"}
      }
    }
  ]
}"##,
        )
        .expect("fixture should parse");

        let mut runtime = BrowserAutomationRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.retryable_failures, 1);
        assert_eq!(summary.failed_cases, 1);

        let state = load_browser_automation_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        let timeout_entry = state
            .results
            .iter()
            .find(|entry| entry.case_key.contains("wait-too-long"))
            .expect("timeout case present");
        assert_eq!(timeout_entry.error_code, "browser_automation_timeout");

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("timeout_failures_observed"));
    }

    #[tokio::test]
    async fn regression_runner_denies_unsafe_action_when_policy_disallows_it() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "unsafe-action",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "unsafe-op",
      "operation": "action",
      "action": "click",
      "selector": "#delete",
      "unsafe_operation": true,
      "expected": {
        "outcome": "malformed_input",
        "status_code": 403,
        "error_code": "browser_automation_unsafe_operation",
        "response_body": {"status":"rejected","reason":"unsafe_operation"}
      }
    }
  ]
}"##,
        )
        .expect("fixture should parse");

        let mut runtime = BrowserAutomationRuntime::new(config).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.denied_unsafe_actions, 1);
    }

    #[tokio::test]
    async fn integration_runner_allows_unsafe_action_when_policy_enabled() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.allow_unsafe_actions = true;
        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "unsafe-allowed",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "unsafe-op-allowed",
      "operation": "action",
      "action": "click",
      "selector": "#delete",
      "unsafe_operation": true,
      "timeout_ms": 3000,
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "action",
          "action": "click",
          "selector": "#delete",
          "repeat_count": 1,
          "text": "",
          "timeout_ms": 3000
        }
      }
    }
  ]
}"##,
        )
        .expect("fixture should parse");

        let mut runtime = BrowserAutomationRuntime::new(config).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.denied_unsafe_actions, 0);
    }

    #[tokio::test]
    async fn regression_runner_events_log_contains_reason_codes_for_healthy_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path("single-success.json");
        let fixture = load_browser_automation_contract_fixture(&config.fixture_path)
            .expect("fixture should load");

        let mut runtime = BrowserAutomationRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 0);
        assert_eq!(summary.malformed_cases, 0);

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(BROWSER_AUTOMATION_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        let last_line = events_log.lines().last().expect("events line");
        let payload: Value = serde_json::from_str(last_line).expect("parse json line");
        let codes = payload
            .get("reason_codes")
            .and_then(Value::as_array)
            .expect("reason codes array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(codes, vec!["healthy_cycle"]);
    }
}
