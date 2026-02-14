use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tau_ai::{LlmClient, ModelRef};
use tau_browser_automation::browser_automation_contract::load_browser_automation_contract_fixture;
use tau_browser_automation::browser_automation_live::{
    run_browser_automation_live_fixture, BrowserAutomationLivePolicy, BrowserSessionManager,
    PlaywrightCliActionExecutor,
};
use tau_cli::validation::{
    validate_browser_automation_contract_runner_cli, validate_browser_automation_live_runner_cli,
    validate_custom_command_contract_runner_cli, validate_dashboard_contract_runner_cli,
    validate_deployment_contract_runner_cli, validate_events_runner_cli,
    validate_gateway_contract_runner_cli, validate_gateway_openresponses_server_cli,
    validate_github_issues_bridge_cli, validate_memory_contract_runner_cli,
    validate_multi_agent_contract_runner_cli, validate_multi_channel_contract_runner_cli,
    validate_multi_channel_live_connectors_runner_cli, validate_multi_channel_live_runner_cli,
    validate_slack_bridge_cli, validate_voice_contract_runner_cli, validate_voice_live_runner_cli,
};
use tau_cli::Cli;
use tau_cli::CliGatewayOpenResponsesAuthMode;
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_custom_command::custom_command_policy::{
    default_custom_command_execution_policy, normalize_sandbox_profile,
    CustomCommandExecutionPolicy,
};
use tau_custom_command::custom_command_runtime::{
    run_custom_command_contract_runner, CustomCommandRuntimeConfig,
};
use tau_dashboard::dashboard_runtime::{run_dashboard_contract_runner, DashboardRuntimeConfig};
use tau_deployment::deployment_runtime::{run_deployment_contract_runner, DeploymentRuntimeConfig};
use tau_diagnostics::{build_doctor_command_config, DoctorCommandConfig};
use tau_gateway::{
    GatewayOpenResponsesAuthMode, GatewayOpenResponsesServerConfig, GatewayRuntimeConfig,
    GatewayToolRegistrarFn,
};
use tau_memory::memory_runtime::{run_memory_contract_runner, MemoryRuntimeConfig};
use tau_multi_channel::{
    MultiChannelCommandHandlers, MultiChannelLiveConnectorsConfig, MultiChannelLiveRuntimeConfig,
    MultiChannelMediaUnderstandingConfig, MultiChannelOutboundConfig, MultiChannelPairingEvaluator,
    MultiChannelRuntimeConfig, MultiChannelTelemetryConfig,
};
use tau_ops::TransportHealthSnapshot;
use tau_orchestrator::multi_agent_runtime::MultiAgentRuntimeConfig;
use tau_provider::{
    load_credential_store, resolve_credential_store_encryption_mode, AuthCommandConfig,
};
use tau_skills::default_skills_lock_path;
use tau_tools::tools::{register_builtin_tools, ToolPolicy};
use tau_voice::voice_runtime::{
    run_voice_contract_runner, run_voice_live_runner, VoiceLiveRuntimeConfig, VoiceRuntimeConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `MultiChannelTransportMode` values.
pub enum MultiChannelTransportMode {
    None,
    ContractRunner,
    LiveRunner,
    LiveConnectorsRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `BridgeTransportMode` values.
pub enum BridgeTransportMode {
    None,
    GithubIssuesBridge,
    SlackBridge,
    EventsRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ContractTransportMode` values.
pub enum ContractTransportMode {
    None,
    MultiAgent,
    BrowserAutomationLive,
    Memory,
    Dashboard,
    Gateway,
    Deployment,
    CustomCommand,
    Voice,
    VoiceLive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `TransportRuntimeMode` values.
pub enum TransportRuntimeMode {
    None,
    GatewayOpenResponsesServer,
    GithubIssuesBridge,
    SlackBridge,
    EventsRunner,
    MultiChannelContractRunner,
    MultiChannelLiveRunner,
    MultiChannelLiveConnectorsRunner,
    MultiAgentContractRunner,
    BrowserAutomationLiveRunner,
    MemoryContractRunner,
    DashboardContractRunner,
    GatewayContractRunner,
    DeploymentContractRunner,
    CustomCommandContractRunner,
    VoiceContractRunner,
    VoiceLiveRunner,
}

#[async_trait::async_trait]
/// Trait contract for `TransportRuntimeExecutor` behavior.
pub trait TransportRuntimeExecutor {
    async fn run_gateway_openresponses_server(&self) -> Result<()>;
    async fn run_github_issues_bridge(&self) -> Result<()>;
    async fn run_slack_bridge(&self) -> Result<()>;
    async fn run_events_runner(&self) -> Result<()>;
    async fn run_multi_channel_contract_runner(&self) -> Result<()>;
    async fn run_multi_channel_live_runner(&self) -> Result<()>;
    async fn run_multi_channel_live_connectors_runner(&self) -> Result<()>;
    async fn run_multi_agent_contract_runner(&self) -> Result<()>;
    async fn run_browser_automation_live_runner(&self) -> Result<()>;
    async fn run_memory_contract_runner(&self) -> Result<()>;
    async fn run_dashboard_contract_runner(&self) -> Result<()>;
    async fn run_gateway_contract_runner(&self) -> Result<()>;
    async fn run_deployment_contract_runner(&self) -> Result<()>;
    async fn run_custom_command_contract_runner(&self) -> Result<()>;
    async fn run_voice_contract_runner(&self) -> Result<()>;
    async fn run_voice_live_runner(&self) -> Result<()>;
}

pub async fn execute_transport_runtime_mode<E>(
    mode: TransportRuntimeMode,
    executor: &E,
) -> Result<bool>
where
    E: TransportRuntimeExecutor + Sync,
{
    match mode {
        TransportRuntimeMode::GatewayOpenResponsesServer => {
            executor.run_gateway_openresponses_server().await?;
            Ok(true)
        }
        TransportRuntimeMode::GithubIssuesBridge => {
            executor.run_github_issues_bridge().await?;
            Ok(true)
        }
        TransportRuntimeMode::SlackBridge => {
            executor.run_slack_bridge().await?;
            Ok(true)
        }
        TransportRuntimeMode::EventsRunner => {
            executor.run_events_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::MultiChannelContractRunner => {
            executor.run_multi_channel_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::MultiChannelLiveRunner => {
            executor.run_multi_channel_live_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::MultiChannelLiveConnectorsRunner => {
            executor.run_multi_channel_live_connectors_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::MultiAgentContractRunner => {
            executor.run_multi_agent_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::BrowserAutomationLiveRunner => {
            executor.run_browser_automation_live_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::MemoryContractRunner => {
            executor.run_memory_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::DashboardContractRunner => {
            executor.run_dashboard_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::GatewayContractRunner => {
            executor.run_gateway_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::DeploymentContractRunner => {
            executor.run_deployment_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::CustomCommandContractRunner => {
            executor.run_custom_command_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::VoiceContractRunner => {
            executor.run_voice_contract_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::VoiceLiveRunner => {
            executor.run_voice_live_runner().await?;
            Ok(true)
        }
        TransportRuntimeMode::None => Ok(false),
    }
}

pub async fn run_transport_mode_if_requested<E>(cli: &Cli, executor: &E) -> Result<bool>
where
    E: TransportRuntimeExecutor + Sync,
{
    validate_transport_mode_cli(cli)?;
    execute_transport_runtime_mode(resolve_transport_runtime_mode(cli), executor).await
}

pub fn validate_transport_mode_cli(cli: &Cli) -> Result<()> {
    validate_github_issues_bridge_cli(cli)?;
    validate_slack_bridge_cli(cli)?;
    validate_events_runner_cli(cli)?;
    validate_multi_channel_contract_runner_cli(cli)?;
    validate_multi_channel_live_runner_cli(cli)?;
    validate_multi_channel_live_connectors_runner_cli(cli)?;
    validate_multi_agent_contract_runner_cli(cli)?;
    validate_browser_automation_contract_runner_cli(cli)?;
    validate_browser_automation_live_runner_cli(cli)?;
    validate_memory_contract_runner_cli(cli)?;
    validate_dashboard_contract_runner_cli(cli)?;
    validate_gateway_openresponses_server_cli(cli)?;
    validate_gateway_contract_runner_cli(cli)?;
    validate_deployment_contract_runner_cli(cli)?;
    validate_custom_command_contract_runner_cli(cli)?;
    validate_voice_contract_runner_cli(cli)?;
    validate_voice_live_runner_cli(cli)?;
    Ok(())
}

pub fn build_transport_doctor_config(cli: &Cli, model_ref: &ModelRef) -> DoctorCommandConfig {
    let fallback_model_refs = Vec::new();
    let skills_lock_path = default_skills_lock_path(&cli.skills_dir);
    build_doctor_command_config(cli, model_ref, &fallback_model_refs, &skills_lock_path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `TransportRuntimeDefaults` used across Tau components.
pub struct TransportRuntimeDefaults {
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub turn_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub session_lock_wait_ms: u64,
    pub session_lock_stale_ms: u64,
}

pub fn build_transport_runtime_defaults(
    cli: &Cli,
    model_ref: &ModelRef,
    system_prompt: &str,
) -> TransportRuntimeDefaults {
    TransportRuntimeDefaults {
        model: model_ref.model.clone(),
        system_prompt: system_prompt.to_string(),
        max_turns: cli.max_turns,
        turn_timeout_ms: cli.turn_timeout_ms,
        request_timeout_ms: cli.request_timeout_ms,
        session_lock_wait_ms: cli.session_lock_wait_ms,
        session_lock_stale_ms: cli.session_lock_stale_ms,
    }
}

pub fn build_multi_channel_runtime_dependencies<
    THandlers,
    TPairingEvaluator,
    FBuildHandlers,
    FBuildPairing,
>(
    cli: &Cli,
    model_ref: &ModelRef,
    build_command_handlers: FBuildHandlers,
    build_pairing_evaluator: FBuildPairing,
) -> (THandlers, TPairingEvaluator)
where
    FBuildHandlers: FnOnce(AuthCommandConfig, DoctorCommandConfig) -> THandlers,
    FBuildPairing: FnOnce() -> TPairingEvaluator,
{
    let auth_config = crate::startup_config::build_auth_command_config(cli);
    let doctor_config = build_transport_doctor_config(cli, model_ref);
    (
        build_command_handlers(auth_config, doctor_config),
        build_pairing_evaluator(),
    )
}

pub async fn run_multi_channel_contract_runner_with_runtime_dependencies_if_requested<
    FBuildHandlers,
    FBuildPairing,
>(
    cli: &Cli,
    model_ref: &ModelRef,
    build_command_handlers: FBuildHandlers,
    build_pairing_evaluator: FBuildPairing,
) -> Result<bool>
where
    FBuildHandlers: FnOnce(AuthCommandConfig, DoctorCommandConfig) -> MultiChannelCommandHandlers,
    FBuildPairing: FnOnce() -> std::sync::Arc<dyn MultiChannelPairingEvaluator>,
{
    let (command_handlers, pairing_evaluator) = build_multi_channel_runtime_dependencies(
        cli,
        model_ref,
        build_command_handlers,
        build_pairing_evaluator,
    );
    run_multi_channel_contract_runner_if_requested(cli, command_handlers, pairing_evaluator).await
}

pub fn resolve_multi_channel_transport_mode(cli: &Cli) -> MultiChannelTransportMode {
    if cli.multi_channel_contract_runner {
        MultiChannelTransportMode::ContractRunner
    } else if cli.multi_channel_live_runner {
        MultiChannelTransportMode::LiveRunner
    } else if cli.multi_channel_live_connectors_runner {
        MultiChannelTransportMode::LiveConnectorsRunner
    } else {
        MultiChannelTransportMode::None
    }
}

pub fn resolve_bridge_transport_mode(cli: &Cli) -> BridgeTransportMode {
    if cli.github_issues_bridge {
        BridgeTransportMode::GithubIssuesBridge
    } else if cli.slack_bridge {
        BridgeTransportMode::SlackBridge
    } else if cli.events_runner {
        BridgeTransportMode::EventsRunner
    } else {
        BridgeTransportMode::None
    }
}

pub fn resolve_contract_transport_mode(cli: &Cli) -> ContractTransportMode {
    if cli.multi_agent_contract_runner {
        ContractTransportMode::MultiAgent
    } else if cli.browser_automation_live_runner {
        ContractTransportMode::BrowserAutomationLive
    } else if cli.memory_contract_runner {
        ContractTransportMode::Memory
    } else if cli.dashboard_contract_runner {
        ContractTransportMode::Dashboard
    } else if cli.gateway_contract_runner {
        ContractTransportMode::Gateway
    } else if cli.deployment_contract_runner {
        ContractTransportMode::Deployment
    } else if cli.custom_command_contract_runner {
        ContractTransportMode::CustomCommand
    } else if cli.voice_contract_runner {
        ContractTransportMode::Voice
    } else if cli.voice_live_runner {
        ContractTransportMode::VoiceLive
    } else {
        ContractTransportMode::None
    }
}

pub fn resolve_transport_runtime_mode(cli: &Cli) -> TransportRuntimeMode {
    if cli.gateway_openresponses_server {
        return TransportRuntimeMode::GatewayOpenResponsesServer;
    }

    match resolve_bridge_transport_mode(cli) {
        BridgeTransportMode::GithubIssuesBridge => {
            return TransportRuntimeMode::GithubIssuesBridge;
        }
        BridgeTransportMode::SlackBridge => return TransportRuntimeMode::SlackBridge,
        BridgeTransportMode::EventsRunner => return TransportRuntimeMode::EventsRunner,
        BridgeTransportMode::None => {}
    }

    match resolve_multi_channel_transport_mode(cli) {
        MultiChannelTransportMode::ContractRunner => {
            return TransportRuntimeMode::MultiChannelContractRunner;
        }
        MultiChannelTransportMode::LiveRunner => {
            return TransportRuntimeMode::MultiChannelLiveRunner;
        }
        MultiChannelTransportMode::LiveConnectorsRunner => {
            return TransportRuntimeMode::MultiChannelLiveConnectorsRunner;
        }
        MultiChannelTransportMode::None => {}
    }

    match resolve_contract_transport_mode(cli) {
        ContractTransportMode::MultiAgent => return TransportRuntimeMode::MultiAgentContractRunner,
        ContractTransportMode::BrowserAutomationLive => {
            return TransportRuntimeMode::BrowserAutomationLiveRunner;
        }
        ContractTransportMode::Memory => return TransportRuntimeMode::MemoryContractRunner,
        ContractTransportMode::Dashboard => return TransportRuntimeMode::DashboardContractRunner,
        ContractTransportMode::Gateway => return TransportRuntimeMode::GatewayContractRunner,
        ContractTransportMode::Deployment => return TransportRuntimeMode::DeploymentContractRunner,
        ContractTransportMode::CustomCommand => {
            return TransportRuntimeMode::CustomCommandContractRunner;
        }
        ContractTransportMode::Voice => return TransportRuntimeMode::VoiceContractRunner,
        ContractTransportMode::VoiceLive => return TransportRuntimeMode::VoiceLiveRunner,
        ContractTransportMode::None => {}
    }

    TransportRuntimeMode::None
}

pub fn map_gateway_openresponses_auth_mode(
    mode: CliGatewayOpenResponsesAuthMode,
) -> GatewayOpenResponsesAuthMode {
    match mode {
        CliGatewayOpenResponsesAuthMode::Token => GatewayOpenResponsesAuthMode::Token,
        CliGatewayOpenResponsesAuthMode::PasswordSession => {
            GatewayOpenResponsesAuthMode::PasswordSession
        }
        CliGatewayOpenResponsesAuthMode::LocalhostDev => GatewayOpenResponsesAuthMode::LocalhostDev,
    }
}

pub fn resolve_gateway_openresponses_auth(cli: &Cli) -> (Option<String>, Option<String>) {
    let auth_token = resolve_non_empty_cli_value(cli.gateway_openresponses_auth_token.as_deref());
    let auth_password =
        resolve_non_empty_cli_value(cli.gateway_openresponses_auth_password.as_deref());
    (auth_token, auth_password)
}

pub fn build_gateway_openresponses_server_config(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> GatewayOpenResponsesServerConfig {
    let (auth_token, auth_password) = resolve_gateway_openresponses_auth(cli);
    let policy = tool_policy.clone();
    GatewayOpenResponsesServerConfig {
        client,
        model: model_ref.model.clone(),
        system_prompt: system_prompt.to_string(),
        max_turns: cli.max_turns,
        tool_registrar: Arc::new(GatewayToolRegistrarFn::new(move |agent| {
            register_builtin_tools(agent, policy.clone());
        })),
        turn_timeout_ms: cli.turn_timeout_ms,
        session_lock_wait_ms: cli.session_lock_wait_ms,
        session_lock_stale_ms: cli.session_lock_stale_ms,
        state_dir: cli.gateway_state_dir.clone(),
        bind: cli.gateway_openresponses_bind.clone(),
        auth_mode: map_gateway_openresponses_auth_mode(cli.gateway_openresponses_auth_mode),
        auth_token,
        auth_password,
        session_ttl_seconds: cli.gateway_openresponses_session_ttl_seconds,
        rate_limit_window_seconds: cli.gateway_openresponses_rate_limit_window_seconds,
        rate_limit_max_requests: cli.gateway_openresponses_rate_limit_max_requests,
        max_input_chars: cli.gateway_openresponses_max_input_chars,
    }
}

pub async fn run_gateway_openresponses_server_if_requested(
    cli: &Cli,
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
) -> Result<bool> {
    if !cli.gateway_openresponses_server {
        return Ok(false);
    }
    let config = build_gateway_openresponses_server_config(
        cli,
        client,
        model_ref,
        system_prompt,
        tool_policy,
    );
    tau_gateway::run_gateway_openresponses_server(config).await?;
    Ok(true)
}

pub fn build_gateway_contract_runner_config(cli: &Cli) -> GatewayRuntimeConfig {
    GatewayRuntimeConfig {
        fixture_path: cli.gateway_fixture.clone(),
        state_dir: cli.gateway_state_dir.clone(),
        queue_limit: 64,
        processed_case_cap: 10_000,
        retry_max_attempts: 4,
        retry_base_delay_ms: 0,
        guardrail_failure_streak_threshold: cli.gateway_guardrail_failure_streak_threshold.max(1),
        guardrail_retryable_failures_threshold: cli
            .gateway_guardrail_retryable_failures_threshold
            .max(1),
    }
}

pub async fn run_gateway_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.gateway_contract_runner {
        return Ok(false);
    }
    let config = build_gateway_contract_runner_config(cli);
    tau_gateway::run_gateway_contract_runner(config).await?;
    Ok(true)
}

pub fn build_multi_agent_contract_runner_config(cli: &Cli) -> MultiAgentRuntimeConfig {
    MultiAgentRuntimeConfig {
        fixture_path: cli.multi_agent_fixture.clone(),
        state_dir: cli.multi_agent_state_dir.clone(),
        queue_limit: cli.multi_agent_queue_limit.max(1),
        processed_case_cap: cli.multi_agent_processed_case_cap.max(1),
        retry_max_attempts: cli.multi_agent_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.multi_agent_retry_base_delay_ms,
    }
}

pub async fn run_multi_agent_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.multi_agent_contract_runner {
        return Ok(false);
    }
    let config = build_multi_agent_contract_runner_config(cli);
    tau_orchestrator::multi_agent_runtime::run_multi_agent_contract_runner(config).await?;
    Ok(true)
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BrowserAutomationLiveRunnerConfig` used across Tau components.
pub struct BrowserAutomationLiveRunnerConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub playwright_cli: String,
    pub policy: BrowserAutomationLivePolicy,
}

const BROWSER_AUTOMATION_LIVE_STATE_SCHEMA_VERSION: u32 = 1;
const BROWSER_AUTOMATION_LIVE_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";

fn browser_automation_live_state_schema_version() -> u32 {
    BROWSER_AUTOMATION_LIVE_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BrowserAutomationLiveStateFile {
    #[serde(default = "browser_automation_live_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    fixture_path: String,
    #[serde(default)]
    playwright_cli: String,
    #[serde(default)]
    policy_action_timeout_ms: u64,
    #[serde(default)]
    policy_max_actions_per_case: usize,
    #[serde(default)]
    policy_allow_unsafe_actions: bool,
    #[serde(default)]
    discovered_cases: usize,
    #[serde(default)]
    success_cases: usize,
    #[serde(default)]
    malformed_cases: usize,
    #[serde(default)]
    retryable_failures: usize,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize)]
struct BrowserAutomationLiveCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    discovered_cases: usize,
    success_cases: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `StandardContractRunnerConfig` used across Tau components.
pub struct StandardContractRunnerConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `EventsRunnerCliConfig` used across Tau components.
pub struct EventsRunnerCliConfig {
    pub channel_store_root: PathBuf,
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub poll_interval_ms: u64,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SlackBridgeCliConfig` used across Tau components.
pub struct SlackBridgeCliConfig {
    pub state_dir: PathBuf,
    pub api_base: String,
    pub app_token: String,
    pub bot_token: String,
    pub bot_user_id: Option<String>,
    pub detail_thread_output: bool,
    pub detail_thread_threshold_chars: usize,
    pub processed_event_cap: usize,
    pub max_event_age_seconds: u64,
    pub reconnect_delay_ms: u64,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub artifact_retention_days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `GithubIssuesBridgeCliConfig` used across Tau components.
pub struct GithubIssuesBridgeCliConfig {
    pub state_dir: PathBuf,
    pub repo_slug: String,
    pub api_base: String,
    pub token: String,
    pub bot_login: Option<String>,
    pub poll_interval_seconds: u64,
    pub poll_once: bool,
    pub required_labels: Vec<String>,
    pub required_issue_numbers: Vec<u64>,
    pub include_issue_body: bool,
    pub include_edited_comments: bool,
    pub processed_event_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub artifact_retention_days: u64,
}

fn load_browser_automation_live_previous_health(state_path: &Path) -> TransportHealthSnapshot {
    let Ok(raw) = std::fs::read_to_string(state_path) else {
        return TransportHealthSnapshot::default();
    };
    serde_json::from_str::<BrowserAutomationLiveStateFile>(&raw)
        .map(|state| state.health)
        .unwrap_or_default()
}

fn build_browser_automation_live_health_snapshot(
    discovered_cases: usize,
    success_cases: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let failure_streak = if retryable_failures > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };
    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: 0,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: discovered_cases,
        last_cycle_processed: discovered_cases,
        last_cycle_completed: success_cases.saturating_add(malformed_cases),
        last_cycle_failed: retryable_failures,
        last_cycle_duplicates: 0,
    }
}

fn browser_automation_live_reason_codes(
    success_cases: usize,
    malformed_cases: usize,
    retryable_failures: usize,
) -> Vec<String> {
    let mut codes = Vec::new();
    if success_cases > 0 {
        codes.push("live_actions_succeeded".to_string());
    }
    if malformed_cases > 0 {
        codes.push("malformed_cases_observed".to_string());
    }
    if retryable_failures > 0 {
        codes.push("retryable_failures_observed".to_string());
    }
    if codes.is_empty() {
        codes.push("no_cases_processed".to_string());
    }
    codes
}

fn persist_browser_automation_live_state(
    config: &BrowserAutomationLiveRunnerConfig,
    discovered_cases: usize,
    success_cases: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    health: &TransportHealthSnapshot,
) -> Result<()> {
    std::fs::create_dir_all(&config.state_dir)?;
    let state = BrowserAutomationLiveStateFile {
        schema_version: BROWSER_AUTOMATION_LIVE_STATE_SCHEMA_VERSION,
        fixture_path: config.fixture_path.display().to_string(),
        playwright_cli: config.playwright_cli.clone(),
        policy_action_timeout_ms: config.policy.action_timeout_ms,
        policy_max_actions_per_case: config.policy.max_actions_per_case,
        policy_allow_unsafe_actions: config.policy.allow_unsafe_actions,
        discovered_cases,
        success_cases,
        malformed_cases,
        retryable_failures,
        health: health.clone(),
    };
    let state_raw = serde_json::to_string_pretty(&state)?;
    write_text_atomic(&config.state_dir.join("state.json"), &state_raw)?;

    let classification = health.classify();
    let report = BrowserAutomationLiveCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: classification.state.as_str().to_string(),
        health_reason: classification.reason,
        reason_codes: browser_automation_live_reason_codes(
            success_cases,
            malformed_cases,
            retryable_failures,
        ),
        discovered_cases,
        success_cases,
        malformed_cases,
        retryable_failures,
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&report)?;
    let events_path = config
        .state_dir
        .join(BROWSER_AUTOMATION_LIVE_EVENTS_LOG_FILE);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_path)?;
    writeln!(file, "{line}")?;
    file.flush()?;
    Ok(())
}

pub fn build_browser_automation_live_runner_config(cli: &Cli) -> BrowserAutomationLiveRunnerConfig {
    BrowserAutomationLiveRunnerConfig {
        fixture_path: cli.browser_automation_live_fixture.clone(),
        state_dir: cli.browser_automation_state_dir.clone(),
        playwright_cli: cli.browser_automation_playwright_cli.trim().to_string(),
        policy: BrowserAutomationLivePolicy::default(),
    }
}

pub async fn run_browser_automation_live_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.browser_automation_live_runner {
        return Ok(false);
    }
    let config = build_browser_automation_live_runner_config(cli);
    let cycle_started = Instant::now();
    let previous_health =
        load_browser_automation_live_previous_health(&config.state_dir.join("state.json"));
    let fixture = load_browser_automation_contract_fixture(&config.fixture_path)?;
    let executor = PlaywrightCliActionExecutor::new(config.playwright_cli.clone())?;
    let mut manager = BrowserSessionManager::new(executor);
    let summary = run_browser_automation_live_fixture(&fixture, &mut manager, &config.policy)?;
    let cycle_duration_ms = u64::try_from(cycle_started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let health = build_browser_automation_live_health_snapshot(
        summary.discovered_cases,
        summary.success_cases,
        summary.malformed_cases,
        summary.retryable_failures,
        cycle_duration_ms,
        previous_health.failure_streak,
    );
    persist_browser_automation_live_state(
        &config,
        summary.discovered_cases,
        summary.success_cases,
        summary.malformed_cases,
        summary.retryable_failures,
        &health,
    )?;
    let classification = health.classify();

    println!(
        "browser automation live summary: discovered={} success={} malformed={} retryable_failures={}",
        summary.discovered_cases,
        summary.success_cases,
        summary.malformed_cases,
        summary.retryable_failures
    );
    println!(
        "browser automation live health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(true)
}

pub fn build_memory_contract_runner_config(cli: &Cli) -> StandardContractRunnerConfig {
    StandardContractRunnerConfig {
        fixture_path: cli.memory_fixture.clone(),
        state_dir: cli.memory_state_dir.clone(),
        queue_limit: cli.memory_queue_limit.max(1),
        processed_case_cap: cli.memory_processed_case_cap.max(1),
        retry_max_attempts: cli.memory_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.memory_retry_base_delay_ms,
    }
}

pub async fn run_memory_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.memory_contract_runner {
        return Ok(false);
    }
    let config = build_memory_contract_runner_config(cli);
    run_memory_contract_runner(MemoryRuntimeConfig {
        fixture_path: config.fixture_path,
        state_dir: config.state_dir,
        queue_limit: config.queue_limit,
        processed_case_cap: config.processed_case_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
    })
    .await?;
    Ok(true)
}

pub fn build_dashboard_contract_runner_config(cli: &Cli) -> StandardContractRunnerConfig {
    StandardContractRunnerConfig {
        fixture_path: cli.dashboard_fixture.clone(),
        state_dir: cli.dashboard_state_dir.clone(),
        queue_limit: cli.dashboard_queue_limit.max(1),
        processed_case_cap: cli.dashboard_processed_case_cap.max(1),
        retry_max_attempts: cli.dashboard_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.dashboard_retry_base_delay_ms,
    }
}

pub async fn run_dashboard_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.dashboard_contract_runner {
        return Ok(false);
    }
    let config = build_dashboard_contract_runner_config(cli);
    run_dashboard_contract_runner(DashboardRuntimeConfig {
        fixture_path: config.fixture_path,
        state_dir: config.state_dir,
        queue_limit: config.queue_limit,
        processed_case_cap: config.processed_case_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
    })
    .await?;
    Ok(true)
}

pub fn build_deployment_contract_runner_config(cli: &Cli) -> StandardContractRunnerConfig {
    StandardContractRunnerConfig {
        fixture_path: cli.deployment_fixture.clone(),
        state_dir: cli.deployment_state_dir.clone(),
        queue_limit: cli.deployment_queue_limit.max(1),
        processed_case_cap: cli.deployment_processed_case_cap.max(1),
        retry_max_attempts: cli.deployment_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.deployment_retry_base_delay_ms,
    }
}

pub async fn run_deployment_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.deployment_contract_runner {
        return Ok(false);
    }
    let config = build_deployment_contract_runner_config(cli);
    run_deployment_contract_runner(DeploymentRuntimeConfig {
        fixture_path: config.fixture_path,
        state_dir: config.state_dir,
        queue_limit: config.queue_limit,
        processed_case_cap: config.processed_case_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
    })
    .await?;
    Ok(true)
}

pub fn build_custom_command_contract_runner_config(cli: &Cli) -> StandardContractRunnerConfig {
    StandardContractRunnerConfig {
        fixture_path: cli.custom_command_fixture.clone(),
        state_dir: cli.custom_command_state_dir.clone(),
        queue_limit: cli.custom_command_queue_limit.max(1),
        processed_case_cap: cli.custom_command_processed_case_cap.max(1),
        retry_max_attempts: cli.custom_command_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.custom_command_retry_base_delay_ms,
    }
}

fn build_custom_command_default_execution_policy(cli: &Cli) -> CustomCommandExecutionPolicy {
    let mut policy = default_custom_command_execution_policy();
    policy.require_approval = cli.custom_command_policy_require_approval;
    policy.allow_shell = cli.custom_command_policy_allow_shell;
    policy.sandbox_profile =
        normalize_sandbox_profile(cli.custom_command_policy_sandbox_profile.as_str());
    if !cli.custom_command_policy_allowed_env.is_empty() {
        policy.allowed_env = cli.custom_command_policy_allowed_env.clone();
    }
    if !cli.custom_command_policy_denied_env.is_empty() {
        policy.denied_env = cli.custom_command_policy_denied_env.clone();
    }
    policy
}

pub async fn run_custom_command_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.custom_command_contract_runner {
        return Ok(false);
    }
    let config = build_custom_command_contract_runner_config(cli);
    run_custom_command_contract_runner(CustomCommandRuntimeConfig {
        fixture_path: config.fixture_path,
        state_dir: config.state_dir,
        queue_limit: config.queue_limit,
        processed_case_cap: config.processed_case_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
        run_timeout_ms: 30_000,
        default_execution_policy: build_custom_command_default_execution_policy(cli),
    })
    .await?;
    Ok(true)
}

pub fn build_voice_contract_runner_config(cli: &Cli) -> StandardContractRunnerConfig {
    StandardContractRunnerConfig {
        fixture_path: cli.voice_fixture.clone(),
        state_dir: cli.voice_state_dir.clone(),
        queue_limit: cli.voice_queue_limit.max(1),
        processed_case_cap: cli.voice_processed_case_cap.max(1),
        retry_max_attempts: cli.voice_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.voice_retry_base_delay_ms,
    }
}

pub async fn run_voice_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.voice_contract_runner {
        return Ok(false);
    }
    let config = build_voice_contract_runner_config(cli);
    run_voice_contract_runner(VoiceRuntimeConfig {
        fixture_path: config.fixture_path,
        state_dir: config.state_dir,
        queue_limit: config.queue_limit,
        processed_case_cap: config.processed_case_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
    })
    .await?;
    Ok(true)
}

pub fn build_voice_live_runner_config(cli: &Cli) -> VoiceLiveRuntimeConfig {
    VoiceLiveRuntimeConfig {
        input_path: cli.voice_live_input.clone(),
        state_dir: cli.voice_state_dir.clone(),
        wake_word: cli.voice_live_wake_word.trim().to_ascii_lowercase(),
        max_turns: cli.voice_live_max_turns.max(1),
        tts_output_enabled: cli.voice_live_tts_output,
    }
}

pub async fn run_voice_live_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.voice_live_runner {
        return Ok(false);
    }
    let config = build_voice_live_runner_config(cli);
    run_voice_live_runner(config).await?;
    Ok(true)
}

pub fn build_events_runner_cli_config(cli: &Cli) -> EventsRunnerCliConfig {
    EventsRunnerCliConfig {
        channel_store_root: cli.channel_store_root.clone(),
        events_dir: cli.events_dir.clone(),
        state_path: cli.events_state_path.clone(),
        poll_interval_ms: cli.events_poll_interval_ms.max(1),
        queue_limit: cli.events_queue_limit.max(1),
        stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
    }
}

pub async fn run_events_runner_if_requested<FRun, Fut>(cli: &Cli, run_events: FRun) -> Result<bool>
where
    FRun: FnOnce(EventsRunnerCliConfig) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    if !cli.events_runner {
        return Ok(false);
    }
    let config = build_events_runner_cli_config(cli);
    run_events(config).await?;
    Ok(true)
}

pub async fn run_events_runner_with_runtime_defaults_if_requested<FRun, Fut>(
    cli: &Cli,
    model_ref: &ModelRef,
    system_prompt: &str,
    run_events: FRun,
) -> Result<bool>
where
    FRun: FnOnce(EventsRunnerCliConfig, TransportRuntimeDefaults) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let runtime_defaults = build_transport_runtime_defaults(cli, model_ref, system_prompt);
    run_events_runner_if_requested(cli, move |config| run_events(config, runtime_defaults)).await
}

pub fn build_slack_bridge_cli_config(
    cli: &Cli,
    app_token: String,
    bot_token: String,
) -> SlackBridgeCliConfig {
    SlackBridgeCliConfig {
        state_dir: cli.slack_state_dir.clone(),
        api_base: cli.slack_api_base.clone(),
        app_token,
        bot_token,
        bot_user_id: cli.slack_bot_user_id.clone(),
        detail_thread_output: cli.slack_thread_detail_output,
        detail_thread_threshold_chars: cli.slack_thread_detail_threshold_chars.max(1),
        processed_event_cap: cli.slack_processed_event_cap.max(1),
        max_event_age_seconds: cli.slack_max_event_age_seconds,
        reconnect_delay_ms: cli.slack_reconnect_delay_ms.max(1),
        retry_max_attempts: cli.slack_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.slack_retry_base_delay_ms.max(1),
        artifact_retention_days: cli.slack_artifact_retention_days,
    }
}

pub fn resolve_slack_bridge_tokens_from_cli<FResolveSecret>(
    cli: &Cli,
    mut resolve_secret: FResolveSecret,
) -> Result<(String, String)>
where
    FResolveSecret: FnMut(Option<&str>, Option<&str>, &str) -> Result<Option<String>>,
{
    let app_token = resolve_secret(
        cli.slack_app_token.as_deref(),
        cli.slack_app_token_id.as_deref(),
        "--slack-app-token-id",
    )?
    .ok_or_else(|| {
        anyhow!(
            "--slack-app-token (or --slack-app-token-id) is required when --slack-bridge is set"
        )
    })?;
    let bot_token = resolve_secret(
        cli.slack_bot_token.as_deref(),
        cli.slack_bot_token_id.as_deref(),
        "--slack-bot-token-id",
    )?
    .ok_or_else(|| {
        anyhow!(
            "--slack-bot-token (or --slack-bot-token-id) is required when --slack-bridge is set"
        )
    })?;
    Ok((app_token, bot_token))
}

pub async fn run_slack_bridge_if_requested<FResolveTokens, FRunBridge, Fut>(
    cli: &Cli,
    resolve_tokens: FResolveTokens,
    run_bridge: FRunBridge,
) -> Result<bool>
where
    FResolveTokens: FnOnce() -> Result<(String, String)>,
    FRunBridge: FnOnce(SlackBridgeCliConfig) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    if !cli.slack_bridge {
        return Ok(false);
    }
    let (app_token, bot_token) = resolve_tokens()?;
    let config = build_slack_bridge_cli_config(cli, app_token, bot_token);
    run_bridge(config).await?;
    Ok(true)
}

pub async fn run_slack_bridge_with_runtime_defaults_if_requested<FResolveTokens, FRunBridge, Fut>(
    cli: &Cli,
    model_ref: &ModelRef,
    system_prompt: &str,
    resolve_tokens: FResolveTokens,
    run_bridge: FRunBridge,
) -> Result<bool>
where
    FResolveTokens: FnOnce() -> Result<(String, String)>,
    FRunBridge: FnOnce(SlackBridgeCliConfig, TransportRuntimeDefaults) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let runtime_defaults = build_transport_runtime_defaults(cli, model_ref, system_prompt);
    run_slack_bridge_if_requested(cli, resolve_tokens, move |config| {
        run_bridge(config, runtime_defaults)
    })
    .await
}

pub fn build_github_issues_bridge_cli_config(
    cli: &Cli,
    repo_slug: String,
    token: String,
) -> GithubIssuesBridgeCliConfig {
    GithubIssuesBridgeCliConfig {
        state_dir: cli.github_state_dir.clone(),
        repo_slug,
        api_base: cli.github_api_base.clone(),
        token,
        bot_login: cli.github_bot_login.clone(),
        poll_interval_seconds: cli.github_poll_interval_seconds.max(1),
        poll_once: cli.github_poll_once,
        required_labels: cli
            .github_required_label
            .iter()
            .map(|label| label.trim().to_string())
            .collect(),
        required_issue_numbers: cli.github_issue_number.clone(),
        include_issue_body: cli.github_include_issue_body,
        include_edited_comments: cli.github_include_edited_comments,
        processed_event_cap: cli.github_processed_event_cap.max(1),
        retry_max_attempts: cli.github_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.github_retry_base_delay_ms.max(1),
        artifact_retention_days: cli.github_artifact_retention_days,
    }
}

pub fn resolve_github_issues_bridge_repo_and_token_from_cli<FResolveSecret>(
    cli: &Cli,
    mut resolve_secret: FResolveSecret,
) -> Result<(String, String)>
where
    FResolveSecret: FnMut(Option<&str>, Option<&str>, &str) -> Result<Option<String>>,
{
    let repo_slug = cli
        .github_repo
        .clone()
        .ok_or_else(|| anyhow!("--github-repo is required when --github-issues-bridge is set"))?;
    let token = resolve_secret(
        cli.github_token.as_deref(),
        cli.github_token_id.as_deref(),
        "--github-token-id",
    )?
    .ok_or_else(|| {
        anyhow!(
            "--github-token (or --github-token-id) is required when --github-issues-bridge is set"
        )
    })?;
    Ok((repo_slug, token))
}

pub async fn run_github_issues_bridge_if_requested<FResolveRepoAndToken, FRunBridge, Fut>(
    cli: &Cli,
    resolve_repo_and_token: FResolveRepoAndToken,
    run_bridge: FRunBridge,
) -> Result<bool>
where
    FResolveRepoAndToken: FnOnce() -> Result<(String, String)>,
    FRunBridge: FnOnce(GithubIssuesBridgeCliConfig) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    if !cli.github_issues_bridge {
        return Ok(false);
    }
    let (repo_slug, token) = resolve_repo_and_token()?;
    let config = build_github_issues_bridge_cli_config(cli, repo_slug, token);
    run_bridge(config).await?;
    Ok(true)
}

pub async fn run_github_issues_bridge_with_runtime_defaults_if_requested<
    FResolveRepoAndToken,
    FRunBridge,
    Fut,
>(
    cli: &Cli,
    model_ref: &ModelRef,
    system_prompt: &str,
    resolve_repo_and_token: FResolveRepoAndToken,
    run_bridge: FRunBridge,
) -> Result<bool>
where
    FResolveRepoAndToken: FnOnce() -> Result<(String, String)>,
    FRunBridge: FnOnce(GithubIssuesBridgeCliConfig, TransportRuntimeDefaults) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let runtime_defaults = build_transport_runtime_defaults(cli, model_ref, system_prompt);
    run_github_issues_bridge_if_requested(cli, resolve_repo_and_token, move |config| {
        run_bridge(config, runtime_defaults)
    })
    .await
}

pub fn build_multi_channel_contract_runner_config(
    cli: &Cli,
    command_handlers: MultiChannelCommandHandlers,
    pairing_evaluator: std::sync::Arc<dyn MultiChannelPairingEvaluator>,
) -> MultiChannelRuntimeConfig {
    MultiChannelRuntimeConfig {
        fixture_path: cli.multi_channel_fixture.clone(),
        state_dir: cli.multi_channel_state_dir.clone(),
        orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        queue_limit: cli.multi_channel_queue_limit.max(1),
        processed_event_cap: cli.multi_channel_processed_event_cap.max(1),
        retry_max_attempts: cli.multi_channel_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.multi_channel_retry_base_delay_ms,
        retry_jitter_ms: cli.multi_channel_retry_jitter_ms,
        outbound: build_multi_channel_outbound_config(cli),
        telemetry: build_multi_channel_telemetry_config(cli),
        media: build_multi_channel_media_config(cli),
        command_handlers,
        pairing_evaluator,
    }
}

pub async fn run_multi_channel_contract_runner_if_requested(
    cli: &Cli,
    command_handlers: MultiChannelCommandHandlers,
    pairing_evaluator: std::sync::Arc<dyn MultiChannelPairingEvaluator>,
) -> Result<bool> {
    if !cli.multi_channel_contract_runner {
        return Ok(false);
    }
    let config =
        build_multi_channel_contract_runner_config(cli, command_handlers, pairing_evaluator);
    tau_multi_channel::run_multi_channel_contract_runner(config).await?;
    Ok(true)
}

pub fn build_multi_channel_live_runner_config(
    cli: &Cli,
    command_handlers: MultiChannelCommandHandlers,
    pairing_evaluator: std::sync::Arc<dyn MultiChannelPairingEvaluator>,
) -> MultiChannelLiveRuntimeConfig {
    MultiChannelLiveRuntimeConfig {
        ingress_dir: cli.multi_channel_live_ingress_dir.clone(),
        state_dir: cli.multi_channel_state_dir.clone(),
        orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        queue_limit: cli.multi_channel_queue_limit.max(1),
        processed_event_cap: cli.multi_channel_processed_event_cap.max(1),
        retry_max_attempts: cli.multi_channel_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.multi_channel_retry_base_delay_ms,
        retry_jitter_ms: cli.multi_channel_retry_jitter_ms,
        outbound: build_multi_channel_outbound_config(cli),
        telemetry: build_multi_channel_telemetry_config(cli),
        media: build_multi_channel_media_config(cli),
        command_handlers,
        pairing_evaluator,
    }
}

pub async fn run_multi_channel_live_runner_if_requested(
    cli: &Cli,
    command_handlers: MultiChannelCommandHandlers,
    pairing_evaluator: std::sync::Arc<dyn MultiChannelPairingEvaluator>,
) -> Result<bool> {
    if !cli.multi_channel_live_runner {
        return Ok(false);
    }
    let config = build_multi_channel_live_runner_config(cli, command_handlers, pairing_evaluator);
    tau_multi_channel::run_multi_channel_live_runner(config).await?;
    Ok(true)
}

pub async fn run_multi_channel_live_runner_with_runtime_dependencies_if_requested<
    FBuildHandlers,
    FBuildPairing,
>(
    cli: &Cli,
    model_ref: &ModelRef,
    build_command_handlers: FBuildHandlers,
    build_pairing_evaluator: FBuildPairing,
) -> Result<bool>
where
    FBuildHandlers: FnOnce(AuthCommandConfig, DoctorCommandConfig) -> MultiChannelCommandHandlers,
    FBuildPairing: FnOnce() -> std::sync::Arc<dyn MultiChannelPairingEvaluator>,
{
    let (command_handlers, pairing_evaluator) = build_multi_channel_runtime_dependencies(
        cli,
        model_ref,
        build_command_handlers,
        build_pairing_evaluator,
    );
    run_multi_channel_live_runner_if_requested(cli, command_handlers, pairing_evaluator).await
}

pub fn build_multi_channel_live_connectors_config(cli: &Cli) -> MultiChannelLiveConnectorsConfig {
    MultiChannelLiveConnectorsConfig {
        state_path: cli.multi_channel_live_connectors_state_path.clone(),
        ingress_dir: cli.multi_channel_live_ingress_dir.clone(),
        processed_event_cap: cli.multi_channel_processed_event_cap.max(1),
        retry_max_attempts: cli.multi_channel_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.multi_channel_retry_base_delay_ms,
        poll_once: cli.multi_channel_live_connectors_poll_once,
        webhook_bind: cli.multi_channel_live_webhook_bind.clone(),
        telegram_mode: cli.multi_channel_telegram_ingress_mode.into(),
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        telegram_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_telegram_bot_token.as_deref(),
            "telegram-bot-token",
        ),
        telegram_webhook_secret: resolve_non_empty_cli_value(
            cli.multi_channel_telegram_webhook_secret.as_deref(),
        ),
        discord_mode: cli.multi_channel_discord_ingress_mode.into(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        discord_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_discord_bot_token.as_deref(),
            "discord-bot-token",
        ),
        discord_ingress_channel_ids: cli
            .multi_channel_discord_ingress_channel_ids
            .iter()
            .map(|value| value.trim().to_string())
            .collect(),
        whatsapp_mode: cli.multi_channel_whatsapp_ingress_mode.into(),
        whatsapp_webhook_verify_token: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_webhook_verify_token.as_deref(),
        ),
        whatsapp_webhook_app_secret: resolve_non_empty_cli_value(
            cli.multi_channel_whatsapp_webhook_app_secret.as_deref(),
        ),
    }
}

pub async fn run_multi_channel_live_connectors_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.multi_channel_live_connectors_runner {
        return Ok(false);
    }
    let config = build_multi_channel_live_connectors_config(cli);
    tau_multi_channel::run_multi_channel_live_connectors_runner(config).await?;
    Ok(true)
}

pub fn resolve_multi_channel_outbound_secret(
    cli: &Cli,
    direct_secret: Option<&str>,
    integration_id: &str,
) -> Option<String> {
    if let Some(secret) = resolve_non_empty_cli_value(direct_secret) {
        return Some(secret);
    }
    let store = load_credential_store(
        &cli.credential_store,
        resolve_credential_store_encryption_mode(cli),
        cli.credential_store_key.as_deref(),
    )
    .ok()?;
    let entry = store.integrations.get(integration_id)?;
    if entry.revoked {
        return None;
    }
    entry
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn build_multi_channel_outbound_config(cli: &Cli) -> MultiChannelOutboundConfig {
    MultiChannelOutboundConfig {
        mode: cli.multi_channel_outbound_mode.into(),
        max_chars: cli.multi_channel_outbound_max_chars.max(1),
        http_timeout_ms: cli.multi_channel_outbound_http_timeout_ms.max(1),
        telegram_api_base: cli.multi_channel_telegram_api_base.trim().to_string(),
        discord_api_base: cli.multi_channel_discord_api_base.trim().to_string(),
        whatsapp_api_base: cli.multi_channel_whatsapp_api_base.trim().to_string(),
        telegram_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_telegram_bot_token.as_deref(),
            "telegram-bot-token",
        ),
        discord_bot_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_discord_bot_token.as_deref(),
            "discord-bot-token",
        ),
        whatsapp_access_token: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_whatsapp_access_token.as_deref(),
            "whatsapp-access-token",
        ),
        whatsapp_phone_number_id: resolve_multi_channel_outbound_secret(
            cli,
            cli.multi_channel_whatsapp_phone_number_id.as_deref(),
            "whatsapp-phone-number-id",
        ),
    }
}

pub fn build_multi_channel_telemetry_config(cli: &Cli) -> MultiChannelTelemetryConfig {
    MultiChannelTelemetryConfig {
        typing_presence_enabled: cli.multi_channel_telemetry_typing_presence,
        usage_summary_enabled: cli.multi_channel_telemetry_usage_summary,
        include_identifiers: cli.multi_channel_telemetry_include_identifiers,
        typing_presence_min_response_chars: cli.multi_channel_telemetry_min_response_chars.max(1),
    }
}

pub fn build_multi_channel_media_config(cli: &Cli) -> MultiChannelMediaUnderstandingConfig {
    MultiChannelMediaUnderstandingConfig {
        enabled: cli.multi_channel_media_understanding,
        max_attachments_per_event: cli.multi_channel_media_max_attachments.max(1),
        max_summary_chars: cli.multi_channel_media_max_summary_chars.max(16),
    }
}

fn resolve_non_empty_cli_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests;
