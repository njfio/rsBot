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

/// Valid artifact entry discovered during manifest scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactManifestEntry {
    /// Artifact file path.
    pub path: PathBuf,
    /// Artifact schema version.
    pub schema_version: u32,
    /// Benchmark suite id from artifact payload.
    pub benchmark_suite_id: String,
    /// Baseline policy id from artifact payload.
    pub baseline_policy_id: String,
    /// Candidate policy id from artifact payload.
    pub candidate_policy_id: String,
    /// Artifact generation timestamp from payload.
    pub generated_at_epoch_ms: u64,
}

/// Invalid artifact file diagnostic emitted during manifest scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactInvalidFile {
    /// Artifact file path.
    pub path: PathBuf,
    /// Deterministic diagnostic reason.
    pub reason: String,
}

/// Directory-level benchmark artifact manifest summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Scanned directory path.
    pub directory: PathBuf,
    /// Number of JSON files scanned.
    pub scanned_json_files: usize,
    /// Parsed valid artifact entries.
    pub valid_entries: Vec<BenchmarkArtifactManifestEntry>,
    /// Invalid artifact diagnostics.
    pub invalid_files: Vec<BenchmarkArtifactInvalidFile>,
}

impl BenchmarkArtifactManifest {
    /// Projects the manifest into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "directory": self.directory.display().to_string(),
            "scanned_json_files": self.scanned_json_files,
            "valid_entries": self.valid_entries.iter().map(|entry| {
                json!({
                    "path": entry.path.display().to_string(),
                    "schema_version": entry.schema_version,
                    "benchmark_suite_id": entry.benchmark_suite_id,
                    "baseline_policy_id": entry.baseline_policy_id,
                    "candidate_policy_id": entry.candidate_policy_id,
                    "generated_at_epoch_ms": entry.generated_at_epoch_ms,
                })
            }).collect::<Vec<_>>(),
            "invalid_files": self.invalid_files.iter().map(|entry| {
                json!({
                    "path": entry.path.display().to_string(),
                    "reason": entry.reason,
                })
            }).collect::<Vec<_>>(),
        })
    }
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

/// Loads and validates an exported benchmark artifact JSON file.
#[instrument(skip(path))]
pub fn validate_exported_benchmark_artifact(path: impl AsRef<Path>) -> Result<Value> {
    const REQUIRED_KEYS: [&str; 9] = [
        "schema_version",
        "benchmark_suite_id",
        "baseline_policy_id",
        "candidate_policy_id",
        "generated_at_epoch_ms",
        "policy_improvement",
        "seed_reproducibility",
        "sample_size_sensitivity",
        "checkpoint_promotion",
    ];

    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read benchmark artifact {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse benchmark artifact {}", path.display()))?;

    let Value::Object(object) = &value else {
        bail!("benchmark artifact must be a top-level JSON object");
    };

    for key in REQUIRED_KEYS {
        if !object.contains_key(key) {
            bail!("benchmark artifact missing required key: {key}");
        }
    }

    let Some(schema_version) = object.get("schema_version").and_then(Value::as_u64) else {
        bail!("benchmark artifact schema_version must be an unsigned integer");
    };
    if schema_version != u64::from(BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1) {
        bail!(
            "unsupported schema_version {schema_version}; expected {}",
            BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1
        );
    }

    Ok(value)
}

/// Builds a deterministic artifact manifest from a benchmark export directory.
#[instrument(skip(directory))]
pub fn build_benchmark_artifact_manifest(
    directory: impl AsRef<Path>,
) -> Result<BenchmarkArtifactManifest> {
    let directory = directory.as_ref();
    if !directory.exists() {
        bail!(
            "benchmark artifact manifest missing directory: {}",
            directory.display()
        );
    }
    if !directory.is_dir() {
        bail!(
            "benchmark artifact manifest path is not a directory: {}",
            directory.display()
        );
    }

    let mut json_paths = Vec::new();
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("failed to scan benchmark directory {}", directory.display()))?
    {
        let path = entry
            .with_context(|| {
                format!(
                    "failed to read benchmark directory entry in {}",
                    directory.display()
                )
            })?
            .path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("json"))
        {
            json_paths.push(path);
        }
    }

    json_paths.sort();
    let scanned_json_files = json_paths.len();

    let mut valid_entries = Vec::new();
    let mut invalid_files = Vec::new();
    for path in json_paths {
        match parse_manifest_entry(&path) {
            Ok(entry) => valid_entries.push(entry),
            Err(error) => invalid_files.push(BenchmarkArtifactInvalidFile {
                path,
                reason: error.to_string(),
            }),
        }
    }

    Ok(BenchmarkArtifactManifest {
        schema_version: BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1,
        directory: directory.to_path_buf(),
        scanned_json_files,
        valid_entries,
        invalid_files,
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

fn parse_manifest_entry(path: &Path) -> Result<BenchmarkArtifactManifestEntry> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read benchmark artifact {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse benchmark artifact {}", path.display()))?;
    let Value::Object(object) = value else {
        bail!("benchmark artifact must be a top-level JSON object");
    };

    let schema_version_raw = required_u64(&object, "schema_version")?;
    let schema_version = u32::try_from(schema_version_raw)
        .with_context(|| format!("schema_version out of range in {}", path.display()))?;
    if schema_version != BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1 {
        bail!(
            "unsupported schema_version {schema_version}; expected {}",
            BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1
        );
    }

    Ok(BenchmarkArtifactManifestEntry {
        path: path.to_path_buf(),
        schema_version,
        benchmark_suite_id: required_string(&object, "benchmark_suite_id")?,
        baseline_policy_id: required_string(&object, "baseline_policy_id")?,
        candidate_policy_id: required_string(&object, "candidate_policy_id")?,
        generated_at_epoch_ms: required_u64(&object, "generated_at_epoch_ms")?,
    })
}

fn required_string(object: &serde_json::Map<String, Value>, key: &'static str) -> Result<String> {
    match object.get(key) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(value.clone()),
        Some(_) => bail!("benchmark artifact key '{key}' must be a non-empty string"),
        None => bail!("benchmark artifact missing required key: {key}"),
    }
}

fn required_u64(object: &serde_json::Map<String, Value>, key: &'static str) -> Result<u64> {
    match object.get(key).and_then(Value::as_u64) {
        Some(value) => Ok(value),
        None if object.contains_key(key) => {
            bail!("benchmark artifact key '{key}' must be an unsigned integer")
        }
        None => bail!("benchmark artifact missing required key: {key}"),
    }
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
        build_benchmark_artifact_manifest, build_benchmark_evaluation_artifact,
        export_benchmark_evaluation_artifact, validate_exported_benchmark_artifact,
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

    #[test]
    fn spec_1970_c01_validator_accepts_valid_exported_artifact() {
        let artifact = sample_artifact();
        let output_dir = temp_output_dir("benchmark-validate-c01");
        let summary = export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export");

        let validated = validate_exported_benchmark_artifact(&summary.path)
            .expect("validate exported artifact");
        assert_eq!(validated["schema_version"], json!(1));
        assert_eq!(validated["benchmark_suite_id"], json!("reasoning-suite"));

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1970_c02_validator_rejects_malformed_json() {
        let output_dir = temp_output_dir("benchmark-validate-c02");
        fs::create_dir_all(&output_dir).expect("create output dir");
        let artifact_path = output_dir.join("invalid.json");
        fs::write(&artifact_path, "{ invalid-json").expect("write malformed artifact");

        let error = validate_exported_benchmark_artifact(&artifact_path)
            .expect_err("malformed JSON should fail");
        assert!(error.to_string().contains("failed to parse"));

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1970_c03_validator_reports_missing_required_keys() {
        let output_dir = temp_output_dir("benchmark-validate-c03");
        fs::create_dir_all(&output_dir).expect("create output dir");
        let artifact_path = output_dir.join("missing-key.json");
        fs::write(
            &artifact_path,
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "baseline_policy_id": "policy-a"
            }))
            .expect("serialize malformed artifact"),
        )
        .expect("write malformed artifact");

        let error = validate_exported_benchmark_artifact(&artifact_path)
            .expect_err("missing keys should fail");
        assert!(error.to_string().contains("missing required key"));
        assert!(error.to_string().contains("benchmark_suite_id"));

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1970_c04_validator_rejects_unsupported_schema_versions() {
        let output_dir = temp_output_dir("benchmark-validate-c04");
        fs::create_dir_all(&output_dir).expect("create output dir");
        let artifact_path = output_dir.join("unsupported-schema.json");
        fs::write(
            &artifact_path,
            serde_json::to_vec_pretty(&json!({
                "schema_version": 99,
                "benchmark_suite_id": "suite",
                "baseline_policy_id": "policy-a",
                "candidate_policy_id": "policy-b",
                "generated_at_epoch_ms": 1,
                "policy_improvement": {},
                "seed_reproducibility": null,
                "sample_size_sensitivity": null,
                "checkpoint_promotion": {}
            }))
            .expect("serialize artifact"),
        )
        .expect("write artifact");

        let error = validate_exported_benchmark_artifact(&artifact_path)
            .expect_err("unsupported schema should fail");
        assert!(error.to_string().contains("unsupported schema_version"));

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn regression_validator_rejects_non_object_payloads() {
        let output_dir = temp_output_dir("benchmark-validate-regression");
        fs::create_dir_all(&output_dir).expect("create output dir");
        let artifact_path = output_dir.join("array-payload.json");
        fs::write(
            &artifact_path,
            serde_json::to_vec_pretty(&json!([1, 2, 3])).expect("serialize array"),
        )
        .expect("write payload");

        let error = validate_exported_benchmark_artifact(&artifact_path)
            .expect_err("non-object payload should fail");
        assert!(error.to_string().contains("top-level JSON object"));

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1972_c01_manifest_builder_returns_sorted_deterministic_entries() {
        let output_dir = temp_output_dir("benchmark-manifest-c01");
        let artifact_b = sample_artifact();
        let artifact_a = build_benchmark_evaluation_artifact(BenchmarkEvaluationArtifactInput {
            benchmark_suite_id: "alpha-suite".to_string(),
            baseline_policy_id: "policy-1".to_string(),
            candidate_policy_id: "policy-2".to_string(),
            generated_at_epoch_ms: 1_706_000_010_000,
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

        // Export in reverse order to assert deterministic manifest sorting.
        export_benchmark_evaluation_artifact(&artifact_b, &output_dir).expect("export artifact b");
        export_benchmark_evaluation_artifact(&artifact_a, &output_dir).expect("export artifact a");

        let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
        assert_eq!(manifest.valid_entries.len(), 2);

        let first = manifest.valid_entries[0]
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("first filename");
        let second = manifest.valid_entries[1]
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("second filename");
        assert!(first < second);

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1972_c02_manifest_builder_reports_invalid_files_without_aborting_scan() {
        let output_dir = temp_output_dir("benchmark-manifest-c02");
        let artifact = sample_artifact();
        export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");

        let malformed_path = output_dir.join("broken-artifact.json");
        fs::write(&malformed_path, "{ malformed").expect("write malformed artifact");

        let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
        assert_eq!(manifest.scanned_json_files, 2);
        assert_eq!(manifest.valid_entries.len(), 1);
        assert_eq!(manifest.invalid_files.len(), 1);
        assert!(manifest.invalid_files[0].reason.contains("parse"));

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1972_c03_manifest_json_is_machine_readable_with_totals_and_diagnostics() {
        let output_dir = temp_output_dir("benchmark-manifest-c03");
        let artifact = sample_artifact();
        export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");

        let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
        let payload = manifest.to_json_value();
        assert_eq!(payload["schema_version"], json!(1));
        assert!(payload["valid_entries"].is_array());
        assert!(payload["invalid_files"].is_array());
        assert!(payload["scanned_json_files"].as_u64().is_some());

        fs::remove_dir_all(output_dir).expect("cleanup");
    }

    #[test]
    fn spec_1972_c04_manifest_builder_rejects_missing_directory() {
        let missing = temp_output_dir("benchmark-manifest-c04");
        let error =
            build_benchmark_artifact_manifest(&missing).expect_err("missing directory must fail");
        assert!(error.to_string().contains("missing directory"));
    }

    #[test]
    fn regression_manifest_builder_ignores_non_json_files() {
        let output_dir = temp_output_dir("benchmark-manifest-regression");
        let artifact = sample_artifact();
        export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");
        fs::write(output_dir.join("notes.txt"), "not a benchmark artifact").expect("write notes");

        let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
        assert_eq!(manifest.scanned_json_files, 1);
        assert_eq!(manifest.valid_entries.len(), 1);
        assert_eq!(manifest.invalid_files.len(), 0);

        fs::remove_dir_all(output_dir).expect("cleanup");
    }
}
