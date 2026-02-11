use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const MEMORY_CONTRACT_SCHEMA_VERSION: u32 = 1;
const MEMORY_CONTRACT_DEFAULT_RETRIEVAL_LIMIT: usize = 5;
const MEMORY_CONTRACT_MAX_RETRIEVAL_LIMIT: usize = 50;

pub const MEMORY_ERROR_EMPTY_INPUT: &str = "memory_empty_input";
pub const MEMORY_ERROR_INVALID_SCOPE: &str = "memory_invalid_scope";
pub const MEMORY_ERROR_BACKEND_UNAVAILABLE: &str = "memory_backend_unavailable";

fn memory_contract_schema_version() -> u32 {
    MEMORY_CONTRACT_SCHEMA_VERSION
}

fn default_retrieval_limit() -> usize {
    MEMORY_CONTRACT_DEFAULT_RETRIEVAL_LIMIT
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFixtureMode {
    Extract,
    Retrieve,
}

impl MemoryFixtureMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extract => "extract",
            Self::Retrieve => "retrieve",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryScope {
    pub workspace_id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub actor_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryEntry {
    pub memory_id: String,
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub facts: Vec<String>,
    #[serde(default)]
    pub source_event_key: String,
    #[serde(default)]
    pub recency_weight_bps: u16,
    #[serde(default)]
    pub confidence_bps: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryCaseExpectation {
    pub outcome: MemoryOutcomeKind,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub entries: Vec<MemoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryContractCase {
    #[serde(default = "memory_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub mode: MemoryFixtureMode,
    pub scope: MemoryScope,
    #[serde(default)]
    pub input_text: String,
    #[serde(default)]
    pub query_text: String,
    #[serde(default = "default_retrieval_limit")]
    pub retrieval_limit: usize,
    #[serde(default)]
    pub prior_entries: Vec<MemoryEntry>,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    pub expected: MemoryCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<MemoryContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryContractCapabilities {
    pub schema_version: u32,
    pub supported_modes: BTreeSet<MemoryFixtureMode>,
    pub supported_outcomes: BTreeSet<MemoryOutcomeKind>,
    pub supported_error_codes: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReplayResult {
    pub step: MemoryReplayStep,
    pub error_code: Option<String>,
    pub entries: Vec<MemoryEntry>,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryReplaySummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

#[cfg(test)]
pub trait MemoryContractDriver {
    fn apply_case(&mut self, case: &MemoryContractCase) -> Result<MemoryReplayResult>;
}

pub fn parse_memory_contract_fixture(raw: &str) -> Result<MemoryContractFixture> {
    let fixture = serde_json::from_str::<MemoryContractFixture>(raw)
        .context("failed to parse memory contract fixture")?;
    validate_memory_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_memory_contract_fixture(path: &Path) -> Result<MemoryContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_memory_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn memory_contract_capabilities() -> MemoryContractCapabilities {
    MemoryContractCapabilities {
        schema_version: MEMORY_CONTRACT_SCHEMA_VERSION,
        supported_modes: [MemoryFixtureMode::Extract, MemoryFixtureMode::Retrieve]
            .into_iter()
            .collect(),
        supported_outcomes: [
            MemoryOutcomeKind::Success,
            MemoryOutcomeKind::MalformedInput,
            MemoryOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .collect(),
        supported_error_codes: supported_error_codes()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

pub fn validate_memory_contract_compatibility(fixture: &MemoryContractFixture) -> Result<()> {
    let capabilities = memory_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported memory contract schema version {} (expected {})",
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
        let code = case.expected.error_code.trim();
        if !code.is_empty() && !capabilities.supported_error_codes.contains(code) {
            bail!(
                "fixture case '{}' uses unsupported error_code '{}'",
                case.case_id,
                code
            );
        }
    }
    Ok(())
}

pub fn validate_memory_contract_fixture(fixture: &MemoryContractFixture) -> Result<()> {
    if fixture.schema_version != MEMORY_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported memory contract schema version {} (expected {})",
            fixture.schema_version,
            MEMORY_CONTRACT_SCHEMA_VERSION
        );
    }
    if fixture.name.trim().is_empty() {
        bail!("fixture name cannot be empty");
    }
    if fixture.cases.is_empty() {
        bail!("fixture must include at least one case");
    }

    let capabilities = memory_contract_capabilities();
    let mut case_ids = HashSet::new();
    for (index, case) in fixture.cases.iter().enumerate() {
        validate_memory_case(case, index)?;
        let trimmed_case_id = case.case_id.trim().to_string();
        if !case_ids.insert(trimmed_case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", trimmed_case_id);
        }
    }

    validate_memory_contract_compatibility(fixture)?;
    if capabilities.schema_version != MEMORY_CONTRACT_SCHEMA_VERSION {
        bail!(
            "memory contract capabilities mismatch: capabilities schema={} fixture schema={}",
            capabilities.schema_version,
            MEMORY_CONTRACT_SCHEMA_VERSION
        );
    }
    Ok(())
}

#[cfg(test)]
pub fn run_memory_contract_replay<D: MemoryContractDriver>(
    fixture: &MemoryContractFixture,
    driver: &mut D,
) -> Result<MemoryReplaySummary> {
    validate_memory_contract_fixture(fixture)?;
    let mut summary = MemoryReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..MemoryReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        assert_memory_replay_matches_expectation(case, &result)?;
        match case.expected.outcome {
            MemoryOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1);
            }
            MemoryOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1);
            }
            MemoryOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1);
            }
        }
    }

    Ok(summary)
}

fn validate_memory_case(case: &MemoryContractCase, index: usize) -> Result<()> {
    if case.schema_version != MEMORY_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            MEMORY_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if case.retrieval_limit == 0 {
        bail!(
            "fixture case '{}' has retrieval_limit 0; expected at least 1",
            case.case_id
        );
    }
    if case.retrieval_limit > MEMORY_CONTRACT_MAX_RETRIEVAL_LIMIT {
        bail!(
            "fixture case '{}' has retrieval_limit {} above supported max {}",
            case.case_id,
            case.retrieval_limit,
            MEMORY_CONTRACT_MAX_RETRIEVAL_LIMIT
        );
    }
    if case.scope.workspace_id.trim().is_empty()
        && case.expected.outcome != MemoryOutcomeKind::MalformedInput
    {
        bail!(
            "fixture case '{}' has empty scope.workspace_id without malformed_input expectation",
            case.case_id
        );
    }
    if case.simulate_retryable_failure
        && case.expected.outcome != MemoryOutcomeKind::RetryableFailure
    {
        bail!(
            "fixture case '{}' sets simulate_retryable_failure=true but expected outcome is {:?}",
            case.case_id,
            case.expected.outcome
        );
    }
    if case.expected.outcome == MemoryOutcomeKind::RetryableFailure
        && !case.simulate_retryable_failure
    {
        bail!(
            "fixture case '{}' expects retryable_failure but simulate_retryable_failure=false",
            case.case_id
        );
    }

    match case.mode {
        MemoryFixtureMode::Extract => {
            if case.expected.outcome != MemoryOutcomeKind::MalformedInput
                && case.input_text.trim().is_empty()
            {
                bail!(
                    "fixture case '{}' extract mode requires non-empty input_text unless malformed_input",
                    case.case_id
                );
            }
        }
        MemoryFixtureMode::Retrieve => {
            if case.expected.outcome != MemoryOutcomeKind::MalformedInput
                && case.query_text.trim().is_empty()
            {
                bail!(
                    "fixture case '{}' retrieve mode requires non-empty query_text unless malformed_input",
                    case.case_id
                );
            }
            if case.expected.outcome == MemoryOutcomeKind::Success && case.prior_entries.is_empty()
            {
                bail!(
                    "fixture case '{}' retrieve mode requires prior_entries for success cases",
                    case.case_id
                );
            }
        }
    }

    for entry in &case.prior_entries {
        validate_memory_entry(entry, &case.case_id, "prior_entries")?;
    }

    validate_case_expectation(case)
}

fn validate_case_expectation(case: &MemoryContractCase) -> Result<()> {
    let expected = &case.expected;
    let error_code = expected.error_code.trim();
    match expected.outcome {
        MemoryOutcomeKind::Success => {
            if !error_code.is_empty() {
                bail!(
                    "fixture case '{}' success expectation must have empty error_code",
                    case.case_id
                );
            }
            if expected.entries.is_empty() {
                bail!(
                    "fixture case '{}' success expectation must include at least one entry",
                    case.case_id
                );
            }
            for entry in &expected.entries {
                validate_memory_entry(entry, &case.case_id, "expected.entries")?;
            }
        }
        MemoryOutcomeKind::MalformedInput => {
            if expected.entries.is_empty() {
                // malformed cases still validate deterministic replay with an explicit empty payload.
            } else {
                bail!(
                    "fixture case '{}' malformed_input expectation must not include entries",
                    case.case_id
                );
            }
            if error_code.is_empty() {
                bail!(
                    "fixture case '{}' malformed_input expectation must provide error_code",
                    case.case_id
                );
            }
            if !supported_error_codes().contains(&error_code) {
                bail!(
                    "fixture case '{}' uses unsupported error_code '{}'",
                    case.case_id,
                    error_code
                );
            }
        }
        MemoryOutcomeKind::RetryableFailure => {
            if !expected.entries.is_empty() {
                bail!(
                    "fixture case '{}' retryable_failure expectation must not include entries",
                    case.case_id
                );
            }
            if error_code != MEMORY_ERROR_BACKEND_UNAVAILABLE {
                bail!(
                    "fixture case '{}' retryable_failure must use error_code '{}'",
                    case.case_id,
                    MEMORY_ERROR_BACKEND_UNAVAILABLE
                );
            }
        }
    }
    Ok(())
}

fn validate_memory_entry(entry: &MemoryEntry, case_id: &str, location: &str) -> Result<()> {
    if entry.memory_id.trim().is_empty() {
        bail!(
            "fixture case '{}' has {} entry with empty memory_id",
            case_id,
            location
        );
    }
    if entry.summary.trim().is_empty() {
        bail!(
            "fixture case '{}' has {} entry with empty summary",
            case_id,
            location
        );
    }
    if entry.source_event_key.trim().is_empty() {
        bail!(
            "fixture case '{}' has {} entry with empty source_event_key",
            case_id,
            location
        );
    }
    if entry.recency_weight_bps > 10_000 {
        bail!(
            "fixture case '{}' has {} entry with recency_weight_bps {} above 10000",
            case_id,
            location,
            entry.recency_weight_bps
        );
    }
    if entry.confidence_bps > 10_000 {
        bail!(
            "fixture case '{}' has {} entry with confidence_bps {} above 10000",
            case_id,
            location,
            entry.confidence_bps
        );
    }

    let mut tag_set = HashSet::new();
    for tag in &entry.tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            bail!(
                "fixture case '{}' has {} entry with empty tag",
                case_id,
                location
            );
        }
        if !tag_set.insert(trimmed.to_lowercase()) {
            bail!(
                "fixture case '{}' has {} entry with duplicate tag '{}'",
                case_id,
                location,
                trimmed
            );
        }
    }

    let mut fact_set = HashSet::new();
    for fact in &entry.facts {
        let trimmed = fact.trim();
        if trimmed.is_empty() {
            bail!(
                "fixture case '{}' has {} entry with empty fact",
                case_id,
                location
            );
        }
        if !fact_set.insert(trimmed.to_lowercase()) {
            bail!(
                "fixture case '{}' has {} entry with duplicate fact '{}'",
                case_id,
                location,
                trimmed
            );
        }
    }

    Ok(())
}

#[cfg(test)]
fn assert_memory_replay_matches_expectation(
    case: &MemoryContractCase,
    result: &MemoryReplayResult,
) -> Result<()> {
    let expected_step = expected_replay_step(case.expected.outcome);
    if result.step != expected_step {
        bail!(
            "case '{}' expected replay step {:?} but observed {:?}",
            case.case_id,
            expected_step,
            result.step
        );
    }
    match case.expected.outcome {
        MemoryOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected no error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
            if result.entries != case.expected.entries {
                bail!(
                    "case '{}' expected replay entries {:?} but observed {:?}",
                    case.case_id,
                    case.expected.entries,
                    result.entries
                );
            }
        }
        MemoryOutcomeKind::MalformedInput | MemoryOutcomeKind::RetryableFailure => {
            if !result.entries.is_empty() {
                bail!(
                    "case '{}' expected empty replay entries for {:?} but observed {} entries",
                    case.case_id,
                    case.expected.outcome,
                    result.entries.len()
                );
            }
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
    Ok(())
}

fn supported_error_codes() -> [&'static str; 3] {
    [
        MEMORY_ERROR_EMPTY_INPUT,
        MEMORY_ERROR_INVALID_SCOPE,
        MEMORY_ERROR_BACKEND_UNAVAILABLE,
    ]
}

#[cfg(test)]
fn expected_replay_step(outcome: MemoryOutcomeKind) -> MemoryReplayStep {
    match outcome {
        MemoryOutcomeKind::Success => MemoryReplayStep::Success,
        MemoryOutcomeKind::MalformedInput => MemoryReplayStep::MalformedInput,
        MemoryOutcomeKind::RetryableFailure => MemoryReplayStep::RetryableFailure,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    use anyhow::Result;

    use super::{
        load_memory_contract_fixture, parse_memory_contract_fixture, run_memory_contract_replay,
        MemoryContractCase, MemoryContractDriver, MemoryEntry, MemoryFixtureMode,
        MemoryReplayResult, MemoryReplayStep, MEMORY_ERROR_BACKEND_UNAVAILABLE,
        MEMORY_ERROR_EMPTY_INPUT, MEMORY_ERROR_INVALID_SCOPE,
    };

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("memory-contract")
            .join(name)
    }

    #[derive(Default)]
    struct DeterministicMemoryDriver;

    impl MemoryContractDriver for DeterministicMemoryDriver {
        fn apply_case(&mut self, case: &MemoryContractCase) -> Result<MemoryReplayResult> {
            let workspace = case.scope.workspace_id.trim();
            if workspace.is_empty() {
                return Ok(MemoryReplayResult {
                    step: MemoryReplayStep::MalformedInput,
                    error_code: Some(MEMORY_ERROR_INVALID_SCOPE.to_string()),
                    entries: Vec::new(),
                });
            }

            match case.mode {
                MemoryFixtureMode::Extract => {
                    if case.input_text.trim().is_empty() {
                        return Ok(MemoryReplayResult {
                            step: MemoryReplayStep::MalformedInput,
                            error_code: Some(MEMORY_ERROR_EMPTY_INPUT.to_string()),
                            entries: Vec::new(),
                        });
                    }
                    if case.simulate_retryable_failure {
                        return Ok(MemoryReplayResult {
                            step: MemoryReplayStep::RetryableFailure,
                            error_code: Some(MEMORY_ERROR_BACKEND_UNAVAILABLE.to_string()),
                            entries: Vec::new(),
                        });
                    }
                    Ok(MemoryReplayResult {
                        step: MemoryReplayStep::Success,
                        error_code: None,
                        entries: vec![build_extract_entry(case)],
                    })
                }
                MemoryFixtureMode::Retrieve => {
                    if case.query_text.trim().is_empty() {
                        return Ok(MemoryReplayResult {
                            step: MemoryReplayStep::MalformedInput,
                            error_code: Some(MEMORY_ERROR_EMPTY_INPUT.to_string()),
                            entries: Vec::new(),
                        });
                    }
                    if case.simulate_retryable_failure {
                        return Ok(MemoryReplayResult {
                            step: MemoryReplayStep::RetryableFailure,
                            error_code: Some(MEMORY_ERROR_BACKEND_UNAVAILABLE.to_string()),
                            entries: Vec::new(),
                        });
                    }
                    Ok(MemoryReplayResult {
                        step: MemoryReplayStep::Success,
                        error_code: None,
                        entries: retrieve_ranked_entries(case),
                    })
                }
            }
        }
    }

    fn build_extract_entry(case: &MemoryContractCase) -> MemoryEntry {
        let normalized_input = normalize_whitespace(&case.input_text);
        MemoryEntry {
            memory_id: format!("mem-{}", case.case_id.trim()),
            summary: normalized_input.clone(),
            tags: derive_tags(&normalized_input),
            facts: vec![format!("scope={}", case.scope.workspace_id.trim())],
            source_event_key: format!(
                "{}:{}:{}",
                case.scope.workspace_id.trim(),
                case.mode.as_str(),
                case.case_id.trim()
            ),
            recency_weight_bps: 9_000,
            confidence_bps: 8_200,
        }
    }

    fn retrieve_ranked_entries(case: &MemoryContractCase) -> Vec<MemoryEntry> {
        let query_tokens = tokenize_word_set(&case.query_text);
        let mut ranked = case
            .prior_entries
            .iter()
            .cloned()
            .map(|entry| {
                (
                    score_entry_against_query(&entry, &query_tokens),
                    entry.recency_weight_bps,
                    entry.confidence_bps,
                    entry.memory_id.clone(),
                    entry,
                )
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.cmp(&left.1))
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| left.3.cmp(&right.3))
        });
        ranked
            .into_iter()
            .take(case.retrieval_limit)
            .map(|item| item.4)
            .collect()
    }

    fn score_entry_against_query(entry: &MemoryEntry, query_tokens: &HashSet<String>) -> u32 {
        if query_tokens.is_empty() {
            return 0;
        }
        let summary = entry.summary.to_ascii_lowercase();
        let facts = entry.facts.join(" ").to_ascii_lowercase();
        let tags = entry
            .tags
            .iter()
            .map(|tag| tag.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        query_tokens.iter().fold(0_u32, |score, token| {
            let mut updated = score;
            if summary.contains(token) {
                updated = updated.saturating_add(2);
            }
            if facts.contains(token) {
                updated = updated.saturating_add(1);
            }
            if tags.contains(token) {
                updated = updated.saturating_add(3);
            }
            updated
        })
    }

    fn derive_tags(text: &str) -> Vec<String> {
        let mut tags = Vec::new();
        let mut seen = HashSet::new();
        for token in tokenize_words(text) {
            if token.len() < 4 {
                continue;
            }
            if seen.insert(token.clone()) {
                tags.push(token);
            }
            if tags.len() >= 3 {
                break;
            }
        }
        if tags.is_empty() {
            tags.push("memory".to_string());
        }
        tags
    }

    fn normalize_whitespace(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn tokenize_words(text: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut ordered = Vec::new();
        for token in text.split(|character: char| !character.is_ascii_alphanumeric()) {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = trimmed.to_ascii_lowercase();
            if seen.insert(normalized.clone()) {
                ordered.push(normalized);
            }
        }
        ordered
    }

    fn tokenize_word_set(text: &str) -> HashSet<String> {
        text.split(|character: char| !character.is_ascii_alphanumeric())
            .filter_map(|token| {
                let trimmed = token.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_ascii_lowercase())
                }
            })
            .collect()
    }

    #[test]
    fn unit_parse_memory_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "unsupported-schema",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "extract-basic",
      "mode": "extract",
      "scope": {"workspace_id": "tau-core"},
      "input_text": "remember this",
      "expected": {
        "outcome": "success",
        "entries": [
          {
            "memory_id": "mem-extract-basic",
            "summary": "remember this",
            "tags": ["remember"],
            "facts": ["scope=tau-core"],
            "source_event_key": "tau-core:extract:extract-basic",
            "recency_weight_bps": 9000,
            "confidence_bps": 8200
          }
        ]
      }
    }
  ]
}"#;
        let error = parse_memory_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported memory contract schema version"));
    }

    #[test]
    fn unit_validate_memory_contract_fixture_rejects_duplicate_case_id() {
        let error = load_memory_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
            .expect_err("duplicate case id should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("duplicate case_id"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_memory_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        assert_eq!(fixture.cases.len(), 3);

        let mut success = 0;
        let mut malformed = 0;
        let mut retryable = 0;
        for case in &fixture.cases {
            match case.expected.outcome {
                super::MemoryOutcomeKind::Success => success += 1,
                super::MemoryOutcomeKind::MalformedInput => malformed += 1,
                super::MemoryOutcomeKind::RetryableFailure => retryable += 1,
            }
        }
        assert_eq!(success, 1);
        assert_eq!(malformed, 1);
        assert_eq!(retryable, 1);
    }

    #[test]
    fn functional_memory_contract_replay_executes_outcome_matrix() {
        let fixture = load_memory_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("fixture should load");
        let mut driver = DeterministicMemoryDriver;
        let summary = run_memory_contract_replay(&fixture, &mut driver).expect("replay");
        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.success_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 1);
    }

    #[test]
    fn integration_memory_contract_replay_is_deterministic_across_reloads() {
        let path = fixture_path("retrieve-ranking.json");
        let first_fixture = load_memory_contract_fixture(&path).expect("first load");
        let second_fixture = load_memory_contract_fixture(&path).expect("second load");

        let mut first_driver = DeterministicMemoryDriver;
        let mut second_driver = DeterministicMemoryDriver;
        let first_summary =
            run_memory_contract_replay(&first_fixture, &mut first_driver).expect("first replay");
        let second_summary =
            run_memory_contract_replay(&second_fixture, &mut second_driver).expect("second replay");

        assert_eq!(first_summary, second_summary);
        assert_eq!(first_summary.success_cases, 1);
        assert_eq!(first_summary.malformed_cases, 0);
        assert_eq!(first_summary.retryable_failures, 0);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_memory_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error_code should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("unsupported error_code"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn regression_memory_contract_replay_rejects_mismatched_expected_entries() {
        let mut fixture = load_memory_contract_fixture(&fixture_path("retrieve-ranking.json"))
            .expect("fixture should load");
        fixture.cases[0].expected.entries[0].summary = "incorrect summary".to_string();

        let mut driver = DeterministicMemoryDriver;
        let error =
            run_memory_contract_replay(&fixture, &mut driver).expect_err("mismatch should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("expected replay entries"),
            "unexpected error output: {rendered}"
        );
    }
}
