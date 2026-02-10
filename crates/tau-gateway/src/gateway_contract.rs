use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const GATEWAY_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub const GATEWAY_ERROR_INVALID_REQUEST: &str = "gateway_invalid_request";
pub const GATEWAY_ERROR_UNSUPPORTED_METHOD: &str = "gateway_unsupported_method";
pub const GATEWAY_ERROR_BACKEND_UNAVAILABLE: &str = "gateway_backend_unavailable";

fn gateway_contract_schema_version() -> u32 {
    GATEWAY_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GatewayOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayCaseExpectation {
    pub outcome: GatewayOutcomeKind,
    pub status_code: u16,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub response_body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayContractCase {
    #[serde(default = "gateway_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub method: String,
    pub endpoint: String,
    pub actor_id: String,
    #[serde(default)]
    pub body: serde_json::Value,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    pub expected: GatewayCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<GatewayContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayContractCapabilities {
    pub schema_version: u32,
    pub supported_outcomes: BTreeSet<GatewayOutcomeKind>,
    pub supported_error_codes: BTreeSet<String>,
    pub supported_methods: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayReplayResult {
    pub step: GatewayReplayStep,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub response_body: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayReplaySummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

#[cfg(test)]
pub trait GatewayContractDriver {
    fn apply_case(&mut self, case: &GatewayContractCase) -> Result<GatewayReplayResult>;
}

pub fn parse_gateway_contract_fixture(raw: &str) -> Result<GatewayContractFixture> {
    let fixture = serde_json::from_str::<GatewayContractFixture>(raw)
        .context("failed to parse gateway contract fixture")?;
    validate_gateway_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_gateway_contract_fixture(path: &Path) -> Result<GatewayContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_gateway_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn gateway_contract_capabilities() -> GatewayContractCapabilities {
    GatewayContractCapabilities {
        schema_version: GATEWAY_CONTRACT_SCHEMA_VERSION,
        supported_outcomes: [
            GatewayOutcomeKind::Success,
            GatewayOutcomeKind::MalformedInput,
            GatewayOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .collect(),
        supported_error_codes: supported_error_codes()
            .into_iter()
            .map(str::to_string)
            .collect(),
        supported_methods: supported_methods()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

pub fn validate_gateway_contract_compatibility(fixture: &GatewayContractFixture) -> Result<()> {
    let capabilities = gateway_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported gateway contract schema version {} (expected {})",
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
        let code = case.expected.error_code.trim();
        if !code.is_empty() && !capabilities.supported_error_codes.contains(code) {
            bail!(
                "fixture case '{}' uses unsupported error_code '{}'",
                case.case_id,
                code
            );
        }
        let method = normalize_method(&case.method);
        if case.expected.outcome != GatewayOutcomeKind::MalformedInput
            && !capabilities.supported_methods.contains(&method)
        {
            bail!(
                "fixture case '{}' uses unsupported method '{}' for non-malformed outcome",
                case.case_id,
                case.method
            );
        }
    }
    Ok(())
}

pub fn validate_gateway_contract_fixture(fixture: &GatewayContractFixture) -> Result<()> {
    if fixture.schema_version != GATEWAY_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported gateway contract schema version {} (expected {})",
            fixture.schema_version,
            GATEWAY_CONTRACT_SCHEMA_VERSION
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
        validate_gateway_case(case, index)?;
        let case_id = case.case_id.trim().to_string();
        if !case_ids.insert(case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", case_id);
        }
    }
    validate_gateway_contract_compatibility(fixture)?;
    Ok(())
}

pub fn evaluate_gateway_case(case: &GatewayContractCase) -> GatewayReplayResult {
    if case.simulate_retryable_failure {
        return GatewayReplayResult {
            step: GatewayReplayStep::RetryableFailure,
            status_code: 503,
            error_code: Some(GATEWAY_ERROR_BACKEND_UNAVAILABLE.to_string()),
            response_body: json!({"status":"retryable","reason":"backend_unavailable"}),
        };
    }

    let method = normalize_method(&case.method);
    if !supported_methods().contains(method.as_str()) {
        return GatewayReplayResult {
            step: GatewayReplayStep::MalformedInput,
            status_code: 405,
            error_code: Some(GATEWAY_ERROR_UNSUPPORTED_METHOD.to_string()),
            response_body: json!({"status":"rejected","reason":"unsupported_method"}),
        };
    }

    let endpoint = case.endpoint.trim();
    let actor_id = case.actor_id.trim();
    if endpoint.is_empty() || !endpoint.starts_with('/') || actor_id.is_empty() {
        return GatewayReplayResult {
            step: GatewayReplayStep::MalformedInput,
            status_code: 400,
            error_code: Some(GATEWAY_ERROR_INVALID_REQUEST.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_request"}),
        };
    }

    let status_code = match method.as_str() {
        "POST" => 201,
        "DELETE" => 202,
        _ => 200,
    };
    GatewayReplayResult {
        step: GatewayReplayStep::Success,
        status_code,
        error_code: None,
        response_body: json!({
            "status":"accepted",
            "method": method,
            "endpoint": endpoint,
            "actor_id": actor_id,
        }),
    }
}

pub fn validate_gateway_case_result_against_contract(
    case: &GatewayContractCase,
    result: &GatewayReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        GatewayOutcomeKind::Success => GatewayReplayStep::Success,
        GatewayOutcomeKind::MalformedInput => GatewayReplayStep::MalformedInput,
        GatewayOutcomeKind::RetryableFailure => GatewayReplayStep::RetryableFailure,
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
        GatewayOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
        }
        GatewayOutcomeKind::MalformedInput | GatewayOutcomeKind::RetryableFailure => {
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
pub fn run_gateway_contract_replay<D: GatewayContractDriver>(
    fixture: &GatewayContractFixture,
    driver: &mut D,
) -> Result<GatewayReplaySummary> {
    validate_gateway_contract_fixture(fixture)?;
    let mut summary = GatewayReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..GatewayReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        validate_gateway_case_result_against_contract(case, &result)?;
        match case.expected.outcome {
            GatewayOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1);
            }
            GatewayOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1);
            }
            GatewayOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn validate_gateway_case(case: &GatewayContractCase, index: usize) -> Result<()> {
    if case.schema_version != GATEWAY_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            GATEWAY_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if case.method.trim().is_empty() {
        bail!("fixture case '{}' has empty method", case.case_id);
    }
    if case.endpoint.trim().is_empty() {
        bail!("fixture case '{}' has empty endpoint", case.case_id);
    }
    if !case.body.is_object() && !case.body.is_null() {
        bail!(
            "fixture case '{}' body must be object or null",
            case.case_id
        );
    }

    if case.simulate_retryable_failure
        && case.expected.outcome != GatewayOutcomeKind::RetryableFailure
    {
        bail!(
            "fixture case '{}' sets simulate_retryable_failure=true but expected outcome is {:?}",
            case.case_id,
            case.expected.outcome
        );
    }
    if case.expected.outcome == GatewayOutcomeKind::RetryableFailure
        && !case.simulate_retryable_failure
    {
        bail!(
            "fixture case '{}' expects retryable_failure but simulate_retryable_failure=false",
            case.case_id
        );
    }

    validate_gateway_expectation(case)?;
    Ok(())
}

fn validate_gateway_expectation(case: &GatewayContractCase) -> Result<()> {
    if !case.expected.response_body.is_object() {
        bail!(
            "fixture case '{}' expected.response_body must be an object",
            case.case_id
        );
    }

    match case.expected.outcome {
        GatewayOutcomeKind::Success => {
            if !case.expected.error_code.trim().is_empty() {
                bail!(
                    "fixture case '{}' success outcome must not include error_code",
                    case.case_id
                );
            }
            if !(200..=299).contains(&case.expected.status_code) {
                bail!(
                    "fixture case '{}' success outcome requires 2xx status_code (found {})",
                    case.case_id,
                    case.expected.status_code
                );
            }
        }
        GatewayOutcomeKind::MalformedInput | GatewayOutcomeKind::RetryableFailure => {
            let code = case.expected.error_code.trim();
            if code.is_empty() {
                bail!(
                    "fixture case '{}' {:?} outcome requires error_code",
                    case.case_id,
                    case.expected.outcome
                );
            }
            if !supported_error_codes().contains(&code) {
                bail!(
                    "fixture case '{}' uses unsupported error_code '{}'",
                    case.case_id,
                    code
                );
            }
            if case.expected.status_code < 400 {
                bail!(
                    "fixture case '{}' non-success outcome requires >=400 status_code (found {})",
                    case.case_id,
                    case.expected.status_code
                );
            }
        }
    }
    Ok(())
}

fn normalize_method(raw: &str) -> String {
    raw.trim().to_ascii_uppercase()
}

fn supported_methods() -> BTreeSet<&'static str> {
    ["GET", "POST", "PUT", "PATCH", "DELETE"]
        .into_iter()
        .collect()
}

fn supported_error_codes() -> BTreeSet<&'static str> {
    [
        GATEWAY_ERROR_INVALID_REQUEST,
        GATEWAY_ERROR_UNSUPPORTED_METHOD,
        GATEWAY_ERROR_BACKEND_UNAVAILABLE,
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use serde_json::json;

    use super::{
        evaluate_gateway_case, load_gateway_contract_fixture, parse_gateway_contract_fixture,
        run_gateway_contract_replay, GatewayContractCase, GatewayContractDriver,
    };
    use super::{GatewayReplayResult, GATEWAY_ERROR_UNSUPPORTED_METHOD};

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("gateway-contract")
            .join(name)
    }

    #[derive(Default)]
    struct DeterministicGatewayDriver;

    impl GatewayContractDriver for DeterministicGatewayDriver {
        fn apply_case(&mut self, case: &GatewayContractCase) -> Result<GatewayReplayResult> {
            Ok(evaluate_gateway_case(case))
        }
    }

    #[test]
    fn unit_parse_gateway_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "gateway-invalid-schema",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "bad-schema",
      "method": "POST",
      "endpoint": "/v1/tasks",
      "actor_id": "ops",
      "body": {},
      "expected": {
        "outcome": "success",
        "status_code": 201,
        "response_body": {"status":"accepted","method":"POST","endpoint":"/v1/tasks","actor_id":"ops"}
      }
    }
  ]
}"#;
        let error = parse_gateway_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported gateway contract schema version"));
    }

    #[test]
    fn unit_validate_gateway_contract_fixture_rejects_duplicate_case_id() {
        let error = load_gateway_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
            .expect_err("duplicate case_id fixture should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("duplicate case_id"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_gateway_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load mixed outcomes fixture");
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.cases.len(), 3);
        assert_eq!(fixture.cases[0].case_id, "gateway-success");
        assert_eq!(
            fixture.cases[1].case_id,
            "gateway-malformed-unsupported-method"
        );
        assert_eq!(fixture.cases[2].case_id, "gateway-retryable-backend");
    }

    #[test]
    fn integration_gateway_contract_replay_is_deterministic_across_reloads() {
        let fixture_path = fixture_path("mixed-outcomes.json");
        let fixture_a = load_gateway_contract_fixture(&fixture_path).expect("load fixture a");
        let fixture_b = load_gateway_contract_fixture(&fixture_path).expect("load fixture b");
        let mut driver_a = DeterministicGatewayDriver;
        let mut driver_b = DeterministicGatewayDriver;

        let summary_a =
            run_gateway_contract_replay(&fixture_a, &mut driver_a).expect("replay fixture a");
        let summary_b =
            run_gateway_contract_replay(&fixture_b, &mut driver_b).expect("replay fixture b");

        assert_eq!(summary_a, summary_b);
        assert_eq!(summary_a.discovered_cases, 3);
        assert_eq!(summary_a.success_cases, 1);
        assert_eq!(summary_a.malformed_cases, 1);
        assert_eq!(summary_a.retryable_failures, 1);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_gateway_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error code should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("unsupported error_code"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn regression_gateway_contract_replay_rejects_mismatched_expected_response_body() {
        let mut fixture = load_gateway_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        fixture.cases[0].expected.response_body = json!({
            "status":"accepted",
            "method":"POST",
            "endpoint":"/v1/tasks",
            "actor_id":"unexpected"
        });
        let mut driver = DeterministicGatewayDriver;
        let error =
            run_gateway_contract_replay(&fixture, &mut driver).expect_err("replay should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[test]
    fn regression_gateway_contract_evaluator_marks_unsupported_method_as_malformed() {
        let fixture = load_gateway_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        let result = evaluate_gateway_case(&fixture.cases[1]);
        assert_eq!(
            result.error_code.as_deref(),
            Some(GATEWAY_ERROR_UNSUPPORTED_METHOD)
        );
        assert_eq!(result.status_code, 405);
    }
}
