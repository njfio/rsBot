use super::*;

#[test]
fn unit_cli_orchestrator_flags_default_values_are_stable() {
    let cli = parse_cli_with_stack(["tau-rs"]);
    assert_eq!(cli.orchestrator_mode, CliOrchestratorMode::Off);
    assert_eq!(cli.orchestrator_max_plan_steps, 8);
    assert_eq!(cli.orchestrator_max_delegated_steps, 8);
    assert_eq!(cli.orchestrator_max_executor_response_chars, 20_000);
    assert_eq!(cli.orchestrator_max_delegated_step_response_chars, 20_000);
    assert_eq!(cli.orchestrator_max_delegated_total_response_chars, 160_000);
    assert!(!cli.orchestrator_delegate_steps);
    assert!(cli.orchestrator_route_table.is_none());
}

#[test]
fn functional_cli_orchestrator_flags_accept_overrides() {
    let cli = parse_cli_with_stack([
        "tau-rs",
        "--orchestrator-mode",
        "plan-first",
        "--orchestrator-max-plan-steps",
        "5",
        "--orchestrator-max-delegated-steps",
        "3",
        "--orchestrator-max-executor-response-chars",
        "160",
        "--orchestrator-max-delegated-step-response-chars",
        "80",
        "--orchestrator-max-delegated-total-response-chars",
        "240",
        "--orchestrator-route-table",
        ".tau/orchestrator/route-table.json",
        "--orchestrator-delegate-steps",
    ]);
    assert_eq!(cli.orchestrator_mode, CliOrchestratorMode::PlanFirst);
    assert_eq!(cli.orchestrator_max_plan_steps, 5);
    assert_eq!(cli.orchestrator_max_delegated_steps, 3);
    assert_eq!(cli.orchestrator_max_executor_response_chars, 160);
    assert_eq!(cli.orchestrator_max_delegated_step_response_chars, 80);
    assert_eq!(cli.orchestrator_max_delegated_total_response_chars, 240);
    assert_eq!(
        cli.orchestrator_route_table.as_deref(),
        Some(Path::new(".tau/orchestrator/route-table.json"))
    );
    assert!(cli.orchestrator_delegate_steps);
}

#[test]
fn regression_cli_orchestrator_executor_response_budget_rejects_zero() {
    let parse =
        try_parse_cli_with_stack(["tau-rs", "--orchestrator-max-executor-response-chars", "0"]);
    let error = parse.expect_err("zero executor budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn regression_cli_orchestrator_delegated_step_count_budget_rejects_zero() {
    let parse = try_parse_cli_with_stack(["tau-rs", "--orchestrator-max-delegated-steps", "0"]);
    let error = parse.expect_err("zero delegated step count budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn regression_cli_orchestrator_delegated_step_response_budget_rejects_zero() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--orchestrator-max-delegated-step-response-chars",
        "0",
    ]);
    let error = parse.expect_err("zero delegated step budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}

#[test]
fn regression_cli_orchestrator_delegated_total_response_budget_rejects_zero() {
    let parse = try_parse_cli_with_stack([
        "tau-rs",
        "--orchestrator-max-delegated-total-response-chars",
        "0",
    ]);
    let error = parse.expect_err("zero delegated total budget should be rejected");
    assert!(error.to_string().contains("greater than 0"));
}
