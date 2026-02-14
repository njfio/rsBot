//! Tests for dashboard contract runner, gateway service, daemon, and remote-profile inspect CLI validation.

use super::*;

#[test]
fn unit_validate_dashboard_contract_runner_cli_accepts_minimum_configuration() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("dashboard-fixture.json");
    std::fs::write(
        &fixture_path,
        r#"{
  "schema_version": 1,
  "name": "single-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "snapshot-basic",
      "mode": "snapshot",
      "scope": { "workspace_id": "tau-core", "operator_id": "ops-1" },
      "requested_widgets": [
        {
          "widget_id": "health-summary",
          "kind": "health_summary",
          "title": "Health Summary",
          "query_key": "health.summary",
          "refresh_interval_ms": 15000
        }
      ],
      "expected": {
        "outcome": "success",
        "widgets": [
          {
            "widget_id": "health-summary",
            "kind": "health_summary",
            "title": "Health Summary",
            "query_key": "health.summary",
            "refresh_interval_ms": 15000
          }
        ]
      }
    }
  ]
}"#,
    )
    .expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path;

    validate_dashboard_contract_runner_cli(&cli).expect("dashboard runner config should validate");
}

#[test]
fn functional_validate_dashboard_contract_runner_cli_rejects_prompt_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path;
    cli.prompt = Some("conflict".to_string());

    let error = validate_dashboard_contract_runner_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--dashboard-contract-runner cannot be combined"));
}

#[test]
fn integration_validate_dashboard_contract_runner_cli_rejects_transport_conflicts() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path;
    cli.memory_contract_runner = true;

    let error = validate_dashboard_contract_runner_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--github-issues-bridge, --slack-bridge, --events-runner, --multi-channel-contract-runner, --multi-channel-live-runner, or --memory-contract-runner"
    ));
}

#[test]
fn regression_validate_dashboard_contract_runner_cli_rejects_zero_limits() {
    let temp = tempdir().expect("tempdir");
    let fixture_path = temp.path().join("fixture.json");
    std::fs::write(&fixture_path, "{}").expect("write fixture");

    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = fixture_path.clone();
    cli.dashboard_queue_limit = 0;
    let queue_error = validate_dashboard_contract_runner_cli(&cli).expect_err("zero queue limit");
    assert!(queue_error
        .to_string()
        .contains("--dashboard-queue-limit must be greater than 0"));

    cli.dashboard_queue_limit = 1;
    cli.dashboard_processed_case_cap = 0;
    let processed_case_error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("zero processed case cap");
    assert!(processed_case_error
        .to_string()
        .contains("--dashboard-processed-case-cap must be greater than 0"));

    cli.dashboard_processed_case_cap = 1;
    cli.dashboard_retry_max_attempts = 0;
    let retry_error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("zero retry max attempts");
    assert!(retry_error
        .to_string()
        .contains("--dashboard-retry-max-attempts must be greater than 0"));
}

#[test]
fn regression_validate_dashboard_contract_runner_cli_requires_existing_fixture() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = temp.path().join("missing.json");

    let error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("missing fixture should fail");
    assert!(error.to_string().contains("does not exist"));
}

#[test]
fn regression_validate_dashboard_contract_runner_cli_requires_fixture_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.dashboard_contract_runner = true;
    cli.dashboard_fixture = temp.path().to_path_buf();

    let error =
        validate_dashboard_contract_runner_cli(&cli).expect_err("directory fixture should fail");
    assert!(error.to_string().contains("must point to a file"));
}

#[test]
fn unit_validate_gateway_service_cli_accepts_status_mode() {
    let mut cli = test_cli();
    cli.gateway_service_status = true;
    cli.gateway_service_status_json = true;

    validate_gateway_service_cli(&cli).expect("gateway service status config should validate");
}

#[test]
fn functional_validate_gateway_service_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_service_start = true;
    cli.prompt = Some("conflict".to_string());

    let error = validate_gateway_service_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--gateway-service-* commands cannot be combined"));
}

#[test]
fn integration_validate_gateway_service_cli_rejects_transport_conflicts() {
    let mut cli = test_cli();
    cli.gateway_service_stop = true;
    cli.gateway_contract_runner = true;

    let error = validate_gateway_service_cli(&cli).expect_err("transport conflict");
    assert!(error.to_string().contains(
        "--gateway-service-* commands cannot be combined with active transport runtime flags"
    ));
}

#[test]
fn regression_validate_gateway_service_cli_rejects_whitespace_stop_reason() {
    let mut cli = test_cli();
    cli.gateway_service_stop = true;
    cli.gateway_service_stop_reason = Some("   ".to_string());

    let error = validate_gateway_service_cli(&cli).expect_err("whitespace stop reason should fail");
    assert!(error
        .to_string()
        .contains("--gateway-service-stop-reason cannot be empty or whitespace"));
}

#[test]
fn unit_validate_daemon_cli_accepts_status_mode() {
    let mut cli = test_cli();
    cli.daemon_status = true;
    cli.daemon_status_json = true;

    validate_daemon_cli(&cli).expect("daemon status config should validate");
}

#[test]
fn functional_validate_daemon_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.daemon_install = true;
    cli.prompt = Some("conflict".to_string());

    let error = validate_daemon_cli(&cli).expect_err("prompt conflict");
    assert!(error
        .to_string()
        .contains("--daemon-* commands cannot be combined"));
}

#[test]
fn integration_validate_daemon_cli_rejects_transport_conflicts() {
    let mut cli = test_cli();
    cli.daemon_start = true;
    cli.gateway_contract_runner = true;

    let error = validate_daemon_cli(&cli).expect_err("transport conflict");
    assert!(error
        .to_string()
        .contains("--daemon-* commands cannot be combined with active transport/runtime flags"));
}

#[test]
fn integration_validate_daemon_cli_rejects_status_preflight_conflicts() {
    let mut cli = test_cli();
    cli.daemon_status = true;
    cli.gateway_status_inspect = true;

    let error = validate_daemon_cli(&cli).expect_err("status conflict");
    assert!(error.to_string().contains(
        "--daemon-* commands cannot be combined with status/inspection preflight commands"
    ));
}

#[test]
fn regression_validate_daemon_cli_rejects_whitespace_stop_reason() {
    let mut cli = test_cli();
    cli.daemon_stop = true;
    cli.daemon_stop_reason = Some("   ".to_string());

    let error = validate_daemon_cli(&cli).expect_err("whitespace stop reason should fail");
    assert!(error
        .to_string()
        .contains("--daemon-stop-reason cannot be empty or whitespace"));
}

#[test]
fn unit_validate_gateway_remote_profile_inspect_cli_accepts_minimum_configuration() {
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;

    validate_gateway_remote_profile_inspect_cli(&cli)
        .expect("gateway remote profile inspect config should validate");
}

#[test]
fn functional_validate_gateway_remote_profile_inspect_cli_rejects_prompt_conflicts() {
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;
    cli.prompt = Some("conflict".to_string());

    let error =
        validate_gateway_remote_profile_inspect_cli(&cli).expect_err("prompt conflict should fail");
    assert!(error
        .to_string()
        .contains("--gateway-remote-profile-inspect cannot be combined"));
}
