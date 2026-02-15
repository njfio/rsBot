//! Reproducibility guards for benchmark significance reporting.
//!
//! These helpers intentionally provide deterministic guardrails instead of full
//! inferential statistics.

use anyhow::{bail, Result};
use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
    use super::{
        evaluate_checkpoint_promotion_gate, evaluate_sample_size_sensitivity,
        evaluate_seed_reproducibility, CheckpointPromotionPolicy, ReproducibilityBands,
        SignificanceObservation,
    };

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
}
