use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};

use crate::deployment_contract::{
    evaluate_deployment_case, load_deployment_contract_fixture,
    validate_deployment_case_result_against_contract, DeploymentContractCase,
    DeploymentContractFixture, DeploymentReplayResult, DeploymentReplayStep,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::TransportHealthSnapshot;

const DEPLOYMENT_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const DEPLOYMENT_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn deployment_runtime_state_schema_version() -> u32 {
    DEPLOYMENT_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
/// Public struct `DeploymentRuntimeConfig` used across Tau components.
pub struct DeploymentRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `DeploymentRuntimeSummary` used across Tau components.
pub struct DeploymentRuntimeSummary {
    pub discovered_cases: usize,
    pub queued_cases: usize,
    pub applied_cases: usize,
    pub duplicate_skips: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub retry_attempts: usize,
    pub failed_cases: usize,
    pub upserted_rollouts: usize,
    pub wasm_rollouts: usize,
    pub cloud_rollouts: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DeploymentRuntimeCycleReport {
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
    upserted_rollouts: usize,
    wasm_rollouts: usize,
    cloud_rollouts: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct DeploymentPlanRecord {
    case_key: String,
    case_id: String,
    deploy_target: String,
    runtime_profile: String,
    blueprint_id: String,
    environment: String,
    region: String,
    artifact: String,
    #[serde(default)]
    artifact_sha256: String,
    #[serde(default)]
    artifact_size_bytes: u64,
    #[serde(default)]
    artifact_manifest: String,
    #[serde(default)]
    runtime_constraints: Vec<String>,
    replicas: u16,
    rollout_strategy: String,
    status_code: u16,
    outcome: String,
    error_code: String,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeploymentRuntimeState {
    #[serde(default = "deployment_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    rollouts: Vec<DeploymentPlanRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for DeploymentRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: DEPLOYMENT_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            rollouts: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DeploymentMutationCounts {
    upserted_rollouts: usize,
    wasm_rollouts: usize,
    cloud_rollouts: usize,
}

pub async fn run_deployment_contract_runner(config: DeploymentRuntimeConfig) -> Result<()> {
    let fixture = load_deployment_contract_fixture(&config.fixture_path)?;
    let mut runtime = DeploymentRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "deployment runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} upserted_rollouts={} wasm_rollouts={} cloud_rollouts={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.upserted_rollouts,
        summary.wasm_rollouts,
        summary.cloud_rollouts
    );
    println!(
        "deployment runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct DeploymentRuntime {
    config: DeploymentRuntimeConfig,
    state: DeploymentRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl DeploymentRuntime {
    fn new(config: DeploymentRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_deployment_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .rollouts
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
        fixture: &DeploymentContractFixture,
    ) -> Result<DeploymentRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = DeploymentRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..DeploymentRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.blueprint_id.cmp(&right.blueprint_id))
                .then_with(|| left.deploy_target.cmp(&right.deploy_target))
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
                let result = evaluate_deployment_case(&case);
                validate_deployment_case_result_against_contract(&case, &result)?;
                match result.step {
                    DeploymentReplayStep::Success => {
                        let mutation = self.persist_success_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.upserted_rollouts = summary
                            .upserted_rollouts
                            .saturating_add(mutation.upserted_rollouts);
                        summary.wasm_rollouts =
                            summary.wasm_rollouts.saturating_add(mutation.wasm_rollouts);
                        summary.cloud_rollouts = summary
                            .cloud_rollouts
                            .saturating_add(mutation.cloud_rollouts);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    DeploymentReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(&case, &case_key, &result)?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    DeploymentReplayStep::RetryableFailure => {
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

        save_deployment_runtime_state(&self.state_path(), &self.state)?;
        append_deployment_cycle_report(
            &self
                .config
                .state_dir
                .join(DEPLOYMENT_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn persist_success_result(
        &mut self,
        case: &DeploymentContractCase,
        case_key: &str,
        result: &DeploymentReplayResult,
    ) -> Result<DeploymentMutationCounts> {
        let deploy_target = case.deploy_target.trim().to_ascii_lowercase();
        let runtime_profile = case.runtime_profile.trim().to_ascii_lowercase();
        let blueprint_id = case.blueprint_id.trim().to_string();
        let environment = case.environment.trim().to_ascii_lowercase();
        let region = case.region.trim().to_string();
        let artifact = result
            .response_body
            .get("artifact")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let artifact_sha256 = result
            .response_body
            .get("artifact_sha256")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let artifact_size_bytes = result
            .response_body
            .get("artifact_size_bytes")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let artifact_manifest = result
            .response_body
            .get("artifact_manifest")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let runtime_constraints = result
            .response_body
            .get("runtime_constraints")
            .and_then(serde_json::Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let rollout_strategy = result
            .response_body
            .get("rollout_strategy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let record = DeploymentPlanRecord {
            case_key: case_key.to_string(),
            case_id: case.case_id.clone(),
            deploy_target: deploy_target.clone(),
            runtime_profile: runtime_profile.clone(),
            blueprint_id: blueprint_id.clone(),
            environment,
            region,
            artifact: artifact.clone(),
            artifact_sha256: artifact_sha256.clone(),
            artifact_size_bytes,
            artifact_manifest: artifact_manifest.clone(),
            runtime_constraints: runtime_constraints.clone(),
            replicas: case.replicas,
            rollout_strategy: rollout_strategy.clone(),
            status_code: result.status_code,
            outcome: outcome_name(result.step).to_string(),
            error_code: String::new(),
            updated_unix_ms: current_unix_timestamp_ms(),
        };

        if let Some(existing) = self
            .state
            .rollouts
            .iter_mut()
            .find(|existing| existing.case_key == record.case_key)
        {
            *existing = record;
        } else {
            self.state.rollouts.push(record);
        }
        self.state
            .rollouts
            .sort_by(|left, right| left.case_key.cmp(&right.case_key));

        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-deployment-runner".to_string(),
                payload: json!({
                    "outcome": "success",
                    "case_id": case.case_id,
                    "deploy_target": deploy_target,
                    "runtime_profile": runtime_profile,
                    "blueprint_id": blueprint_id,
                    "status_code": result.status_code,
                    "artifact": artifact,
                    "artifact_sha256": artifact_sha256,
                    "artifact_size_bytes": artifact_size_bytes,
                    "artifact_manifest": artifact_manifest,
                    "runtime_constraints": runtime_constraints,
                    "rollout_strategy": rollout_strategy,
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "deployment case {} applied target={} runtime={} status={}",
                    case.case_id, case.deploy_target, case.runtime_profile, result.status_code
                ),
            })?;
            store.write_memory(&render_deployment_snapshot(
                &self.state.rollouts,
                case.blueprint_id.trim(),
            ))?;
        }

        Ok(DeploymentMutationCounts {
            upserted_rollouts: 1,
            wasm_rollouts: usize::from(deploy_target == "wasm"),
            cloud_rollouts: usize::from(deploy_target != "wasm"),
        })
    }

    fn persist_non_success_result(
        &self,
        case: &DeploymentContractCase,
        case_key: &str,
        result: &DeploymentReplayResult,
    ) -> Result<()> {
        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            let outcome = outcome_name(result.step);
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-deployment-runner".to_string(),
                payload: json!({
                    "outcome": outcome,
                    "case_id": case.case_id,
                    "deploy_target": case.deploy_target.trim().to_ascii_lowercase(),
                    "runtime_profile": case.runtime_profile.trim().to_ascii_lowercase(),
                    "blueprint_id": case.blueprint_id.trim(),
                    "status_code": result.status_code,
                    "error_code": result.error_code.clone().unwrap_or_default(),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "deployment case {} outcome={} error_code={} status={}",
                    case.case_id,
                    outcome,
                    result.error_code.clone().unwrap_or_default(),
                    result.status_code
                ),
            })?;
        }
        Ok(())
    }

    fn scope_channel_store(&self, case: &DeploymentContractCase) -> Result<Option<ChannelStore>> {
        let channel_id = channel_id_for_case(case);
        if channel_id.is_empty() {
            return Ok(None);
        }
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "deployment",
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

fn normalized_blueprint_channel_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "deployment".to_string();
    }
    if trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return trimmed.to_string();
    }
    "deployment".to_string()
}

fn channel_id_for_case(case: &DeploymentContractCase) -> String {
    normalized_blueprint_channel_id(&case.blueprint_id)
}

fn case_runtime_key(case: &DeploymentContractCase) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        case.deploy_target.trim().to_ascii_lowercase(),
        case.runtime_profile.trim().to_ascii_lowercase(),
        case.environment.trim().to_ascii_lowercase(),
        case.blueprint_id.trim().to_ascii_lowercase(),
        case.case_id.trim()
    )
}

fn outcome_name(step: DeploymentReplayStep) -> &'static str {
    match step {
        DeploymentReplayStep::Success => "success",
        DeploymentReplayStep::MalformedInput => "malformed_input",
        DeploymentReplayStep::RetryableFailure => "retryable_failure",
    }
}

fn build_transport_health_snapshot(
    summary: &DeploymentRuntimeSummary,
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

fn cycle_reason_codes(summary: &DeploymentRuntimeSummary) -> Vec<String> {
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
    if summary.wasm_rollouts > 0 {
        codes.push("wasm_rollout_applied".to_string());
    }
    if summary.cloud_rollouts > 0 {
        codes.push("cloud_rollout_applied".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_deployment_cycle_report(
    path: &Path,
    summary: &DeploymentRuntimeSummary,
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
    let payload = DeploymentRuntimeCycleReport {
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
        upserted_rollouts: summary.upserted_rollouts,
        wasm_rollouts: summary.wasm_rollouts,
        cloud_rollouts: summary.cloud_rollouts,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize deployment runtime report")?;
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

fn render_deployment_snapshot(records: &[DeploymentPlanRecord], blueprint_id: &str) -> String {
    let filtered = records
        .iter()
        .filter(|record| record.blueprint_id == blueprint_id)
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        return format!("# Tau Deployment Snapshot ({blueprint_id})\n\n- No persisted rollouts");
    }

    let mut lines = vec![
        format!("# Tau Deployment Snapshot ({blueprint_id})"),
        String::new(),
    ];
    for record in filtered {
        lines.push(format!(
            "- {} target={} runtime={} artifact={} replicas={} status={} outcome={}",
            record.case_id,
            record.deploy_target,
            record.runtime_profile,
            record.artifact,
            record.replicas,
            record.status_code,
            record.outcome
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

fn load_deployment_runtime_state(path: &Path) -> Result<DeploymentRuntimeState> {
    if !path.exists() {
        return Ok(DeploymentRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<DeploymentRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "deployment runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(DeploymentRuntimeState::default());
        }
    };
    if parsed.schema_version != DEPLOYMENT_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "deployment runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(DeploymentRuntimeState::default());
    }
    Ok(parsed)
}

fn save_deployment_runtime_state(path: &Path, state: &DeploymentRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize deployment state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_deployment_runtime_state, retry_delay_ms, DeploymentRuntime, DeploymentRuntimeConfig,
        DEPLOYMENT_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::deployment_contract::{
        load_deployment_contract_fixture, parse_deployment_contract_fixture,
    };
    use crate::deployment_wasm::{package_deployment_wasm_artifact, DeploymentWasmPackageConfig};
    use tau_runtime::channel_store::ChannelStore;
    use tau_runtime::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("deployment-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> DeploymentRuntimeConfig {
        DeploymentRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/deployment"),
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
    async fn functional_runner_processes_fixture_and_persists_deployment_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_deployment_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = DeploymentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.upserted_rollouts, 1);
        assert_eq!(summary.wasm_rollouts, 1);
        assert_eq!(summary.cloud_rollouts, 0);
        assert_eq!(summary.duplicate_skips, 0);

        let state =
            load_deployment_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.rollouts.len(), 1);
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log =
            std::fs::read_to_string(config.state_dir.join(DEPLOYMENT_RUNTIME_EVENTS_LOG_FILE))
                .expect("read events log");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("case_processing_failed"));
        assert!(events_log.contains("wasm_rollout_applied"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "deployment",
            "edge-wasm",
        )
        .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Deployment Snapshot (edge-wasm)"));
        assert!(memory.contains("deployment-success-wasm"));
    }

    #[tokio::test]
    async fn integration_runner_consumes_packaged_wasm_manifest_metadata() {
        let temp = tempdir().expect("tempdir");
        let module_path = temp.path().join("edge.wasm");
        std::fs::write(
            &module_path,
            [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
        )
        .expect("write wasm");
        let state_dir = temp.path().join(".tau/deployment");
        let package_report = package_deployment_wasm_artifact(&DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm-packaged".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("wasm-out"),
            state_dir: state_dir.clone(),
        })
        .expect("package wasm");

        let manifest_raw =
            std::fs::read_to_string(&package_report.manifest_path).expect("read manifest");
        let manifest: Value = serde_json::from_str(&manifest_raw).expect("parse manifest");
        let artifact = manifest
            .get("artifact_path")
            .and_then(Value::as_str)
            .expect("artifact path");
        let artifact_sha = manifest
            .get("artifact_sha256")
            .and_then(Value::as_str)
            .expect("artifact sha");
        let artifact_size = manifest
            .get("artifact_size_bytes")
            .and_then(Value::as_u64)
            .expect("artifact size");
        let constraints = manifest
            .get("capability_constraints")
            .cloned()
            .expect("constraints");

        let fixture_json = json!({
            "schema_version": 1,
            "name": "wasm-manifest-fixture",
            "cases": [
                {
                    "schema_version": 1,
                    "case_id": "deployment-wasm-manifest-success",
                    "deploy_target": "wasm",
                    "runtime_profile": "wasm_wasi",
                    "blueprint_id": "edge-wasm-packaged",
                    "environment": "production",
                    "region": "iad",
                    "wasm_manifest": package_report.manifest_path,
                    "replicas": 1,
                    "expected": {
                        "outcome": "success",
                        "status_code": 201,
                        "response_body": {
                            "status": "accepted",
                            "blueprint_id": "edge-wasm-packaged",
                            "deploy_target": "wasm",
                            "runtime_profile": "wasm_wasi",
                            "environment": "production",
                            "region": "iad",
                            "artifact": artifact,
                            "artifact_sha256": artifact_sha,
                            "artifact_size_bytes": artifact_size,
                            "artifact_manifest": package_report.manifest_path,
                            "runtime_constraints": constraints,
                            "replicas": 1,
                            "rollout_strategy": "canary"
                        }
                    }
                }
            ]
        });
        let fixture_path = temp.path().join("wasm-manifest-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture_json).expect("serialize fixture"),
        )
        .expect("write fixture");
        let fixture = parse_deployment_contract_fixture(
            &serde_json::to_string(&fixture_json).expect("raw fixture"),
        )
        .expect("parse fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;
        config.state_dir = state_dir;
        let mut runtime = DeploymentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.wasm_rollouts, 1);

        let state =
            load_deployment_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state.rollouts.len(), 1);
        assert_eq!(state.rollouts[0].artifact_sha256, artifact_sha);
        assert_eq!(state.rollouts[0].artifact_size_bytes, artifact_size);
        assert_eq!(
            state.rollouts[0].artifact_manifest,
            package_report.manifest_path
        );
        assert!(!state.rollouts[0].runtime_constraints.is_empty());
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_deployment_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = DeploymentRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.failed_cases, 1);
        let state =
            load_deployment_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert!(state.rollouts.is_empty());
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_deployment_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = DeploymentRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);

        let mut second_runtime = DeploymentRuntime::new(config).expect("second runtime");
        let second = second_runtime.run_once(&fixture).await.expect("second run");
        assert_eq!(second.duplicate_skips, 2);
        assert_eq!(second.applied_cases, 0);
        assert_eq!(second.malformed_cases, 0);
        assert_eq!(second.failed_cases, 1);
    }

    #[tokio::test]
    async fn regression_runner_rejects_contract_drift_between_expected_and_runtime_result() {
        let temp = tempdir().expect("tempdir");
        let mut fixture = load_deployment_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture");
        let success_case = fixture
            .cases
            .iter_mut()
            .find(|case| case.case_id == "deployment-success-wasm")
            .expect("success case");
        success_case.expected.response_body = json!({
            "status":"accepted",
            "blueprint_id":"edge-wasm",
            "deploy_target":"wasm",
            "runtime_profile":"wasm_wasi",
            "environment":"staging",
            "region":"iad",
            "artifact":"edge/runtime-v2.wasm",
            "replicas":2,
            "rollout_strategy":"canary"
        });
        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize fixture"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let mut runtime = DeploymentRuntime::new(config).expect("runtime");
        let error = runtime
            .run_once(&fixture)
            .await
            .expect_err("contract drift should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[tokio::test]
    async fn regression_runner_failure_streak_resets_after_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());

        let fixture_fail = load_deployment_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture");
        let mut runtime_fail = DeploymentRuntime::new(config.clone()).expect("runtime fail");
        let fail_summary = runtime_fail
            .run_once(&fixture_fail)
            .await
            .expect("run fail");
        assert_eq!(fail_summary.failed_cases, 1);

        let state_after_fail =
            load_deployment_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let fixture_success = load_deployment_contract_fixture(&fixture_path("rollout-pass.json"))
            .expect("fixture success");
        config.fixture_path = fixture_path("rollout-pass.json");
        let mut runtime_success = DeploymentRuntime::new(config.clone()).expect("runtime success");
        let success_summary = runtime_success
            .run_once(&fixture_success)
            .await
            .expect("run success");
        assert_eq!(success_summary.failed_cases, 0);
        assert!(success_summary.applied_cases > 0);

        let state_after_success =
            load_deployment_runtime_state(&config.state_dir.join("state.json")).expect("state");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }

    #[test]
    fn regression_load_state_recovers_from_invalid_json() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("state.json");
        std::fs::write(&state_path, "{not-json").expect("write invalid json");

        let state = load_deployment_runtime_state(&state_path).expect("load state");
        assert!(state.rollouts.is_empty());
        assert!(state.processed_case_keys.is_empty());
    }

    #[test]
    fn regression_parse_fixture_rejects_runtime_mismatch_for_non_malformed() {
        let raw = r#"{
  "schema_version": 1,
  "name": "invalid-runtime-mismatch",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "bad",
      "deploy_target": "wasm",
      "runtime_profile": "native",
      "blueprint_id": "edge",
      "environment": "staging",
      "region": "iad",
      "wasm_module": "edge/runtime.wasm",
      "replicas": 1,
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {
          "status":"accepted",
          "blueprint_id":"edge",
          "deploy_target":"wasm",
          "runtime_profile":"native",
          "environment":"staging",
          "region":"iad",
          "artifact":"edge/runtime.wasm",
          "replicas":1,
          "rollout_strategy":"canary"
        }
      }
    }
  ]
}"#;
        let error =
            parse_deployment_contract_fixture(raw).expect_err("runtime mismatch should fail");
        assert!(error.to_string().contains("uses unsupported runtime"));
    }

    #[test]
    fn integration_events_log_lines_are_valid_json_objects() {
        let temp = tempdir().expect("tempdir");
        let state_dir = temp.path().join(".tau/deployment");
        std::fs::create_dir_all(&state_dir).expect("create state dir");
        let log_path = state_dir.join(DEPLOYMENT_RUNTIME_EVENTS_LOG_FILE);
        std::fs::write(
            &log_path,
            r#"{"health_state":"healthy","reason_codes":["healthy_cycle"]}
{"health_state":"degraded","reason_codes":["retry_attempted"]}
"#,
        )
        .expect("write events log");

        let raw = std::fs::read_to_string(&log_path).expect("read events log");
        for line in raw.lines() {
            let parsed: Value = serde_json::from_str(line).expect("line should parse");
            assert!(parsed.is_object());
        }
    }
}
