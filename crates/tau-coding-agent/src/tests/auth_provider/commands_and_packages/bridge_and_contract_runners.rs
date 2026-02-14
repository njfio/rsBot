//! Tests for bridge and contract-runner CLI validation surfaces.

use super::*;
use tau_cli::{
    validate_browser_automation_contract_runner_cli, validate_browser_automation_live_runner_cli,
};

#[test]
fn unit_validate_github_issues_bridge_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());

    validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
}

#[test]
fn unit_validate_github_issues_bridge_cli_accepts_token_id_configuration() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token_id = Some("github-token".to_string());

    validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
}

#[test]
fn functional_validate_github_issues_bridge_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.prompt = Some("conflict".to_string());

    let error = validate_github_issues_bridge_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--github-issues-bridge cannot be combined"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_github_issues_bridge_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_requires_credentials() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = None;
    cli.github_token_id = None;

    let error = validate_github_issues_bridge_cli(&cli).expect_err("missing token");
    assert!(error
        .to_string()
        .contains("--github-token (or --github-token-id) is required"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_empty_required_labels() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.github_required_label = vec!["  ".to_string()];

    let error = validate_github_issues_bridge_cli(&cli).expect_err("empty label should fail");
    assert!(error
        .to_string()
        .contains("--github-required-label cannot be empty"));
}

#[test]
fn regression_validate_github_issues_bridge_cli_rejects_zero_issue_number() {
    let mut cli = test_cli();
    cli.github_issues_bridge = true;
    cli.github_repo = Some("owner/repo".to_string());
    cli.github_token = Some("token".to_string());
    cli.github_issue_number = vec![0];

    let error = validate_github_issues_bridge_cli(&cli).expect_err("zero issue number");
    assert!(error
        .to_string()
        .contains("--github-issue-number must be greater than 0"));
}

#[test]
fn unit_validate_slack_bridge_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());

    validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
}

#[test]
fn unit_validate_slack_bridge_cli_accepts_token_id_configuration() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token_id = Some("slack-app-token".to_string());
    cli.slack_bot_token_id = Some("slack-bot-token".to_string());

    validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
}

#[test]
fn functional_validate_slack_bridge_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());
    cli.prompt = Some("conflict".to_string());

    let error = validate_slack_bridge_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--slack-bridge cannot be combined"));
}

#[test]
fn regression_validate_slack_bridge_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = Some("xoxb-test".to_string());
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_slack_bridge_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn regression_validate_slack_bridge_cli_rejects_missing_tokens() {
    let mut cli = test_cli();
    cli.slack_bridge = true;
    cli.slack_app_token = Some("xapp-test".to_string());
    cli.slack_bot_token = None;
    cli.slack_app_token_id = None;
    cli.slack_bot_token_id = None;

    let error = validate_slack_bridge_cli(&cli).expect_err("missing slack bot token");
    assert!(error
        .to_string()
        .contains("--slack-bot-token (or --slack-bot-token-id) is required"));
}

#[test]
fn unit_validate_events_runner_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.events_runner = true;
    validate_events_runner_cli(&cli).expect("events runner config should validate");
}

#[test]
fn functional_validate_events_runner_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.events_runner = true;
    cli.prompt = Some("conflict".to_string());
    let error = validate_events_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--events-runner cannot be combined"));
}

#[test]
fn regression_validate_events_runner_cli_rejects_prompt_template_conflicts() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.events_runner = true;
    cli.prompt_template_file = Some(temp.path().join("template.txt"));

    let error = validate_events_runner_cli(&cli).expect_err("template conflict");
    assert!(error.to_string().contains("--prompt-template-file"));
}

#[test]
fn unit_validate_multi_channel_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-event",
  "events": [
    {
      "schema_version": 1,
      "transport": "telegram",
      "event_kind": "message",
      "event_id": "telegram-1",
      "conversation_id": "telegram-chat-1",
      "actor_id": "telegram-user-1",
      "timestamp_ms": 1760000000000,
      "text": "hello",
      "metadata": {}
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path;

    validate_multi_channel_contract_runner_cli(&cli)
        .expect("multi-channel runner config should validate");
}

#[test]
fn functional_validate_multi_channel_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_multi_channel_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-channel-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path;
    cli.events_runner = true;

    let error = validate_multi_channel_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_channel_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = fixture_path.clone();
    cli.multi_channel_queue_limit = 0;
    let queue_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--multi-channel-queue-limit must be greater than 0"));

    cli.multi_channel_queue_limit = 1;
    cli.multi_channel_processed_event_cap = 0;
    let processed_cap_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero processed event cap");
    assert!(processed_cap_error
        .to_string()
        .contains("--multi-channel-processed-event-cap must be greater than 0"));

    cli.multi_channel_processed_event_cap = 1;
    cli.multi_channel_retry_max_attempts = 0;
    let retry_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--multi-channel-retry-max-attempts must be greater than 0"));

    cli.multi_channel_retry_max_attempts = 1;
    cli.multi_channel_outbound_max_chars = 0;
    let outbound_chunk_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero outbound chunk size");
    assert!(outbound_chunk_error
        .to_string()
        .contains("--multi-channel-outbound-max-chars must be greater than 0"));

    cli.multi_channel_outbound_max_chars = 1;
    cli.multi_channel_outbound_http_timeout_ms = 0;
    let outbound_timeout_error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("zero outbound timeout");
    assert!(outbound_timeout_error
        .to_string()
        .contains("--multi-channel-outbound-http-timeout-ms must be greater than 0"));
}

#[test]
fn regression_validate_multi_channel_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = temp.path().join("missing.json");

    let error =
        validate_multi_channel_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_channel_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_contract_runner = true;
    cli.multi_channel_fixture = temp.path().to_path_buf();

    let error = validate_multi_channel_contract_runner_cli(&cli)
        .expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_multi_channel_live_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;

    validate_multi_channel_live_runner_cli(&cli)
        .expect("multi-channel live runner config should validate");
}

#[test]
fn functional_validate_multi_channel_live_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;
    cli.prompt = Some("conflict".to_string());

    let error = validate_multi_channel_live_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-channel-live-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_live_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;
    cli.events_runner = true;

    let error = validate_multi_channel_live_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_channel_live_runner_cli_rejects_missing_ingress_dir() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("missing-ingress");

    let error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("missing ingress dir should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_channel_live_runner_cli_requires_ingress_directory() {
    let temp = tempdir().expect("tempdir");
    let ingress_file = temp.path().join("ingress.ndjson");
    std::fs::write(&ingress_file, "{}\n").expect("write ingress file");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_file;

    let error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("ingress path file should fail");
    assert!(error.to_string().contains("must point to a directory"));
}

#[test]
fn regression_validate_multi_channel_live_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress directory");

    let mut cli = test_cli();
    cli.multi_channel_live_runner = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;
    cli.multi_channel_queue_limit = 0;
    let queue_error = validate_multi_channel_live_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--multi-channel-queue-limit must be greater than 0"));

    cli.multi_channel_queue_limit = 1;
    cli.multi_channel_outbound_max_chars = 0;
    let chunk_error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("zero outbound chunk size");
    assert!(chunk_error
        .to_string()
        .contains("--multi-channel-outbound-max-chars must be greater than 0"));

    cli.multi_channel_outbound_max_chars = 1;
    cli.multi_channel_outbound_http_timeout_ms = 0;
    let timeout_error =
        validate_multi_channel_live_runner_cli(&cli).expect_err("zero outbound timeout");
    assert!(timeout_error
        .to_string()
        .contains("--multi-channel-outbound-http-timeout-ms must be greater than 0"));
}

#[test]
fn unit_validate_multi_channel_live_connectors_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_telegram_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;

    validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect("multi-channel live connectors config should validate");
}

#[test]
fn functional_validate_multi_channel_live_connectors_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_telegram_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;
    cli.prompt = Some("conflict".to_string());

    let error =
        validate_multi_channel_live_connectors_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-channel-live-connectors-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_live_connectors_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");
    cli.multi_channel_telegram_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;
    cli.events_runner = true;

    let error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("transport conflict should fail");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_channel_live_connectors_runner_cli_rejects_invalid_modes_and_bindings()
{
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    cli.multi_channel_live_connectors_runner = true;
    cli.multi_channel_live_ingress_dir = temp.path().join("live-ingress");
    cli.multi_channel_live_connectors_state_path = temp.path().join("connectors-state.json");

    let no_mode_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("missing mode should fail");
    assert!(no_mode_error
        .to_string()
        .contains("at least one connector mode must be enabled"));

    cli.multi_channel_discord_ingress_mode = CliMultiChannelLiveConnectorMode::Webhook;
    let discord_mode_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("discord webhook should fail");
    assert!(discord_mode_error
        .to_string()
        .contains("--multi-channel-discord-ingress-mode=webhook is not supported"));

    cli.multi_channel_discord_ingress_mode = CliMultiChannelLiveConnectorMode::Polling;
    let discord_ids_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("discord polling without channel ids should fail");
    assert!(discord_ids_error
        .to_string()
        .contains("--multi-channel-discord-ingress-channel-id is required"));

    cli.multi_channel_discord_ingress_channel_ids = vec!["ops-room".to_string()];
    cli.multi_channel_whatsapp_ingress_mode = CliMultiChannelLiveConnectorMode::Webhook;
    cli.multi_channel_live_connectors_poll_once = true;
    let poll_once_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("poll once cannot pair with webhook mode");
    assert!(poll_once_error.to_string().contains(
        "--multi-channel-live-connectors-poll-once cannot be used with webhook connector modes"
    ));

    cli.multi_channel_live_connectors_poll_once = false;
    cli.multi_channel_live_webhook_bind = "invalid bind".to_string();
    let bind_error = validate_multi_channel_live_connectors_runner_cli(&cli)
        .expect_err("invalid bind should fail");
    assert!(bind_error
        .to_string()
        .contains("invalid --multi-channel-live-webhook-bind"));
}

#[test]
fn unit_validate_multi_channel_live_ingest_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("telegram-update.json");
    std::fs::write(
        &payload_file,
        r#"{"update_id":1,"message":{"message_id":2,"chat":{"id":"chat-1"},"from":{"id":"user-1"},"date":1760100000,"text":"hello"}}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    validate_multi_channel_live_ingest_cli(&cli)
        .expect("multi-channel live ingest config should validate");
}

#[test]
fn functional_validate_multi_channel_live_ingest_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("discord-message.json");
    std::fs::write(
        &payload_file,
        r#"{"id":"m1","channel_id":"c1","timestamp":"2026-01-10T00:00:00Z","content":"hello","author":{"id":"u1"}}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Discord);
    cli.events_runner = true;

    let error =
        validate_multi_channel_live_ingest_cli(&cli).expect_err("transport conflict should fail");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, or --memory-contract-runner"
    ));
}

#[test]
fn integration_validate_multi_channel_live_ingest_cli_requires_existing_payload_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(temp.path().join("missing.json"));
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Whatsapp);
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    let error =
        validate_multi_channel_live_ingest_cli(&cli).expect_err("missing payload should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_channel_live_ingest_cli_rejects_empty_provider() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("whatsapp-message.json");
    std::fs::write(
        &payload_file,
        r#"{"metadata":{"phone_number_id":"p1"},"messages":[{"id":"mid","from":"15550001111","timestamp":"1760300000","text":{"body":"hello"}}]}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Whatsapp);
    cli.multi_channel_live_ingest_provider = "   ".to_string();
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    let error = validate_multi_channel_live_ingest_cli(&cli)
        .expect_err("empty provider should be rejected");
    assert!(error
        .to_string()
        .contains("--multi-channel-live-ingest-provider cannot be empty"));
}

#[test]
fn unit_validate_multi_channel_channel_lifecycle_cli_accepts_status_mode() {
    let mut cli = test_cli();
    cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Telegram);
    validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect("multi-channel lifecycle status config should validate");
}

#[test]
fn functional_validate_multi_channel_channel_lifecycle_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Discord);
    cli.prompt = Some("conflict".to_string());
    let error =
        validate_multi_channel_channel_lifecycle_cli(&cli).expect_err("prompt conflict expected");
    assert!(error
        .to_string()
        .contains("--multi-channel-channel-* commands cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_channel_lifecycle_cli_rejects_runtime_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_channel_probe = Some(CliMultiChannelTransport::Whatsapp);
    cli.events_runner = true;
    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_multi_channel_channel_lifecycle_cli_rejects_multiple_actions() {
    let mut cli = test_cli();
    cli.multi_channel_channel_login = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_channel_probe = Some(CliMultiChannelTransport::Telegram);
    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("multiple lifecycle actions should fail");
    assert!(error.to_string().contains("mutually exclusive"));
}

#[test]
fn regression_validate_multi_channel_channel_lifecycle_cli_rejects_file_state_dir() {
    let temp = tempdir().expect("tempdir");
    let state_file = temp.path().join("multi-channel-state-file");
    std::fs::write(&state_file, "{}").expect("write state file");

    let mut cli = test_cli();
    cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_state_dir = state_file;
    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("state-dir file path should fail");
    assert!(error.to_string().contains("--multi-channel-state-dir"));
}

#[test]
fn regression_validate_multi_channel_channel_lifecycle_cli_rejects_probe_online_without_probe() {
    let mut cli = test_cli();
    cli.multi_channel_channel_probe_online = true;

    let error = validate_multi_channel_channel_lifecycle_cli(&cli)
        .expect_err("probe online without probe action should fail");
    assert!(error
        .to_string()
        .contains("--multi-channel-channel-probe-online requires --multi-channel-channel-probe"));
}

#[test]
fn unit_validate_multi_channel_send_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_send_target = Some("-100123456".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
    validate_multi_channel_send_cli(&cli).expect("multi-channel send config should validate");
}

#[test]
fn functional_validate_multi_channel_send_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Discord);
    cli.multi_channel_send_target = Some("1234567890123".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
    cli.prompt = Some("conflict".to_string());
    let error = validate_multi_channel_send_cli(&cli).expect_err("prompt conflict expected");
    assert!(error
        .to_string()
        .contains("--multi-channel-send cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_send_cli_rejects_runtime_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Whatsapp);
    cli.multi_channel_send_target = Some("+15551230000".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;
    cli.events_runner = true;
    let error = validate_multi_channel_send_cli(&cli).expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_multi_channel_send_cli_rejects_channel_store_mode() {
    let mut cli = test_cli();
    cli.multi_channel_send = Some(CliMultiChannelTransport::Discord);
    cli.multi_channel_send_target = Some("1234567890123".to_string());
    cli.multi_channel_send_text = Some("hello".to_string());
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::ChannelStore;
    let error = validate_multi_channel_send_cli(&cli).expect_err("channel-store mode should fail");
    assert!(error.to_string().contains(
        "--multi-channel-send requires --multi-channel-outbound-mode=dry-run or provider"
    ));
}

#[test]
fn unit_validate_multi_channel_incident_timeline_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    validate_multi_channel_incident_timeline_cli(&cli)
        .expect("incident timeline config should validate");
}

#[test]
fn functional_validate_multi_channel_incident_timeline_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    cli.prompt = Some("conflict".to_string());
    let error = validate_multi_channel_incident_timeline_cli(&cli)
        .expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--multi-channel-incident-timeline cannot be combined"));
}

#[test]
fn integration_validate_multi_channel_incident_timeline_cli_rejects_runtime_conflicts() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    cli.events_runner = true;
    let error = validate_multi_channel_incident_timeline_cli(&cli)
        .expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_multi_channel_incident_timeline_cli_rejects_inverted_window() {
    let mut cli = test_cli();
    cli.multi_channel_incident_timeline = true;
    cli.multi_channel_incident_start_unix_ms = Some(200);
    cli.multi_channel_incident_end_unix_ms = Some(100);
    let error = validate_multi_channel_incident_timeline_cli(&cli)
        .expect_err("inverted window should fail");
    assert!(error.to_string().contains(
        "--multi-channel-incident-end-unix-ms must be greater than or equal to --multi-channel-incident-start-unix-ms"
    ));
}

#[test]
fn unit_validate_multi_agent_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("multi-agent-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "planner-success",
      "phase": "planner",
      "route_table": {
        "schema_version": 1,
        "roles": {
          "planner": {},
          "reviewer": {}
        },
        "planner": { "role": "planner" },
        "delegated": { "role": "planner" },
        "review": { "role": "reviewer" }
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path;

    validate_multi_agent_contract_runner_cli(&cli)
        .expect("multi-agent runner config should validate");
}

#[test]
fn functional_validate_multi_agent_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_multi_agent_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--multi-agent-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_multi_agent_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path;
    cli.dashboard_contract_runner = true;

    let error = validate_multi_agent_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --memory-contract-runner, or --dashboard-contract-runner"
    ));
}

#[test]
fn regression_validate_multi_agent_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = fixture_path.clone();
    cli.multi_agent_queue_limit = 0;
    let queue_error = validate_multi_agent_contract_runner_cli(&cli).expect_err("zero queue");
    assert!(queue_error
        .to_string()
        .contains("--multi-agent-queue-limit must be greater than 0"));

    cli.multi_agent_queue_limit = 1;
    cli.multi_agent_processed_case_cap = 0;
    let processed_error =
        validate_multi_agent_contract_runner_cli(&cli).expect_err("zero processed cap");
    assert!(processed_error
        .to_string()
        .contains("--multi-agent-processed-case-cap must be greater than 0"));

    cli.multi_agent_processed_case_cap = 1;
    cli.multi_agent_retry_max_attempts = 0;
    let retry_error = validate_multi_agent_contract_runner_cli(&cli).expect_err("zero retry max");
    assert!(retry_error
        .to_string()
        .contains("--multi-agent-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_multi_agent_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = temp.path().join("missing.json");

    let error =
        validate_multi_agent_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_multi_agent_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_agent_contract_runner = true;
    cli.multi_agent_fixture = temp.path().to_path_buf();

    let error =
        validate_multi_agent_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_browser_automation_contract_runner_cli_is_removed() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("browser-automation-fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.browser_automation_contract_runner = true;
    cli.browser_automation_fixture = fixture_path;

    let error = validate_browser_automation_contract_runner_cli(&cli)
        .expect_err("contract runner should be rejected");
    assert!(error
        .to_string()
        .contains("--browser-automation-contract-runner has been removed"));
    assert!(error
        .to_string()
        .contains("--browser-automation-live-runner"));
}

#[test]
fn regression_validate_browser_automation_contract_runner_cli_reports_migration_even_with_conflicts(
) {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.browser_automation_contract_runner = true;
    cli.browser_automation_fixture = fixture_path;
    cli.dashboard_contract_runner = true;

    let error =
        validate_browser_automation_contract_runner_cli(&cli).expect_err("contract runner removal");
    assert!(error
        .to_string()
        .contains("--browser-automation-contract-runner has been removed"));
}

#[test]
fn unit_validate_browser_automation_live_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("browser-automation-live-fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.browser_automation_live_runner = true;
    cli.browser_automation_live_fixture = fixture_path;

    validate_browser_automation_live_runner_cli(&cli)
        .expect("browser automation live runner config should validate");
}

#[test]
fn integration_validate_browser_automation_live_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("browser-automation-live-fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.browser_automation_live_runner = true;
    cli.browser_automation_live_fixture = fixture_path;
    cli.voice_live_runner = true;

    let error = validate_browser_automation_live_runner_cli(&cli).expect_err("transport conflict");
    assert!(error
        .to_string()
        .contains("--browser-automation-contract-runner"));
    assert!(error.to_string().contains("--voice-live-runner"));
}

#[test]
fn regression_validate_browser_automation_live_runner_cli_requires_non_empty_playwright_cli() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("browser-automation-live-fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.browser_automation_live_runner = true;
    cli.browser_automation_live_fixture = fixture_path;
    cli.browser_automation_playwright_cli = "   ".to_string();

    let error = validate_browser_automation_live_runner_cli(&cli)
        .expect_err("empty playwright cli should fail");
    assert!(error
        .to_string()
        .contains("--browser-automation-playwright-cli"));
}

#[test]
fn regression_validate_browser_automation_live_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.browser_automation_live_runner = true;
    cli.browser_automation_live_fixture = temp.path().join("missing-live-fixture.json");

    let error =
        validate_browser_automation_live_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn unit_validate_memory_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("memory-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "extract-basic",
      "mode": "extract",
      "scope": { "workspace_id": "tau-core" },
      "input_text": "Remember release checklist",
      "expected": {
        "outcome": "success",
        "entries": [
          {
            "memory_id": "mem-extract-basic",
            "summary": "Remember release checklist",
            "tags": [ "remember", "release", "checklist" ],
            "facts": [ "scope=tau-core" ],
            "source_event_key": "tau-core:extract:extract-basic",
            "recency_weight_bps": 9000,
            "confidence_bps": 8200
          }
        ]
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path;

    validate_memory_contract_runner_cli(&cli).expect("memory runner config should validate");
}

#[test]
fn functional_validate_memory_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_memory_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--memory-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_memory_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path;
    cli.multi_channel_contract_runner = true;

    let error = validate_memory_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, or --multi-channel-live-runner"
    ));
}

#[test]
fn regression_validate_memory_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = fixture_path.clone();
    cli.memory_queue_limit = 0;
    let queue_error = validate_memory_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--memory-queue-limit must be greater than 0"));

    cli.memory_queue_limit = 1;
    cli.memory_processed_case_cap = 0;
    let processed_case_error =
        validate_memory_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_case_error
        .to_string()
        .contains("--memory-processed-case-cap must be greater than 0"));

    cli.memory_processed_case_cap = 1;
    cli.memory_retry_max_attempts = 0;
    let retry_error =
        validate_memory_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--memory-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_memory_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = temp.path().join("missing.json");

    let error = validate_memory_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_memory_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.memory_contract_runner = true;
    cli.memory_fixture = temp.path().to_path_buf();

    let error =
        validate_memory_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}
