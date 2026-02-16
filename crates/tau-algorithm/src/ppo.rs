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
    /// Number of passes over the sample set per update call.
    pub epochs: usize,
    /// Coefficient for KL penalty term added to total loss.
    pub kl_penalty_coefficient: f64,
    /// Optional target KL used for diagnostics and future scheduling hooks.
    pub target_kl: Option<f64>,
    /// Optional max KL threshold that triggers early-stop guidance.
    pub max_kl: Option<f64>,
}

impl Default for PpoConfig {
    fn default() -> Self {
        Self {
            clip_epsilon: 0.2,
            value_loss_coefficient: 0.5,
            entropy_coefficient: 0.01,
            mini_batch_size: 32,
            gradient_accumulation_steps: 1,
            epochs: 1,
            kl_penalty_coefficient: 0.0,
            target_kl: None,
            max_kl: None,
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
    pub approx_kl: f64,
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
    pub epochs: usize,
    pub mini_batch_count: usize,
    pub optimizer_step_count: usize,
    pub mini_batch_losses: Vec<PpoLossBreakdown>,
    pub optimizer_steps: Vec<PpoOptimizerStep>,
    pub mean_loss: PpoLossBreakdown,
    pub early_stop_triggered: bool,
    pub early_stop_reason: Option<String>,
    pub observed_approx_kl: f64,
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
    let mut approx_kl_sum = 0.0;
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
        let delta = sample.new_logprob - sample.old_logprob;
        approx_kl_sum += 0.5 * delta * delta;
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
    let approx_kl = approx_kl_sum / sample_count;

    let total_loss = policy_loss + (config.value_loss_coefficient * value_loss)
        - (config.entropy_coefficient * entropy_bonus)
        + (config.kl_penalty_coefficient * approx_kl);

    let breakdown = PpoLossBreakdown {
        policy_loss,
        value_loss,
        entropy_bonus,
        total_loss,
        mean_ratio,
        clipped_fraction,
        approx_kl,
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
    let mut epoch_batches = Vec::with_capacity(mini_batch_slices.len() * config.epochs);
    for _epoch in 0..config.epochs {
        epoch_batches.extend(mini_batch_slices.iter().copied());
    }

    let mini_batch_losses = epoch_batches
        .iter()
        .map(|batch| compute_ppo_loss(config, batch))
        .collect::<Result<Vec<_>>>()?;

    let mut optimizer_steps = Vec::new();
    for (step_index, batch_group) in epoch_batches
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

    let mean_loss = mean_loss(&mini_batch_losses);
    let observed_approx_kl = mean_loss.approx_kl;
    let (early_stop_triggered, early_stop_reason) = if config
        .max_kl
        .is_some_and(|max_kl| observed_approx_kl > max_kl)
    {
        (true, Some("ppo.max_kl_exceeded".to_string()))
    } else {
        (false, None)
    };

    let summary = PpoUpdateSummary {
        epochs: config.epochs,
        mini_batch_count: mini_batch_losses.len(),
        optimizer_step_count: optimizer_steps.len(),
        mini_batch_losses: mini_batch_losses.clone(),
        optimizer_steps,
        mean_loss,
        early_stop_triggered,
        early_stop_reason,
        observed_approx_kl,
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
    if !config.kl_penalty_coefficient.is_finite() || config.kl_penalty_coefficient < 0.0 {
        bail!(
            "invalid ppo config: kl_penalty_coefficient must be finite and >= 0.0, found {}",
            config.kl_penalty_coefficient
        );
    }
    if config
        .target_kl
        .is_some_and(|target_kl| !target_kl.is_finite() || target_kl < 0.0)
    {
        bail!("invalid ppo config: target_kl must be finite and >= 0.0");
    }
    if config
        .max_kl
        .is_some_and(|max_kl| !max_kl.is_finite() || max_kl < 0.0)
    {
        bail!("invalid ppo config: max_kl must be finite and >= 0.0");
    }
    if let (Some(target_kl), Some(max_kl)) = (config.target_kl, config.max_kl) {
        if max_kl < target_kl {
            bail!("invalid ppo config: max_kl must be >= target_kl when both are set");
        }
    }
    if config.mini_batch_size == 0 {
        bail!("invalid ppo config: mini_batch_size must be > 0");
    }
    if config.gradient_accumulation_steps == 0 {
        bail!("invalid ppo config: gradient_accumulation_steps must be > 0");
    }
    if config.epochs == 0 {
        bail!("invalid ppo config: epochs must be > 0");
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
        approx_kl: losses.iter().map(|loss| loss.approx_kl).sum::<f64>() / count,
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
        ("approx_kl", loss.approx_kl),
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
    use super::{compute_ppo_loss, compute_ppo_update, PpoConfig, PpoLossBreakdown, PpoSample};
    use anyhow::{bail, Context, Result};
    use serde_json::Value;

    const PPO_REFERENCE_FIXTURE: &str = include_str!("../testdata/ppo/reference_loss_cases.json");

    #[derive(Debug, Clone)]
    struct PpoReferenceCase {
        name: String,
        tolerance: f64,
        config: PpoConfig,
        samples: Vec<PpoSample>,
        expected_loss: PpoLossBreakdown,
    }

    #[test]
    fn spec_c01_compute_ppo_loss_matches_reference_vector() -> Result<()> {
        let config = PpoConfig {
            clip_epsilon: 0.2,
            value_loss_coefficient: 0.5,
            entropy_coefficient: 0.01,
            mini_batch_size: 2,
            gradient_accumulation_steps: 1,
            epochs: 1,
            kl_penalty_coefficient: 0.0,
            target_kl: None,
            max_kl: None,
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
            "spec_c01_inline_reference",
            loss,
            PpoLossBreakdown {
                policy_loss: -0.352_585_459_037_823_85,
                value_loss: 0.0325,
                entropy_bonus: 0.6,
                total_loss: -0.342_335_459_037_823_84,
                mean_ratio: 0.922_994_569_378_682_7,
                clipped_fraction: 0.5,
                approx_kl: 0.025,
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
            epochs: 1,
            kl_penalty_coefficient: 0.0,
            target_kl: None,
            max_kl: None,
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

    #[test]
    fn unit_ppo_config_rejects_negative_kl_penalty_coefficient() {
        let config = PpoConfig {
            kl_penalty_coefficient: -0.1,
            ..PpoConfig::default()
        };
        let samples = vec![PpoSample {
            old_logprob: -0.2,
            new_logprob: -0.1,
            advantage: 0.5,
            return_value: 0.5,
            value_prediction: 0.4,
            entropy: 0.2,
        }];

        let error = compute_ppo_loss(&config, &samples).expect_err("negative KL penalty must fail");
        assert!(error.to_string().contains("kl_penalty_coefficient"));
    }

    #[test]
    fn unit_ppo_config_rejects_zero_epochs() {
        let config = PpoConfig {
            epochs: 0,
            ..PpoConfig::default()
        };
        let samples = vec![PpoSample {
            old_logprob: -0.2,
            new_logprob: -0.1,
            advantage: 0.5,
            return_value: 0.5,
            value_prediction: 0.4,
            entropy: 0.2,
        }];

        let error = compute_ppo_update(&config, &samples).expect_err("epochs=0 must fail closed");
        assert!(error.to_string().contains("epochs"));
    }

    #[test]
    fn functional_ppo_loss_includes_kl_penalty_term() -> Result<()> {
        let config = PpoConfig {
            kl_penalty_coefficient: 2.0,
            ..PpoConfig::default()
        };
        let samples = vec![
            PpoSample {
                old_logprob: -0.2,
                new_logprob: -0.05,
                advantage: 1.0,
                return_value: 1.2,
                value_prediction: 1.1,
                entropy: 0.7,
            },
            PpoSample {
                old_logprob: -0.3,
                new_logprob: -0.15,
                advantage: 0.8,
                return_value: 0.9,
                value_prediction: 0.75,
                entropy: 0.6,
            },
        ];

        let loss = compute_ppo_loss(&config, &samples)?;
        let expected_total = loss.policy_loss + (config.value_loss_coefficient * loss.value_loss)
            - (config.entropy_coefficient * loss.entropy_bonus)
            + (config.kl_penalty_coefficient * loss.approx_kl);
        assert!((loss.total_loss - expected_total).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn regression_ppo_update_triggers_early_stop_when_mean_kl_exceeds_threshold() -> Result<()> {
        let config = PpoConfig {
            max_kl: Some(0.02),
            ..PpoConfig::default()
        };
        let samples = vec![
            PpoSample {
                old_logprob: -0.5,
                new_logprob: 0.0,
                advantage: 0.6,
                return_value: 1.0,
                value_prediction: 0.8,
                entropy: 0.3,
            },
            PpoSample {
                old_logprob: -0.4,
                new_logprob: 0.1,
                advantage: 0.4,
                return_value: 0.9,
                value_prediction: 0.7,
                entropy: 0.2,
            },
        ];

        let summary = compute_ppo_update(&config, &samples)?;
        assert!(summary.early_stop_triggered);
        assert_eq!(
            summary.early_stop_reason.as_deref(),
            Some("ppo.max_kl_exceeded")
        );
        Ok(())
    }

    #[test]
    fn functional_ppo_update_expands_minibatches_across_epochs() -> Result<()> {
        let config = PpoConfig {
            mini_batch_size: 2,
            gradient_accumulation_steps: 2,
            epochs: 3,
            ..PpoConfig::default()
        };
        let samples = vec![
            PpoSample {
                old_logprob: -0.1,
                new_logprob: -0.05,
                advantage: 1.0,
                return_value: 1.0,
                value_prediction: 0.8,
                entropy: 0.4,
            },
            PpoSample {
                old_logprob: -0.2,
                new_logprob: -0.1,
                advantage: 0.7,
                return_value: 0.8,
                value_prediction: 0.6,
                entropy: 0.35,
            },
            PpoSample {
                old_logprob: -0.3,
                new_logprob: -0.2,
                advantage: 0.5,
                return_value: 0.6,
                value_prediction: 0.5,
                entropy: 0.3,
            },
            PpoSample {
                old_logprob: -0.4,
                new_logprob: -0.25,
                advantage: 0.4,
                return_value: 0.5,
                value_prediction: 0.45,
                entropy: 0.25,
            },
            PpoSample {
                old_logprob: -0.5,
                new_logprob: -0.3,
                advantage: 0.3,
                return_value: 0.4,
                value_prediction: 0.35,
                entropy: 0.2,
            },
        ];

        let summary = compute_ppo_update(&config, &samples)?;
        assert_eq!(summary.epochs, 3);
        assert_eq!(summary.mini_batch_count, 9);
        assert_eq!(summary.optimizer_step_count, 5);
        Ok(())
    }

    #[test]
    fn regression_ppo_update_rejects_overflowing_approx_kl() {
        let config = PpoConfig::default();
        let samples = vec![PpoSample {
            old_logprob: -1.0e308,
            new_logprob: 1.0e308,
            advantage: 0.5,
            return_value: 0.5,
            value_prediction: 0.4,
            entropy: 0.2,
        }];

        let error =
            compute_ppo_update(&config, &samples).expect_err("overflowing approx_kl must fail");
        assert!(error.to_string().contains("non-finite ppo loss field"));
    }

    #[test]
    fn integration_ppo_epoch_fixture_runs_are_deterministic() -> Result<()> {
        let mut case = load_ppo_reference_cases()?
            .into_iter()
            .next()
            .context("expected at least one fixture case")?;
        case.config.epochs = 2;

        let first = compute_ppo_update(&case.config, &case.samples)?;
        let second = compute_ppo_update(&case.config, &case.samples)?;
        assert_eq!(first.mini_batch_count, second.mini_batch_count);
        assert_eq!(first.optimizer_step_count, second.optimizer_step_count);
        assert!((first.mean_loss.total_loss - second.mean_loss.total_loss).abs() < 1e-12);
        assert!((first.mean_loss.approx_kl - second.mean_loss.approx_kl).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn regression_ppo_reference_fixtures_match_expected_tolerance() -> Result<()> {
        let cases = load_ppo_reference_cases()?;
        assert!(
            !cases.is_empty(),
            "fixture suite must include at least one conformance case"
        );

        for case in cases {
            let loss = compute_ppo_loss(&case.config, &case.samples)?;
            assert_loss_close(&case.name, loss, case.expected_loss, case.tolerance);
        }
        Ok(())
    }

    #[test]
    fn unit_ppo_update_fixture_cases_remain_finite_and_deterministic() -> Result<()> {
        for case in load_ppo_reference_cases()? {
            let summary = compute_ppo_update(&case.config, &case.samples)?;
            assert_eq!(summary.mini_batch_count, 1);
            assert_eq!(summary.optimizer_step_count, 1);
            assert_eq!(summary.optimizer_steps[0].sample_count, case.samples.len());
            assert_eq!(summary.optimizer_steps[0].micro_batch_count, 1);
            assert_loss_close(
                &format!("{}_update_mean_loss", case.name),
                summary.mean_loss,
                case.expected_loss,
                case.tolerance,
            );
        }

        Ok(())
    }

    fn load_ppo_reference_cases() -> Result<Vec<PpoReferenceCase>> {
        let payload: Value = serde_json::from_str(PPO_REFERENCE_FIXTURE)
            .context("failed to parse PPO reference fixture JSON")?;
        let schema_version = payload
            .get("schema_version")
            .and_then(Value::as_u64)
            .context("fixture missing numeric schema_version")?;
        if schema_version != 1 {
            bail!("unsupported fixture schema_version {schema_version} (expected 1)");
        }

        let cases = payload
            .get("cases")
            .and_then(Value::as_array)
            .context("fixture missing array field 'cases'")?;
        cases
            .iter()
            .enumerate()
            .map(|(index, case)| parse_reference_case(case, index))
            .collect()
    }

    fn parse_reference_case(case: &Value, case_index: usize) -> Result<PpoReferenceCase> {
        let name = case
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .with_context(|| format!("fixture case[{case_index}] missing string field 'name'"))?;
        let tolerance = case
            .get("tolerance")
            .and_then(Value::as_f64)
            .with_context(|| format!("fixture case '{name}' missing numeric field 'tolerance'"))?;
        let config = parse_config(
            case.get("config")
                .context("fixture case missing object field 'config'")?,
            &name,
        )?;
        let samples = parse_samples(
            case.get("samples")
                .context("fixture case missing array field 'samples'")?,
            &name,
        )?;
        let expected_loss = parse_expected_loss(
            case.get("expected_loss")
                .context("fixture case missing object field 'expected_loss'")?,
            &name,
        )?;

        Ok(PpoReferenceCase {
            name,
            tolerance,
            config,
            samples,
            expected_loss,
        })
    }

    fn parse_config(config: &Value, case_name: &str) -> Result<PpoConfig> {
        let object = config
            .as_object()
            .with_context(|| format!("fixture case '{case_name}' has non-object config"))?;
        let defaults = PpoConfig::default();
        Ok(PpoConfig {
            clip_epsilon: required_f64(object, "clip_epsilon", case_name)?,
            value_loss_coefficient: required_f64(object, "value_loss_coefficient", case_name)?,
            entropy_coefficient: required_f64(object, "entropy_coefficient", case_name)?,
            mini_batch_size: required_u64(object, "mini_batch_size", case_name)? as usize,
            gradient_accumulation_steps: required_u64(
                object,
                "gradient_accumulation_steps",
                case_name,
            )? as usize,
            epochs: optional_u64(object, "epochs").unwrap_or(defaults.epochs as u64) as usize,
            kl_penalty_coefficient: optional_f64(object, "kl_penalty_coefficient")
                .unwrap_or(defaults.kl_penalty_coefficient),
            target_kl: optional_f64(object, "target_kl"),
            max_kl: optional_f64(object, "max_kl"),
        })
    }

    fn parse_samples(samples: &Value, case_name: &str) -> Result<Vec<PpoSample>> {
        let items = samples
            .as_array()
            .with_context(|| format!("fixture case '{case_name}' has non-array samples"))?;
        if items.is_empty() {
            bail!("fixture case '{case_name}' must provide at least one sample");
        }

        items
            .iter()
            .enumerate()
            .map(|(index, sample)| parse_sample(sample, case_name, index))
            .collect()
    }

    fn parse_sample(sample: &Value, case_name: &str, index: usize) -> Result<PpoSample> {
        let object = sample
            .as_object()
            .with_context(|| format!("fixture case '{case_name}' sample[{index}] is not object"))?;
        Ok(PpoSample {
            old_logprob: required_f64(object, "old_logprob", case_name)?,
            new_logprob: required_f64(object, "new_logprob", case_name)?,
            advantage: required_f64(object, "advantage", case_name)?,
            return_value: required_f64(object, "return_value", case_name)?,
            value_prediction: required_f64(object, "value_prediction", case_name)?,
            entropy: required_f64(object, "entropy", case_name)?,
        })
    }

    fn parse_expected_loss(expected_loss: &Value, case_name: &str) -> Result<PpoLossBreakdown> {
        let object = expected_loss
            .as_object()
            .with_context(|| format!("fixture case '{case_name}' has non-object expected_loss"))?;
        Ok(PpoLossBreakdown {
            policy_loss: required_f64(object, "policy_loss", case_name)?,
            value_loss: required_f64(object, "value_loss", case_name)?,
            entropy_bonus: required_f64(object, "entropy_bonus", case_name)?,
            total_loss: required_f64(object, "total_loss", case_name)?,
            mean_ratio: required_f64(object, "mean_ratio", case_name)?,
            clipped_fraction: required_f64(object, "clipped_fraction", case_name)?,
            approx_kl: required_f64(object, "approx_kl", case_name)?,
        })
    }

    fn required_f64(
        object: &serde_json::Map<String, Value>,
        key: &str,
        case_name: &str,
    ) -> Result<f64> {
        object
            .get(key)
            .and_then(Value::as_f64)
            .with_context(|| format!("fixture case '{case_name}' missing numeric field '{key}'"))
    }

    fn required_u64(
        object: &serde_json::Map<String, Value>,
        key: &str,
        case_name: &str,
    ) -> Result<u64> {
        object
            .get(key)
            .and_then(Value::as_u64)
            .with_context(|| format!("fixture case '{case_name}' missing integer field '{key}'"))
    }

    fn optional_f64(object: &serde_json::Map<String, Value>, key: &str) -> Option<f64> {
        object.get(key).and_then(Value::as_f64)
    }

    fn optional_u64(object: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
        object.get(key).and_then(Value::as_u64)
    }

    fn assert_loss_close(
        case_name: &str,
        actual: PpoLossBreakdown,
        expected: PpoLossBreakdown,
        tolerance: f64,
    ) {
        assert_close(
            case_name,
            "policy_loss",
            actual.policy_loss,
            expected.policy_loss,
            tolerance,
        );
        assert_close(
            case_name,
            "value_loss",
            actual.value_loss,
            expected.value_loss,
            tolerance,
        );
        assert_close(
            case_name,
            "entropy_bonus",
            actual.entropy_bonus,
            expected.entropy_bonus,
            tolerance,
        );
        assert_close(
            case_name,
            "total_loss",
            actual.total_loss,
            expected.total_loss,
            tolerance,
        );
        assert_close(
            case_name,
            "mean_ratio",
            actual.mean_ratio,
            expected.mean_ratio,
            tolerance,
        );
        assert_close(
            case_name,
            "clipped_fraction",
            actual.clipped_fraction,
            expected.clipped_fraction,
            tolerance,
        );
        assert_close(
            case_name,
            "approx_kl",
            actual.approx_kl,
            expected.approx_kl,
            tolerance,
        );
    }

    fn assert_close(case_name: &str, field: &str, actual: f64, expected: f64, tolerance: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta <= tolerance,
            "fixture case '{case_name}' field '{field}' delta {delta} exceeds tolerance {tolerance}; actual={actual}, expected={expected}"
        );
    }
}
