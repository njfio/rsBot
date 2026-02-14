//! Training-mode runtime wiring for rollout execution and SQLite persistence.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tau_agent_core::AgentConfig;
use tau_ai::{LlmClient, ModelRef};
use tau_cli::Cli;
use tau_onboarding::startup_local_runtime::{build_local_runtime_agent, LocalRuntimeAgentSettings};
use tau_trainer::{Trainer, TrainerConfig};
use tau_training_runner::TauAgentExecutor;
use tau_training_store::{SqliteTrainingStore, TrainingStore};

use crate::model_catalog::ModelCatalog;
use crate::tools::ToolPolicy;

const TRAINING_STATUS_SCHEMA_VERSION: u32 = 1;
const TRAINING_STATUS_FILE: &str = "status.json";

#[derive(Debug, Deserialize)]
struct TrainingConfigFile {
    #[serde(default)]
    train: Vec<Value>,
    #[serde(default)]
    val: Vec<Value>,
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

pub(crate) async fn run_training_mode_if_requested(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    model_catalog: &ModelCatalog,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> Result<bool> {
    let Some(config_path) = cli.train_config.as_ref() else {
        return Ok(false);
    };

    validate_training_cli_request(cli)?;
    let config = load_training_config(config_path)?;
    if config.train.is_empty() && config.val.is_empty() {
        bail!(
            "training config '{}' must contain at least one rollout in 'train' or 'val'",
            config_path.display()
        );
    }

    let store_path = cli.train_store_sqlite.clone();
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

    let train_dataset = (!config.train.is_empty()).then_some(config.train);
    let val_dataset = (!config.val.is_empty()).then_some(config.val);
    let summary = trainer
        .fit(executor, train_dataset, val_dataset)
        .await
        .context("training run failed")?;

    let report = TrainingRunReport {
        model_ref: format!("{}/{}", model_ref.provider.as_str(), model_ref.model),
        store_path: store_path.display().to_string(),
        total_rollouts: summary.total_rollouts,
        succeeded: summary.succeeded,
        failed: summary.failed,
        cancelled: summary.cancelled,
    };
    persist_training_status_report(&report, &store_path)?;
    print_training_report(&report, cli.train_json)?;
    Ok(true)
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
    let payload = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read training config '{}'", path.display()))?;
    serde_json::from_str::<TrainingConfigFile>(&payload).with_context(|| {
        format!(
            "failed to parse training config '{}': expected JSON object",
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
            "--train-config cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
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
        bail!("--train-config cannot be combined with transport or contract runner modes");
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
        bail!("--train-config cannot be combined with preflight/maintenance command modes");
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
            "training complete: model={} store={} total={} succeeded={} failed={} cancelled={}",
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
    use super::{build_trainer_config, run_training_mode_if_requested, TrainingConfigFile};
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

    #[test]
    fn unit_build_trainer_config_applies_overrides() {
        let config = TrainingConfigFile {
            train: vec![json!({ "prompt": "one" })],
            val: Vec::new(),
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
    async fn integration_training_mode_executes_rollouts_and_persists_sqlite() {
        let temp = tempdir().expect("create tempdir");
        let config_path = temp.path().join("train.json");
        let store_path = temp.path().join("train.sqlite");
        let config_payload = json!({
            "train": [
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
        cli.train_config = Some(config_path.clone());
        cli.train_store_sqlite = store_path.clone();
        cli.train_json = true;

        let handled = run_training_mode_if_requested(
            &cli,
            Arc::new(MockClient),
            &ModelRef::parse("openai/gpt-4o-mini").expect("parse model"),
            &ModelCatalog::built_in(),
            "You are helpful.",
            &ToolPolicy::new(vec![temp.path().to_path_buf()]),
        )
        .await
        .expect("run training mode");
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
    fn regression_training_mode_rejects_prompt_conflicts() {
        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.train_config = Some(std::path::PathBuf::from("train.json"));
        cli.prompt = Some("hello".to_string());

        let error = super::validate_training_cli_request(&cli)
            .expect_err("prompt + train-config must fail");
        assert!(error
            .to_string()
            .contains("--train-config cannot be combined"));
    }

    #[tokio::test]
    async fn regression_training_mode_surfaces_timeout_failures() {
        let temp = tempdir().expect("create tempdir");
        let config_path = temp.path().join("train-timeout.json");
        let store_path = temp.path().join("train-timeout.sqlite");
        let config_payload = json!({
            "train": [
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
        cli.train_config = Some(config_path);
        cli.train_store_sqlite = store_path;

        let error = run_training_mode_if_requested(
            &cli,
            Arc::new(SlowClient),
            &ModelRef::parse("openai/gpt-4o-mini").expect("parse model"),
            &ModelCatalog::built_in(),
            "You are helpful.",
            &ToolPolicy::new(vec![temp.path().to_path_buf()]),
        )
        .await
        .expect_err("slow training should hit completion timeout");
        let message = format!("{error:#}");
        assert!(message.contains("training run failed"));
        assert!(message.contains("timeout waiting for rollouts"));
    }
}
