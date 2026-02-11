use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::multi_agent_router::{
    parse_multi_agent_route_table, select_multi_agent_route, MultiAgentRoutePhase,
};

pub const MULTI_AGENT_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub const MULTI_AGENT_ERROR_INVALID_ROUTE_TABLE: &str = "multi_agent_invalid_route_table";
pub const MULTI_AGENT_ERROR_EMPTY_STEP_TEXT: &str = "multi_agent_empty_step_text";
pub const MULTI_AGENT_ERROR_ROLE_UNAVAILABLE: &str = "multi_agent_role_unavailable";

fn multi_agent_contract_schema_version() -> u32 {
    MULTI_AGENT_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MultiAgentOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

impl MultiAgentOutcomeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::MalformedInput => "malformed_input",
            Self::RetryableFailure => "retryable_failure",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiAgentContractExpectation {
    pub outcome: MultiAgentOutcomeKind,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub selected_role: String,
    #[serde(default)]
    pub attempted_roles: Vec<String>,
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiAgentContractCase {
    #[serde(default = "multi_agent_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub phase: MultiAgentRoutePhase,
    pub route_table: Value,
    #[serde(default)]
    pub step_text: String,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    pub expected: MultiAgentContractExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MultiAgentContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<MultiAgentContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiAgentContractCapabilities {
    pub schema_version: u32,
    pub supported_phases: Vec<String>,
    pub supported_outcomes: Vec<String>,
    pub supported_error_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiAgentReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiAgentReplayResult {
    pub step: MultiAgentReplayStep,
    pub error_code: Option<String>,
    pub selected_role: String,
    pub attempted_roles: Vec<String>,
    pub category: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MultiAgentReplaySummary {
    pub(crate) discovered_cases: usize,
    pub(crate) success_cases: usize,
    pub(crate) malformed_cases: usize,
    pub(crate) retryable_failures: usize,
}

#[cfg(test)]
pub(crate) trait MultiAgentContractDriver {
    fn apply_case(&mut self, case: &MultiAgentContractCase) -> Result<MultiAgentReplayResult>;
}

pub fn parse_multi_agent_contract_fixture(raw: &str) -> Result<MultiAgentContractFixture> {
    let fixture = serde_json::from_str::<MultiAgentContractFixture>(raw)
        .context("failed to parse multi-agent contract fixture")?;
    validate_multi_agent_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_multi_agent_contract_fixture(path: &Path) -> Result<MultiAgentContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_multi_agent_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn multi_agent_contract_capabilities() -> MultiAgentContractCapabilities {
    MultiAgentContractCapabilities {
        schema_version: MULTI_AGENT_CONTRACT_SCHEMA_VERSION,
        supported_phases: [
            MultiAgentRoutePhase::Planner,
            MultiAgentRoutePhase::DelegatedStep,
            MultiAgentRoutePhase::Review,
        ]
        .into_iter()
        .map(|phase| phase.as_str().to_string())
        .collect(),
        supported_outcomes: [
            MultiAgentOutcomeKind::Success,
            MultiAgentOutcomeKind::MalformedInput,
            MultiAgentOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .map(|outcome| outcome.as_str().to_string())
        .collect(),
        supported_error_codes: supported_error_codes()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

pub fn validate_multi_agent_contract_compatibility(
    fixture: &MultiAgentContractFixture,
) -> Result<()> {
    let capabilities = multi_agent_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported multi-agent contract schema version {} (expected {})",
            fixture.schema_version,
            capabilities.schema_version
        );
    }
    for case in &fixture.cases {
        let phase = case.phase.as_str();
        if !capabilities
            .supported_phases
            .iter()
            .any(|supported| supported.as_str() == phase)
        {
            bail!(
                "fixture case '{}' uses unsupported phase '{}'",
                case.case_id,
                phase
            );
        }
        let outcome = case.expected.outcome.as_str();
        if !capabilities
            .supported_outcomes
            .iter()
            .any(|supported| supported.as_str() == outcome)
        {
            bail!(
                "fixture case '{}' uses unsupported outcome '{}'",
                case.case_id,
                outcome
            );
        }
        let error_code = case.expected.error_code.trim();
        if !error_code.is_empty()
            && !capabilities
                .supported_error_codes
                .iter()
                .any(|supported| supported.as_str() == error_code)
        {
            bail!(
                "fixture case '{}' uses unsupported error_code '{}'",
                case.case_id,
                error_code
            );
        }
    }
    Ok(())
}

pub fn validate_multi_agent_contract_fixture(fixture: &MultiAgentContractFixture) -> Result<()> {
    if fixture.schema_version != MULTI_AGENT_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported multi-agent contract schema version {} (expected {})",
            fixture.schema_version,
            MULTI_AGENT_CONTRACT_SCHEMA_VERSION
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
        validate_multi_agent_contract_case(case, index)?;
        let trimmed_case_id = case.case_id.trim().to_string();
        if !case_ids.insert(trimmed_case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", trimmed_case_id);
        }
    }

    validate_multi_agent_contract_compatibility(fixture)?;
    Ok(())
}

fn validate_multi_agent_contract_case(case: &MultiAgentContractCase, index: usize) -> Result<()> {
    if case.schema_version != MULTI_AGENT_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            MULTI_AGENT_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if !case.route_table.is_object() {
        bail!(
            "fixture case '{}' route_table must be a JSON object",
            case.case_id
        );
    }
    if case.simulate_retryable_failure
        && case.expected.outcome != MultiAgentOutcomeKind::RetryableFailure
    {
        bail!(
            "fixture case '{}' sets simulate_retryable_failure=true but expected outcome is {}",
            case.case_id,
            case.expected.outcome.as_str()
        );
    }
    if case.expected.outcome == MultiAgentOutcomeKind::RetryableFailure
        && !case.simulate_retryable_failure
    {
        bail!(
            "fixture case '{}' expects retryable_failure but simulate_retryable_failure=false",
            case.case_id
        );
    }
    if case.phase == MultiAgentRoutePhase::DelegatedStep
        && case.expected.outcome == MultiAgentOutcomeKind::Success
        && case.step_text.trim().is_empty()
    {
        bail!(
            "fixture case '{}' delegated_step success requires non-empty step_text",
            case.case_id
        );
    }

    validate_multi_agent_expectation(case)?;
    validate_case_route_table_contract(case)?;
    Ok(())
}

fn validate_multi_agent_expectation(case: &MultiAgentContractCase) -> Result<()> {
    let error_code = case.expected.error_code.trim();
    match case.expected.outcome {
        MultiAgentOutcomeKind::Success => {
            if !error_code.is_empty() {
                bail!(
                    "fixture case '{}' success outcome must not include error_code",
                    case.case_id
                );
            }
            if case.expected.selected_role.trim().is_empty() {
                bail!(
                    "fixture case '{}' success outcome requires expected.selected_role",
                    case.case_id
                );
            }
            if case.expected.attempted_roles.is_empty() {
                bail!(
                    "fixture case '{}' success outcome requires expected.attempted_roles",
                    case.case_id
                );
            }
            let mut seen_roles = HashSet::new();
            for role in &case.expected.attempted_roles {
                let trimmed_role = role.trim();
                if trimmed_role.is_empty() {
                    bail!(
                        "fixture case '{}' success outcome includes empty attempted role",
                        case.case_id
                    );
                }
                if !seen_roles.insert(trimmed_role.to_string()) {
                    bail!(
                        "fixture case '{}' success outcome includes duplicate attempted role '{}'",
                        case.case_id,
                        trimmed_role
                    );
                }
            }
            if case.expected.selected_role.trim() != case.expected.attempted_roles[0].trim() {
                bail!(
                    "fixture case '{}' selected_role must match first attempted_roles entry",
                    case.case_id
                );
            }
        }
        MultiAgentOutcomeKind::MalformedInput | MultiAgentOutcomeKind::RetryableFailure => {
            if error_code.is_empty() {
                bail!(
                    "fixture case '{}' {} outcome requires error_code",
                    case.case_id,
                    case.expected.outcome.as_str()
                );
            }
            if !supported_error_codes().contains(&error_code) {
                bail!(
                    "fixture case '{}' uses unsupported error_code '{}'",
                    case.case_id,
                    error_code
                );
            }
            if !case.expected.selected_role.trim().is_empty() {
                bail!(
                    "fixture case '{}' non-success outcome must not include selected_role",
                    case.case_id
                );
            }
            if !case.expected.attempted_roles.is_empty() {
                bail!(
                    "fixture case '{}' non-success outcome must not include attempted_roles",
                    case.case_id
                );
            }
            if !case.expected.category.trim().is_empty() {
                bail!(
                    "fixture case '{}' non-success outcome must not include category",
                    case.case_id
                );
            }
        }
    }
    Ok(())
}

fn validate_case_route_table_contract(case: &MultiAgentContractCase) -> Result<()> {
    let route_table_raw = serde_json::to_string(&case.route_table).with_context(|| {
        format!(
            "failed to serialize route_table for case '{}'",
            case.case_id
        )
    })?;
    let parsed = parse_multi_agent_route_table(&route_table_raw);
    match case.expected.outcome {
        MultiAgentOutcomeKind::MalformedInput => {
            if parsed.is_ok()
                && case.phase == MultiAgentRoutePhase::DelegatedStep
                && case.step_text.trim().is_empty()
                && case.expected.error_code.trim() != MULTI_AGENT_ERROR_EMPTY_STEP_TEXT
            {
                bail!(
                    "fixture case '{}' delegated malformed_input with empty step_text must use error_code '{}'",
                    case.case_id,
                    MULTI_AGENT_ERROR_EMPTY_STEP_TEXT
                );
            }
        }
        MultiAgentOutcomeKind::Success | MultiAgentOutcomeKind::RetryableFailure => {
            let table = parsed.with_context(|| {
                format!(
                    "fixture case '{}' requires a valid route_table for {} outcome",
                    case.case_id,
                    case.expected.outcome.as_str()
                )
            })?;
            if case.expected.outcome == MultiAgentOutcomeKind::Success {
                let step_text = if case.phase == MultiAgentRoutePhase::DelegatedStep {
                    Some(case.step_text.as_str())
                } else {
                    None
                };
                let selection = select_multi_agent_route(&table, case.phase, step_text);
                if selection.primary_role != case.expected.selected_role {
                    bail!(
                        "fixture case '{}' expected.selected_role '{}' does not match selected role '{}'",
                        case.case_id,
                        case.expected.selected_role,
                        selection.primary_role
                    );
                }
                if selection.attempt_roles != case.expected.attempted_roles {
                    bail!(
                        "fixture case '{}' expected.attempted_roles {:?} do not match {:?}",
                        case.case_id,
                        case.expected.attempted_roles,
                        selection.attempt_roles
                    );
                }
                let expected_category = case.expected.category.trim();
                let observed_category = selection.category.as_deref().unwrap_or_default();
                if expected_category != observed_category {
                    bail!(
                        "fixture case '{}' expected.category '{}' does not match '{}'",
                        case.case_id,
                        expected_category,
                        observed_category
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn evaluate_multi_agent_case(case: &MultiAgentContractCase) -> MultiAgentReplayResult {
    let route_table_raw = match serde_json::to_string(&case.route_table) {
        Ok(raw) => raw,
        Err(_) => {
            return MultiAgentReplayResult {
                step: MultiAgentReplayStep::MalformedInput,
                error_code: Some(MULTI_AGENT_ERROR_INVALID_ROUTE_TABLE.to_string()),
                selected_role: String::new(),
                attempted_roles: Vec::new(),
                category: String::new(),
            };
        }
    };

    let table = match parse_multi_agent_route_table(&route_table_raw) {
        Ok(parsed) => parsed,
        Err(_) => {
            return MultiAgentReplayResult {
                step: MultiAgentReplayStep::MalformedInput,
                error_code: Some(MULTI_AGENT_ERROR_INVALID_ROUTE_TABLE.to_string()),
                selected_role: String::new(),
                attempted_roles: Vec::new(),
                category: String::new(),
            };
        }
    };

    if case.simulate_retryable_failure {
        return MultiAgentReplayResult {
            step: MultiAgentReplayStep::RetryableFailure,
            error_code: Some(MULTI_AGENT_ERROR_ROLE_UNAVAILABLE.to_string()),
            selected_role: String::new(),
            attempted_roles: Vec::new(),
            category: String::new(),
        };
    }

    if case.phase == MultiAgentRoutePhase::DelegatedStep && case.step_text.trim().is_empty() {
        return MultiAgentReplayResult {
            step: MultiAgentReplayStep::MalformedInput,
            error_code: Some(MULTI_AGENT_ERROR_EMPTY_STEP_TEXT.to_string()),
            selected_role: String::new(),
            attempted_roles: Vec::new(),
            category: String::new(),
        };
    }

    let step_text = if case.phase == MultiAgentRoutePhase::DelegatedStep {
        Some(case.step_text.as_str())
    } else {
        None
    };
    let selection = select_multi_agent_route(&table, case.phase, step_text);
    MultiAgentReplayResult {
        step: MultiAgentReplayStep::Success,
        error_code: None,
        selected_role: selection.primary_role,
        attempted_roles: selection.attempt_roles,
        category: selection.category.unwrap_or_default(),
    }
}

pub fn validate_multi_agent_case_result_against_contract(
    case: &MultiAgentContractCase,
    result: &MultiAgentReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        MultiAgentOutcomeKind::Success => MultiAgentReplayStep::Success,
        MultiAgentOutcomeKind::MalformedInput => MultiAgentReplayStep::MalformedInput,
        MultiAgentOutcomeKind::RetryableFailure => MultiAgentReplayStep::RetryableFailure,
    };
    if result.step != expected_step {
        bail!(
            "case '{}' expected step {:?} but observed {:?}",
            case.case_id,
            expected_step,
            result.step
        );
    }

    match case.expected.outcome {
        MultiAgentOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
            if result.selected_role != case.expected.selected_role {
                bail!(
                    "case '{}' expected selected_role '{}' but observed '{}'",
                    case.case_id,
                    case.expected.selected_role,
                    result.selected_role
                );
            }
            if result.attempted_roles != case.expected.attempted_roles {
                bail!(
                    "case '{}' expected attempted_roles {:?} but observed {:?}",
                    case.case_id,
                    case.expected.attempted_roles,
                    result.attempted_roles
                );
            }
            if result.category != case.expected.category {
                bail!(
                    "case '{}' expected category '{}' but observed '{}'",
                    case.case_id,
                    case.expected.category,
                    result.category
                );
            }
        }
        MultiAgentOutcomeKind::MalformedInput | MultiAgentOutcomeKind::RetryableFailure => {
            let expected_code = case.expected.error_code.trim();
            if result.error_code.as_deref() != Some(expected_code) {
                bail!(
                    "case '{}' expected error_code '{}' but observed {:?}",
                    case.case_id,
                    expected_code,
                    result.error_code
                );
            }
            if !result.selected_role.is_empty() {
                bail!(
                    "case '{}' expected empty selected_role for non-success outcome but observed '{}'",
                    case.case_id,
                    result.selected_role
                );
            }
            if !result.attempted_roles.is_empty() {
                bail!(
                    "case '{}' expected empty attempted_roles for non-success outcome but observed {:?}",
                    case.case_id,
                    result.attempted_roles
                );
            }
            if !result.category.is_empty() {
                bail!(
                    "case '{}' expected empty category for non-success outcome but observed '{}'",
                    case.case_id,
                    result.category
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
pub(crate) fn run_multi_agent_contract_replay<D: MultiAgentContractDriver>(
    fixture: &MultiAgentContractFixture,
    driver: &mut D,
) -> Result<MultiAgentReplaySummary> {
    validate_multi_agent_contract_fixture(fixture)?;
    let mut summary = MultiAgentReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..MultiAgentReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        validate_multi_agent_case_result_against_contract(case, &result)?;
        match case.expected.outcome {
            MultiAgentOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1);
            }
            MultiAgentOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1);
            }
            MultiAgentOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1);
            }
        }
    }

    Ok(summary)
}

fn supported_error_codes() -> [&'static str; 3] {
    [
        MULTI_AGENT_ERROR_INVALID_ROUTE_TABLE,
        MULTI_AGENT_ERROR_EMPTY_STEP_TEXT,
        MULTI_AGENT_ERROR_ROLE_UNAVAILABLE,
    ]
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        evaluate_multi_agent_case, load_multi_agent_contract_fixture,
        parse_multi_agent_contract_fixture, run_multi_agent_contract_replay,
        MultiAgentContractCase, MultiAgentContractDriver, MultiAgentReplayResult,
    };

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("multi-agent-contract")
            .join(name)
    }

    #[derive(Default)]
    struct DeterministicMultiAgentDriver;

    impl MultiAgentContractDriver for DeterministicMultiAgentDriver {
        fn apply_case(
            &mut self,
            case: &MultiAgentContractCase,
        ) -> anyhow::Result<MultiAgentReplayResult> {
            Ok(evaluate_multi_agent_case(case))
        }
    }

    #[test]
    fn unit_parse_multi_agent_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "unsupported",
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
        "planner": {"role": "planner"},
        "delegated": {"role": "planner"},
        "review": {"role": "reviewer"}
      },
      "expected": {
        "outcome": "success",
        "selected_role": "planner",
        "attempted_roles": ["planner"]
      }
    }
  ]
}"#;
        let error = parse_multi_agent_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported multi-agent contract schema version"));
    }

    #[test]
    fn unit_validate_multi_agent_contract_fixture_rejects_duplicate_case_id() {
        let error =
            load_multi_agent_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
                .expect_err("duplicate case_id fixture should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("duplicate case_id"),
            "unexpected duplicate-case error: {rendered}"
        );
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_multi_agent_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load mixed outcome fixture");
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.cases.len(), 3);
        assert_eq!(fixture.cases[0].case_id, "planner-success");
        assert_eq!(fixture.cases[1].case_id, "delegated-malformed-route-table");
        assert_eq!(
            fixture.cases[2].case_id,
            "delegated-retryable-role-unavailable"
        );
    }

    #[test]
    fn integration_multi_agent_contract_replay_is_deterministic_across_reloads() {
        let fixture_path = fixture_path("mixed-outcomes.json");
        let fixture_a = load_multi_agent_contract_fixture(&fixture_path).expect("load fixture a");
        let fixture_b = load_multi_agent_contract_fixture(&fixture_path).expect("load fixture b");

        let mut driver_a = DeterministicMultiAgentDriver;
        let mut driver_b = DeterministicMultiAgentDriver;
        let summary_a =
            run_multi_agent_contract_replay(&fixture_a, &mut driver_a).expect("replay fixture a");
        let summary_b =
            run_multi_agent_contract_replay(&fixture_b, &mut driver_b).expect("replay fixture b");

        assert_eq!(summary_a, summary_b);
        assert_eq!(summary_a.discovered_cases, 3);
        assert_eq!(summary_a.success_cases, 1);
        assert_eq!(summary_a.malformed_cases, 1);
        assert_eq!(summary_a.retryable_failures, 1);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_multi_agent_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error code should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("unsupported error_code"),
            "unexpected unsupported-error-code message: {rendered}"
        );
    }

    #[test]
    fn regression_multi_agent_contract_replay_rejects_mismatched_selected_role() {
        let mut fixture = load_multi_agent_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        fixture.cases[0].expected.selected_role = "reviewer".to_string();
        fixture.cases[0].expected.attempted_roles =
            vec!["reviewer".to_string(), "planner-fallback".to_string()];
        let mut driver = DeterministicMultiAgentDriver;
        let error = run_multi_agent_contract_replay(&fixture, &mut driver)
            .expect_err("mismatched selected_role should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("expected.selected_role"),
            "unexpected mismatch error: {rendered}"
        );
    }
}
