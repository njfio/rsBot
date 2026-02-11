use anyhow::Result;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use tau_ai::{LlmClient, ModelRef};
use tau_browser_automation::browser_automation_runtime::{
    run_browser_automation_contract_runner, BrowserAutomationRuntimeConfig,
};
use tau_cli::validation::{
    validate_browser_automation_contract_runner_cli, validate_custom_command_contract_runner_cli,
    validate_dashboard_contract_runner_cli, validate_deployment_contract_runner_cli,
    validate_events_runner_cli, validate_gateway_contract_runner_cli,
    validate_gateway_openresponses_server_cli, validate_github_issues_bridge_cli,
    validate_memory_contract_runner_cli, validate_multi_agent_contract_runner_cli,
    validate_multi_channel_contract_runner_cli, validate_multi_channel_live_connectors_runner_cli,
    validate_multi_channel_live_runner_cli, validate_slack_bridge_cli,
    validate_voice_contract_runner_cli,
};
use tau_cli::Cli;
use tau_cli::CliGatewayOpenResponsesAuthMode;
use tau_custom_command::custom_command_runtime::{
    run_custom_command_contract_runner, CustomCommandRuntimeConfig,
};
use tau_dashboard::dashboard_runtime::{run_dashboard_contract_runner, DashboardRuntimeConfig};
use tau_deployment::deployment_runtime::{run_deployment_contract_runner, DeploymentRuntimeConfig};
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
use tau_orchestrator::multi_agent_runtime::MultiAgentRuntimeConfig;
use tau_provider::{load_credential_store, resolve_credential_store_encryption_mode};
use tau_tools::tools::{register_builtin_tools, ToolPolicy};
use tau_voice::voice_runtime::{run_voice_contract_runner, VoiceRuntimeConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiChannelTransportMode {
    None,
    ContractRunner,
    LiveRunner,
    LiveConnectorsRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeTransportMode {
    None,
    GithubIssuesBridge,
    SlackBridge,
    EventsRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractTransportMode {
    None,
    MultiAgent,
    BrowserAutomation,
    Memory,
    Dashboard,
    Gateway,
    Deployment,
    CustomCommand,
    Voice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    BrowserAutomationContractRunner,
    MemoryContractRunner,
    DashboardContractRunner,
    GatewayContractRunner,
    DeploymentContractRunner,
    CustomCommandContractRunner,
    VoiceContractRunner,
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
    validate_memory_contract_runner_cli(cli)?;
    validate_dashboard_contract_runner_cli(cli)?;
    validate_gateway_openresponses_server_cli(cli)?;
    validate_gateway_contract_runner_cli(cli)?;
    validate_deployment_contract_runner_cli(cli)?;
    validate_custom_command_contract_runner_cli(cli)?;
    validate_voice_contract_runner_cli(cli)?;
    Ok(())
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
    } else if cli.browser_automation_contract_runner {
        ContractTransportMode::BrowserAutomation
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
        ContractTransportMode::BrowserAutomation => {
            return TransportRuntimeMode::BrowserAutomationContractRunner;
        }
        ContractTransportMode::Memory => return TransportRuntimeMode::MemoryContractRunner,
        ContractTransportMode::Dashboard => return TransportRuntimeMode::DashboardContractRunner,
        ContractTransportMode::Gateway => return TransportRuntimeMode::GatewayContractRunner,
        ContractTransportMode::Deployment => return TransportRuntimeMode::DeploymentContractRunner,
        ContractTransportMode::CustomCommand => {
            return TransportRuntimeMode::CustomCommandContractRunner;
        }
        ContractTransportMode::Voice => return TransportRuntimeMode::VoiceContractRunner,
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
pub struct BrowserAutomationContractRunnerConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub action_timeout_ms: u64,
    pub max_actions_per_case: usize,
    pub allow_unsafe_actions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardContractRunnerConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventsRunnerCliConfig {
    pub channel_store_root: PathBuf,
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub poll_interval_ms: u64,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub fn build_browser_automation_contract_runner_config(
    cli: &Cli,
) -> BrowserAutomationContractRunnerConfig {
    BrowserAutomationContractRunnerConfig {
        fixture_path: cli.browser_automation_fixture.clone(),
        state_dir: cli.browser_automation_state_dir.clone(),
        queue_limit: cli.browser_automation_queue_limit.max(1),
        processed_case_cap: cli.browser_automation_processed_case_cap.max(1),
        retry_max_attempts: cli.browser_automation_retry_max_attempts.max(1),
        retry_base_delay_ms: cli.browser_automation_retry_base_delay_ms,
        action_timeout_ms: cli.browser_automation_action_timeout_ms.max(1),
        max_actions_per_case: cli.browser_automation_max_actions_per_case.max(1),
        allow_unsafe_actions: cli.browser_automation_allow_unsafe_actions,
    }
}

pub async fn run_browser_automation_contract_runner_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.browser_automation_contract_runner {
        return Ok(false);
    }
    let config = build_browser_automation_contract_runner_config(cli);
    run_browser_automation_contract_runner(BrowserAutomationRuntimeConfig {
        fixture_path: config.fixture_path,
        state_dir: config.state_dir,
        queue_limit: config.queue_limit,
        processed_case_cap: config.processed_case_cap,
        retry_max_attempts: config.retry_max_attempts,
        retry_base_delay_ms: config.retry_base_delay_ms,
        action_timeout_ms: config.action_timeout_ms,
        max_actions_per_case: config.max_actions_per_case,
        allow_unsafe_actions: config.allow_unsafe_actions,
    })
    .await?;
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
mod tests {
    use super::{
        build_browser_automation_contract_runner_config,
        build_custom_command_contract_runner_config, build_dashboard_contract_runner_config,
        build_deployment_contract_runner_config, build_events_runner_cli_config,
        build_gateway_contract_runner_config, build_gateway_openresponses_server_config,
        build_github_issues_bridge_cli_config, build_memory_contract_runner_config,
        build_multi_agent_contract_runner_config, build_multi_channel_contract_runner_config,
        build_multi_channel_live_connectors_config, build_multi_channel_live_runner_config,
        build_multi_channel_media_config, build_multi_channel_outbound_config,
        build_multi_channel_telemetry_config, build_slack_bridge_cli_config,
        build_voice_contract_runner_config, map_gateway_openresponses_auth_mode,
        resolve_bridge_transport_mode, resolve_contract_transport_mode,
        resolve_gateway_openresponses_auth, resolve_multi_channel_outbound_secret,
        resolve_multi_channel_transport_mode, resolve_transport_runtime_mode,
        run_events_runner_if_requested, run_github_issues_bridge_if_requested,
        run_slack_bridge_if_requested, validate_transport_mode_cli, BridgeTransportMode,
        ContractTransportMode, EventsRunnerCliConfig, MultiChannelTransportMode,
        SlackBridgeCliConfig, TransportRuntimeMode,
    };
    use async_trait::async_trait;
    use clap::Parser;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, ModelRef, TauAiError};
    use tau_cli::{Cli, CliGatewayOpenResponsesAuthMode};
    use tau_gateway::GatewayOpenResponsesAuthMode;
    use tau_multi_channel::{
        MultiChannelLiveConnectorMode, MultiChannelPairingDecision, MultiChannelPairingEvaluator,
    };
    use tau_provider::{
        load_credential_store, save_credential_store, CredentialStoreData,
        CredentialStoreEncryptionMode, IntegrationCredentialStoreRecord,
    };
    use tau_tools::tools::ToolPolicy;
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    struct NoopClient;

    #[async_trait]
    impl LlmClient for NoopClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct AllowAllPairingEvaluator;

    impl MultiChannelPairingEvaluator for AllowAllPairingEvaluator {
        fn evaluate_pairing(
            &self,
            _state_dir: &Path,
            _policy_channel: &str,
            _actor_id: &str,
            _now_unix_ms: u64,
        ) -> anyhow::Result<MultiChannelPairingDecision> {
            Ok(MultiChannelPairingDecision::Allow {
                reason_code: "allowed_for_test".to_string(),
            })
        }
    }

    fn write_integration_secret(
        path: &Path,
        integration_id: &str,
        secret: Option<&str>,
        revoked: bool,
    ) {
        let mut store = load_credential_store(path, CredentialStoreEncryptionMode::None, None)
            .unwrap_or(CredentialStoreData {
                encryption: CredentialStoreEncryptionMode::None,
                providers: BTreeMap::new(),
                integrations: BTreeMap::new(),
            });
        store.integrations.insert(
            integration_id.to_string(),
            IntegrationCredentialStoreRecord {
                secret: secret.map(str::to_string),
                revoked,
                updated_unix: Some(100),
            },
        );
        save_credential_store(path, &store, None).expect("save credential store");
    }

    #[test]
    fn unit_resolve_multi_channel_outbound_secret_prefers_direct_secret() {
        let cli = parse_cli_with_stack();
        let resolved =
            resolve_multi_channel_outbound_secret(&cli, Some("  direct-secret  "), "unused");
        assert_eq!(resolved.as_deref(), Some("direct-secret"));
    }

    #[test]
    fn unit_map_gateway_openresponses_auth_mode_matches_cli_variants() {
        assert_eq!(
            map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::Token),
            GatewayOpenResponsesAuthMode::Token
        );
        assert_eq!(
            map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::PasswordSession),
            GatewayOpenResponsesAuthMode::PasswordSession
        );
        assert_eq!(
            map_gateway_openresponses_auth_mode(CliGatewayOpenResponsesAuthMode::LocalhostDev),
            GatewayOpenResponsesAuthMode::LocalhostDev
        );
    }

    #[test]
    fn unit_validate_transport_mode_cli_accepts_default_cli() {
        let cli = parse_cli_with_stack();
        validate_transport_mode_cli(&cli).expect("default transport validation should succeed");
    }

    #[test]
    fn functional_validate_transport_mode_cli_accepts_minimum_github_bridge_configuration() {
        let mut cli = parse_cli_with_stack();
        cli.github_issues_bridge = true;
        cli.github_repo = Some("owner/repo".to_string());
        cli.github_token = Some("token-value".to_string());

        validate_transport_mode_cli(&cli)
            .expect("github bridge minimum configuration should validate");
    }

    #[test]
    fn unit_resolve_multi_channel_transport_mode_defaults_to_none() {
        let cli = parse_cli_with_stack();
        assert_eq!(
            resolve_multi_channel_transport_mode(&cli),
            MultiChannelTransportMode::None
        );
    }

    #[test]
    fn functional_resolve_multi_channel_transport_mode_prefers_contract_runner() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_contract_runner = true;
        cli.multi_channel_live_runner = true;
        cli.multi_channel_live_connectors_runner = true;
        assert_eq!(
            resolve_multi_channel_transport_mode(&cli),
            MultiChannelTransportMode::ContractRunner
        );
    }

    #[test]
    fn integration_resolve_multi_channel_transport_mode_selects_live_runner() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_live_runner = true;
        assert_eq!(
            resolve_multi_channel_transport_mode(&cli),
            MultiChannelTransportMode::LiveRunner
        );
    }

    #[test]
    fn regression_resolve_multi_channel_transport_mode_selects_connectors_runner() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_live_connectors_runner = true;
        assert_eq!(
            resolve_multi_channel_transport_mode(&cli),
            MultiChannelTransportMode::LiveConnectorsRunner
        );
    }

    #[test]
    fn unit_resolve_bridge_transport_mode_defaults_to_none() {
        let cli = parse_cli_with_stack();
        assert_eq!(
            resolve_bridge_transport_mode(&cli),
            BridgeTransportMode::None
        );
    }

    #[test]
    fn functional_resolve_bridge_transport_mode_prefers_github_issues_bridge() {
        let mut cli = parse_cli_with_stack();
        cli.github_issues_bridge = true;
        cli.slack_bridge = true;
        cli.events_runner = true;
        assert_eq!(
            resolve_bridge_transport_mode(&cli),
            BridgeTransportMode::GithubIssuesBridge
        );
    }

    #[test]
    fn integration_resolve_bridge_transport_mode_selects_slack_bridge() {
        let mut cli = parse_cli_with_stack();
        cli.slack_bridge = true;
        assert_eq!(
            resolve_bridge_transport_mode(&cli),
            BridgeTransportMode::SlackBridge
        );
    }

    #[test]
    fn regression_resolve_bridge_transport_mode_selects_events_runner() {
        let mut cli = parse_cli_with_stack();
        cli.events_runner = true;
        assert_eq!(
            resolve_bridge_transport_mode(&cli),
            BridgeTransportMode::EventsRunner
        );
    }

    #[test]
    fn unit_resolve_transport_runtime_mode_defaults_to_none() {
        let cli = parse_cli_with_stack();
        assert_eq!(
            resolve_transport_runtime_mode(&cli),
            TransportRuntimeMode::None
        );
    }

    #[test]
    fn functional_resolve_transport_runtime_mode_prefers_gateway_openresponses_server() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_server = true;
        cli.github_issues_bridge = true;
        cli.slack_bridge = true;
        cli.events_runner = true;
        cli.multi_channel_contract_runner = true;
        cli.multi_channel_live_runner = true;
        cli.multi_channel_live_connectors_runner = true;
        cli.multi_agent_contract_runner = true;
        cli.browser_automation_contract_runner = true;
        cli.memory_contract_runner = true;
        cli.dashboard_contract_runner = true;
        cli.gateway_contract_runner = true;
        cli.deployment_contract_runner = true;
        cli.custom_command_contract_runner = true;
        cli.voice_contract_runner = true;
        assert_eq!(
            resolve_transport_runtime_mode(&cli),
            TransportRuntimeMode::GatewayOpenResponsesServer
        );
    }

    #[test]
    fn integration_resolve_transport_runtime_mode_prefers_bridge_before_multi_channel_and_contract()
    {
        let mut cli = parse_cli_with_stack();
        cli.github_issues_bridge = true;
        cli.multi_channel_contract_runner = true;
        cli.multi_agent_contract_runner = true;
        assert_eq!(
            resolve_transport_runtime_mode(&cli),
            TransportRuntimeMode::GithubIssuesBridge
        );
    }

    #[test]
    fn integration_resolve_transport_runtime_mode_prefers_multi_channel_before_contract() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_live_runner = true;
        cli.voice_contract_runner = true;
        assert_eq!(
            resolve_transport_runtime_mode(&cli),
            TransportRuntimeMode::MultiChannelLiveRunner
        );
    }

    #[test]
    fn regression_resolve_transport_runtime_mode_selects_voice_contract_runner() {
        let mut cli = parse_cli_with_stack();
        cli.voice_contract_runner = true;
        assert_eq!(
            resolve_transport_runtime_mode(&cli),
            TransportRuntimeMode::VoiceContractRunner
        );
    }

    #[test]
    fn unit_resolve_contract_transport_mode_defaults_to_none() {
        let cli = parse_cli_with_stack();
        assert_eq!(
            resolve_contract_transport_mode(&cli),
            ContractTransportMode::None
        );
    }

    #[test]
    fn functional_resolve_contract_transport_mode_prefers_multi_agent() {
        let mut cli = parse_cli_with_stack();
        cli.multi_agent_contract_runner = true;
        cli.browser_automation_contract_runner = true;
        cli.memory_contract_runner = true;
        cli.dashboard_contract_runner = true;
        cli.gateway_contract_runner = true;
        cli.deployment_contract_runner = true;
        cli.custom_command_contract_runner = true;
        cli.voice_contract_runner = true;
        assert_eq!(
            resolve_contract_transport_mode(&cli),
            ContractTransportMode::MultiAgent
        );
    }

    #[test]
    fn integration_resolve_contract_transport_mode_selects_memory() {
        let mut cli = parse_cli_with_stack();
        cli.memory_contract_runner = true;
        assert_eq!(
            resolve_contract_transport_mode(&cli),
            ContractTransportMode::Memory
        );
    }

    #[test]
    fn regression_resolve_contract_transport_mode_selects_voice() {
        let mut cli = parse_cli_with_stack();
        cli.voice_contract_runner = true;
        assert_eq!(
            resolve_contract_transport_mode(&cli),
            ContractTransportMode::Voice
        );
    }

    #[test]
    fn regression_validate_transport_mode_cli_rejects_events_prompt_template_conflict() {
        let mut cli = parse_cli_with_stack();
        cli.events_runner = true;
        cli.prompt_template_file = Some(PathBuf::from("template.txt"));

        let error = validate_transport_mode_cli(&cli).expect_err("conflicting flags should fail");
        assert!(error.to_string().contains("--prompt-template-file"));
    }

    #[test]
    fn functional_resolve_gateway_openresponses_auth_trims_non_empty_values() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_auth_token = Some(" token-value ".to_string());
        cli.gateway_openresponses_auth_password = Some(" password-value ".to_string());

        let (token, password) = resolve_gateway_openresponses_auth(&cli);
        assert_eq!(token.as_deref(), Some("token-value"));
        assert_eq!(password.as_deref(), Some("password-value"));
    }

    #[test]
    fn regression_resolve_gateway_openresponses_auth_ignores_empty_values() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_auth_token = Some("   ".to_string());
        cli.gateway_openresponses_auth_password = Some(String::new());

        let (token, password) = resolve_gateway_openresponses_auth(&cli);
        assert!(token.is_none());
        assert!(password.is_none());
    }

    #[test]
    fn regression_build_gateway_contract_runner_config_enforces_guardrail_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_guardrail_failure_streak_threshold = 0;
        cli.gateway_guardrail_retryable_failures_threshold = 0;

        let config = build_gateway_contract_runner_config(&cli);
        assert_eq!(config.guardrail_failure_streak_threshold, 1);
        assert_eq!(config.guardrail_retryable_failures_threshold, 1);
    }

    #[test]
    fn regression_build_multi_agent_contract_runner_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.multi_agent_queue_limit = 0;
        cli.multi_agent_processed_case_cap = 0;
        cli.multi_agent_retry_max_attempts = 0;

        let config = build_multi_agent_contract_runner_config(&cli);
        assert_eq!(config.queue_limit, 1);
        assert_eq!(config.processed_case_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
    }

    #[test]
    fn regression_build_multi_channel_contract_runner_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_queue_limit = 0;
        cli.multi_channel_processed_event_cap = 0;
        cli.multi_channel_retry_max_attempts = 0;

        let config = build_multi_channel_contract_runner_config(
            &cli,
            tau_multi_channel::MultiChannelCommandHandlers::default(),
            Arc::new(AllowAllPairingEvaluator),
        );
        assert_eq!(config.queue_limit, 1);
        assert_eq!(config.processed_event_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
    }

    #[test]
    fn regression_build_browser_automation_contract_runner_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.browser_automation_queue_limit = 0;
        cli.browser_automation_processed_case_cap = 0;
        cli.browser_automation_retry_max_attempts = 0;
        cli.browser_automation_action_timeout_ms = 0;
        cli.browser_automation_max_actions_per_case = 0;

        let config = build_browser_automation_contract_runner_config(&cli);
        assert_eq!(config.queue_limit, 1);
        assert_eq!(config.processed_case_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
        assert_eq!(config.action_timeout_ms, 1);
        assert_eq!(config.max_actions_per_case, 1);
    }

    #[test]
    fn regression_build_memory_contract_runner_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.memory_queue_limit = 0;
        cli.memory_processed_case_cap = 0;
        cli.memory_retry_max_attempts = 0;

        let config = build_memory_contract_runner_config(&cli);
        assert_eq!(config.queue_limit, 1);
        assert_eq!(config.processed_case_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
    }

    #[test]
    fn regression_build_standard_contract_runner_builders_enforce_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.dashboard_queue_limit = 0;
        cli.dashboard_processed_case_cap = 0;
        cli.dashboard_retry_max_attempts = 0;
        cli.deployment_queue_limit = 0;
        cli.deployment_processed_case_cap = 0;
        cli.deployment_retry_max_attempts = 0;
        cli.custom_command_queue_limit = 0;
        cli.custom_command_processed_case_cap = 0;
        cli.custom_command_retry_max_attempts = 0;
        cli.voice_queue_limit = 0;
        cli.voice_processed_case_cap = 0;
        cli.voice_retry_max_attempts = 0;

        let dashboard = build_dashboard_contract_runner_config(&cli);
        let deployment = build_deployment_contract_runner_config(&cli);
        let custom = build_custom_command_contract_runner_config(&cli);
        let voice = build_voice_contract_runner_config(&cli);

        assert_eq!(dashboard.queue_limit, 1);
        assert_eq!(dashboard.processed_case_cap, 1);
        assert_eq!(dashboard.retry_max_attempts, 1);
        assert_eq!(deployment.queue_limit, 1);
        assert_eq!(deployment.processed_case_cap, 1);
        assert_eq!(deployment.retry_max_attempts, 1);
        assert_eq!(custom.queue_limit, 1);
        assert_eq!(custom.processed_case_cap, 1);
        assert_eq!(custom.retry_max_attempts, 1);
        assert_eq!(voice.queue_limit, 1);
        assert_eq!(voice.processed_case_cap, 1);
        assert_eq!(voice.retry_max_attempts, 1);
    }

    #[test]
    fn regression_build_events_runner_cli_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.events_poll_interval_ms = 0;
        cli.events_queue_limit = 0;

        let config = build_events_runner_cli_config(&cli);
        assert_eq!(config.poll_interval_ms, 1);
        assert_eq!(config.queue_limit, 1);
    }

    #[test]
    fn regression_build_slack_bridge_cli_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.slack_thread_detail_threshold_chars = 0;
        cli.slack_processed_event_cap = 0;
        cli.slack_reconnect_delay_ms = 0;
        cli.slack_retry_max_attempts = 0;
        cli.slack_retry_base_delay_ms = 0;

        let config =
            build_slack_bridge_cli_config(&cli, "app-token".to_string(), "bot-token".to_string());
        assert_eq!(config.detail_thread_threshold_chars, 1);
        assert_eq!(config.processed_event_cap, 1);
        assert_eq!(config.reconnect_delay_ms, 1);
        assert_eq!(config.retry_max_attempts, 1);
        assert_eq!(config.retry_base_delay_ms, 1);
    }

    #[test]
    fn regression_build_github_issues_bridge_cli_config_enforces_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.github_poll_interval_seconds = 0;
        cli.github_processed_event_cap = 0;
        cli.github_retry_max_attempts = 0;
        cli.github_retry_base_delay_ms = 0;

        let config = build_github_issues_bridge_cli_config(
            &cli,
            "owner/repo".to_string(),
            "token".to_string(),
        );
        assert_eq!(config.poll_interval_seconds, 1);
        assert_eq!(config.processed_event_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
        assert_eq!(config.retry_base_delay_ms, 1);
    }

    #[test]
    fn functional_build_github_issues_bridge_cli_config_trims_required_labels() {
        let mut cli = parse_cli_with_stack();
        cli.github_required_label = vec![" bug ".to_string(), "triage".to_string()];

        let config = build_github_issues_bridge_cli_config(
            &cli,
            "owner/repo".to_string(),
            "token".to_string(),
        );
        assert_eq!(
            config.required_labels,
            vec!["bug".to_string(), "triage".to_string()]
        );
    }

    #[test]
    fn integration_build_gateway_openresponses_server_config_preserves_runtime_fields() {
        let mut cli = parse_cli_with_stack();
        cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
        cli.gateway_openresponses_auth_password = Some("  secret-pass  ".to_string());
        cli.gateway_openresponses_auth_token = Some("  secret-token  ".to_string());
        cli.gateway_openresponses_bind = "127.0.0.1:9090".to_string();
        cli.max_turns = 7;
        cli.turn_timeout_ms = 20_000;
        cli.gateway_openresponses_session_ttl_seconds = 1_800;
        cli.gateway_openresponses_rate_limit_window_seconds = 120;
        cli.gateway_openresponses_rate_limit_max_requests = 40;
        cli.gateway_openresponses_max_input_chars = 24_000;

        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
        let tool_policy = ToolPolicy::new(vec![]);
        let config = build_gateway_openresponses_server_config(
            &cli,
            client.clone(),
            &model_ref,
            "system prompt",
            &tool_policy,
        );

        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.system_prompt, "system prompt");
        assert_eq!(config.max_turns, 7);
        assert_eq!(config.turn_timeout_ms, 20_000);
        assert_eq!(config.bind, "127.0.0.1:9090");
        assert_eq!(
            config.auth_mode,
            GatewayOpenResponsesAuthMode::PasswordSession
        );
        assert_eq!(config.auth_token.as_deref(), Some("secret-token"));
        assert_eq!(config.auth_password.as_deref(), Some("secret-pass"));
        assert_eq!(config.session_ttl_seconds, 1_800);
        assert_eq!(config.rate_limit_window_seconds, 120);
        assert_eq!(config.rate_limit_max_requests, 40);
        assert_eq!(config.max_input_chars, 24_000);
    }

    #[tokio::test]
    async fn unit_run_gateway_openresponses_server_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client: Arc<dyn LlmClient> = Arc::new(NoopClient);
        let tool_policy = ToolPolicy::new(vec![]);

        let handled = super::run_gateway_openresponses_server_if_requested(
            &cli,
            client,
            &model_ref,
            "system prompt",
            &tool_policy,
        )
        .await
        .expect("gateway helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_gateway_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_gateway_contract_runner_if_requested(&cli)
            .await
            .expect("gateway contract helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_multi_agent_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_multi_agent_contract_runner_if_requested(&cli)
            .await
            .expect("multi-agent contract helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_multi_channel_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_multi_channel_contract_runner_if_requested(
            &cli,
            tau_multi_channel::MultiChannelCommandHandlers::default(),
            Arc::new(AllowAllPairingEvaluator),
        )
        .await
        .expect("multi-channel contract helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_multi_channel_live_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_multi_channel_live_runner_if_requested(
            &cli,
            tau_multi_channel::MultiChannelCommandHandlers::default(),
            Arc::new(AllowAllPairingEvaluator),
        )
        .await
        .expect("multi-channel live helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_browser_automation_contract_runner_if_requested_returns_false_when_disabled()
    {
        let cli = parse_cli_with_stack();

        let handled = super::run_browser_automation_contract_runner_if_requested(&cli)
            .await
            .expect("browser automation helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_memory_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_memory_contract_runner_if_requested(&cli)
            .await
            .expect("memory helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_dashboard_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_dashboard_contract_runner_if_requested(&cli)
            .await
            .expect("dashboard helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_deployment_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_deployment_contract_runner_if_requested(&cli)
            .await
            .expect("deployment helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_custom_command_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_custom_command_contract_runner_if_requested(&cli)
            .await
            .expect("custom command helper");

        assert!(!handled);
    }

    #[tokio::test]
    async fn unit_run_voice_contract_runner_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_voice_contract_runner_if_requested(&cli)
            .await
            .expect("voice helper");

        assert!(!handled);
    }

    #[test]
    fn integration_build_gateway_contract_runner_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.gateway_fixture = temp.path().join("gateway-fixture.json");
        cli.gateway_state_dir = temp.path().join("gateway-state");
        cli.gateway_guardrail_failure_streak_threshold = 7;
        cli.gateway_guardrail_retryable_failures_threshold = 9;

        let config = build_gateway_contract_runner_config(&cli);
        assert_eq!(config.fixture_path, cli.gateway_fixture);
        assert_eq!(config.state_dir, cli.gateway_state_dir);
        assert_eq!(config.queue_limit, 64);
        assert_eq!(config.processed_case_cap, 10_000);
        assert_eq!(config.retry_max_attempts, 4);
        assert_eq!(config.retry_base_delay_ms, 0);
        assert_eq!(config.guardrail_failure_streak_threshold, 7);
        assert_eq!(config.guardrail_retryable_failures_threshold, 9);
    }

    #[test]
    fn integration_build_multi_agent_contract_runner_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.multi_agent_fixture = temp.path().join("multi-agent-fixture.json");
        cli.multi_agent_state_dir = temp.path().join("multi-agent-state");
        cli.multi_agent_queue_limit = 42;
        cli.multi_agent_processed_case_cap = 3_200;
        cli.multi_agent_retry_max_attempts = 8;
        cli.multi_agent_retry_base_delay_ms = 250;

        let config = build_multi_agent_contract_runner_config(&cli);
        assert_eq!(config.fixture_path, cli.multi_agent_fixture);
        assert_eq!(config.state_dir, cli.multi_agent_state_dir);
        assert_eq!(config.queue_limit, 42);
        assert_eq!(config.processed_case_cap, 3_200);
        assert_eq!(config.retry_max_attempts, 8);
        assert_eq!(config.retry_base_delay_ms, 250);
    }

    #[test]
    fn integration_build_browser_automation_contract_runner_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.browser_automation_fixture = temp.path().join("browser-automation-fixture.json");
        cli.browser_automation_state_dir = temp.path().join("browser-automation-state");
        cli.browser_automation_queue_limit = 31;
        cli.browser_automation_processed_case_cap = 5_500;
        cli.browser_automation_retry_max_attempts = 7;
        cli.browser_automation_retry_base_delay_ms = 70;
        cli.browser_automation_action_timeout_ms = 123;
        cli.browser_automation_max_actions_per_case = 9;
        cli.browser_automation_allow_unsafe_actions = true;

        let config = build_browser_automation_contract_runner_config(&cli);
        assert_eq!(config.fixture_path, cli.browser_automation_fixture);
        assert_eq!(config.state_dir, cli.browser_automation_state_dir);
        assert_eq!(config.queue_limit, 31);
        assert_eq!(config.processed_case_cap, 5_500);
        assert_eq!(config.retry_max_attempts, 7);
        assert_eq!(config.retry_base_delay_ms, 70);
        assert_eq!(config.action_timeout_ms, 123);
        assert_eq!(config.max_actions_per_case, 9);
        assert!(config.allow_unsafe_actions);
    }

    #[test]
    fn integration_build_memory_contract_runner_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.memory_fixture = temp.path().join("memory-fixture.json");
        cli.memory_state_dir = temp.path().join("memory-state");
        cli.memory_queue_limit = 37;
        cli.memory_processed_case_cap = 9_100;
        cli.memory_retry_max_attempts = 6;
        cli.memory_retry_base_delay_ms = 45;

        let config = build_memory_contract_runner_config(&cli);
        assert_eq!(config.fixture_path, cli.memory_fixture);
        assert_eq!(config.state_dir, cli.memory_state_dir);
        assert_eq!(config.queue_limit, 37);
        assert_eq!(config.processed_case_cap, 9_100);
        assert_eq!(config.retry_max_attempts, 6);
        assert_eq!(config.retry_base_delay_ms, 45);
    }

    #[test]
    fn integration_build_standard_contract_runner_builders_preserve_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.dashboard_fixture = temp.path().join("dashboard-fixture.json");
        cli.dashboard_state_dir = temp.path().join("dashboard-state");
        cli.dashboard_queue_limit = 33;
        cli.dashboard_processed_case_cap = 9_000;
        cli.dashboard_retry_max_attempts = 7;
        cli.dashboard_retry_base_delay_ms = 31;

        cli.deployment_fixture = temp.path().join("deployment-fixture.json");
        cli.deployment_state_dir = temp.path().join("deployment-state");
        cli.deployment_queue_limit = 19;
        cli.deployment_processed_case_cap = 2_000;
        cli.deployment_retry_max_attempts = 3;
        cli.deployment_retry_base_delay_ms = 8;

        cli.custom_command_fixture = temp.path().join("custom-fixture.json");
        cli.custom_command_state_dir = temp.path().join("custom-state");
        cli.custom_command_queue_limit = 41;
        cli.custom_command_processed_case_cap = 1_234;
        cli.custom_command_retry_max_attempts = 5;
        cli.custom_command_retry_base_delay_ms = 16;

        cli.voice_fixture = temp.path().join("voice-fixture.json");
        cli.voice_state_dir = temp.path().join("voice-state");
        cli.voice_queue_limit = 27;
        cli.voice_processed_case_cap = 4_444;
        cli.voice_retry_max_attempts = 9;
        cli.voice_retry_base_delay_ms = 12;

        let dashboard = build_dashboard_contract_runner_config(&cli);
        let deployment = build_deployment_contract_runner_config(&cli);
        let custom = build_custom_command_contract_runner_config(&cli);
        let voice = build_voice_contract_runner_config(&cli);

        assert_eq!(dashboard.fixture_path, cli.dashboard_fixture);
        assert_eq!(dashboard.state_dir, cli.dashboard_state_dir);
        assert_eq!(dashboard.queue_limit, 33);
        assert_eq!(dashboard.processed_case_cap, 9_000);
        assert_eq!(dashboard.retry_max_attempts, 7);
        assert_eq!(dashboard.retry_base_delay_ms, 31);

        assert_eq!(deployment.fixture_path, cli.deployment_fixture);
        assert_eq!(deployment.state_dir, cli.deployment_state_dir);
        assert_eq!(deployment.queue_limit, 19);
        assert_eq!(deployment.processed_case_cap, 2_000);
        assert_eq!(deployment.retry_max_attempts, 3);
        assert_eq!(deployment.retry_base_delay_ms, 8);

        assert_eq!(custom.fixture_path, cli.custom_command_fixture);
        assert_eq!(custom.state_dir, cli.custom_command_state_dir);
        assert_eq!(custom.queue_limit, 41);
        assert_eq!(custom.processed_case_cap, 1_234);
        assert_eq!(custom.retry_max_attempts, 5);
        assert_eq!(custom.retry_base_delay_ms, 16);

        assert_eq!(voice.fixture_path, cli.voice_fixture);
        assert_eq!(voice.state_dir, cli.voice_state_dir);
        assert_eq!(voice.queue_limit, 27);
        assert_eq!(voice.processed_case_cap, 4_444);
        assert_eq!(voice.retry_max_attempts, 9);
        assert_eq!(voice.retry_base_delay_ms, 12);
    }

    #[test]
    fn integration_build_events_runner_cli_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.channel_store_root = temp.path().join("channel-store");
        cli.events_dir = temp.path().join("events");
        cli.events_state_path = temp.path().join("events-state.json");
        cli.events_poll_interval_ms = 2_500;
        cli.events_queue_limit = 77;
        cli.events_stale_immediate_max_age_seconds = 9_999;

        let config = build_events_runner_cli_config(&cli);
        assert_eq!(config.channel_store_root, cli.channel_store_root);
        assert_eq!(config.events_dir, cli.events_dir);
        assert_eq!(config.state_path, cli.events_state_path);
        assert_eq!(config.poll_interval_ms, 2_500);
        assert_eq!(config.queue_limit, 77);
        assert_eq!(config.stale_immediate_max_age_seconds, 9_999);
    }

    #[test]
    fn integration_build_slack_bridge_cli_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.slack_state_dir = temp.path().join("slack-state");
        cli.slack_api_base = "https://slack.example/api".to_string();
        cli.slack_bot_user_id = Some("U123".to_string());
        cli.slack_thread_detail_output = true;
        cli.slack_thread_detail_threshold_chars = 500;
        cli.slack_processed_event_cap = 250;
        cli.slack_max_event_age_seconds = 3_600;
        cli.slack_reconnect_delay_ms = 2_500;
        cli.slack_retry_max_attempts = 8;
        cli.slack_retry_base_delay_ms = 650;
        cli.slack_artifact_retention_days = 14;

        let config =
            build_slack_bridge_cli_config(&cli, "app-token".to_string(), "bot-token".to_string());
        assert_eq!(config.state_dir, cli.slack_state_dir);
        assert_eq!(config.api_base, "https://slack.example/api");
        assert_eq!(config.app_token, "app-token");
        assert_eq!(config.bot_token, "bot-token");
        assert_eq!(config.bot_user_id.as_deref(), Some("U123"));
        assert!(config.detail_thread_output);
        assert_eq!(config.detail_thread_threshold_chars, 500);
        assert_eq!(config.processed_event_cap, 250);
        assert_eq!(config.max_event_age_seconds, 3_600);
        assert_eq!(config.reconnect_delay_ms, 2_500);
        assert_eq!(config.retry_max_attempts, 8);
        assert_eq!(config.retry_base_delay_ms, 650);
        assert_eq!(config.artifact_retention_days, 14);
    }

    #[test]
    fn integration_build_github_issues_bridge_cli_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.github_state_dir = temp.path().join("github-state");
        cli.github_api_base = "https://github.example/api/v3".to_string();
        cli.github_bot_login = Some("tau-bot".to_string());
        cli.github_poll_interval_seconds = 45;
        cli.github_poll_once = true;
        cli.github_required_label = vec!["ops".to_string(), "triage".to_string()];
        cli.github_issue_number = vec![7, 42];
        cli.github_include_issue_body = true;
        cli.github_include_edited_comments = true;
        cli.github_processed_event_cap = 321;
        cli.github_retry_max_attempts = 9;
        cli.github_retry_base_delay_ms = 700;
        cli.github_artifact_retention_days = 21;

        let config = build_github_issues_bridge_cli_config(
            &cli,
            "owner/repo".to_string(),
            "token-value".to_string(),
        );
        assert_eq!(config.state_dir, cli.github_state_dir);
        assert_eq!(config.repo_slug, "owner/repo");
        assert_eq!(config.api_base, "https://github.example/api/v3");
        assert_eq!(config.token, "token-value");
        assert_eq!(config.bot_login.as_deref(), Some("tau-bot"));
        assert_eq!(config.poll_interval_seconds, 45);
        assert!(config.poll_once);
        assert_eq!(
            config.required_labels,
            vec!["ops".to_string(), "triage".to_string()]
        );
        assert_eq!(config.required_issue_numbers, vec![7, 42]);
        assert!(config.include_issue_body);
        assert!(config.include_edited_comments);
        assert_eq!(config.processed_event_cap, 321);
        assert_eq!(config.retry_max_attempts, 9);
        assert_eq!(config.retry_base_delay_ms, 700);
        assert_eq!(config.artifact_retention_days, 21);
    }

    #[test]
    fn integration_build_multi_channel_contract_runner_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_fixture = temp.path().join("multi-channel-fixture.json");
        cli.multi_channel_state_dir = temp.path().join("multi-channel-state");
        cli.orchestrator_route_table = Some(temp.path().join("route-table.json"));
        cli.multi_channel_queue_limit = 24;
        cli.multi_channel_processed_event_cap = 8_000;
        cli.multi_channel_retry_max_attempts = 5;
        cli.multi_channel_retry_base_delay_ms = 17;
        cli.multi_channel_retry_jitter_ms = 33;

        let config = build_multi_channel_contract_runner_config(
            &cli,
            tau_multi_channel::MultiChannelCommandHandlers::default(),
            Arc::new(AllowAllPairingEvaluator),
        );
        assert_eq!(config.fixture_path, cli.multi_channel_fixture);
        assert_eq!(config.state_dir, cli.multi_channel_state_dir);
        assert_eq!(
            config.orchestrator_route_table_path,
            cli.orchestrator_route_table
        );
        assert_eq!(config.queue_limit, 24);
        assert_eq!(config.processed_event_cap, 8_000);
        assert_eq!(config.retry_max_attempts, 5);
        assert_eq!(config.retry_base_delay_ms, 17);
        assert_eq!(config.retry_jitter_ms, 33);
    }

    #[test]
    fn integration_build_multi_channel_live_runner_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
        cli.multi_channel_state_dir = temp.path().join("multi-channel-state");
        cli.orchestrator_route_table = Some(temp.path().join("route-table.json"));
        cli.multi_channel_queue_limit = 19;
        cli.multi_channel_processed_event_cap = 4_500;
        cli.multi_channel_retry_max_attempts = 6;
        cli.multi_channel_retry_base_delay_ms = 21;
        cli.multi_channel_retry_jitter_ms = 34;

        let config = build_multi_channel_live_runner_config(
            &cli,
            tau_multi_channel::MultiChannelCommandHandlers::default(),
            Arc::new(AllowAllPairingEvaluator),
        );
        assert_eq!(config.ingress_dir, cli.multi_channel_live_ingress_dir);
        assert_eq!(config.state_dir, cli.multi_channel_state_dir);
        assert_eq!(
            config.orchestrator_route_table_path,
            cli.orchestrator_route_table
        );
        assert_eq!(config.queue_limit, 19);
        assert_eq!(config.processed_event_cap, 4_500);
        assert_eq!(config.retry_max_attempts, 6);
        assert_eq!(config.retry_base_delay_ms, 21);
        assert_eq!(config.retry_jitter_ms, 34);
    }

    #[tokio::test]
    async fn unit_run_multi_channel_live_connectors_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();

        let handled = super::run_multi_channel_live_connectors_if_requested(&cli)
            .await
            .expect("connectors helper");

        assert!(!handled);
    }

    #[test]
    fn integration_build_multi_channel_live_connectors_config_preserves_runtime_fields() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
        cli.multi_channel_live_ingress_dir = temp.path().join("ingress");
        cli.multi_channel_processed_event_cap = 0;
        cli.multi_channel_retry_max_attempts = 0;
        cli.multi_channel_retry_base_delay_ms = 42;
        cli.multi_channel_live_connectors_poll_once = true;
        cli.multi_channel_live_webhook_bind = "127.0.0.1:9999".to_string();
        cli.multi_channel_telegram_ingress_mode =
            tau_cli::CliMultiChannelLiveConnectorMode::Polling;
        cli.multi_channel_discord_ingress_mode = tau_cli::CliMultiChannelLiveConnectorMode::Webhook;
        cli.multi_channel_whatsapp_ingress_mode =
            tau_cli::CliMultiChannelLiveConnectorMode::Webhook;
        cli.multi_channel_telegram_api_base = " https://telegram.example ".to_string();
        cli.multi_channel_discord_api_base = " https://discord.example ".to_string();
        cli.multi_channel_telegram_bot_token = Some(" telegram-direct ".to_string());
        cli.multi_channel_discord_ingress_channel_ids =
            vec![" 111 ".to_string(), "222".to_string()];
        cli.multi_channel_telegram_webhook_secret = Some(" tg-secret ".to_string());
        cli.multi_channel_whatsapp_webhook_verify_token = Some(" wa-verify-secret ".to_string());
        cli.multi_channel_whatsapp_webhook_app_secret = Some(" wa-app-secret ".to_string());
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-store"),
            false,
        );

        let config = build_multi_channel_live_connectors_config(&cli);
        assert_eq!(
            config.state_path,
            cli.multi_channel_live_connectors_state_path
        );
        assert_eq!(config.ingress_dir, cli.multi_channel_live_ingress_dir);
        assert_eq!(config.processed_event_cap, 1);
        assert_eq!(config.retry_max_attempts, 1);
        assert_eq!(config.retry_base_delay_ms, 42);
        assert!(config.poll_once);
        assert_eq!(config.webhook_bind, "127.0.0.1:9999");
        assert_eq!(config.telegram_mode, MultiChannelLiveConnectorMode::Polling);
        assert_eq!(config.discord_mode, MultiChannelLiveConnectorMode::Webhook);
        assert_eq!(config.whatsapp_mode, MultiChannelLiveConnectorMode::Webhook);
        assert_eq!(config.telegram_api_base, "https://telegram.example");
        assert_eq!(config.discord_api_base, "https://discord.example");
        assert_eq!(
            config.telegram_bot_token.as_deref(),
            Some("telegram-direct")
        );
        assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
        assert_eq!(config.discord_ingress_channel_ids, vec!["111", "222"]);
        assert_eq!(config.telegram_webhook_secret.as_deref(), Some("tg-secret"));
        assert_eq!(
            config.whatsapp_webhook_verify_token.as_deref(),
            Some("wa-verify-secret")
        );
        assert_eq!(
            config.whatsapp_webhook_app_secret.as_deref(),
            Some("wa-app-secret")
        );
    }

    #[test]
    fn functional_resolve_multi_channel_outbound_secret_reads_active_store_entry() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        write_integration_secret(
            &cli.credential_store,
            "telegram-bot-token",
            Some("telegram-secret"),
            false,
        );

        let resolved = resolve_multi_channel_outbound_secret(&cli, None, "telegram-bot-token");
        assert_eq!(resolved.as_deref(), Some("telegram-secret"));
    }

    #[test]
    fn functional_build_multi_channel_outbound_config_resolves_direct_and_store_values() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_telegram_bot_token = Some(" telegram-direct ".to_string());
        cli.multi_channel_telegram_api_base = " https://telegram.example ".to_string();
        cli.multi_channel_discord_api_base = " https://discord.example ".to_string();
        cli.multi_channel_whatsapp_api_base = " https://whatsapp.example ".to_string();
        cli.multi_channel_outbound_max_chars = 0;
        cli.multi_channel_outbound_http_timeout_ms = 0;
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-store"),
            false,
        );
        write_integration_secret(
            &cli.credential_store,
            "whatsapp-access-token",
            Some("whatsapp-store"),
            false,
        );
        write_integration_secret(
            &cli.credential_store,
            "whatsapp-phone-number-id",
            Some("phone-store"),
            false,
        );

        let config = build_multi_channel_outbound_config(&cli);
        assert_eq!(
            config.telegram_bot_token.as_deref(),
            Some("telegram-direct")
        );
        assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
        assert_eq!(
            config.whatsapp_access_token.as_deref(),
            Some("whatsapp-store")
        );
        assert_eq!(
            config.whatsapp_phone_number_id.as_deref(),
            Some("phone-store")
        );
        assert_eq!(config.telegram_api_base, "https://telegram.example");
        assert_eq!(config.discord_api_base, "https://discord.example");
        assert_eq!(config.whatsapp_api_base, "https://whatsapp.example");
        assert_eq!(config.max_chars, 1);
        assert_eq!(config.http_timeout_ms, 1);
    }

    #[test]
    fn functional_build_multi_channel_live_connectors_config_resolves_store_fallbacks() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_telegram_bot_token = None;
        cli.multi_channel_discord_bot_token = None;
        write_integration_secret(
            &cli.credential_store,
            "telegram-bot-token",
            Some("telegram-store"),
            false,
        );
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-store"),
            false,
        );

        let config = build_multi_channel_live_connectors_config(&cli);
        assert_eq!(config.telegram_bot_token.as_deref(), Some("telegram-store"));
        assert_eq!(config.discord_bot_token.as_deref(), Some("discord-store"));
    }

    #[test]
    fn regression_resolve_multi_channel_outbound_secret_returns_none_for_revoked_entry() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-secret"),
            true,
        );

        let resolved = resolve_multi_channel_outbound_secret(&cli, None, "discord-bot-token");
        assert!(resolved.is_none());
    }

    #[test]
    fn regression_build_multi_channel_telemetry_and_media_config_enforce_minimums() {
        let mut cli = parse_cli_with_stack();
        cli.multi_channel_telemetry_min_response_chars = 0;
        cli.multi_channel_media_max_attachments = 0;
        cli.multi_channel_media_max_summary_chars = 0;

        let telemetry = build_multi_channel_telemetry_config(&cli);
        let media = build_multi_channel_media_config(&cli);

        assert_eq!(telemetry.typing_presence_min_response_chars, 1);
        assert_eq!(media.max_attachments_per_event, 1);
        assert_eq!(media.max_summary_chars, 16);
    }

    #[test]
    fn regression_build_multi_channel_live_connectors_config_ignores_revoked_store_secret() {
        let temp = tempdir().expect("tempdir");
        let mut cli = parse_cli_with_stack();
        cli.credential_store = temp.path().join("credentials.json");
        cli.multi_channel_discord_bot_token = None;
        write_integration_secret(
            &cli.credential_store,
            "discord-bot-token",
            Some("discord-secret"),
            true,
        );

        let config = build_multi_channel_live_connectors_config(&cli);
        assert!(config.discord_bot_token.is_none());
    }

    #[tokio::test]
    async fn unit_run_github_issues_bridge_if_requested_returns_false_when_disabled() {
        let cli = parse_cli_with_stack();
        let resolver_called = Arc::new(Mutex::new(false));
        let resolver_called_sink = Arc::clone(&resolver_called);

        let ran = run_github_issues_bridge_if_requested(
            &cli,
            move || {
                *resolver_called_sink.lock().expect("resolver lock") = true;
                Ok(("owner/repo".to_string(), "token".to_string()))
            },
            |_config| async { Ok(()) },
        )
        .await
        .expect("github helper succeeds");

        assert!(!ran);
        assert!(!*resolver_called.lock().expect("resolver lock"));
    }

    #[tokio::test]
    async fn functional_run_slack_bridge_if_requested_builds_config_with_resolved_tokens() {
        let mut cli = parse_cli_with_stack();
        cli.slack_bridge = true;
        cli.slack_thread_detail_threshold_chars = 0;
        cli.slack_processed_event_cap = 0;
        cli.slack_reconnect_delay_ms = 0;
        cli.slack_retry_max_attempts = 0;
        cli.slack_retry_base_delay_ms = 0;
        let captured = Arc::new(Mutex::new(None::<SlackBridgeCliConfig>));
        let captured_sink = Arc::clone(&captured);

        let ran = run_slack_bridge_if_requested(
            &cli,
            || Ok(("app-token".to_string(), "bot-token".to_string())),
            move |config| {
                let sink = Arc::clone(&captured_sink);
                async move {
                    *sink.lock().expect("capture lock") = Some(config);
                    Ok(())
                }
            },
        )
        .await
        .expect("slack helper succeeds");

        assert!(ran);
        let config = captured
            .lock()
            .expect("capture lock")
            .clone()
            .expect("captured config");
        assert_eq!(config.app_token, "app-token");
        assert_eq!(config.bot_token, "bot-token");
        assert_eq!(config.detail_thread_threshold_chars, 1);
        assert_eq!(config.processed_event_cap, 1);
        assert_eq!(config.reconnect_delay_ms, 1);
        assert_eq!(config.retry_max_attempts, 1);
        assert_eq!(config.retry_base_delay_ms, 1);
    }

    #[tokio::test]
    async fn integration_run_events_runner_if_requested_passes_normalized_config() {
        let mut cli = parse_cli_with_stack();
        cli.events_runner = true;
        cli.events_poll_interval_ms = 0;
        cli.events_queue_limit = 0;
        let captured = Arc::new(Mutex::new(None::<EventsRunnerCliConfig>));
        let captured_sink = Arc::clone(&captured);

        let ran = run_events_runner_if_requested(&cli, move |config| {
            let sink = Arc::clone(&captured_sink);
            async move {
                *sink.lock().expect("capture lock") = Some(config);
                Ok(())
            }
        })
        .await
        .expect("events helper succeeds");

        assert!(ran);
        let config = captured
            .lock()
            .expect("capture lock")
            .clone()
            .expect("captured config");
        assert_eq!(config.poll_interval_ms, 1);
        assert_eq!(config.queue_limit, 1);
    }

    #[tokio::test]
    async fn regression_run_github_issues_bridge_if_requested_propagates_resolver_errors() {
        let mut cli = parse_cli_with_stack();
        cli.github_issues_bridge = true;

        let error = run_github_issues_bridge_if_requested(
            &cli,
            || Err(anyhow::anyhow!("missing bridge credentials")),
            |_config| async { Ok(()) },
        )
        .await
        .expect_err("resolver failures should propagate");

        assert!(
            error.to_string().contains("missing bridge credentials"),
            "unexpected error: {error}"
        );
    }
}
