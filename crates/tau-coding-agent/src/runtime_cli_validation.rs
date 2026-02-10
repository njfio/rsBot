use super::*;

fn has_prompt_or_command_input(cli: &Cli) -> bool {
    cli.prompt.is_some()
        || cli.prompt_file.is_some()
        || cli.prompt_template_file.is_some()
        || cli.command_file.is_some()
}

pub(crate) fn validate_github_issues_bridge_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_slack_bridge_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_events_runner_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_multi_channel_contract_runner_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_multi_channel_live_runner_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_multi_agent_contract_runner_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_memory_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.memory_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--memory-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--memory-contract-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
    {
        bail!("--memory-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, or --multi-channel-live-runner");
    }
    if cli.memory_queue_limit == 0 {
        bail!("--memory-queue-limit must be greater than 0");
    }
    if cli.memory_processed_case_cap == 0 {
        bail!("--memory-processed-case-cap must be greater than 0");
    }
    if cli.memory_retry_max_attempts == 0 {
        bail!("--memory-retry-max-attempts must be greater than 0");
    }
    if !cli.memory_fixture.exists() {
        bail!(
            "--memory-fixture '{}' does not exist",
            cli.memory_fixture.display()
        );
    }
    if !cli.memory_fixture.is_file() {
        bail!(
            "--memory-fixture '{}' must point to a file",
            cli.memory_fixture.display()
        );
    }

    Ok(())
}

pub(crate) fn validate_dashboard_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.dashboard_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--dashboard-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--dashboard-contract-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge
        || cli.slack_bridge
        || cli.events_runner
        || cli.multi_channel_contract_runner
        || cli.multi_channel_live_runner
        || cli.memory_contract_runner
    {
        bail!("--dashboard-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, or --memory-contract-runner");
    }
    if cli.dashboard_queue_limit == 0 {
        bail!("--dashboard-queue-limit must be greater than 0");
    }
    if cli.dashboard_processed_case_cap == 0 {
        bail!("--dashboard-processed-case-cap must be greater than 0");
    }
    if cli.dashboard_retry_max_attempts == 0 {
        bail!("--dashboard-retry-max-attempts must be greater than 0");
    }
    if !cli.dashboard_fixture.exists() {
        bail!(
            "--dashboard-fixture '{}' does not exist",
            cli.dashboard_fixture.display()
        );
    }
    if !cli.dashboard_fixture.is_file() {
        bail!(
            "--dashboard-fixture '{}' must point to a file",
            cli.dashboard_fixture.display()
        );
    }

    Ok(())
}

pub(crate) fn validate_gateway_contract_runner_cli(cli: &Cli) -> Result<()> {
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

    Ok(())
}

pub(crate) fn validate_deployment_contract_runner_cli(cli: &Cli) -> Result<()> {
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

pub(crate) fn validate_custom_command_contract_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.custom_command_contract_runner {
        return Ok(());
    }

    if has_prompt_or_command_input(cli) {
        bail!("--custom-command-contract-runner cannot be combined with --prompt, --prompt-file, --prompt-template-file, or --command-file");
    }
    if cli.no_session {
        bail!("--custom-command-contract-runner cannot be used together with --no-session");
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
    {
        bail!("--custom-command-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, or --gateway-contract-runner");
    }
    if cli.custom_command_queue_limit == 0 {
        bail!("--custom-command-queue-limit must be greater than 0");
    }
    if cli.custom_command_processed_case_cap == 0 {
        bail!("--custom-command-processed-case-cap must be greater than 0");
    }
    if cli.custom_command_retry_max_attempts == 0 {
        bail!("--custom-command-retry-max-attempts must be greater than 0");
    }
    if !cli.custom_command_fixture.exists() {
        bail!(
            "--custom-command-fixture '{}' does not exist",
            cli.custom_command_fixture.display()
        );
    }
    if !cli.custom_command_fixture.is_file() {
        bail!(
            "--custom-command-fixture '{}' must point to a file",
            cli.custom_command_fixture.display()
        );
    }

    Ok(())
}

pub(crate) fn validate_voice_contract_runner_cli(cli: &Cli) -> Result<()> {
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
    {
        bail!("--voice-contract-runner cannot be combined with --github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, or --custom-command-contract-runner");
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

pub(crate) fn validate_event_webhook_ingest_cli(cli: &Cli) -> Result<()> {
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
