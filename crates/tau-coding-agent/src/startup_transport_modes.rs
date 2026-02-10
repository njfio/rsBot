use super::*;

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
    validate_multi_agent_contract_runner_cli(cli)?;
    validate_memory_contract_runner_cli(cli)?;
    validate_dashboard_contract_runner_cli(cli)?;
    validate_gateway_contract_runner_cli(cli)?;
    validate_deployment_contract_runner_cli(cli)?;
    validate_custom_command_contract_runner_cli(cli)?;
    validate_voice_contract_runner_cli(cli)?;

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
            state_dir: cli.slack_state_dir.clone(),
            api_base: cli.slack_api_base.clone(),
            app_token,
            bot_token,
            bot_user_id: cli.slack_bot_user_id.clone(),
            detail_thread_output: cli.slack_thread_detail_output,
            detail_thread_threshold_chars: cli.slack_thread_detail_threshold_chars.max(1),
            processed_event_cap: cli.slack_processed_event_cap.max(1),
            max_event_age_seconds: cli.slack_max_event_age_seconds,
            reconnect_delay: Duration::from_millis(cli.slack_reconnect_delay_ms.max(1)),
            retry_max_attempts: cli.slack_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.slack_retry_base_delay_ms.max(1),
            artifact_retention_days: cli.slack_artifact_retention_days,
        })
        .await?;
        return Ok(true);
    }

    if cli.events_runner {
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
            channel_store_root: cli.channel_store_root.clone(),
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            poll_interval: Duration::from_millis(cli.events_poll_interval_ms.max(1)),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        })
        .await?;
        return Ok(true);
    }

    if cli.multi_channel_contract_runner {
        run_multi_channel_contract_runner(MultiChannelRuntimeConfig {
            fixture_path: cli.multi_channel_fixture.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            queue_limit: cli.multi_channel_queue_limit.max(1),
            processed_event_cap: cli.multi_channel_processed_event_cap.max(1),
            retry_max_attempts: cli.multi_channel_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.multi_channel_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.multi_channel_live_runner {
        run_multi_channel_live_runner(MultiChannelLiveRuntimeConfig {
            ingress_dir: cli.multi_channel_live_ingress_dir.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            queue_limit: cli.multi_channel_queue_limit.max(1),
            processed_event_cap: cli.multi_channel_processed_event_cap.max(1),
            retry_max_attempts: cli.multi_channel_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.multi_channel_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.multi_agent_contract_runner {
        run_multi_agent_contract_runner(MultiAgentRuntimeConfig {
            fixture_path: cli.multi_agent_fixture.clone(),
            state_dir: cli.multi_agent_state_dir.clone(),
            queue_limit: cli.multi_agent_queue_limit.max(1),
            processed_case_cap: cli.multi_agent_processed_case_cap.max(1),
            retry_max_attempts: cli.multi_agent_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.multi_agent_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.memory_contract_runner {
        run_memory_contract_runner(MemoryRuntimeConfig {
            fixture_path: cli.memory_fixture.clone(),
            state_dir: cli.memory_state_dir.clone(),
            queue_limit: cli.memory_queue_limit.max(1),
            processed_case_cap: cli.memory_processed_case_cap.max(1),
            retry_max_attempts: cli.memory_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.memory_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.dashboard_contract_runner {
        run_dashboard_contract_runner(DashboardRuntimeConfig {
            fixture_path: cli.dashboard_fixture.clone(),
            state_dir: cli.dashboard_state_dir.clone(),
            queue_limit: cli.dashboard_queue_limit.max(1),
            processed_case_cap: cli.dashboard_processed_case_cap.max(1),
            retry_max_attempts: cli.dashboard_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.dashboard_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.gateway_contract_runner {
        run_gateway_contract_runner(GatewayRuntimeConfig {
            fixture_path: cli.gateway_fixture.clone(),
            state_dir: cli.gateway_state_dir.clone(),
            queue_limit: 64,
            processed_case_cap: 10_000,
            retry_max_attempts: 4,
            retry_base_delay_ms: 0,
            guardrail_failure_streak_threshold: cli
                .gateway_guardrail_failure_streak_threshold
                .max(1),
            guardrail_retryable_failures_threshold: cli
                .gateway_guardrail_retryable_failures_threshold
                .max(1),
        })
        .await?;
        return Ok(true);
    }

    if cli.deployment_contract_runner {
        run_deployment_contract_runner(DeploymentRuntimeConfig {
            fixture_path: cli.deployment_fixture.clone(),
            state_dir: cli.deployment_state_dir.clone(),
            queue_limit: cli.deployment_queue_limit.max(1),
            processed_case_cap: cli.deployment_processed_case_cap.max(1),
            retry_max_attempts: cli.deployment_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.deployment_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.custom_command_contract_runner {
        run_custom_command_contract_runner(CustomCommandRuntimeConfig {
            fixture_path: cli.custom_command_fixture.clone(),
            state_dir: cli.custom_command_state_dir.clone(),
            queue_limit: cli.custom_command_queue_limit.max(1),
            processed_case_cap: cli.custom_command_processed_case_cap.max(1),
            retry_max_attempts: cli.custom_command_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.custom_command_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    if cli.voice_contract_runner {
        run_voice_contract_runner(VoiceRuntimeConfig {
            fixture_path: cli.voice_fixture.clone(),
            state_dir: cli.voice_state_dir.clone(),
            queue_limit: cli.voice_queue_limit.max(1),
            processed_case_cap: cli.voice_processed_case_cap.max(1),
            retry_max_attempts: cli.voice_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.voice_retry_base_delay_ms,
        })
        .await?;
        return Ok(true);
    }

    Ok(false)
}
