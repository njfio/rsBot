use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::process::Command;

use crate::custom_command_contract::{
    evaluate_custom_command_case_with_policy, load_custom_command_contract_fixture,
    validate_custom_command_case_result_against_contract, CustomCommandContractCase,
    CustomCommandContractFixture, CustomCommandReplayResult, CustomCommandReplayStep,
    CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE, CUSTOM_COMMAND_ERROR_INVALID_NAME,
    CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE, CUSTOM_COMMAND_ERROR_POLICY_DENIED,
};
use crate::custom_command_policy::{
    is_valid_env_key, validate_custom_command_template_and_arguments, CustomCommandExecutionPolicy,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use tau_runtime::transport_health::TransportHealthSnapshot;

const CUSTOM_COMMAND_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const DEFAULT_CUSTOM_COMMAND_RUN_TIMEOUT_MS: u64 = 30_000;
const CUSTOM_COMMAND_RUN_OUTPUT_MAX_BYTES: usize = 16 * 1024;

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
    pub run_timeout_ms: u64,
    pub default_execution_policy: CustomCommandExecutionPolicy,
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
    pub run_timeout_failures: usize,
    pub run_non_zero_exit_failures: usize,
    pub run_spawn_failures: usize,
    pub run_missing_command_failures: usize,
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
    run_timeout_failures: usize,
    run_non_zero_exit_failures: usize,
    run_spawn_failures: usize,
    run_missing_command_failures: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CustomCommandRecord {
    case_key: String,
    case_id: String,
    command_name: String,
    template: String,
    #[serde(default)]
    execution_policy: CustomCommandExecutionPolicy,
    operation: String,
    last_status_code: u16,
    last_outcome: String,
    run_count: u64,
    #[serde(default)]
    last_error_code: String,
    #[serde(default)]
    last_command_line: String,
    #[serde(default)]
    last_exit_code: Option<i32>,
    #[serde(default)]
    last_stdout: String,
    #[serde(default)]
    last_stderr: String,
    #[serde(default)]
    last_timed_out: bool,
    #[serde(default)]
    last_duration_ms: u64,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct CustomCommandRunAudit {
    executed: bool,
    rendered_command: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    duration_ms: u64,
    failure_reason: String,
}

#[derive(Debug, Clone)]
struct CustomCommandRunOutcome {
    result: CustomCommandReplayResult,
    audit: Option<CustomCommandRunAudit>,
}

pub async fn run_custom_command_contract_runner(config: CustomCommandRuntimeConfig) -> Result<()> {
    let fixture = load_custom_command_contract_fixture(&config.fixture_path)?;
    let mut runtime = CustomCommandRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "custom-command runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} upserted_commands={} deleted_commands={} executed_runs={} run_timeout_failures={} run_non_zero_exit_failures={} run_spawn_failures={} run_missing_command_failures={}",
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
        summary.executed_runs,
        summary.run_timeout_failures,
        summary.run_non_zero_exit_failures,
        summary.run_spawn_failures,
        summary.run_missing_command_failures
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
                let mut result = evaluate_custom_command_case_with_policy(
                    &case,
                    &self.config.default_execution_policy,
                );
                let mut run_audit = None;
                if normalize_operation(&case.operation) == "RUN"
                    && result.step == CustomCommandReplayStep::Success
                {
                    let run_outcome = self.execute_run_operation(&case).await;
                    result = run_outcome.result;
                    run_audit = run_outcome.audit;
                }
                validate_custom_command_case_result_against_contract(&case, &result)?;
                match result.step {
                    CustomCommandReplayStep::Success => {
                        let mutation = self.persist_success_result(
                            &case,
                            &case_key,
                            &result,
                            run_audit.as_ref(),
                        )?;
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
                        if let Some(audit) = run_audit.as_ref() {
                            if audit.executed {
                                summary.executed_runs = summary.executed_runs.saturating_add(1);
                            }
                            apply_run_failure_reason_to_summary(&mut summary, audit);
                        }
                        self.persist_non_success_result(
                            &case,
                            &case_key,
                            &result,
                            run_audit.as_ref(),
                        )?;
                        self.record_processed_case(&case_key);
                        break;
                    }
                    CustomCommandReplayStep::RetryableFailure => {
                        summary.retryable_failures = summary.retryable_failures.saturating_add(1);
                        if attempt >= self.config.retry_max_attempts {
                            summary.failed_cases = summary.failed_cases.saturating_add(1);
                            if let Some(audit) = run_audit.as_ref() {
                                if audit.executed {
                                    summary.executed_runs = summary.executed_runs.saturating_add(1);
                                }
                                apply_run_failure_reason_to_summary(&mut summary, audit);
                            }
                            self.persist_non_success_result(
                                &case,
                                &case_key,
                                &result,
                                run_audit.as_ref(),
                            )?;
                            break;
                        }
                        summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                        if let Some(audit) = run_audit.as_ref() {
                            apply_run_failure_reason_to_summary(&mut summary, audit);
                        }
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

    async fn execute_run_operation(
        &self,
        case: &CustomCommandContractCase,
    ) -> CustomCommandRunOutcome {
        let command_name = case.command_name.trim();
        let existing_record = self
            .state
            .commands
            .iter()
            .find(|record| record.command_name == command_name);
        let effective_policy = case
            .execution_policy
            .clone()
            .or_else(|| existing_record.map(|record| record.execution_policy.clone()))
            .unwrap_or_else(|| self.config.default_execution_policy.clone());
        let template = if !case.template.trim().is_empty() {
            case.template.trim().to_string()
        } else {
            existing_record
                .map(|record| record.template.clone())
                .unwrap_or_default()
        };
        let mut audit = CustomCommandRunAudit::default();

        let arguments = match case.arguments.as_object() {
            Some(map) => map,
            None => {
                audit.failure_reason = "invalid_arguments".to_string();
                return CustomCommandRunOutcome {
                    result: CustomCommandReplayResult {
                        step: CustomCommandReplayStep::MalformedInput,
                        status_code: 422,
                        error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE.to_string()),
                        response_body: json!({"status":"rejected","reason":"invalid_arguments"}),
                    },
                    audit: Some(audit),
                };
            }
        };

        if template.trim().is_empty() {
            audit.failure_reason = "missing_command".to_string();
            return CustomCommandRunOutcome {
                result: CustomCommandReplayResult {
                    step: CustomCommandReplayStep::MalformedInput,
                    status_code: 404,
                    error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_NAME.to_string()),
                    response_body: json!({"status":"rejected","reason":"command_not_found","command_name":command_name}),
                },
                audit: Some(audit),
            };
        }

        if validate_custom_command_template_and_arguments(
            &template,
            &case.arguments,
            &effective_policy,
        )
        .is_err()
        {
            audit.failure_reason = "policy_denied".to_string();
            return CustomCommandRunOutcome {
                result: CustomCommandReplayResult {
                    step: CustomCommandReplayStep::MalformedInput,
                    status_code: 403,
                    error_code: Some(CUSTOM_COMMAND_ERROR_POLICY_DENIED.to_string()),
                    response_body: json!({"status":"rejected","reason":"policy_denied"}),
                },
                audit: Some(audit),
            };
        }

        let rendered_command = match render_command_template(template.as_str(), arguments) {
            Ok(rendered) => rendered,
            Err(_) => {
                audit.failure_reason = "template_render_failed".to_string();
                return CustomCommandRunOutcome {
                    result: CustomCommandReplayResult {
                        step: CustomCommandReplayStep::MalformedInput,
                        status_code: 422,
                        error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE.to_string()),
                        response_body: json!({"status":"rejected","reason":"invalid_template"}),
                    },
                    audit: Some(audit),
                };
            }
        };
        audit.rendered_command = rendered_command.clone();

        let tokens = match shell_words::split(rendered_command.as_str()) {
            Ok(parsed) if !parsed.is_empty() => parsed,
            _ => {
                audit.failure_reason = "template_tokenization_failed".to_string();
                return CustomCommandRunOutcome {
                    result: CustomCommandReplayResult {
                        step: CustomCommandReplayStep::MalformedInput,
                        status_code: 422,
                        error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE.to_string()),
                        response_body: json!({"status":"rejected","reason":"invalid_template"}),
                    },
                    audit: Some(audit),
                };
            }
        };

        if !effective_policy.allow_shell && is_shell_program(tokens[0].as_str()) {
            audit.failure_reason = "policy_denied".to_string();
            return CustomCommandRunOutcome {
                result: CustomCommandReplayResult {
                    step: CustomCommandReplayStep::MalformedInput,
                    status_code: 403,
                    error_code: Some(CUSTOM_COMMAND_ERROR_POLICY_DENIED.to_string()),
                    response_body: json!({"status":"rejected","reason":"shell_program_disallowed"}),
                },
                audit: Some(audit),
            };
        }

        let mut command = Command::new(tokens[0].as_str());
        command.args(tokens.iter().skip(1).map(String::as_str));
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);
        apply_policy_environment(&mut command, arguments, &effective_policy);

        let started = Instant::now();
        let timeout_ms = if self.config.run_timeout_ms == 0 {
            DEFAULT_CUSTOM_COMMAND_RUN_TIMEOUT_MS
        } else {
            self.config.run_timeout_ms
        };
        let output_result =
            tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), command.output()).await;
        match output_result {
            Ok(Ok(output)) => {
                audit.executed = true;
                audit.duration_ms =
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                audit.exit_code = output.status.code();
                audit.stdout = truncate_command_output(&output.stdout);
                audit.stderr = truncate_command_output(&output.stderr);
                if output.status.success() {
                    return CustomCommandRunOutcome {
                        result: CustomCommandReplayResult {
                            step: CustomCommandReplayStep::Success,
                            status_code: 200,
                            error_code: None,
                            response_body: json!({
                                "status": "accepted",
                                "operation": "run",
                                "command_name": command_name,
                                "arguments": case.arguments,
                            }),
                        },
                        audit: Some(audit),
                    };
                }
                audit.failure_reason = "non_zero_exit".to_string();
                CustomCommandRunOutcome {
                    result: CustomCommandReplayResult {
                        step: CustomCommandReplayStep::RetryableFailure,
                        status_code: 503,
                        error_code: Some(CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE.to_string()),
                        response_body: json!({"status":"retryable","reason":"non_zero_exit","command_name":command_name}),
                    },
                    audit: Some(audit),
                }
            }
            Ok(Err(error)) => {
                let is_not_found = error.kind() == std::io::ErrorKind::NotFound;
                audit.failure_reason = "spawn_error".to_string();
                audit.stderr = error.to_string();
                audit.duration_ms =
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                if is_not_found {
                    return CustomCommandRunOutcome {
                        result: CustomCommandReplayResult {
                            step: CustomCommandReplayStep::MalformedInput,
                            status_code: 422,
                            error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE.to_string()),
                            response_body: json!({"status":"rejected","reason":"executable_not_found","command_name":command_name}),
                        },
                        audit: Some(audit),
                    };
                }
                CustomCommandRunOutcome {
                    result: CustomCommandReplayResult {
                        step: CustomCommandReplayStep::RetryableFailure,
                        status_code: 503,
                        error_code: Some(CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE.to_string()),
                        response_body: json!({"status":"retryable","reason":"spawn_error","command_name":command_name}),
                    },
                    audit: Some(audit),
                }
            }
            Err(_) => {
                audit.executed = true;
                audit.timed_out = true;
                audit.failure_reason = "timeout".to_string();
                audit.duration_ms =
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                CustomCommandRunOutcome {
                    result: CustomCommandReplayResult {
                        step: CustomCommandReplayStep::RetryableFailure,
                        status_code: 503,
                        error_code: Some(CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE.to_string()),
                        response_body: json!({"status":"retryable","reason":"timeout","command_name":command_name}),
                    },
                    audit: Some(audit),
                }
            }
        }
    }

    fn persist_success_result(
        &mut self,
        case: &CustomCommandContractCase,
        case_key: &str,
        result: &CustomCommandReplayResult,
        run_audit: Option<&CustomCommandRunAudit>,
    ) -> Result<CustomCommandMutationCounts> {
        let operation = normalize_operation(&case.operation);
        let command_name = case.command_name.trim().to_string();
        let timestamp_unix_ms = current_unix_timestamp_ms();
        let effective_policy = case
            .execution_policy
            .clone()
            .unwrap_or_else(|| self.config.default_execution_policy.clone());
        let mut mutation = CustomCommandMutationCounts::default();

        match operation.as_str() {
            "CREATE" | "UPDATE" => {
                let record = CustomCommandRecord {
                    case_key: case_key.to_string(),
                    case_id: case.case_id.clone(),
                    command_name: command_name.clone(),
                    template: case.template.trim().to_string(),
                    execution_policy: effective_policy.clone(),
                    operation: operation.clone(),
                    last_status_code: result.status_code,
                    last_outcome: "success".to_string(),
                    run_count: self
                        .state
                        .commands
                        .iter()
                        .find(|existing| existing.command_name == command_name)
                        .map_or(0, |existing| existing.run_count),
                    last_error_code: String::new(),
                    last_command_line: String::new(),
                    last_exit_code: None,
                    last_stdout: String::new(),
                    last_stderr: String::new(),
                    last_timed_out: false,
                    last_duration_ms: 0,
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
                    existing.execution_policy = effective_policy.clone();
                    existing.operation = operation.clone();
                    existing.last_status_code = result.status_code;
                    existing.last_outcome = "success".to_string();
                    existing.last_error_code = String::new();
                    existing.run_count = existing.run_count.saturating_add(1);
                    existing.updated_unix_ms = timestamp_unix_ms;
                    apply_run_audit_to_record(existing, run_audit);
                } else {
                    let mut record = CustomCommandRecord {
                        case_key: case_key.to_string(),
                        case_id: case.case_id.clone(),
                        command_name: command_name.clone(),
                        template: String::new(),
                        execution_policy: effective_policy.clone(),
                        operation: operation.clone(),
                        last_status_code: result.status_code,
                        last_outcome: "success".to_string(),
                        run_count: 1,
                        last_error_code: String::new(),
                        last_command_line: String::new(),
                        last_exit_code: None,
                        last_stdout: String::new(),
                        last_stderr: String::new(),
                        last_timed_out: false,
                        last_duration_ms: 0,
                        updated_unix_ms: timestamp_unix_ms,
                    };
                    apply_run_audit_to_record(&mut record, run_audit);
                    self.state.commands.push(record);
                    mutation.upserted_commands = 1;
                }
                mutation.executed_runs = usize::from(run_audit.is_some_and(|audit| audit.executed));
            }
            "LIST" => {}
            _ => {}
        }

        self.state
            .commands
            .sort_by(|left, right| left.command_name.cmp(&right.command_name));

        if let Some(store) = self.scope_channel_store(case)? {
            let mut payload = serde_json::Map::new();
            payload.insert("outcome".to_string(), json!("success"));
            payload.insert(
                "operation".to_string(),
                json!(operation.to_ascii_lowercase()),
            );
            payload.insert("case_id".to_string(), json!(case.case_id));
            payload.insert("command_name".to_string(), json!(command_name));
            payload.insert("status_code".to_string(), json!(result.status_code));
            payload.insert(
                "upserted_commands".to_string(),
                json!(mutation.upserted_commands),
            );
            payload.insert(
                "deleted_commands".to_string(),
                json!(mutation.deleted_commands),
            );
            payload.insert("executed_runs".to_string(), json!(mutation.executed_runs));
            if let Some(audit) = run_audit {
                payload.insert(
                    "run_audit".to_string(),
                    json!({
                        "executed": audit.executed,
                        "command_line": audit.rendered_command,
                        "exit_code": audit.exit_code,
                        "stdout": audit.stdout,
                        "stderr": audit.stderr,
                        "timed_out": audit.timed_out,
                        "duration_ms": audit.duration_ms
                    }),
                );
            }

            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-custom-command-runner".to_string(),
                payload: Value::Object(payload),
            })?;

            let run_suffix = match run_audit {
                Some(audit) if audit.executed => format!(
                    " exit_code={:?} timed_out={} duration_ms={}",
                    audit.exit_code, audit.timed_out, audit.duration_ms
                ),
                _ => String::new(),
            };
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "custom-command case {} applied operation={} command={} status={}{}",
                    case.case_id,
                    operation.to_ascii_lowercase(),
                    channel_id_for_case(case),
                    result.status_code,
                    run_suffix
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
        &mut self,
        case: &CustomCommandContractCase,
        case_key: &str,
        result: &CustomCommandReplayResult,
        run_audit: Option<&CustomCommandRunAudit>,
    ) -> Result<()> {
        let operation = normalize_operation(&case.operation);
        let command_name = case.command_name.trim().to_string();
        let timestamp_unix_ms = current_unix_timestamp_ms();
        if operation == "RUN" && !command_name.is_empty() && run_audit.is_some() {
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
                existing.last_outcome = outcome_name(result.step).to_string();
                existing.last_error_code = result.error_code.clone().unwrap_or_default();
                if run_audit.is_some_and(|audit| audit.executed) {
                    existing.run_count = existing.run_count.saturating_add(1);
                }
                existing.updated_unix_ms = timestamp_unix_ms;
                apply_run_audit_to_record(existing, run_audit);
            } else {
                let mut record = CustomCommandRecord {
                    case_key: case_key.to_string(),
                    case_id: case.case_id.clone(),
                    command_name: command_name.clone(),
                    template: case.template.trim().to_string(),
                    execution_policy: case
                        .execution_policy
                        .clone()
                        .unwrap_or_else(|| self.config.default_execution_policy.clone()),
                    operation: operation.clone(),
                    last_status_code: result.status_code,
                    last_outcome: outcome_name(result.step).to_string(),
                    run_count: usize::from(run_audit.is_some_and(|audit| audit.executed)) as u64,
                    last_error_code: result.error_code.clone().unwrap_or_default(),
                    last_command_line: String::new(),
                    last_exit_code: None,
                    last_stdout: String::new(),
                    last_stderr: String::new(),
                    last_timed_out: false,
                    last_duration_ms: 0,
                    updated_unix_ms: timestamp_unix_ms,
                };
                apply_run_audit_to_record(&mut record, run_audit);
                self.state.commands.push(record);
                self.state
                    .commands
                    .sort_by(|left, right| left.command_name.cmp(&right.command_name));
            }
        }

        if let Some(store) = self.scope_channel_store(case)? {
            let outcome = outcome_name(result.step);
            let mut payload = serde_json::Map::new();
            payload.insert("outcome".to_string(), json!(outcome));
            payload.insert("case_id".to_string(), json!(case.case_id));
            payload.insert(
                "operation".to_string(),
                json!(operation.to_ascii_lowercase()),
            );
            payload.insert("command_name".to_string(), json!(case.command_name.trim()));
            payload.insert("status_code".to_string(), json!(result.status_code));
            payload.insert(
                "error_code".to_string(),
                json!(result.error_code.clone().unwrap_or_default()),
            );
            if let Some(audit) = run_audit {
                payload.insert(
                    "run_audit".to_string(),
                    json!({
                        "executed": audit.executed,
                        "command_line": audit.rendered_command,
                        "exit_code": audit.exit_code,
                        "stdout": audit.stdout,
                        "stderr": audit.stderr,
                        "timed_out": audit.timed_out,
                        "duration_ms": audit.duration_ms,
                        "failure_reason": audit.failure_reason
                    }),
                );
            }
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-custom-command-runner".to_string(),
                payload: Value::Object(payload),
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
            store.write_memory(&render_custom_command_snapshot(
                &self.state.commands,
                &channel_id_for_case(case),
            ))?;
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

fn apply_run_audit_to_record(
    record: &mut CustomCommandRecord,
    run_audit: Option<&CustomCommandRunAudit>,
) {
    if let Some(audit) = run_audit {
        record.last_command_line = audit.rendered_command.clone();
        record.last_exit_code = audit.exit_code;
        record.last_stdout = audit.stdout.clone();
        record.last_stderr = audit.stderr.clone();
        record.last_timed_out = audit.timed_out;
        record.last_duration_ms = audit.duration_ms;
    }
}

fn apply_run_failure_reason_to_summary(
    summary: &mut CustomCommandRuntimeSummary,
    run_audit: &CustomCommandRunAudit,
) {
    match run_audit.failure_reason.as_str() {
        "timeout" => {
            summary.run_timeout_failures = summary.run_timeout_failures.saturating_add(1);
        }
        "non_zero_exit" => {
            summary.run_non_zero_exit_failures =
                summary.run_non_zero_exit_failures.saturating_add(1);
        }
        "spawn_error" => {
            summary.run_spawn_failures = summary.run_spawn_failures.saturating_add(1);
        }
        "missing_command" => {
            summary.run_missing_command_failures =
                summary.run_missing_command_failures.saturating_add(1);
        }
        _ => {}
    }
}

fn apply_policy_environment(
    command: &mut Command,
    arguments: &Map<String, Value>,
    policy: &CustomCommandExecutionPolicy,
) {
    let sandbox_profile = policy.sandbox_profile.trim().to_ascii_lowercase();
    let restricted_mode = sandbox_profile == "restricted" || sandbox_profile == "workspace_write";
    if restricted_mode {
        command.env_clear();
        if let Ok(path_value) = std::env::var("PATH") {
            if !path_value.trim().is_empty() {
                command.env("PATH", path_value);
            }
        }
    }

    let allow_all = policy.allowed_env.is_empty();
    let allowed: HashSet<String> = policy
        .allowed_env
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect();
    let denied: HashSet<String> = policy
        .denied_env
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect();

    for (key, value) in arguments {
        if !is_valid_env_key(key) {
            continue;
        }
        let normalized = key.trim().to_ascii_lowercase();
        if denied.contains(&normalized) {
            continue;
        }
        if !allow_all && !allowed.contains(&normalized) {
            continue;
        }
        if let Some(env_value) = value_to_template_scalar(value) {
            command.env(key, env_value);
        }
    }
}

fn is_shell_program(program: &str) -> bool {
    let normalized = program.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
    )
}

fn render_command_template(template: &str, arguments: &Map<String, Value>) -> Result<String> {
    let mut rendered = String::with_capacity(template.len());
    let mut start_index = 0usize;
    while let Some(open_rel) = template[start_index..].find("{{") {
        let open = start_index.saturating_add(open_rel);
        rendered.push_str(&template[start_index..open]);

        let close_search_start = open.saturating_add(2);
        let Some(close_rel) = template[close_search_start..].find("}}") else {
            anyhow::bail!("custom command template has unterminated placeholder");
        };
        let close = close_search_start.saturating_add(close_rel);
        let placeholder = template[close_search_start..close].trim();
        if placeholder.is_empty() {
            anyhow::bail!("custom command template contains empty placeholder");
        }
        let Some(value) = arguments.get(placeholder) else {
            anyhow::bail!(
                "custom command template placeholder '{}' is missing from arguments",
                placeholder
            );
        };
        if let Some(value) = value_to_template_scalar(value) {
            rendered.push_str(value.as_str());
        } else {
            anyhow::bail!(
                "custom command template placeholder '{}' has unsupported null value",
                placeholder
            );
        }
        start_index = close.saturating_add(2);
    }
    rendered.push_str(&template[start_index..]);
    Ok(rendered.trim().to_string())
}

fn value_to_template_scalar(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Bool(inner) => Some(inner.to_string()),
        Value::Number(inner) => Some(inner.to_string()),
        Value::String(inner) => Some(inner.clone()),
        Value::Array(_) | Value::Object(_) => Some(value.to_string()),
    }
}

fn truncate_command_output(raw: &[u8]) -> String {
    if raw.is_empty() {
        return String::new();
    }
    if raw.len() <= CUSTOM_COMMAND_RUN_OUTPUT_MAX_BYTES {
        return String::from_utf8_lossy(raw).to_string();
    }
    let truncated = &raw[..CUSTOM_COMMAND_RUN_OUTPUT_MAX_BYTES];
    let suffix = format!("...<truncated:{} bytes>", raw.len() - truncated.len());
    format!("{}{}", String::from_utf8_lossy(truncated), suffix)
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
    if summary.run_timeout_failures > 0 {
        codes.push("command_run_timeout_observed".to_string());
    }
    if summary.run_non_zero_exit_failures > 0 {
        codes.push("command_run_non_zero_exit_observed".to_string());
    }
    if summary.run_spawn_failures > 0 {
        codes.push("command_run_spawn_failures_observed".to_string());
    }
    if summary.run_missing_command_failures > 0 {
        codes.push("command_run_missing_command_observed".to_string());
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
        run_timeout_failures: summary.run_timeout_failures,
        run_non_zero_exit_failures: summary.run_non_zero_exit_failures,
        run_spawn_failures: summary.run_spawn_failures,
        run_missing_command_failures: summary.run_missing_command_failures,
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
        let allowed_env = if record.execution_policy.allowed_env.is_empty() {
            "none".to_string()
        } else {
            record.execution_policy.allowed_env.join(",")
        };
        let denied_env = if record.execution_policy.denied_env.is_empty() {
            "none".to_string()
        } else {
            record.execution_policy.denied_env.join(",")
        };
        lines.push(format!(
            "- {} op={} status={} runs={} template={} policy=approval:{}|shell:{}|network:{}|sandbox:{}|allow_env:{}|deny_env:{} run=exit:{:?}|timeout:{}|duration_ms:{}|error_code:{}|stdout_len:{}|stderr_len:{}",
            record.command_name,
            record.operation.to_ascii_lowercase(),
            record.last_status_code,
            record.run_count,
            record.template,
            record.execution_policy.require_approval,
            record.execution_policy.allow_shell,
            record.execution_policy.allow_network,
            record.execution_policy.sandbox_profile,
            allowed_env,
            denied_env,
            record.last_exit_code,
            record.last_timed_out,
            record.last_duration_ms,
            record.last_error_code,
            record.last_stdout.len(),
            record.last_stderr.len()
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
        DEFAULT_CUSTOM_COMMAND_RUN_TIMEOUT_MS,
    };
    use crate::custom_command_contract::{
        load_custom_command_contract_fixture, parse_custom_command_contract_fixture,
    };
    use crate::custom_command_policy::default_custom_command_execution_policy;
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
            run_timeout_ms: DEFAULT_CUSTOM_COMMAND_RUN_TIMEOUT_MS,
            default_execution_policy: default_custom_command_execution_policy(),
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
    async fn integration_runner_rollout_pass_fixture_executes_all_cases_successfully() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path("rollout-pass.json");
        let fixture = load_custom_command_contract_fixture(&config.fixture_path)
            .expect("fixture should load");
        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 4);
        assert_eq!(summary.queued_cases, 4);
        assert_eq!(summary.applied_cases, 4);
        assert_eq!(summary.malformed_cases, 0);
        assert_eq!(summary.retryable_failures, 0);
        assert_eq!(summary.retry_attempts, 0);
        assert_eq!(summary.failed_cases, 0);
        assert_eq!(summary.upserted_commands, 2);
        assert_eq!(summary.deleted_commands, 0);
        assert_eq!(summary.executed_runs, 1);
        assert_eq!(summary.run_timeout_failures, 0);
        assert_eq!(summary.run_non_zero_exit_failures, 0);
        assert_eq!(summary.run_spawn_failures, 0);
        assert_eq!(summary.run_missing_command_failures, 0);
        assert_eq!(summary.duplicate_skips, 0);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert_eq!(state.commands.len(), 1);
        assert_eq!(state.processed_case_keys.len(), 4);
        assert_eq!(state.health.last_cycle_discovered, 4);
        assert_eq!(state.health.last_cycle_failed, 0);
        assert_eq!(state.health.failure_streak, 0);
        assert_eq!(state.health.classify().state, TransportHealthState::Healthy);

        let record = state.commands.first().expect("command record");
        assert_eq!(record.command_name, "deploy_release");
        assert_eq!(record.run_count, 1);
        assert_eq!(record.last_outcome, "success");
        assert_eq!(record.last_error_code, "");
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
    async fn regression_runner_default_policy_rejects_unsafe_create_template() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "policy-deny-template",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-policy-deny",
      "operation": "create",
      "command_name": "deploy_release",
      "template": "deploy {{env}} && curl https://example.invalid",
      "arguments": {"env":"prod"},
      "expected": {
        "outcome": "malformed_input",
        "status_code": 403,
        "error_code": "custom_command_policy_denied",
        "response_body": {"status":"rejected","reason":"policy_denied"}
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");
        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.discovered_cases, 1);
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.malformed_cases, 1);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        assert!(state.commands.is_empty());
    }

    #[tokio::test]
    async fn integration_runner_executes_real_command_and_persists_run_audit() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "run-success-real-command",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-create-rustc",
      "operation": "create",
      "command_name": "rustc_version",
      "template": "rustc --version",
      "arguments": {},
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {
          "status":"accepted",
          "operation":"create",
          "command_name":"rustc_version"
        }
      }
    },
    {
      "schema_version": 1,
      "case_id": "custom-command-run-rustc",
      "operation": "run",
      "command_name": "rustc_version",
      "template": "",
      "arguments": {},
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status":"accepted",
          "operation":"run",
          "command_name":"rustc_version",
          "arguments": {}
        }
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");

        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.applied_cases, 2);
        assert_eq!(summary.executed_runs, 1);
        assert_eq!(summary.failed_cases, 0);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        let record = state
            .commands
            .iter()
            .find(|command| command.command_name == "rustc_version")
            .expect("run record should exist");
        assert_eq!(record.last_status_code, 200);
        assert_eq!(record.last_outcome, "success");
        assert_eq!(record.last_exit_code, Some(0));
        assert!(!record.last_timed_out);
        assert!(record.last_stdout.to_ascii_lowercase().contains("rustc"));
        assert!(record.last_command_line.contains("rustc --version"));
        assert!(record.last_error_code.is_empty());

        let channel_store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "custom-command",
            "rustc_version",
        )
        .expect("open channel store");
        let logs = channel_store.load_log_entries().expect("load channel logs");
        assert!(logs
            .iter()
            .any(|entry| entry.payload.to_string().contains("run_audit")));

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("command_runs_recorded"));
    }

    #[tokio::test]
    async fn regression_runner_non_zero_exit_triggers_retryable_failure_and_audit() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 2;
        let fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "run-non-zero",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-create-bad-rustc",
      "operation": "create",
      "command_name": "bad_rustc",
      "template": "rustc --definitely-invalid-flag",
      "arguments": {},
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {
          "status":"accepted",
          "operation":"create",
          "command_name":"bad_rustc"
        }
      }
    },
    {
      "schema_version": 1,
      "case_id": "custom-command-run-bad-rustc",
      "operation": "run",
      "command_name": "bad_rustc",
      "template": "",
      "arguments": {},
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "custom_command_backend_unavailable",
        "response_body": {
          "status":"retryable",
          "reason":"non_zero_exit",
          "command_name":"bad_rustc"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");

        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.executed_runs, 1);
        assert!(summary.run_non_zero_exit_failures >= 1);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        let record = state
            .commands
            .iter()
            .find(|command| command.command_name == "bad_rustc")
            .expect("failure record should exist");
        assert_eq!(record.last_status_code, 503);
        assert_eq!(record.last_outcome, "retryable_failure");
        assert_eq!(
            record.last_error_code,
            "custom_command_backend_unavailable".to_string()
        );
        assert!(record.last_exit_code.is_some());
        assert!(!record.last_timed_out);
        assert!(record
            .last_command_line
            .contains("rustc --definitely-invalid-flag"));

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("command_run_non_zero_exit_observed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_runner_timeout_records_failure_reason_and_timeout_flag() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.retry_max_attempts = 1;
        config.run_timeout_ms = 10;
        let fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "run-timeout",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-create-sleeper",
      "operation": "create",
      "command_name": "sleeper",
      "template": "sleep 1",
      "arguments": {},
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {
          "status":"accepted",
          "operation":"create",
          "command_name":"sleeper"
        }
      }
    },
    {
      "schema_version": 1,
      "case_id": "custom-command-run-sleeper",
      "operation": "run",
      "command_name": "sleeper",
      "template": "",
      "arguments": {},
      "expected": {
        "outcome": "retryable_failure",
        "status_code": 503,
        "error_code": "custom_command_backend_unavailable",
        "response_body": {
          "status":"retryable",
          "reason":"timeout",
          "command_name":"sleeper"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");

        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.executed_runs, 1);
        assert_eq!(summary.run_timeout_failures, 1);

        let state = load_custom_command_runtime_state(&config.state_dir.join("state.json"))
            .expect("load state");
        let record = state
            .commands
            .iter()
            .find(|command| command.command_name == "sleeper")
            .expect("timeout record should exist");
        assert_eq!(record.last_status_code, 503);
        assert!(record.last_timed_out);
        assert_eq!(record.last_exit_code, None);
        assert_eq!(record.last_error_code, "custom_command_backend_unavailable");

        let events_log = std::fs::read_to_string(
            config
                .state_dir
                .join(CUSTOM_COMMAND_RUNTIME_EVENTS_LOG_FILE),
        )
        .expect("read runtime events");
        assert!(events_log.contains("command_run_timeout_observed"));
    }

    #[tokio::test]
    async fn regression_runner_missing_command_is_reported_as_malformed() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "run-missing-command",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-run-missing",
      "operation": "run",
      "command_name": "missing_command",
      "template": "",
      "arguments": {},
      "expected": {
        "outcome": "malformed_input",
        "status_code": 404,
        "error_code": "custom_command_invalid_name",
        "response_body": {
          "status":"rejected",
          "reason":"command_not_found",
          "command_name":"missing_command"
        }
      }
    }
  ]
}"#,
        )
        .expect("parse fixture");

        let mut runtime = CustomCommandRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.executed_runs, 0);
        assert_eq!(summary.run_missing_command_failures, 1);
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
