use anyhow::{anyhow, bail, Context, Result};

use crate::{
    Cli, CliGatewayOpenResponsesAuthMode, CliMultiChannelOutboundMode, CliWebhookSignatureAlgorithm,
};

const MEMORY_CONTRACT_RUNNER_REMOVED_MESSAGE: &str = "--memory-contract-runner has been removed; memory recall is owned by tau-agent-core and user-facing memory tools persist under --memory-state-dir. Use --transport-health-inspect memory for diagnostics.";
const CUSTOM_COMMAND_CONTRACT_RUNNER_REMOVED_MESSAGE: &str = "--custom-command-contract-runner has been removed; preserve existing state under --custom-command-state-dir and use --custom-command-status-inspect plus --transport-health-inspect custom-command for diagnostics";
const BROWSER_AUTOMATION_CONTRACT_RUNNER_REMOVED_MESSAGE: &str = "--browser-automation-contract-runner has been removed; use --browser-automation-live-runner with --browser-automation-live-fixture and --browser-automation-playwright-cli";
const DASHBOARD_CONTRACT_RUNNER_REMOVED_MESSAGE: &str = "--dashboard-contract-runner has been removed; use --gateway-openresponses-server for dashboard API/webchat surfaces and --dashboard-status-inspect plus --transport-health-inspect dashboard for diagnostics";

/// Rejects removed contract-runner flags before any runtime dispatch or mode-specific validation.
pub fn validate_removed_contract_runner_flags_cli(cli: &Cli) -> Result<()> {
    validate_browser_automation_contract_runner_cli(cli)?;
    validate_memory_contract_runner_cli(cli)?;
    validate_dashboard_contract_runner_cli(cli)?;
    validate_custom_command_contract_runner_cli(cli)?;
    Ok(())
}

fn resolve_non_empty_cli_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn has_prompt_or_command_input(cli: &Cli) -> bool {
    cli.prompt.is_some()
        || cli.prompt_file.is_some()
        || cli.prompt_template_file.is_some()
        || cli.command_file.is_some()
}

fn gateway_service_mode_requested(cli: &Cli) -> bool {
    cli.gateway_service_start || cli.gateway_service_stop || cli.gateway_service_status
}

fn daemon_mode_requested(cli: &Cli) -> bool {
    cli.daemon_install
        || cli.daemon_uninstall
        || cli.daemon_start
        || cli.daemon_stop
        || cli.daemon_status
}

fn gateway_openresponses_mode_requested(cli: &Cli) -> bool {
    cli.gateway_openresponses_server
}

fn gateway_remote_profile_inspect_mode_requested(cli: &Cli) -> bool {
    cli.gateway_remote_profile_inspect
}

fn gateway_remote_plan_mode_requested(cli: &Cli) -> bool {
    cli.gateway_remote_plan
}

fn multi_channel_channel_lifecycle_mode_requested(cli: &Cli) -> bool {
    cli.multi_channel_channel_status.is_some()
        || cli.multi_channel_channel_login.is_some()
        || cli.multi_channel_channel_logout.is_some()
        || cli.multi_channel_channel_probe.is_some()
        || cli.multi_channel_channel_probe_online
}

fn multi_channel_send_mode_requested(cli: &Cli) -> bool {
    cli.multi_channel_send.is_some()
}

fn multi_channel_incident_timeline_mode_requested(cli: &Cli) -> bool {
    cli.multi_channel_incident_timeline
}

fn project_index_mode_requested(cli: &Cli) -> bool {
    cli.project_index_build || cli.project_index_query.is_some() || cli.project_index_inspect
}

fn deployment_wasm_mode_requested(cli: &Cli) -> bool {
    cli.deployment_wasm_package_module.is_some()
        || cli.deployment_wasm_inspect_manifest.is_some()
        || cli.deployment_wasm_browser_did_init
}

pub fn validate_project_index_cli(cli: &Cli) -> Result<()> {
    let mode_requested = project_index_mode_requested(cli);
    if !mode_requested && !cli.project_index_json {
        return Ok(());
    }
    if !mode_requested && cli.project_index_json {
        bail!("--project-index-json requires one of --project-index-build, --project-index-query, or --project-index-inspect");
    }

    let action_count = usize::from(cli.project_index_build)
        + usize::from(cli.project_index_query.is_some())
        + usize::from(cli.project_index_inspect);
    if action_count != 1 {
        bail!(
            "project index mode requires exactly one action: --project-index-build, --project-index-query, or --project-index-inspect"
        );
    }
    if has_prompt_or_command_input(cli) {
        bail!(
            "project index commands cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if !cli.project_index_root.exists() {
        bail!(
            "--project-index-root '{}' does not exist",
            cli.project_index_root.display()
        );
    }
    if !cli.project_index_root.is_dir() {
        bail!(
            "--project-index-root '{}' must point to a directory",
            cli.project_index_root.display()
        );
    }
    if cli
        .project_index_query
        .as_deref()
        .map(str::trim)
        .is_some_and(str::is_empty)
    {
        bail!("--project-index-query cannot be empty");
    }

    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
        || multi_channel_channel_lifecycle_mode_requested(cli)
        || cli.multi_channel_route_inspect_file.is_some()
        || cli.multi_channel_incident_timeline
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.gateway_openresponses_server
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
        || cli.browser_automation_preflight
        || cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_remote_profile_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
        || gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
    {
        bail!("project index commands cannot be combined with active transport/runtime or status preflight commands");
    }

    Ok(())
}

pub fn validate_github_issues_bridge_cli(cli: &Cli) -> Result<()> {
    if !cli.github_issues_bridge {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!(
            "--github-issues-bridge cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if cli.no_session {
        bail!("--github-issues-bridge cannot be used together with --no-session");
    }
    if cli.github_poll_interval_seconds == 0 {
        bail!("--github-poll-interval-seconds must be greater than 0");
    }
    if cli.github_processed_event_cap == 0 {
        bail!("--github-processed-event-cap must be greater than 0");
    }
    if cli.github_retry_max_attempts == 0 {
        bail!("--github-retry-max-attempts must be greater than 0");
    }
    if cli.github_retry_base_delay_ms == 0 {
        bail!("--github-retry-base-delay-ms must be greater than 0");
    }
    if cli
        .github_required_label
        .iter()
        .any(|label| label.trim().is_empty())
    {
        bail!("--github-required-label cannot be empty");
    }
    if cli.github_issue_number.contains(&0) {
        bail!("--github-issue-number must be greater than 0");
    }
    if cli
        .github_repo
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        bail!("--github-repo is required when --github-issues-bridge is set");
    }
    let has_github_token = resolve_non_empty_cli_value(cli.github_token.as_deref()).is_some();
    let has_github_token_id = resolve_non_empty_cli_value(cli.github_token_id.as_deref()).is_some();
    if !has_github_token && !has_github_token_id {
        bail!(
            "--github-token (or --github-token-id) is required when --github-issues-bridge is set"
        );
    }
    Ok(())
}

pub fn validate_slack_bridge_cli(cli: &Cli) -> Result<()> {
    if !cli.slack_bridge {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--slack-bridge cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--slack-bridge cannot be used together with --no-session");
    }
    if cli.github_issues_bridge {
        bail!("--slack-bridge cannot be combined with --github-issues-bridge");
    }
    let has_slack_app_token = resolve_non_empty_cli_value(cli.slack_app_token.as_deref()).is_some();
    let has_slack_app_token_id =
        resolve_non_empty_cli_value(cli.slack_app_token_id.as_deref()).is_some();
    if !has_slack_app_token && !has_slack_app_token_id {
        bail!("--slack-app-token (or --slack-app-token-id) is required when --slack-bridge is set");
    }
    let has_slack_bot_token = resolve_non_empty_cli_value(cli.slack_bot_token.as_deref()).is_some();
    let has_slack_bot_token_id =
        resolve_non_empty_cli_value(cli.slack_bot_token_id.as_deref()).is_some();
    if !has_slack_bot_token && !has_slack_bot_token_id {
        bail!("--slack-bot-token (or --slack-bot-token-id) is required when --slack-bridge is set");
    }
    if cli.slack_thread_detail_threshold_chars == 0 {
        bail!("--slack-thread-detail-threshold-chars must be greater than 0");
    }
    if cli.slack_processed_event_cap == 0 {
        bail!("--slack-processed-event-cap must be greater than 0");
    }
    if cli.slack_reconnect_delay_ms == 0 {
        bail!("--slack-reconnect-delay-ms must be greater than 0");
    }
    if cli.slack_retry_max_attempts == 0 {
        bail!("--slack-retry-max-attempts must be greater than 0");
    }
    if cli.slack_retry_base_delay_ms == 0 {
        bail!("--slack-retry-base-delay-ms must be greater than 0");
    }

    Ok(())
}

pub fn validate_events_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.events_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--events-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--events-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge || cli.slack_bridge || cli.memory_contract_runner {
        bail!(
            "--events-runner cannot be combined with --github-issues-bridge, --slack-bridge, or --memory-contract-runner"
        );
    }
    if cli.events_poll_interval_ms == 0 {
        bail!("--events-poll-interval-ms must be greater than 0");
    }
    if cli.events_queue_limit == 0 {
        bail!("--events-queue-limit must be greater than 0");
    }
    Ok(())
}

pub fn validate_multi_channel_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.multi_channel_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--multi-channel-contract-runner cannot be used together with --no-session");
    }
    if cli.multi_channel_live_runner {
        bail!(
            "--multi-channel-contract-runner cannot be combined with --multi-channel-live-runner"
        );
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.memory_contract_runner
    {
        bail!("--multi-channel-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner");
    }
    if cli.multi_channel_queue_limit == 0 {
        bail!("--multi-channel-queue-limit must be greater than 0");
    }
    if cli.multi_channel_processed_event_cap == 0 {
        bail!("--multi-channel-processed-event-cap must be greater than 0");
    }
    if cli.multi_channel_retry_max_attempts == 0 {
        bail!("--multi-channel-retry-max-attempts must be greater than 0");
    }
    if cli.multi_channel_media_max_attachments == 0 {
        bail!("--multi-channel-media-max-attachments must be greater than 0");
    }
    if cli.multi_channel_media_max_summary_chars == 0 {
        bail!("--multi-channel-media-max-summary-chars must be greater than 0");
    }
    if cli.multi_channel_outbound_max_chars == 0 {
        bail!("--multi-channel-outbound-max-chars must be greater than 0");
    }
    if cli.multi_channel_outbound_http_timeout_ms == 0 {
        bail!("--multi-channel-outbound-http-timeout-ms must be greater than 0");
    }
    if cli.multi_channel_telegram_api_base.trim().is_empty() {
        bail!("--multi-channel-telegram-api-base cannot be empty");
    }
    if cli.multi_channel_discord_api_base.trim().is_empty() {
        bail!("--multi-channel-discord-api-base cannot be empty");
    }
    if cli.multi_channel_whatsapp_api_base.trim().is_empty() {
        bail!("--multi-channel-whatsapp-api-base cannot be empty");
    }
    if !cli.multi_channel_fixture.exists() {
        bail!(
            "--multi-channel-fixture '{}' does not exist",
            cli.multi_channel_fixture.display()
        );
    }
    if !cli.multi_channel_fixture.is_file() {
        bail!(
            "--multi-channel-fixture '{}' must point to a file",
            cli.multi_channel_fixture.display()
        );
    }

    Ok(())
}

pub fn validate_multi_channel_live_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.multi_channel_live_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-live-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--multi-channel-live-runner cannot be used together with --no-session");
    }
    if cli.multi_channel_contract_runner {
        bail!(
            "--multi-channel-live-runner cannot be combined with --multi-channel-contract-runner"
        );
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.memory_contract_runner
    {
        bail!("--multi-channel-live-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner");
    }
    if cli.multi_channel_queue_limit == 0 {
        bail!("--multi-channel-queue-limit must be greater than 0");
    }
    if cli.multi_channel_processed_event_cap == 0 {
        bail!("--multi-channel-processed-event-cap must be greater than 0");
    }
    if cli.multi_channel_retry_max_attempts == 0 {
        bail!("--multi-channel-retry-max-attempts must be greater than 0");
    }
    if cli.multi_channel_media_max_attachments == 0 {
        bail!("--multi-channel-media-max-attachments must be greater than 0");
    }
    if cli.multi_channel_media_max_summary_chars == 0 {
        bail!("--multi-channel-media-max-summary-chars must be greater than 0");
    }
    if cli.multi_channel_outbound_max_chars == 0 {
        bail!("--multi-channel-outbound-max-chars must be greater than 0");
    }
    if cli.multi_channel_outbound_http_timeout_ms == 0 {
        bail!("--multi-channel-outbound-http-timeout-ms must be greater than 0");
    }
    if cli.multi_channel_telegram_api_base.trim().is_empty() {
        bail!("--multi-channel-telegram-api-base cannot be empty");
    }
    if cli.multi_channel_discord_api_base.trim().is_empty() {
        bail!("--multi-channel-discord-api-base cannot be empty");
    }
    if cli.multi_channel_whatsapp_api_base.trim().is_empty() {
        bail!("--multi-channel-whatsapp-api-base cannot be empty");
    }
    if !cli.multi_channel_live_ingress_dir.exists() {
        bail!(
            "--multi-channel-live-ingress-dir '{}' does not exist",
            cli.multi_channel_live_ingress_dir.display()
        );
    }
    if !cli.multi_channel_live_ingress_dir.is_dir() {
        bail!(
            "--multi-channel-live-ingress-dir '{}' must point to a directory",
            cli.multi_channel_live_ingress_dir.display()
        );
    }

    Ok(())
}

pub fn validate_multi_channel_live_connectors_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.multi_channel_live_connectors_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-live-connectors-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--multi-channel-live-connectors-runner cannot be used together with --no-session");
    }
    if cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
    {
        bail!("--multi-channel-live-connectors-runner cannot be combined with --multi-channel-contract-runner, --multi-channel-live-runner, --multi-channel-live-ingest-file, or --multi-channel-live-readiness-preflight");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.memory_contract_runner
    {
        bail!("--multi-channel-live-connectors-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner");
    }
    if cli.multi_channel_processed_event_cap == 0 {
        bail!("--multi-channel-processed-event-cap must be greater than 0");
    }
    if cli.multi_channel_retry_max_attempts == 0 {
        bail!("--multi-channel-retry-max-attempts must be greater than 0");
    }
    if cli.multi_channel_live_ingress_dir.exists() && !cli.multi_channel_live_ingress_dir.is_dir() {
        bail!(
            "--multi-channel-live-ingress-dir '{}' must point to a directory when it exists",
            cli.multi_channel_live_ingress_dir.display()
        );
    }
    if cli.multi_channel_live_connectors_state_path.exists()
        && !cli.multi_channel_live_connectors_state_path.is_file()
    {
        bail!(
            "--multi-channel-live-connectors-state-path '{}' must point to a file when it exists",
            cli.multi_channel_live_connectors_state_path.display()
        );
    }

    let telegram_mode = cli.multi_channel_telegram_ingress_mode;
    let discord_mode = cli.multi_channel_discord_ingress_mode;
    let whatsapp_mode = cli.multi_channel_whatsapp_ingress_mode;
    if telegram_mode.is_disabled() && discord_mode.is_disabled() && whatsapp_mode.is_disabled() {
        bail!(
            "at least one connector mode must be enabled: --multi-channel-telegram-ingress-mode, --multi-channel-discord-ingress-mode, or --multi-channel-whatsapp-ingress-mode"
        );
    }
    if discord_mode.is_webhook() {
        bail!("--multi-channel-discord-ingress-mode=webhook is not supported; use polling");
    }
    if whatsapp_mode.is_polling() {
        bail!("--multi-channel-whatsapp-ingress-mode=polling is not supported; use webhook");
    }
    if discord_mode.is_polling()
        && cli
            .multi_channel_discord_ingress_channel_ids
            .iter()
            .all(|value| value.trim().is_empty())
    {
        bail!(
            "--multi-channel-discord-ingress-channel-id is required when --multi-channel-discord-ingress-mode=polling"
        );
    }
    let has_discord_guild_ids = !cli.multi_channel_discord_ingress_guild_ids.is_empty();
    let has_non_empty_discord_guild_ids = cli
        .multi_channel_discord_ingress_guild_ids
        .iter()
        .any(|value| !value.trim().is_empty());
    if has_discord_guild_ids && !has_non_empty_discord_guild_ids {
        bail!("--multi-channel-discord-ingress-guild-id cannot be empty");
    }
    if has_non_empty_discord_guild_ids && !discord_mode.is_polling() {
        bail!(
            "--multi-channel-discord-ingress-guild-id requires --multi-channel-discord-ingress-mode=polling"
        );
    }
    if telegram_mode.is_webhook() || whatsapp_mode.is_webhook() {
        tau_gateway::validate_gateway_openresponses_bind(&cli.multi_channel_live_webhook_bind)
            .with_context(|| {
                format!(
                    "invalid --multi-channel-live-webhook-bind '{}'",
                    cli.multi_channel_live_webhook_bind
                )
            })?;
    }
    if cli.multi_channel_live_connectors_poll_once
        && (telegram_mode.is_webhook() || whatsapp_mode.is_webhook())
    {
        bail!(
            "--multi-channel-live-connectors-poll-once cannot be used with webhook connector modes"
        );
    }

    Ok(())
}

pub fn validate_multi_channel_live_ingest_cli(cli: &Cli) -> Result<()> {
    if cli.multi_channel_live_ingest_file.is_none() {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-live-ingest-file cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--multi-channel-live-ingest-file cannot be used together with --no-session");
    }
    if cli.multi_channel_contract_runner || cli.multi_channel_live_runner {
        bail!("--multi-channel-live-ingest-file cannot be combined with --multi-channel-contract-runner or --multi-channel-live-runner");
    }
    if cli.multi_channel_live_readiness_preflight {
        bail!(
            "--multi-channel-live-ingest-file cannot be combined with --multi-channel-live-readiness-preflight"
        );
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.memory_contract_runner
    {
        bail!("--multi-channel-live-ingest-file cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner");
    }

    let ingest_file = cli
        .multi_channel_live_ingest_file
        .as_ref()
        .ok_or_else(|| anyhow!("--multi-channel-live-ingest-file is required"))?;
    if !ingest_file.exists() {
        bail!(
            "--multi-channel-live-ingest-file '{}' does not exist",
            ingest_file.display()
        );
    }
    if !ingest_file.is_file() {
        bail!(
            "--multi-channel-live-ingest-file '{}' must point to a file",
            ingest_file.display()
        );
    }
    if cli.multi_channel_live_ingest_transport.is_none() {
        bail!(
            "--multi-channel-live-ingest-transport is required when --multi-channel-live-ingest-file is set"
        );
    }
    if cli.multi_channel_live_ingest_provider.trim().is_empty() {
        bail!("--multi-channel-live-ingest-provider cannot be empty");
    }
    if cli.multi_channel_live_ingest_dir.exists() && !cli.multi_channel_live_ingest_dir.is_dir() {
        bail!(
            "--multi-channel-live-ingest-dir '{}' must point to a directory when it exists",
            cli.multi_channel_live_ingest_dir.display()
        );
    }

    Ok(())
}

pub fn validate_multi_channel_incident_timeline_cli(cli: &Cli) -> Result<()> {
    if !multi_channel_incident_timeline_mode_requested(cli) {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-incident-timeline cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.multi_channel_route_inspect_file.is_some()
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_remote_profile_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        bail!("--multi-channel-incident-timeline cannot be combined with status/inspection preflight commands");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || gateway_openresponses_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
        || multi_channel_channel_lifecycle_mode_requested(cli)
        || multi_channel_send_mode_requested(cli)
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || project_index_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!("--multi-channel-incident-timeline cannot be combined with active transport/runtime commands");
    }

    let start = cli.multi_channel_incident_start_unix_ms;
    let end = cli.multi_channel_incident_end_unix_ms;
    if let (Some(start_unix_ms), Some(end_unix_ms)) = (start, end) {
        if end_unix_ms < start_unix_ms {
            bail!(
                "--multi-channel-incident-end-unix-ms must be greater than or equal to --multi-channel-incident-start-unix-ms"
            );
        }
    }
    if let Some(limit) = cli.multi_channel_incident_event_limit {
        if limit == 0 {
            bail!("--multi-channel-incident-event-limit must be greater than 0");
        }
    }
    if cli.multi_channel_state_dir.exists() && !cli.multi_channel_state_dir.is_dir() {
        bail!(
            "--multi-channel-state-dir '{}' must point to a directory when it exists",
            cli.multi_channel_state_dir.display()
        );
    }
    if let Some(path) = cli.multi_channel_incident_replay_export.as_ref() {
        if path.exists() && path.is_dir() {
            bail!(
                "--multi-channel-incident-replay-export '{}' must point to a file path",
                path.display()
            );
        }
    }

    Ok(())
}

pub fn validate_multi_channel_channel_lifecycle_cli(cli: &Cli) -> Result<()> {
    if !multi_channel_channel_lifecycle_mode_requested(cli) {
        return Ok(());
    }

    let selected_modes = [
        cli.multi_channel_channel_status.is_some(),
        cli.multi_channel_channel_login.is_some(),
        cli.multi_channel_channel_logout.is_some(),
        cli.multi_channel_channel_probe.is_some(),
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected_modes > 1 {
        bail!("--multi-channel-channel-status, --multi-channel-channel-login, --multi-channel-channel-logout, and --multi-channel-channel-probe are mutually exclusive");
    }
    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-channel-* commands cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.multi_channel_route_inspect_file.is_some()
        || cli.multi_channel_incident_timeline
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_remote_profile_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        bail!("--multi-channel-channel-* commands cannot be combined with status/inspection preflight commands");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || gateway_openresponses_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--multi-channel-channel-* commands cannot be combined with active transport/runtime commands"
        );
    }
    if cli.multi_channel_channel_status_json && cli.multi_channel_channel_status.is_none() {
        bail!("--multi-channel-channel-status-json requires --multi-channel-channel-status");
    }
    if cli.multi_channel_channel_login_json && cli.multi_channel_channel_login.is_none() {
        bail!("--multi-channel-channel-login-json requires --multi-channel-channel-login");
    }
    if cli.multi_channel_channel_logout_json && cli.multi_channel_channel_logout.is_none() {
        bail!("--multi-channel-channel-logout-json requires --multi-channel-channel-logout");
    }
    if cli.multi_channel_channel_probe_json && cli.multi_channel_channel_probe.is_none() {
        bail!("--multi-channel-channel-probe-json requires --multi-channel-channel-probe");
    }
    if cli.multi_channel_channel_probe_online && cli.multi_channel_channel_probe.is_none() {
        bail!("--multi-channel-channel-probe-online requires --multi-channel-channel-probe");
    }
    if cli.multi_channel_state_dir.exists() && !cli.multi_channel_state_dir.is_dir() {
        bail!(
            "--multi-channel-state-dir '{}' must point to a directory when it exists",
            cli.multi_channel_state_dir.display()
        );
    }
    if cli.multi_channel_live_ingress_dir.exists() && !cli.multi_channel_live_ingress_dir.is_dir() {
        bail!(
            "--multi-channel-live-ingress-dir '{}' must point to a directory when it exists",
            cli.multi_channel_live_ingress_dir.display()
        );
    }

    Ok(())
}

pub fn validate_multi_channel_send_cli(cli: &Cli) -> Result<()> {
    if !multi_channel_send_mode_requested(cli) {
        if cli.multi_channel_send_json
            || cli.multi_channel_send_target.is_some()
            || cli.multi_channel_send_text.is_some()
            || cli.multi_channel_send_text_file.is_some()
        {
            bail!("--multi-channel-send-* flags require --multi-channel-send");
        }
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-channel-send cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.multi_channel_route_inspect_file.is_some()
        || cli.multi_channel_incident_timeline
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_remote_profile_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        bail!("--multi-channel-send cannot be combined with status/inspection preflight commands");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || gateway_openresponses_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
        || multi_channel_channel_lifecycle_mode_requested(cli)
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || project_index_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!("--multi-channel-send cannot be combined with active transport/runtime commands");
    }

    let target = cli
        .multi_channel_send_target
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("--multi-channel-send-target is required"))?;
    if target.len() > 256 {
        bail!("--multi-channel-send-target exceeds 256 characters");
    }

    let has_text_inline = cli
        .multi_channel_send_text
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let has_text_file = cli.multi_channel_send_text_file.is_some();
    if !has_text_inline && !has_text_file {
        bail!("multi-channel send requires --multi-channel-send-text or --multi-channel-send-text-file");
    }
    if has_text_file {
        let text_path = cli
            .multi_channel_send_text_file
            .as_ref()
            .ok_or_else(|| anyhow!("--multi-channel-send-text-file is required"))?;
        if !text_path.exists() {
            bail!(
                "--multi-channel-send-text-file '{}' does not exist",
                text_path.display()
            );
        }
        if !text_path.is_file() {
            bail!(
                "--multi-channel-send-text-file '{}' must point to a file",
                text_path.display()
            );
        }
    }

    if cli.multi_channel_outbound_mode == CliMultiChannelOutboundMode::ChannelStore {
        bail!("--multi-channel-send requires --multi-channel-outbound-mode=dry-run or provider");
    }
    if cli.multi_channel_state_dir.exists() && !cli.multi_channel_state_dir.is_dir() {
        bail!(
            "--multi-channel-state-dir '{}' must point to a directory when it exists",
            cli.multi_channel_state_dir.display()
        );
    }

    Ok(())
}

pub fn validate_multi_agent_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.multi_agent_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--multi-agent-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--multi-agent-contract-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
    {
        bail!("--multi-agent-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --memory-contract-runner, or --dashboard-contract-runner");
    }
    if cli.multi_agent_queue_limit == 0 {
        bail!("--multi-agent-queue-limit must be greater than 0");
    }
    if cli.multi_agent_processed_case_cap == 0 {
        bail!("--multi-agent-processed-case-cap must be greater than 0");
    }
    if cli.multi_agent_retry_max_attempts == 0 {
        bail!("--multi-agent-retry-max-attempts must be greater than 0");
    }
    if !cli.multi_agent_fixture.exists() {
        bail!(
            "--multi-agent-fixture '{}' does not exist",
            cli.multi_agent_fixture.display()
        );
    }
    if !cli.multi_agent_fixture.is_file() {
        bail!(
            "--multi-agent-fixture '{}' must point to a file",
            cli.multi_agent_fixture.display()
        );
    }

    Ok(())
}

pub fn validate_browser_automation_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.browser_automation_contract_runner {
        return Ok(());
    }

    bail!(BROWSER_AUTOMATION_CONTRACT_RUNNER_REMOVED_MESSAGE);
}

pub fn validate_browser_automation_live_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.browser_automation_live_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--browser-automation-live-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--browser-automation-live-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--browser-automation-live-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --browser-automation-contract-runner, --browser-automation-preflight, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --deployment-contract-runner, --custom-command-contract-runner, --voice-contract-runner, or --voice-live-runner"
        );
    }
    if cli.browser_automation_playwright_cli.trim().is_empty() {
        bail!("--browser-automation-playwright-cli cannot be empty");
    }
    if !cli.browser_automation_live_fixture.exists() {
        bail!(
            "--browser-automation-live-fixture '{}' does not exist",
            cli.browser_automation_live_fixture.display()
        );
    }
    if !cli.browser_automation_live_fixture.is_file() {
        bail!(
            "--browser-automation-live-fixture '{}' must point to a file",
            cli.browser_automation_live_fixture.display()
        );
    }

    Ok(())
}

pub fn validate_browser_automation_preflight_cli(cli: &Cli) -> Result<()> {
    if !cli.browser_automation_preflight {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--browser-automation-preflight cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_live_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--browser-automation-preflight cannot be combined with active transport/runtime flags"
        );
    }
    if cli.browser_automation_playwright_cli.trim().is_empty() {
        bail!("--browser-automation-playwright-cli cannot be empty");
    }

    Ok(())
}

pub fn validate_memory_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.memory_contract_runner {
        return Ok(());
    }

    bail!(MEMORY_CONTRACT_RUNNER_REMOVED_MESSAGE);
}

pub fn validate_dashboard_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.dashboard_contract_runner {
        return Ok(());
    }

    bail!(DASHBOARD_CONTRACT_RUNNER_REMOVED_MESSAGE);
}

pub fn validate_daemon_cli(cli: &Cli) -> Result<()> {
    if !daemon_mode_requested(cli) {
        return Ok(());
    }

    let selected_modes = [
        cli.daemon_install,
        cli.daemon_uninstall,
        cli.daemon_start,
        cli.daemon_stop,
        cli.daemon_status,
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected_modes > 1 {
        bail!(
            "--daemon-install, --daemon-uninstall, --daemon-start, --daemon-stop, and --daemon-status are mutually exclusive"
        );
    }
    if has_prompt_or_command_input(cli) {
        bail!(
            "--daemon-* commands cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_remote_profile_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        bail!("--daemon-* commands cannot be combined with status/inspection preflight commands");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.gateway_openresponses_server
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
        || gateway_service_mode_requested(cli)
    {
        bail!("--daemon-* commands cannot be combined with active transport/runtime flags");
    }
    if cli.daemon_status_json && !cli.daemon_status {
        bail!("--daemon-status-json requires --daemon-status");
    }
    if cli.daemon_stop {
        let stop_reason = cli.daemon_stop_reason.as_deref().unwrap_or_default();
        if !stop_reason.is_empty() && stop_reason.trim().is_empty() {
            bail!("--daemon-stop-reason cannot be empty or whitespace");
        }
    }
    if cli.daemon_state_dir.exists() && !cli.daemon_state_dir.is_dir() {
        bail!(
            "--daemon-state-dir '{}' must point to a directory when it exists",
            cli.daemon_state_dir.display()
        );
    }

    Ok(())
}

pub fn validate_gateway_service_cli(cli: &Cli) -> Result<()> {
    if !gateway_service_mode_requested(cli) {
        return Ok(());
    }

    let selected_modes = [
        cli.gateway_service_start,
        cli.gateway_service_stop,
        cli.gateway_service_status,
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if selected_modes > 1 {
        bail!(
            "--gateway-service-start, --gateway-service-stop, and --gateway-service-status are mutually exclusive"
        );
    }
    if has_prompt_or_command_input(cli) {
        bail!(
            "--gateway-service-* commands cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
        || daemon_mode_requested(cli)
    {
        bail!(
            "--gateway-service-* commands cannot be combined with active transport runtime flags"
        );
    }
    if cli.gateway_service_status_json && !cli.gateway_service_status {
        bail!("--gateway-service-status-json requires --gateway-service-status");
    }
    if cli.gateway_service_stop {
        let stop_reason = cli
            .gateway_service_stop_reason
            .as_deref()
            .unwrap_or_default();
        if !stop_reason.is_empty() && stop_reason.trim().is_empty() {
            bail!("--gateway-service-stop-reason cannot be empty or whitespace");
        }
    }

    Ok(())
}

pub fn validate_gateway_remote_profile_inspect_cli(cli: &Cli) -> Result<()> {
    if !gateway_remote_profile_inspect_mode_requested(cli) {
        if cli.gateway_remote_profile_json {
            bail!("--gateway-remote-profile-json requires --gateway-remote-profile-inspect");
        }
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!(
            "--gateway-remote-profile-inspect cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_status_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        bail!(
            "--gateway-remote-profile-inspect cannot be combined with status/inspection preflight commands"
        );
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
        || multi_channel_channel_lifecycle_mode_requested(cli)
        || cli.multi_channel_route_inspect_file.is_some()
        || cli.multi_channel_incident_timeline
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || project_index_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--gateway-remote-profile-inspect cannot be combined with active transport/runtime commands"
        );
    }

    crate::gateway_remote_profile::evaluate_gateway_remote_profile(cli)?;
    crate::gateway_remote_profile::validate_gateway_remote_profile_for_openresponses(cli)?;
    Ok(())
}

pub fn validate_gateway_remote_plan_cli(cli: &Cli) -> Result<()> {
    if !gateway_remote_plan_mode_requested(cli) {
        if cli.gateway_remote_plan_json {
            bail!("--gateway-remote-plan-json requires --gateway-remote-plan");
        }
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!(
            "--gateway-remote-plan cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if cli.channel_store_inspect.is_some()
        || cli.channel_store_repair.is_some()
        || cli.transport_health_inspect.is_some()
        || cli.github_status_inspect.is_some()
        || cli.operator_control_summary
        || cli.multi_channel_status_inspect
        || cli.dashboard_status_inspect
        || cli.multi_agent_status_inspect
        || cli.gateway_status_inspect
        || cli.gateway_remote_profile_inspect
        || cli.deployment_status_inspect
        || cli.custom_command_status_inspect
        || cli.voice_status_inspect
    {
        bail!("--gateway-remote-plan cannot be combined with status/inspection preflight commands");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_channel_live_connectors_runner
        || cli.multi_channel_live_ingest_file.is_some()
        || cli.multi_channel_live_readiness_preflight
        || multi_channel_channel_lifecycle_mode_requested(cli)
        || multi_channel_send_mode_requested(cli)
        || multi_channel_incident_timeline_mode_requested(cli)
        || cli.multi_channel_route_inspect_file.is_some()
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || deployment_wasm_mode_requested(cli)
        || project_index_mode_requested(cli)
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!("--gateway-remote-plan cannot be combined with active transport/runtime commands");
    }

    crate::gateway_remote_profile::evaluate_gateway_remote_plan(cli)?;
    Ok(())
}

pub fn validate_gateway_openresponses_server_cli(cli: &Cli) -> Result<()> {
    if !gateway_openresponses_mode_requested(cli) {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!(
            "--gateway-openresponses-server cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file"
        );
    }
    if cli.no_session {
        bail!("--gateway-openresponses-server cannot be used together with --no-session");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--gateway-openresponses-server cannot be combined with gateway service/daemon commands or other active transport runtime flags"
        );
    }
    let auth_token = cli
        .gateway_openresponses_auth_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth_token_id = cli
        .gateway_openresponses_auth_token_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth_password = cli
        .gateway_openresponses_auth_password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth_password_id = cli
        .gateway_openresponses_auth_password_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let has_auth_token = auth_token.is_some() || auth_token_id.is_some();
    let has_auth_password = auth_password.is_some() || auth_password_id.is_some();
    if cli.gateway_openresponses_max_input_chars == 0 {
        bail!("--gateway-openresponses-max-input-chars must be greater than 0");
    }
    if cli.gateway_openresponses_session_ttl_seconds == 0 {
        bail!("--gateway-openresponses-session-ttl-seconds must be greater than 0");
    }
    if cli.gateway_openresponses_rate_limit_window_seconds == 0 {
        bail!("--gateway-openresponses-rate-limit-window-seconds must be greater than 0");
    }
    if cli.gateway_openresponses_rate_limit_max_requests == 0 {
        bail!("--gateway-openresponses-rate-limit-max-requests must be greater than 0");
    }

    let bind = tau_gateway::validate_gateway_openresponses_bind(&cli.gateway_openresponses_bind)?;
    match cli.gateway_openresponses_auth_mode {
        CliGatewayOpenResponsesAuthMode::Token => {
            if !has_auth_token {
                bail!(
                    "--gateway-openresponses-auth-token (or --gateway-openresponses-auth-token-id) is required when --gateway-openresponses-auth-mode=token"
                );
            }
        }
        CliGatewayOpenResponsesAuthMode::PasswordSession => {
            if !has_auth_password {
                bail!(
                    "--gateway-openresponses-auth-password (or --gateway-openresponses-auth-password-id) is required when --gateway-openresponses-auth-mode=password-session"
                );
            }
        }
        CliGatewayOpenResponsesAuthMode::LocalhostDev => {
            if !bind.ip().is_loopback() {
                bail!(
                    "--gateway-openresponses-auth-mode=localhost-dev requires loopback bind address"
                );
            }
        }
    }
    crate::gateway_remote_profile::validate_gateway_remote_profile_for_openresponses(cli)?;
    Ok(())
}

pub fn validate_gateway_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.gateway_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--gateway-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--gateway-contract-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
    {
        bail!("--gateway-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, or --dashboard-contract-runner");
    }
    if !cli.gateway_fixture.exists() {
        bail!(
            "--gateway-fixture '{}' does not exist",
            cli.gateway_fixture.display()
        );
    }
    if !cli.gateway_fixture.is_file() {
        bail!(
            "--gateway-fixture '{}' must point to a file",
            cli.gateway_fixture.display()
        );
    }
    if cli.gateway_guardrail_failure_streak_threshold == 0 {
        bail!("--gateway-guardrail-failure-streak-threshold must be greater than 0");
    }
    if cli.gateway_guardrail_retryable_failures_threshold == 0 {
        bail!("--gateway-guardrail-retryable-failures-threshold must be greater than 0");
    }

    Ok(())
}

pub fn validate_deployment_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.deployment_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--deployment-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--deployment-contract-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!("--deployment-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --custom-command-contract-runner, or --voice-contract-runner");
    }
    if cli.deployment_queue_limit == 0 {
        bail!("--deployment-queue-limit must be greater than 0");
    }
    if cli.deployment_processed_case_cap == 0 {
        bail!("--deployment-processed-case-cap must be greater than 0");
    }
    if cli.deployment_retry_max_attempts == 0 {
        bail!("--deployment-retry-max-attempts must be greater than 0");
    }
    if !cli.deployment_fixture.exists() {
        bail!(
            "--deployment-fixture '{}' does not exist",
            cli.deployment_fixture.display()
        );
    }
    if !cli.deployment_fixture.is_file() {
        bail!(
            "--deployment-fixture '{}' must point to a file",
            cli.deployment_fixture.display()
        );
    }

    Ok(())
}

pub fn validate_deployment_wasm_package_cli(cli: &Cli) -> Result<()> {
    if cli.deployment_wasm_package_module.is_none() {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--deployment-wasm-package-module cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || gateway_openresponses_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--deployment-wasm-package-module cannot be combined with active transport/runtime commands"
        );
    }
    if cli.deployment_wasm_package_blueprint_id.trim().is_empty() {
        bail!("--deployment-wasm-package-blueprint-id cannot be empty");
    }
    let module_path = cli
        .deployment_wasm_package_module
        .as_ref()
        .ok_or_else(|| anyhow!("--deployment-wasm-package-module is required"))?;
    if !module_path.exists() {
        bail!(
            "--deployment-wasm-package-module '{}' does not exist",
            module_path.display()
        );
    }
    if !module_path.is_file() {
        bail!(
            "--deployment-wasm-package-module '{}' must point to a file",
            module_path.display()
        );
    }
    if cli.deployment_wasm_package_output_dir.exists()
        && !cli.deployment_wasm_package_output_dir.is_dir()
    {
        bail!(
            "--deployment-wasm-package-output-dir '{}' must point to a directory when it exists",
            cli.deployment_wasm_package_output_dir.display()
        );
    }
    if cli.deployment_state_dir.exists() && !cli.deployment_state_dir.is_dir() {
        bail!(
            "--deployment-state-dir '{}' must point to a directory when it exists",
            cli.deployment_state_dir.display()
        );
    }
    Ok(())
}

pub fn validate_deployment_wasm_inspect_cli(cli: &Cli) -> Result<()> {
    if cli.deployment_wasm_inspect_manifest.is_none() {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--deployment-wasm-inspect-manifest cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || gateway_openresponses_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.deployment_wasm_package_module.is_some()
        || cli.deployment_wasm_browser_did_init
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--deployment-wasm-inspect-manifest cannot be combined with active transport/runtime commands"
        );
    }
    let manifest_path = cli
        .deployment_wasm_inspect_manifest
        .as_ref()
        .ok_or_else(|| anyhow!("--deployment-wasm-inspect-manifest is required"))?;
    if !manifest_path.exists() {
        bail!(
            "--deployment-wasm-inspect-manifest '{}' does not exist",
            manifest_path.display()
        );
    }
    if !manifest_path.is_file() {
        bail!(
            "--deployment-wasm-inspect-manifest '{}' must point to a file",
            manifest_path.display()
        );
    }
    Ok(())
}

pub fn validate_deployment_wasm_browser_did_init_cli(cli: &Cli) -> Result<()> {
    if !cli.deployment_wasm_browser_did_init {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--deployment-wasm-browser-did-init cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if gateway_service_mode_requested(cli)
        || daemon_mode_requested(cli)
        || gateway_openresponses_mode_requested(cli)
        || cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.browser_automation_contract_runner
        || cli.browser_automation_preflight
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.deployment_contract_runner
        || cli.deployment_wasm_package_module.is_some()
        || cli.deployment_wasm_inspect_manifest.is_some()
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
        || cli.voice_live_runner
    {
        bail!(
            "--deployment-wasm-browser-did-init cannot be combined with active transport/runtime commands"
        );
    }
    if cli.deployment_wasm_browser_did_network.trim().is_empty() {
        bail!("--deployment-wasm-browser-did-network cannot be empty");
    }
    if cli.deployment_wasm_browser_did_subject.trim().is_empty() {
        bail!("--deployment-wasm-browser-did-subject cannot be empty");
    }
    if cli.deployment_wasm_browser_did_entropy.trim().is_empty() {
        bail!("--deployment-wasm-browser-did-entropy cannot be empty");
    }
    if cli.deployment_wasm_browser_did_output.exists()
        && !cli.deployment_wasm_browser_did_output.is_file()
    {
        bail!(
            "--deployment-wasm-browser-did-output '{}' must point to a file path",
            cli.deployment_wasm_browser_did_output.display()
        );
    }
    Ok(())
}

pub fn validate_custom_command_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.custom_command_contract_runner {
        return Ok(());
    }

    bail!(CUSTOM_COMMAND_CONTRACT_RUNNER_REMOVED_MESSAGE);
}

pub fn validate_voice_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.voice_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--voice-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--voice-contract-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_live_runner
    {
        bail!("--voice-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --custom-command-contract-runner, or --voice-live-runner");
    }
    if cli.voice_queue_limit == 0 {
        bail!("--voice-queue-limit must be greater than 0");
    }
    if cli.voice_processed_case_cap == 0 {
        bail!("--voice-processed-case-cap must be greater than 0");
    }
    if cli.voice_retry_max_attempts == 0 {
        bail!("--voice-retry-max-attempts must be greater than 0");
    }
    if !cli.voice_fixture.exists() {
        bail!(
            "--voice-fixture '{}' does not exist",
            cli.voice_fixture.display()
        );
    }
    if !cli.voice_fixture.is_file() {
        bail!(
            "--voice-fixture '{}' must point to a file",
            cli.voice_fixture.display()
        );
    }

    Ok(())
}

pub fn validate_voice_live_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.voice_live_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--voice-live-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--voice-live-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.multi_agent_contract_runner
        || cli.memory_contract_runner
        || cli.dashboard_contract_runner
        || cli.gateway_contract_runner
        || cli.custom_command_contract_runner
        || cli.voice_contract_runner
    {
        bail!("--voice-live-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --custom-command-contract-runner, or --voice-contract-runner");
    }
    if cli.voice_live_wake_word.trim().is_empty() {
        bail!("--voice-live-wake-word cannot be empty");
    }
    if cli.voice_live_max_turns == 0 {
        bail!("--voice-live-max-turns must be greater than 0");
    }
    if !cli.voice_live_input.exists() {
        bail!(
            "--voice-live-input '{}' does not exist",
            cli.voice_live_input.display()
        );
    }
    if !cli.voice_live_input.is_file() {
        bail!(
            "--voice-live-input '{}' must point to a file",
            cli.voice_live_input.display()
        );
    }

    Ok(())
}

pub fn validate_event_webhook_ingest_cli(cli: &Cli) -> Result<()> {
    if cli.event_webhook_ingest_file.is_none() {
        return Ok(());
    }
    if cli.events_runner {
        bail!("--event-webhook-ingest-file cannot be combined with --events-runner");
    }
    if cli
        .event_webhook_channel
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        bail!("--event-webhook-channel is required when --event-webhook-ingest-file is set");
    }
    if cli.event_webhook_debounce_window_seconds == 0 {
        bail!("--event-webhook-debounce-window-seconds must be greater than 0");
    }

    let signing_configured = cli.event_webhook_signature.is_some()
        || cli.event_webhook_timestamp.is_some()
        || cli.event_webhook_secret.is_some()
        || cli.event_webhook_secret_id.is_some()
        || cli.event_webhook_signature_algorithm.is_some();
    if signing_configured {
        if cli
            .event_webhook_signature
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            bail!(
                "--event-webhook-signature is required when webhook signature verification is configured"
            );
        }
        let has_webhook_secret =
            resolve_non_empty_cli_value(cli.event_webhook_secret.as_deref()).is_some();
        let has_webhook_secret_id =
            resolve_non_empty_cli_value(cli.event_webhook_secret_id.as_deref()).is_some();
        if !has_webhook_secret && !has_webhook_secret_id {
            bail!("--event-webhook-secret (or --event-webhook-secret-id) is required when webhook signature verification is configured");
        }
        match cli.event_webhook_signature_algorithm {
            Some(CliWebhookSignatureAlgorithm::GithubSha256) => {}
            Some(CliWebhookSignatureAlgorithm::SlackV0) => {
                if cli
                    .event_webhook_timestamp
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
                {
                    bail!(
                        "--event-webhook-timestamp is required when --event-webhook-signature-algorithm=slack-v0"
                    );
                }
            }
            None => {
                bail!(
                    "--event-webhook-signature-algorithm is required when webhook signature verification is configured"
                );
            }
        }
    }
    Ok(())
}
