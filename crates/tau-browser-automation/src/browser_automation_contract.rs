#![allow(dead_code)]

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub const BROWSER_AUTOMATION_ERROR_INVALID_OPERATION: &str = "browser_automation_invalid_operation";
pub const BROWSER_AUTOMATION_ERROR_INVALID_ACTION: &str = "browser_automation_invalid_action";
pub const BROWSER_AUTOMATION_ERROR_INVALID_URL: &str = "browser_automation_invalid_url";
pub const BROWSER_AUTOMATION_ERROR_INVALID_SELECTOR: &str = "browser_automation_invalid_selector";
pub const BROWSER_AUTOMATION_ERROR_INVALID_INPUT: &str = "browser_automation_invalid_input";
pub const BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION: &str = "browser_automation_unsafe_operation";
pub const BROWSER_AUTOMATION_ERROR_TIMEOUT: &str = "browser_automation_timeout";
pub const BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE: &str =
    "browser_automation_backend_unavailable";
pub const BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED: &str =
    "browser_automation_action_limit_exceeded";

fn browser_automation_contract_schema_version() -> u32 {
    BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION
}

fn default_action_repeat_count() -> usize {
    1
}

fn default_timeout_ms() -> u64 {
    5_000
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BrowserAutomationOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserAutomationCaseExpectation {
    pub outcome: BrowserAutomationOutcomeKind,
    pub status_code: u16,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub response_body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserAutomationContractCase {
    #[serde(default = "browser_automation_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub operation: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub selector: String,
    #[serde(default)]
    pub text: String,
    #[serde(default = "default_action_repeat_count")]
    pub action_repeat_count: usize,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub unsafe_operation: bool,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    #[serde(default)]
    pub simulate_timeout: bool,
    pub expected: BrowserAutomationCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserAutomationContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<BrowserAutomationContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAutomationContractCapabilities {
    pub schema_version: u32,
    pub supported_outcomes: BTreeSet<BrowserAutomationOutcomeKind>,
    pub supported_error_codes: BTreeSet<String>,
    pub supported_operations: BTreeSet<String>,
    pub supported_actions: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserAutomationReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAutomationReplayResult {
    pub step: BrowserAutomationReplayStep,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub response_body: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowserAutomationReplaySummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

#[cfg(test)]
pub trait BrowserAutomationContractDriver {
    fn apply_case(
        &mut self,
        case: &BrowserAutomationContractCase,
    ) -> Result<BrowserAutomationReplayResult>;
}

pub fn parse_browser_automation_contract_fixture(
    raw: &str,
) -> Result<BrowserAutomationContractFixture> {
    let fixture = serde_json::from_str::<BrowserAutomationContractFixture>(raw)
        .context("failed to parse browser automation contract fixture")?;
    validate_browser_automation_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_browser_automation_contract_fixture(
    path: &Path,
) -> Result<BrowserAutomationContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_browser_automation_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn browser_automation_contract_capabilities() -> BrowserAutomationContractCapabilities {
    BrowserAutomationContractCapabilities {
        schema_version: BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION,
        supported_outcomes: [
            BrowserAutomationOutcomeKind::Success,
            BrowserAutomationOutcomeKind::MalformedInput,
            BrowserAutomationOutcomeKind::RetryableFailure,
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
        supported_actions: supported_actions()
            .iter()
            .map(|action| (*action).to_string())
            .collect(),
    }
}

pub fn validate_browser_automation_contract_compatibility(
    fixture: &BrowserAutomationContractFixture,
) -> Result<()> {
    let capabilities = browser_automation_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported browser automation contract schema version {} (expected {})",
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
        if case.expected.outcome != BrowserAutomationOutcomeKind::MalformedInput
            && !capabilities.supported_operations.contains(&operation)
        {
            bail!(
                "fixture case '{}' uses unsupported operation '{}' for non-malformed outcome",
                case.case_id,
                case.operation
            );
        }

        if operation == "action"
            && case.expected.outcome != BrowserAutomationOutcomeKind::MalformedInput
        {
            let action = normalize_action(&case.action);
            if !capabilities.supported_actions.contains(&action) {
                bail!(
                    "fixture case '{}' uses unsupported action '{}' for non-malformed outcome",
                    case.case_id,
                    case.action
                );
            }
        }
    }
    Ok(())
}

pub fn validate_browser_automation_contract_fixture(
    fixture: &BrowserAutomationContractFixture,
) -> Result<()> {
    if fixture.schema_version != BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported browser automation contract schema version {} (expected {})",
            fixture.schema_version,
            BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION
        );
    }
    if fixture.name.trim().is_empty() {
        bail!("fixture name cannot be empty");
    }
    if fixture.cases.is_empty() {
        bail!("fixture must include at least one case");
    }

    let mut case_ids = HashSet::new();
    for (index, case) in fixture.cases.iter().enumerate() {
        validate_browser_automation_case(case, index)?;
        let case_id = case.case_id.trim().to_string();
        if !case_ids.insert(case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", case_id);
        }
    }

    validate_browser_automation_contract_compatibility(fixture)?;
    Ok(())
}

pub fn evaluate_browser_automation_case(
    case: &BrowserAutomationContractCase,
) -> BrowserAutomationReplayResult {
    if case.simulate_retryable_failure {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::RetryableFailure,
            status_code: 503,
            error_code: Some(BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE.to_string()),
            response_body: json!({"status":"retryable","reason":"backend_unavailable"}),
        };
    }

    if case.simulate_timeout {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::RetryableFailure,
            status_code: 504,
            error_code: Some(BROWSER_AUTOMATION_ERROR_TIMEOUT.to_string()),
            response_body: json!({"status":"retryable","reason":"timeout"}),
        };
    }

    let operation = normalize_operation(&case.operation);
    if !supported_operations().contains(&operation.as_str()) {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 400,
            error_code: Some(BROWSER_AUTOMATION_ERROR_INVALID_OPERATION.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_operation"}),
        };
    }

    if case.unsafe_operation {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 403,
            error_code: Some(BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION.to_string()),
            response_body: json!({"status":"rejected","reason":"unsafe_operation"}),
        };
    }

    if operation == "navigate" {
        let url = case.url.trim();
        if !is_valid_url(url) {
            return BrowserAutomationReplayResult {
                step: BrowserAutomationReplayStep::MalformedInput,
                status_code: 400,
                error_code: Some(BROWSER_AUTOMATION_ERROR_INVALID_URL.to_string()),
                response_body: json!({"status":"rejected","reason":"invalid_url"}),
            };
        }

        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::Success,
            status_code: 200,
            error_code: None,
            response_body: json!({
                "status": "ok",
                "operation": "navigate",
                "url": url,
                "title": format!("Fixture page for {}", case.case_id.trim()),
                "dom_nodes": 96,
            }),
        };
    }

    if operation == "snapshot" {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::Success,
            status_code: 200,
            error_code: None,
            response_body: json!({
                "status": "ok",
                "operation": "snapshot",
                "snapshot_id": format!("snapshot-{}", case.case_id.trim()),
                "elements": [
                    {"id":"e1","role":"link","name":"Docs"},
                    {"id":"e2","role":"button","name":"Submit"}
                ],
            }),
        };
    }

    let action = normalize_action(&case.action);
    if !supported_actions().contains(&action.as_str()) {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 400,
            error_code: Some(BROWSER_AUTOMATION_ERROR_INVALID_ACTION.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_action"}),
        };
    }

    if case.selector.trim().is_empty() {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 422,
            error_code: Some(BROWSER_AUTOMATION_ERROR_INVALID_SELECTOR.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_selector"}),
        };
    }

    if action == "type" && case.text.trim().is_empty() {
        return BrowserAutomationReplayResult {
            step: BrowserAutomationReplayStep::MalformedInput,
            status_code: 422,
            error_code: Some(BROWSER_AUTOMATION_ERROR_INVALID_INPUT.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_input"}),
        };
    }

    BrowserAutomationReplayResult {
        step: BrowserAutomationReplayStep::Success,
        status_code: 200,
        error_code: None,
        response_body: json!({
            "status": "ok",
            "operation": "action",
            "action": action,
            "selector": case.selector.trim(),
            "repeat_count": case.action_repeat_count,
            "text": case.text.trim(),
            "timeout_ms": case.timeout_ms,
        }),
    }
}

pub fn assert_browser_automation_result_matches_expectation(
    case: &BrowserAutomationContractCase,
    result: &BrowserAutomationReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        BrowserAutomationOutcomeKind::Success => BrowserAutomationReplayStep::Success,
        BrowserAutomationOutcomeKind::MalformedInput => BrowserAutomationReplayStep::MalformedInput,
        BrowserAutomationOutcomeKind::RetryableFailure => {
            BrowserAutomationReplayStep::RetryableFailure
        }
    };
    if result.step != expected_step {
        bail!(
            "case '{}' expected outcome {:?} but runtime returned {:?}",
            case.case_id,
            case.expected.outcome,
            result.step
        );
    }
    if result.status_code != case.expected.status_code {
        bail!(
            "case '{}' expected status_code {} but runtime returned {}",
            case.case_id,
            case.expected.status_code,
            result.status_code
        );
    }

    let expected_code = case.expected.error_code.trim();
    let actual_code = result.error_code.as_deref().unwrap_or_default().trim();
    if expected_code != actual_code {
        bail!(
            "case '{}' expected error_code '{}' but runtime returned '{}'",
            case.case_id,
            expected_code,
            actual_code
        );
    }

    if !case.expected.response_body.is_null() && result.response_body != case.expected.response_body
    {
        bail!(
            "case '{}' expected response_body {} but runtime returned {}",
            case.case_id,
            case.expected.response_body,
            result.response_body
        );
    }

    Ok(())
}

#[cfg(test)]
pub fn run_browser_automation_contract_replay<D: BrowserAutomationContractDriver>(
    fixture: &BrowserAutomationContractFixture,
    driver: &mut D,
) -> Result<BrowserAutomationReplaySummary> {
    validate_browser_automation_contract_fixture(fixture)?;
    let mut summary = BrowserAutomationReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..BrowserAutomationReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        assert_browser_automation_result_matches_expectation(case, &result)?;
        match case.expected.outcome {
            BrowserAutomationOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1)
            }
            BrowserAutomationOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1)
            }
            BrowserAutomationOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1)
            }
        }
    }

    Ok(summary)
}

fn validate_browser_automation_case(
    case: &BrowserAutomationContractCase,
    index: usize,
) -> Result<()> {
    if case.schema_version != BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            BROWSER_AUTOMATION_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if case.action_repeat_count == 0 {
        bail!(
            "fixture case '{}' has action_repeat_count 0; expected at least 1",
            case.case_id
        );
    }
    if case.timeout_ms == 0 {
        bail!(
            "fixture case '{}' has timeout_ms 0; expected at least 1",
            case.case_id
        );
    }

    Ok(())
}

fn supported_operations() -> [&'static str; 3] {
    ["navigate", "snapshot", "action"]
}

fn supported_actions() -> [&'static str; 3] {
    ["click", "type", "wait"]
}

fn supported_error_codes() -> [&'static str; 9] {
    [
        BROWSER_AUTOMATION_ERROR_INVALID_OPERATION,
        BROWSER_AUTOMATION_ERROR_INVALID_ACTION,
        BROWSER_AUTOMATION_ERROR_INVALID_URL,
        BROWSER_AUTOMATION_ERROR_INVALID_SELECTOR,
        BROWSER_AUTOMATION_ERROR_INVALID_INPUT,
        BROWSER_AUTOMATION_ERROR_UNSAFE_OPERATION,
        BROWSER_AUTOMATION_ERROR_TIMEOUT,
        BROWSER_AUTOMATION_ERROR_BACKEND_UNAVAILABLE,
        BROWSER_AUTOMATION_ERROR_ACTION_LIMIT_EXCEEDED,
    ]
}

fn normalize_operation(operation: &str) -> String {
    operation.trim().to_ascii_lowercase()
}

fn normalize_action(action: &str) -> String {
    action.trim().to_ascii_lowercase()
}

fn is_valid_url(url: &str) -> bool {
    let trimmed = url.trim();
    !trimmed.is_empty() && (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_browser_automation_case, parse_browser_automation_contract_fixture,
        validate_browser_automation_contract_fixture, BrowserAutomationOutcomeKind,
        BROWSER_AUTOMATION_ERROR_INVALID_URL,
    };

    #[test]
    fn unit_validate_browser_automation_fixture_rejects_duplicate_case_ids() {
        let fixture = parse_browser_automation_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "duplicate-case",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "duplicate-id",
      "operation": "snapshot",
      "expected": {"outcome": "success", "status_code": 200, "response_body": {"status":"ok"}}
    },
    {
      "schema_version": 1,
      "case_id": "duplicate-id",
      "operation": "snapshot",
      "expected": {"outcome": "success", "status_code": 200, "response_body": {"status":"ok"}}
    }
  ]
}"#,
        )
        .expect_err("duplicate ids should fail");
        assert!(fixture.to_string().contains("duplicate case_id"));
    }

    #[test]
    fn functional_evaluate_browser_automation_case_navigate_success_returns_structured_payload() {
        let fixture = parse_browser_automation_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "navigate-success",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "navigate-home",
      "operation": "navigate",
      "url": "https://example.com",
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "navigate",
          "url": "https://example.com",
          "title": "Fixture page for navigate-home",
          "dom_nodes": 96
        }
      }
    }
  ]
}"#,
        )
        .expect("fixture should parse");

        let case = fixture.cases.first().expect("one case");
        let result = evaluate_browser_automation_case(case);
        assert_eq!(result.status_code, 200);
        assert_eq!(result.error_code, None);
        assert_eq!(
            result
                .response_body
                .get("operation")
                .and_then(serde_json::Value::as_str),
            Some("navigate")
        );
    }

    #[test]
    fn integration_parse_browser_automation_fixture_accepts_supported_action_case() {
        let fixture = parse_browser_automation_contract_fixture(
            r##"{
  "schema_version": 1,
  "name": "action-type",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "action-type",
      "operation": "action",
      "action": "type",
      "selector": "#search",
      "text": "tau",
      "action_repeat_count": 1,
      "timeout_ms": 1500,
      "expected": {
        "outcome": "success",
        "status_code": 200,
        "response_body": {
          "status": "ok",
          "operation": "action",
          "action": "type",
          "selector": "#search",
          "repeat_count": 1,
          "text": "tau",
          "timeout_ms": 1500
        }
      }
    }
  ]
}"##,
        )
        .expect("fixture should parse");
        assert_eq!(fixture.cases.len(), 1);
        assert_eq!(
            fixture.cases[0].expected.outcome,
            BrowserAutomationOutcomeKind::Success
        );
    }

    #[test]
    fn regression_evaluate_browser_automation_case_rejects_invalid_url() {
        let fixture = parse_browser_automation_contract_fixture(
            r#"{
  "schema_version": 1,
  "name": "invalid-url",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "navigate-invalid-url",
      "operation": "navigate",
      "url": "file:///etc/passwd",
      "expected": {
        "outcome": "malformed_input",
        "status_code": 400,
        "error_code": "browser_automation_invalid_url",
        "response_body": {"status":"rejected","reason":"invalid_url"}
      }
    }
  ]
}"#,
        )
        .expect("fixture should parse");

        validate_browser_automation_contract_fixture(&fixture).expect("fixture should validate");
        let case = fixture.cases.first().expect("one case");
        let result = evaluate_browser_automation_case(case);
        assert_eq!(
            result.error_code.as_deref(),
            Some(BROWSER_AUTOMATION_ERROR_INVALID_URL)
        );
        assert_eq!(result.status_code, 400);
    }
}
