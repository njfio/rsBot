//! Deterministic benchmark suite driver and repeatability checks.

use crate::benchmark_fixtures::{BenchmarkFixtureCase, BenchmarkFixtureSuite};
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use tracing::instrument;

/// Per-case benchmark observation emitted by a benchmark run.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkObservation {
    pub case_id: String,
    pub seed: u64,
    pub score: f64,
}

/// Aggregate output from a benchmark suite execution.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkRunReport {
    pub suite_id: String,
    pub observations: Vec<BenchmarkObservation>,
    pub mean_score: f64,
}

/// Per-case repeatability drift summary across benchmark runs.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseRepeatability {
    pub case_id: String,
    pub min_score: f64,
    pub max_score: f64,
    pub range: f64,
    pub within_tolerance: bool,
}

/// Repeatability report for one or more benchmark runs.
#[derive(Debug, Clone, PartialEq)]
pub struct RepeatabilityReport {
    pub case_reports: Vec<CaseRepeatability>,
    pub max_observed_range: f64,
    pub tolerance: f64,
    pub within_tolerance: bool,
}

/// Scorer contract for deterministic benchmark case evaluation.
pub trait BenchmarkScorer {
    fn score_case(&self, case: &BenchmarkFixtureCase) -> Result<f64>;
}

/// Executes one benchmark suite with the provided scorer.
#[instrument(skip(suite, scorer), fields(suite_id = %suite.suite_id, case_count = suite.cases.len()))]
pub fn run_benchmark_suite<S: BenchmarkScorer>(
    suite: &BenchmarkFixtureSuite,
    scorer: &S,
) -> Result<BenchmarkRunReport> {
    if suite.cases.is_empty() {
        bail!("benchmark suite must contain at least one case");
    }

    let mut observations = Vec::with_capacity(suite.cases.len());
    for case in &suite.cases {
        let score = scorer.score_case(case)?;
        if !score.is_finite() {
            bail!(
                "benchmark scorer produced non-finite score for `{}`",
                case.case_id
            );
        }
        observations.push(BenchmarkObservation {
            case_id: case.case_id.clone(),
            seed: case.seed,
            score,
        });
    }

    observations.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    let mean_score =
        observations.iter().map(|item| item.score).sum::<f64>() / observations.len() as f64;

    Ok(BenchmarkRunReport {
        suite_id: suite.suite_id.clone(),
        observations,
        mean_score,
    })
}

/// Evaluates repeatability variance across benchmark run reports.
#[instrument(skip(run_reports), fields(run_count = run_reports.len(), tolerance = tolerance))]
pub fn evaluate_repeatability(
    run_reports: &[BenchmarkRunReport],
    tolerance: f64,
) -> Result<RepeatabilityReport> {
    if run_reports.len() < 2 {
        bail!("repeatability evaluation requires at least two run reports");
    }
    if !tolerance.is_finite() || tolerance < 0.0 {
        bail!("repeatability tolerance must be finite and non-negative");
    }

    let reference_suite = &run_reports[0].suite_id;
    let reference_case_ids: Vec<String> = run_reports[0]
        .observations
        .iter()
        .map(|obs| obs.case_id.clone())
        .collect();
    if reference_case_ids.is_empty() {
        bail!("run reports must contain observations");
    }

    let mut per_case_scores: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for report in run_reports {
        if report.suite_id != *reference_suite {
            bail!("all run reports must target the same suite");
        }
        let report_case_ids: Vec<String> = report
            .observations
            .iter()
            .map(|obs| obs.case_id.clone())
            .collect();
        if report_case_ids != reference_case_ids {
            bail!("run reports must contain identical case ordering and IDs");
        }

        for observation in &report.observations {
            if !observation.score.is_finite() {
                bail!(
                    "repeatability evaluation requires finite scores (case `{}`)",
                    observation.case_id
                );
            }
            per_case_scores
                .entry(observation.case_id.clone())
                .or_default()
                .push(observation.score);
        }
    }

    let mut case_reports = Vec::new();
    let mut max_observed_range = 0.0_f64;
    for (case_id, scores) in per_case_scores {
        let min_score = scores.iter().copied().fold(f64::INFINITY, f64::min);
        let max_score = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = max_score - min_score;
        if range > max_observed_range {
            max_observed_range = range;
        }
        case_reports.push(CaseRepeatability {
            case_id,
            min_score,
            max_score,
            range,
            within_tolerance: range <= tolerance,
        });
    }

    let within_tolerance = case_reports.iter().all(|case| case.within_tolerance);
    Ok(RepeatabilityReport {
        case_reports,
        max_observed_range,
        tolerance,
        within_tolerance,
    })
}

#[cfg(test)]
mod tests {
    use super::{evaluate_repeatability, run_benchmark_suite, BenchmarkRunReport, BenchmarkScorer};
    use crate::benchmark_fixtures::{load_benchmark_fixture_suite, BenchmarkFixtureCase};
    use std::path::PathBuf;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tau-coding-agent/testdata/rl-benchmark-fixtures")
            .join(name)
    }

    struct DeterministicScorer;

    impl BenchmarkScorer for DeterministicScorer {
        fn score_case(&self, case: &BenchmarkFixtureCase) -> anyhow::Result<f64> {
            let rubric_total = case.scoring_rubric.values().sum::<f64>();
            let seed_component = (case.seed % 1000) as f64 / 10_000.0;
            let case_component =
                (case.case_id.bytes().map(u64::from).sum::<u64>() % 100) as f64 / 1_000.0;
            Ok((rubric_total * 0.7 + seed_component + case_component).min(1.0))
        }
    }

    #[test]
    fn spec_c01_benchmark_driver_is_deterministic_for_seeded_fixture_suite() {
        let suite =
            load_benchmark_fixture_suite(&fixture_path("reasoning-suite.json")).expect("suite");
        let scorer = DeterministicScorer;

        let first = run_benchmark_suite(&suite, &scorer).expect("first run");
        let second = run_benchmark_suite(&suite, &scorer).expect("second run");

        assert_eq!(first.observations, second.observations);
        assert!((first.mean_score - second.mean_score).abs() < 1e-12);
    }

    #[test]
    fn spec_c02_repeatability_report_flags_out_of_tolerance_ranges() {
        let reports = vec![
            BenchmarkRunReport {
                suite_id: "suite".to_string(),
                observations: vec![
                    super::BenchmarkObservation {
                        case_id: "case-a".to_string(),
                        seed: 1,
                        score: 0.55,
                    },
                    super::BenchmarkObservation {
                        case_id: "case-b".to_string(),
                        seed: 2,
                        score: 0.61,
                    },
                ],
                mean_score: 0.58,
            },
            BenchmarkRunReport {
                suite_id: "suite".to_string(),
                observations: vec![
                    super::BenchmarkObservation {
                        case_id: "case-a".to_string(),
                        seed: 1,
                        score: 0.83,
                    },
                    super::BenchmarkObservation {
                        case_id: "case-b".to_string(),
                        seed: 2,
                        score: 0.60,
                    },
                ],
                mean_score: 0.715,
            },
        ];

        let report = evaluate_repeatability(&reports, 0.05).expect("repeatability report");
        assert!(!report.within_tolerance);
        assert!(report.max_observed_range > 0.05);
    }

    #[test]
    fn spec_c03_fixture_failure_paths_are_rejected_deterministically() {
        let error = load_benchmark_fixture_suite(&fixture_path("invalid-duplicate-case-id.json"))
            .expect_err("invalid fixture should fail");
        assert!(error.to_string().contains("duplicate case_id"));
    }
}
