use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::custom_command_contract::{
    evaluate_custom_command_case, load_custom_command_contract_fixture,
    validate_custom_command_case_result_against_contract, CustomCommandContractCase,
    CustomCommandContractFixture, CustomCommandReplayResult, CustomCommandReplayStep,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use tau_runtime::transport_health::TransportHealthSnapshot;

const CUSTOM_COMMAND_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn custom_command_runtime_state_schema_version() -> u32 {
    CUSTOM_COMMAND_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
/// Public struct `CustomCommandRuntimeConfig` used across Tau components.
pub struct CustomCommandRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `CustomCommandRuntimeSummary` used across Tau components.
pub struct CustomCommandRuntimeSummary {
    pub discovered_cases: usize,
    pub queued_cases: usize,
    pub applied_cases: usize,
    pub duplicate_skips: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub retry_attempts: usize,
    pub failed_cases: usize,
    pub upserted_commands: usize,
    pub deleted_commands: usize,
    pub executed_runs: usize,
}

#[derive(Debug, Clone, Serialize)]
struct CustomCommandRuntimeCycleReport {
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
    upserted_commands: usize,
    deleted_commands: usize,
    executed_runs: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CustomCommandRecord {
    case_key: String,
    case_id: String,
    command_name: String,
    template: String,
    operation: String,
    last_status_code: u16,
    last_outcome: String,
    run_count: u64,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CustomCommandRuntimeState {
    #[serde(default = "custom_command_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    commands: Vec<CustomCommandRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for CustomCommandRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: CUSTOM_COMMAND_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            commands: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CustomCommandMutationCounts {
    upserted_commands: usize,
    deleted_commands: usize,
    executed_runs: usize,
}

pub async fn run_custom_command_contract_runner(config: CustomCommandRuntimeConfig) -> Result<()> {
    let fixture = load_custom_command_contract_fixture(&config.fixture_path)?;
    let mut runtime = CustomCommandRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "custom-command runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} upserted_commands={} deleted_commands={} executed_runs={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.upserted_commands,
        summary.deleted_commands,
        summary.executed_runs
    );
    println!(
        "custom-command runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct CustomCommandRuntime {
    config: CustomCommandRuntimeConfig,
    state: CustomCommandRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl CustomCommandRuntime {
    fn new(config: CustomCommandRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .commands
            .sort_by(|left, right| left.command_name.cmp(&right.command_name));
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
        fixture: &CustomCommandContractFixture,
    ) -> Result<CustomCommandRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = CustomCommandRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..CustomCommandRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.operation.cmp(&right.operation))
                .then_with(|| left.command_name.cmp(&right.command_name))
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
                let result = evaluate_custom_command_case(&case);
                validate_custom_command_case_result_against_contract(&case, &result)?;
                match result.step {
                    CustomCommandReplayStep::Success => {
                        let mutation = self.persist_success_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.upserted_commands = summary
                            .upserted_commands
                            .saturating_add(mutation.upserted_commands);
                        summary.deleted_commands = summary
                            .deleted_commands
                            .saturating_add(mutation.deleted_commands);
                        summary.executed_runs =
                            summary.executed_runs.saturating_add(mutation.executed_runs);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    CustomCommandReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(&case, &case_key, &result)?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    CustomCommandReplayStep::RetryableFailure => {
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

        save_custom_command_runtime_state(&self.state_path(), &self.state)?;
        append_custom_command_cycle_report(
            &self
                .config
                .state_dir
                .join(CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn persist_success_result(
        &mut self,
        case: &CustomCommandContractCase,
        case_key: &str,
        result: &CustomCommandReplayResult,
    ) -> Result<CustomCommandMutationCounts> {
        let operation = normalize_operation(&case.operation);
        let command_name = case.command_name.trim().to_string();
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let mut mutation = CustomCommandMutationCounts::default();

        match operation.as_str() {
            "CREATE" | "UPDATE" => {
                let record = CustomCommandRecord {
                    case_key: case_key.to_string(),
                    case_id: case.case_id.clone(),
                    command_name: command_name.clone(),
                    template: case.template.trim().to_string(),
                    operation: operation.clone(),
                    last_status_code: result.status_code,
                    last_outcome: "success".to_string(),
                    run_count: self
                        .state
                        .commands
                        .iter()
                        .find(|existing| existing.command_name == command_name)
                        .map_or(0, |existing| existing.run_count),
                    updated_unix_ms: timestamp_unix_ms,
                };
                if let Some(existing) = self
                    .state
                    .commands
                    .iter_mut()
                    .find(|existing| existing.command_name == command_name)
                {
                    *existing = record;
                } else {
                    self.state.commands.push(record);
                }
                mutation.upserted_commands = 1;
            }
            "DELETE" => {
                let before = self.state.commands.len();
                self.state
                    .commands
                    .retain(|existing| existing.command_name != command_name);
                mutation.deleted_commands = before.saturating_sub(self.state.commands.len());
            }
            "RUN" => {
                if let Some(existing) = self
                    .state
                    .commands
                    .iter_mut()
                    .find(|existing| existing.command_name == command_name)
                {
                    existing.case_key = case_key.to_string();
                    existing.case_id = case.case_id.clone();
                    existing.operation = operation.clone();
                    existing.last_status_code = result.status_code;
                    existing.last_outcome = "success".to_string();
                    existing.run_count = existing.run_count.saturating_add(1);
                    existing.updated_unix_ms = timestamp_unix_ms;
                } else {
                    self.state.commands.push(CustomCommandRecord {
                        case_key: case_key.to_string(),
                        case_id: case.case_id.clone(),
                        command_name: command_name.clone(),
                        template: String::new(),
                        operation: operation.clone(),
                        last_status_code: result.status_code,
                        last_outcome: "success".to_string(),
                        run_count: 1,
                        updated_unix_ms: timestamp_unix_ms,
                    });
                    mutation.upserted_commands = 1;
                }
                mutation.executed_runs = 1;
            }
            "LIST" => {}
            _ => {}
        }

        self.state
            .commands
            .sort_by(|left, right| left.command_name.cmp(&right.command_name));

        if let Some(store) = self.scope_channel_store(case)? {
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-custom-command-runner".to_string(),
                payload: json!({
                    "outcome": "success",
                    "operation": operation.to_ascii_lowercase(),
                    "case_id": case.case_id,
                    "command_name": command_name,
                    "status_code": result.status_code,
                    "upserted_commands": mutation.upserted_commands,
                    "deleted_commands": mutation.deleted_commands,
                    "executed_runs": mutation.executed_runs,
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "custom-command case {} applied operation={} command={} status={}",
                    case.case_id,
                    operation.to_ascii_lowercase(),
                    channel_id_for_case(case),
                    result.status_code
                ),
            })?;
            store.write_memory(&render_custom_command_snapshot(
                &self.state.commands,
                &channel_id_for_case(case),
            ))?;
        }

        Ok(mutation)
    }

    fn persist_non_success_result(
        &self,
        case: &CustomCommandContractCase,
        case_key: &str,
        result: &CustomCommandReplayResult,
    ) -> Result<()> {
        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            let outcome = outcome_name(result.step);
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-custom-command-runner".to_string(),
                payload: json!({
                    "outcome": outcome,
                    "case_id": case.case_id,
                    "operation": normalize_operation(&case.operation).to_ascii_lowercase(),
                    "command_name": case.command_name.trim(),
                    "status_code": result.status_code,
                    "error_code": result.error_code.clone().unwrap_or_default(),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "custom-command case {} outcome={} error_code={} status={}",
                    case.case_id,
                    outcome,
                    result.error_code.clone().unwrap_or_default(),
                    result.status_code
                ),
            })?;
        }
        Ok(())
    }

    fn scope_channel_store(
        &self,
        case: &CustomCommandContractCase,
    ) -> Result<Option<ChannelStore>> {
        let channel_id = channel_id_for_case(case);
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "custom-command",
            &channel_id,
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

fn normalize_operation(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

fn channel_id_for_case(case: &CustomCommandContractCase) -> String {
    let trimmed = case.command_name.trim();
    if trimmed.is_empty() {
        return "registry".to_string();
    }
    if trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return trimmed.to_string();
    }
    "registry".to_string()
}

fn case_runtime_key(case: &CustomCommandContractCase) -> String {
    format!(
        "{}:{}:{}",
        normalize_operation(&case.operation),
        case.command_name.trim(),
        case.case_id.trim()
    )
}

fn outcome_name(step: CustomCommandReplayStep) -> &'static str {
    match step {
        CustomCommandReplayStep::Success => "success",
        CustomCommandReplayStep::MalformedInput => "malformed_input",
        CustomCommandReplayStep::RetryableFailure => "retryable_failure",
    }
}

fn build_transport_health_snapshot(
    summary: &CustomCommandRuntimeSummary,
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

fn cycle_reason_codes(summary: &CustomCommandRuntimeSummary) -> Vec<String> {
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
    if summary.upserted_commands > 0 || summary.deleted_commands > 0 {
        codes.push("command_registry_mutated".to_string());
    }
    if summary.executed_runs > 0 {
        codes.push("command_runs_recorded".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_custom_command_cycle_report(
    path: &Path,
    summary: &CustomCommandRuntimeSummary,
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
    let payload = CustomCommandRuntimeCycleReport {
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
        upserted_commands: summary.upserted_commands,
        deleted_commands: summary.deleted_commands,
        executed_runs: summary.executed_runs,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line =
        serde_json::to_string(&payload).context("serialize custom-command runtime report")?;
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

fn render_custom_command_snapshot(records: &[CustomCommandRecord], channel_id: &str) -> String {
    let filtered = if channel_id == "registry" {
        records.iter().collect::<Vec<_>>()
    } else {
        records
            .iter()
            .filter(|record| record.command_name == channel_id)
            .collect::<Vec<_>>()
    };

    if filtered.is_empty() {
        return format!("# Tau Custom Command Snapshot ({channel_id})\n\n- No registered commands");
    }

    let mut lines = vec![
        format!("# Tau Custom Command Snapshot ({channel_id})"),
        String::new(),
    ];
    for record in filtered {
        lines.push(format!(
            "- {} op={} status={} runs={} template={}",
            record.command_name,
            record.operation.to_ascii_lowercase(),
            record.last_status_code,
            record.run_count,
            record.template
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

fn load_custom_command_runtime_state(path: &Path) -> Result<CustomCommandRuntimeState> {
    if !path.exists() {
        return Ok(CustomCommandRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<CustomCommandRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "custom-command runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(CustomCommandRuntimeState::default());
        }
    };
    if parsed.schema_version != CUSTOM_COMMAND_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "custom-command runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(CustomCommandRuntimeState::default());
    }
    Ok(parsed)
}

fn save_custom_command_runtime_state(path: &Path, state: &CustomCommandRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize custom-command state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        load_custom_command_runtime_state, retry_delay_ms, CustomCommandRuntime,
        CustomCommandRuntimeConfig, CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::custom_command_contract::{
        load_custom_command_contract_fixture, parse_custom_command_contract_fixture,
    };
    use tau_runtime::channel_store::ChannelStore;
    use tau_runtime::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("custom-command-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> CustomCommandRuntimeConfig {
        CustomCommandRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/custom-command"),
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
    async fn functional_runner_processes_fixture_and_persists_custom_command_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = load_custom_command_contract_fixture(&config.fixture_path)
            .expect("fixture should load");
        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.upserted_commands, 1);
        assert_eq!(summary.deleted_commands, 0);
        assert_eq!(summary.executed_runs, 0);
        assert_eq!(summary.duplicate_skips, 0);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.commands.len(), 1);
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
                .join(CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("case_processing_failed"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "custom-command",
            "deploy_release",
        )
        .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Custom Command Snapshot (deploy_release)"));
        assert!(memory.contains("deploy_release"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture = load_custom_command_contract_fixture(&config.fixture_path)
            .expect("fixture should load");
        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.failed_cases, 0);
        assert_eq!(summary.retryable_failures, 0);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.commands.len(), 1);
        assert_eq!(state.health.queue_depth, 1);
        assert_eq!(state.health.classify().state, TransportHealthState::Healthy);
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = load_custom_command_contract_fixture(&config.fixture_path)
            .expect("fixture should load");

        let mut first_runtime = CustomCommandRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);
        assert_eq!(first.failed_cases, 1);

        let mut second_runtime = CustomCommandRuntime::new(config).expect("second runtime");
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
            load_custom_command_contract_fixture(&fixture_path("mixed-outcomes.json"))
                .expect("fixture should load");
        let success_case = fixture
            .cases
            .iter_mut()
            .find(|case| case.case_id == "custom-command-create-success")
            .expect("success case");
        success_case.expected.response_body = json!({
            "status":"accepted",
            "operation":"create",
            "command_name":"unexpected"
        });
        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let mut runtime = CustomCommandRuntime::new(config).expect("runtime");
        let drift_fixture = load_custom_command_contract_fixture(&runtime.config.fixture_path)
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

        let failing_fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "retry-only-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-retry-only",
      "operation": "run",
      "command_name": "deploy_release",
      "template": "",
      "arguments": {"env":"staging"},
      "simulate_retryable_failure": true,
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "custom_command_backend_unavailable",
        "response_body": {"status":"retryable","reason":"backend_unavailable"}
      }
    }
  ]
}"#,
        )
        .expect("parse failing fixture");
        let success_fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-create-only",
      "operation": "create",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "arguments": {"env":"staging"},
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {
          "status":"accepted",
          "operation":"create",
          "command_name":"deploy_release"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse success fixture");

        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let failed = runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail =
            load_custom_command_runtime_state(&config.state_dir.join("state.json"))
                .expect("load state after fail");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let success = runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 1);
        let state_after_success =
            load_custom_command_runtime_state(&config.state_dir.join("state.json"))
                .expect("load state after success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }
}
