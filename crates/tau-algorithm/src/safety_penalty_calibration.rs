//! Safety penalty coefficient calibration helpers for RL reward shaping.

use anyhow::{bail, Result};

/// Observation row from a safety-penalty calibration experiment.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyPenaltyCalibrationObservation {
    pub coefficient: f64,
    pub mean_reward_delta: f64,
    pub mean_safety_penalty: f64,
}

impl SafetyPenaltyCalibrationObservation {
    /// Creates a calibration observation.
    pub fn new(coefficient: f64, mean_reward_delta: f64, mean_safety_penalty: f64) -> Self {
        Self {
            coefficient,
            mean_reward_delta,
            mean_safety_penalty,
        }
    }
}

/// Threshold policy used for candidate filtering.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyPenaltyCalibrationPolicy {
    pub max_mean_safety_penalty: f64,
    pub min_mean_reward_delta: f64,
}

impl Default for SafetyPenaltyCalibrationPolicy {
    fn default() -> Self {
        Self {
            max_mean_safety_penalty: 0.05,
            min_mean_reward_delta: 0.10,
        }
    }
}

/// Report describing calibration candidate filtering/ranking.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyPenaltyCalibrationReport {
    pub total_candidates: usize,
    pub passing_candidates: Vec<SafetyPenaltyCalibrationObservation>,
    pub filtered_out_candidates: Vec<SafetyPenaltyCalibrationObservation>,
}

/// Selection result containing derived default coefficient and full report.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyPenaltyCalibrationSelection {
    pub default_coefficient: f64,
    pub report: SafetyPenaltyCalibrationReport,
}

/// Evaluates a candidate grid using safety/reward thresholds.
pub fn calibrate_safety_penalty_grid(
    observations: &[SafetyPenaltyCalibrationObservation],
    policy: &SafetyPenaltyCalibrationPolicy,
) -> Result<SafetyPenaltyCalibrationReport> {
    if observations.is_empty() {
        bail!("safety penalty calibration requires at least one observation");
    }
    if !policy.max_mean_safety_penalty.is_finite() || policy.max_mean_safety_penalty < 0.0 {
        bail!("max_mean_safety_penalty must be finite and non-negative");
    }
    if !policy.min_mean_reward_delta.is_finite() {
        bail!("min_mean_reward_delta must be finite");
    }

    let mut passing_candidates = Vec::new();
    let mut filtered_out_candidates = Vec::new();

    for candidate in observations {
        if !candidate.coefficient.is_finite() || candidate.coefficient < 0.0 {
            bail!("coefficient must be finite and non-negative");
        }
        if !candidate.mean_reward_delta.is_finite() || !candidate.mean_safety_penalty.is_finite() {
            bail!("mean_reward_delta and mean_safety_penalty must be finite");
        }
        if candidate.mean_reward_delta >= policy.min_mean_reward_delta
            && candidate.mean_safety_penalty <= policy.max_mean_safety_penalty
        {
            passing_candidates.push(candidate.clone());
        } else {
            filtered_out_candidates.push(candidate.clone());
        }
    }

    sort_candidates(&mut passing_candidates);
    sort_candidates(&mut filtered_out_candidates);

    Ok(SafetyPenaltyCalibrationReport {
        total_candidates: observations.len(),
        passing_candidates,
        filtered_out_candidates,
    })
}

/// Selects a default safety penalty coefficient from calibrated candidates.
pub fn select_default_safety_penalty_coefficient(
    observations: &[SafetyPenaltyCalibrationObservation],
    policy: &SafetyPenaltyCalibrationPolicy,
) -> Result<SafetyPenaltyCalibrationSelection> {
    let report = calibrate_safety_penalty_grid(observations, policy)?;
    let Some(best) = report.passing_candidates.first() else {
        bail!("no safety penalty calibration candidates satisfy policy");
    };
    Ok(SafetyPenaltyCalibrationSelection {
        default_coefficient: best.coefficient,
        report,
    })
}

fn sort_candidates(candidates: &mut [SafetyPenaltyCalibrationObservation]) {
    candidates.sort_by(|left, right| {
        right
            .mean_reward_delta
            .total_cmp(&left.mean_reward_delta)
            .then(
                left.mean_safety_penalty
                    .total_cmp(&right.mean_safety_penalty),
            )
            .then(left.coefficient.total_cmp(&right.coefficient))
    });
}

#[cfg(test)]
mod tests {
    use super::{
        calibrate_safety_penalty_grid, select_default_safety_penalty_coefficient,
        SafetyPenaltyCalibrationObservation, SafetyPenaltyCalibrationPolicy,
    };
    use std::path::Path;

    fn load_fixture(path: &Path) -> Vec<SafetyPenaltyCalibrationObservation> {
        let raw = std::fs::read_to_string(path).expect("read calibration fixture");
        let payload: serde_json::Value =
            serde_json::from_str(&raw).expect("parse calibration fixture");
        let rows = payload["observations"]
            .as_array()
            .expect("fixture observations should be array");
        rows.iter()
            .map(|row| {
                let coefficient = row["coefficient"]
                    .as_f64()
                    .expect("coefficient should be f64");
                let mean_reward_delta = row["mean_reward_delta"]
                    .as_f64()
                    .expect("mean_reward_delta should be f64");
                let mean_safety_penalty = row["mean_safety_penalty"]
                    .as_f64()
                    .expect("mean_safety_penalty should be f64");
                SafetyPenaltyCalibrationObservation::new(
                    coefficient,
                    mean_reward_delta,
                    mean_safety_penalty,
                )
            })
            .collect::<Vec<_>>()
    }

    #[test]
    fn functional_calibration_grid_filters_and_ranks_candidates_deterministically() {
        let observations = vec![
            SafetyPenaltyCalibrationObservation::new(0.05, 0.19, 0.03),
            SafetyPenaltyCalibrationObservation::new(0.10, 0.21, 0.04),
            SafetyPenaltyCalibrationObservation::new(0.20, 0.24, 0.10),
        ];
        let policy = SafetyPenaltyCalibrationPolicy {
            max_mean_safety_penalty: 0.06,
            min_mean_reward_delta: 0.15,
        };
        let report = calibrate_safety_penalty_grid(&observations, &policy).expect("report");
        assert_eq!(report.passing_candidates.len(), 2);
        assert_eq!(report.passing_candidates[0].coefficient, 0.10);
        assert_eq!(report.passing_candidates[1].coefficient, 0.05);
    }

    #[test]
    fn integration_select_default_safety_penalty_coefficient_from_fixture() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("safety_penalty_calibration_grid.json");
        let observations = load_fixture(&fixture_path);
        let policy = SafetyPenaltyCalibrationPolicy {
            max_mean_safety_penalty: 0.05,
            min_mean_reward_delta: 0.10,
        };
        let selected =
            select_default_safety_penalty_coefficient(&observations, &policy).expect("selection");
        assert_eq!(selected.default_coefficient, 0.12);
        assert_eq!(selected.report.passing_candidates.len(), 3);
    }

    #[test]
    fn regression_calibration_fails_closed_when_no_candidate_passes_thresholds() {
        let observations = vec![
            SafetyPenaltyCalibrationObservation::new(0.25, 0.04, 0.11),
            SafetyPenaltyCalibrationObservation::new(0.30, 0.02, 0.13),
        ];
        let policy = SafetyPenaltyCalibrationPolicy {
            max_mean_safety_penalty: 0.05,
            min_mean_reward_delta: 0.10,
        };
        let error = select_default_safety_penalty_coefficient(&observations, &policy)
            .expect_err("selection should fail");
        assert!(error
            .to_string()
            .contains("no safety penalty calibration candidates satisfy policy"));
    }
}
