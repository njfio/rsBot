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
    if cli.github_issues_bridge || cli.slack_bridge {
        bail!("--events-runner cannot be combined with --github-issues-bridge or --slack-bridge");
    }
    if cli.events_poll_interval_ms == 0 {
        bail!("--events-poll-interval-ms must be greater than 0");
    }
    if cli.events_queue_limit == 0 {
        bail!("--events-queue-limit must be greater than 0");
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
