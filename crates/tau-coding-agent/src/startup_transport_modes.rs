use std::{sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use tau_ai::{LlmClient, ModelRef};
use tau_cli::Cli;
use tau_github_issues_runtime::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
use tau_onboarding::startup_config::build_auth_command_config;
use tau_onboarding::startup_transport_modes::{
    build_transport_doctor_config as build_onboarding_transport_doctor_config,
    resolve_github_issues_bridge_repo_and_token_from_cli as resolve_onboarding_github_issues_bridge_repo_and_token_from_cli,
    resolve_slack_bridge_tokens_from_cli as resolve_onboarding_slack_bridge_tokens_from_cli,
    run_browser_automation_live_runner_if_requested,
    run_custom_command_contract_runner_if_requested, run_dashboard_contract_runner_if_requested,
    run_deployment_contract_runner_if_requested,
    run_events_runner_with_runtime_defaults_if_requested as run_onboarding_events_runner_with_runtime_defaults_if_requested,
    run_gateway_contract_runner_if_requested, run_gateway_openresponses_server_if_requested,
    run_github_issues_bridge_with_runtime_defaults_if_requested as run_onboarding_github_issues_bridge_with_runtime_defaults_if_requested,
    run_memory_contract_runner_if_requested, run_multi_agent_contract_runner_if_requested,
    run_multi_channel_contract_runner_with_runtime_dependencies_if_requested as run_onboarding_multi_channel_contract_runner_with_runtime_dependencies_if_requested,
    run_multi_channel_live_connectors_if_requested,
    run_multi_channel_live_runner_with_runtime_dependencies_if_requested as run_onboarding_multi_channel_live_runner_with_runtime_dependencies_if_requested,
    run_slack_bridge_with_runtime_defaults_if_requested as run_onboarding_slack_bridge_with_runtime_defaults_if_requested,
    run_transport_mode_if_requested as run_onboarding_transport_mode_if_requested,
    run_voice_contract_runner_if_requested, run_voice_live_runner_if_requested,
    TransportRuntimeExecutor,
};
use tau_provider::resolve_secret_from_cli_or_store_id;
use tau_slack_runtime::{run_slack_bridge, SlackBridgeRuntimeConfig};

use crate::channel_adapters::{
    build_multi_channel_command_handlers, build_multi_channel_pairing_evaluator,
};
use crate::events::{run_event_scheduler, EventSchedulerConfig};
use crate::runtime_types::RenderOptions;
use crate::tools::ToolPolicy;

struct CodingAgentTransportRuntimeExecutor<'a> {
    cli: &'a Cli,
    client: &'a Arc<dyn LlmClient>,
    model_ref: &'a ModelRef,
    system_prompt: &'a str,
    tool_policy: &'a ToolPolicy,
    render_options: RenderOptions,
}

#[async_trait]
impl TransportRuntimeExecutor for CodingAgentTransportRuntimeExecutor<'_> {
    async fn run_gateway_openresponses_server(&self) -> Result<()> {
        run_gateway_openresponses_server_if_requested(
            self.cli,
            self.client.clone(),
            self.model_ref,
            self.system_prompt,
            self.tool_policy,
        )
        .await?;
        Ok(())
    }

    async fn run_github_issues_bridge(&self) -> Result<()> {
        run_onboarding_github_issues_bridge_with_runtime_defaults_if_requested(
            self.cli,
            self.model_ref,
            self.system_prompt,
            || {
                resolve_onboarding_github_issues_bridge_repo_and_token_from_cli(
                    self.cli,
                    |direct_secret, secret_id, secret_id_flag| {
                        resolve_secret_from_cli_or_store_id(
                            self.cli,
                            direct_secret,
                            secret_id,
                            secret_id_flag,
                        )
                    },
                )
            },
            |config, runtime_defaults| async move {
                run_github_issues_bridge(GithubIssuesBridgeRuntimeConfig {
                    client: self.client.clone(),
                    model: runtime_defaults.model,
                    system_prompt: runtime_defaults.system_prompt,
                    max_turns: runtime_defaults.max_turns,
                    tool_policy: self.tool_policy.clone(),
                    turn_timeout_ms: runtime_defaults.turn_timeout_ms,
                    request_timeout_ms: runtime_defaults.request_timeout_ms,
                    render_options: self.render_options,
                    session_lock_wait_ms: runtime_defaults.session_lock_wait_ms,
                    session_lock_stale_ms: runtime_defaults.session_lock_stale_ms,
                    state_dir: config.state_dir,
                    repo_slug: config.repo_slug,
                    api_base: config.api_base,
                    token: config.token,
                    bot_login: config.bot_login,
                    poll_interval: Duration::from_secs(config.poll_interval_seconds),
                    poll_once: config.poll_once,
                    required_labels: config.required_labels,
                    required_issue_numbers: config.required_issue_numbers,
                    include_issue_body: config.include_issue_body,
                    include_edited_comments: config.include_edited_comments,
                    processed_event_cap: config.processed_event_cap,
                    retry_max_attempts: config.retry_max_attempts,
                    retry_base_delay_ms: config.retry_base_delay_ms,
                    artifact_retention_days: config.artifact_retention_days,
                    auth_command_config: build_auth_command_config(self.cli),
                    demo_index_repo_root: None,
                    demo_index_script_path: None,
                    demo_index_binary_path: None,
                    doctor_config: build_onboarding_transport_doctor_config(
                        self.cli,
                        self.model_ref,
                    ),
                })
                .await
            },
        )
        .await?;
        Ok(())
    }

    async fn run_slack_bridge(&self) -> Result<()> {
        run_onboarding_slack_bridge_with_runtime_defaults_if_requested(
            self.cli,
            self.model_ref,
            self.system_prompt,
            || {
                resolve_onboarding_slack_bridge_tokens_from_cli(
                    self.cli,
                    |direct_secret, secret_id, secret_id_flag| {
                        resolve_secret_from_cli_or_store_id(
                            self.cli,
                            direct_secret,
                            secret_id,
                            secret_id_flag,
                        )
                    },
                )
            },
            |config, runtime_defaults| async move {
                run_slack_bridge(SlackBridgeRuntimeConfig {
                    client: self.client.clone(),
                    model: runtime_defaults.model,
                    system_prompt: runtime_defaults.system_prompt,
                    max_turns: runtime_defaults.max_turns,
                    tool_policy: self.tool_policy.clone(),
                    turn_timeout_ms: runtime_defaults.turn_timeout_ms,
                    request_timeout_ms: runtime_defaults.request_timeout_ms,
                    render_options: self.render_options,
                    session_lock_wait_ms: runtime_defaults.session_lock_wait_ms,
                    session_lock_stale_ms: runtime_defaults.session_lock_stale_ms,
                    state_dir: config.state_dir,
                    api_base: config.api_base,
                    app_token: config.app_token,
                    bot_token: config.bot_token,
                    bot_user_id: config.bot_user_id,
                    detail_thread_output: config.detail_thread_output,
                    detail_thread_threshold_chars: config.detail_thread_threshold_chars,
                    processed_event_cap: config.processed_event_cap,
                    max_event_age_seconds: config.max_event_age_seconds,
                    reconnect_delay: Duration::from_millis(config.reconnect_delay_ms),
                    retry_max_attempts: config.retry_max_attempts,
                    retry_base_delay_ms: config.retry_base_delay_ms,
                    artifact_retention_days: config.artifact_retention_days,
                })
                .await
            },
        )
        .await?;
        Ok(())
    }

    async fn run_events_runner(&self) -> Result<()> {
        run_onboarding_events_runner_with_runtime_defaults_if_requested(
            self.cli,
            self.model_ref,
            self.system_prompt,
            |config, runtime_defaults| async move {
                run_event_scheduler(EventSchedulerConfig {
                    client: self.client.clone(),
                    model: runtime_defaults.model,
                    system_prompt: runtime_defaults.system_prompt,
                    max_turns: runtime_defaults.max_turns,
                    tool_policy: self.tool_policy.clone(),
                    turn_timeout_ms: runtime_defaults.turn_timeout_ms,
                    render_options: self.render_options,
                    session_lock_wait_ms: runtime_defaults.session_lock_wait_ms,
                    session_lock_stale_ms: runtime_defaults.session_lock_stale_ms,
                    channel_store_root: config.channel_store_root,
                    events_dir: config.events_dir,
                    state_path: config.state_path,
                    poll_interval: Duration::from_millis(config.poll_interval_ms),
                    queue_limit: config.queue_limit,
                    stale_immediate_max_age_seconds: config.stale_immediate_max_age_seconds,
                })
                .await
            },
        )
        .await?;
        Ok(())
    }

    async fn run_multi_channel_contract_runner(&self) -> Result<()> {
        run_onboarding_multi_channel_contract_runner_with_runtime_dependencies_if_requested(
            self.cli,
            self.model_ref,
            build_multi_channel_command_handlers,
            build_multi_channel_pairing_evaluator,
        )
        .await?;
        Ok(())
    }

    async fn run_multi_channel_live_runner(&self) -> Result<()> {
        run_onboarding_multi_channel_live_runner_with_runtime_dependencies_if_requested(
            self.cli,
            self.model_ref,
            build_multi_channel_command_handlers,
            build_multi_channel_pairing_evaluator,
        )
        .await?;
        Ok(())
    }

    async fn run_multi_channel_live_connectors_runner(&self) -> Result<()> {
        run_multi_channel_live_connectors_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_multi_agent_contract_runner(&self) -> Result<()> {
        run_multi_agent_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_browser_automation_live_runner(&self) -> Result<()> {
        run_browser_automation_live_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_memory_contract_runner(&self) -> Result<()> {
        run_memory_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_dashboard_contract_runner(&self) -> Result<()> {
        run_dashboard_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_gateway_contract_runner(&self) -> Result<()> {
        run_gateway_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_deployment_contract_runner(&self) -> Result<()> {
        run_deployment_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_custom_command_contract_runner(&self) -> Result<()> {
        run_custom_command_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_voice_contract_runner(&self) -> Result<()> {
        run_voice_contract_runner_if_requested(self.cli).await?;
        Ok(())
    }

    async fn run_voice_live_runner(&self) -> Result<()> {
        run_voice_live_runner_if_requested(self.cli).await?;
        Ok(())
    }
}

pub(crate) async fn run_transport_mode_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    let executor = CodingAgentTransportRuntimeExecutor {
        cli,
        client,
        model_ref,
        system_prompt,
        tool_policy,
        render_options,
    };
    run_onboarding_transport_mode_if_requested(cli, &executor).await
}
