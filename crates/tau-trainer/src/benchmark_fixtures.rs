//! RL benchmark fixture loading and validation contracts.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Fixture family for benchmark workloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum BenchmarkFixtureFamily {
    Reasoning,
    ToolUse,
}

impl BenchmarkFixtureFamily {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "reasoning" => Ok(Self::Reasoning),
            "tool_use" => Ok(Self::ToolUse),
            _ => bail!("unsupported fixture family `{raw}`"),
        }
    }
}

/// One benchmark case with deterministic seed and scoring rubric.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkFixtureCase {
    pub case_id: String,
    pub seed: u64,
    pub prompt: String,
    pub expected_outcome: String,
    pub scoring_rubric: BTreeMap<String, f64>,
}

/// A fixture suite representing one workload family.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkFixtureSuite {
    pub suite_id: String,
    pub family: BenchmarkFixtureFamily,
    pub description: String,
    pub cases: Vec<BenchmarkFixtureCase>,
}

/// Loads a benchmark fixture suite from JSON.
pub fn load_benchmark_fixture_suite(path: &Path) -> Result<BenchmarkFixtureSuite> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse fixture JSON {}", path.display()))?;
    let suite = parse_fixture_suite(&value)?;
    validate_benchmark_fixture_suite(&suite)?;
    Ok(suite)
}

/// Validates fixture structural and rubric invariants.
pub fn validate_benchmark_fixture_suite(suite: &BenchmarkFixtureSuite) -> Result<()> {
    if suite.suite_id.trim().is_empty() {
        bail!("suite_id must not be empty");
    }
    if suite.description.trim().is_empty() {
        bail!("description must not be empty");
    }
    if suite.cases.is_empty() {
        bail!("fixture suite must include at least one case");
    }

    let mut seen_case_ids = BTreeSet::new();
    for case in &suite.cases {
        if case.case_id.trim().is_empty() {
            bail!("case_id must not be empty");
        }
        if !seen_case_ids.insert(case.case_id.clone()) {
            bail!("duplicate case_id `{}`", case.case_id);
        }
        if case.seed == 0 {
            bail!("seed must be > 0 for case `{}`", case.case_id);
        }
        if case.prompt.trim().is_empty() {
            bail!("prompt must not be empty for case `{}`", case.case_id);
        }
        if case.expected_outcome.trim().is_empty() {
            bail!(
                "expected_outcome must not be empty for case `{}`",
                case.case_id
            );
        }
        if case.scoring_rubric.is_empty() {
            bail!(
                "scoring_rubric must not be empty for case `{}`",
                case.case_id
            );
        }

        let mut total = 0.0_f64;
        for (dimension, weight) in &case.scoring_rubric {
            if dimension.trim().is_empty() {
                bail!("scoring_rubric dimension must not be empty");
            }
            if !weight.is_finite() {
                bail!(
                    "scoring_rubric weight for `{dimension}` in case `{}` must be finite",
                    case.case_id
                );
            }
            if *weight < 0.0 {
                bail!(
                    "scoring_rubric weight for `{dimension}` in case `{}` must be non-negative",
                    case.case_id
                );
            }
            total += weight;
        }
        if (total - 1.0).abs() > 1e-6 {
            bail!(
                "scoring_rubric weights must sum to 1.0 for case `{}` (observed {total:.6})",
                case.case_id
            );
        }
    }

    Ok(())
}

fn parse_fixture_suite(value: &Value) -> Result<BenchmarkFixtureSuite> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("malformed fixture: root must be object"))?;

    let suite_id = required_str(object, "suite_id")?;
    let family = BenchmarkFixtureFamily::parse(required_str(object, "family")?.as_str())?;
    let description = required_str(object, "description")?;
    let cases_value = object
        .get("cases")
        .ok_or_else(|| anyhow::anyhow!("malformed fixture: missing `cases` field"))?;
    let cases_array = cases_value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("malformed fixture: `cases` must be an array"))?;

    let mut cases = Vec::with_capacity(cases_array.len());
    for (index, case_value) in cases_array.iter().enumerate() {
        cases.push(parse_case(case_value, index)?);
    }

    Ok(BenchmarkFixtureSuite {
        suite_id,
        family,
        description,
        cases,
    })
}

fn parse_case(value: &Value, index: usize) -> Result<BenchmarkFixtureCase> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("malformed fixture: case[{index}] must be object"))?;

    let case_id = required_str(object, "case_id")?;
    let seed = object
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("malformed fixture: case[{index}] missing `seed` u64"))?;
    let prompt = required_str(object, "prompt")?;
    let expected_outcome = required_str(object, "expected_outcome")?;
    let rubric_object = object
        .get("scoring_rubric")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            anyhow::anyhow!("malformed fixture: case[{index}] missing `scoring_rubric` object")
        })?;

    let mut scoring_rubric = BTreeMap::new();
    for (dimension, weight) in rubric_object {
        let numeric_weight = weight.as_f64().ok_or_else(|| {
            anyhow::anyhow!(
                "malformed fixture: scoring_rubric `{dimension}` in case[{index}] must be number"
            )
        })?;
        scoring_rubric.insert(dimension.clone(), numeric_weight);
    }

    Ok(BenchmarkFixtureCase {
        case_id,
        seed,
        prompt,
        expected_outcome,
        scoring_rubric,
    })
}

fn required_str(object: &serde_json::Map<String, Value>, field: &'static str) -> Result<String> {
    let raw = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("malformed fixture: missing `{field}` string"))?;
    if raw.trim().is_empty() {
        bail!("malformed fixture: `{field}` must not be empty");
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::{load_benchmark_fixture_suite, validate_benchmark_fixture_suite};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tau-coding-agent/testdata/rl-benchmark-fixtures")
            .join(name)
    }

    #[test]
    fn spec_c01_fixture_families_are_reproducible_and_diverse() {
        let reasoning =
            load_benchmark_fixture_suite(&fixture_path("reasoning-suite.json")).expect("reasoning");
        let tool_use =
            load_benchmark_fixture_suite(&fixture_path("tool-use-suite.json")).expect("tool-use");

        let family_set: BTreeSet<_> = [reasoning.family, tool_use.family].into_iter().collect();
        assert_eq!(
            family_set.len(),
            2,
            "expected reasoning + tool-use families"
        );

        let reasoning_ids: Vec<_> = reasoning
            .cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect();
        let reasoning_seeds: Vec<_> = reasoning.cases.iter().map(|case| case.seed).collect();
        let tool_use_ids: Vec<_> = tool_use
            .cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect();
        let tool_use_seeds: Vec<_> = tool_use.cases.iter().map(|case| case.seed).collect();
        assert_eq!(
            reasoning_ids,
            vec!["reasoning_chain_of_thought_001", "reasoning_multi_step_002"]
        );
        assert_eq!(reasoning_seeds, vec![10101, 20202]);
        assert_eq!(
            tool_use_ids,
            vec![
                "tool_use_repository_search_001",
                "tool_use_patch_workflow_002"
            ]
        );
        assert_eq!(tool_use_seeds, vec![30303, 40404]);
    }

    #[test]
    fn spec_c02_fixture_scoring_rubrics_are_normalized() {
        for fixture_name in ["reasoning-suite.json", "tool-use-suite.json"] {
            let suite = load_benchmark_fixture_suite(&fixture_path(fixture_name))
                .expect("fixture should load");
            validate_benchmark_fixture_suite(&suite).expect("fixture should validate");
        }
    }

    #[test]
    fn spec_c03_loader_rejects_malformed_fixture_contracts() {
        let duplicate = fixture_path("invalid-duplicate-case-id.json");
        let duplicate_error = load_benchmark_fixture_suite(&duplicate)
            .expect_err("duplicate case fixture should fail");
        assert!(
            duplicate_error.to_string().contains("duplicate case_id"),
            "unexpected duplicate-case error: {duplicate_error}"
        );

        let invalid_weight = fixture_path("invalid-rubric-weight.json");
        let invalid_weight_error = load_benchmark_fixture_suite(&invalid_weight)
            .expect_err("invalid-weight fixture should fail");
        assert!(
            invalid_weight_error
                .to_string()
                .contains("must be non-negative")
                || invalid_weight_error.to_string().contains("must sum to 1.0"),
            "unexpected invalid-weight error: {invalid_weight_error}"
        );

        let missing_field = fixture_path("invalid-missing-field.json");
        let missing_field_error = load_benchmark_fixture_suite(&missing_field)
            .expect_err("missing-field fixture should fail");
        assert!(
            missing_field_error
                .to_string()
                .contains("malformed fixture: missing"),
            "unexpected missing-field error: {missing_field_error}"
        );
    }
}
