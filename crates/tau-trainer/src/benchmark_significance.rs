//! Reproducibility guards for benchmark significance reporting.
//!
//! These helpers intentionally provide deterministic guardrails instead of full
//! inferential statistics.

use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tracing::instrument;

/// Single significance observation emitted by benchmark tooling.
#[derive(Debug, Clone, PartialEq)]
pub struct SignificanceObservation {
    pub seed: u64,
    pub sample_size: usize,
    pub p_value: f64,
    pub reward_delta: f64,
}

impl SignificanceObservation {
    /// Creates a significance observation for reproducibility checks.
    pub fn new(seed: u64, sample_size: usize, p_value: f64, reward_delta: f64) -> Self {
        Self {
            seed,
            sample_size,
            p_value,
            reward_delta,
        }
    }
}

/// Configurable reproducibility/sensitivity thresholds.
#[derive(Debug, Clone, PartialEq)]
pub struct ReproducibilityBands {
    pub max_seed_p_value_range: f64,
    pub max_seed_reward_delta_range: f64,
    pub max_sample_p_value_drift: f64,
    pub max_sample_reward_delta_drift: f64,
}

impl Default for ReproducibilityBands {
    fn default() -> Self {
        Self {
            max_seed_p_value_range: 0.05,
            max_seed_reward_delta_range: 0.05,
            max_sample_p_value_drift: 0.04,
            max_sample_reward_delta_drift: 0.08,
        }
    }
}

/// Safety threshold and reproducibility policy for checkpoint promotion.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointPromotionPolicy {
    pub max_safety_regression: f64,
    pub require_seed_reproducibility: bool,
    pub require_sample_size_sensitivity: bool,
}

impl Default for CheckpointPromotionPolicy {
    fn default() -> Self {
        Self {
            max_safety_regression: 0.05,
            require_seed_reproducibility: true,
            require_sample_size_sensitivity: true,
        }
    }
}

/// Outcome of checkpoint promotion gate evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointPromotionDecision {
    pub promotion_allowed: bool,
    pub safety_regression: f64,
    pub max_safety_regression: f64,
    pub reason_codes: Vec<String>,
}

/// Aggregate seeded-run reproducibility summary at fixed sample size.
#[derive(Debug, Clone, PartialEq)]
pub struct SeedReproducibilityReport {
    pub sample_size: usize,
    pub run_count: usize,
    pub p_value_range: f64,
    pub reward_delta_range: f64,
    pub within_band: bool,
}

/// Sample-size sensitivity point.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleSizePoint {
    pub sample_size: usize,
    pub mean_p_value: f64,
    pub mean_reward_delta: f64,
}

/// Sample-size sensitivity summary for a single seed.
#[derive(Debug, Clone, PartialEq)]
pub struct SampleSizeSensitivityReport {
    pub seed: u64,
    pub points: Vec<SampleSizePoint>,
    pub max_p_value_drift: f64,
    pub max_reward_delta_drift: f64,
    pub within_band: bool,
}

/// Deterministic summary statistics for one benchmark sample vector.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryStatistics {
    pub count: usize,
    pub mean: f64,
    pub variance: f64,
    pub std_dev: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub alpha: f64,
}

impl SummaryStatistics {
    /// Serializes summary statistics to a machine-readable JSON object.
    pub fn to_json_value(&self) -> Value {
        json!({
            "count": self.count,
            "mean": self.mean,
            "variance": self.variance,
            "std_dev": self.std_dev,
            "ci_low": self.ci_low,
            "ci_high": self.ci_high,
            "alpha": self.alpha
        })
    }
}

/// Comparative baseline-vs-candidate significance report.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyImprovementReport {
    pub baseline: SummaryStatistics,
    pub candidate: SummaryStatistics,
    pub mean_delta: f64,
    pub delta_ci_low: f64,
    pub delta_ci_high: f64,
    pub pooled_std_dev: f64,
    pub cohens_d: f64,
    pub is_significant_improvement: bool,
    pub alpha: f64,
}

impl PolicyImprovementReport {
    /// Serializes the comparative report to machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "baseline": self.baseline.to_json_value(),
            "candidate": self.candidate.to_json_value(),
            "mean_delta": self.mean_delta,
            "delta_ci_low": self.delta_ci_low,
            "delta_ci_high": self.delta_ci_high,
            "pooled_std_dev": self.pooled_std_dev,
            "cohens_d": self.cohens_d,
            "is_significant_improvement": self.is_significant_improvement,
            "alpha": self.alpha
        })
    }
}

/// Computes deterministic summary statistics, including an approximate two-sided
/// confidence interval.
#[instrument(skip(samples), fields(sample_count = samples.len(), alpha = alpha))]
pub fn compute_summary_statistics(samples: &[f64], alpha: f64) -> Result<SummaryStatistics> {
    if samples.len() < 2 {
        bail!("summary statistics require at least two observations");
    }
    if !(0.0..1.0).contains(&alpha) || !alpha.is_finite() {
        bail!("alpha must be finite and in (0, 1)");
    }
    for value in samples {
        if !value.is_finite() {
            bail!("summary statistics require finite observations");
        }
    }

    let count = samples.len();
    let mean = samples.iter().sum::<f64>() / count as f64;
    let variance = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (count as f64 - 1.0);
    let std_dev = variance.sqrt();
    let z_score = z_score_for_alpha(alpha)?;
    let margin = z_score * (std_dev / (count as f64).sqrt());

    Ok(SummaryStatistics {
        count,
        mean,
        variance,
        std_dev,
        ci_low: mean - margin,
        ci_high: mean + margin,
        alpha,
    })
}

/// Compares baseline and candidate benchmark vectors and reports confidence for
/// improvement claims.
#[instrument(skip(baseline, candidate), fields(baseline_count = baseline.len(), candidate_count = candidate.len(), alpha = alpha))]
pub fn compare_policy_improvement(
    baseline: &[f64],
    candidate: &[f64],
    alpha: f64,
) -> Result<PolicyImprovementReport> {
    let baseline_stats = compute_summary_statistics(baseline, alpha)?;
    let candidate_stats = compute_summary_statistics(candidate, alpha)?;
    let mean_delta = candidate_stats.mean - baseline_stats.mean;

    let baseline_n = baseline_stats.count as f64;
    let candidate_n = candidate_stats.count as f64;
    let standard_error_delta =
        (baseline_stats.variance / baseline_n + candidate_stats.variance / candidate_n).sqrt();
    let z_score = z_score_for_alpha(alpha)?;
    let delta_margin = z_score * standard_error_delta;
    let delta_ci_low = mean_delta - delta_margin;
    let delta_ci_high = mean_delta + delta_margin;

    let pooled_variance = ((baseline_n - 1.0) * baseline_stats.variance
        + (candidate_n - 1.0) * candidate_stats.variance)
        / (baseline_n + candidate_n - 2.0);
    let pooled_std_dev = pooled_variance.max(0.0).sqrt();
    let cohens_d = if pooled_std_dev == 0.0 {
        0.0
    } else {
        mean_delta / pooled_std_dev
    };

    Ok(PolicyImprovementReport {
        baseline: baseline_stats,
        candidate: candidate_stats,
        mean_delta,
        delta_ci_low,
        delta_ci_high,
        pooled_std_dev,
        cohens_d,
        is_significant_improvement: delta_ci_low > 0.0,
        alpha,
    })
}

/// Evaluates fixed-sample reproducibility across multiple seeds.
pub fn evaluate_seed_reproducibility(
    observations: &[SignificanceObservation],
    sample_size: usize,
    bands: &ReproducibilityBands,
) -> Result<SeedReproducibilityReport> {
    if observations.is_empty() {
        bail!("seed reproducibility evaluation requires observations");
    }

    let filtered: Vec<&SignificanceObservation> = observations
        .iter()
        .filter(|item| item.sample_size == sample_size)
        .collect();

    if filtered.len() < 2 {
        bail!(
            "seed reproducibility requires at least two observations for sample_size={sample_size}"
        );
    }

    for item in &filtered {
        if !(0.0..=1.0).contains(&item.p_value) || !item.p_value.is_finite() {
            bail!("p_value must be finite and within [0, 1]");
        }
        if !item.reward_delta.is_finite() {
            bail!("reward_delta must be finite");
        }
    }

    let p_value_range = range(filtered.iter().map(|item| item.p_value));
    let reward_delta_range = range(filtered.iter().map(|item| item.reward_delta));
    let within_band = p_value_range <= bands.max_seed_p_value_range
        && reward_delta_range <= bands.max_seed_reward_delta_range;

    Ok(SeedReproducibilityReport {
        sample_size,
        run_count: filtered.len(),
        p_value_range,
        reward_delta_range,
        within_band,
    })
}

/// Evaluates sensitivity drift as sample size changes for a fixed seed.
pub fn evaluate_sample_size_sensitivity(
    observations: &[SignificanceObservation],
    seed: u64,
    bands: &ReproducibilityBands,
) -> Result<SampleSizeSensitivityReport> {
    if observations.is_empty() {
        bail!("sample-size sensitivity evaluation requires observations");
    }

    let mut grouped: BTreeMap<usize, Vec<&SignificanceObservation>> = BTreeMap::new();
    for observation in observations.iter().filter(|item| item.seed == seed) {
        if !(0.0..=1.0).contains(&observation.p_value) || !observation.p_value.is_finite() {
            bail!("p_value must be finite and within [0, 1]");
        }
        if !observation.reward_delta.is_finite() {
            bail!("reward_delta must be finite");
        }
        grouped
            .entry(observation.sample_size)
            .or_default()
            .push(observation);
    }

    if grouped.len() < 2 {
        bail!("sample-size sensitivity requires at least two sample sizes for seed={seed}");
    }

    let points: Vec<SampleSizePoint> = grouped
        .into_iter()
        .map(|(sample_size, rows)| SampleSizePoint {
            sample_size,
            mean_p_value: rows.iter().map(|item| item.p_value).sum::<f64>() / rows.len() as f64,
            mean_reward_delta: rows.iter().map(|item| item.reward_delta).sum::<f64>()
                / rows.len() as f64,
        })
        .collect();

    let mut max_p_value_drift = 0.0_f64;
    let mut max_reward_delta_drift = 0.0_f64;
    for pair in points.windows(2) {
        let first = &pair[0];
        let second = &pair[1];
        let p_drift = (second.mean_p_value - first.mean_p_value).abs();
        let reward_drift = (second.mean_reward_delta - first.mean_reward_delta).abs();
        if p_drift > max_p_value_drift {
            max_p_value_drift = p_drift;
        }
        if reward_drift > max_reward_delta_drift {
            max_reward_delta_drift = reward_drift;
        }
    }
    let within_band = max_p_value_drift <= bands.max_sample_p_value_drift
        && max_reward_delta_drift <= bands.max_sample_reward_delta_drift;

    Ok(SampleSizeSensitivityReport {
        seed,
        points,
        max_p_value_drift,
        max_reward_delta_drift,
        within_band,
    })
}

/// Evaluates whether checkpoint promotion should proceed under safety and
/// reproducibility requirements.
pub fn evaluate_checkpoint_promotion_gate(
    baseline_mean_safety_penalty: f64,
    candidate_mean_safety_penalty: f64,
    seeded_reproducibility_within_band: bool,
    sample_sensitivity_within_band: bool,
    policy: &CheckpointPromotionPolicy,
) -> Result<CheckpointPromotionDecision> {
    if !baseline_mean_safety_penalty.is_finite() || !candidate_mean_safety_penalty.is_finite() {
        bail!("safety penalties must be finite");
    }
    if !policy.max_safety_regression.is_finite() || policy.max_safety_regression < 0.0 {
        bail!("max_safety_regression must be finite and non-negative");
    }

    let safety_regression = candidate_mean_safety_penalty - baseline_mean_safety_penalty;
    let mut reason_codes = Vec::new();

    if safety_regression > policy.max_safety_regression {
        reason_codes.push("checkpoint_promotion_blocked_safety_regression".to_string());
    }
    if policy.require_seed_reproducibility && !seeded_reproducibility_within_band {
        reason_codes.push("checkpoint_promotion_blocked_seed_reproducibility".to_string());
    }
    if policy.require_sample_size_sensitivity && !sample_sensitivity_within_band {
        reason_codes.push("checkpoint_promotion_blocked_sample_sensitivity".to_string());
    }

    Ok(CheckpointPromotionDecision {
        promotion_allowed: reason_codes.is_empty(),
        safety_regression,
        max_safety_regression: policy.max_safety_regression,
        reason_codes,
    })
}

fn range(values: impl Iterator<Item = f64>) -> f64 {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for value in values {
        if value < min {
            min = value;
        }
        if value > max {
            max = value;
        }
    }
    if min.is_infinite() || max.is_infinite() {
        0.0
    } else {
        max - min
    }
}

fn z_score_for_alpha(alpha: f64) -> Result<f64> {
    if (alpha - 0.10).abs() < 1e-12 {
        Ok(1.645)
    } else if (alpha - 0.05).abs() < 1e-12 {
        Ok(1.96)
    } else if (alpha - 0.01).abs() < 1e-12 {
        Ok(2.576)
    } else {
        bail!("unsupported alpha {alpha}; supported values are 0.10, 0.05, 0.01")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compare_policy_improvement, compute_summary_statistics, evaluate_checkpoint_promotion_gate,
        evaluate_sample_size_sensitivity, evaluate_seed_reproducibility, CheckpointPromotionPolicy,
        ReproducibilityBands, SignificanceObservation,
    };
    use serde_json::Value;

    #[test]
    fn seeded_reproducibility_passes_when_ranges_within_band() {
        let observations = vec![
            SignificanceObservation::new(1, 200, 0.022, 0.18),
            SignificanceObservation::new(2, 200, 0.028, 0.20),
            SignificanceObservation::new(3, 200, 0.025, 0.19),
        ];

        let bands = ReproducibilityBands {
            max_seed_p_value_range: 0.02,
            max_seed_reward_delta_range: 0.03,
            ..ReproducibilityBands::default()
        };

        let report =
            evaluate_seed_reproducibility(&observations, 200, &bands).expect("seed report");
        assert!(report.within_band);
        assert_eq!(report.run_count, 3);
    }

    #[test]
    fn regression_seeded_reproducibility_flags_out_of_band_ranges() {
        let observations = vec![
            SignificanceObservation::new(11, 200, 0.01, 0.10),
            SignificanceObservation::new(12, 200, 0.14, 0.30),
            SignificanceObservation::new(13, 200, 0.03, 0.18),
        ];
        let bands = ReproducibilityBands::default();
        let report =
            evaluate_seed_reproducibility(&observations, 200, &bands).expect("seed report");
        assert!(!report.within_band);
    }

    #[test]
    fn sample_size_sensitivity_passes_when_drift_within_band() {
        let observations = vec![
            SignificanceObservation::new(7, 128, 0.045, 0.12),
            SignificanceObservation::new(7, 256, 0.038, 0.14),
            SignificanceObservation::new(7, 512, 0.033, 0.16),
        ];
        let bands = ReproducibilityBands::default();
        let report =
            evaluate_sample_size_sensitivity(&observations, 7, &bands).expect("sample report");
        assert!(report.within_band);
        assert_eq!(report.points.len(), 3);
    }

    #[test]
    fn regression_sample_size_sensitivity_flags_excessive_drift() {
        let observations = vec![
            SignificanceObservation::new(9, 128, 0.02, 0.09),
            SignificanceObservation::new(9, 256, 0.12, 0.02),
            SignificanceObservation::new(9, 512, 0.18, -0.01),
        ];
        let bands = ReproducibilityBands::default();
        let report =
            evaluate_sample_size_sensitivity(&observations, 9, &bands).expect("sample report");
        assert!(!report.within_band);
    }

    #[test]
    fn unit_checkpoint_promotion_gate_blocks_on_safety_regression_threshold() {
        let policy = CheckpointPromotionPolicy {
            max_safety_regression: 0.05,
            ..CheckpointPromotionPolicy::default()
        };
        let decision =
            evaluate_checkpoint_promotion_gate(0.10, 0.19, true, true, &policy).expect("decision");
        assert!(!decision.promotion_allowed);
        assert!((decision.safety_regression - 0.09).abs() < 1e-9);
        assert!(decision
            .reason_codes
            .iter()
            .any(|code| code == "checkpoint_promotion_blocked_safety_regression"));
    }

    #[test]
    fn integration_checkpoint_promotion_gate_requires_significance_stability() {
        let observations = vec![
            SignificanceObservation::new(1, 256, 0.024, 0.15),
            SignificanceObservation::new(2, 256, 0.029, 0.16),
            SignificanceObservation::new(7, 128, 0.031, 0.14),
            SignificanceObservation::new(7, 256, 0.027, 0.15),
            SignificanceObservation::new(7, 512, 0.023, 0.16),
        ];
        let bands = ReproducibilityBands::default();
        let seed_report =
            evaluate_seed_reproducibility(&observations, 256, &bands).expect("seed report");
        let sample_report =
            evaluate_sample_size_sensitivity(&observations, 7, &bands).expect("sample report");
        let policy = CheckpointPromotionPolicy::default();
        let decision = evaluate_checkpoint_promotion_gate(
            0.10,
            0.12,
            seed_report.within_band,
            sample_report.within_band,
            &policy,
        )
        .expect("decision");
        assert!(decision.promotion_allowed);
        assert!(decision.reason_codes.is_empty());
    }

    #[test]
    fn spec_c01_summary_statistics_match_reference_vector() {
        let stats =
            compute_summary_statistics(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.05).expect("summary stats");
        assert_eq!(stats.count, 5);
        assert!((stats.mean - 3.0).abs() < 1e-12);
        assert!((stats.variance - 2.5).abs() < 1e-12);
        assert!((stats.std_dev - 1.581_138_830_084_189_8).abs() < 1e-12);
        assert!((stats.ci_low - 1.614_070_708_874_366_9).abs() < 1e-9);
        assert!((stats.ci_high - 4.385_929_291_125_633).abs() < 1e-9);
    }

    #[test]
    fn spec_c02_policy_comparison_reports_significant_improvement() {
        let baseline = [0.10, 0.15, 0.20, 0.18, 0.16];
        let candidate = [0.32, 0.35, 0.31, 0.36, 0.33];
        let report = compare_policy_improvement(&baseline, &candidate, 0.05).expect("report");
        assert!(report.mean_delta > 0.0);
        assert!(report.delta_ci_low > 0.0);
        assert!(report.is_significant_improvement);
    }

    #[test]
    fn spec_c03_significance_report_is_machine_readable() {
        let baseline = [0.21, 0.24, 0.22, 0.20, 0.25];
        let candidate = [0.27, 0.29, 0.31, 0.28, 0.30];
        let report = compare_policy_improvement(&baseline, &candidate, 0.05).expect("report");
        let json = report.to_json_value();

        let Value::Object(map) = json else {
            panic!("report must serialize to JSON object");
        };
        assert!(map.contains_key("baseline"));
        assert!(map.contains_key("candidate"));
        assert!(map.contains_key("mean_delta"));
        assert!(map.contains_key("delta_ci_low"));
        assert!(map.contains_key("delta_ci_high"));
        assert!(map.contains_key("is_significant_improvement"));
    }

    #[test]
    fn regression_significance_statistics_reject_invalid_inputs() {
        let empty = compute_summary_statistics(&[], 0.05).expect_err("empty sample must fail");
        assert!(empty.to_string().contains("at least two observations"));

        let non_finite =
            compute_summary_statistics(&[1.0, f64::NAN, 2.0], 0.05).expect_err("nan must fail");
        assert!(non_finite.to_string().contains("finite"));

        let mismatched = compare_policy_improvement(&[0.1], &[0.2], 0.05)
            .expect_err("single-entry vectors must fail");
        assert!(mismatched.to_string().contains("at least two observations"));
    }
}
