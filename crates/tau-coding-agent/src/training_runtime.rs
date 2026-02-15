//! Prompt-optimization runtime wiring for rollout execution and SQLite persistence.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::{io::Write, path::PathBuf};
use tau_access::{
    enforce_rl_lifecycle_action_with_policy_path, resolve_local_principal, rl_lifecycle_action_key,
    RlLifecycleAction,
};
use tau_agent_core::{AgentConfig, SafetyMode};
use tau_ai::{LlmClient, ModelRef};
use tau_cli::{Cli, CliPromptSanitizerMode};
use tau_onboarding::startup_local_runtime::{build_local_runtime_agent, LocalRuntimeAgentSettings};
use tau_trainer::checkpoint_store::load_policy_checkpoint;
use tau_trainer::{Trainer, TrainerConfig};
use tau_training_runner::TauAgentExecutor;
use tau_training_store::{SqliteTrainingStore, TrainingStore};

use crate::model_catalog::ModelCatalog;
use crate::tools::ToolPolicy;

const TRAINING_STATUS_SCHEMA_VERSION: u32 = 1;
const TRAINING_STATUS_FILE: &str = "status.json";
const TRAINING_CONTROL_STATE_SCHEMA_VERSION: u32 = 1;
const TRAINING_CONTROL_STATE_FILE: &str = "control-state.json";
const TRAINING_CONTROL_AUDIT_FILE: &str = "control-audit.jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptOptimizationControlAction {
    Status,
    Pause,
    Resume,
    Cancel,
    Rollback,
}

impl PromptOptimizationControlAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
            Self::Rollback => "rollback",
        }
    }

    fn as_rl_lifecycle_action(self) -> RlLifecycleAction {
        match self {
            Self::Status => RlLifecycleAction::Status,
            Self::Pause => RlLifecycleAction::Pause,
            Self::Resume => RlLifecycleAction::Resume,
            Self::Cancel => RlLifecycleAction::Cancel,
            Self::Rollback => RlLifecycleAction::Rollback,
        }
    }
}

fn resolve_safety_mode(mode: CliPromptSanitizerMode) -> SafetyMode {
    match mode {
        CliPromptSanitizerMode::Warn => SafetyMode::Warn,
        CliPromptSanitizerMode::Redact => SafetyMode::Redact,
        CliPromptSanitizerMode::Block => SafetyMode::Block,
    }
}

#[derive(Debug, Deserialize)]
struct TrainingConfigFile {
    #[serde(default)]
    #[serde(alias = "train")]
    optimize: Vec<Value>,
    #[serde(default)]
    #[serde(alias = "val")]
    validate: Vec<Value>,
    #[serde(default)]
    resources: HashMap<String, Value>,
    #[serde(default)]
    worker_count: Option<usize>,
    #[serde(default)]
    poll_interval_ms: Option<u64>,
    #[serde(default)]
    heartbeat_interval_ms: Option<u64>,
    #[serde(default)]
    completion_poll_interval_ms: Option<u64>,
    #[serde(default)]
    completion_timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TrainingRunReport {
    model_ref: String,
    store_path: String,
    total_rollouts: usize,
    succeeded: usize,
    failed: usize,
    cancelled: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrainingStatusFile {
    schema_version: u32,
    updated_unix_ms: u64,
    run_state: String,
    model_ref: String,
    store_path: String,
    total_rollouts: usize,
    succeeded: usize,
    failed: usize,
    cancelled: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct TrainingControlStateFile {
    schema_version: u32,
    updated_unix_ms: u64,
    lifecycle_state: String,
    last_action: String,
    principal: String,
    rollback_checkpoint: Option<String>,
}

#[derive(Debug, Serialize)]
struct TrainingControlAuditRecord {
    schema_version: u32,
    timestamp_unix_ms: u64,
    principal: String,
    action: String,
    action_key: String,
    lifecycle_state: String,
    idempotent: bool,
    rollback_checkpoint: Option<String>,
}

#[derive(Debug, Serialize)]
struct TrainingControlStatusReport {
    schema_version: u32,
    state_dir: String,
    control_state_path: String,
    control_audit_path: String,
    action: String,
    principal: String,
    idempotent: Option<bool>,
    rollback_checkpoint: Option<String>,
    checkpoint_run_id: Option<String>,
    control_state: Option<TrainingControlStateFile>,
    training_status: Option<TrainingStatusFile>,
}

pub(crate) async fn run_prompt_optimization_mode_if_requested(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    model_catalog: &ModelCatalog,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> Result<bool> {
    let Some(config_path) = cli.prompt_optimization_config.as_ref() else {
        return Ok(false);
    };

    validate_training_cli_request(cli)?;
    let config = load_training_config(config_path)?;
    if config.optimize.is_empty() && config.validate.is_empty() {
        bail!(
            "prompt-optimization config '{}' must contain at least one rollout in 'optimize' or 'validate'",
            config_path.display()
        );
    }

    let store_path = cli.prompt_optimization_store_sqlite.clone();
    let store: Arc<dyn TrainingStore> =
        Arc::new(SqliteTrainingStore::new(&store_path).with_context(|| {
            format!(
                "failed to initialize sqlite store '{}'",
                store_path.display()
            )
        })?);
    if !config.resources.is_empty() {
        store
            .update_resources(config.resources.clone())
            .await
            .context("failed to seed initial training resources")?;
    }

    let trainer_config = build_trainer_config(&config)?;
    let trainer = Trainer::new(store, trainer_config);
    let executor = build_executor(
        cli,
        client,
        model_ref,
        model_catalog,
        system_prompt,
        tool_policy,
    );

    let train_dataset = (!config.optimize.is_empty()).then_some(config.optimize);
    let val_dataset = (!config.validate.is_empty()).then_some(config.validate);
    let summary = trainer
        .fit(executor, train_dataset, val_dataset)
        .await
        .context("prompt-optimization run failed")?;

    let report = TrainingRunReport {
        model_ref: format!("{}/{}", model_ref.provider.as_str(), model_ref.model),
        store_path: store_path.display().to_string(),
        total_rollouts: summary.total_rollouts,
        succeeded: summary.succeeded,
        failed: summary.failed,
        cancelled: summary.cancelled,
    };
    persist_training_status_report(&report, &store_path)?;
    print_training_report(&report, cli.prompt_optimization_json)?;
    Ok(true)
}

pub(crate) fn execute_prompt_optimization_control_command(cli: &Cli) -> Result<()> {
    if !prompt_optimization_control_mode_requested(cli) {
        return Ok(());
    }

    validate_prompt_optimization_control_cli_request(cli)?;
    let (action, rollback_checkpoint_path) = resolve_prompt_optimization_control_action(cli)?;
    let principal = resolve_prompt_optimization_control_principal(cli)?;
    enforce_rl_lifecycle_action_with_policy_path(
        &principal,
        action.as_rl_lifecycle_action(),
        &cli.prompt_optimization_control_rbac_policy,
    )
    .with_context(|| {
        format!(
            "failed to authorize prompt-optimization lifecycle control action '{}'",
            action.as_str()
        )
    })?;

    let state_dir = cli.prompt_optimization_control_state_dir.as_path();
    std::fs::create_dir_all(state_dir).with_context(|| {
        format!(
            "failed to create control state dir '{}'",
            state_dir.display()
        )
    })?;
    let control_state_path = state_dir.join(TRAINING_CONTROL_STATE_FILE);
    let control_audit_path = state_dir.join(TRAINING_CONTROL_AUDIT_FILE);
    let training_status_path = state_dir.join(TRAINING_STATUS_FILE);

    let existing_control_state = load_training_control_state(&control_state_path)?;
    let training_status = load_training_status_file(&training_status_path)?;

    if action == PromptOptimizationControlAction::Status {
        print_training_control_report(
            &TrainingControlStatusReport {
                schema_version: TRAINING_CONTROL_STATE_SCHEMA_VERSION,
                state_dir: state_dir.display().to_string(),
                control_state_path: control_state_path.display().to_string(),
                control_audit_path: control_audit_path.display().to_string(),
                action: action.as_str().to_string(),
                principal,
                idempotent: None,
                rollback_checkpoint: None,
                checkpoint_run_id: None,
                control_state: existing_control_state,
                training_status,
            },
            cli.prompt_optimization_control_json,
        )?;
        return Ok(());
    }

    let now_unix_ms = tau_core::current_unix_timestamp_ms();
    let mut checkpoint_run_id = None;
    let rollback_checkpoint = if action == PromptOptimizationControlAction::Rollback {
        let checkpoint_path = rollback_checkpoint_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "--prompt-optimization-control-rollback requires a checkpoint payload path"
            )
        })?;
        let checkpoint = load_policy_checkpoint(checkpoint_path).with_context(|| {
            format!(
                "failed to load rollback checkpoint '{}'",
                checkpoint_path.display()
            )
        })?;
        checkpoint_run_id = Some(checkpoint.run_id);
        Some(checkpoint_path.display().to_string())
    } else {
        None
    };

    let mut next_state =
        existing_control_state.unwrap_or_else(|| default_training_control_state(&principal));
    let target_lifecycle_state = lifecycle_state_for_control_action(action);
    let idempotent = next_state.lifecycle_state == target_lifecycle_state
        && next_state.rollback_checkpoint == rollback_checkpoint;

    next_state.schema_version = TRAINING_CONTROL_STATE_SCHEMA_VERSION;
    next_state.updated_unix_ms = now_unix_ms;
    next_state.lifecycle_state = target_lifecycle_state.to_string();
    next_state.last_action = action.as_str().to_string();
    next_state.principal = principal.clone();
    next_state.rollback_checkpoint = rollback_checkpoint.clone();

    persist_training_control_state(&control_state_path, &next_state)?;
    append_training_control_audit(
        &control_audit_path,
        &TrainingControlAuditRecord {
            schema_version: TRAINING_CONTROL_STATE_SCHEMA_VERSION,
            timestamp_unix_ms: now_unix_ms,
            principal: principal.clone(),
            action: action.as_str().to_string(),
            action_key: rl_lifecycle_action_key(action.as_rl_lifecycle_action()).to_string(),
            lifecycle_state: target_lifecycle_state.to_string(),
            idempotent,
            rollback_checkpoint: rollback_checkpoint.clone(),
        },
    )?;

    print_training_control_report(
        &TrainingControlStatusReport {
            schema_version: TRAINING_CONTROL_STATE_SCHEMA_VERSION,
            state_dir: state_dir.display().to_string(),
            control_state_path: control_state_path.display().to_string(),
            control_audit_path: control_audit_path.display().to_string(),
            action: action.as_str().to_string(),
            principal,
            idempotent: Some(idempotent),
            rollback_checkpoint,
            checkpoint_run_id,
            control_state: Some(next_state),
            training_status,
        },
        cli.prompt_optimization_control_json,
    )?;

    Ok(())
}

fn prompt_optimization_control_mode_requested(cli: &Cli) -> bool {
    cli.prompt_optimization_control_status
        || cli.prompt_optimization_control_pause
        || cli.prompt_optimization_control_resume
        || cli.prompt_optimization_control_cancel
        || cli.prompt_optimization_control_rollback.is_some()
}

fn validate_prompt_optimization_control_cli_request(cli: &Cli) -> Result<()> {
    let requested_action_count = usize::from(cli.prompt_optimization_control_status)
        + usize::from(cli.prompt_optimization_control_pause)
        + usize::from(cli.prompt_optimization_control_resume)
        + usize::from(cli.prompt_optimization_control_cancel)
        + usize::from(cli.prompt_optimization_control_rollback.is_some());

    if requested_action_count != 1 {
        bail!(
            "prompt-optimization control mode requires exactly one action: --prompt-optimization-control-status, --prompt-optimization-control-pause, --prompt-optimization-control-resume, --prompt-optimization-control-cancel, or --prompt-optimization-control-rollback <path>"
        );
    }

    let has_prompt_or_command_input = cli.prompt.is_some()
        || cli.prompt_file.is_some()
        || cli.prompt_template_file.is_some()
        || cli.command_file.is_some();
    if has_prompt_or_command_input {
        bail!(
            "prompt-optimization control commands cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }

    if cli.prompt_optimization_config.is_some() {
        bail!(
            "prompt-optimization control commands cannot be combined with --prompt-optimization-config"
        );
    }

    if cli.prompt_optimization_proxy_server {
        bail!(
            "prompt-optimization control commands cannot be combined with --prompt-optimization-proxy-server"
        );
    }

    let has_other_runtime_mode = cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.gateway_openresponses_server;
    if has_other_runtime_mode {
        bail!(
            "prompt-optimization control commands cannot be combined with active transport/runtime modes"
        );
    }

    Ok(())
}

fn resolve_prompt_optimization_control_action(
    cli: &Cli,
) -> Result<(PromptOptimizationControlAction, Option<PathBuf>)> {
    if cli.prompt_optimization_control_status {
        return Ok((PromptOptimizationControlAction::Status, None));
    }
    if cli.prompt_optimization_control_pause {
        return Ok((PromptOptimizationControlAction::Pause, None));
    }
    if cli.prompt_optimization_control_resume {
        return Ok((PromptOptimizationControlAction::Resume, None));
    }
    if cli.prompt_optimization_control_cancel {
        return Ok((PromptOptimizationControlAction::Cancel, None));
    }
    if let Some(path) = cli.prompt_optimization_control_rollback.as_ref() {
        return Ok((
            PromptOptimizationControlAction::Rollback,
            Some(path.clone()),
        ));
    }
    bail!(
        "prompt-optimization control action not set; expected one of status|pause|resume|cancel|rollback"
    )
}

fn resolve_prompt_optimization_control_principal(cli: &Cli) -> Result<String> {
    if let Some(principal) = cli
        .prompt_optimization_control_principal
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(principal.to_string());
    }
    Ok(resolve_local_principal())
}

fn lifecycle_state_for_control_action(action: PromptOptimizationControlAction) -> &'static str {
    match action {
        PromptOptimizationControlAction::Status => "unknown",
        PromptOptimizationControlAction::Pause => "paused",
        PromptOptimizationControlAction::Resume => "running",
        PromptOptimizationControlAction::Cancel => "cancelled",
        PromptOptimizationControlAction::Rollback => "rollback_requested",
    }
}

fn default_training_control_state(principal: &str) -> TrainingControlStateFile {
    TrainingControlStateFile {
        schema_version: TRAINING_CONTROL_STATE_SCHEMA_VERSION,
        updated_unix_ms: tau_core::current_unix_timestamp_ms(),
        lifecycle_state: "running".to_string(),
        last_action: "status".to_string(),
        principal: principal.to_string(),
        rollback_checkpoint: None,
    }
}

fn load_training_status_file(path: &Path) -> Result<Option<TrainingStatusFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read training status file '{}'", path.display()))?;
    let status = serde_json::from_str::<TrainingStatusFile>(&raw)
        .with_context(|| format!("failed to parse training status file '{}'", path.display()))?;
    if status.schema_version != TRAINING_STATUS_SCHEMA_VERSION {
        bail!(
            "unsupported training status schema version {} (expected {})",
            status.schema_version,
            TRAINING_STATUS_SCHEMA_VERSION
        );
    }
    Ok(Some(status))
}

fn load_training_control_state(path: &Path) -> Result<Option<TrainingControlStateFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read training control state '{}'", path.display()))?;
    let state = serde_json::from_str::<TrainingControlStateFile>(&raw).with_context(|| {
        format!(
            "failed to parse training control state '{}'",
            path.display()
        )
    })?;
    if state.schema_version != TRAINING_CONTROL_STATE_SCHEMA_VERSION {
        bail!(
            "unsupported training control schema version {} (expected {})",
            state.schema_version,
            TRAINING_CONTROL_STATE_SCHEMA_VERSION
        );
    }
    Ok(Some(state))
}

fn persist_training_control_state(path: &Path, state: &TrainingControlStateFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create training control state parent '{}'",
                parent.display()
            )
        })?;
    }
    let encoded =
        serde_json::to_string_pretty(state).context("failed to encode training control state")?;
    std::fs::write(path, encoded).with_context(|| {
        format!(
            "failed to write training control state '{}'",
            path.display()
        )
    })?;
    Ok(())
}

fn append_training_control_audit(path: &Path, record: &TrainingControlAuditRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create training control audit parent '{}'",
                parent.display()
            )
        })?;
    }
    let line =
        serde_json::to_string(record).context("failed to encode training control audit record")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open training control audit '{}'", path.display()))?;
    writeln!(file, "{line}").with_context(|| {
        format!(
            "failed to append training control audit '{}'",
            path.display()
        )
    })?;
    Ok(())
}

fn print_training_control_report(
    report: &TrainingControlStatusReport,
    as_json: bool,
) -> Result<()> {
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(report)
                .context("failed to encode training control report JSON")?
        );
        return Ok(());
    }

    let lifecycle_state = report
        .control_state
        .as_ref()
        .map(|state| state.lifecycle_state.as_str())
        .unwrap_or("unknown");
    let training_state = report
        .training_status
        .as_ref()
        .map(|status| status.run_state.as_str())
        .unwrap_or("missing");
    let idempotent = report
        .idempotent
        .map(|value| value.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    let rollback_checkpoint = report.rollback_checkpoint.as_deref().unwrap_or("none");
    let checkpoint_run_id = report.checkpoint_run_id.as_deref().unwrap_or("none");
    println!(
        "prompt optimization lifecycle control: action={} principal={} lifecycle_state={} training_run_state={} idempotent={} rollback_checkpoint={} checkpoint_run_id={} state_dir={} state_file={} audit_file={}",
        report.action,
        report.principal,
        lifecycle_state,
        training_state,
        idempotent,
        rollback_checkpoint,
        checkpoint_run_id,
        report.state_dir,
        report.control_state_path,
        report.control_audit_path
    );
    Ok(())
}

fn persist_training_status_report(report: &TrainingRunReport, store_path: &Path) -> Result<()> {
    let training_root = store_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    std::fs::create_dir_all(&training_root).with_context(|| {
        format!(
            "failed to create training status directory '{}'",
            training_root.display()
        )
    })?;

    let status_payload = TrainingStatusFile {
        schema_version: TRAINING_STATUS_SCHEMA_VERSION,
        updated_unix_ms: tau_core::current_unix_timestamp_ms(),
        run_state: "completed".to_string(),
        model_ref: report.model_ref.clone(),
        store_path: report.store_path.clone(),
        total_rollouts: report.total_rollouts,
        succeeded: report.succeeded,
        failed: report.failed,
        cancelled: report.cancelled,
    };
    let status_path = training_root.join(TRAINING_STATUS_FILE);
    let encoded = serde_json::to_string_pretty(&status_payload)
        .context("failed to serialize training status payload")?;
    std::fs::write(&status_path, encoded).with_context(|| {
        format!(
            "failed to write training status file '{}'",
            status_path.display()
        )
    })?;
    Ok(())
}

fn load_training_config(path: &Path) -> Result<TrainingConfigFile> {
    let payload = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read prompt-optimization config '{}'",
            path.display()
        )
    })?;
    serde_json::from_str::<TrainingConfigFile>(&payload).with_context(|| {
        format!(
            "failed to parse prompt-optimization config '{}': expected JSON object",
            path.display()
        )
    })
}

fn validate_training_cli_request(cli: &Cli) -> Result<()> {
    let has_prompt_or_command_input = cli.prompt.is_some()
        || cli.prompt_file.is_some()
        || cli.prompt_template_file.is_some()
        || cli.command_file.is_some();
    if has_prompt_or_command_input {
        bail!(
            "--prompt-optimization-config cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }

    let has_transport_mode = cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.gateway_openresponses_server;
    if has_transport_mode {
        bail!(
            "--prompt-optimization-config cannot be combined with transport or contract runner modes"
        );
    }

    let has_preflight_mode = cli.onboard
        || cli.qa_loop
        || cli.mcp_server
        || cli.rpc_capabilities
        || cli.rpc_validate_frame_file.is_some()
        || cli.rpc_dispatch_frame_file.is_some()
        || cli.rpc_dispatch_ndjson_file.is_some()
        || cli.rpc_serve_ndjson
        || cli.package_activate
        || cli.package_validate.is_some()
        || cli.package_show.is_some()
        || cli.package_install.is_some()
        || cli.package_update.is_some()
        || cli.package_list
        || cli.package_remove.is_some()
        || cli.package_rollback.is_some()
        || cli.package_conflicts
        || cli.project_index_build
        || cli.project_index_query.is_some()
        || cli.project_index_inspect;
    if has_preflight_mode {
        bail!(
            "--prompt-optimization-config cannot be combined with preflight/maintenance command modes"
        );
    }

    Ok(())
}

fn build_trainer_config(config: &TrainingConfigFile) -> Result<TrainerConfig> {
    let mut trainer_config = TrainerConfig::default();

    if let Some(worker_count) = config.worker_count {
        if worker_count == 0 {
            bail!("training config field 'worker_count' must be greater than 0");
        }
        trainer_config.worker_count = worker_count;
    }

    if let Some(poll_interval_ms) = config.poll_interval_ms {
        if poll_interval_ms == 0 {
            bail!("training config field 'poll_interval_ms' must be greater than 0");
        }
        trainer_config.poll_interval = Duration::from_millis(poll_interval_ms);
    }

    if let Some(heartbeat_interval_ms) = config.heartbeat_interval_ms {
        if heartbeat_interval_ms == 0 {
            bail!("training config field 'heartbeat_interval_ms' must be greater than 0");
        }
        trainer_config.heartbeat_interval = Duration::from_millis(heartbeat_interval_ms);
    }

    if let Some(completion_poll_interval_ms) = config.completion_poll_interval_ms {
        if completion_poll_interval_ms == 0 {
            bail!("training config field 'completion_poll_interval_ms' must be greater than 0");
        }
        trainer_config.completion_poll_interval =
            Duration::from_millis(completion_poll_interval_ms);
    }

    if let Some(completion_timeout_secs) = config.completion_timeout_secs {
        if completion_timeout_secs == 0 {
            bail!("training config field 'completion_timeout_secs' must be greater than 0");
        }
        trainer_config.completion_timeout = Duration::from_secs(completion_timeout_secs);
    }

    Ok(trainer_config)
}

fn build_executor(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    model_catalog: &ModelCatalog,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> Arc<TauAgentExecutor> {
    let agent_defaults = AgentConfig::default();
    let model_catalog_entry = model_catalog.find_model_ref(model_ref);
    let settings = LocalRuntimeAgentSettings {
        max_turns: cli.max_turns,
        max_parallel_tool_calls: cli.agent_max_parallel_tool_calls,
        max_context_messages: cli.agent_max_context_messages,
        request_max_retries: cli.agent_request_max_retries,
        request_retry_initial_backoff_ms: cli.agent_request_retry_initial_backoff_ms,
        request_retry_max_backoff_ms: cli.agent_request_retry_max_backoff_ms,
        request_timeout_ms: agent_defaults.request_timeout_ms,
        tool_timeout_ms: agent_defaults.tool_timeout_ms,
        model_input_cost_per_million: model_catalog_entry
            .and_then(|entry| entry.input_cost_per_million),
        model_output_cost_per_million: model_catalog_entry
            .and_then(|entry| entry.output_cost_per_million),
        cost_budget_usd: cli.agent_cost_budget_usd,
        cost_alert_thresholds_percent: cli.agent_cost_alert_threshold_percent.clone(),
        prompt_sanitizer_enabled: cli.prompt_sanitizer_enabled,
        prompt_sanitizer_mode: resolve_safety_mode(cli.prompt_sanitizer_mode),
        prompt_sanitizer_redaction_token: cli.prompt_sanitizer_redaction_token.clone(),
        secret_leak_detector_enabled: cli.secret_leak_detector_enabled,
        secret_leak_detector_mode: resolve_safety_mode(cli.secret_leak_detector_mode),
        secret_leak_redaction_token: cli.secret_leak_redaction_token.clone(),
    };

    let base_system_prompt = system_prompt.to_string();
    let model_ref = model_ref.clone();
    let tool_policy = tool_policy.clone();

    Arc::new(TauAgentExecutor::new(move |resources| {
        let effective_system_prompt = resources
            .and_then(|snapshot| snapshot.resources.get("system_prompt"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| base_system_prompt.clone());
        build_local_runtime_agent(
            client.clone(),
            &model_ref,
            &effective_system_prompt,
            settings.clone(),
            tool_policy.clone(),
        )
    }))
}

fn print_training_report(report: &TrainingRunReport, as_json: bool) -> Result<()> {
    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(report)
                .context("failed to encode training summary JSON")?
        );
    } else {
        println!(
            "prompt optimization complete: model={} store={} total={} succeeded={} failed={} cancelled={}",
            report.model_ref,
            report.store_path,
            report.total_rollouts,
            report.succeeded,
            report.failed,
            report.cancelled
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_trainer_config, execute_prompt_optimization_control_command,
        run_prompt_optimization_mode_if_requested, TrainingConfigFile, TRAINING_CONTROL_AUDIT_FILE,
        TRAINING_CONTROL_STATE_FILE,
    };
    use crate::model_catalog::ModelCatalog;
    use crate::tools::ToolPolicy;
    use async_trait::async_trait;
    use clap::Parser;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, ModelRef, TauAiError};
    use tau_cli::Cli;
    use tau_trainer::checkpoint_store::{save_policy_checkpoint, PolicyCheckpoint};
    use tau_training_store::{RolloutQuery, RolloutStatus, SqliteTrainingStore, TrainingStore};
    use tempfile::tempdir;
    use tokio::time::sleep;

    fn parse_cli_with_stack(args: &[&str]) -> Cli {
        let owned_args: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(owned_args))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    struct MockClient;

    #[async_trait]
    impl LlmClient for MockClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("mock-response"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct SlowClient;

    #[async_trait]
    impl LlmClient for SlowClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            sleep(Duration::from_secs(2)).await;
            Ok(ChatResponse {
                message: Message::assistant_text("slow-response"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    fn write_rbac_policy(path: &std::path::Path, payload: &serde_json::Value) {
        std::fs::write(path, format!("{payload}\n")).expect("write rbac policy");
    }

    fn checkpoint_payload() -> PolicyCheckpoint {
        PolicyCheckpoint {
            checkpoint_version: 1,
            run_id: "run-checkpoint-1".to_string(),
            policy_state: json!({ "weights": [0.1, 0.2] }),
            optimizer_state: json!({ "lr": 0.0003 }),
            global_step: 12,
            optimizer_step: 12,
            saved_at_unix_seconds: 1_760_000_000,
        }
    }

    #[test]
    fn unit_build_trainer_config_applies_overrides() {
        let config = TrainingConfigFile {
            optimize: vec![json!({ "prompt": "one" })],
            validate: Vec::new(),
            resources: HashMap::new(),
            worker_count: Some(3),
            poll_interval_ms: Some(25),
            heartbeat_interval_ms: Some(300),
            completion_poll_interval_ms: Some(45),
            completion_timeout_secs: Some(4),
        };

        let trainer_config = build_trainer_config(&config).expect("build trainer config");
        assert_eq!(trainer_config.worker_count, 3);
        assert_eq!(trainer_config.poll_interval.as_millis(), 25);
        assert_eq!(trainer_config.heartbeat_interval.as_millis(), 300);
        assert_eq!(trainer_config.completion_poll_interval.as_millis(), 45);
        assert_eq!(trainer_config.completion_timeout.as_secs(), 4);
    }

    #[tokio::test]
    async fn integration_prompt_optimization_mode_executes_rollouts_and_persists_sqlite() {
        let temp = tempdir().expect("create tempdir");
        let config_path = temp.path().join("prompt-optimization.json");
        let store_path = temp.path().join("train.sqlite");
        let config_payload = json!({
            "optimize": [
                { "prompt": "hello", "expected": "mock-response" }
            ],
            "worker_count": 1,
            "completion_timeout_secs": 5
        });
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&config_payload).expect("encode config"),
        )
        .expect("write config");

        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_config = Some(config_path.clone());
        cli.prompt_optimization_store_sqlite = store_path.clone();
        cli.prompt_optimization_json = true;

        let handled = run_prompt_optimization_mode_if_requested(
            &cli,
            Arc::new(MockClient),
            &ModelRef::parse("openai/gpt-4o-mini").expect("parse model"),
            &ModelCatalog::built_in(),
            "You are helpful.",
            &ToolPolicy::new(vec![temp.path().to_path_buf()]),
        )
        .await
        .expect("run prompt optimization mode");
        assert!(handled);

        let store = SqliteTrainingStore::new(&store_path).expect("open sqlite store");
        let rollouts = store
            .query_rollouts(RolloutQuery {
                statuses: Some(vec![RolloutStatus::Succeeded]),
                ..RolloutQuery::default()
            })
            .await
            .expect("query succeeded rollouts");
        assert_eq!(rollouts.len(), 1);

        let status_path = store_path
            .parent()
            .expect("store path parent")
            .join("status.json");
        let status_raw = std::fs::read_to_string(&status_path).expect("read status file");
        let status_json: serde_json::Value =
            serde_json::from_str(&status_raw).expect("parse status payload");
        assert_eq!(
            status_json["schema_version"],
            serde_json::Value::from(1_u64)
        );
        assert_eq!(
            status_json["run_state"],
            serde_json::Value::String("completed".to_string())
        );
        assert_eq!(
            status_json["total_rollouts"],
            serde_json::Value::from(1_u64)
        );
        assert_eq!(status_json["succeeded"], serde_json::Value::from(1_u64));
    }

    #[test]
    fn regression_prompt_optimization_mode_rejects_prompt_conflicts() {
        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_config = Some(std::path::PathBuf::from("train.json"));
        cli.prompt = Some("hello".to_string());

        let error = super::validate_training_cli_request(&cli)
            .expect_err("prompt + train-config must fail");
        assert!(error
            .to_string()
            .contains("--prompt-optimization-config cannot be combined"));
    }

    #[tokio::test]
    async fn regression_prompt_optimization_mode_surfaces_timeout_failures() {
        let temp = tempdir().expect("create tempdir");
        let config_path = temp.path().join("train-timeout.json");
        let store_path = temp.path().join("train-timeout.sqlite");
        let config_payload = json!({
            "optimize": [
                { "prompt": "timeout" }
            ],
            "worker_count": 1,
            "completion_timeout_secs": 1
        });
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&config_payload).expect("encode timeout config"),
        )
        .expect("write timeout config");

        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_config = Some(config_path);
        cli.prompt_optimization_store_sqlite = store_path;

        let error = run_prompt_optimization_mode_if_requested(
            &cli,
            Arc::new(SlowClient),
            &ModelRef::parse("openai/gpt-4o-mini").expect("parse model"),
            &ModelCatalog::built_in(),
            "You are helpful.",
            &ToolPolicy::new(vec![temp.path().to_path_buf()]),
        )
        .await
        .expect_err("slow prompt optimization should hit completion timeout");
        let message = format!("{error:#}");
        assert!(message.contains("prompt-optimization run failed"));
        assert!(message.contains("timeout waiting for rollouts"));
    }

    #[test]
    fn regression_training_config_supports_legacy_train_val_keys() {
        let config = serde_json::json!({
            "train": [{"prompt": "legacy-train"}],
            "val": [{"prompt": "legacy-val"}],
        });
        let parsed: TrainingConfigFile =
            serde_json::from_value(config).expect("parse legacy train/val config");
        assert_eq!(parsed.optimize.len(), 1);
        assert_eq!(parsed.validate.len(), 1);
    }

    #[test]
    fn functional_prompt_optimization_control_pause_is_idempotent_and_audited() {
        let temp = tempdir().expect("create tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_rbac_policy(
            &policy_path,
            &json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    { "principal": "local:rl-operator", "roles": ["rl-control"] }
                ],
                "roles": {
                    "rl-control": {
                        "allow": ["control:rl:*"]
                    }
                }
            }),
        );

        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_control_pause = true;
        cli.prompt_optimization_control_state_dir = temp.path().join("training");
        cli.prompt_optimization_control_principal = Some("local:rl-operator".to_string());
        cli.prompt_optimization_control_rbac_policy = policy_path;

        execute_prompt_optimization_control_command(&cli).expect("first pause action");
        execute_prompt_optimization_control_command(&cli).expect("second pause action");

        let state_path = cli
            .prompt_optimization_control_state_dir
            .join(TRAINING_CONTROL_STATE_FILE);
        let state_raw = std::fs::read_to_string(&state_path).expect("read control state");
        let state_json: serde_json::Value =
            serde_json::from_str(&state_raw).expect("parse control state");
        assert_eq!(state_json["lifecycle_state"], "paused");
        assert_eq!(state_json["last_action"], "pause");
        assert_eq!(state_json["principal"], "local:rl-operator");

        let audit_path = cli
            .prompt_optimization_control_state_dir
            .join(TRAINING_CONTROL_AUDIT_FILE);
        let audit_lines = std::fs::read_to_string(&audit_path)
            .expect("read control audit")
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert_eq!(audit_lines.len(), 2);
        let first: serde_json::Value =
            serde_json::from_str(&audit_lines[0]).expect("parse first audit row");
        let second: serde_json::Value =
            serde_json::from_str(&audit_lines[1]).expect("parse second audit row");
        assert_eq!(first["action"], "pause");
        assert_eq!(first["idempotent"], false);
        assert_eq!(second["action"], "pause");
        assert_eq!(second["idempotent"], true);
    }

    #[test]
    fn regression_prompt_optimization_control_blocks_unauthorized_action() {
        let temp = tempdir().expect("create tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_rbac_policy(
            &policy_path,
            &json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    { "principal": "local:rl-viewer", "roles": ["rl-view"] }
                ],
                "roles": {
                    "rl-view": {
                        "allow": ["control:rl:status"]
                    }
                }
            }),
        );

        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_control_pause = true;
        cli.prompt_optimization_control_state_dir = temp.path().join("training");
        cli.prompt_optimization_control_principal = Some("local:rl-viewer".to_string());
        cli.prompt_optimization_control_rbac_policy = policy_path;

        let error = execute_prompt_optimization_control_command(&cli)
            .expect_err("unauthorized pause should fail");
        let message = format!("{error:#}");
        assert!(message.contains("unauthorized rl lifecycle action"));
        assert!(message.contains("action=control:rl:pause"));
    }

    #[test]
    fn functional_prompt_optimization_control_rollback_persists_checkpoint_target() {
        let temp = tempdir().expect("create tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_rbac_policy(
            &policy_path,
            &json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    { "principal": "local:rl-operator", "roles": ["rl-control"] }
                ],
                "roles": {
                    "rl-control": {
                        "allow": ["control:rl:*"]
                    }
                }
            }),
        );
        let checkpoint_path = temp.path().join("checkpoint.json");
        save_policy_checkpoint(&checkpoint_path, &checkpoint_payload()).expect("save checkpoint");

        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_control_rollback = Some(checkpoint_path.clone());
        cli.prompt_optimization_control_state_dir = temp.path().join("training");
        cli.prompt_optimization_control_principal = Some("local:rl-operator".to_string());
        cli.prompt_optimization_control_rbac_policy = policy_path;

        execute_prompt_optimization_control_command(&cli).expect("rollback command");

        let state_path = cli
            .prompt_optimization_control_state_dir
            .join(TRAINING_CONTROL_STATE_FILE);
        let state_raw = std::fs::read_to_string(&state_path).expect("read control state");
        let state_json: serde_json::Value =
            serde_json::from_str(&state_raw).expect("parse control state");
        assert_eq!(state_json["lifecycle_state"], "rollback_requested");
        assert_eq!(
            state_json["rollback_checkpoint"],
            checkpoint_path.display().to_string()
        );

        let audit_path = cli
            .prompt_optimization_control_state_dir
            .join(TRAINING_CONTROL_AUDIT_FILE);
        let audit_lines = std::fs::read_to_string(&audit_path)
            .expect("read control audit")
            .lines()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert_eq!(audit_lines.len(), 1);
        let row: serde_json::Value =
            serde_json::from_str(&audit_lines[0]).expect("parse audit row");
        assert_eq!(row["action"], "rollback");
        assert_eq!(
            row["rollback_checkpoint"],
            checkpoint_path.display().to_string()
        );
    }

    #[test]
    fn regression_prompt_optimization_control_rollback_rejects_invalid_checkpoint_payload() {
        let temp = tempdir().expect("create tempdir");
        let policy_path = temp.path().join("rbac.json");
        write_rbac_policy(
            &policy_path,
            &json!({
                "schema_version": 1,
                "team_mode": true,
                "bindings": [
                    { "principal": "local:rl-operator", "roles": ["rl-control"] }
                ],
                "roles": {
                    "rl-control": {
                        "allow": ["control:rl:*"]
                    }
                }
            }),
        );
        let checkpoint_path = temp.path().join("invalid-checkpoint.json");
        std::fs::write(&checkpoint_path, "{\"checkpoint_version\":1}\n")
            .expect("write invalid checkpoint");

        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.prompt_optimization_control_rollback = Some(checkpoint_path.clone());
        cli.prompt_optimization_control_state_dir = temp.path().join("training");
        cli.prompt_optimization_control_principal = Some("local:rl-operator".to_string());
        cli.prompt_optimization_control_rbac_policy = policy_path;

        let error = execute_prompt_optimization_control_command(&cli)
            .expect_err("invalid checkpoint should fail closed");
        let message = format!("{error:#}");
        assert!(message.contains("failed to load rollback checkpoint"));
    }
}
