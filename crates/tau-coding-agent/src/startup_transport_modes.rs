use super::*;
use crate::channel_adapters::{
    build_multi_channel_command_handlers, build_multi_channel_pairing_evaluator,
};
use async_trait::async_trait;
use std::sync::Arc;
use tau_onboarding::startup_transport_modes::{
    build_multi_channel_runtime_dependencies as build_onboarding_multi_channel_runtime_dependencies,
    build_transport_doctor_config as build_onboarding_transport_doctor_config,
    build_transport_runtime_defaults as build_onboarding_transport_runtime_defaults,
    execute_transport_runtime_mode, resolve_transport_runtime_mode,
    run_browser_automation_contract_runner_if_requested,
    run_custom_command_contract_runner_if_requested, run_dashboard_contract_runner_if_requested,
    run_deployment_contract_runner_if_requested,
    run_events_runner_if_requested as run_onboarding_events_runner_if_requested,
    run_gateway_contract_runner_if_requested, run_gateway_openresponses_server_if_requested,
    run_github_issues_bridge_if_requested as run_onboarding_github_issues_bridge_if_requested,
    run_memory_contract_runner_if_requested, run_multi_agent_contract_runner_if_requested,
    run_multi_channel_contract_runner_if_requested, run_multi_channel_live_connectors_if_requested,
    run_multi_channel_live_runner_if_requested,
    run_slack_bridge_if_requested as run_onboarding_slack_bridge_if_requested,
    run_voice_contract_runner_if_requested, validate_transport_mode_cli, TransportRuntimeExecutor,
};

async fn run_github_issues_bridge_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    let runtime_defaults =
        build_onboarding_transport_runtime_defaults(cli, model_ref, system_prompt);
    run_onboarding_github_issues_bridge_if_requested(
        cli,
        || {
            let repo_slug = cli.github_repo.clone().ok_or_else(|| {
                anyhow!("--github-repo is required when --github-issues-bridge is set")
            })?;
            let token = resolve_secret_from_cli_or_store_id(
                cli,
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
        },
        |config| async move {
            run_github_issues_bridge(GithubIssuesBridgeRuntimeConfig {
                client: client.clone(),
                model: runtime_defaults.model.clone(),
                system_prompt: runtime_defaults.system_prompt.clone(),
                max_turns: runtime_defaults.max_turns,
                tool_policy: tool_policy.clone(),
                turn_timeout_ms: runtime_defaults.turn_timeout_ms,
                request_timeout_ms: runtime_defaults.request_timeout_ms,
                render_options,
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
                auth_command_config: build_auth_command_config(cli),
                demo_index_repo_root: None,
                demo_index_script_path: None,
                demo_index_binary_path: None,
                doctor_config: build_onboarding_transport_doctor_config(cli, model_ref),
            })
            .await
        },
    )
    .await
}

async fn run_slack_bridge_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    let runtime_defaults =
        build_onboarding_transport_runtime_defaults(cli, model_ref, system_prompt);
    run_onboarding_slack_bridge_if_requested(
        cli,
        || {
            let app_token = resolve_secret_from_cli_or_store_id(
                cli,
                cli.slack_app_token.as_deref(),
                cli.slack_app_token_id.as_deref(),
                "--slack-app-token-id",
            )?
            .ok_or_else(|| {
                anyhow!(
                    "--slack-app-token (or --slack-app-token-id) is required when --slack-bridge is set"
                )
            })?;
            let bot_token = resolve_secret_from_cli_or_store_id(
                cli,
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
        },
        |config| async move {
            run_slack_bridge(SlackBridgeRuntimeConfig {
                client: client.clone(),
                model: runtime_defaults.model.clone(),
                system_prompt: runtime_defaults.system_prompt.clone(),
                max_turns: runtime_defaults.max_turns,
                tool_policy: tool_policy.clone(),
                turn_timeout_ms: runtime_defaults.turn_timeout_ms,
                request_timeout_ms: runtime_defaults.request_timeout_ms,
                render_options,
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
    .await
}

async fn run_events_runner_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    let runtime_defaults =
        build_onboarding_transport_runtime_defaults(cli, model_ref, system_prompt);
    run_onboarding_events_runner_if_requested(cli, |config| async move {
        run_event_scheduler(EventSchedulerConfig {
            client: client.clone(),
            model: runtime_defaults.model.clone(),
            system_prompt: runtime_defaults.system_prompt.clone(),
            max_turns: runtime_defaults.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: runtime_defaults.turn_timeout_ms,
            render_options,
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
    })
    .await
}

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
        self::run_github_issues_bridge_if_requested(
            self.cli,
            self.client,
            self.model_ref,
            self.system_prompt,
            self.tool_policy,
            self.render_options,
        )
        .await?;
        Ok(())
    }

    async fn run_slack_bridge(&self) -> Result<()> {
        self::run_slack_bridge_if_requested(
            self.cli,
            self.client,
            self.model_ref,
            self.system_prompt,
            self.tool_policy,
            self.render_options,
        )
        .await?;
        Ok(())
    }

    async fn run_events_runner(&self) -> Result<()> {
        self::run_events_runner_if_requested(
            self.cli,
            self.client,
            self.model_ref,
            self.system_prompt,
            self.tool_policy,
            self.render_options,
        )
        .await?;
        Ok(())
    }

    async fn run_multi_channel_contract_runner(&self) -> Result<()> {
        let (command_handlers, pairing_evaluator) =
            build_onboarding_multi_channel_runtime_dependencies(
                self.cli,
                self.model_ref,
                build_multi_channel_command_handlers,
                build_multi_channel_pairing_evaluator,
            );
        run_multi_channel_contract_runner_if_requested(
            self.cli,
            command_handlers,
            pairing_evaluator,
        )
        .await?;
        Ok(())
    }

    async fn run_multi_channel_live_runner(&self) -> Result<()> {
        let (command_handlers, pairing_evaluator) =
            build_onboarding_multi_channel_runtime_dependencies(
                self.cli,
                self.model_ref,
                build_multi_channel_command_handlers,
                build_multi_channel_pairing_evaluator,
            );
        run_multi_channel_live_runner_if_requested(self.cli, command_handlers, pairing_evaluator)
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

    async fn run_browser_automation_contract_runner(&self) -> Result<()> {
        run_browser_automation_contract_runner_if_requested(self.cli).await?;
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
}

pub(crate) async fn run_transport_mode_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    validate_transport_mode_cli(cli)?;

    let executor = CodingAgentTransportRuntimeExecutor {
        cli,
        client,
        model_ref,
        system_prompt,
        tool_policy,
        render_options,
    };
    execute_transport_runtime_mode(resolve_transport_runtime_mode(cli), &executor).await
}
