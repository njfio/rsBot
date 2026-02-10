#![allow(dead_code)]

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub(crate) const DEPLOYMENT_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub(crate) const DEPLOYMENT_ERROR_INVALID_BLUEPRINT: &str = "deployment_invalid_blueprint";
pub(crate) const DEPLOYMENT_ERROR_UNSUPPORTED_RUNTIME: &str = "deployment_unsupported_runtime";
pub(crate) const DEPLOYMENT_ERROR_MISSING_ARTIFACT: &str = "deployment_missing_artifact";
pub(crate) const DEPLOYMENT_ERROR_BACKEND_UNAVAILABLE: &str = "deployment_backend_unavailable";

fn deployment_contract_schema_version() -> u32 {
    DEPLOYMENT_CONTRACT_SCHEMA_VERSION
}

fn default_replicas() -> u16 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeploymentOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeploymentCaseExpectation {
    pub(crate) outcome: DeploymentOutcomeKind,
    pub(crate) status_code: u16,
    #[serde(default)]
    pub(crate) error_code: String,
    #[serde(default)]
    pub(crate) response_body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeploymentContractCase {
    #[serde(default = "deployment_contract_schema_version")]
    pub(crate) schema_version: u32,
    pub(crate) case_id: String,
    pub(crate) deploy_target: String,
    pub(crate) runtime_profile: String,
    pub(crate) blueprint_id: String,
    pub(crate) environment: String,
    pub(crate) region: String,
    #[serde(default)]
    pub(crate) container_image: String,
    #[serde(default)]
    pub(crate) wasm_module: String,
    #[serde(default = "default_replicas")]
    pub(crate) replicas: u16,
    #[serde(default)]
    pub(crate) simulate_retryable_failure: bool,
    pub(crate) expected: DeploymentCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DeploymentContractFixture {
    pub(crate) schema_version: u32,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    pub(crate) cases: Vec<DeploymentContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeploymentContractCapabilities {
    pub(crate) schema_version: u32,
    pub(crate) supported_outcomes: BTreeSet<DeploymentOutcomeKind>,
    pub(crate) supported_error_codes: BTreeSet<String>,
    pub(crate) supported_targets: BTreeSet<String>,
    pub(crate) supported_runtimes: BTreeSet<String>,
    pub(crate) supported_environments: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeploymentReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeploymentReplayResult {
    pub(crate) step: DeploymentReplayStep,
    pub(crate) status_code: u16,
    pub(crate) error_code: Option<String>,
    pub(crate) response_body: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeploymentReplaySummary {
    pub(crate) discovered_cases: usize,
    pub(crate) success_cases: usize,
    pub(crate) malformed_cases: usize,
    pub(crate) retryable_failures: usize,
}

#[cfg(test)]
pub(crate) trait DeploymentContractDriver {
    fn apply_case(&mut self, case: &DeploymentContractCase) -> Result<DeploymentReplayResult>;
}

pub(crate) fn parse_deployment_contract_fixture(raw: &str) -> Result<DeploymentContractFixture> {
    let fixture = serde_json::from_str::<DeploymentContractFixture>(raw)
        .context("failed to parse deployment contract fixture")?;
    validate_deployment_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub(crate) fn load_deployment_contract_fixture(path: &Path) -> Result<DeploymentContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_deployment_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub(crate) fn deployment_contract_capabilities() -> DeploymentContractCapabilities {
    DeploymentContractCapabilities {
        schema_version: DEPLOYMENT_CONTRACT_SCHEMA_VERSION,
        supported_outcomes: [
            DeploymentOutcomeKind::Success,
            DeploymentOutcomeKind::MalformedInput,
            DeploymentOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .collect(),
        supported_error_codes: supported_error_codes()
            .into_iter()
            .map(str::to_string)
            .collect(),
        supported_targets: supported_targets()
            .into_iter()
            .map(str::to_string)
            .collect(),
        supported_runtimes: supported_runtimes()
            .into_iter()
            .map(str::to_string)
            .collect(),
        supported_environments: supported_environments()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

pub(crate) fn validate_deployment_contract_compatibility(
    fixture: &DeploymentContractFixture,
) -> Result<()> {
    let capabilities = deployment_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported deployment contract schema version {} (expected {})",
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
        if case.expected.outcome != DeploymentOutcomeKind::MalformedInput {
            let target = normalize_target(&case.deploy_target);
            let runtime = normalize_runtime(&case.runtime_profile);
            let environment = normalize_environment(&case.environment);
            if !capabilities.supported_targets.contains(&target) {
                bail!(
                    "fixture case '{}' uses unsupported deploy_target '{}'",
                    case.case_id,
                    case.deploy_target
                );
            }
            if !capabilities.supported_runtimes.contains(&runtime) {
                bail!(
                    "fixture case '{}' uses unsupported runtime_profile '{}'",
                    case.case_id,
                    case.runtime_profile
                );
            }
            if !capabilities.supported_environments.contains(&environment) {
                bail!(
                    "fixture case '{}' uses unsupported environment '{}'",
                    case.case_id,
                    case.environment
                );
            }
            if !is_runtime_supported_for_target(target.as_str(), runtime.as_str()) {
                bail!(
                    "fixture case '{}' uses unsupported runtime '{}' for deploy_target '{}'",
                    case.case_id,
                    case.runtime_profile,
                    case.deploy_target
                );
            }
            if target == "wasm" && case.wasm_module.trim().is_empty() {
                bail!(
                    "fixture case '{}' deploy_target=wasm requires wasm_module",
                    case.case_id
                );
            }
            if target != "wasm" && case.container_image.trim().is_empty() {
                bail!(
                    "fixture case '{}' deploy_target='{}' requires container_image",
                    case.case_id,
                    case.deploy_target
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_deployment_contract_fixture(
    fixture: &DeploymentContractFixture,
) -> Result<()> {
    if fixture.schema_version != DEPLOYMENT_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported deployment contract schema version {} (expected {})",
            fixture.schema_version,
            DEPLOYMENT_CONTRACT_SCHEMA_VERSION
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
        validate_deployment_case(case, index)?;
        let case_id = case.case_id.trim().to_string();
        if !case_ids.insert(case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", case_id);
        }
    }
    validate_deployment_contract_compatibility(fixture)?;
    Ok(())
}

pub(crate) fn evaluate_deployment_case(case: &DeploymentContractCase) -> DeploymentReplayResult {
    if case.simulate_retryable_failure {
        return DeploymentReplayResult {
            step: DeploymentReplayStep::RetryableFailure,
            status_code: 503,
            error_code: Some(DEPLOYMENT_ERROR_BACKEND_UNAVAILABLE.to_string()),
            response_body: json!({"status":"retryable","reason":"control_plane_unavailable"}),
        };
    }

    let target = normalize_target(&case.deploy_target);
    if !supported_targets().contains(target.as_str()) {
        return malformed_result(
            400,
            DEPLOYMENT_ERROR_INVALID_BLUEPRINT,
            "unsupported_target",
        );
    }

    let runtime = normalize_runtime(&case.runtime_profile);
    if !supported_runtimes().contains(runtime.as_str()) {
        return malformed_result(
            400,
            DEPLOYMENT_ERROR_INVALID_BLUEPRINT,
            "unsupported_runtime_profile",
        );
    }

    let environment = normalize_environment(&case.environment);
    if !supported_environments().contains(environment.as_str()) {
        return malformed_result(
            400,
            DEPLOYMENT_ERROR_INVALID_BLUEPRINT,
            "unsupported_environment",
        );
    }

    if !is_runtime_supported_for_target(target.as_str(), runtime.as_str()) {
        return malformed_result(
            422,
            DEPLOYMENT_ERROR_UNSUPPORTED_RUNTIME,
            "runtime_target_mismatch",
        );
    }

    let blueprint_id = case.blueprint_id.trim();
    if blueprint_id.is_empty() {
        return malformed_result(
            400,
            DEPLOYMENT_ERROR_INVALID_BLUEPRINT,
            "missing_blueprint_id",
        );
    }

    let region = case.region.trim();
    if region.is_empty() {
        return malformed_result(400, DEPLOYMENT_ERROR_INVALID_BLUEPRINT, "missing_region");
    }

    if case.replicas == 0 {
        return malformed_result(
            400,
            DEPLOYMENT_ERROR_INVALID_BLUEPRINT,
            "invalid_replica_count",
        );
    }

    let artifact = if target == "wasm" {
        let wasm_module = case.wasm_module.trim();
        if wasm_module.is_empty() {
            return malformed_result(
                400,
                DEPLOYMENT_ERROR_MISSING_ARTIFACT,
                "missing_wasm_module",
            );
        }
        wasm_module.to_string()
    } else {
        let container_image = case.container_image.trim();
        if container_image.is_empty() {
            return malformed_result(
                400,
                DEPLOYMENT_ERROR_MISSING_ARTIFACT,
                "missing_container_image",
            );
        }
        container_image.to_string()
    };

    let status_code = if target == "wasm" { 201 } else { 202 };
    DeploymentReplayResult {
        step: DeploymentReplayStep::Success,
        status_code,
        error_code: None,
        response_body: json!({
            "status":"accepted",
            "blueprint_id": blueprint_id,
            "deploy_target": target,
            "runtime_profile": runtime,
            "environment": environment,
            "region": region,
            "artifact": artifact,
            "replicas": case.replicas,
            "rollout_strategy": rollout_strategy_for_target(target.as_str()),
        }),
    }
}

pub(crate) fn validate_deployment_case_result_against_contract(
    case: &DeploymentContractCase,
    result: &DeploymentReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        DeploymentOutcomeKind::Success => DeploymentReplayStep::Success,
        DeploymentOutcomeKind::MalformedInput => DeploymentReplayStep::MalformedInput,
        DeploymentOutcomeKind::RetryableFailure => DeploymentReplayStep::RetryableFailure,
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
        DeploymentOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
        }
        DeploymentOutcomeKind::MalformedInput | DeploymentOutcomeKind::RetryableFailure => {
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
pub(crate) fn run_deployment_contract_replay<D: DeploymentContractDriver>(
    fixture: &DeploymentContractFixture,
    driver: &mut D,
) -> Result<DeploymentReplaySummary> {
    validate_deployment_contract_fixture(fixture)?;
    let mut summary = DeploymentReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..DeploymentReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        validate_deployment_case_result_against_contract(case, &result)?;
        match case.expected.outcome {
            DeploymentOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1);
            }
            DeploymentOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1);
            }
            DeploymentOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1);
            }
        }
    }
    Ok(summary)
}

fn validate_deployment_case(case: &DeploymentContractCase, index: usize) -> Result<()> {
    if case.schema_version != DEPLOYMENT_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            DEPLOYMENT_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if case.deploy_target.trim().is_empty() {
        bail!("fixture case '{}' has empty deploy_target", case.case_id);
    }
    if case.runtime_profile.trim().is_empty() {
        bail!("fixture case '{}' has empty runtime_profile", case.case_id);
    }
    if case.blueprint_id.trim().is_empty() {
        bail!("fixture case '{}' has empty blueprint_id", case.case_id);
    }
    if case.environment.trim().is_empty() {
        bail!("fixture case '{}' has empty environment", case.case_id);
    }
    if case.region.trim().is_empty() {
        bail!("fixture case '{}' has empty region", case.case_id);
    }
    if case.replicas == 0 {
        bail!("fixture case '{}' has replicas=0", case.case_id);
    }

    if case.simulate_retryable_failure
        && case.expected.outcome != DeploymentOutcomeKind::RetryableFailure
    {
        bail!(
            "fixture case '{}' sets simulate_retryable_failure=true but expected outcome is {:?}",
            case.case_id,
            case.expected.outcome
        );
    }
    if case.expected.outcome == DeploymentOutcomeKind::RetryableFailure
        && !case.simulate_retryable_failure
    {
        bail!(
            "fixture case '{}' expects retryable_failure but simulate_retryable_failure=false",
            case.case_id
        );
    }

    validate_deployment_expectation(case)?;
    Ok(())
}

fn validate_deployment_expectation(case: &DeploymentContractCase) -> Result<()> {
    if !case.expected.response_body.is_object() {
        bail!(
            "fixture case '{}' expected.response_body must be an object",
            case.case_id
        );
    }

    match case.expected.outcome {
        DeploymentOutcomeKind::Success => {
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
        DeploymentOutcomeKind::MalformedInput | DeploymentOutcomeKind::RetryableFailure => {
            let code = case.expected.error_code.trim();
            if code.is_empty() {
                bail!(
                    "fixture case '{}' {:?} outcome requires error_code",
                    case.case_id,
                    case.expected.outcome
                );
            }
            if !supported_error_codes().contains(code) {
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

fn normalize_target(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn normalize_runtime(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn normalize_environment(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn supported_targets() -> BTreeSet<&'static str> {
    ["container", "kubernetes", "wasm"].into_iter().collect()
}

fn supported_runtimes() -> BTreeSet<&'static str> {
    ["native", "wasm_wasi"].into_iter().collect()
}

fn supported_environments() -> BTreeSet<&'static str> {
    ["staging", "production"].into_iter().collect()
}

fn supported_error_codes() -> BTreeSet<&'static str> {
    [
        DEPLOYMENT_ERROR_INVALID_BLUEPRINT,
        DEPLOYMENT_ERROR_UNSUPPORTED_RUNTIME,
        DEPLOYMENT_ERROR_MISSING_ARTIFACT,
        DEPLOYMENT_ERROR_BACKEND_UNAVAILABLE,
    ]
    .into_iter()
    .collect()
}

fn is_runtime_supported_for_target(target: &str, runtime: &str) -> bool {
    matches!(
        (target, runtime),
        ("container", "native") | ("kubernetes", "native") | ("wasm", "wasm_wasi")
    )
}

fn rollout_strategy_for_target(target: &str) -> &'static str {
    match target {
        "kubernetes" => "rolling",
        "container" => "recreate",
        "wasm" => "canary",
        _ => "unknown",
    }
}

fn malformed_result(status_code: u16, error_code: &str, reason: &str) -> DeploymentReplayResult {
    DeploymentReplayResult {
        step: DeploymentReplayStep::MalformedInput,
        status_code,
        error_code: Some(error_code.to_string()),
        response_body: json!({"status":"rejected","reason":reason}),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use anyhow::Result;
    use serde_json::json;

    use super::{
        evaluate_deployment_case, load_deployment_contract_fixture,
        parse_deployment_contract_fixture, run_deployment_contract_replay, DeploymentContractCase,
        DeploymentContractDriver, DeploymentReplayResult, DEPLOYMENT_ERROR_UNSUPPORTED_RUNTIME,
    };

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("deployment-contract")
            .join(name)
    }

    #[derive(Default)]
    struct DeterministicDeploymentDriver;

    impl DeploymentContractDriver for DeterministicDeploymentDriver {
        fn apply_case(&mut self, case: &DeploymentContractCase) -> Result<DeploymentReplayResult> {
            Ok(evaluate_deployment_case(case))
        }
    }

    #[test]
    fn unit_parse_deployment_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "deployment-invalid-schema",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "bad-schema",
      "deploy_target": "container",
      "runtime_profile": "native",
      "blueprint_id": "staging-api",
      "environment": "staging",
      "region": "us-east-1",
      "container_image": "ghcr.io/njfio/tau:staging",
      "replicas": 2,
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status":"accepted",
          "blueprint_id":"staging-api",
          "deploy_target":"container",
          "runtime_profile":"native",
          "environment":"staging",
          "region":"us-east-1",
          "artifact":"ghcr.io/njfio/tau:staging",
          "replicas":2,
          "rollout_strategy":"recreate"
        }
      }
    }
  ]
}"#;
        let error = parse_deployment_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported deployment contract schema version"));
    }

    #[test]
    fn unit_validate_deployment_contract_fixture_rejects_duplicate_case_id() {
        let error =
            load_deployment_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
                .expect_err("duplicate case_id fixture should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("duplicate case_id"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_deployment_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load mixed outcomes fixture");
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.cases.len(), 3);
        assert_eq!(fixture.cases[0].case_id, "deployment-success-wasm");
        assert_eq!(
            fixture.cases[1].case_id,
            "deployment-malformed-runtime-mismatch"
        );
        assert_eq!(
            fixture.cases[2].case_id,
            "deployment-retryable-control-plane"
        );
    }

    #[test]
    fn integration_deployment_contract_replay_is_deterministic_across_reloads() {
        let fixture_path = fixture_path("mixed-outcomes.json");
        let fixture_a = load_deployment_contract_fixture(&fixture_path).expect("load fixture a");
        let fixture_b = load_deployment_contract_fixture(&fixture_path).expect("load fixture b");
        let mut driver_a = DeterministicDeploymentDriver;
        let mut driver_b = DeterministicDeploymentDriver;

        let summary_a =
            run_deployment_contract_replay(&fixture_a, &mut driver_a).expect("replay fixture a");
        let summary_b =
            run_deployment_contract_replay(&fixture_b, &mut driver_b).expect("replay fixture b");

        assert_eq!(summary_a, summary_b);
        assert_eq!(summary_a.discovered_cases, 3);
        assert_eq!(summary_a.success_cases, 1);
        assert_eq!(summary_a.malformed_cases, 1);
        assert_eq!(summary_a.retryable_failures, 1);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_deployment_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error code should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("unsupported error_code"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn regression_deployment_contract_replay_rejects_mismatched_expected_response_body() {
        let mut fixture = load_deployment_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        fixture.cases[0].expected.response_body = json!({
            "status":"accepted",
            "blueprint_id":"edge-wasm",
            "deploy_target":"wasm",
            "runtime_profile":"wasm_wasi",
            "environment":"staging",
            "region":"iad",
            "artifact":"edge/runtime-v2.wasm",
            "replicas": 2,
            "rollout_strategy":"canary"
        });
        let mut driver = DeterministicDeploymentDriver;
        let error =
            run_deployment_contract_replay(&fixture, &mut driver).expect_err("replay should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[test]
    fn regression_deployment_contract_evaluator_marks_runtime_mismatch_as_malformed() {
        let fixture = load_deployment_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        let result = evaluate_deployment_case(&fixture.cases[1]);
        assert_eq!(
            result.error_code.as_deref(),
            Some(DEPLOYMENT_ERROR_UNSUPPORTED_RUNTIME)
        );
        assert_eq!(result.status_code, 422);
    }
}
