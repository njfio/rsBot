use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const DASHBOARD_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub const DASHBOARD_ERROR_EMPTY_INPUT: &str = "dashboard_empty_input";
pub const DASHBOARD_ERROR_INVALID_SCOPE: &str = "dashboard_invalid_scope";
pub const DASHBOARD_ERROR_INVALID_FILTER: &str = "dashboard_invalid_filter";
pub const DASHBOARD_ERROR_INVALID_ACTION: &str = "dashboard_invalid_action";
pub const DASHBOARD_ERROR_BACKEND_UNAVAILABLE: &str = "dashboard_backend_unavailable";

const DASHBOARD_WIDGET_REFRESH_INTERVAL_MAX_MS: u64 = 86_400_000;

fn dashboard_contract_schema_version() -> u32 {
    DASHBOARD_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DashboardFixtureMode {
    Snapshot,
    Filter,
    Control,
}

impl DashboardFixtureMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Filter => "filter",
            Self::Control => "control",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DashboardOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DashboardFilterKind {
    TimeWindow,
    Channel,
    Severity,
    Search,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DashboardControlAction {
    Pause,
    Resume,
    Refresh,
}

impl DashboardControlAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Refresh => "refresh",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DashboardWidgetKind {
    HealthSummary,
    TransportTable,
    QueueChart,
    RunTimeline,
    SessionList,
    AlertFeed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardScope {
    pub workspace_id: String,
    #[serde(default)]
    pub operator_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardFilter {
    pub filter_id: String,
    pub kind: DashboardFilterKind,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardWidgetSpec {
    pub widget_id: String,
    pub kind: DashboardWidgetKind,
    pub title: String,
    pub query_key: String,
    pub refresh_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardCaseExpectation {
    pub outcome: DashboardOutcomeKind,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub widgets: Vec<DashboardWidgetSpec>,
    #[serde(default)]
    pub audit_event_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardContractCase {
    #[serde(default = "dashboard_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub mode: DashboardFixtureMode,
    pub scope: DashboardScope,
    #[serde(default)]
    pub filters: Vec<DashboardFilter>,
    #[serde(default)]
    pub requested_widgets: Vec<DashboardWidgetSpec>,
    #[serde(default)]
    pub control_action: Option<DashboardControlAction>,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    pub expected: DashboardCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DashboardContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<DashboardContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardContractCapabilities {
    pub schema_version: u32,
    pub supported_modes: BTreeSet<DashboardFixtureMode>,
    pub supported_outcomes: BTreeSet<DashboardOutcomeKind>,
    pub supported_error_codes: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardReplayResult {
    pub step: DashboardReplayStep,
    pub error_code: Option<String>,
    pub widgets: Vec<DashboardWidgetSpec>,
    pub audit_event_key: String,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DashboardReplaySummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

#[cfg(test)]
pub trait DashboardContractDriver {
    fn apply_case(&mut self, case: &DashboardContractCase) -> Result<DashboardReplayResult>;
}

pub fn parse_dashboard_contract_fixture(raw: &str) -> Result<DashboardContractFixture> {
    let fixture = serde_json::from_str::<DashboardContractFixture>(raw)
        .context("failed to parse dashboard contract fixture")?;
    validate_dashboard_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_dashboard_contract_fixture(path: &Path) -> Result<DashboardContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_dashboard_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn dashboard_contract_capabilities() -> DashboardContractCapabilities {
    DashboardContractCapabilities {
        schema_version: DASHBOARD_CONTRACT_SCHEMA_VERSION,
        supported_modes: [
            DashboardFixtureMode::Snapshot,
            DashboardFixtureMode::Filter,
            DashboardFixtureMode::Control,
        ]
        .into_iter()
        .collect(),
        supported_outcomes: [
            DashboardOutcomeKind::Success,
            DashboardOutcomeKind::MalformedInput,
            DashboardOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .collect(),
        supported_error_codes: supported_error_codes()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

pub fn validate_dashboard_contract_compatibility(fixture: &DashboardContractFixture) -> Result<()> {
    let capabilities = dashboard_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported dashboard contract schema version {} (expected {})",
            fixture.schema_version,
            capabilities.schema_version
        );
    }
    for case in &fixture.cases {
        if !capabilities.supported_modes.contains(&case.mode) {
            bail!(
                "fixture case '{}' uses unsupported mode '{}'",
                case.case_id,
                case.mode.as_str()
            );
        }
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
        let error_code = case.expected.error_code.trim();
        if !error_code.is_empty() && !capabilities.supported_error_codes.contains(error_code) {
            bail!(
                "fixture case '{}' uses unsupported error_code '{}'",
                case.case_id,
                error_code
            );
        }
    }
    Ok(())
}

pub fn validate_dashboard_contract_fixture(fixture: &DashboardContractFixture) -> Result<()> {
    if fixture.schema_version != DASHBOARD_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported dashboard contract schema version {} (expected {})",
            fixture.schema_version,
            DASHBOARD_CONTRACT_SCHEMA_VERSION
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
        validate_dashboard_case(case, index)?;
        let trimmed_case_id = case.case_id.trim().to_string();
        if !case_ids.insert(trimmed_case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", trimmed_case_id);
        }
    }

    validate_dashboard_contract_compatibility(fixture)?;
    Ok(())
}

#[cfg(test)]
pub fn run_dashboard_contract_replay<D: DashboardContractDriver>(
    fixture: &DashboardContractFixture,
    driver: &mut D,
) -> Result<DashboardReplaySummary> {
    validate_dashboard_contract_fixture(fixture)?;
    let mut summary = DashboardReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..DashboardReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        assert_dashboard_replay_matches_expectation(case, &result)?;
        match case.expected.outcome {
            DashboardOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1);
            }
            DashboardOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1);
            }
            DashboardOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1);
            }
        }
    }

    Ok(summary)
}

fn validate_dashboard_case(case: &DashboardContractCase, index: usize) -> Result<()> {
    if case.schema_version != DASHBOARD_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            DASHBOARD_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if case.scope.workspace_id.trim().is_empty()
        && case.expected.outcome != DashboardOutcomeKind::MalformedInput
    {
        bail!(
            "fixture case '{}' has empty scope.workspace_id without malformed_input expectation",
            case.case_id
        );
    }
    if case.simulate_retryable_failure
        && case.expected.outcome != DashboardOutcomeKind::RetryableFailure
    {
        bail!(
            "fixture case '{}' sets simulate_retryable_failure=true but expected outcome is {:?}",
            case.case_id,
            case.expected.outcome
        );
    }
    if case.expected.outcome == DashboardOutcomeKind::RetryableFailure
        && !case.simulate_retryable_failure
    {
        bail!(
            "fixture case '{}' expects retryable_failure but simulate_retryable_failure=false",
            case.case_id
        );
    }
    if case.mode == DashboardFixtureMode::Control && case.control_action.is_none() {
        bail!(
            "fixture case '{}' control mode requires control_action",
            case.case_id
        );
    }
    if case.mode == DashboardFixtureMode::Filter
        && case.expected.outcome != DashboardOutcomeKind::MalformedInput
        && case.filters.is_empty()
    {
        bail!(
            "fixture case '{}' filter mode requires at least one filter unless malformed_input",
            case.case_id
        );
    }
    if case.expected.outcome == DashboardOutcomeKind::Success && case.requested_widgets.is_empty() {
        bail!(
            "fixture case '{}' success outcome requires requested_widgets",
            case.case_id
        );
    }

    validate_filters(case)?;
    validate_widgets(&case.requested_widgets, &case.case_id, "requested_widgets")?;
    validate_expectation(case)?;
    Ok(())
}

fn validate_filters(case: &DashboardContractCase) -> Result<()> {
    let mut filter_ids = HashSet::new();
    for filter in &case.filters {
        if filter.filter_id.trim().is_empty() {
            bail!(
                "fixture case '{}' has filter with empty filter_id",
                case.case_id
            );
        }
        let normalized_id = filter.filter_id.trim().to_string();
        if !filter_ids.insert(normalized_id.clone()) {
            bail!(
                "fixture case '{}' has duplicate filter_id '{}'",
                case.case_id,
                normalized_id
            );
        }
        if case.expected.outcome != DashboardOutcomeKind::MalformedInput
            && filter.value.trim().is_empty()
        {
            bail!(
                "fixture case '{}' filter '{}' has empty value without malformed_input expectation",
                case.case_id,
                normalized_id
            );
        }
    }
    Ok(())
}

fn validate_widgets(
    widgets: &[DashboardWidgetSpec],
    case_id: &str,
    field_name: &str,
) -> Result<()> {
    let mut widget_ids = HashSet::new();
    for widget in widgets {
        if widget.widget_id.trim().is_empty() {
            bail!("fixture case '{}' has widget with empty widget_id", case_id);
        }
        let normalized_widget_id = widget.widget_id.trim().to_string();
        if !widget_ids.insert(normalized_widget_id.clone()) {
            bail!(
                "fixture case '{}' has duplicate widget_id '{}' in {}",
                case_id,
                normalized_widget_id,
                field_name
            );
        }
        if widget.title.trim().is_empty() {
            bail!(
                "fixture case '{}' widget '{}' has empty title",
                case_id,
                normalized_widget_id
            );
        }
        if widget.query_key.trim().is_empty() {
            bail!(
                "fixture case '{}' widget '{}' has empty query_key",
                case_id,
                normalized_widget_id
            );
        }
        if widget.refresh_interval_ms == 0 {
            bail!(
                "fixture case '{}' widget '{}' has refresh_interval_ms=0",
                case_id,
                normalized_widget_id
            );
        }
        if widget.refresh_interval_ms > DASHBOARD_WIDGET_REFRESH_INTERVAL_MAX_MS {
            bail!(
                "fixture case '{}' widget '{}' has refresh_interval_ms {} above max {}",
                case_id,
                normalized_widget_id,
                widget.refresh_interval_ms,
                DASHBOARD_WIDGET_REFRESH_INTERVAL_MAX_MS
            );
        }
    }
    Ok(())
}

fn validate_expectation(case: &DashboardContractCase) -> Result<()> {
    let error_code = case.expected.error_code.trim();
    match case.expected.outcome {
        DashboardOutcomeKind::Success => {
            if !error_code.is_empty() {
                bail!(
                    "fixture case '{}' success outcome must not include error_code",
                    case.case_id
                );
            }
            if case.expected.widgets.is_empty() {
                bail!(
                    "fixture case '{}' success outcome requires expected.widgets",
                    case.case_id
                );
            }
            if case.mode == DashboardFixtureMode::Control
                && case.expected.audit_event_key.is_empty()
            {
                bail!(
                    "fixture case '{}' control success requires expected.audit_event_key",
                    case.case_id
                );
            }
        }
        DashboardOutcomeKind::MalformedInput | DashboardOutcomeKind::RetryableFailure => {
            if error_code.is_empty() {
                bail!(
                    "fixture case '{}' {:?} outcome requires error_code",
                    case.case_id,
                    case.expected.outcome
                );
            }
            if !supported_error_codes().contains(&error_code) {
                bail!(
                    "fixture case '{}' uses unsupported error_code '{}'",
                    case.case_id,
                    error_code
                );
            }
            if !case.expected.widgets.is_empty() {
                bail!(
                    "fixture case '{}' non-success outcome must not include expected.widgets",
                    case.case_id
                );
            }
            if !case.expected.audit_event_key.is_empty() {
                bail!(
                    "fixture case '{}' non-success outcome must not include expected.audit_event_key",
                    case.case_id
                );
            }
        }
    }

    validate_widgets(&case.expected.widgets, &case.case_id, "expected.widgets")?;
    Ok(())
}

#[cfg(test)]
fn assert_dashboard_replay_matches_expectation(
    case: &DashboardContractCase,
    result: &DashboardReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        DashboardOutcomeKind::Success => DashboardReplayStep::Success,
        DashboardOutcomeKind::MalformedInput => DashboardReplayStep::MalformedInput,
        DashboardOutcomeKind::RetryableFailure => DashboardReplayStep::RetryableFailure,
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
        DashboardOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
            if result.widgets != case.expected.widgets {
                bail!(
                    "case '{}' expected widgets {:?} but observed {:?}",
                    case.case_id,
                    case.expected.widgets,
                    result.widgets
                );
            }
            if result.audit_event_key != case.expected.audit_event_key {
                bail!(
                    "case '{}' expected audit_event_key '{}' but observed '{}'",
                    case.case_id,
                    case.expected.audit_event_key,
                    result.audit_event_key
                );
            }
        }
        DashboardOutcomeKind::MalformedInput | DashboardOutcomeKind::RetryableFailure => {
            let expected_code = case.expected.error_code.trim();
            if result.error_code.as_deref() != Some(expected_code) {
                bail!(
                    "case '{}' expected error_code '{}' but observed {:?}",
                    case.case_id,
                    expected_code,
                    result.error_code
                );
            }
            if !result.widgets.is_empty() {
                bail!(
                    "case '{}' expected no widgets for non-success outcome but observed {} widgets",
                    case.case_id,
                    result.widgets.len()
                );
            }
            if !result.audit_event_key.is_empty() {
                bail!(
                    "case '{}' expected empty audit_event_key for non-success outcome but observed '{}'",
                    case.case_id,
                    result.audit_event_key
                );
            }
        }
    }
    Ok(())
}

fn supported_error_codes() -> [&'static str; 5] {
    [
        DASHBOARD_ERROR_EMPTY_INPUT,
        DASHBOARD_ERROR_INVALID_SCOPE,
        DASHBOARD_ERROR_INVALID_FILTER,
        DASHBOARD_ERROR_INVALID_ACTION,
        DASHBOARD_ERROR_BACKEND_UNAVAILABLE,
    ]
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        load_dashboard_contract_fixture, parse_dashboard_contract_fixture,
        run_dashboard_contract_replay, DashboardContractCase, DashboardContractDriver,
        DashboardContractFixture, DashboardFixtureMode, DashboardReplayResult, DashboardReplayStep,
        DASHBOARD_ERROR_BACKEND_UNAVAILABLE, DASHBOARD_ERROR_EMPTY_INPUT,
        DASHBOARD_ERROR_INVALID_ACTION, DASHBOARD_ERROR_INVALID_FILTER,
        DASHBOARD_ERROR_INVALID_SCOPE,
    };

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("dashboard-contract")
            .join(name)
    }

    #[derive(Default)]
    struct DeterministicDashboardDriver;

    impl DashboardContractDriver for DeterministicDashboardDriver {
        fn apply_case(
            &mut self,
            case: &DashboardContractCase,
        ) -> anyhow::Result<DashboardReplayResult> {
            if case.simulate_retryable_failure {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::RetryableFailure,
                    error_code: Some(DASHBOARD_ERROR_BACKEND_UNAVAILABLE.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }

            if case.scope.workspace_id.trim().is_empty() {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_INVALID_SCOPE.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }

            if case.requested_widgets.is_empty() {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_EMPTY_INPUT.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }

            if case.mode == DashboardFixtureMode::Filter && case.filters.is_empty() {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_EMPTY_INPUT.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }
            if case
                .filters
                .iter()
                .any(|filter| filter.value.trim().is_empty())
            {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_INVALID_FILTER.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }

            if case.mode == DashboardFixtureMode::Control && case.control_action.is_none() {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_INVALID_ACTION.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }

            let mut widgets = case.requested_widgets.clone();
            widgets.sort_by(|left, right| left.widget_id.cmp(&right.widget_id));

            let audit_event_key = match case.mode {
                DashboardFixtureMode::Control => {
                    let action = case
                        .control_action
                        .expect("control mode should include action")
                        .as_str();
                    format!("dashboard-control:{action}:{}", case.case_id.trim())
                }
                _ => String::new(),
            };

            Ok(DashboardReplayResult {
                step: DashboardReplayStep::Success,
                error_code: None,
                widgets,
                audit_event_key,
            })
        }
    }

    #[test]
    fn unit_parse_dashboard_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "unsupported",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "snapshot",
      "mode": "snapshot",
      "scope": { "workspace_id": "tau-core" },
      "requested_widgets": [
        {
          "widget_id": "health-summary",
          "kind": "health_summary",
          "title": "Health",
          "query_key": "transport.health",
          "refresh_interval_ms": 5000
        }
      ],
      "expected": {
        "outcome": "success",
        "widgets": [
          {
            "widget_id": "health-summary",
            "kind": "health_summary",
            "title": "Health",
            "query_key": "transport.health",
            "refresh_interval_ms": 5000
          }
        ]
      }
    }
  ]
}"#;
        let error = parse_dashboard_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported dashboard contract schema version"));
    }

    #[test]
    fn unit_validate_dashboard_contract_fixture_rejects_duplicate_case_id() {
        let error =
            load_dashboard_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
                .expect_err("duplicate case id should fail");
        assert!(format!("{error:#}").contains("duplicate case_id"));
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_dashboard_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        assert_eq!(fixture.cases.len(), 3);
        assert_eq!(fixture.cases[0].case_id, "snapshot-core-overview");
        assert_eq!(fixture.cases[1].case_id, "filter-empty-close-window");
        assert_eq!(fixture.cases[2].case_id, "control-pause-retryable");
    }

    #[test]
    fn functional_dashboard_contract_replay_executes_outcome_matrix() {
        let fixture = load_dashboard_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        let mut driver = DeterministicDashboardDriver;
        let summary = run_dashboard_contract_replay(&fixture, &mut driver).expect("replay");
        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.success_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 1);
    }

    #[test]
    fn integration_dashboard_contract_replay_is_deterministic_across_reloads() {
        let path = fixture_path("snapshot-layout.json");
        let first = load_dashboard_contract_fixture(&path).expect("first load");
        let second = load_dashboard_contract_fixture(&path).expect("second load");
        assert_eq!(first, second);

        let mut first_driver = DeterministicDashboardDriver;
        let first_summary =
            run_dashboard_contract_replay(&first, &mut first_driver).expect("first replay");

        let mut second_driver = DeterministicDashboardDriver;
        let second_summary =
            run_dashboard_contract_replay(&second, &mut second_driver).expect("second replay");

        assert_eq!(first_summary, second_summary);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_dashboard_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error code should fail");
        assert!(format!("{error:#}").contains("unsupported error_code"));
    }

    #[test]
    fn regression_dashboard_contract_replay_rejects_mismatched_expected_widgets() {
        let mut fixture = load_dashboard_contract_fixture(&fixture_path("snapshot-layout.json"))
            .expect("fixture");
        fixture.cases[0].expected.widgets[0].title = "Incorrect".to_string();

        let mut driver = DeterministicDashboardDriver;
        let error =
            run_dashboard_contract_replay(&fixture, &mut driver).expect_err("mismatch should fail");
        assert!(error.to_string().contains("expected widgets"));
    }

    #[test]
    fn regression_dashboard_contract_replay_rejects_mismatched_control_audit_event_key() {
        let mut fixture = DashboardContractFixture {
            schema_version: 1,
            name: "control-mismatch".to_string(),
            description: String::new(),
            cases: vec![
                load_dashboard_contract_fixture(&fixture_path("snapshot-layout.json"))
                    .expect("fixture")
                    .cases
                    .into_iter()
                    .find(|case| case.mode == DashboardFixtureMode::Control)
                    .expect("control case"),
            ],
        };
        fixture.cases[0].expected.audit_event_key = "wrong-audit-key".to_string();
        let mut driver = DeterministicDashboardDriver;
        let error =
            run_dashboard_contract_replay(&fixture, &mut driver).expect_err("audit mismatch");
        assert!(error.to_string().contains("expected audit_event_key"));
    }
}
