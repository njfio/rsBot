use super::*;
use crate::channel_adapters::{
    build_multi_channel_command_handlers, build_multi_channel_pairing_evaluator,
};
use crate::validate_multi_channel_live_connectors_runner_cli;
use std::sync::Arc;
use tau_onboarding::startup_transport_modes::{
    build_browser_automation_contract_runner_config, build_custom_command_contract_runner_config,
    build_dashboard_contract_runner_config, build_deployment_contract_runner_config,
    build_events_runner_cli_config, build_memory_contract_runner_config,
    build_slack_bridge_cli_config, build_voice_contract_runner_config,
    run_gateway_contract_runner_if_requested, run_gateway_openresponses_server_if_requested,
    run_multi_agent_contract_runner_if_requested, run_multi_channel_contract_runner_if_requested,
    run_multi_channel_live_connectors_if_requested, run_multi_channel_live_runner_if_requested,
};

pub(crate) async fn run_transport_mode_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
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

    if run_gateway_openresponses_server_if_requested(
        cli,
        client.clone(),
        model_ref,
        system_prompt,
        tool_policy,
    )
    .await?
    {
        return Ok(true);
    }

    if cli.github_issues_bridge {
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
        run_github_issues_bridge(GithubIssuesBridgeRuntimeConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.to_string(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            state_dir: cli.github_state_dir.clone(),
            repo_slug,
            api_base: cli.github_api_base.clone(),
            token,
            bot_login: cli.github_bot_login.clone(),
            poll_interval: Duration::from_secs(cli.github_poll_interval_seconds.max(1)),
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
            auth_command_config: build_auth_command_config(cli),
            demo_index_repo_root: None,
            demo_index_script_path: None,
            demo_index_binary_path: None,
            doctor_config: {
                let fallback_model_refs = Vec::new();
                let skills_lock_path = default_skills_lock_path(&cli.skills_dir);
                let mut config = build_doctor_command_config(
                    cli,
                    model_ref,
                    &fallback_model_refs,
                    &skills_lock_path,
                );
                config.skills_dir = cli.skills_dir.clone();
                config.skills_lock_path = skills_lock_path;
                config
            },
        })
        .await?;
        return Ok(true);
    }

    if cli.slack_bridge {
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
        let config = build_slack_bridge_cli_config(cli, app_token, bot_token);
        run_slack_bridge(SlackBridgeRuntimeConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.to_string(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
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
        .await?;
        return Ok(true);
    }

    if cli.events_runner {
        let config = build_events_runner_cli_config(cli);
        run_event_scheduler(EventSchedulerConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.to_string(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            channel_store_root: config.channel_store_root,
            events_dir: config.events_dir,
            state_path: config.state_path,
            poll_interval: Duration::from_millis(config.poll_interval_ms),
            queue_limit: config.queue_limit,
            stale_immediate_max_age_seconds: config.stale_immediate_max_age_seconds,
        })
        .await?;
        return Ok(true);
    }

    if cli.multi_channel_contract_runner {
        let fallback_model_refs = Vec::new();
        let skills_lock_path = default_skills_lock_path(&cli.skills_dir);
        let auth_config = build_auth_command_config(cli);
        let doctor_config =
            build_doctor_command_config(cli, model_ref, &fallback_model_refs, &skills_lock_path);
        run_multi_channel_contract_runner_if_requested(
            cli,
            build_multi_channel_command_handlers(auth_config, doctor_config),
            build_multi_channel_pairing_evaluator(),
        )
        .await?;
        return Ok(true);
    }

    if cli.multi_channel_live_runner {
        let fallback_model_refs = Vec::new();
        let skills_lock_path = default_skills_lock_path(&cli.skills_dir);
        let auth_config = build_auth_command_config(cli);
        let doctor_config =
            build_doctor_command_config(cli, model_ref, &fallback_model_refs, &skills_lock_path);
        run_multi_channel_live_runner_if_requested(
            cli,
            build_multi_channel_command_handlers(auth_config, doctor_config),
            build_multi_channel_pairing_evaluator(),
        )
        .await?;
        return Ok(true);
    }

    if run_multi_channel_live_connectors_if_requested(cli).await? {
        return Ok(true);
    }

    if run_multi_agent_contract_runner_if_requested(cli).await? {
        return Ok(true);
    }

    if cli.browser_automation_contract_runner {
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
        return Ok(true);
    }

    if cli.memory_contract_runner {
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
        return Ok(true);
    }

    if cli.dashboard_contract_runner {
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
        return Ok(true);
    }

    if run_gateway_contract_runner_if_requested(cli).await? {
        return Ok(true);
    }

    if cli.deployment_contract_runner {
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
        return Ok(true);
    }

    if cli.custom_command_contract_runner {
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
        return Ok(true);
    }

    if cli.voice_contract_runner {
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
        return Ok(true);
    }

    Ok(false)
}
