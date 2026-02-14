//! CLI validation tests for transport/runtime mode flag compatibility and guardrails.

use std::path::{Path, PathBuf};

use tau_cli::validation::validate_project_index_cli;
use tau_cli::{
    CliCredentialStoreEncryptionMode, CliDaemonProfile, CliDeploymentWasmRuntimeProfile,
    CliGatewayOpenResponsesAuthMode, CliGatewayRemoteProfile, CliMultiChannelLiveConnectorMode,
    CliMultiChannelOutboundMode, CliMultiChannelTransport, CliProviderAuthMode,
};

use super::{parse_cli_with_stack, try_parse_cli_with_stack};

#[test]
fn unit_cli_provider_retry_flags_accept_explicit_baseline_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--provider-max-retries",
        "2",
        "--provider-retry-budget-ms",
        "0",
        "--provider-retry-jitter",
        "true",
    ]);
    assert_eq!(cli.provider_max_retries, 2);
    assert_eq!(cli.provider_retry_budget_ms, 0);
    assert!(cli.provider_retry_jitter);
}

#[test]
fn functional_cli_provider_retry_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--provider-max-retries",
        "5",
        "--provider-retry-budget-ms",
        "1500",
        "--provider-retry-jitter",
        "false",
    ]);
    assert_eq!(cli.provider_max_retries, 5);
    assert_eq!(cli.provider_retry_budget_ms, 1500);
    assert!(!cli.provider_retry_jitter);
}

#[test]
fn unit_cli_agent_cost_budget_flags_default_values_are_stable() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert_eq!(cli.agent_cost_budget_usd, None);
    assert_eq!(cli.agent_cost_alert_threshold_percent, vec![80, 100]);
}

#[test]
fn functional_cli_agent_cost_budget_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--agent-cost-budget-usd",
        "25.5",
        "--agent-cost-alert-threshold-percent",
        "40,75,95",
    ]);
    assert_eq!(cli.agent_cost_budget_usd, Some(25.5));
    assert_eq!(cli.agent_cost_alert_threshold_percent, vec![40, 75, 95]);
}

#[test]
fn regression_cli_agent_cost_budget_rejects_non_positive_values() {
    let error = try_parse_cli_with_stack(["tau-rs", "--agent-cost-budget-usd", "0"])
        .expect_err("zero budget should be rejected");
    assert!(error
        .to_string()
        .contains("value must be a finite number greater than 0"));
}

#[test]
fn regression_cli_agent_cost_threshold_rejects_out_of_range_values() {
    let error =
        try_parse_cli_with_stack(["tau-rs", "--agent-cost-alert-threshold-percent", "0,101"])
            .expect_err("out-of-range threshold should be rejected");
    assert!(error.to_string().contains("value must be in range 1..=100"));
}

#[test]
fn unit_cli_model_catalog_flags_default_values_are_stable() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert_eq!(cli.model_catalog_url, None);
    assert_eq!(
        cli.model_catalog_cache,
        PathBuf::from(".tau/models/catalog.json")
    );
    assert!(!cli.model_catalog_offline);
    assert_eq!(cli.model_catalog_stale_after_hours, 24);
}

#[test]
fn functional_cli_model_catalog_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--model-catalog-url",
        "https://example.com/models.json",
        "--model-catalog-cache",
        "/tmp/catalog.json",
        "--model-catalog-offline=true",
        "--model-catalog-stale-after-hours",
        "48",
    ]);
    assert_eq!(
        cli.model_catalog_url.as_deref(),
        Some("https://example.com/models.json")
    );
    assert_eq!(cli.model_catalog_cache, PathBuf::from("/tmp/catalog.json"));
    assert!(cli.model_catalog_offline);
    assert_eq!(cli.model_catalog_stale_after_hours, 48);
}

#[test]
fn unit_cli_provider_auth_mode_flags_default_to_api_key() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert_eq!(cli.openai_auth_mode, CliProviderAuthMode::ApiKey);
    assert_eq!(cli.anthropic_auth_mode, CliProviderAuthMode::ApiKey);
    assert_eq!(cli.google_auth_mode, CliProviderAuthMode::ApiKey);
}

#[test]
fn functional_cli_provider_auth_mode_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--openai-auth-mode",
        "oauth-token",
        "--anthropic-auth-mode",
        "session-token",
        "--google-auth-mode",
        "adc",
    ]);
    assert_eq!(cli.openai_auth_mode, CliProviderAuthMode::OauthToken);
    assert_eq!(cli.anthropic_auth_mode, CliProviderAuthMode::SessionToken);
    assert_eq!(cli.google_auth_mode, CliProviderAuthMode::Adc);
}

#[test]
fn unit_cli_provider_subscription_strict_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.provider_subscription_strict);
}

#[test]
fn functional_cli_provider_subscription_strict_accepts_overrides() {
    let cli = parse_cli_with_stack(["tau-rs", "--provider-subscription-strict=true"]);
    assert!(cli.provider_subscription_strict);
}

#[test]
fn unit_cli_openai_codex_backend_flags_default_to_enabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.openai_codex_backend);
    assert_eq!(cli.openai_codex_cli, "codex");
    assert!(cli.openai_codex_args.is_empty());
    assert_eq!(cli.openai_codex_timeout_ms, 120_000);
}

#[test]
fn functional_cli_openai_codex_backend_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--openai-codex-backend=false",
        "--openai-codex-cli",
        "/tmp/mock-codex",
        "--openai-codex-args=--json,--profile,test",
        "--openai-codex-timeout-ms",
        "9000",
    ]);
    assert!(!cli.openai_codex_backend);
    assert_eq!(cli.openai_codex_cli, "/tmp/mock-codex");
    assert_eq!(
        cli.openai_codex_args,
        vec![
            "--json".to_string(),
            "--profile".to_string(),
            "test".to_string()
        ]
    );
    assert_eq!(cli.openai_codex_timeout_ms, 9000);
}

#[test]
fn unit_cli_anthropic_claude_backend_flags_default_to_enabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.anthropic_claude_backend);
    assert_eq!(cli.anthropic_claude_cli, "claude");
    assert!(cli.anthropic_claude_args.is_empty());
    assert_eq!(cli.anthropic_claude_timeout_ms, 120_000);
}

#[test]
fn functional_cli_anthropic_claude_backend_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--anthropic-claude-backend=false",
        "--anthropic-claude-cli",
        "/tmp/mock-claude",
        "--anthropic-claude-args=--print,--verbose",
        "--anthropic-claude-timeout-ms",
        "8000",
    ]);
    assert!(!cli.anthropic_claude_backend);
    assert_eq!(cli.anthropic_claude_cli, "/tmp/mock-claude");
    assert_eq!(
        cli.anthropic_claude_args,
        vec!["--print".to_string(), "--verbose".to_string()]
    );
    assert_eq!(cli.anthropic_claude_timeout_ms, 8000);
}

#[test]
fn unit_cli_google_gemini_backend_flags_default_to_enabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.google_gemini_backend);
    assert_eq!(cli.google_gemini_cli, "gemini");
    assert_eq!(cli.google_gcloud_cli, "gcloud");
    assert!(cli.google_gemini_args.is_empty());
    assert_eq!(cli.google_gemini_timeout_ms, 120_000);
}

#[test]
fn functional_cli_google_gemini_backend_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--google-gemini-backend=false",
        "--google-gemini-cli",
        "/tmp/mock-gemini",
        "--google-gcloud-cli",
        "/tmp/mock-gcloud",
        "--google-gemini-args=--sandbox,readonly,--profile,test",
        "--google-gemini-timeout-ms",
        "7000",
    ]);
    assert!(!cli.google_gemini_backend);
    assert_eq!(cli.google_gemini_cli, "/tmp/mock-gemini");
    assert_eq!(cli.google_gcloud_cli, "/tmp/mock-gcloud");
    assert_eq!(
        cli.google_gemini_args,
        vec![
            "--sandbox".to_string(),
            "readonly".to_string(),
            "--profile".to_string(),
            "test".to_string()
        ]
    );
    assert_eq!(cli.google_gemini_timeout_ms, 7000);
}

#[test]
fn unit_cli_credential_store_flags_default_to_auto_mode_and_default_path() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert_eq!(cli.credential_store, PathBuf::from(".tau/credentials.json"));
    assert!(cli.credential_store_key.is_none());
    assert_eq!(
        cli.credential_store_encryption,
        CliCredentialStoreEncryptionMode::Auto
    );
}

#[test]
fn functional_cli_credential_store_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--credential-store",
        "custom/credentials.json",
        "--credential-store-key",
        "secret-store-key",
        "--credential-store-encryption",
        "keyed",
    ]);
    assert_eq!(
        cli.credential_store,
        PathBuf::from("custom/credentials.json")
    );
    assert_eq!(
        cli.credential_store_key.as_deref(),
        Some("secret-store-key")
    );
    assert_eq!(
        cli.credential_store_encryption,
        CliCredentialStoreEncryptionMode::Keyed
    );
}

#[test]
fn unit_cli_integration_secret_id_flags_default_to_none() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.event_webhook_secret_id.is_none());
    assert!(cli.github_token_id.is_none());
    assert!(cli.slack_app_token_id.is_none());
    assert!(cli.slack_bot_token_id.is_none());
}

#[test]
fn functional_cli_integration_secret_id_flags_accept_explicit_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--event-webhook-ingest-file",
        "payload.json",
        "--github-issues-bridge",
        "--slack-bridge",
        "--event-webhook-secret-id",
        "event-webhook-secret",
        "--github-token-id",
        "github-token",
        "--slack-app-token-id",
        "slack-app-token",
        "--slack-bot-token-id",
        "slack-bot-token",
    ]);
    assert_eq!(
        cli.event_webhook_secret_id.as_deref(),
        Some("event-webhook-secret")
    );
    assert_eq!(cli.github_token_id.as_deref(), Some("github-token"));
    assert_eq!(cli.slack_app_token_id.as_deref(), Some("slack-app-token"));
    assert_eq!(cli.slack_bot_token_id.as_deref(), Some("slack-bot-token"));
}

#[test]
fn unit_cli_artifact_retention_flags_default_to_30_days() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.github_poll_once);
    assert_eq!(cli.github_artifact_retention_days, 30);
    assert_eq!(cli.slack_artifact_retention_days, 30);
}

#[test]
fn functional_cli_artifact_retention_flags_accept_explicit_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--github-issues-bridge",
        "--slack-bridge",
        "--github-poll-once",
        "--github-artifact-retention-days",
        "14",
        "--slack-artifact-retention-days",
        "0",
    ]);
    assert!(cli.github_poll_once);
    assert_eq!(cli.github_artifact_retention_days, 14);
    assert_eq!(cli.slack_artifact_retention_days, 0);
}

#[test]
fn regression_cli_github_poll_once_accepts_explicit_false() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--github-issues-bridge",
        "--github-poll-once=false",
    ]);
    assert!(!cli.github_poll_once);
}

#[test]
fn unit_cli_github_required_label_defaults_empty() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.github_required_label.is_empty());
}

#[test]
fn functional_cli_github_required_label_accepts_repeat_and_csv_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--github-issues-bridge",
        "--github-required-label",
        "tau-ready",
        "--github-required-label",
        "ops,triage",
    ]);
    assert_eq!(
        cli.github_required_label,
        vec![
            "tau-ready".to_string(),
            "ops".to_string(),
            "triage".to_string()
        ]
    );
}

#[test]
fn unit_cli_github_issue_number_defaults_empty() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.github_issue_number.is_empty());
}

#[test]
fn functional_cli_github_issue_number_accepts_repeat_and_csv_values() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--github-issues-bridge",
        "--github-issue-number",
        "7",
        "--github-issue-number",
        "9,11",
    ]);
    assert_eq!(cli.github_issue_number, vec![7, 9, 11]);
}

#[test]
fn regression_cli_github_issue_number_rejects_zero() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--github-issues-bridge",
        "--github-issue-number",
        "0",
    ]);
    let error = parse.expect_err("zero issue number should be rejected");
    assert!(error.to_string().contains("value must be greater than 0"));
}

#[test]
fn unit_cli_multi_channel_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.multi_channel_contract_runner);
    assert!(!cli.multi_channel_live_runner);
    assert!(!cli.multi_channel_live_connectors_runner);
    assert!(!cli.multi_channel_live_connectors_status);
    assert!(!cli.multi_channel_live_connectors_status_json);
    assert!(!cli.multi_channel_live_connectors_poll_once);
    assert_eq!(
        cli.multi_channel_live_connectors_state_path,
        PathBuf::from(".tau/multi-channel/live-connectors-state.json")
    );
    assert_eq!(cli.multi_channel_live_webhook_bind, "127.0.0.1:8788");
    assert_eq!(
        cli.multi_channel_telegram_ingress_mode,
        CliMultiChannelLiveConnectorMode::Disabled
    );
    assert_eq!(
        cli.multi_channel_discord_ingress_mode,
        CliMultiChannelLiveConnectorMode::Disabled
    );
    assert_eq!(
        cli.multi_channel_whatsapp_ingress_mode,
        CliMultiChannelLiveConnectorMode::Disabled
    );
    assert!(cli.multi_channel_discord_ingress_channel_ids.is_empty());
    assert!(cli.multi_channel_telegram_webhook_secret.is_none());
    assert!(cli.multi_channel_whatsapp_webhook_verify_token.is_none());
    assert!(cli.multi_channel_whatsapp_webhook_app_secret.is_none());
    assert!(cli.multi_channel_live_ingest_file.is_none());
    assert!(cli.multi_channel_live_ingest_transport.is_none());
    assert_eq!(cli.multi_channel_live_ingest_provider, "native-ingress");
    assert_eq!(
        cli.multi_channel_live_ingest_dir,
        PathBuf::from(".tau/multi-channel/live-ingress")
    );
    assert!(!cli.multi_channel_live_readiness_preflight);
    assert!(!cli.multi_channel_live_readiness_json);
    assert!(cli.multi_channel_channel_status.is_none());
    assert!(!cli.multi_channel_channel_status_json);
    assert!(cli.multi_channel_channel_login.is_none());
    assert!(!cli.multi_channel_channel_login_json);
    assert!(cli.multi_channel_channel_logout.is_none());
    assert!(!cli.multi_channel_channel_logout_json);
    assert!(cli.multi_channel_channel_probe.is_none());
    assert!(!cli.multi_channel_channel_probe_json);
    assert!(!cli.multi_channel_channel_probe_online);
    assert!(cli.multi_channel_send.is_none());
    assert!(cli.multi_channel_send_target.is_none());
    assert!(cli.multi_channel_send_text.is_none());
    assert!(cli.multi_channel_send_text_file.is_none());
    assert!(!cli.multi_channel_send_json);
    assert_eq!(
        cli.multi_channel_fixture,
        PathBuf::from(
            "crates/tau-multi-channel/testdata/multi-channel-contract/baseline-three-channel.json"
        )
    );
    assert_eq!(
        cli.multi_channel_live_ingress_dir,
        PathBuf::from(".tau/multi-channel/live-ingress")
    );
    assert_eq!(
        cli.multi_channel_state_dir,
        PathBuf::from(".tau/multi-channel")
    );
    assert_eq!(cli.multi_channel_queue_limit, 64);
    assert_eq!(cli.multi_channel_processed_event_cap, 10_000);
    assert_eq!(cli.multi_channel_retry_max_attempts, 4);
    assert_eq!(cli.multi_channel_retry_base_delay_ms, 0);
    assert_eq!(cli.multi_channel_retry_jitter_ms, 0);
    assert!(cli.multi_channel_telemetry_typing_presence);
    assert!(cli.multi_channel_telemetry_usage_summary);
    assert!(!cli.multi_channel_telemetry_include_identifiers);
    assert_eq!(cli.multi_channel_telemetry_min_response_chars, 120);
    assert_eq!(
        cli.multi_channel_outbound_mode,
        CliMultiChannelOutboundMode::ChannelStore
    );
    assert_eq!(cli.multi_channel_outbound_max_chars, 1200);
    assert_eq!(cli.multi_channel_outbound_http_timeout_ms, 5000);
    assert_eq!(
        cli.multi_channel_telegram_api_base,
        "https://api.telegram.org".to_string()
    );
    assert_eq!(
        cli.multi_channel_discord_api_base,
        "https://discord.com/api/v10".to_string()
    );
    assert_eq!(
        cli.multi_channel_whatsapp_api_base,
        "https://graph.facebook.com/v20.0".to_string()
    );
}

#[test]
fn functional_cli_multi_channel_channel_lifecycle_flags_accept_explicit_overrides() {
    let status_cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-channel-status",
        "telegram",
        "--multi-channel-channel-status-json",
    ]);
    assert_eq!(
        status_cli.multi_channel_channel_status,
        Some(CliMultiChannelTransport::Telegram)
    );
    assert!(status_cli.multi_channel_channel_status_json);

    let login_cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-channel-login",
        "discord",
        "--multi-channel-channel-login-json",
    ]);
    assert_eq!(
        login_cli.multi_channel_channel_login,
        Some(CliMultiChannelTransport::Discord)
    );
    assert!(login_cli.multi_channel_channel_login_json);

    let logout_cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-channel-logout",
        "whatsapp",
        "--multi-channel-channel-logout-json",
    ]);
    assert_eq!(
        logout_cli.multi_channel_channel_logout,
        Some(CliMultiChannelTransport::Whatsapp)
    );
    assert!(logout_cli.multi_channel_channel_logout_json);

    let probe_cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-channel-probe",
        "telegram",
        "--multi-channel-channel-probe-json",
        "--multi-channel-channel-probe-online",
    ]);
    assert_eq!(
        probe_cli.multi_channel_channel_probe,
        Some(CliMultiChannelTransport::Telegram)
    );
    assert!(probe_cli.multi_channel_channel_probe_json);
    assert!(probe_cli.multi_channel_channel_probe_online);

    let send_cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-send",
        "discord",
        "--multi-channel-send-target",
        "123456789012345678",
        "--multi-channel-send-text",
        "hello",
        "--multi-channel-send-json",
    ]);
    assert_eq!(
        send_cli.multi_channel_send,
        Some(CliMultiChannelTransport::Discord)
    );
    assert_eq!(
        send_cli.multi_channel_send_target.as_deref(),
        Some("123456789012345678")
    );
    assert_eq!(send_cli.multi_channel_send_text.as_deref(), Some("hello"));
    assert!(send_cli.multi_channel_send_json);
}

#[test]
fn regression_cli_multi_channel_channel_status_json_requires_status_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-channel-status-json"]);
    let error = parse.expect_err("status json should require status action");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_channel_probe_online_requires_probe_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-channel-probe-online"]);
    let error = parse.expect_err("probe online should require probe action");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_send_target_requires_send_flag() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--multi-channel-send-target", "1234567890123"]);
    let error = parse.expect_err("send target should require send action");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_channel_login_conflicts_with_probe() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-channel-login",
        "discord",
        "--multi-channel-channel-probe",
        "discord",
    ]);
    let error = parse.expect_err("login and probe should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn functional_cli_multi_channel_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-contract-runner",
        "--multi-channel-fixture",
        "fixtures/multi-channel.json",
        "--multi-channel-state-dir",
        ".tau/multi-channel-custom",
        "--multi-channel-queue-limit",
        "128",
        "--multi-channel-processed-event-cap",
        "25000",
        "--multi-channel-retry-max-attempts",
        "7",
        "--multi-channel-retry-base-delay-ms",
        "25",
        "--multi-channel-retry-jitter-ms",
        "9",
        "--multi-channel-telemetry-typing-presence=false",
        "--multi-channel-telemetry-usage-summary=false",
        "--multi-channel-telemetry-include-identifiers=true",
        "--multi-channel-telemetry-min-response-chars",
        "80",
        "--multi-channel-outbound-mode",
        "dry-run",
        "--multi-channel-outbound-max-chars",
        "333",
        "--multi-channel-outbound-http-timeout-ms",
        "8000",
        "--multi-channel-telegram-api-base",
        "https://telegram.internal",
        "--multi-channel-discord-api-base",
        "https://discord.internal/api",
        "--multi-channel-whatsapp-api-base",
        "https://whatsapp.internal",
    ]);
    assert!(cli.multi_channel_contract_runner);
    assert_eq!(
        cli.multi_channel_fixture,
        PathBuf::from("fixtures/multi-channel.json")
    );
    assert_eq!(
        cli.multi_channel_state_dir,
        PathBuf::from(".tau/multi-channel-custom")
    );
    assert_eq!(cli.multi_channel_queue_limit, 128);
    assert_eq!(cli.multi_channel_processed_event_cap, 25_000);
    assert_eq!(cli.multi_channel_retry_max_attempts, 7);
    assert_eq!(cli.multi_channel_retry_base_delay_ms, 25);
    assert_eq!(cli.multi_channel_retry_jitter_ms, 9);
    assert!(!cli.multi_channel_telemetry_typing_presence);
    assert!(!cli.multi_channel_telemetry_usage_summary);
    assert!(cli.multi_channel_telemetry_include_identifiers);
    assert_eq!(cli.multi_channel_telemetry_min_response_chars, 80);
    assert_eq!(
        cli.multi_channel_outbound_mode,
        CliMultiChannelOutboundMode::DryRun
    );
    assert_eq!(cli.multi_channel_outbound_max_chars, 333);
    assert_eq!(cli.multi_channel_outbound_http_timeout_ms, 8000);
    assert_eq!(
        cli.multi_channel_telegram_api_base,
        "https://telegram.internal".to_string()
    );
    assert_eq!(
        cli.multi_channel_discord_api_base,
        "https://discord.internal/api".to_string()
    );
    assert_eq!(
        cli.multi_channel_whatsapp_api_base,
        "https://whatsapp.internal".to_string()
    );
}

#[test]
fn functional_cli_multi_channel_live_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-runner",
        "--multi-channel-live-ingress-dir",
        ".tau/multi-channel/live-inbox",
        "--multi-channel-state-dir",
        ".tau/multi-channel-live",
        "--multi-channel-queue-limit",
        "40",
        "--multi-channel-processed-event-cap",
        "512",
        "--multi-channel-retry-max-attempts",
        "5",
    ]);
    assert!(cli.multi_channel_live_runner);
    assert!(!cli.multi_channel_contract_runner);
    assert_eq!(
        cli.multi_channel_live_ingress_dir,
        PathBuf::from(".tau/multi-channel/live-inbox")
    );
    assert_eq!(
        cli.multi_channel_state_dir,
        PathBuf::from(".tau/multi-channel-live")
    );
    assert_eq!(cli.multi_channel_queue_limit, 40);
    assert_eq!(cli.multi_channel_processed_event_cap, 512);
    assert_eq!(cli.multi_channel_retry_max_attempts, 5);
}

#[test]
fn regression_cli_multi_channel_telemetry_min_response_chars_rejects_zero() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-contract-runner",
        "--multi-channel-telemetry-min-response-chars",
        "0",
    ]);
    let error = parse.expect_err("zero threshold should be rejected");
    assert!(error.to_string().contains("invalid value"));
}

#[test]
fn functional_cli_multi_channel_live_connectors_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-connectors-runner",
        "--multi-channel-live-connectors-poll-once",
        "--multi-channel-live-connectors-state-path",
        ".tau/multi-channel/connectors.json",
        "--multi-channel-live-webhook-bind",
        "0.0.0.0:9797",
        "--multi-channel-telegram-ingress-mode",
        "polling",
        "--multi-channel-discord-ingress-mode",
        "polling",
        "--multi-channel-whatsapp-ingress-mode",
        "webhook",
        "--multi-channel-discord-ingress-channel-id",
        "room-1,room-2",
        "--multi-channel-telegram-webhook-secret",
        "telegram-secret",
        "--multi-channel-whatsapp-webhook-verify-token",
        "wa-verify",
        "--multi-channel-whatsapp-webhook-app-secret",
        "wa-secret",
    ]);
    assert!(cli.multi_channel_live_connectors_runner);
    assert!(cli.multi_channel_live_connectors_poll_once);
    assert_eq!(
        cli.multi_channel_live_connectors_state_path,
        PathBuf::from(".tau/multi-channel/connectors.json")
    );
    assert_eq!(cli.multi_channel_live_webhook_bind, "0.0.0.0:9797");
    assert_eq!(
        cli.multi_channel_telegram_ingress_mode,
        CliMultiChannelLiveConnectorMode::Polling
    );
    assert_eq!(
        cli.multi_channel_discord_ingress_mode,
        CliMultiChannelLiveConnectorMode::Polling
    );
    assert_eq!(
        cli.multi_channel_whatsapp_ingress_mode,
        CliMultiChannelLiveConnectorMode::Webhook
    );
    assert_eq!(
        cli.multi_channel_discord_ingress_channel_ids,
        vec!["room-1".to_string(), "room-2".to_string()]
    );
    assert_eq!(
        cli.multi_channel_telegram_webhook_secret.as_deref(),
        Some("telegram-secret")
    );
    assert_eq!(
        cli.multi_channel_whatsapp_webhook_verify_token.as_deref(),
        Some("wa-verify")
    );
    assert_eq!(
        cli.multi_channel_whatsapp_webhook_app_secret.as_deref(),
        Some("wa-secret")
    );
}

#[test]
fn functional_cli_multi_channel_outbound_provider_secret_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-runner",
        "--multi-channel-live-ingress-dir",
        ".tau/multi-channel/live-ingress",
        "--multi-channel-outbound-mode",
        "provider",
        "--multi-channel-telegram-bot-token",
        "telegram-secret",
        "--multi-channel-discord-bot-token",
        "discord-secret",
        "--multi-channel-whatsapp-access-token",
        "whatsapp-secret",
        "--multi-channel-whatsapp-phone-number-id",
        "15551234567",
    ]);
    assert_eq!(
        cli.multi_channel_outbound_mode,
        CliMultiChannelOutboundMode::Provider
    );
    assert_eq!(
        cli.multi_channel_telegram_bot_token.as_deref(),
        Some("telegram-secret")
    );
    assert_eq!(
        cli.multi_channel_discord_bot_token.as_deref(),
        Some("discord-secret")
    );
    assert_eq!(
        cli.multi_channel_whatsapp_access_token.as_deref(),
        Some("whatsapp-secret")
    );
    assert_eq!(
        cli.multi_channel_whatsapp_phone_number_id.as_deref(),
        Some("15551234567")
    );
}

#[test]
fn functional_cli_multi_channel_live_ingest_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-ingest-file",
        "fixtures/telegram-update.json",
        "--multi-channel-live-ingest-transport",
        "telegram",
        "--multi-channel-live-ingest-provider",
        "telegram-bot-api",
        "--multi-channel-live-ingest-dir",
        ".tau/multi-channel/ingress",
    ]);
    assert_eq!(
        cli.multi_channel_live_ingest_file,
        Some(PathBuf::from("fixtures/telegram-update.json"))
    );
    assert_eq!(
        cli.multi_channel_live_ingest_transport,
        Some(CliMultiChannelTransport::Telegram)
    );
    assert_eq!(
        cli.multi_channel_live_ingest_provider,
        "telegram-bot-api".to_string()
    );
    assert_eq!(
        cli.multi_channel_live_ingest_dir,
        PathBuf::from(".tau/multi-channel/ingress")
    );
}

#[test]
fn regression_cli_multi_channel_live_ingest_transport_requires_ingest_file() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--multi-channel-live-ingest-transport", "discord"]);
    let error = parse.expect_err("transport flag should require ingest-file");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_live_connectors_status_json_requires_status_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-live-connectors-status-json"]);
    let error = parse.expect_err("status json should require connector status action");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn functional_cli_multi_channel_live_readiness_preflight_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-readiness-preflight",
        "--multi-channel-live-readiness-json",
    ]);
    assert!(cli.multi_channel_live_readiness_preflight);
    assert!(cli.multi_channel_live_readiness_json);
    assert!(!cli.multi_channel_contract_runner);
    assert!(!cli.multi_channel_live_runner);
}

#[test]
fn regression_cli_multi_channel_fixture_requires_multi_channel_runner_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-fixture",
        "fixtures/multi-channel.json",
    ]);
    let error = parse.expect_err("fixture flag should require multi-channel runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_live_ingress_dir_requires_live_runner_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-ingress-dir",
        ".tau/multi-channel/live-inbox",
    ]);
    let error = parse.expect_err("live ingress dir should require live runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_live_readiness_json_requires_preflight_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-live-readiness-json"]);
    let error = parse.expect_err("readiness json should require readiness preflight mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_multi_channel_live_readiness_preflight_conflicts_with_live_runner() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-readiness-preflight",
        "--multi-channel-live-runner",
    ]);
    let error = parse.expect_err("readiness preflight should conflict with live runner");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn regression_cli_multi_channel_live_runner_conflicts_with_contract_runner() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-live-runner",
        "--multi-channel-contract-runner",
    ]);
    let error = parse.expect_err("live runner should conflict with contract runner");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn unit_cli_multi_agent_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.multi_agent_contract_runner);
    assert_eq!(
        cli.multi_agent_fixture,
        PathBuf::from("crates/tau-coding-agent/testdata/multi-agent-contract/mixed-outcomes.json")
    );
    assert_eq!(cli.multi_agent_state_dir, PathBuf::from(".tau/multi-agent"));
    assert_eq!(cli.multi_agent_queue_limit, 64);
    assert_eq!(cli.multi_agent_processed_case_cap, 10_000);
    assert_eq!(cli.multi_agent_retry_max_attempts, 4);
    assert_eq!(cli.multi_agent_retry_base_delay_ms, 0);
}

#[test]
fn functional_cli_multi_agent_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-agent-contract-runner",
        "--multi-agent-fixture",
        "fixtures/multi-agent.json",
        "--multi-agent-state-dir",
        ".tau/multi-agent-custom",
        "--multi-agent-queue-limit",
        "90",
        "--multi-agent-processed-case-cap",
        "12000",
        "--multi-agent-retry-max-attempts",
        "6",
        "--multi-agent-retry-base-delay-ms",
        "30",
    ]);
    assert!(cli.multi_agent_contract_runner);
    assert_eq!(
        cli.multi_agent_fixture,
        PathBuf::from("fixtures/multi-agent.json")
    );
    assert_eq!(
        cli.multi_agent_state_dir,
        PathBuf::from(".tau/multi-agent-custom")
    );
    assert_eq!(cli.multi_agent_queue_limit, 90);
    assert_eq!(cli.multi_agent_processed_case_cap, 12_000);
    assert_eq!(cli.multi_agent_retry_max_attempts, 6);
    assert_eq!(cli.multi_agent_retry_base_delay_ms, 30);
}

#[test]
fn regression_cli_multi_agent_fixture_requires_multi_agent_runner_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--multi-agent-fixture",
        "fixtures/multi-agent.json",
    ]);
    let error = parse.expect_err("fixture flag should require multi-agent runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_memory_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.memory_contract_runner);
    assert_eq!(
        cli.memory_fixture,
        PathBuf::from("crates/tau-coding-agent/testdata/memory-contract/mixed-outcomes.json")
    );
    assert_eq!(cli.memory_state_dir, PathBuf::from(".tau/memory"));
    assert_eq!(cli.memory_queue_limit, 64);
    assert_eq!(cli.memory_processed_case_cap, 10_000);
    assert_eq!(cli.memory_retry_max_attempts, 4);
    assert_eq!(cli.memory_retry_base_delay_ms, 0);
}

#[test]
fn functional_cli_memory_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--memory-contract-runner",
        "--memory-fixture",
        "fixtures/memory.json",
        "--memory-state-dir",
        ".tau/memory-custom",
        "--memory-queue-limit",
        "80",
        "--memory-processed-case-cap",
        "9000",
        "--memory-retry-max-attempts",
        "6",
        "--memory-retry-base-delay-ms",
        "15",
    ]);
    assert!(cli.memory_contract_runner);
    assert_eq!(cli.memory_fixture, PathBuf::from("fixtures/memory.json"));
    assert_eq!(cli.memory_state_dir, PathBuf::from(".tau/memory-custom"));
    assert_eq!(cli.memory_queue_limit, 80);
    assert_eq!(cli.memory_processed_case_cap, 9_000);
    assert_eq!(cli.memory_retry_max_attempts, 6);
    assert_eq!(cli.memory_retry_base_delay_ms, 15);
}

#[test]
fn regression_cli_memory_fixture_requires_memory_runner_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--memory-fixture", "fixtures/memory.json"]);
    let error = parse.expect_err("fixture flag should require memory runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_dashboard_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.dashboard_contract_runner);
    assert_eq!(
        cli.dashboard_fixture,
        PathBuf::from("crates/tau-coding-agent/testdata/dashboard-contract/mixed-outcomes.json")
    );
    assert_eq!(cli.dashboard_state_dir, PathBuf::from(".tau/dashboard"));
    assert_eq!(cli.dashboard_queue_limit, 64);
    assert_eq!(cli.dashboard_processed_case_cap, 10_000);
    assert_eq!(cli.dashboard_retry_max_attempts, 4);
    assert_eq!(cli.dashboard_retry_base_delay_ms, 0);
}

#[test]
fn functional_cli_dashboard_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--dashboard-contract-runner",
        "--dashboard-fixture",
        "fixtures/dashboard.json",
        "--dashboard-state-dir",
        ".tau/dashboard-custom",
        "--dashboard-queue-limit",
        "120",
        "--dashboard-processed-case-cap",
        "12000",
        "--dashboard-retry-max-attempts",
        "5",
        "--dashboard-retry-base-delay-ms",
        "20",
    ]);
    assert!(cli.dashboard_contract_runner);
    assert_eq!(
        cli.dashboard_fixture,
        PathBuf::from("fixtures/dashboard.json")
    );
    assert_eq!(
        cli.dashboard_state_dir,
        PathBuf::from(".tau/dashboard-custom")
    );
    assert_eq!(cli.dashboard_queue_limit, 120);
    assert_eq!(cli.dashboard_processed_case_cap, 12_000);
    assert_eq!(cli.dashboard_retry_max_attempts, 5);
    assert_eq!(cli.dashboard_retry_base_delay_ms, 20);
}

#[test]
fn regression_cli_dashboard_fixture_requires_dashboard_runner_flag() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--dashboard-fixture", "fixtures/dashboard.json"]);
    let error = parse.expect_err("fixture flag should require dashboard runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_gateway_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.gateway_openresponses_server);
    assert_eq!(cli.gateway_openresponses_bind, "127.0.0.1:8787");
    assert_eq!(
        cli.gateway_openresponses_auth_mode,
        CliGatewayOpenResponsesAuthMode::Token
    );
    assert!(cli.gateway_openresponses_auth_token.is_none());
    assert!(cli.gateway_openresponses_auth_password.is_none());
    assert_eq!(cli.gateway_openresponses_session_ttl_seconds, 3_600);
    assert_eq!(cli.gateway_openresponses_rate_limit_window_seconds, 60);
    assert_eq!(cli.gateway_openresponses_rate_limit_max_requests, 120);
    assert_eq!(cli.gateway_openresponses_max_input_chars, 32_000);
    assert!(!cli.gateway_contract_runner);
    assert_eq!(
        cli.gateway_fixture,
        PathBuf::from("crates/tau-gateway/testdata/gateway-contract/mixed-outcomes.json")
    );
    assert_eq!(cli.gateway_state_dir, PathBuf::from(".tau/gateway"));
    assert_eq!(cli.gateway_guardrail_failure_streak_threshold, 2);
    assert_eq!(cli.gateway_guardrail_retryable_failures_threshold, 2);
}

#[test]
fn functional_cli_gateway_openresponses_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-openresponses-server",
        "--gateway-openresponses-bind",
        "127.0.0.1:8899",
        "--gateway-openresponses-auth-mode",
        "password-session",
        "--gateway-openresponses-auth-password",
        "secret-password",
        "--gateway-openresponses-session-ttl-seconds",
        "1800",
        "--gateway-openresponses-rate-limit-window-seconds",
        "30",
        "--gateway-openresponses-rate-limit-max-requests",
        "40",
        "--gateway-openresponses-max-input-chars",
        "24000",
    ]);
    assert!(cli.gateway_openresponses_server);
    assert_eq!(cli.gateway_openresponses_bind, "127.0.0.1:8899");
    assert_eq!(
        cli.gateway_openresponses_auth_mode,
        CliGatewayOpenResponsesAuthMode::PasswordSession
    );
    assert!(cli.gateway_openresponses_auth_token.is_none());
    assert_eq!(
        cli.gateway_openresponses_auth_password.as_deref(),
        Some("secret-password")
    );
    assert_eq!(cli.gateway_openresponses_session_ttl_seconds, 1_800);
    assert_eq!(cli.gateway_openresponses_rate_limit_window_seconds, 30);
    assert_eq!(cli.gateway_openresponses_rate_limit_max_requests, 40);
    assert_eq!(cli.gateway_openresponses_max_input_chars, 24_000);
}

#[test]
fn regression_cli_gateway_openresponses_bind_requires_server_flag() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--gateway-openresponses-bind", "127.0.0.1:8899"]);
    let error = parse.expect_err("bind override should require gateway openresponses server mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_gateway_openresponses_max_input_chars_requires_server_flag() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--gateway-openresponses-max-input-chars", "24000"]);
    let error =
        parse.expect_err("max input override should require gateway openresponses server mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn functional_cli_gateway_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-contract-runner",
        "--gateway-fixture",
        "fixtures/gateway.json",
        "--gateway-state-dir",
        ".tau/gateway-custom",
        "--gateway-guardrail-failure-streak-threshold",
        "4",
        "--gateway-guardrail-retryable-failures-threshold",
        "5",
    ]);
    assert!(cli.gateway_contract_runner);
    assert_eq!(cli.gateway_fixture, PathBuf::from("fixtures/gateway.json"));
    assert_eq!(cli.gateway_state_dir, PathBuf::from(".tau/gateway-custom"));
    assert_eq!(cli.gateway_guardrail_failure_streak_threshold, 4);
    assert_eq!(cli.gateway_guardrail_retryable_failures_threshold, 5);
}

#[test]
fn regression_cli_gateway_fixture_requires_gateway_runner_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--gateway-fixture", "fixtures/gateway.json"]);
    let error = parse.expect_err("fixture flag should require gateway runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_gateway_guardrail_flags_require_gateway_runner_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--gateway-guardrail-failure-streak-threshold",
        "3",
    ]);
    let error = parse.expect_err("guardrail failure threshold should require gateway runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_browser_automation_live_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.browser_automation_contract_runner);
    assert!(!cli.browser_automation_live_runner);
    assert_eq!(
        cli.browser_automation_live_fixture,
        PathBuf::from(
            "crates/tau-coding-agent/testdata/browser-automation-live/live-sequence.json"
        )
    );
    assert_eq!(cli.browser_automation_playwright_cli, "playwright-cli");
}

#[test]
fn functional_cli_browser_automation_live_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--browser-automation-live-runner",
        "--browser-automation-live-fixture",
        "fixtures/browser-live.json",
        "--browser-automation-playwright-cli",
        "./bin/mock-playwright-cli",
    ]);
    assert!(cli.browser_automation_live_runner);
    assert_eq!(
        cli.browser_automation_live_fixture,
        PathBuf::from("fixtures/browser-live.json")
    );
    assert_eq!(
        cli.browser_automation_playwright_cli,
        "./bin/mock-playwright-cli"
    );
}

#[test]
fn regression_cli_browser_automation_live_fixture_requires_live_runner_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--browser-automation-live-fixture",
        "fixtures/browser-live.json",
    ]);
    let error = parse.expect_err("fixture flag should require live runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_browser_automation_live_runner_conflicts_with_contract_runner() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--browser-automation-live-runner",
        "--browser-automation-contract-runner",
    ]);
    let error = parse.expect_err("live runner should conflict with contract runner");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn unit_cli_deployment_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.deployment_contract_runner);
    assert!(cli.deployment_wasm_package_module.is_none());
    assert_eq!(
        cli.deployment_wasm_package_blueprint_id,
        "edge-wasm".to_string()
    );
    assert_eq!(
        cli.deployment_wasm_package_runtime_profile,
        CliDeploymentWasmRuntimeProfile::WasmWasi
    );
    assert_eq!(
        cli.deployment_wasm_package_output_dir,
        PathBuf::from(".tau/deployment/wasm-artifacts")
    );
    assert!(!cli.deployment_wasm_package_json);
    assert!(cli.deployment_wasm_inspect_manifest.is_none());
    assert!(!cli.deployment_wasm_inspect_json);
    assert_eq!(
        cli.deployment_fixture,
        PathBuf::from("crates/tau-coding-agent/testdata/deployment-contract/mixed-outcomes.json")
    );
    assert_eq!(cli.deployment_state_dir, PathBuf::from(".tau/deployment"));
    assert_eq!(cli.deployment_queue_limit, 64);
    assert_eq!(cli.deployment_processed_case_cap, 10_000);
    assert_eq!(cli.deployment_retry_max_attempts, 4);
    assert_eq!(cli.deployment_retry_base_delay_ms, 0);
}

#[test]
fn functional_cli_deployment_wasm_inspect_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--deployment-wasm-inspect-manifest",
        "fixtures/edge.manifest.json",
        "--deployment-wasm-inspect-json",
    ]);
    assert_eq!(
        cli.deployment_wasm_inspect_manifest,
        Some(PathBuf::from("fixtures/edge.manifest.json"))
    );
    assert!(cli.deployment_wasm_inspect_json);
}

#[test]
fn regression_cli_deployment_wasm_inspect_json_requires_manifest_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--deployment-wasm-inspect-json"]);
    let error = parse.expect_err("inspect json should require inspect manifest");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn functional_cli_deployment_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--deployment-contract-runner",
        "--deployment-fixture",
        "fixtures/deployment.json",
        "--deployment-state-dir",
        ".tau/deployment-custom",
        "--deployment-queue-limit",
        "81",
        "--deployment-processed-case-cap",
        "9100",
        "--deployment-retry-max-attempts",
        "7",
        "--deployment-retry-base-delay-ms",
        "25",
    ]);
    assert!(cli.deployment_contract_runner);
    assert_eq!(
        cli.deployment_fixture,
        PathBuf::from("fixtures/deployment.json")
    );
    assert_eq!(
        cli.deployment_state_dir,
        PathBuf::from(".tau/deployment-custom")
    );
    assert_eq!(cli.deployment_queue_limit, 81);
    assert_eq!(cli.deployment_processed_case_cap, 9_100);
    assert_eq!(cli.deployment_retry_max_attempts, 7);
    assert_eq!(cli.deployment_retry_base_delay_ms, 25);
}

#[test]
fn regression_cli_deployment_fixture_requires_runner_flag() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--deployment-fixture", "fixtures/deployment.json"]);
    let error = parse.expect_err("fixture flag should require deployment runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_custom_command_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.custom_command_contract_runner);
    assert_eq!(
        cli.custom_command_fixture,
        PathBuf::from(
            "crates/tau-coding-agent/testdata/custom-command-contract/mixed-outcomes.json"
        )
    );
    assert_eq!(
        cli.custom_command_state_dir,
        PathBuf::from(".tau/custom-command")
    );
    assert_eq!(cli.custom_command_queue_limit, 64);
    assert_eq!(cli.custom_command_processed_case_cap, 10_000);
    assert_eq!(cli.custom_command_retry_max_attempts, 4);
    assert_eq!(cli.custom_command_retry_base_delay_ms, 0);
    assert!(cli.custom_command_policy_require_approval);
    assert!(!cli.custom_command_policy_allow_shell);
    assert_eq!(cli.custom_command_policy_sandbox_profile, "restricted");
    assert!(cli.custom_command_policy_allowed_env.is_empty());
    assert!(cli.custom_command_policy_denied_env.is_empty());
}

#[test]
fn functional_cli_custom_command_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--custom-command-contract-runner",
        "--custom-command-fixture",
        "fixtures/custom-command.json",
        "--custom-command-state-dir",
        ".tau/custom-command-custom",
        "--custom-command-queue-limit",
        "73",
        "--custom-command-processed-case-cap",
        "42000",
        "--custom-command-retry-max-attempts",
        "8",
        "--custom-command-retry-base-delay-ms",
        "35",
        "--custom-command-policy-require-approval=false",
        "--custom-command-policy-allow-shell=true",
        "--custom-command-policy-sandbox-profile",
        "workspace_write",
        "--custom-command-policy-allowed-env",
        "DEPLOY_ENV,REGION",
        "--custom-command-policy-denied-env",
        "OPENAI_API_KEY,ANTHROPIC_API_KEY",
    ]);
    assert!(cli.custom_command_contract_runner);
    assert_eq!(
        cli.custom_command_fixture,
        PathBuf::from("fixtures/custom-command.json")
    );
    assert_eq!(
        cli.custom_command_state_dir,
        PathBuf::from(".tau/custom-command-custom")
    );
    assert_eq!(cli.custom_command_queue_limit, 73);
    assert_eq!(cli.custom_command_processed_case_cap, 42_000);
    assert_eq!(cli.custom_command_retry_max_attempts, 8);
    assert_eq!(cli.custom_command_retry_base_delay_ms, 35);
    assert!(!cli.custom_command_policy_require_approval);
    assert!(cli.custom_command_policy_allow_shell);
    assert_eq!(cli.custom_command_policy_sandbox_profile, "workspace_write");
    assert_eq!(
        cli.custom_command_policy_allowed_env,
        vec!["DEPLOY_ENV".to_string(), "REGION".to_string()]
    );
    assert_eq!(
        cli.custom_command_policy_denied_env,
        vec![
            "OPENAI_API_KEY".to_string(),
            "ANTHROPIC_API_KEY".to_string()
        ]
    );
}

#[test]
fn regression_cli_custom_command_fixture_requires_runner_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--custom-command-fixture",
        "fixtures/custom-command.json",
    ]);
    let error = parse.expect_err("fixture flag should require custom command runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_voice_runner_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.voice_contract_runner);
    assert!(!cli.voice_live_runner);
    assert_eq!(
        cli.voice_fixture,
        PathBuf::from("crates/tau-coding-agent/testdata/voice-contract/mixed-outcomes.json")
    );
    assert_eq!(
        cli.voice_live_input,
        PathBuf::from("crates/tau-coding-agent/testdata/voice-live/single-turn.json")
    );
    assert_eq!(cli.voice_state_dir, PathBuf::from(".tau/voice"));
    assert_eq!(cli.voice_queue_limit, 64);
    assert_eq!(cli.voice_processed_case_cap, 10_000);
    assert_eq!(cli.voice_retry_max_attempts, 4);
    assert_eq!(cli.voice_retry_base_delay_ms, 0);
    assert_eq!(cli.voice_live_wake_word, "tau");
    assert_eq!(cli.voice_live_max_turns, 64);
    assert!(cli.voice_live_tts_output);
}

#[test]
fn functional_cli_voice_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--voice-contract-runner",
        "--voice-fixture",
        "fixtures/voice.json",
        "--voice-state-dir",
        ".tau/voice-custom",
        "--voice-queue-limit",
        "71",
        "--voice-processed-case-cap",
        "41000",
        "--voice-retry-max-attempts",
        "6",
        "--voice-retry-base-delay-ms",
        "45",
    ]);
    assert!(cli.voice_contract_runner);
    assert_eq!(cli.voice_fixture, PathBuf::from("fixtures/voice.json"));
    assert_eq!(cli.voice_state_dir, PathBuf::from(".tau/voice-custom"));
    assert_eq!(cli.voice_queue_limit, 71);
    assert_eq!(cli.voice_processed_case_cap, 41_000);
    assert_eq!(cli.voice_retry_max_attempts, 6);
    assert_eq!(cli.voice_retry_base_delay_ms, 45);
}

#[test]
fn regression_cli_voice_fixture_requires_runner_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--voice-fixture", "fixtures/voice.json"]);
    let error = parse.expect_err("fixture flag should require voice runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn functional_cli_voice_live_runner_flags_accept_explicit_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--voice-live-runner",
        "--voice-live-input",
        "fixtures/voice-live.json",
        "--voice-live-wake-word",
        "hello",
        "--voice-live-max-turns",
        "7",
        "--voice-live-tts-output=false",
        "--voice-state-dir",
        ".tau/voice-live-custom",
    ]);
    assert!(cli.voice_live_runner);
    assert_eq!(
        cli.voice_live_input,
        PathBuf::from("fixtures/voice-live.json")
    );
    assert_eq!(cli.voice_live_wake_word, "hello");
    assert_eq!(cli.voice_live_max_turns, 7);
    assert!(!cli.voice_live_tts_output);
    assert_eq!(cli.voice_state_dir, PathBuf::from(".tau/voice-live-custom"));
}

#[test]
fn regression_cli_voice_live_input_requires_runner_flag() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--voice-live-input", "fixtures/voice-live.json"]);
    let error = parse.expect_err("live input should require live runner mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_transport_health_inspect_accepts_multi_channel_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "multi-channel"]);
    assert_eq!(
        cli.transport_health_inspect.as_deref(),
        Some("multi-channel")
    );
}

#[test]
fn functional_cli_transport_health_inspect_accepts_multi_channel_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "multi-channel",
        "--multi-channel-state-dir",
        ".tau/multi-channel-alt",
    ]);
    assert_eq!(
        cli.multi_channel_state_dir,
        PathBuf::from(".tau/multi-channel-alt")
    );
}

#[test]
fn unit_cli_transport_health_inspect_accepts_multi_agent_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "multi-agent"]);
    assert_eq!(cli.transport_health_inspect.as_deref(), Some("multi-agent"));
}

#[test]
fn functional_cli_transport_health_inspect_accepts_multi_agent_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "multi-agent",
        "--multi-agent-state-dir",
        ".tau/multi-agent-alt",
    ]);
    assert_eq!(
        cli.multi_agent_state_dir,
        PathBuf::from(".tau/multi-agent-alt")
    );
}

#[test]
fn unit_cli_transport_health_inspect_accepts_memory_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "memory"]);
    assert_eq!(cli.transport_health_inspect.as_deref(), Some("memory"));
}

#[test]
fn functional_cli_transport_health_inspect_accepts_memory_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "memory",
        "--memory-state-dir",
        ".tau/memory-alt",
    ]);
    assert_eq!(cli.memory_state_dir, PathBuf::from(".tau/memory-alt"));
}

#[test]
fn unit_cli_transport_health_inspect_accepts_dashboard_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "dashboard"]);
    assert_eq!(cli.transport_health_inspect.as_deref(), Some("dashboard"));
}

#[test]
fn functional_cli_transport_health_inspect_accepts_dashboard_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "dashboard",
        "--dashboard-state-dir",
        ".tau/dashboard-alt",
    ]);
    assert_eq!(cli.dashboard_state_dir, PathBuf::from(".tau/dashboard-alt"));
}

#[test]
fn unit_cli_transport_health_inspect_accepts_gateway_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "gateway"]);
    assert_eq!(cli.transport_health_inspect.as_deref(), Some("gateway"));
}

#[test]
fn functional_cli_transport_health_inspect_accepts_gateway_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "gateway",
        "--gateway-state-dir",
        ".tau/gateway-alt",
    ]);
    assert_eq!(cli.gateway_state_dir, PathBuf::from(".tau/gateway-alt"));
}

#[test]
fn unit_cli_transport_health_inspect_accepts_deployment_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "deployment"]);
    assert_eq!(cli.transport_health_inspect.as_deref(), Some("deployment"));
}

#[test]
fn functional_cli_transport_health_inspect_accepts_deployment_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "deployment",
        "--deployment-state-dir",
        ".tau/deployment-alt",
    ]);
    assert_eq!(
        cli.deployment_state_dir,
        PathBuf::from(".tau/deployment-alt")
    );
}

#[test]
fn unit_cli_transport_health_inspect_accepts_custom_command_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "custom-command"]);
    assert_eq!(
        cli.transport_health_inspect.as_deref(),
        Some("custom-command")
    );
}

#[test]
fn functional_cli_transport_health_inspect_accepts_custom_command_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "custom-command",
        "--custom-command-state-dir",
        ".tau/custom-command-alt",
    ]);
    assert_eq!(
        cli.custom_command_state_dir,
        PathBuf::from(".tau/custom-command-alt")
    );
}

#[test]
fn unit_cli_transport_health_inspect_accepts_voice_target() {
    let cli = parse_cli_with_stack(["tau-rs", "--transport-health-inspect", "voice"]);
    assert_eq!(cli.transport_health_inspect.as_deref(), Some("voice"));
}

#[test]
fn functional_cli_transport_health_inspect_accepts_voice_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--transport-health-inspect",
        "voice",
        "--voice-state-dir",
        ".tau/voice-alt",
    ]);
    assert_eq!(cli.voice_state_dir, PathBuf::from(".tau/voice-alt"));
}

#[test]
fn unit_cli_project_index_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.project_index_build);
    assert!(cli.project_index_query.is_none());
    assert!(!cli.project_index_inspect);
    assert!(!cli.project_index_json);
    assert_eq!(cli.project_index_root, PathBuf::from("."));
    assert_eq!(cli.project_index_state_dir, PathBuf::from(".tau/index"));
    assert_eq!(cli.project_index_limit, 25);
}

#[test]
fn functional_cli_project_index_query_accepts_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--project-index-query",
        "router state",
        "--project-index-json",
        "--project-index-root",
        "workspace",
        "--project-index-state-dir",
        ".tau/index-custom",
        "--project-index-limit",
        "9",
    ]);
    assert_eq!(cli.project_index_query.as_deref(), Some("router state"));
    assert!(cli.project_index_json);
    assert_eq!(cli.project_index_root, PathBuf::from("workspace"));
    assert_eq!(
        cli.project_index_state_dir,
        PathBuf::from(".tau/index-custom")
    );
    assert_eq!(cli.project_index_limit, 9);
}

#[test]
fn regression_validate_project_index_cli_json_requires_action() {
    let cli = parse_cli_with_stack(["tau-rs", "--project-index-json"]);
    let error = validate_project_index_cli(&cli).expect_err("json requires index mode");
    assert!(error
        .to_string()
        .contains("--project-index-json requires one of"));
}

#[test]
fn unit_cli_github_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.github_status_inspect.is_none());
    assert!(!cli.github_status_json);
}

#[test]
fn functional_cli_github_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--github-status-inspect",
        "owner/repo",
        "--github-status-json",
        "--github-state-dir",
        ".tau/github-observe",
    ]);
    assert_eq!(cli.github_status_inspect.as_deref(), Some("owner/repo"));
    assert!(cli.github_status_json);
    assert_eq!(cli.github_state_dir, PathBuf::from(".tau/github-observe"));
}

#[test]
fn regression_cli_github_status_json_requires_github_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--github-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_operator_control_summary_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.operator_control_summary);
    assert!(!cli.operator_control_summary_json);
}

#[test]
fn functional_cli_operator_control_summary_accepts_json_mode() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--operator-control-summary",
        "--operator-control-summary-json",
    ]);
    assert!(cli.operator_control_summary);
    assert!(cli.operator_control_summary_json);
}

#[test]
fn functional_cli_operator_control_summary_accepts_snapshot_and_compare_paths() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--operator-control-summary",
        "--operator-control-summary-compare",
        ".tau/operator-control-baseline.json",
        "--operator-control-summary-snapshot-out",
        ".tau/operator-control-current.json",
    ]);
    assert!(cli.operator_control_summary);
    assert_eq!(
        cli.operator_control_summary_compare,
        Some(PathBuf::from(".tau/operator-control-baseline.json"))
    );
    assert_eq!(
        cli.operator_control_summary_snapshot_out,
        Some(PathBuf::from(".tau/operator-control-current.json"))
    );
}

#[test]
fn regression_cli_operator_control_summary_json_requires_summary_mode() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--operator-control-summary-json"]);
    let error = parse.expect_err("json output should require summary flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_operator_control_summary_compare_requires_summary_mode() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--operator-control-summary-compare",
        ".tau/operator-control-baseline.json",
    ]);
    let error = parse.expect_err("compare path should require summary flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_operator_control_summary_snapshot_out_requires_summary_mode() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--operator-control-summary-snapshot-out",
        ".tau/operator-control-current.json",
    ]);
    let error = parse.expect_err("snapshot path should require summary flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_dashboard_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.dashboard_status_inspect);
    assert!(!cli.dashboard_status_json);
}

#[test]
fn functional_cli_dashboard_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--dashboard-status-inspect",
        "--dashboard-status-json",
        "--dashboard-state-dir",
        ".tau/dashboard-observe",
    ]);
    assert!(cli.dashboard_status_inspect);
    assert!(cli.dashboard_status_json);
    assert_eq!(
        cli.dashboard_state_dir,
        PathBuf::from(".tau/dashboard-observe")
    );
}

#[test]
fn regression_cli_dashboard_status_json_requires_dashboard_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--dashboard-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_multi_channel_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.multi_channel_status_inspect);
    assert!(!cli.multi_channel_status_json);
}

#[test]
fn functional_cli_multi_channel_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-status-inspect",
        "--multi-channel-status-json",
        "--multi-channel-state-dir",
        ".tau/multi-channel-observe",
    ]);
    assert!(cli.multi_channel_status_inspect);
    assert!(cli.multi_channel_status_json);
    assert_eq!(
        cli.multi_channel_state_dir,
        PathBuf::from(".tau/multi-channel-observe")
    );
}

#[test]
fn regression_cli_multi_channel_status_json_requires_multi_channel_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_multi_channel_route_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.multi_channel_route_inspect_file.is_none());
    assert!(!cli.multi_channel_route_inspect_json);
}

#[test]
fn functional_cli_multi_channel_route_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-route-inspect-file",
        "fixtures/event.json",
        "--multi-channel-route-inspect-json",
        "--multi-channel-state-dir",
        ".tau/multi-channel-observe",
    ]);
    assert_eq!(
        cli.multi_channel_route_inspect_file.as_deref(),
        Some(Path::new("fixtures/event.json"))
    );
    assert!(cli.multi_channel_route_inspect_json);
    assert_eq!(
        cli.multi_channel_state_dir,
        PathBuf::from(".tau/multi-channel-observe")
    );
}

#[test]
fn regression_cli_multi_channel_route_inspect_json_requires_file_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-route-inspect-json"]);
    let error = parse.expect_err("json output should require route inspect file flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_multi_channel_incident_timeline_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.multi_channel_incident_timeline);
    assert!(!cli.multi_channel_incident_timeline_json);
    assert!(cli.multi_channel_incident_start_unix_ms.is_none());
    assert!(cli.multi_channel_incident_end_unix_ms.is_none());
    assert!(cli.multi_channel_incident_event_limit.is_none());
    assert!(cli.multi_channel_incident_replay_export.is_none());
}

#[test]
fn functional_cli_multi_channel_incident_timeline_accepts_filters_and_export_path() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-channel-incident-timeline",
        "--multi-channel-incident-timeline-json",
        "--multi-channel-incident-start-unix-ms",
        "1760200000000",
        "--multi-channel-incident-end-unix-ms",
        "1760200009999",
        "--multi-channel-incident-event-limit",
        "25",
        "--multi-channel-incident-replay-export",
        ".tau/incident-replay.json",
    ]);
    assert!(cli.multi_channel_incident_timeline);
    assert!(cli.multi_channel_incident_timeline_json);
    assert_eq!(
        cli.multi_channel_incident_start_unix_ms,
        Some(1_760_200_000_000)
    );
    assert_eq!(
        cli.multi_channel_incident_end_unix_ms,
        Some(1_760_200_009_999)
    );
    assert_eq!(cli.multi_channel_incident_event_limit, Some(25));
    assert_eq!(
        cli.multi_channel_incident_replay_export,
        Some(PathBuf::from(".tau/incident-replay.json"))
    );
}

#[test]
fn regression_cli_multi_channel_incident_timeline_json_requires_timeline_mode() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-channel-incident-timeline-json"]);
    let error = parse.expect_err("json output should require incident timeline mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_multi_agent_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.multi_agent_status_inspect);
    assert!(!cli.multi_agent_status_json);
}

#[test]
fn functional_cli_multi_agent_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--multi-agent-status-inspect",
        "--multi-agent-status-json",
        "--multi-agent-state-dir",
        ".tau/multi-agent-observe",
    ]);
    assert!(cli.multi_agent_status_inspect);
    assert!(cli.multi_agent_status_json);
    assert_eq!(
        cli.multi_agent_state_dir,
        PathBuf::from(".tau/multi-agent-observe")
    );
}

#[test]
fn regression_cli_multi_agent_status_json_requires_multi_agent_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--multi-agent-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_gateway_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.gateway_status_inspect);
    assert!(!cli.gateway_status_json);
}

#[test]
fn functional_cli_gateway_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-status-inspect",
        "--gateway-status-json",
        "--gateway-state-dir",
        ".tau/gateway-observe",
    ]);
    assert!(cli.gateway_status_inspect);
    assert!(cli.gateway_status_json);
    assert_eq!(cli.gateway_state_dir, PathBuf::from(".tau/gateway-observe"));
}

#[test]
fn regression_cli_gateway_status_json_requires_gateway_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--gateway-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_gateway_remote_profile_flags_default_to_local_only() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.gateway_remote_profile_inspect);
    assert!(!cli.gateway_remote_profile_json);
    assert_eq!(
        cli.gateway_remote_profile,
        CliGatewayRemoteProfile::LocalOnly
    );
}

#[test]
fn functional_cli_gateway_remote_profile_inspect_accepts_json_and_profile_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-remote-profile-inspect",
        "--gateway-remote-profile-json",
        "--gateway-openresponses-server",
        "--gateway-remote-profile",
        "proxy-remote",
        "--gateway-openresponses-auth-mode",
        "token",
        "--gateway-openresponses-auth-token",
        "edge-token",
        "--gateway-openresponses-bind",
        "127.0.0.1:8787",
    ]);
    assert!(cli.gateway_remote_profile_inspect);
    assert!(cli.gateway_remote_profile_json);
    assert_eq!(
        cli.gateway_remote_profile,
        CliGatewayRemoteProfile::ProxyRemote
    );
    assert!(cli.gateway_openresponses_server);
}

#[test]
fn functional_cli_gateway_remote_profile_accepts_tailscale_funnel_profile() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-remote-profile-inspect",
        "--gateway-openresponses-server",
        "--gateway-remote-profile",
        "tailscale-funnel",
        "--gateway-openresponses-auth-mode",
        "password-session",
        "--gateway-openresponses-auth-password",
        "edge-password",
    ]);
    assert_eq!(
        cli.gateway_remote_profile,
        CliGatewayRemoteProfile::TailscaleFunnel
    );
}

#[test]
fn regression_cli_gateway_remote_profile_json_requires_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--gateway-remote-profile-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_gateway_remote_plan_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.gateway_remote_plan);
    assert!(!cli.gateway_remote_plan_json);
}

#[test]
fn functional_cli_gateway_remote_plan_accepts_json_and_profile_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-remote-plan",
        "--gateway-remote-plan-json",
        "--gateway-remote-profile",
        "tailscale-serve",
    ]);
    assert!(cli.gateway_remote_plan);
    assert!(cli.gateway_remote_plan_json);
    assert_eq!(
        cli.gateway_remote_profile,
        CliGatewayRemoteProfile::TailscaleServe
    );
}

#[test]
fn regression_cli_gateway_remote_plan_json_requires_plan() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--gateway-remote-plan-json"]);
    let error = parse.expect_err("json output should require plan flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_gateway_service_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.gateway_service_start);
    assert!(!cli.gateway_service_stop);
    assert!(cli.gateway_service_stop_reason.is_none());
    assert!(!cli.gateway_service_status);
    assert!(!cli.gateway_service_status_json);
}

#[test]
fn functional_cli_gateway_service_stop_accepts_reason_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-service-stop",
        "--gateway-service-stop-reason",
        "operator_planned_maintenance",
        "--gateway-state-dir",
        ".tau/gateway-service",
    ]);
    assert!(cli.gateway_service_stop);
    assert_eq!(
        cli.gateway_service_stop_reason.as_deref(),
        Some("operator_planned_maintenance")
    );
    assert_eq!(cli.gateway_state_dir, PathBuf::from(".tau/gateway-service"));
}

#[test]
fn functional_cli_gateway_service_status_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--gateway-service-status",
        "--gateway-service-status-json",
        "--gateway-state-dir",
        ".tau/gateway-service",
    ]);
    assert!(cli.gateway_service_status);
    assert!(cli.gateway_service_status_json);
    assert_eq!(cli.gateway_state_dir, PathBuf::from(".tau/gateway-service"));
}

#[test]
fn regression_cli_gateway_service_status_json_requires_gateway_service_status() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--gateway-service-status-json"]);
    let error = parse.expect_err("json output should require gateway service status flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_gateway_service_stop_reason_requires_gateway_service_stop() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--gateway-service-stop-reason",
        "operator_planned_maintenance",
    ]);
    let error = parse.expect_err("stop reason should require gateway service stop flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_gateway_service_start_conflicts_with_gateway_service_stop() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--gateway-service-start",
        "--gateway-service-stop",
    ]);
    let error = parse.expect_err("start and stop should conflict");
    assert!(error.to_string().contains("cannot be used with"));
}

#[test]
fn unit_cli_daemon_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.daemon_install);
    assert!(!cli.daemon_uninstall);
    assert!(!cli.daemon_start);
    assert!(!cli.daemon_stop);
    assert!(cli.daemon_stop_reason.is_none());
    assert!(!cli.daemon_status);
    assert!(!cli.daemon_status_json);
    assert_eq!(cli.daemon_profile, CliDaemonProfile::Auto);
    assert_eq!(cli.daemon_state_dir, PathBuf::from(".tau/daemon"));
}

#[test]
fn functional_cli_daemon_stop_accepts_reason_profile_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--daemon-stop",
        "--daemon-stop-reason",
        "operator_maintenance",
        "--daemon-profile",
        "systemd-user",
        "--daemon-state-dir",
        ".tau/daemon-service",
    ]);
    assert!(cli.daemon_stop);
    assert_eq!(
        cli.daemon_stop_reason.as_deref(),
        Some("operator_maintenance")
    );
    assert_eq!(cli.daemon_profile, CliDaemonProfile::SystemdUser);
    assert_eq!(cli.daemon_state_dir, PathBuf::from(".tau/daemon-service"));
}

#[test]
fn functional_cli_daemon_status_accepts_json_output() {
    let cli = parse_cli_with_stack(["tau-rs", "--daemon-status", "--daemon-status-json"]);
    assert!(cli.daemon_status);
    assert!(cli.daemon_status_json);
}

#[test]
fn regression_cli_daemon_status_json_requires_daemon_status() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--daemon-status-json"]);
    let error = parse.expect_err("daemon status json should require daemon status flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn regression_cli_daemon_stop_reason_requires_daemon_stop() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--daemon-stop-reason", "ops-window"]);
    let error = parse.expect_err("daemon stop reason should require daemon stop flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_deployment_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.deployment_status_inspect);
    assert!(!cli.deployment_status_json);
}

#[test]
fn functional_cli_deployment_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--deployment-status-inspect",
        "--deployment-status-json",
        "--deployment-state-dir",
        ".tau/deployment-observe",
    ]);
    assert!(cli.deployment_status_inspect);
    assert!(cli.deployment_status_json);
    assert_eq!(
        cli.deployment_state_dir,
        PathBuf::from(".tau/deployment-observe")
    );
}

#[test]
fn regression_cli_deployment_status_json_requires_deployment_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--deployment-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_custom_command_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.custom_command_status_inspect);
    assert!(!cli.custom_command_status_json);
}

#[test]
fn functional_cli_custom_command_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--custom-command-status-inspect",
        "--custom-command-status-json",
        "--custom-command-state-dir",
        ".tau/custom-command-observe",
    ]);
    assert!(cli.custom_command_status_inspect);
    assert!(cli.custom_command_status_json);
    assert_eq!(
        cli.custom_command_state_dir,
        PathBuf::from(".tau/custom-command-observe")
    );
}

#[test]
fn regression_cli_custom_command_status_json_requires_custom_command_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--custom-command-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_voice_status_inspect_defaults_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.voice_status_inspect);
    assert!(!cli.voice_status_json);
}

#[test]
fn functional_cli_voice_status_inspect_accepts_json_and_state_dir_override() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--voice-status-inspect",
        "--voice-status-json",
        "--voice-state-dir",
        ".tau/voice-observe",
    ]);
    assert!(cli.voice_status_inspect);
    assert!(cli.voice_status_json);
    assert_eq!(cli.voice_state_dir, PathBuf::from(".tau/voice-observe"));
}

#[test]
fn regression_cli_voice_status_json_requires_voice_status_inspect() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--voice-status-json"]);
    let error = parse.expect_err("json output should require inspect flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_qa_loop_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(!cli.qa_loop);
    assert!(cli.qa_loop_config.is_none());
    assert!(!cli.qa_loop_json);
    assert!(cli.qa_loop_stage_timeout_ms.is_none());
    assert!(cli.qa_loop_retry_failures.is_none());
    assert!(cli.qa_loop_max_output_bytes.is_none());
    assert!(cli.qa_loop_changed_file_limit.is_none());
}

#[test]
fn functional_cli_qa_loop_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--qa-loop",
        "--qa-loop-config",
        ".tau/qa-loop.json",
        "--qa-loop-json",
        "--qa-loop-stage-timeout-ms",
        "45000",
        "--qa-loop-retry-failures",
        "3",
        "--qa-loop-max-output-bytes",
        "2048",
        "--qa-loop-changed-file-limit",
        "40",
    ]);
    assert!(cli.qa_loop);
    assert_eq!(cli.qa_loop_config, Some(PathBuf::from(".tau/qa-loop.json")));
    assert!(cli.qa_loop_json);
    assert_eq!(cli.qa_loop_stage_timeout_ms, Some(45_000));
    assert_eq!(cli.qa_loop_retry_failures, Some(3));
    assert_eq!(cli.qa_loop_max_output_bytes, Some(2048));
    assert_eq!(cli.qa_loop_changed_file_limit, Some(40));
}

#[test]
fn regression_cli_qa_loop_config_requires_qa_loop() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--qa-loop-config", "qa-loop.json"]);
    let error = parse.expect_err("qa-loop-config should require qa-loop flag");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn functional_cli_mcp_server_flags_accept_external_config_and_context_providers() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--mcp-server",
        "--mcp-external-server-config",
        ".tau/mcp/servers.json",
        "--mcp-context-provider",
        "session",
        "--mcp-context-provider",
        "skills",
    ]);
    assert!(cli.mcp_server);
    assert_eq!(
        cli.mcp_external_server_config.as_deref(),
        Some(Path::new(".tau/mcp/servers.json"))
    );
    assert_eq!(
        cli.mcp_context_provider,
        vec!["session".to_string(), "skills".to_string()]
    );
}

#[test]
fn regression_cli_mcp_context_provider_requires_mcp_server_flag() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--mcp-context-provider", "session"]);
    let error = parse.expect_err("mcp context provider should require mcp server mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}

#[test]
fn unit_cli_prompt_optimization_flags_default_to_disabled() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert!(cli.prompt_optimization_config.is_none());
    assert_eq!(
        cli.prompt_optimization_store_sqlite,
        PathBuf::from(".tau/training/store.sqlite")
    );
    assert!(!cli.prompt_optimization_json);
    assert!(!cli.prompt_optimization_proxy_server);
    assert_eq!(cli.prompt_optimization_proxy_bind, "127.0.0.1:8788");
    assert!(cli.prompt_optimization_proxy_upstream_url.is_none());
    assert_eq!(
        cli.prompt_optimization_proxy_state_dir,
        PathBuf::from(".tau")
    );
    assert_eq!(cli.prompt_optimization_proxy_timeout_ms, 30_000);
}

#[test]
fn functional_cli_prompt_optimization_flags_accept_canonical_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--prompt-optimization-config",
        ".tau/prompt-optimization.json",
        "--prompt-optimization-store-sqlite",
        ".tau/training/alt.sqlite",
        "--prompt-optimization-json",
    ]);
    assert_eq!(
        cli.prompt_optimization_config.as_deref(),
        Some(Path::new(".tau/prompt-optimization.json"))
    );
    assert_eq!(
        cli.prompt_optimization_store_sqlite,
        PathBuf::from(".tau/training/alt.sqlite")
    );
    assert!(cli.prompt_optimization_json);
}

#[test]
fn regression_cli_prompt_optimization_flags_accept_legacy_train_aliases() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--train-config",
        ".tau/train-legacy.json",
        "--train-store-sqlite",
        ".tau/training/legacy.sqlite",
        "--train-json",
    ]);
    assert_eq!(
        cli.prompt_optimization_config.as_deref(),
        Some(Path::new(".tau/train-legacy.json"))
    );
    assert_eq!(
        cli.prompt_optimization_store_sqlite,
        PathBuf::from(".tau/training/legacy.sqlite")
    );
    assert!(cli.prompt_optimization_json);
}

#[test]
fn functional_cli_prompt_optimization_proxy_flags_accept_canonical_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--prompt-optimization-proxy-server",
        "--prompt-optimization-proxy-bind",
        "127.0.0.1:8899",
        "--prompt-optimization-proxy-upstream-url",
        "http://127.0.0.1:4000",
        "--prompt-optimization-proxy-state-dir",
        ".tau-alt",
        "--prompt-optimization-proxy-timeout-ms",
        "45000",
    ]);
    assert!(cli.prompt_optimization_proxy_server);
    assert_eq!(cli.prompt_optimization_proxy_bind, "127.0.0.1:8899");
    assert_eq!(
        cli.prompt_optimization_proxy_upstream_url.as_deref(),
        Some("http://127.0.0.1:4000")
    );
    assert_eq!(
        cli.prompt_optimization_proxy_state_dir,
        PathBuf::from(".tau-alt")
    );
    assert_eq!(cli.prompt_optimization_proxy_timeout_ms, 45_000);
}

#[test]
fn regression_cli_prompt_optimization_proxy_flags_accept_legacy_training_aliases() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--training-proxy-server",
        "--training-proxy-bind",
        "127.0.0.1:8866",
        "--training-proxy-upstream-url",
        "http://127.0.0.1:5000",
        "--training-proxy-state-dir",
        ".tau-legacy",
        "--training-proxy-timeout-ms",
        "42000",
    ]);
    assert!(cli.prompt_optimization_proxy_server);
    assert_eq!(cli.prompt_optimization_proxy_bind, "127.0.0.1:8866");
    assert_eq!(
        cli.prompt_optimization_proxy_upstream_url.as_deref(),
        Some("http://127.0.0.1:5000")
    );
    assert_eq!(
        cli.prompt_optimization_proxy_state_dir,
        PathBuf::from(".tau-legacy")
    );
    assert_eq!(cli.prompt_optimization_proxy_timeout_ms, 42_000);
}

#[test]
fn regression_cli_prompt_optimization_proxy_bind_requires_proxy_server_flag() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--prompt-optimization-proxy-bind",
        "127.0.0.1:8899",
    ]);
    let error = parse.expect_err("proxy bind should require proxy server mode");
    assert!(error
        .to_string()
        .contains("required arguments were not provided"));
}
