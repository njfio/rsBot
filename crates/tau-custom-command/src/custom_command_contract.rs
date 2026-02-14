use std::collections::BTreeSet;
use std::path::Path;

use crate::custom_command_policy::{
    default_custom_command_execution_policy, is_valid_command_name,
    validate_custom_command_execution_policy, validate_custom_command_spec,
    validate_custom_command_template_and_arguments, CustomCommandExecutionPolicy,
    CustomCommandSpec,
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tau_contract::{
    ensure_unique_case_ids, load_fixture_from_path, parse_fixture_with_validation,
    validate_fixture_header,
};

pub const CUSTOM_COMMAND_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub const CUSTOM_COMMAND_ERROR_INVALID_OPERATION: &str = "custom_command_invalid_operation";
pub const CUSTOM_COMMAND_ERROR_INVALID_NAME: &str = "custom_command_invalid_name";
pub const CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE: &str = "custom_command_invalid_template";
pub const CUSTOM_COMMAND_ERROR_POLICY_DENIED: &str = "custom_command_policy_denied";
pub const CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE: &str = "custom_command_backend_unavailable";

fn custom_command_contract_schema_version() -> u32 {
    CUSTOM_COMMAND_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `CustomCommandOutcomeKind` values.
pub enum CustomCommandOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `CustomCommandCaseExpectation` used across Tau components.
pub struct CustomCommandCaseExpectation {
    pub outcome: CustomCommandOutcomeKind,
    pub status_code: u16,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub response_body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `CustomCommandContractCase` used across Tau components.
pub struct CustomCommandContractCase {
    #[serde(default = "custom_command_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub operation: String,
    #[serde(default)]
    pub command_name: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub execution_policy: Option<CustomCommandExecutionPolicy>,
    #[serde(default)]
    pub command_spec: Option<CustomCommandSpec>,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    pub expected: CustomCommandCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `CustomCommandContractFixture` used across Tau components.
pub struct CustomCommandContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<CustomCommandContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `CustomCommandContractCapabilities` used across Tau components.
pub struct CustomCommandContractCapabilities {
    pub schema_version: u32,
    pub supported_outcomes: BTreeSet<CustomCommandOutcomeKind>,
    pub supported_error_codes: BTreeSet<String>,
    pub supported_operations: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `CustomCommandReplayStep` values.
pub enum CustomCommandReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `CustomCommandReplayResult` used across Tau components.
pub struct CustomCommandReplayResult {
    pub step: CustomCommandReplayStep,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub response_body: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `CustomCommandReplaySummary` used across Tau components.
pub struct CustomCommandReplaySummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

#[cfg(test)]
/// Trait contract for `CustomCommandContractDriver` behavior.
pub trait CustomCommandContractDriver {
    fn apply_case(&mut self, case: &CustomCommandContractCase)
        -> Result<CustomCommandReplayResult>;
}

pub fn parse_custom_command_contract_fixture(raw: &str) -> Result<CustomCommandContractFixture> {
    parse_fixture_with_validation(
        raw,
        "failed to parse custom-command contract fixture",
        validate_custom_command_contract_fixture,
    )
}

pub fn load_custom_command_contract_fixture(path: &Path) -> Result<CustomCommandContractFixture> {
    load_fixture_from_path(path, parse_custom_command_contract_fixture)
}

pub fn custom_command_contract_capabilities() -> CustomCommandContractCapabilities {
    CustomCommandContractCapabilities {
        schema_version: CUSTOM_COMMAND_CONTRACT_SCHEMA_VERSION,
        supported_outcomes: [
            CustomCommandOutcomeKind::Success,
            CustomCommandOutcomeKind::MalformedInput,
            CustomCommandOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .collect(),
        supported_error_codes: supported_error_codes()
            .iter()
            .map(|code| (*code).to_string())
            .collect(),
        supported_operations: supported_operations()
            .iter()
            .map(|operation| (*operation).to_string())
            .collect(),
    }
}

pub fn validate_custom_command_contract_compatibility(
    fixture: &CustomCommandContractFixture,
) -> Result<()> {
    let capabilities = custom_command_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported custom-command contract schema version {} (expected {})",
            fixture.schema_version,
            capabilities.schema_version
        );
    }

    for case in &fixture.cases {
        if !capabilities
            .supported_outcomes
            .contains(&case.expected.outcome)
        {
            bail!(
                "fixture case '{}' uses unsupported outcome {:?}",
                case.case_id,
                case.expected.outcome
            );
        }

        let expected_code = case.expected.error_code.trim();
        if !expected_code.is_empty() && !capabilities.supported_error_codes.contains(expected_code)
        {
            bail!(
                "fixture case '{}' uses unsupported error_code '{}'",
                case.case_id,
                expected_code
            );
        }

        let operation = normalize_operation(&case.operation);
        if case.expected.outcome != CustomCommandOutcomeKind::MalformedInput
            && !capabilities.supported_operations.contains(&operation)
        {
            bail!(
                "fixture case '{}' uses unsupported operation '{}' for non-malformed outcome",
                case.case_id,
                case.operation
            );
        }
    }
    Ok(())
}

pub fn validate_custom_command_contract_fixture(
    fixture: &CustomCommandContractFixture,
) -> Result<()> {
    validate_fixture_header(
        "custom-command",
        fixture.schema_version,
        CUSTOM_COMMAND_CONTRACT_SCHEMA_VERSION,
        &fixture.name,
        fixture.cases.len(),
    )?;

    for (index, case) in fixture.cases.iter().enumerate() {
        validate_custom_command_case(case, index)?;
    }
    ensure_unique_case_ids(fixture.cases.iter().map(|case| case.case_id.as_str()))?;

    validate_custom_command_contract_compatibility(fixture)?;
    Ok(())
}

pub fn evaluate_custom_command_case(case: &CustomCommandContractCase) -> CustomCommandReplayResult {
    evaluate_custom_command_case_with_policy(case, &default_custom_command_execution_policy())
}

pub fn evaluate_custom_command_case_with_policy(
    case: &CustomCommandContractCase,
    default_policy: &CustomCommandExecutionPolicy,
) -> CustomCommandReplayResult {
    if case.simulate_retryable_failure {
        return CustomCommandReplayResult {
            step: CustomCommandReplayStep::RetryableFailure,
            status_code: 503,
            error_code: Some(CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE.to_string()),
            response_body: json!({"status":"retryable","reason":"backend_unavailable"}),
        };
    }

    let operation = normalize_operation(&case.operation);
    if !supported_operations().contains(&operation.as_str()) {
        return CustomCommandReplayResult {
            step: CustomCommandReplayStep::MalformedInput,
            status_code: 400,
            error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_OPERATION.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_operation"}),
        };
    }

    let command_name = case.command_name.trim();
    if operation != "LIST" && !is_valid_command_name(command_name) {
        return CustomCommandReplayResult {
            step: CustomCommandReplayStep::MalformedInput,
            status_code: 400,
            error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_NAME.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_name"}),
        };
    }

    if (operation == "CREATE" || operation == "UPDATE") && case.template.trim().is_empty() {
        return CustomCommandReplayResult {
            step: CustomCommandReplayStep::MalformedInput,
            status_code: 422,
            error_code: Some(CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_template"}),
        };
    }

    let effective_policy = case
        .execution_policy
        .clone()
        .unwrap_or_else(|| default_policy.clone());
    if validate_custom_command_execution_policy(&effective_policy).is_err() {
        return CustomCommandReplayResult {
            step: CustomCommandReplayStep::MalformedInput,
            status_code: 403,
            error_code: Some(CUSTOM_COMMAND_ERROR_POLICY_DENIED.to_string()),
            response_body: json!({"status":"rejected","reason":"policy_denied"}),
        };
    }

    if let Some(spec) = &case.command_spec {
        if validate_custom_command_spec(spec).is_err() {
            return CustomCommandReplayResult {
                step: CustomCommandReplayStep::MalformedInput,
                status_code: 403,
                error_code: Some(CUSTOM_COMMAND_ERROR_POLICY_DENIED.to_string()),
                response_body: json!({"status":"rejected","reason":"policy_denied"}),
            };
        }
    }

    if (operation == "CREATE" || operation == "UPDATE")
        && validate_custom_command_template_and_arguments(
            case.template.as_str(),
            &case.arguments,
            &effective_policy,
        )
        .is_err()
    {
        return CustomCommandReplayResult {
            step: CustomCommandReplayStep::MalformedInput,
            status_code: 403,
            error_code: Some(CUSTOM_COMMAND_ERROR_POLICY_DENIED.to_string()),
            response_body: json!({"status":"rejected","reason":"policy_denied"}),
        };
    }

    let status_code = if operation == "CREATE" { 201 } else { 200 };
    let response_body = if operation == "LIST" {
        json!({
            "status": "accepted",
            "operation": "list",
            "commands": ["deploy_release", "triage_alerts"]
        })
    } else if operation == "RUN" {
        json!({
            "status": "accepted",
            "operation": "run",
            "command_name": command_name,
            "arguments": case.arguments,
        })
    } else {
        json!({
            "status": "accepted",
            "operation": operation.to_ascii_lowercase(),
            "command_name": command_name,
        })
    };

    CustomCommandReplayResult {
        step: CustomCommandReplayStep::Success,
        status_code,
        error_code: None,
        response_body,
    }
}

pub fn validate_custom_command_case_result_against_contract(
    case: &CustomCommandContractCase,
    result: &CustomCommandReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        CustomCommandOutcomeKind::Success => CustomCommandReplayStep::Success,
        CustomCommandOutcomeKind::MalformedInput => CustomCommandReplayStep::MalformedInput,
        CustomCommandOutcomeKind::RetryableFailure => CustomCommandReplayStep::RetryableFailure,
    };
    if result.step != expected_step {
        bail!(
            "case '{}' expected step {:?} but observed {:?}",
            case.case_id,
            expected_step,
            result.step
        );
    }

    if result.status_code != case.expected.status_code {
        bail!(
            "case '{}' expected status_code {} but observed {}",
            case.case_id,
            case.expected.status_code,
            result.status_code
        );
    }

    match case.expected.outcome {
        CustomCommandOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
        }
        CustomCommandOutcomeKind::MalformedInput | CustomCommandOutcomeKind::RetryableFailure => {
            let expected_code = case.expected.error_code.trim();
            if result.error_code.as_deref() != Some(expected_code) {
                bail!(
                    "case '{}' expected error_code '{}' but observed {:?}",
                    case.case_id,
                    expected_code,
                    result.error_code
                );
            }
        }
    }

    if result.response_body != case.expected.response_body {
        bail!(
            "case '{}' expected response_body {} but observed {}",
            case.case_id,
            case.expected.response_body,
            result.response_body
        );
    }

    Ok(())
}

#[cfg(test)]
pub fn run_custom_command_contract_replay<D: CustomCommandContractDriver>(
    fixture: &CustomCommandContractFixture,
    driver: &mut D,
) -> Result<CustomCommandReplaySummary> {
    validate_custom_command_contract_fixture(fixture)?;

    let mut summary = CustomCommandReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..CustomCommandReplaySummary::default()
    };
    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        validate_custom_command_case_result_against_contract(case, &result)?;
        match result.step {
            CustomCommandReplayStep::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1)
            }
            CustomCommandReplayStep::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1)
            }
            CustomCommandReplayStep::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1)
            }
        }
    }
    Ok(summary)
}

fn validate_custom_command_case(case: &CustomCommandContractCase, index: usize) -> Result<()> {
    if case.schema_version != CUSTOM_COMMAND_CONTRACT_SCHEMA_VERSION {
        bail!(
            "case index {} has unsupported schema version {} (expected {})",
            index,
            case.schema_version,
            CUSTOM_COMMAND_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("case index {} has empty case_id", index);
    }
    if case.operation.trim().is_empty() {
        bail!("case '{}' has empty operation", case.case_id);
    }
    if case.expected.status_code == 0 {
        bail!("case '{}' has invalid expected.status_code=0", case.case_id);
    }

    if let Some(policy) = &case.execution_policy {
        validate_custom_command_execution_policy(policy)?;
    }
    if let Some(spec) = &case.command_spec {
        validate_custom_command_spec(spec)?;
    }

    let operation = normalize_operation(&case.operation);
    let effective_policy = case
        .execution_policy
        .clone()
        .unwrap_or_else(default_custom_command_execution_policy);
    if (operation == "CREATE" || operation == "UPDATE")
        && case.expected.outcome == CustomCommandOutcomeKind::Success
    {
        if let Some(spec) = &case.command_spec {
            if !case.command_name.trim().is_empty() && case.command_name.trim() != spec.name.trim()
            {
                bail!(
                    "case '{}' command_spec.name '{}' does not match command_name '{}'",
                    case.case_id,
                    spec.name,
                    case.command_name
                );
            }
            if !case.template.trim().is_empty() && case.template.trim() != spec.template.trim() {
                bail!(
                    "case '{}' command_spec.template does not match template",
                    case.case_id
                );
            }
        }
        if !case.template.trim().is_empty() {
            validate_custom_command_template_and_arguments(
                case.template.as_str(),
                &case.arguments,
                &effective_policy,
            )?;
        }
    }

    validate_custom_command_expectation(case)?;
    Ok(())
}

fn validate_custom_command_expectation(case: &CustomCommandContractCase) -> Result<()> {
    match case.expected.outcome {
        CustomCommandOutcomeKind::Success => {
            if !case.expected.error_code.trim().is_empty() {
                bail!(
                    "case '{}' expects success but provides error_code '{}'",
                    case.case_id,
                    case.expected.error_code
                );
            }
        }
        CustomCommandOutcomeKind::MalformedInput | CustomCommandOutcomeKind::RetryableFailure => {
            if case.expected.error_code.trim().is_empty() {
                bail!(
                    "case '{}' expects {:?} but does not provide error_code",
                    case.case_id,
                    case.expected.outcome
                );
            }
        }
    }
    Ok(())
}

fn supported_operations() -> &'static [&'static str] {
    &["CREATE", "UPDATE", "DELETE", "RUN", "LIST"]
}

fn supported_error_codes() -> &'static [&'static str] {
    &[
        CUSTOM_COMMAND_ERROR_INVALID_OPERATION,
        CUSTOM_COMMAND_ERROR_INVALID_NAME,
        CUSTOM_COMMAND_ERROR_INVALID_TEMPLATE,
        CUSTOM_COMMAND_ERROR_POLICY_DENIED,
        CUSTOM_COMMAND_ERROR_BACKEND_UNAVAILABLE,
    ]
}

fn normalize_operation(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_custom_command_case, load_custom_command_contract_fixture,
        parse_custom_command_contract_fixture, run_custom_command_contract_replay,
        CustomCommandContractCase, CustomCommandContractDriver, CustomCommandReplayResult,
        CustomCommandReplayStep, CUSTOM_COMMAND_ERROR_POLICY_DENIED,
    };
    use anyhow::Result;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("custom-command-contract")
            .join(name)
    }

    struct StaticDriver;

    impl CustomCommandContractDriver for StaticDriver {
        fn apply_case(
            &mut self,
            case: &CustomCommandContractCase,
        ) -> Result<CustomCommandReplayResult> {
            Ok(evaluate_custom_command_case(case))
        }
    }

    #[test]
    fn unit_parse_custom_command_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 9,
  "name": "custom-command-invalid-schema",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "x",
      "operation": "create",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {"status":"accepted","operation":"create","command_name":"deploy_release"}
      }
    }
  ]
}"#;
        let error = parse_custom_command_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported custom-command contract schema version"));
    }

    #[test]
    fn unit_validate_custom_command_contract_fixture_rejects_duplicate_case_id() {
        let error =
            load_custom_command_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
                .expect_err("duplicate case id should fail");
        let message = format!("{error:#}");
        assert!(
            message.contains("fixture contains duplicate case_id"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_custom_command_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        assert_eq!(fixture.cases.len(), 3);
        assert_eq!(fixture.cases[0].case_id, "custom-command-create-success");
        assert_eq!(
            fixture.cases[1].case_id,
            "custom-command-malformed-invalid-operation"
        );
        assert_eq!(
            fixture.cases[2].case_id,
            "custom-command-retryable-run-backend"
        );
    }

    #[test]
    fn integration_custom_command_contract_replay_is_deterministic_across_reloads() {
        let fixture_path = fixture_path("mixed-outcomes.json");
        let fixture_a =
            load_custom_command_contract_fixture(&fixture_path).expect("load fixture a");
        let fixture_b =
            load_custom_command_contract_fixture(&fixture_path).expect("load fixture b");

        let mut driver_a = StaticDriver;
        let mut driver_b = StaticDriver;
        let summary_a = run_custom_command_contract_replay(&fixture_a, &mut driver_a)
            .expect("replay fixture a");
        let summary_b = run_custom_command_contract_replay(&fixture_b, &mut driver_b)
            .expect("replay fixture b");
        assert_eq!(summary_a, summary_b);
        assert_eq!(summary_a.discovered_cases, 3);
        assert_eq!(summary_a.success_cases, 1);
        assert_eq!(summary_a.malformed_cases, 1);
        assert_eq!(summary_a.retryable_failures, 1);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_custom_command_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error code should fail");
        let message = format!("{error:#}");
        assert!(
            message.contains("uses unsupported error_code"),
            "unexpected error: {message}"
        );
    }

    #[test]
    fn regression_custom_command_contract_replay_rejects_mismatched_expected_response_body() {
        let mut fixture =
            load_custom_command_contract_fixture(&fixture_path("mixed-outcomes.json"))
                .expect("fixture should load");
        fixture.cases[0].expected.response_body = serde_json::json!({
            "status": "accepted",
            "operation": "create",
            "command_name": "unexpected"
        });

        let mut driver = StaticDriver;
        let error = run_custom_command_contract_replay(&fixture, &mut driver)
            .expect_err("replay should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[test]
    fn regression_custom_command_contract_evaluator_marks_unsupported_operation_as_malformed() {
        let fixture = load_custom_command_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        let result = evaluate_custom_command_case(&fixture.cases[1]);
        assert_eq!(result.step, CustomCommandReplayStep::MalformedInput);
        assert_eq!(result.status_code, 400);
        assert_eq!(
            result.error_code.as_deref(),
            Some("custom_command_invalid_operation")
        );
    }

    #[test]
    fn functional_custom_command_contract_evaluator_rejects_unsafe_template_by_default_policy() {
        let fixture = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "policy-denied-template",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-policy-denied",
      "operation": "create",
      "command_name": "deploy_release",
      "template": "deploy {{env}} && curl https://example.invalid",
      "arguments": {"env":"prod"},
      "expected": {
        "outcome": "malformed_input",
        "status_code": 403,
        "error_code": "custom_command_policy_denied",
        "response_body": {"status":"rejected","reason":"policy_denied"}
      }
    }
  ]
}"#,
        )
        .expect("fixture should parse");
        let result = evaluate_custom_command_case(&fixture.cases[0]);
        assert_eq!(result.step, CustomCommandReplayStep::MalformedInput);
        assert_eq!(result.status_code, 403);
        assert_eq!(
            result.error_code.as_deref(),
            Some(CUSTOM_COMMAND_ERROR_POLICY_DENIED)
        );
        assert_eq!(result.response_body["status"], "rejected");
        assert_eq!(result.response_body["reason"], "policy_denied");
    }

    #[test]
    fn regression_custom_command_contract_fixture_rejects_invalid_execution_policy_schema() {
        let error = parse_custom_command_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "invalid-policy",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "custom-command-invalid-policy",
      "operation": "create",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "arguments": {"env":"prod"},
      "execution_policy": {
        "schema_version": 1,
        "sandbox_profile": "forbidden-profile"
      },
      "expected": {
        "outcome": "malformed_input",
        "status_code": 403,
        "error_code": "custom_command_policy_denied",
        "response_body": {"status":"rejected","reason":"policy_denied"}
      }
    }
  ]
}"#,
        )
        .expect_err("invalid sandbox profile should fail");
        assert!(error
            .to_string()
            .contains("unsupported custom command sandbox profile"));
    }
}
