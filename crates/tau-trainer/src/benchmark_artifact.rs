//! Deterministic benchmark evaluation artifact bundling.

use crate::benchmark_significance::{
    CheckpointPromotionDecision, PolicyImprovementReport, SampleSizeSensitivityReport,
    SeedReproducibilityReport,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::instrument;

/// Machine-readable benchmark-evaluation artifact payload.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkEvaluationArtifact {
    /// Artifact schema version for compatibility checks.
    pub schema_version: u32,
    /// Stable benchmark suite identifier.
    pub benchmark_suite_id: String,
    /// Baseline policy identifier used in comparison.
    pub baseline_policy_id: String,
    /// Candidate policy identifier used in comparison.
    pub candidate_policy_id: String,
    /// Generation timestamp in Unix milliseconds.
    pub generated_at_epoch_ms: u64,
    /// Baseline-vs-candidate significance output.
    pub policy_improvement: PolicyImprovementReport,
    /// Optional seeded reproducibility summary.
    pub seed_reproducibility: Option<SeedReproducibilityReport>,
    /// Optional sample-size sensitivity summary.
    pub sample_size_sensitivity: Option<SampleSizeSensitivityReport>,
    /// Promotion gate decision and reason codes.
    pub checkpoint_promotion: CheckpointPromotionDecision,
}

/// Input payload consumed by benchmark artifact builder.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkEvaluationArtifactInput {
    pub benchmark_suite_id: String,
    pub baseline_policy_id: String,
    pub candidate_policy_id: String,
    pub generated_at_epoch_ms: u64,
    pub policy_improvement: PolicyImprovementReport,
    pub seed_reproducibility: Option<SeedReproducibilityReport>,
    pub sample_size_sensitivity: Option<SampleSizeSensitivityReport>,
    pub checkpoint_promotion: CheckpointPromotionDecision,
}

/// Export metadata for persisted benchmark artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactExportSummary {
    /// Filesystem path to the exported artifact.
    pub path: PathBuf,
    /// Number of bytes written.
    pub bytes_written: usize,
}

impl BenchmarkEvaluationArtifact {
    /// Initial schema version for benchmark evaluation artifacts.
    pub const SCHEMA_VERSION_V1: u32 = 1;

    /// Projects the artifact into a deterministic JSON payload.
    pub fn to_json_value(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "benchmark_suite_id": self.benchmark_suite_id,
            "baseline_policy_id": self.baseline_policy_id,
            "candidate_policy_id": self.candidate_policy_id,
            "generated_at_epoch_ms": self.generated_at_epoch_ms,
            "policy_improvement": self.policy_improvement.to_json_value(),
            "seed_reproducibility": self.seed_reproducibility.as_ref().map(seed_reproducibility_to_json),
            "sample_size_sensitivity": self.sample_size_sensitivity.as_ref().map(sample_size_sensitivity_to_json),
            "checkpoint_promotion": checkpoint_promotion_to_json(&self.checkpoint_promotion),
        })
    }
}

/// Builds a deterministic benchmark-evaluation artifact bundle.
#[instrument(skip(input))]
pub fn build_benchmark_evaluation_artifact(
    input: BenchmarkEvaluationArtifactInput,
) -> Result<BenchmarkEvaluationArtifact> {
    let BenchmarkEvaluationArtifactInput {
        benchmark_suite_id,
        baseline_policy_id,
        candidate_policy_id,
        generated_at_epoch_ms,
        policy_improvement,
        seed_reproducibility,
        sample_size_sensitivity,
        checkpoint_promotion,
    } = input;

    if benchmark_suite_id.trim().is_empty() {
        bail!("benchmark_suite_id must not be blank");
    }
    if baseline_policy_id.trim().is_empty() {
        bail!("baseline_policy_id must not be blank");
    }
    if candidate_policy_id.trim().is_empty() {
        bail!("candidate_policy_id must not be blank");
    }

    Ok(BenchmarkEvaluationArtifact {
        schema_version: BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1,
        benchmark_suite_id,
        baseline_policy_id,
        candidate_policy_id,
        generated_at_epoch_ms,
        policy_improvement,
        seed_reproducibility,
        sample_size_sensitivity,
        checkpoint_promotion,
    })
}

/// Persists a benchmark artifact to a deterministic JSON file.
#[instrument(skip(artifact, output_dir))]
pub fn export_benchmark_evaluation_artifact(
    artifact: &BenchmarkEvaluationArtifact,
    output_dir: impl AsRef<Path>,
) -> Result<BenchmarkArtifactExportSummary> {
    let output_dir = output_dir.as_ref();

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "benchmark artifact export destination is not a directory: {}",
            output_dir.display()
        );
    }

    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create benchmark artifact output directory {}",
            output_dir.display()
        )
    })?;

    let file_name = deterministic_file_name(artifact);
    let path = output_dir.join(file_name);
    let payload = serde_json::to_vec_pretty(&artifact.to_json_value())?;
    std::fs::write(&path, &payload)
        .with_context(|| format!("failed to write benchmark artifact {}", path.display()))?;

    Ok(BenchmarkArtifactExportSummary {
        path,
        bytes_written: payload.len(),
    })
}

fn seed_reproducibility_to_json(report: &SeedReproducibilityReport) -> Value {
    json!({
        "sample_size": report.sample_size,
        "run_count": report.run_count,
        "p_value_range": report.p_value_range,
        "reward_delta_range": report.reward_delta_range,
        "within_band": report.within_band,
    })
}

fn sample_size_sensitivity_to_json(report: &SampleSizeSensitivityReport) -> Value {
    json!({
        "seed": report.seed,
        "points": report.points.iter().map(|point| {
            json!({
                "sample_size": point.sample_size,
                "mean_p_value": point.mean_p_value,
                "mean_reward_delta": point.mean_reward_delta,
            })
        }).collect::<Vec<_>>(),
        "max_p_value_drift": report.max_p_value_drift,
        "max_reward_delta_drift": report.max_reward_delta_drift,
        "within_band": report.within_band,
    })
}

fn checkpoint_promotion_to_json(decision: &CheckpointPromotionDecision) -> Value {
    json!({
        "promotion_allowed": decision.promotion_allowed,
        "safety_regression": decision.safety_regression,
        "max_safety_regression": decision.max_safety_regression,
        "reason_codes": decision.reason_codes,
    })
}

fn deterministic_file_name(artifact: &BenchmarkEvaluationArtifact) -> String {
    format!(
        "benchmark-{}-{}-vs-{}-{}.json",
        sanitize_file_component(&artifact.benchmark_suite_id),
        sanitize_file_component(&artifact.baseline_policy_id),
        sanitize_file_component(&artifact.candidate_policy_id),
        artifact.generated_at_epoch_ms
    )
}

fn sanitize_file_component(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut previous_was_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_was_dash = false;
        } else if !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_benchmark_evaluation_artifact, export_benchmark_evaluation_artifact,
        BenchmarkEvaluationArtifactInput,
    };
    use crate::benchmark_significance::{
        compare_policy_improvement, CheckpointPromotionDecision, SampleSizePoint,
        SampleSizeSensitivityReport, SeedReproducibilityReport,
    };
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_policy_report() -> crate::benchmark_significance::PolicyImprovementReport {
        let baseline = [0.20, 0.22, 0.24, 0.21, 0.23];
        let candidate = [0.30, 0.29, 0.32, 0.31, 0.28];
        compare_policy_improvement(&baseline, &candidate, 0.05).expect("policy report")
    }

    fn sample_artifact() -> super::BenchmarkEvaluationArtifact {
        build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "reasoning-suite".to_string(),
            baseline_policy_id: "policy-a".to_string(),
            candidate_policy_id: "policy-b".to_string(),
            generated_at_epoch_ms: 1_706_000_006_000,
            policy_improvement: sample_policy_report(),
            seed_reproducibility: None,
            sample_size_sensitivity: None,
            checkpoint_promotion: CheckpointPromotionDecision {
                promotion_allowed: true,
                safety_regression: 0.0,
                max_safety_regression: 0.05,
                reason_codes: Vec::new(),
            },
        })
        .expect("sample artifact")
    }

    fn temp_output_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("tau-{prefix}-{}-{nanos}", std::process::id()));
        if path.exists() {
            fs::remove_dir_all(&path).expect("remove pre-existing temp path");
        }
        path
    }

    #[test]
    fn spec_1966_c01_artifact_builder_returns_deterministic_typed_bundle() {
        let artifact = build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "reasoning-suite".to_string(),
            baseline_policy_id: "policy-baseline-v1".to_string(),
            candidate_policy_id: "policy-candidate-v2".to_string(),
            generated_at_epoch_ms: 1_706_000_001_000,
            policy_improvement: sample_policy_report(),
            seed_reproducibility: Some(SeedReproducibilityReport {
                sample_size: 256,
                run_count: 3,
                p_value_range: 0.02,
                reward_delta_range: 0.03,
                within_band: true,
            }),
            sample_size_sensitivity: Some(SampleSizeSensitivityReport {
                seed: 42,
                points: vec![
                    SampleSizePoint {
                        sample_size: 128,
                        mean_p_value: 0.04,
                        mean_reward_delta: 0.12,
                    },
                    SampleSizePoint {
                        sample_size: 256,
                        mean_p_value: 0.03,
                        mean_reward_delta: 0.13,
                    },
                ],
                max_p_value_drift: 0.01,
                max_reward_delta_drift: 0.01,
                within_band: true,
            }),
            checkpoint_promotion: CheckpointPromotionDecision {
                promotion_allowed: true,
                safety_regression: 0.01,
                max_safety_regression: 0.05,
                reason_codes: Vec::new(),
            },
        })
        .expect("artifact");

        assert_eq!(artifact.schema_version, 1);
        assert_eq!(artifact.benchmark_suite_id, "reasoning-suite");
        assert_eq!(artifact.baseline_policy_id, "policy-baseline-v1");
        assert_eq!(artifact.candidate_policy_id, "policy-candidate-v2");
    }

    #[test]
    fn spec_1966_c02_artifact_json_contains_schema_and_machine_readable_sections() {
        let artifact = build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "tool-use-suite".to_string(),
            baseline_policy_id: "policy-a".to_string(),
            candidate_policy_id: "policy-b".to_string(),
            generated_at_epoch_ms: 1_706_000_002_000,
            policy_improvement: sample_policy_report(),
            seed_reproducibility: None,
            sample_size_sensitivity: None,
            checkpoint_promotion: CheckpointPromotionDecision {
                promotion_allowed: false,
                safety_regression: 0.09,
                max_safety_regression: 0.05,
                reason_codes: vec!["checkpoint_promotion_blocked_safety_regression".to_string()],
            },
        })
        .expect("artifact");

        let payload = artifact.to_json_value();
        assert_eq!(payload["schema_version"], json!(1));
        assert_eq!(payload["benchmark_suite_id"], json!("tool-use-suite"));
        assert!(payload["policy_improvement"].is_object());
        assert!(payload["checkpoint_promotion"].is_object());
    }

    #[test]
    fn spec_1966_c03_artifact_preserves_promotion_reason_codes() {
        let reason_codes = vec![
            "checkpoint_promotion_blocked_safety_regression".to_string(),
            "checkpoint_promotion_blocked_seed_reproducibility".to_string(),
        ];

        let artifact = build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "reasoning-suite".to_string(),
            baseline_policy_id: "policy-a".to_string(),
            candidate_policy_id: "policy-b".to_string(),
            generated_at_epoch_ms: 1_706_000_003_000,
            policy_improvement: sample_policy_report(),
            seed_reproducibility: None,
            sample_size_sensitivity: None,
            checkpoint_promotion: CheckpointPromotionDecision {
                promotion_allowed: false,
                safety_regression: 0.08,
                max_safety_regression: 0.05,
                reason_codes: reason_codes.clone(),
            },
        })
        .expect("artifact");

        assert_eq!(artifact.checkpoint_promotion.reason_codes, reason_codes);
    }

    #[test]
    fn spec_1966_c04_optional_reproducibility_sections_serialize_as_null() {
        let artifact = build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "reasoning-suite".to_string(),
            baseline_policy_id: "policy-a".to_string(),
            candidate_policy_id: "policy-b".to_string(),
            generated_at_epoch_ms: 1_706_000_004_000,
            policy_improvement: sample_policy_report(),
            seed_reproducibility: None,
            sample_size_sensitivity: None,
            checkpoint_promotion: CheckpointPromotionDecision {
                promotion_allowed: true,
                safety_regression: 0.0,
                max_safety_regression: 0.05,
                reason_codes: Vec::new(),
            },
        })
        .expect("artifact");

        let payload = artifact.to_json_value();
        assert!(payload["seed_reproducibility"].is_null());
        assert!(payload["sample_size_sensitivity"].is_null());
    }

    #[test]
    fn regression_artifact_builder_rejects_empty_metadata_ids() {
        let error = build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "   ".to_string(),
            baseline_policy_id: "policy-a".to_string(),
            candidate_policy_id: "policy-b".to_string(),
            generated_at_epoch_ms: 1_706_000_005_000,
            policy_improvement: sample_policy_report(),
            seed_reproducibility: None,
            sample_size_sensitivity: None,
            checkpoint_promotion: CheckpointPromotionDecision {
                promotion_allowed: true,
                safety_regression: 0.0,
                max_safety_regression: 0.05,
                reason_codes: Vec::new(),
            },
        })
        .expect_err("empty benchmark suite id should fail");

        assert!(error.to_string().contains("benchmark_suite_id"));
    }

    #[test]
    fn spec_1968_c01_export_writes_deterministic_filename() {
        let artifact = sample_artifact();
        let output_dir = temp_output_dir("benchmark-export-c01");
        let summary = export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export");

        let file_name = summary.path.file_name().and_then(|value| value.to_str());
        assert_eq!(
            file_name,
            Some("benchmark-reasoning-suite-policy-a-vs-policy-b-1706000006000.json")
        );
        assert!(summary.path.exists());
        assert!(summary.bytes_written > 0);

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1968_c02_exported_json_matches_in_memory_artifact_payload() {
        let artifact = sample_artifact();
        let output_dir = temp_output_dir("benchmark-export-c02");
        let summary = export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export");

        let raw = fs::read_to_string(&summary.path).expect("read exported file");
        let expected = serde_json::to_string_pretty(&artifact.to_json_value())
            .expect("serialize expected payload");
        let _parsed: serde_json::Value = serde_json::from_str(&raw).expect("parse exported file");
        assert_eq!(raw, expected);

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1968_c03_export_creates_nested_output_directories() {
        let artifact = sample_artifact();
        let output_dir = temp_output_dir("benchmark-export-c03")
            .join("nested")
            .join("reports");
        let summary = export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export");

        assert!(summary.path.exists());
        assert!(output_dir.is_dir());

        let root = output_dir
            .parent()
            .and_then(|path| path.parent())
            .expect("nested root");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn spec_1968_c04_export_rejects_file_destination_path() {
        let artifact = sample_artifact();
        let root = temp_output_dir("benchmark-export-c04");
        fs::create_dir_all(&root).expect("create root");
        let file_path = root.join("not-a-directory");
        fs::write(&file_path, "occupied").expect("write file");

        let error = export_benchmark_evaluation_artifact(&artifact, &file_path)
            .expect_err("file destination should fail");
        assert!(error.to_string().contains("not a directory"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
