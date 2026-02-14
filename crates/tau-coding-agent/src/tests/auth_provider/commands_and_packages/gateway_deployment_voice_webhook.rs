//! Tests for gateway/deployment/custom-command/voice/webhook CLI validation guardrails.

use super::*;

#[test]
fn unit_validate_gateway_remote_plan_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;

    validate_gateway_remote_plan_cli(&cli).expect("gateway remote plan config should validate");
}

#[test]
fn functional_validate_gateway_remote_plan_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.prompt = Some("conflict".to_string());

    let error = validate_gateway_remote_plan_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--gateway-remote-plan cannot be combined"));
}

#[test]
fn regression_validate_gateway_remote_plan_cli_rejects_hold_profile_configuration() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleFunnel;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = None;
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let error = validate_gateway_remote_plan_cli(&cli)
        .expect_err("invalid selected profile posture should fail closed");
    assert!(error
        .to_string()
        .contains("gateway remote plan rejected: profile=tailscale-funnel gate=hold"));
    assert!(error
        .to_string()
        .contains("tailscale_funnel_missing_password"));
}

#[test]
fn unit_validate_gateway_openresponses_server_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());

    validate_gateway_openresponses_server_cli(&cli)
        .expect("gateway openresponses server config should validate");
}

#[test]
fn functional_validate_gateway_openresponses_server_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.prompt = Some("conflict".to_string());

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--gateway-openresponses-server cannot be combined"));
}

#[test]
fn integration_validate_gateway_openresponses_server_cli_rejects_transport_conflicts() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.github_issues_bridge = true;

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("transport conflict should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-server cannot be combined with gateway service/daemon commands or other active transport runtime flags"
    ));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_requires_auth_token() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = None;

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("missing auth token should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-token is required when --gateway-openresponses-auth-mode=token"
    ));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_whitespace_auth_token() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("   ".to_string());

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("whitespace auth token should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-token is required when --gateway-openresponses-auth-mode=token"
    ));
}

#[test]
fn unit_validate_gateway_openresponses_server_cli_accepts_password_session_configuration() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = Some("pw-secret".to_string());

    validate_gateway_openresponses_server_cli(&cli)
        .expect("password-session config should validate");
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_requires_password_for_password_mode() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = None;

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("missing password should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-password is required when --gateway-openresponses-auth-mode=password-session"
    ));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_non_loopback_localhost_dev() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::LocalhostDev;
    cli.gateway_openresponses_bind = "0.0.0.0:8787".to_string();

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("non-loopback bind should fail");
    assert!(error.to_string().contains(
        "--gateway-openresponses-auth-mode=localhost-dev requires loopback bind address"
    ));
}

#[test]
fn integration_validate_gateway_openresponses_server_cli_rejects_unsafe_local_only_remote_combo() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::LocalOnly;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.gateway_openresponses_bind = "0.0.0.0:8787".to_string();

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("non-loopback local-only profile should fail");
    assert!(error
        .to_string()
        .contains("gateway remote profile rejected"));
    assert!(error.to_string().contains("local_only_non_loopback_bind"));
}

#[test]
fn integration_validate_gateway_openresponses_server_cli_accepts_tailscale_funnel_profile() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleFunnel;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = Some("edge-password".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    validate_gateway_openresponses_server_cli(&cli)
        .expect("tailscale funnel profile with password-session auth should pass");
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_tailscale_serve_localhost_dev_auth()
{
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleServe;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::LocalhostDev;
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("tailscale-serve should reject localhost-dev auth");
    assert!(error
        .to_string()
        .contains("tailscale_serve_localhost_dev_auth_unsupported"));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_zero_max_input_chars() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.gateway_openresponses_max_input_chars = 0;

    let error = validate_gateway_openresponses_server_cli(&cli)
        .expect_err("zero max input chars should fail");
    assert!(error
        .to_string()
        .contains("--gateway-openresponses-max-input-chars must be greater than 0"));
}

#[test]
fn regression_validate_gateway_openresponses_server_cli_rejects_invalid_bind() {
    let mut cli = test_cli();
    cli.gateway_openresponses_server = true;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("secret-token".to_string());
    cli.gateway_openresponses_bind = "invalid-bind".to_string();

    let error =
        validate_gateway_openresponses_server_cli(&cli).expect_err("invalid bind should fail");
    assert!(error
        .to_string()
        .contains("invalid gateway socket address 'invalid-bind'"));
}

#[test]
fn unit_validate_gateway_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("gateway-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "gateway-success-only",
      "method": "GET",
      "endpoint": "/v1/health",
      "actor_id": "ops-bot",
      "body": {},
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status":"accepted",
          "method":"GET",
          "endpoint":"/v1/health",
          "actor_id":"ops-bot"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path;

    validate_gateway_contract_runner_cli(&cli).expect("gateway runner config should validate");
}

#[test]
fn functional_validate_gateway_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_gateway_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--gateway-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_gateway_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path;
    cli.multi_agent_contract_runner = true;

    let error = validate_gateway_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, or --dashboard-contract-runner"
    ));
}

#[test]
fn regression_validate_gateway_contract_runner_cli_rejects_zero_guardrail_thresholds() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = fixture_path.clone();
    cli.gateway_guardrail_failure_streak_threshold = 0;

    let failure_streak_error = validate_gateway_contract_runner_cli(&cli)
        .expect_err("zero failure-streak threshold should fail");
    assert!(failure_streak_error
        .to_string()
        .contains("--gateway-guardrail-failure-streak-threshold must be greater than 0"));

    cli.gateway_guardrail_failure_streak_threshold = 1;
    cli.gateway_guardrail_retryable_failures_threshold = 0;

    let retryable_error = validate_gateway_contract_runner_cli(&cli)
        .expect_err("zero retryable-failures threshold should fail");
    assert!(retryable_error
        .to_string()
        .contains("--gateway-guardrail-retryable-failures-threshold must be greater than 0"));
}

#[test]
fn regression_validate_gateway_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = temp.path().join("missing.json");

    let error =
        validate_gateway_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_gateway_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.gateway_contract_runner = true;
    cli.gateway_fixture = temp.path().to_path_buf();

    let error =
        validate_gateway_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_deployment_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("deployment-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "deployment-success-only",
      "deploy_target": "container",
      "runtime_profile": "native",
      "blueprint_id": "staging-container",
      "environment": "staging",
      "region": "us-west-2",
      "container_image": "ghcr.io/njfio/tau:staging",
      "replicas": 1,
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status":"accepted",
          "blueprint_id":"staging-container",
          "deploy_target":"container",
          "runtime_profile":"native",
          "environment":"staging",
          "region":"us-west-2",
          "artifact":"ghcr.io/njfio/tau:staging",
          "replicas":1,
          "rollout_strategy":"recreate"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path;

    validate_deployment_contract_runner_cli(&cli)
        .expect("deployment runner config should validate");
}

#[test]
fn functional_validate_deployment_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_deployment_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--deployment-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_deployment_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path;
    cli.voice_contract_runner = true;

    let error = validate_deployment_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --custom-command-contract-runner, or --voice-contract-runner"
    ));
}

#[test]
fn regression_validate_deployment_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = fixture_path.clone();
    cli.deployment_queue_limit = 0;
    let queue_error = validate_deployment_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--deployment-queue-limit must be greater than 0"));

    cli.deployment_queue_limit = 1;
    cli.deployment_processed_case_cap = 0;
    let processed_error =
        validate_deployment_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_error
        .to_string()
        .contains("--deployment-processed-case-cap must be greater than 0"));

    cli.deployment_processed_case_cap = 1;
    cli.deployment_retry_max_attempts = 0;
    let retry_error =
        validate_deployment_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--deployment-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_deployment_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = temp.path().join("missing.json");

    let error =
        validate_deployment_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_deployment_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_contract_runner = true;
    cli.deployment_fixture = temp.path().to_path_buf();

    let error =
        validate_deployment_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_deployment_wasm_package_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.deployment_wasm_package_output_dir = temp.path().join("out");
    cli.deployment_state_dir = temp.path().join(".tau/deployment");

    validate_deployment_wasm_package_cli(&cli)
        .expect("deployment wasm package config should validate");
}

#[test]
fn functional_validate_deployment_wasm_package_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.prompt = Some("conflict".to_string());
    let error =
        validate_deployment_wasm_package_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--deployment-wasm-package-module cannot be combined"));
}

#[test]
fn integration_validate_deployment_wasm_package_cli_rejects_runtime_conflicts() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.events_runner = true;
    let error =
        validate_deployment_wasm_package_cli(&cli).expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_deployment_wasm_package_cli_requires_existing_module() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(temp.path().join("missing.wasm"));
    let error = validate_deployment_wasm_package_cli(&cli).expect_err("missing module should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_deployment_wasm_package_cli_rejects_non_directory_output() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");
    let output_file = temp.path().join("out-file");
    std::fs::write(&output_file, "{}").expect("write output file");

    let mut cli = test_cli();
    cli.deployment_wasm_package_module = Some(module_path);
    cli.deployment_wasm_package_output_dir = output_file;
    let error =
        validate_deployment_wasm_package_cli(&cli).expect_err("output file path should fail");
    assert!(error
        .to_string()
        .contains("--deployment-wasm-package-output-dir"));
}

#[test]
fn unit_validate_deployment_wasm_inspect_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("edge.manifest.json");
    std::fs::write(&manifest_path, "{}").expect("write manifest placeholder");

    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);

    validate_deployment_wasm_inspect_cli(&cli)
        .expect("deployment wasm inspect config should validate");
}

#[test]
fn functional_validate_deployment_wasm_inspect_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("edge.manifest.json");
    std::fs::write(&manifest_path, "{}").expect("write manifest placeholder");

    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);
    cli.prompt = Some("conflict".to_string());
    let error =
        validate_deployment_wasm_inspect_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--deployment-wasm-inspect-manifest cannot be combined"));
}

#[test]
fn integration_validate_deployment_wasm_inspect_cli_rejects_runtime_conflicts() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("edge.manifest.json");
    std::fs::write(&manifest_path, "{}").expect("write manifest placeholder");

    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);
    cli.events_runner = true;
    let error =
        validate_deployment_wasm_inspect_cli(&cli).expect_err("runtime conflict should fail");
    assert!(error
        .to_string()
        .contains("active transport/runtime commands"));
}

#[test]
fn regression_validate_deployment_wasm_inspect_cli_requires_existing_manifest() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(temp.path().join("missing.manifest.json"));
    let error =
        validate_deployment_wasm_inspect_cli(&cli).expect_err("missing manifest should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_deployment_wasm_inspect_cli_rejects_directory_manifest_path() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.deployment_wasm_inspect_manifest = Some(temp.path().to_path_buf());
    let error = validate_deployment_wasm_inspect_cli(&cli)
        .expect_err("directory manifest path should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_custom_command_contract_runner_cli_is_removed() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("custom-command-fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = fixture_path;

    let error = validate_custom_command_contract_runner_cli(&cli)
        .expect_err("contract runner should be rejected");
    assert!(error
        .to_string()
        .contains("--custom-command-contract-runner has been removed"));
    assert!(error
        .to_string()
        .contains("--custom-command-status-inspect"));
}

#[test]
fn regression_validate_custom_command_contract_runner_cli_reports_migration_even_with_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.custom_command_contract_runner = true;
    cli.custom_command_fixture = fixture_path;
    cli.gateway_contract_runner = true;

    let error =
        validate_custom_command_contract_runner_cli(&cli).expect_err("contract runner removal");
    assert!(error
        .to_string()
        .contains("--custom-command-contract-runner has been removed"));
}

#[test]
fn unit_validate_voice_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "voice-success-only",
      "mode": "turn",
      "wake_word": "tau",
      "transcript": "tau open dashboard",
      "locale": "en-US",
      "speaker_id": "ops",
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status":"accepted",
          "mode":"turn",
          "wake_word":"tau",
          "utterance":"open dashboard",
          "locale":"en-US",
          "speaker_id":"ops"
        }
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;

    validate_voice_contract_runner_cli(&cli).expect("voice runner config should validate");
}

#[test]
fn functional_validate_voice_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_voice_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--voice-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_voice_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path;
    cli.custom_command_contract_runner = true;

    let error = validate_voice_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, --multi-agent-contract-runner, --memory-contract-runner, --dashboard-contract-runner, --gateway-contract-runner, --custom-command-contract-runner, or --voice-live-runner"
    ));
}

#[test]
fn regression_validate_voice_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = fixture_path.clone();
    cli.voice_queue_limit = 0;
    let queue_error = validate_voice_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--voice-queue-limit must be greater than 0"));

    cli.voice_queue_limit = 1;
    cli.voice_processed_case_cap = 0;
    let processed_error =
        validate_voice_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_error
        .to_string()
        .contains("--voice-processed-case-cap must be greater than 0"));

    cli.voice_processed_case_cap = 1;
    cli.voice_retry_max_attempts = 0;
    let retry_error =
        validate_voice_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--voice-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_voice_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = temp.path().join("missing.json");

    let error = validate_voice_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_voice_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_contract_runner = true;
    cli.voice_fixture = temp.path().to_path_buf();

    let error =
        validate_voice_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_voice_live_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-live.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "session_id": "ops-live",
  "frames": [
    {
      "frame_id": "f-1",
      "transcript": "tau open dashboard",
      "speaker_id": "ops-live",
      "locale": "en-US"
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.voice_live_runner = true;
    cli.voice_live_input = fixture_path;

    validate_voice_live_runner_cli(&cli).expect("voice live config should validate");
}

#[test]
fn functional_validate_voice_live_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("voice-live.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.voice_live_runner = true;
    cli.voice_live_input = fixture_path;
    cli.voice_contract_runner = true;

    let error = validate_voice_live_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains("--voice-contract-runner"));
}

#[test]
fn regression_validate_voice_live_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_live_runner = true;
    cli.voice_live_input = temp.path().join("missing.json");

    let error = validate_voice_live_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_event_webhook_ingest_cli_requires_channel() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = None;
    let error = validate_event_webhook_ingest_cli(&cli).expect_err("missing channel");
    assert!(error
        .to_string()
        .contains("--event-webhook-channel is required"));
}

#[test]
fn functional_validate_event_webhook_ingest_cli_requires_signing_arguments_together() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());

    let error = validate_event_webhook_ingest_cli(&cli).expect_err("algorithm should be required");
    assert!(error
        .to_string()
        .contains("--event-webhook-signature-algorithm is required"));
}

#[test]
fn functional_validate_event_webhook_ingest_cli_accepts_secret_id_configuration() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret_id = Some("event-webhook-secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

    validate_event_webhook_ingest_cli(&cli).expect("webhook config should validate");
}

#[test]
fn regression_validate_event_webhook_ingest_cli_requires_timestamp_for_slack_v0() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("slack/C123".to_string());
    cli.event_webhook_signature = Some("v0=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::SlackV0);
    cli.event_webhook_timestamp = None;

    let error = validate_event_webhook_ingest_cli(&cli).expect_err("timestamp should be required");
    assert!(error
        .to_string()
        .contains("--event-webhook-timestamp is required"));
}

#[test]
fn unit_validate_event_webhook_ingest_cli_accepts_signed_github_configuration() {
    let mut cli = test_cli();
    cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
    cli.event_webhook_channel = Some("github/owner/repo#1".to_string());
    cli.event_webhook_signature = Some("sha256=abcd".to_string());
    cli.event_webhook_secret = Some("secret".to_string());
    cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

    validate_event_webhook_ingest_cli(&cli).expect("signed github webhook config should pass");
}
