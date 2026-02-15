//! PPO objective math core with deterministic update aggregation utilities.

use anyhow::{bail, Result};

/// Configuration for PPO objective and update aggregation.
#[derive(Debug, Clone)]
pub struct PpoConfig {
    /// Surrogate clipping coefficient, usually around `0.2`.
    pub clip_epsilon: f64,
    /// Scalar coefficient applied to value loss.
    pub value_loss_coefficient: f64,
    /// Scalar entropy-bonus coefficient subtracted from total loss.
    pub entropy_coefficient: f64,
    /// Number of samples per minibatch.
    pub mini_batch_size: usize,
    /// Number of minibatches per optimizer step.
    pub gradient_accumulation_steps: usize,
}

impl Default for PpoConfig {
    fn default() -> Self {
        Self {
            clip_epsilon: 0.2,
            value_loss_coefficient: 0.5,
            entropy_coefficient: 0.01,
            mini_batch_size: 32,
            gradient_accumulation_steps: 1,
        }
    }
}

/// One PPO training sample aligned to a policy action step.
#[derive(Debug, Clone, Copy)]
pub struct PpoSample {
    pub old_logprob: f64,
    pub new_logprob: f64,
    pub advantage: f64,
    pub return_value: f64,
    pub value_prediction: f64,
    pub entropy: f64,
}

/// Scalar PPO loss terms computed over a sample set.
#[derive(Debug, Clone, Copy, Default)]
pub struct PpoLossBreakdown {
    pub policy_loss: f64,
    pub value_loss: f64,
    pub entropy_bonus: f64,
    pub total_loss: f64,
    pub mean_ratio: f64,
    pub clipped_fraction: f64,
}

/// Aggregated optimizer step summary after gradient accumulation.
#[derive(Debug, Clone)]
pub struct PpoOptimizerStep {
    pub step_index: usize,
    pub micro_batch_count: usize,
    pub sample_count: usize,
    pub loss: PpoLossBreakdown,
}

/// Update summary over all minibatches and optimizer steps.
#[derive(Debug, Clone)]
pub struct PpoUpdateSummary {
    pub mini_batch_count: usize,
    pub optimizer_step_count: usize,
    pub mini_batch_losses: Vec<PpoLossBreakdown>,
    pub optimizer_steps: Vec<PpoOptimizerStep>,
    pub mean_loss: PpoLossBreakdown,
}

/// Computes PPO loss terms over one batch of samples.
#[tracing::instrument(level = "debug", skip(samples))]
pub fn compute_ppo_loss(config: &PpoConfig, samples: &[PpoSample]) -> Result<PpoLossBreakdown> {
    validate_config(config)?;
    if samples.is_empty() {
        bail!("ppo loss requires at least one sample");
    }

    let mut policy_loss_sum = 0.0;
    let mut value_loss_sum = 0.0;
    let mut entropy_bonus_sum = 0.0;
    let mut ratio_sum = 0.0;
    let mut clipped_count = 0usize;

    let clip_lower = 1.0 - config.clip_epsilon;
    let clip_upper = 1.0 + config.clip_epsilon;
    for (index, sample) in samples.iter().enumerate() {
        validate_sample(index, sample)?;

        let ratio = (sample.new_logprob - sample.old_logprob).exp();
        let clipped_ratio = ratio.clamp(clip_lower, clip_upper);
        let surrogate_unclipped = ratio * sample.advantage;
        let surrogate_clipped = clipped_ratio * sample.advantage;

        let clipped_surrogate = surrogate_unclipped.min(surrogate_clipped);
        policy_loss_sum += -clipped_surrogate;

        let value_error = sample.value_prediction - sample.return_value;
        value_loss_sum += 0.5 * value_error * value_error;

        entropy_bonus_sum += sample.entropy;
        ratio_sum += ratio;
        if (ratio - clipped_ratio).abs() > f64::EPSILON {
            clipped_count += 1;
        }
    }

    let sample_count = samples.len() as f64;
    let policy_loss = policy_loss_sum / sample_count;
    let value_loss = value_loss_sum / sample_count;
    let entropy_bonus = entropy_bonus_sum / sample_count;
    let mean_ratio = ratio_sum / sample_count;
    let clipped_fraction = clipped_count as f64 / sample_count;

    let total_loss = policy_loss + (config.value_loss_coefficient * value_loss)
        - (config.entropy_coefficient * entropy_bonus);

    let breakdown = PpoLossBreakdown {
        policy_loss,
        value_loss,
        entropy_bonus,
        total_loss,
        mean_ratio,
        clipped_fraction,
    };
    ensure_loss_is_finite(&breakdown)?;
    Ok(breakdown)
}

/// Computes minibatch PPO losses and folds them into accumulated optimizer steps.
#[tracing::instrument(level = "debug", skip(samples))]
pub fn compute_ppo_update(config: &PpoConfig, samples: &[PpoSample]) -> Result<PpoUpdateSummary> {
    validate_config(config)?;
    if samples.is_empty() {
        bail!("ppo update requires at least one sample");
    }

    let mini_batch_slices: Vec<&[PpoSample]> = samples.chunks(config.mini_batch_size).collect();
    let mini_batch_losses = mini_batch_slices
        .iter()
        .map(|batch| compute_ppo_loss(config, batch))
        .collect::<Result<Vec<_>>>()?;

    let mut optimizer_steps = Vec::new();
    for (step_index, batch_group) in mini_batch_slices
        .chunks(config.gradient_accumulation_steps)
        .enumerate()
    {
        let start = step_index * config.gradient_accumulation_steps;
        let end = start + batch_group.len();
        let losses = &mini_batch_losses[start..end];

        optimizer_steps.push(PpoOptimizerStep {
            step_index,
            micro_batch_count: batch_group.len(),
            sample_count: batch_group.iter().map(|batch| batch.len()).sum(),
            loss: mean_loss(losses),
        });
    }

    let summary = PpoUpdateSummary {
        mini_batch_count: mini_batch_losses.len(),
        optimizer_step_count: optimizer_steps.len(),
        mini_batch_losses: mini_batch_losses.clone(),
        optimizer_steps,
        mean_loss: mean_loss(&mini_batch_losses),
    };
    ensure_loss_is_finite(&summary.mean_loss)?;
    Ok(summary)
}

fn validate_config(config: &PpoConfig) -> Result<()> {
    if !config.clip_epsilon.is_finite() || config.clip_epsilon < 0.0 {
        bail!(
            "invalid ppo config: clip_epsilon must be finite and >= 0.0, found {}",
            config.clip_epsilon
        );
    }
    if !config.value_loss_coefficient.is_finite() || config.value_loss_coefficient < 0.0 {
        bail!(
            "invalid ppo config: value_loss_coefficient must be finite and >= 0.0, found {}",
            config.value_loss_coefficient
        );
    }
    if !config.entropy_coefficient.is_finite() || config.entropy_coefficient < 0.0 {
        bail!(
            "invalid ppo config: entropy_coefficient must be finite and >= 0.0, found {}",
            config.entropy_coefficient
        );
    }
    if config.mini_batch_size == 0 {
        bail!("invalid ppo config: mini_batch_size must be > 0");
    }
    if config.gradient_accumulation_steps == 0 {
        bail!("invalid ppo config: gradient_accumulation_steps must be > 0");
    }
    Ok(())
}

fn validate_sample(index: usize, sample: &PpoSample) -> Result<()> {
    validate_finite("old_logprob", index, sample.old_logprob)?;
    validate_finite("new_logprob", index, sample.new_logprob)?;
    validate_finite("advantage", index, sample.advantage)?;
    validate_finite("return_value", index, sample.return_value)?;
    validate_finite("value_prediction", index, sample.value_prediction)?;
    validate_finite("entropy", index, sample.entropy)?;
    Ok(())
}

fn validate_finite(field: &str, index: usize, value: f64) -> Result<()> {
    if value.is_finite() {
        return Ok(());
    }
    bail!("non-finite PPO sample field '{field}' at index {index}")
}

fn mean_loss(losses: &[PpoLossBreakdown]) -> PpoLossBreakdown {
    let count = losses.len() as f64;
    PpoLossBreakdown {
        policy_loss: losses.iter().map(|loss| loss.policy_loss).sum::<f64>() / count,
        value_loss: losses.iter().map(|loss| loss.value_loss).sum::<f64>() / count,
        entropy_bonus: losses.iter().map(|loss| loss.entropy_bonus).sum::<f64>() / count,
        total_loss: losses.iter().map(|loss| loss.total_loss).sum::<f64>() / count,
        mean_ratio: losses.iter().map(|loss| loss.mean_ratio).sum::<f64>() / count,
        clipped_fraction: losses.iter().map(|loss| loss.clipped_fraction).sum::<f64>() / count,
    }
}

fn ensure_loss_is_finite(loss: &PpoLossBreakdown) -> Result<()> {
    let values = [
        ("policy_loss", loss.policy_loss),
        ("value_loss", loss.value_loss),
        ("entropy_bonus", loss.entropy_bonus),
        ("total_loss", loss.total_loss),
        ("mean_ratio", loss.mean_ratio),
        ("clipped_fraction", loss.clipped_fraction),
    ];
    for (field, value) in values {
        if !value.is_finite() {
            bail!("non-finite ppo loss field '{field}'");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        compute_ppo_loss, compute_ppo_update, PpoConfig, PpoLossBreakdown, PpoOptimizerStep,
        PpoSample,
    };
    use anyhow::Result;

    #[test]
    fn spec_c01_compute_ppo_loss_matches_reference_vector() -> Result<()> {
        let config = PpoConfig {
            clip_epsilon: 0.2,
            value_loss_coefficient: 0.5,
            entropy_coefficient: 0.01,
            mini_batch_size: 2,
            gradient_accumulation_steps: 1,
        };
        let samples = vec![
            PpoSample {
                old_logprob: -0.2,
                new_logprob: -0.1,
                advantage: 1.0,
                return_value: 1.2,
                value_prediction: 1.0,
                entropy: 0.7,
            },
            PpoSample {
                old_logprob: -0.3,
                new_logprob: -0.6,
                advantage: -0.5,
                return_value: -0.2,
                value_prediction: 0.1,
                entropy: 0.5,
            },
        ];

        let loss = compute_ppo_loss(&config, &samples)?;
        assert_loss_close(
            loss,
            PpoLossBreakdown {
                policy_loss: -0.352_585_459_037_823_85,
                value_loss: 0.0325,
                entropy_bonus: 0.6,
                total_loss: -0.342_335_459_037_823_84,
                mean_ratio: 0.922_994_569_378_682_7,
                clipped_fraction: 0.5,
            },
            1e-12,
        );

        Ok(())
    }

    #[test]
    fn spec_c02_compute_ppo_update_accumulates_minibatches_deterministically() -> Result<()> {
        let config = PpoConfig {
            clip_epsilon: 0.2,
            value_loss_coefficient: 0.5,
            entropy_coefficient: 0.01,
            mini_batch_size: 2,
            gradient_accumulation_steps: 2,
        };
        let samples = vec![
            PpoSample {
                old_logprob: -0.10,
                new_logprob: -0.08,
                advantage: 0.8,
                return_value: 1.0,
                value_prediction: 0.9,
                entropy: 0.6,
            },
            PpoSample {
                old_logprob: -0.40,
                new_logprob: -0.55,
                advantage: -0.4,
                return_value: -0.1,
                value_prediction: 0.0,
                entropy: 0.4,
            },
            PpoSample {
                old_logprob: -0.20,
                new_logprob: -0.25,
                advantage: 0.3,
                return_value: 0.5,
                value_prediction: 0.45,
                entropy: 0.3,
            },
            PpoSample {
                old_logprob: -0.80,
                new_logprob: -0.60,
                advantage: -0.2,
                return_value: -0.4,
                value_prediction: -0.1,
                entropy: 0.2,
            },
            PpoSample {
                old_logprob: -0.05,
                new_logprob: -0.02,
                advantage: 0.6,
                return_value: 0.8,
                value_prediction: 0.7,
                entropy: 0.5,
            },
        ];

        let summary = compute_ppo_update(&config, &samples)?;
        assert_eq!(summary.mini_batch_count, 3);
        assert_eq!(summary.optimizer_step_count, 2);
        assert_eq!(summary.optimizer_steps.len(), 2);
        assert_eq!(
            summary
                .optimizer_steps
                .iter()
                .map(|step| step.sample_count)
                .collect::<Vec<_>>(),
            vec![4, 1]
        );
        assert_eq!(
            summary
                .optimizer_steps
                .iter()
                .map(|step| step.micro_batch_count)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert!(summary.mean_loss.total_loss.is_finite());

        Ok(())
    }

    #[test]
    fn spec_c03_compute_ppo_loss_rejects_non_finite_samples() {
        let config = PpoConfig::default();
        let samples = vec![PpoSample {
            old_logprob: -0.1,
            new_logprob: -0.1,
            advantage: f64::NAN,
            return_value: 0.0,
            value_prediction: 0.0,
            entropy: 0.0,
        }];

        let error = compute_ppo_loss(&config, &samples).expect_err("non-finite sample should fail");
        assert!(
            error.to_string().contains("non-finite"),
            "unexpected error: {error:#}"
        );
    }

    fn assert_loss_close(actual: PpoLossBreakdown, expected: PpoLossBreakdown, tolerance: f64) {
        assert_close(actual.policy_loss, expected.policy_loss, tolerance);
        assert_close(actual.value_loss, expected.value_loss, tolerance);
        assert_close(actual.entropy_bonus, expected.entropy_bonus, tolerance);
        assert_close(actual.total_loss, expected.total_loss, tolerance);
        assert_close(actual.mean_ratio, expected.mean_ratio, tolerance);
        assert_close(
            actual.clipped_fraction,
            expected.clipped_fraction,
            tolerance,
        );
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta <= tolerance,
            "delta {delta} exceeds tolerance {tolerance}; actual={actual}, expected={expected}"
        );
    }

    #[allow(dead_code)]
    fn _assert_step_shape(step: &PpoOptimizerStep) {
        assert!(step.loss.total_loss.is_finite());
    }
}
