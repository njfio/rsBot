//! Generalized Advantage Estimation (GAE) utilities for RL batches.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use tau_training_types::{AdvantageBatch, EpisodeTrajectory};

/// Configuration for GAE discounting, normalization, and clipping.
#[derive(Debug, Clone)]
pub struct GaeConfig {
    /// Reward discount factor.
    pub gamma: f64,
    /// GAE trace decay coefficient.
    pub lambda: f64,
    /// Normalize advantages to zero mean and unit variance.
    pub normalize_advantages: bool,
    /// Optional absolute clip bound for advantages.
    pub clip_advantages: Option<f64>,
    /// Normalize returns to zero mean and unit variance.
    pub normalize_returns: bool,
    /// Optional absolute clip bound for returns.
    pub clip_returns: Option<f64>,
    /// Epsilon used by normalization denominator.
    pub normalization_epsilon: f64,
}

impl Default for GaeConfig {
    fn default() -> Self {
        Self {
            gamma: 0.99,
            lambda: 0.95,
            normalize_advantages: true,
            clip_advantages: None,
            normalize_returns: false,
            clip_returns: None,
            normalization_epsilon: 1e-8,
        }
    }
}

impl GaeConfig {
    /// Parses `GaeConfig` from a JSON object.
    #[tracing::instrument(level = "debug", skip(value))]
    pub fn from_json(value: &Value) -> Result<Self> {
        let object = value
            .as_object()
            .context("gae config JSON payload must be an object")?;
        let mut config = Self::default();

        if let Some(gamma) = object.get("gamma") {
            config.gamma = gamma
                .as_f64()
                .context("gae config field 'gamma' must be numeric")?;
        }
        if let Some(lambda) = object.get("lambda") {
            config.lambda = lambda
                .as_f64()
                .context("gae config field 'lambda' must be numeric")?;
        }
        if let Some(normalize_advantages) = object.get("normalize_advantages") {
            config.normalize_advantages = normalize_advantages
                .as_bool()
                .context("gae config field 'normalize_advantages' must be boolean")?;
        }
        if let Some(clip_advantages) = object.get("clip_advantages") {
            config.clip_advantages = if clip_advantages.is_null() {
                None
            } else {
                Some(
                    clip_advantages
                        .as_f64()
                        .context("gae config field 'clip_advantages' must be numeric or null")?,
                )
            };
        }
        if let Some(normalize_returns) = object.get("normalize_returns") {
            config.normalize_returns = normalize_returns
                .as_bool()
                .context("gae config field 'normalize_returns' must be boolean")?;
        }
        if let Some(clip_returns) = object.get("clip_returns") {
            config.clip_returns = if clip_returns.is_null() {
                None
            } else {
                Some(
                    clip_returns
                        .as_f64()
                        .context("gae config field 'clip_returns' must be numeric or null")?,
                )
            };
        }
        if let Some(normalization_epsilon) = object.get("normalization_epsilon") {
            config.normalization_epsilon = normalization_epsilon
                .as_f64()
                .context("gae config field 'normalization_epsilon' must be numeric")?;
        }

        validate_config(&config)?;
        Ok(config)
    }
}

/// Computes an `AdvantageBatch` from reward/value/done arrays.
#[tracing::instrument(level = "debug", skip(batch_id, trajectory_id, rewards, values, dones))]
pub fn compute_gae_batch_from_slices(
    config: &GaeConfig,
    batch_id: impl Into<String>,
    trajectory_id: impl Into<String>,
    rewards: &[f64],
    values: &[f64],
    dones: &[bool],
    bootstrap_value: f64,
) -> Result<AdvantageBatch> {
    validate_config(config)?;
    validate_slice_inputs(rewards, values, dones, bootstrap_value)?;

    let mut advantages = vec![0.0; rewards.len()];
    let mut gae = 0.0;
    let mut next_value = bootstrap_value;
    for index in (0..rewards.len()).rev() {
        let non_terminal = if dones[index] { 0.0 } else { 1.0 };
        let delta = rewards[index] + (config.gamma * next_value * non_terminal) - values[index];
        gae = delta + (config.gamma * config.lambda * non_terminal * gae);
        advantages[index] = gae;
        next_value = values[index];
    }
    ensure_finite_slice("advantages", &advantages)?;

    let mut returns = advantages
        .iter()
        .zip(values.iter())
        .map(|(advantage, value)| advantage + value)
        .collect::<Vec<_>>();
    ensure_finite_slice("returns", &returns)?;

    if config.normalize_advantages {
        normalize_values(&mut advantages, config.normalization_epsilon)?;
    }
    if let Some(bound) = config.clip_advantages {
        clip_values(&mut advantages, bound);
    }
    if config.normalize_returns {
        normalize_values(&mut returns, config.normalization_epsilon)?;
    }
    if let Some(bound) = config.clip_returns {
        clip_values(&mut returns, bound);
    }
    ensure_finite_slice("advantages", &advantages)?;
    ensure_finite_slice("returns", &returns)?;

    let mut batch = AdvantageBatch::new(batch_id, trajectory_id, advantages, returns);
    batch.normalized = config.normalize_advantages || config.normalize_returns;
    batch.value_targets = values.to_vec();
    batch.validate().context("generated GAE batch is invalid")?;
    Ok(batch)
}

/// Computes an `AdvantageBatch` from a trajectory value-estimate trace.
#[tracing::instrument(level = "debug", skip(batch_id, trajectory))]
pub fn compute_gae_batch_from_trajectory(
    config: &GaeConfig,
    batch_id: impl Into<String>,
    trajectory: &EpisodeTrajectory,
    bootstrap_value: f64,
) -> Result<AdvantageBatch> {
    trajectory
        .validate()
        .context("trajectory input failed schema validation")?;

    let rewards = trajectory
        .steps
        .iter()
        .map(|step| step.reward)
        .collect::<Vec<_>>();
    let dones = trajectory
        .steps
        .iter()
        .map(|step| step.done)
        .collect::<Vec<_>>();
    let values = trajectory
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            step.value_estimate
                .with_context(|| format!("trajectory step {index} missing value_estimate"))
        })
        .collect::<Result<Vec<_>>>()?;

    let trajectory_id = trajectory.trajectory_id.clone();
    compute_gae_batch_from_slices(
        config,
        batch_id,
        trajectory_id,
        &rewards,
        &values,
        &dones,
        bootstrap_value,
    )
}

fn validate_config(config: &GaeConfig) -> Result<()> {
    validate_probability("gamma", config.gamma)?;
    validate_probability("lambda", config.lambda)?;
    validate_positive("normalization_epsilon", config.normalization_epsilon)?;
    if let Some(bound) = config.clip_advantages {
        validate_positive("clip_advantages", bound)?;
    }
    if let Some(bound) = config.clip_returns {
        validate_positive("clip_returns", bound)?;
    }
    Ok(())
}

fn validate_slice_inputs(
    rewards: &[f64],
    values: &[f64],
    dones: &[bool],
    bootstrap_value: f64,
) -> Result<()> {
    if rewards.is_empty() {
        bail!("gae inputs must contain at least one step");
    }
    if rewards.len() != values.len() || rewards.len() != dones.len() {
        bail!(
            "gae input length mismatch: rewards={}, values={}, dones={}",
            rewards.len(),
            values.len(),
            dones.len()
        );
    }

    ensure_finite_slice("rewards", rewards)?;
    ensure_finite_slice("values", values)?;
    if !bootstrap_value.is_finite() {
        bail!("gae bootstrap_value must be finite");
    }
    Ok(())
}

fn validate_probability(label: &str, value: f64) -> Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        bail!("gae config field '{label}' must be finite and within [0.0, 1.0]");
    }
    Ok(())
}

fn validate_positive(label: &str, value: f64) -> Result<()> {
    if !value.is_finite() || value <= 0.0 {
        bail!("gae config field '{label}' must be finite and > 0.0");
    }
    Ok(())
}

fn normalize_values(values: &mut [f64], epsilon: f64) -> Result<()> {
    ensure_finite_slice("normalization_input", values)?;
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values
        .iter()
        .map(|value| {
            let centered = value - mean;
            centered * centered
        })
        .sum::<f64>()
        / count;
    let std_dev = (variance + epsilon).sqrt();
    if !std_dev.is_finite() || std_dev <= 0.0 {
        bail!("gae normalization produced invalid denominator");
    }
    for value in values {
        *value = (*value - mean) / std_dev;
    }
    Ok(())
}

fn clip_values(values: &mut [f64], bound: f64) {
    for value in values {
        *value = value.clamp(-bound, bound);
    }
}

fn ensure_finite_slice(field: &str, values: &[f64]) -> Result<()> {
    if values.iter().any(|value| !value.is_finite()) {
        bail!("non-finite values detected in '{field}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{compute_gae_batch_from_slices, compute_gae_batch_from_trajectory, GaeConfig};
    use anyhow::Result;
    use serde_json::json;
    use tau_training_types::{EpisodeTrajectory, TrajectoryStep};

    #[test]
    fn spec_c01_gae_outputs_match_known_vector() -> Result<()> {
        let config = GaeConfig {
            normalize_advantages: false,
            normalize_returns: false,
            ..GaeConfig::default()
        };
        let batch = compute_gae_batch_from_slices(
            &config,
            "batch-c01",
            "trajectory-c01",
            &[1.0, 1.0, 1.0],
            &[0.5, 0.4, 0.3],
            &[false, false, true],
            0.0,
        )?;

        assert_close(batch.advantages[0], 2.358_806_675, 1e-12);
        assert_close(batch.advantages[1], 1.555_35, 1e-12);
        assert_close(batch.advantages[2], 0.7, 1e-12);
        assert_close(batch.returns[0], 2.858_806_675, 1e-12);
        assert_close(batch.returns[1], 1.955_35, 1e-12);
        assert_close(batch.returns[2], 1.0, 1e-12);

        Ok(())
    }

    #[test]
    fn spec_c02_gae_normalization_and_clipping_controls_apply() -> Result<()> {
        let config = GaeConfig::from_json(&json!({
            "gamma": 0.99,
            "lambda": 0.95,
            "normalize_advantages": true,
            "clip_advantages": 0.5,
            "normalize_returns": true,
            "clip_returns": 0.75,
            "normalization_epsilon": 1e-8
        }))?;
        let batch = compute_gae_batch_from_slices(
            &config,
            "batch-c02",
            "trajectory-c02",
            &[1.0, 0.5, -0.25, 0.0],
            &[0.3, 0.1, 0.2, -0.1],
            &[false, false, false, true],
            0.0,
        )?;

        assert!(batch.normalized);
        assert!(
            batch
                .advantages
                .iter()
                .all(|value| value.abs() <= 0.5 + 1e-12),
            "advantages should respect clip bound"
        );
        assert!(
            batch
                .returns
                .iter()
                .all(|value| value.abs() <= 0.75 + 1e-12),
            "returns should respect clip bound"
        );

        Ok(())
    }

    #[test]
    fn spec_c03_gae_rejects_invalid_lengths_and_missing_values() {
        let config = GaeConfig::default();

        let length_error = compute_gae_batch_from_slices(
            &config,
            "batch-c03-len",
            "trajectory-c03-len",
            &[1.0, 0.5],
            &[0.2],
            &[false, true],
            0.0,
        )
        .expect_err("length mismatch should fail");
        assert!(length_error.to_string().contains("length"));

        let mut step = TrajectoryStep::new(0, json!({"obs": 0}), json!({"act": 0}), 1.0, true);
        step.value_estimate = None;
        let trajectory = EpisodeTrajectory::new(
            "trajectory-c03-missing",
            Some("rollout-c03-missing".to_string()),
            None,
            vec![step],
        );
        let missing_value_error =
            compute_gae_batch_from_trajectory(&config, "batch-c03-missing", &trajectory, 0.0)
                .expect_err("missing value estimate should fail");
        assert!(
            missing_value_error.to_string().contains("value_estimate"),
            "unexpected error: {missing_value_error:#}"
        );
    }

    #[test]
    fn unit_gae_truncation_scenario_with_bootstrap_remains_finite() -> Result<()> {
        let config = GaeConfig {
            normalize_advantages: false,
            normalize_returns: false,
            ..GaeConfig::default()
        };
        let batch = compute_gae_batch_from_slices(
            &config,
            "batch-truncation",
            "trajectory-truncation",
            &[0.4, 0.0, 0.2, 0.1],
            &[0.3, 0.2, 0.15, 0.05],
            &[false, false, false, false],
            0.7,
        )?;

        assert_all_finite_values(&batch.advantages);
        assert_all_finite_values(&batch.returns);
        assert_eq!(batch.advantages.len(), 4);
        assert_eq!(batch.returns.len(), 4);
        Ok(())
    }

    #[test]
    fn regression_gae_terminal_state_masks_bootstrap_value() -> Result<()> {
        let config = GaeConfig {
            normalize_advantages: false,
            normalize_returns: false,
            ..GaeConfig::default()
        };
        let batch = compute_gae_batch_from_slices(
            &config,
            "batch-terminal-mask",
            "trajectory-terminal-mask",
            &[0.0, 1.0],
            &[0.5, 0.2],
            &[false, true],
            5.0,
        )?;

        // Terminal transition must ignore bootstrap and reduce to reward - value.
        assert_close(batch.advantages[1], 0.8, 1e-12);
        Ok(())
    }

    #[test]
    fn unit_gae_sparse_reward_sequence_stays_finite() -> Result<()> {
        let config = GaeConfig {
            normalize_advantages: true,
            normalize_returns: true,
            clip_advantages: Some(1.0),
            clip_returns: Some(1.5),
            ..GaeConfig::default()
        };
        let batch = compute_gae_batch_from_slices(
            &config,
            "batch-sparse",
            "trajectory-sparse",
            &[0.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            &[0.1, 0.1, 0.1, 0.1, 0.1, 0.1],
            &[false, false, false, false, false, true],
            0.0,
        )?;

        assert_all_finite_values(&batch.advantages);
        assert_all_finite_values(&batch.returns);
        Ok(())
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta <= tolerance,
            "delta {delta} exceeds tolerance {tolerance}; actual={actual}, expected={expected}"
        );
    }

    fn assert_all_finite_values(values: &[f64]) {
        assert!(
            values.iter().all(|value| value.is_finite()),
            "expected all values to be finite, found {values:?}"
        );
    }
}
