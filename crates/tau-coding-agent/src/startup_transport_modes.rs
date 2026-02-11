use super::*;
use crate::channel_adapters::{
    build_multi_channel_command_handlers, build_multi_channel_pairing_evaluator,
};
use std::sync::Arc;
use tau_onboarding::startup_transport_modes::{
    build_events_runner_cli_config, build_github_issues_bridge_cli_config,
    build_slack_bridge_cli_config, resolve_bridge_transport_mode, resolve_contract_transport_mode,
    resolve_multi_channel_transport_mode, run_browser_automation_contract_runner_if_requested,
    run_custom_command_contract_runner_if_requested, run_dashboard_contract_runner_if_requested,
    run_deployment_contract_runner_if_requested, run_gateway_contract_runner_if_requested,
    run_gateway_openresponses_server_if_requested, run_memory_contract_runner_if_requested,
    run_multi_agent_contract_runner_if_requested, run_multi_channel_contract_runner_if_requested,
    run_multi_channel_live_connectors_if_requested, run_multi_channel_live_runner_if_requested,
    run_voice_contract_runner_if_requested, validate_transport_mode_cli, BridgeTransportMode,
    ContractTransportMode, MultiChannelTransportMode,
};

fn build_multi_channel_runtime_dependencies(
    cli: &Cli,
    model_ref: &ModelRef,
) -> (
    tau_multi_channel::MultiChannelCommandHandlers,
    Arc<dyn tau_multi_channel::MultiChannelPairingEvaluator>,
) {
    let fallback_model_refs = Vec::new();
    let skills_lock_path = default_skills_lock_path(&cli.skills_dir);
    let auth_config = build_auth_command_config(cli);
    let doctor_config =
        build_doctor_command_config(cli, model_ref, &fallback_model_refs, &skills_lock_path);
    (
        build_multi_channel_command_handlers(auth_config, doctor_config),
        build_multi_channel_pairing_evaluator(),
    )
}

fn build_transport_doctor_config(
    cli: &Cli,
    model_ref: &ModelRef,
) -> tau_diagnostics::DoctorCommandConfig {
    let fallback_model_refs = Vec::new();
    let skills_lock_path = default_skills_lock_path(&cli.skills_dir);
    let mut config =
        build_doctor_command_config(cli, model_ref, &fallback_model_refs, &skills_lock_path);
    config.skills_dir = cli.skills_dir.clone();
    config.skills_lock_path = skills_lock_path;
    config
}

async fn run_github_issues_bridge_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    if !cli.github_issues_bridge {
        return Ok(false);
    }

    let repo_slug = cli
        .github_repo
        .clone()
        .ok_or_else(|| anyhow!("--github-repo is required when --github-issues-bridge is set"))?;
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
    let config = build_github_issues_bridge_cli_config(cli, repo_slug, token);

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
        doctor_config: build_transport_doctor_config(cli, model_ref),
    })
    .await?;

    Ok(true)
}

async fn run_slack_bridge_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    if !cli.slack_bridge {
        return Ok(false);
    }

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

    Ok(true)
}

async fn run_events_runner_if_requested(
    cli: &Cli,
    client: &Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    tool_policy: &ToolPolicy,
    render_options: RenderOptions,
) -> Result<bool> {
    if !cli.events_runner {
        return Ok(false);
    }

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

    Ok(true)
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

    match resolve_bridge_transport_mode(cli) {
        BridgeTransportMode::GithubIssuesBridge => {
            run_github_issues_bridge_if_requested(
                cli,
                client,
                model_ref,
                system_prompt,
                tool_policy,
                render_options,
            )
            .await?;
            return Ok(true);
        }
        BridgeTransportMode::SlackBridge => {
            run_slack_bridge_if_requested(
                cli,
                client,
                model_ref,
                system_prompt,
                tool_policy,
                render_options,
            )
            .await?;
            return Ok(true);
        }
        BridgeTransportMode::EventsRunner => {
            run_events_runner_if_requested(
                cli,
                client,
                model_ref,
                system_prompt,
                tool_policy,
                render_options,
            )
            .await?;
            return Ok(true);
        }
        BridgeTransportMode::None => {}
    }

    match resolve_multi_channel_transport_mode(cli) {
        MultiChannelTransportMode::ContractRunner => {
            let (command_handlers, pairing_evaluator) =
                build_multi_channel_runtime_dependencies(cli, model_ref);
            run_multi_channel_contract_runner_if_requested(
                cli,
                command_handlers,
                pairing_evaluator,
            )
            .await?;
            return Ok(true);
        }
        MultiChannelTransportMode::LiveRunner => {
            let (command_handlers, pairing_evaluator) =
                build_multi_channel_runtime_dependencies(cli, model_ref);
            run_multi_channel_live_runner_if_requested(cli, command_handlers, pairing_evaluator)
                .await?;
            return Ok(true);
        }
        MultiChannelTransportMode::LiveConnectorsRunner => {
            run_multi_channel_live_connectors_if_requested(cli).await?;
            return Ok(true);
        }
        MultiChannelTransportMode::None => {}
    }

    match resolve_contract_transport_mode(cli) {
        ContractTransportMode::MultiAgent => {
            run_multi_agent_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::BrowserAutomation => {
            run_browser_automation_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::Memory => {
            run_memory_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::Dashboard => {
            run_dashboard_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::Gateway => {
            run_gateway_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::Deployment => {
            run_deployment_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::CustomCommand => {
            run_custom_command_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::Voice => {
            run_voice_contract_runner_if_requested(cli).await?;
            return Ok(true);
        }
        ContractTransportMode::None => {}
    }

    Ok(false)
}
