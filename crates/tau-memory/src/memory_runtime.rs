use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::memory_contract::{
    load_memory_contract_fixture, MemoryContractCase, MemoryContractFixture, MemoryEntry,
    MemoryFixtureMode, MemoryOutcomeKind, MemoryReplayResult, MemoryReplayStep,
    MEMORY_ERROR_BACKEND_UNAVAILABLE, MEMORY_ERROR_EMPTY_INPUT, MEMORY_ERROR_INVALID_SCOPE,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use tau_runtime::transport_health::TransportHealthSnapshot;

const MEMORY_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const MEMORY_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn memory_runtime_state_schema_version() -> u32 {
    MEMORY_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub struct MemoryRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryRuntimeSummary {
    pub discovered_cases: usize,
    pub queued_cases: usize,
    pub applied_cases: usize,
    pub duplicate_skips: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub retry_attempts: usize,
    pub failed_cases: usize,
    pub upserted_entries: usize,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryRuntimeCycleReport {
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
    upserted_entries: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryRuntimeState {
    #[serde(default = "memory_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    entries: Vec<MemoryEntry>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for MemoryRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: MEMORY_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            entries: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub async fn run_memory_contract_runner(config: MemoryRuntimeConfig) -> Result<()> {
    let fixture = load_memory_contract_fixture(&config.fixture_path)?;
    let mut runtime = MemoryRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "memory runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} upserted_entries={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.upserted_entries
    );
    println!(
        "memory runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct MemoryRuntime {
    config: MemoryRuntimeConfig,
    state: MemoryRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl MemoryRuntime {
    fn new(config: MemoryRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_memory_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .entries
            .sort_by(|left, right| left.memory_id.cmp(&right.memory_id));

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

    async fn run_once(&mut self, fixture: &MemoryContractFixture) -> Result<MemoryRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = MemoryRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..MemoryRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.mode.as_str().cmp(right.mode.as_str()))
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
                let result = evaluate_memory_case(&case);
                validate_case_result_against_contract(&case, &result)?;
                match result.step {
                    MemoryReplayStep::Success => {
                        let upserted =
                            self.persist_success_result(&case, &case_key, &result.entries)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.upserted_entries =
                            summary.upserted_entries.saturating_add(upserted);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    MemoryReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        self.persist_non_success_result(&case, &case_key, &result)?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    MemoryReplayStep::RetryableFailure => {
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

        save_memory_runtime_state(&self.state_path(), &self.state)?;
        append_memory_cycle_report(
            &self.config.state_dir.join(MEMORY_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn persist_success_result(
        &mut self,
        case: &MemoryContractCase,
        case_key: &str,
        entries: &[MemoryEntry],
    ) -> Result<usize> {
        let mut upserted = 0_usize;
        for entry in entries {
            if let Some(existing) = self
                .state
                .entries
                .iter_mut()
                .find(|existing| existing.memory_id == entry.memory_id)
            {
                *existing = entry.clone();
            } else {
                self.state.entries.push(entry.clone());
            }
            upserted = upserted.saturating_add(1);
        }
        self.state
            .entries
            .sort_by(|left, right| left.memory_id.cmp(&right.memory_id));

        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-memory-runner".to_string(),
                payload: json!({
                    "outcome": "success",
                    "mode": case.mode.as_str(),
                    "case_id": case.case_id,
                    "upserted_entries": upserted,
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "memory case {} applied with {} upserted entries",
                    case.case_id, upserted
                ),
            })?;
            let rendered = render_workspace_memory_snapshot(
                &self.state.entries,
                case.scope.workspace_id.trim(),
            );
            store.write_memory(&rendered)?;
        }

        Ok(upserted)
    }

    fn persist_non_success_result(
        &self,
        case: &MemoryContractCase,
        case_key: &str,
        result: &MemoryReplayResult,
    ) -> Result<()> {
        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            let outcome = match result.step {
                MemoryReplayStep::Success => "success",
                MemoryReplayStep::MalformedInput => "malformed_input",
                MemoryReplayStep::RetryableFailure => "retryable_failure",
            };
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-memory-runner".to_string(),
                payload: json!({
                    "outcome": outcome,
                    "mode": case.mode.as_str(),
                    "case_id": case.case_id,
                    "error_code": result.error_code.clone().unwrap_or_default(),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "memory case {} outcome={} error_code={}",
                    case.case_id,
                    outcome,
                    result.error_code.clone().unwrap_or_default()
                ),
            })?;
        }
        Ok(())
    }

    fn scope_channel_store(&self, case: &MemoryContractCase) -> Result<Option<ChannelStore>> {
        let channel_id = case.scope.channel_id.trim();
        if channel_id.is_empty() {
            return Ok(None);
        }
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "memory",
            channel_id,
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

fn case_runtime_key(case: &MemoryContractCase) -> String {
    format!("{}:{}", case.mode.as_str(), case.case_id.trim())
}

fn build_transport_health_snapshot(
    summary: &MemoryRuntimeSummary,
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

fn cycle_reason_codes(summary: &MemoryRuntimeSummary) -> Vec<String> {
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

fn append_memory_cycle_report(
    path: &Path,
    summary: &MemoryRuntimeSummary,
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
    let payload = MemoryRuntimeCycleReport {
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
        upserted_entries: summary.upserted_entries,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize memory runtime report")?;
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

fn evaluate_memory_case(case: &MemoryContractCase) -> MemoryReplayResult {
    let workspace = case.scope.workspace_id.trim();
    if workspace.is_empty() {
        return MemoryReplayResult {
            step: MemoryReplayStep::MalformedInput,
            error_code: Some(MEMORY_ERROR_INVALID_SCOPE.to_string()),
            entries: Vec::new(),
        };
    }
    match case.mode {
        MemoryFixtureMode::Extract => {
            if case.input_text.trim().is_empty() {
                return MemoryReplayResult {
                    step: MemoryReplayStep::MalformedInput,
                    error_code: Some(MEMORY_ERROR_EMPTY_INPUT.to_string()),
                    entries: Vec::new(),
                };
            }
            if case.simulate_retryable_failure {
                return MemoryReplayResult {
                    step: MemoryReplayStep::RetryableFailure,
                    error_code: Some(MEMORY_ERROR_BACKEND_UNAVAILABLE.to_string()),
                    entries: Vec::new(),
                };
            }
            MemoryReplayResult {
                step: MemoryReplayStep::Success,
                error_code: None,
                entries: vec![derive_extract_entry(case)],
            }
        }
        MemoryFixtureMode::Retrieve => {
            if case.query_text.trim().is_empty() {
                return MemoryReplayResult {
                    step: MemoryReplayStep::MalformedInput,
                    error_code: Some(MEMORY_ERROR_EMPTY_INPUT.to_string()),
                    entries: Vec::new(),
                };
            }
            if case.simulate_retryable_failure {
                return MemoryReplayResult {
                    step: MemoryReplayStep::RetryableFailure,
                    error_code: Some(MEMORY_ERROR_BACKEND_UNAVAILABLE.to_string()),
                    entries: Vec::new(),
                };
            }
            MemoryReplayResult {
                step: MemoryReplayStep::Success,
                error_code: None,
                entries: retrieve_ranked_entries(case),
            }
        }
    }
}

fn validate_case_result_against_contract(
    case: &MemoryContractCase,
    result: &MemoryReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        MemoryOutcomeKind::Success => MemoryReplayStep::Success,
        MemoryOutcomeKind::MalformedInput => MemoryReplayStep::MalformedInput,
        MemoryOutcomeKind::RetryableFailure => MemoryReplayStep::RetryableFailure,
    };
    if result.step != expected_step {
        bail!(
            "case '{}' expected step {:?} but observed {:?}",
            case.case_id,
            expected_step,
            result.step
        );
    }
    match case.expected.outcome {
        MemoryOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
            if result.entries != case.expected.entries {
                bail!(
                    "case '{}' expected entries {:?} but observed {:?}",
                    case.case_id,
                    case.expected.entries,
                    result.entries
                );
            }
        }
        MemoryOutcomeKind::MalformedInput | MemoryOutcomeKind::RetryableFailure => {
            if !result.entries.is_empty() {
                bail!(
                    "case '{}' expected no entries for {:?} but observed {} entries",
                    case.case_id,
                    case.expected.outcome,
                    result.entries.len()
                );
            }
            let expected_code = case.expected.error_code.trim();
            if result.error_code.as_deref() != Some(expected_code) {
                bail!(
                    "case '{}' expected error_code '{}' but observed {:?}",
                    case.case_id,
                    expected_code,
                    result.error_code
                );
            }
        }
    }
    Ok(())
}

fn derive_extract_entry(case: &MemoryContractCase) -> MemoryEntry {
    let normalized_input = normalize_whitespace(&case.input_text);
    MemoryEntry {
        memory_id: format!("mem-{}", case.case_id.trim()),
        summary: normalized_input.clone(),
        tags: derive_tags(&normalized_input),
        facts: vec![format!("scope={}", case.scope.workspace_id.trim())],
        source_event_key: format!(
            "{}:{}:{}",
            case.scope.workspace_id.trim(),
            case.mode.as_str(),
            case.case_id.trim()
        ),
        recency_weight_bps: 9_000,
        confidence_bps: 8_200,
    }
}

fn retrieve_ranked_entries(case: &MemoryContractCase) -> Vec<MemoryEntry> {
    let query_tokens = tokenize_word_set(&case.query_text);
    let mut ranked = case
        .prior_entries
        .iter()
        .cloned()
        .map(|entry| {
            (
                score_entry_against_query(&entry, &query_tokens),
                entry.recency_weight_bps,
                entry.confidence_bps,
                entry.memory_id.clone(),
                entry,
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| left.3.cmp(&right.3))
    });
    ranked
        .into_iter()
        .take(case.retrieval_limit)
        .map(|item| item.4)
        .collect()
}

fn score_entry_against_query(entry: &MemoryEntry, query_tokens: &HashSet<String>) -> u32 {
    if query_tokens.is_empty() {
        return 0;
    }
    let summary = entry.summary.to_ascii_lowercase();
    let facts = entry.facts.join(" ").to_ascii_lowercase();
    let tags = entry
        .tags
        .iter()
        .map(|tag| tag.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    query_tokens.iter().fold(0_u32, |score, token| {
        let mut updated = score;
        if summary.contains(token) {
            updated = updated.saturating_add(2);
        }
        if facts.contains(token) {
            updated = updated.saturating_add(1);
        }
        if tags.contains(token) {
            updated = updated.saturating_add(3);
        }
        updated
    })
}

fn derive_tags(text: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = HashSet::new();
    for token in tokenize_words(text) {
        if token.len() < 4 {
            continue;
        }
        if seen.insert(token.clone()) {
            tags.push(token);
        }
        if tags.len() >= 3 {
            break;
        }
    }
    if tags.is_empty() {
        tags.push("memory".to_string());
    }
    tags
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokenize_words(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for token in text.split(|character: char| !character.is_ascii_alphanumeric()) {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed.to_ascii_lowercase();
        if seen.insert(normalized.clone()) {
            ordered.push(normalized);
        }
    }
    ordered
}

fn tokenize_word_set(text: &str) -> HashSet<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter_map(|token| {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_ascii_lowercase())
            }
        })
        .collect()
}

fn render_workspace_memory_snapshot(entries: &[MemoryEntry], workspace_id: &str) -> String {
    let prefix = format!("{}:", workspace_id.trim());
    let scoped_entries = entries
        .iter()
        .filter(|entry| entry.source_event_key.starts_with(&prefix))
        .collect::<Vec<_>>();

    if scoped_entries.is_empty() {
        return format!(
            "# Tau Memory Snapshot ({})\n\n- No persisted entries",
            workspace_id.trim()
        );
    }

    let mut lines = vec![
        format!("# Tau Memory Snapshot ({})", workspace_id.trim()),
        String::new(),
    ];
    for entry in scoped_entries {
        lines.push(format!("- {}: {}", entry.memory_id, entry.summary));
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

fn load_memory_runtime_state(path: &Path) -> Result<MemoryRuntimeState> {
    if !path.exists() {
        return Ok(MemoryRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<MemoryRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "memory runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(MemoryRuntimeState::default());
        }
    };
    if parsed.schema_version != MEMORY_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "memory runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(MemoryRuntimeState::default());
    }
    Ok(parsed)
}

fn save_memory_runtime_state(path: &Path, state: &MemoryRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize memory state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_memory_runtime_state, retry_delay_ms, MemoryRuntime, MemoryRuntimeConfig,
        MEMORY_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::memory_contract::{load_memory_contract_fixture, parse_memory_contract_fixture};
    use tau_runtime::channel_store::ChannelStore;
    use tau_runtime::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("memory-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> MemoryRuntimeConfig {
        MemoryRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/memory"),
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
    async fn functional_runner_processes_fixture_and_persists_memory_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_memory_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MemoryRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.upserted_entries, 1);
        assert_eq!(summary.duplicate_skips, 0);

        let state =
            load_memory_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log =
            std::fs::read_to_string(config.state_dir.join(MEMORY_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("case_processing_failed"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "memory",
            "discord:agents",
        )
        .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Memory Snapshot (tau-core)"));
        assert!(memory.contains("mem-extract-user-preference"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_memory_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MemoryRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 0);
        assert_eq!(summary.failed_cases, 1);
        let state =
            load_memory_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.entries.len(), 1);
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_memory_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = MemoryRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);

        let mut second_runtime = MemoryRuntime::new(config).expect("second runtime");
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
            load_memory_contract_fixture(&fixture_path("mixed-outcomes.json")).expect("fixture");
        fixture.cases[0].expected.entries[0].summary = "invalid-summary".to_string();
        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;

        let mut runtime = MemoryRuntime::new(config).expect("runtime");
        let drift_fixture = load_memory_contract_fixture(&runtime.config.fixture_path)
            .expect("fixture should load");
        let error = runtime
            .run_once(&drift_fixture)
            .await
            .expect_err("drift should fail");
        assert!(error.to_string().contains("expected entries"));
    }

    #[tokio::test]
    async fn regression_runner_failure_streak_resets_after_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 1;

        let failing_fixture = parse_memory_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "retry-only-failure",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "extract-retryable",
      "mode": "extract",
      "scope": { "workspace_id": "tau-core", "channel_id": "discord:alerts" },
      "input_text": "Persist retryable memory",
      "simulate_retryable_failure": true,
      "expected": {
        "outcome": "retryable_failure",
        "error_code": "memory_backend_unavailable",
        "entries": []
      }
    }
  ]
}"#,
        )
        .expect("parse failing fixture");
        let success_fixture = parse_memory_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "single-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "extract-success",
      "mode": "extract",
      "scope": { "workspace_id": "tau-core", "channel_id": "discord:alerts" },
      "input_text": "Remember rollout checklist",
      "expected": {
        "outcome": "success",
        "entries": [
          {
            "memory_id": "mem-extract-success",
            "summary": "Remember rollout checklist",
            "tags": [ "remember", "rollout", "checklist" ],
            "facts": [ "scope=tau-core" ],
            "source_event_key": "tau-core:extract:extract-success",
            "recency_weight_bps": 9000,
            "confidence_bps": 8200
          }
        ]
      }
    }
  ]
}"#,
        )
        .expect("parse success fixture");

        let mut runtime = MemoryRuntime::new(config.clone()).expect("runtime");
        let failed = runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail = load_memory_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state after fail");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let success = runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 1);
        let state_after_success = load_memory_runtime_state(&config.state_dir.join("state.json"))
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
        config.fixture_path = fixture_path("retrieve-ranking.json");
        let fixture =
            load_memory_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MemoryRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 0);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(MEMORY_RUNTIME_EVENTS_LOG_FILE))
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
            load_memory_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = MemoryRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 1);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(MEMORY_RUNTIME_EVENTS_LOG_FILE))
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
