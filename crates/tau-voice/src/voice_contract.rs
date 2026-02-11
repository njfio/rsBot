#![allow(dead_code)]

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub const VOICE_CONTRACT_SCHEMA_VERSION: u32 = 1;

pub const VOICE_ERROR_EMPTY_TRANSCRIPT: &str = "voice_empty_transcript";
pub const VOICE_ERROR_INVALID_WAKE_WORD: &str = "voice_invalid_wake_word";
pub const VOICE_ERROR_INVALID_LOCALE: &str = "voice_invalid_locale";
pub const VOICE_ERROR_BACKEND_UNAVAILABLE: &str = "voice_backend_unavailable";

fn voice_contract_schema_version() -> u32 {
    VOICE_CONTRACT_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum VoiceFixtureMode {
    WakeWord,
    Turn,
}

impl VoiceFixtureMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WakeWord => "wake_word",
            Self::Turn => "turn",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum VoiceOutcomeKind {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceCaseExpectation {
    pub outcome: VoiceOutcomeKind,
    pub status_code: u16,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub response_body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceContractCase {
    #[serde(default = "voice_contract_schema_version")]
    pub schema_version: u32,
    pub case_id: String,
    pub mode: VoiceFixtureMode,
    #[serde(default)]
    pub wake_word: String,
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub speaker_id: String,
    #[serde(default)]
    pub simulate_retryable_failure: bool,
    pub expected: VoiceCaseExpectation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceContractFixture {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<VoiceContractCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceContractCapabilities {
    pub schema_version: u32,
    pub supported_modes: BTreeSet<VoiceFixtureMode>,
    pub supported_outcomes: BTreeSet<VoiceOutcomeKind>,
    pub supported_error_codes: BTreeSet<String>,
    pub supported_wake_words: BTreeSet<String>,
    pub supported_locales: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceReplayStep {
    Success,
    MalformedInput,
    RetryableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceReplayResult {
    pub step: VoiceReplayStep,
    pub status_code: u16,
    pub error_code: Option<String>,
    pub response_body: serde_json::Value,
}

#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceReplaySummary {
    pub discovered_cases: usize,
    pub success_cases: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
}

#[cfg(test)]
pub trait VoiceContractDriver {
    fn apply_case(&mut self, case: &VoiceContractCase) -> Result<VoiceReplayResult>;
}

pub fn parse_voice_contract_fixture(raw: &str) -> Result<VoiceContractFixture> {
    let fixture = serde_json::from_str::<VoiceContractFixture>(raw)
        .context("failed to parse voice contract fixture")?;
    validate_voice_contract_fixture(&fixture)?;
    Ok(fixture)
}

pub fn load_voice_contract_fixture(path: &Path) -> Result<VoiceContractFixture> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    parse_voice_contract_fixture(&raw)
        .with_context(|| format!("invalid fixture {}", path.display()))
}

pub fn voice_contract_capabilities() -> VoiceContractCapabilities {
    VoiceContractCapabilities {
        schema_version: VOICE_CONTRACT_SCHEMA_VERSION,
        supported_modes: [VoiceFixtureMode::WakeWord, VoiceFixtureMode::Turn]
            .into_iter()
            .collect(),
        supported_outcomes: [
            VoiceOutcomeKind::Success,
            VoiceOutcomeKind::MalformedInput,
            VoiceOutcomeKind::RetryableFailure,
        ]
        .into_iter()
        .collect(),
        supported_error_codes: supported_error_codes()
            .into_iter()
            .map(str::to_string)
            .collect(),
        supported_wake_words: supported_wake_words()
            .into_iter()
            .map(str::to_string)
            .collect(),
        supported_locales: supported_locales()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

pub fn validate_voice_contract_compatibility(fixture: &VoiceContractFixture) -> Result<()> {
    let capabilities = voice_contract_capabilities();
    if fixture.schema_version != capabilities.schema_version {
        bail!(
            "unsupported voice contract schema version {} (expected {})",
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
        let expected_code = case.expected.error_code.trim();
        if !expected_code.is_empty() && !capabilities.supported_error_codes.contains(expected_code)
        {
            bail!(
                "fixture case '{}' uses unsupported error_code '{}'",
                case.case_id,
                expected_code
            );
        }

        if case.expected.outcome != VoiceOutcomeKind::MalformedInput {
            let normalized_wake_word = normalize_wake_word(&case.wake_word);
            if !capabilities
                .supported_wake_words
                .contains(normalized_wake_word.as_str())
            {
                bail!(
                    "fixture case '{}' uses unsupported wake_word '{}'",
                    case.case_id,
                    case.wake_word
                );
            }
            if !capabilities.supported_locales.contains(case.locale.trim()) {
                bail!(
                    "fixture case '{}' uses unsupported locale '{}'",
                    case.case_id,
                    case.locale
                );
            }
        }
    }
    Ok(())
}

pub fn validate_voice_contract_fixture(fixture: &VoiceContractFixture) -> Result<()> {
    if fixture.schema_version != VOICE_CONTRACT_SCHEMA_VERSION {
        bail!(
            "unsupported voice contract schema version {} (expected {})",
            fixture.schema_version,
            VOICE_CONTRACT_SCHEMA_VERSION
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
        validate_voice_case(case, index)?;
        let trimmed_case_id = case.case_id.trim().to_string();
        if !case_ids.insert(trimmed_case_id.clone()) {
            bail!("fixture contains duplicate case_id '{}'", trimmed_case_id);
        }
    }

    validate_voice_contract_compatibility(fixture)?;
    Ok(())
}

pub fn evaluate_voice_case(case: &VoiceContractCase) -> VoiceReplayResult {
    if case.simulate_retryable_failure {
        return VoiceReplayResult {
            step: VoiceReplayStep::RetryableFailure,
            status_code: 503,
            error_code: Some(VOICE_ERROR_BACKEND_UNAVAILABLE.to_string()),
            response_body: json!({"status":"retryable","reason":"backend_unavailable"}),
        };
    }

    let wake_word = normalize_wake_word(&case.wake_word);
    if !supported_wake_words().contains(wake_word.as_str()) {
        return VoiceReplayResult {
            step: VoiceReplayStep::MalformedInput,
            status_code: 422,
            error_code: Some(VOICE_ERROR_INVALID_WAKE_WORD.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_wake_word"}),
        };
    }

    let locale = case.locale.trim();
    if !supported_locales().contains(locale) {
        return VoiceReplayResult {
            step: VoiceReplayStep::MalformedInput,
            status_code: 422,
            error_code: Some(VOICE_ERROR_INVALID_LOCALE.to_string()),
            response_body: json!({"status":"rejected","reason":"invalid_locale"}),
        };
    }

    let transcript = case.transcript.trim();
    if transcript.is_empty() {
        return VoiceReplayResult {
            step: VoiceReplayStep::MalformedInput,
            status_code: 400,
            error_code: Some(VOICE_ERROR_EMPTY_TRANSCRIPT.to_string()),
            response_body: json!({"status":"rejected","reason":"empty_transcript"}),
        };
    }

    match case.mode {
        VoiceFixtureMode::WakeWord => {
            if !contains_wake_word(transcript, &wake_word) {
                return VoiceReplayResult {
                    step: VoiceReplayStep::MalformedInput,
                    status_code: 422,
                    error_code: Some(VOICE_ERROR_INVALID_WAKE_WORD.to_string()),
                    response_body: json!({"status":"rejected","reason":"wake_word_not_detected"}),
                };
            }
            VoiceReplayResult {
                step: VoiceReplayStep::Success,
                status_code: 202,
                error_code: None,
                response_body: json!({
                    "status":"accepted",
                    "mode":"wake_word",
                    "wake_word":wake_word,
                    "wake_detected":true
                }),
            }
        }
        VoiceFixtureMode::Turn => {
            let utterance = extract_utterance(transcript, &wake_word);
            if utterance.is_empty() {
                return VoiceReplayResult {
                    step: VoiceReplayStep::MalformedInput,
                    status_code: 400,
                    error_code: Some(VOICE_ERROR_EMPTY_TRANSCRIPT.to_string()),
                    response_body: json!({"status":"rejected","reason":"empty_utterance"}),
                };
            }
            VoiceReplayResult {
                step: VoiceReplayStep::Success,
                status_code: 202,
                error_code: None,
                response_body: json!({
                    "status":"accepted",
                    "mode":"turn",
                    "wake_word":wake_word,
                    "utterance":utterance,
                    "locale":locale,
                    "speaker_id":case.speaker_id.trim(),
                }),
            }
        }
    }
}

pub fn validate_voice_case_result_against_contract(
    case: &VoiceContractCase,
    result: &VoiceReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        VoiceOutcomeKind::Success => VoiceReplayStep::Success,
        VoiceOutcomeKind::MalformedInput => VoiceReplayStep::MalformedInput,
        VoiceOutcomeKind::RetryableFailure => VoiceReplayStep::RetryableFailure,
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
        VoiceOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
        }
        VoiceOutcomeKind::MalformedInput | VoiceOutcomeKind::RetryableFailure => {
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
pub fn run_voice_contract_replay<D: VoiceContractDriver>(
    fixture: &VoiceContractFixture,
    driver: &mut D,
) -> Result<VoiceReplaySummary> {
    validate_voice_contract_fixture(fixture)?;

    let mut summary = VoiceReplaySummary {
        discovered_cases: fixture.cases.len(),
        ..VoiceReplaySummary::default()
    };

    for case in &fixture.cases {
        let result = driver.apply_case(case)?;
        validate_voice_case_result_against_contract(case, &result)?;
        match case.expected.outcome {
            VoiceOutcomeKind::Success => {
                summary.success_cases = summary.success_cases.saturating_add(1)
            }
            VoiceOutcomeKind::MalformedInput => {
                summary.malformed_cases = summary.malformed_cases.saturating_add(1)
            }
            VoiceOutcomeKind::RetryableFailure => {
                summary.retryable_failures = summary.retryable_failures.saturating_add(1)
            }
        }
    }
    Ok(summary)
}

fn validate_voice_case(case: &VoiceContractCase, index: usize) -> Result<()> {
    if case.schema_version != VOICE_CONTRACT_SCHEMA_VERSION {
        bail!(
            "fixture case index {} has unsupported schema_version {} (expected {})",
            index,
            case.schema_version,
            VOICE_CONTRACT_SCHEMA_VERSION
        );
    }
    if case.case_id.trim().is_empty() {
        bail!("fixture case index {} has empty case_id", index);
    }
    if case.expected.status_code == 0 {
        bail!(
            "fixture case '{}' has invalid expected.status_code=0",
            case.case_id
        );
    }
    if case.expected.outcome != VoiceOutcomeKind::MalformedInput {
        if case.wake_word.trim().is_empty() {
            bail!(
                "fixture case '{}' non-malformed outcome requires wake_word",
                case.case_id
            );
        }
        if case.locale.trim().is_empty() {
            bail!(
                "fixture case '{}' non-malformed outcome requires locale",
                case.case_id
            );
        }
        if case.transcript.trim().is_empty() {
            bail!(
                "fixture case '{}' non-malformed outcome requires transcript",
                case.case_id
            );
        }
    }

    if case.simulate_retryable_failure
        && case.expected.outcome != VoiceOutcomeKind::RetryableFailure
    {
        bail!(
            "fixture case '{}' sets simulate_retryable_failure=true but expected outcome is {:?}",
            case.case_id,
            case.expected.outcome
        );
    }
    if case.expected.outcome == VoiceOutcomeKind::RetryableFailure
        && !case.simulate_retryable_failure
    {
        bail!(
            "fixture case '{}' expects retryable_failure but simulate_retryable_failure=false",
            case.case_id
        );
    }

    validate_voice_expectation(case)?;
    Ok(())
}

fn validate_voice_expectation(case: &VoiceContractCase) -> Result<()> {
    if !case.expected.response_body.is_object() {
        bail!(
            "fixture case '{}' expected.response_body must be an object",
            case.case_id
        );
    }
    match case.expected.outcome {
        VoiceOutcomeKind::Success => {
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
        VoiceOutcomeKind::MalformedInput | VoiceOutcomeKind::RetryableFailure => {
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

fn contains_wake_word(transcript: &str, wake_word: &str) -> bool {
    transcript.to_ascii_lowercase().contains(wake_word)
}

fn extract_utterance(transcript: &str, wake_word: &str) -> String {
    let trimmed = transcript.trim();
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.starts_with(wake_word) {
        let suffix = &trimmed[wake_word.len()..];
        return suffix
            .trim_start_matches(|ch: char| {
                ch.is_ascii_whitespace() || ch == ',' || ch == ':' || ch == '-'
            })
            .trim()
            .to_string();
    }
    trimmed.to_string()
}

fn normalize_wake_word(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn supported_wake_words() -> BTreeSet<&'static str> {
    ["tau", "hey tau"].into_iter().collect()
}

fn supported_locales() -> BTreeSet<&'static str> {
    ["en-US", "en-GB"].into_iter().collect()
}

fn supported_error_codes() -> BTreeSet<&'static str> {
    [
        VOICE_ERROR_EMPTY_TRANSCRIPT,
        VOICE_ERROR_INVALID_WAKE_WORD,
        VOICE_ERROR_INVALID_LOCALE,
        VOICE_ERROR_BACKEND_UNAVAILABLE,
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
        evaluate_voice_case, load_voice_contract_fixture, parse_voice_contract_fixture,
        run_voice_contract_replay, VoiceContractCase, VoiceContractDriver, VoiceReplayResult,
        VoiceReplayStep, VOICE_ERROR_INVALID_WAKE_WORD,
    };

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("voice-contract")
            .join(name)
    }

    #[derive(Default)]
    struct DeterministicVoiceDriver;

    impl VoiceContractDriver for DeterministicVoiceDriver {
        fn apply_case(&mut self, case: &VoiceContractCase) -> Result<VoiceReplayResult> {
            Ok(evaluate_voice_case(case))
        }
    }

    #[test]
    fn unit_parse_voice_contract_fixture_rejects_unsupported_schema() {
        let raw = r#"{
  "schema_version": 99,
  "name": "voice-invalid-schema",
  "cases": [
    {
      "schema_version": 1,
      "case_id": "voice-success",
      "mode": "turn",
      "wake_word": "tau",
      "transcript": "tau open dashboard",
      "locale": "en-US",
      "speaker_id": "ops",
      "expected": {
        "outcome": "success",
        "status_code": 202,
        "response_body": {
          "status": "accepted",
          "mode": "turn",
          "wake_word": "tau",
          "utterance": "open dashboard",
          "locale": "en-US",
          "speaker_id": "ops"
        }
      }
    }
  ]
}"#;
        let error = parse_voice_contract_fixture(raw).expect_err("schema should fail");
        assert!(error
            .to_string()
            .contains("unsupported voice contract schema version"));
    }

    #[test]
    fn unit_validate_voice_contract_fixture_rejects_duplicate_case_id() {
        let error = load_voice_contract_fixture(&fixture_path("invalid-duplicate-case-id.json"))
            .expect_err("duplicate case_id fixture should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("duplicate case_id"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn functional_fixture_loads_success_malformed_and_retryable_cases() {
        let fixture = load_voice_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.cases.len(), 3);
        assert_eq!(fixture.cases[0].case_id, "voice-success-turn");
        assert_eq!(
            fixture.cases[1].case_id,
            "voice-malformed-invalid-wake-word"
        );
        assert_eq!(fixture.cases[2].case_id, "voice-retryable-backend");
    }

    #[test]
    fn integration_voice_contract_replay_is_deterministic_across_reloads() {
        let fixture_path = fixture_path("mixed-outcomes.json");
        let fixture_a = load_voice_contract_fixture(&fixture_path).expect("load fixture a");
        let fixture_b = load_voice_contract_fixture(&fixture_path).expect("load fixture b");
        let mut driver_a = DeterministicVoiceDriver;
        let mut driver_b = DeterministicVoiceDriver;

        let summary_a =
            run_voice_contract_replay(&fixture_a, &mut driver_a).expect("replay fixture a");
        let summary_b =
            run_voice_contract_replay(&fixture_b, &mut driver_b).expect("replay fixture b");
        assert_eq!(summary_a, summary_b);
        assert_eq!(summary_a.discovered_cases, 3);
        assert_eq!(summary_a.success_cases, 1);
        assert_eq!(summary_a.malformed_cases, 1);
        assert_eq!(summary_a.retryable_failures, 1);
    }

    #[test]
    fn regression_fixture_rejects_unsupported_error_code() {
        let error = load_voice_contract_fixture(&fixture_path("invalid-error-code.json"))
            .expect_err("unsupported error code should fail");
        let rendered = format!("{error:#}");
        assert!(
            rendered.contains("unsupported error_code"),
            "unexpected error output: {rendered}"
        );
    }

    #[test]
    fn regression_voice_contract_replay_rejects_mismatched_expected_response_body() {
        let mut fixture = load_voice_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        fixture.cases[0].expected.response_body = json!({
            "status":"accepted",
            "mode":"turn",
            "wake_word":"tau",
            "utterance":"unexpected",
            "locale":"en-US",
            "speaker_id":"ops-1"
        });
        let mut driver = DeterministicVoiceDriver;
        let error =
            run_voice_contract_replay(&fixture, &mut driver).expect_err("replay should fail");
        assert!(error.to_string().contains("expected response_body"));
    }

    #[test]
    fn regression_voice_contract_evaluator_marks_unknown_wake_word_as_malformed() {
        let fixture = load_voice_contract_fixture(&fixture_path("mixed-outcomes.json"))
            .expect("load fixture");
        let result = evaluate_voice_case(&fixture.cases[1]);
        assert_eq!(result.step, VoiceReplayStep::MalformedInput);
        assert_eq!(result.status_code, 422);
        assert_eq!(
            result.error_code.as_deref(),
            Some(VOICE_ERROR_INVALID_WAKE_WORD)
        );
    }
}
