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

/// Input section for benchmark gate evidence in the M24 exit bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceBenchmarkInput {
    /// Whether benchmark evidence passed gate thresholds.
    pub pass: bool,
    /// Number of passing benchmark reports.
    pub pass_reports: usize,
    /// Number of failing benchmark reports.
    pub fail_reports: usize,
    /// Number of invalid benchmark files.
    pub invalid_files: usize,
    /// Deterministic benchmark reason codes.
    pub reason_codes: Vec<String>,
    /// Stable reference to benchmark evidence artifact.
    pub report_ref: String,
}

/// Input section for safety gate evidence in the M24 exit bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceSafetyInput {
    /// Whether safety evidence passed gate thresholds.
    pub pass: bool,
    /// Observed safety regression value.
    pub observed_regression: f64,
    /// Maximum allowed safety regression threshold.
    pub max_allowed_regression: f64,
    /// Deterministic safety reason codes.
    pub reason_codes: Vec<String>,
    /// Stable reference to safety evidence artifact.
    pub report_ref: String,
}

/// Input section for operations evidence in the M24 exit bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceOperationsInput {
    /// Whether pause/resume controls were proven.
    pub pause_resume_proven: bool,
    /// Whether rollback controls were proven.
    pub rollback_proven: bool,
    /// Whether crash recovery drill was proven.
    pub recovery_drill_proven: bool,
    /// Stable reference to operations recovery log.
    pub recovery_log_ref: String,
}

/// Input section for runbook evidence in the M24 exit bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M24RLGateEvidenceRunbooksInput {
    /// Stable operator runbook reference.
    pub operator_runbook_ref: String,
    /// Stable incident playbook reference.
    pub incident_playbook_ref: String,
}

/// Input payload consumed by the M24 RL gate evidence bundle builder.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceBundleInput {
    /// Bundle generation timestamp in Unix milliseconds.
    pub generated_at_epoch_ms: u64,
    /// Benchmark evidence section.
    pub benchmark: M24RLGateEvidenceBenchmarkInput,
    /// Safety evidence section.
    pub safety: M24RLGateEvidenceSafetyInput,
    /// Operations evidence section.
    pub operations: M24RLGateEvidenceOperationsInput,
    /// Runbook evidence section.
    pub runbooks: M24RLGateEvidenceRunbooksInput,
}

/// Benchmark section in the persisted M24 RL gate evidence bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceBenchmark {
    /// Whether benchmark evidence passed gate thresholds.
    pub pass: bool,
    /// Number of passing benchmark reports.
    pub pass_reports: usize,
    /// Number of failing benchmark reports.
    pub fail_reports: usize,
    /// Number of invalid benchmark files.
    pub invalid_files: usize,
    /// Deterministic benchmark reason codes.
    pub reason_codes: Vec<String>,
    /// Stable reference to benchmark evidence artifact.
    pub report_ref: String,
}

/// Safety section in the persisted M24 RL gate evidence bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceSafety {
    /// Whether safety evidence passed gate thresholds.
    pub pass: bool,
    /// Observed safety regression value.
    pub observed_regression: f64,
    /// Maximum allowed safety regression threshold.
    pub max_allowed_regression: f64,
    /// Deterministic safety reason codes.
    pub reason_codes: Vec<String>,
    /// Stable reference to safety evidence artifact.
    pub report_ref: String,
}

/// Operations section in the persisted M24 RL gate evidence bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M24RLGateEvidenceOperations {
    /// Whether all operations controls passed.
    pub pass: bool,
    /// Whether pause/resume controls were proven.
    pub pause_resume_proven: bool,
    /// Whether rollback controls were proven.
    pub rollback_proven: bool,
    /// Whether crash recovery drill was proven.
    pub recovery_drill_proven: bool,
    /// Stable reference to operations recovery log.
    pub recovery_log_ref: String,
}

/// Runbooks section in the persisted M24 RL gate evidence bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M24RLGateEvidenceRunbooks {
    /// Stable operator runbook reference.
    pub operator_runbook_ref: String,
    /// Stable incident playbook reference.
    pub incident_playbook_ref: String,
}

/// Machine-readable M24 RL gate evidence bundle payload.
#[derive(Debug, Clone, PartialEq)]
pub struct M24RLGateEvidenceBundle {
    /// Bundle schema version for compatibility checks.
    pub schema_version: u32,
    /// Bundle generation timestamp in Unix milliseconds.
    pub generated_at_epoch_ms: u64,
    /// Benchmark evidence section.
    pub benchmark: M24RLGateEvidenceBenchmark,
    /// Safety evidence section.
    pub safety: M24RLGateEvidenceSafety,
    /// Operations evidence section.
    pub operations: M24RLGateEvidenceOperations,
    /// Runbooks evidence section.
    pub runbooks: M24RLGateEvidenceRunbooks,
}

impl M24RLGateEvidenceBundle {
    /// Stable schema version for the M24 evidence bundle payload.
    pub const SCHEMA_VERSION_V1: u32 = 1;

    /// Projects the evidence bundle into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "generated_at_epoch_ms": self.generated_at_epoch_ms,
            "benchmark": {
                "pass": self.benchmark.pass,
                "pass_reports": self.benchmark.pass_reports,
                "fail_reports": self.benchmark.fail_reports,
                "invalid_files": self.benchmark.invalid_files,
                "reason_codes": self.benchmark.reason_codes,
                "report_ref": self.benchmark.report_ref,
            },
            "safety": {
                "pass": self.safety.pass,
                "observed_regression": self.safety.observed_regression,
                "max_allowed_regression": self.safety.max_allowed_regression,
                "reason_codes": self.safety.reason_codes,
                "report_ref": self.safety.report_ref,
            },
            "operations": {
                "pass": self.operations.pass,
                "pause_resume_proven": self.operations.pause_resume_proven,
                "rollback_proven": self.operations.rollback_proven,
                "recovery_drill_proven": self.operations.recovery_drill_proven,
                "recovery_log_ref": self.operations.recovery_log_ref,
            },
            "runbooks": {
                "operator_runbook_ref": self.runbooks.operator_runbook_ref,
                "incident_playbook_ref": self.runbooks.incident_playbook_ref,
            },
        })
    }
}

/// Deterministic M24 exit gate decision derived from bundle evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M24RLGateExitDecision {
    /// Whether the M24 exit gate passes.
    pub pass: bool,
    /// Whether benchmark evidence passes gate checks.
    pub benchmark_pass: bool,
    /// Whether safety evidence passes gate checks.
    pub safety_pass: bool,
    /// Whether operations evidence passes gate checks.
    pub operations_pass: bool,
    /// Whether both required runbook references are present.
    pub runbooks_present: bool,
    /// Deterministic reason codes when the gate fails.
    pub reason_codes: Vec<String>,
}

impl M24RLGateExitDecision {
    /// Projects the decision into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "pass": self.pass,
            "benchmark_pass": self.benchmark_pass,
            "safety_pass": self.safety_pass,
            "operations_pass": self.operations_pass,
            "runbooks_present": self.runbooks_present,
            "reason_codes": self.reason_codes,
        })
    }
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

/// Input counters used to evaluate manifest quality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactManifestQualityInput {
    /// Number of valid artifact entries in the manifest.
    pub valid_entries: usize,
    /// Number of invalid artifact files in the manifest.
    pub invalid_entries: usize,
}

/// Policy thresholds for manifest quality decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactManifestQualityPolicy {
    /// Minimum number of valid entries required to pass.
    pub min_valid_entries: usize,
    /// Maximum acceptable invalid ratio in `[0.0, 1.0]`.
    pub max_invalid_ratio: f64,
}

impl Default for BenchmarkArtifactManifestQualityPolicy {
    fn default() -> Self {
        Self {
            min_valid_entries: 1,
            max_invalid_ratio: 0.20,
        }
    }
}

/// Deterministic pass/fail decision for manifest quality.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactManifestQualityDecision {
    /// Whether the manifest passes quality policy.
    pub pass: bool,
    /// Number of valid entries considered.
    pub valid_entries: usize,
    /// Number of invalid entries considered.
    pub invalid_entries: usize,
    /// Total scanned entries (`valid + invalid`).
    pub scanned_entries: usize,
    /// Computed invalid ratio.
    pub invalid_ratio: f64,
    /// Policy threshold used for minimum valid entries.
    pub min_valid_entries: usize,
    /// Policy threshold used for maximum invalid ratio.
    pub max_invalid_ratio: f64,
    /// Deterministic reason codes for failures.
    pub reason_codes: Vec<String>,
}

impl BenchmarkArtifactManifestQualityDecision {
    /// Projects the decision into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "pass": self.pass,
            "valid_entries": self.valid_entries,
            "invalid_entries": self.invalid_entries,
            "scanned_entries": self.scanned_entries,
            "invalid_ratio": self.invalid_ratio,
            "min_valid_entries": self.min_valid_entries,
            "max_invalid_ratio": self.max_invalid_ratio,
            "reason_codes": self.reason_codes,
        })
    }
}

/// Combined manifest + quality decision gate report.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateReport {
    /// Source manifest produced by directory scan.
    pub manifest: BenchmarkArtifactManifest,
    /// Deterministic quality decision derived from the manifest.
    pub quality: BenchmarkArtifactManifestQualityDecision,
}

impl BenchmarkArtifactGateReport {
    /// Projects the gate report into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "manifest": self.manifest.to_json_value(),
            "quality": self.quality.to_json_value(),
        })
    }
}

/// Valid gate report entry discovered during summary-manifest scans.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateReportSummaryEntry {
    /// Gate report file path.
    pub path: PathBuf,
    /// Quality pass/fail result.
    pub pass: bool,
    /// Number of valid artifacts counted by the gate decision.
    pub valid_entries: usize,
    /// Number of invalid artifacts counted by the gate decision.
    pub invalid_entries: usize,
    /// Number of scanned artifacts counted by the gate decision.
    pub scanned_entries: usize,
    /// Invalid ratio from the quality decision.
    pub invalid_ratio: f64,
    /// Deterministic reason codes from the quality decision.
    pub reason_codes: Vec<String>,
}

/// Invalid gate report diagnostic emitted during summary-manifest scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactGateReportSummaryInvalidFile {
    /// Gate report file path.
    pub path: PathBuf,
    /// Deterministic diagnostic reason.
    pub reason: String,
}

/// Directory-level gate report summary manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateReportSummaryManifest {
    /// Summary manifest schema version.
    pub schema_version: u32,
    /// Scanned directory path.
    pub directory: PathBuf,
    /// Number of JSON files scanned.
    pub scanned_json_files: usize,
    /// Parsed valid gate report entries.
    pub entries: Vec<BenchmarkArtifactGateReportSummaryEntry>,
    /// Invalid gate report diagnostics.
    pub invalid_files: Vec<BenchmarkArtifactGateReportSummaryInvalidFile>,
    /// Number of passing entries.
    pub pass_entries: usize,
    /// Number of failing entries.
    pub fail_entries: usize,
}

impl BenchmarkArtifactGateReportSummaryManifest {
    /// Projects the summary manifest into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "directory": self.directory.display().to_string(),
            "scanned_json_files": self.scanned_json_files,
            "entries": self.entries.iter().map(|entry| {
                json!({
                    "path": entry.path.display().to_string(),
                    "pass": entry.pass,
                    "valid_entries": entry.valid_entries,
                    "invalid_entries": entry.invalid_entries,
                    "scanned_entries": entry.scanned_entries,
                    "invalid_ratio": entry.invalid_ratio,
                    "reason_codes": entry.reason_codes,
                })
            }).collect::<Vec<_>>(),
            "invalid_files": self.invalid_files.iter().map(|entry| {
                json!({
                    "path": entry.path.display().to_string(),
                    "reason": entry.reason,
                })
            }).collect::<Vec<_>>(),
            "pass_entries": self.pass_entries,
            "fail_entries": self.fail_entries,
        })
    }
}

/// Policy thresholds for gate report summary quality decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateReportSummaryQualityPolicy {
    /// Minimum number of passing entries required.
    pub min_pass_entries: usize,
    /// Maximum acceptable fail ratio in `[0.0, 1.0]`.
    pub max_fail_ratio: f64,
    /// Maximum acceptable invalid-file ratio in `[0.0, 1.0]`.
    pub max_invalid_file_ratio: f64,
}

impl Default for BenchmarkArtifactGateReportSummaryQualityPolicy {
    fn default() -> Self {
        Self {
            min_pass_entries: 1,
            max_fail_ratio: 0.50,
            max_invalid_file_ratio: 0.20,
        }
    }
}

/// Deterministic quality decision for a gate report summary manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateReportSummaryQualityDecision {
    /// Whether the summary passes policy thresholds.
    pub pass: bool,
    /// Number of passing entries considered.
    pub pass_entries: usize,
    /// Number of failing entries considered.
    pub fail_entries: usize,
    /// Number of invalid files considered.
    pub invalid_files: usize,
    /// Total evaluated entries (`pass + fail`).
    pub total_entries: usize,
    /// Computed fail ratio.
    pub fail_ratio: f64,
    /// Computed invalid-file ratio.
    pub invalid_file_ratio: f64,
    /// Policy threshold used for minimum pass entries.
    pub min_pass_entries: usize,
    /// Policy threshold used for maximum fail ratio.
    pub max_fail_ratio: f64,
    /// Policy threshold used for maximum invalid-file ratio.
    pub max_invalid_file_ratio: f64,
    /// Deterministic reason codes for failures.
    pub reason_codes: Vec<String>,
}

impl BenchmarkArtifactGateReportSummaryQualityDecision {
    /// Projects the quality decision into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "pass": self.pass,
            "pass_entries": self.pass_entries,
            "fail_entries": self.fail_entries,
            "invalid_files": self.invalid_files,
            "total_entries": self.total_entries,
            "fail_ratio": self.fail_ratio,
            "invalid_file_ratio": self.invalid_file_ratio,
            "min_pass_entries": self.min_pass_entries,
            "max_fail_ratio": self.max_fail_ratio,
            "max_invalid_file_ratio": self.max_invalid_file_ratio,
            "reason_codes": self.reason_codes,
        })
    }
}

/// Combined summary-manifest + summary-quality decision report.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateSummaryReport {
    /// Source summary manifest produced by directory scan.
    pub summary: BenchmarkArtifactGateReportSummaryManifest,
    /// Deterministic quality decision derived from the summary manifest.
    pub quality: BenchmarkArtifactGateReportSummaryQualityDecision,
}

impl BenchmarkArtifactGateSummaryReport {
    /// Projects the summary gate report into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "summary": self.summary.to_json_value(),
            "quality": self.quality.to_json_value(),
        })
    }
}

/// Valid summary gate report entry discovered during directory-manifest scans.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateSummaryReportManifestEntry {
    /// Summary gate report file path.
    pub path: PathBuf,
    /// Quality pass/fail result.
    pub pass: bool,
    /// Number of passing reports considered.
    pub pass_reports: usize,
    /// Number of failing reports considered.
    pub fail_reports: usize,
    /// Number of invalid files considered by summary quality decision.
    pub invalid_files: usize,
    /// Total reports considered (`pass_reports + fail_reports`).
    pub total_reports: usize,
    /// Computed fail ratio from summary quality decision.
    pub fail_ratio: f64,
    /// Computed invalid-file ratio from summary quality decision.
    pub invalid_file_ratio: f64,
    /// Deterministic reason codes from summary quality decision.
    pub reason_codes: Vec<String>,
}

/// Invalid summary gate report diagnostic emitted during directory-manifest scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkArtifactGateSummaryReportManifestInvalidFile {
    /// Summary gate report file path.
    pub path: PathBuf,
    /// Deterministic diagnostic reason.
    pub reason: String,
}

/// Directory-level summary gate report manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateSummaryReportManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Scanned directory path.
    pub directory: PathBuf,
    /// Number of JSON files scanned.
    pub scanned_json_files: usize,
    /// Parsed valid summary gate report entries.
    pub entries: Vec<BenchmarkArtifactGateSummaryReportManifestEntry>,
    /// Invalid summary gate report diagnostics.
    pub invalid_files: Vec<BenchmarkArtifactGateSummaryReportManifestInvalidFile>,
    /// Number of passing summary gate reports.
    pub pass_reports: usize,
    /// Number of failing summary gate reports.
    pub fail_reports: usize,
}

impl BenchmarkArtifactGateSummaryReportManifest {
    /// Projects the manifest into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "directory": self.directory.display().to_string(),
            "scanned_json_files": self.scanned_json_files,
            "entries": self.entries.iter().map(|entry| {
                json!({
                    "path": entry.path.display().to_string(),
                    "pass": entry.pass,
                    "pass_reports": entry.pass_reports,
                    "fail_reports": entry.fail_reports,
                    "invalid_files": entry.invalid_files,
                    "total_reports": entry.total_reports,
                    "fail_ratio": entry.fail_ratio,
                    "invalid_file_ratio": entry.invalid_file_ratio,
                    "reason_codes": entry.reason_codes,
                })
            }).collect::<Vec<_>>(),
            "invalid_files": self.invalid_files.iter().map(|entry| {
                json!({
                    "path": entry.path.display().to_string(),
                    "reason": entry.reason,
                })
            }).collect::<Vec<_>>(),
            "pass_reports": self.pass_reports,
            "fail_reports": self.fail_reports,
        })
    }
}

/// Policy thresholds for summary gate report manifest quality decisions.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
    /// Minimum number of passing reports required.
    pub min_pass_reports: usize,
    /// Maximum acceptable fail ratio in `[0.0, 1.0]`.
    pub max_fail_ratio: f64,
    /// Maximum acceptable invalid-file ratio in `[0.0, 1.0]`.
    pub max_invalid_file_ratio: f64,
}

impl Default for BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
    fn default() -> Self {
        Self {
            min_pass_reports: 1,
            max_fail_ratio: 0.50,
            max_invalid_file_ratio: 0.20,
        }
    }
}

/// Deterministic quality decision for a summary gate report manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateSummaryReportManifestQualityDecision {
    /// Whether the manifest passes policy thresholds.
    pub pass: bool,
    /// Number of passing reports considered.
    pub pass_reports: usize,
    /// Number of failing reports considered.
    pub fail_reports: usize,
    /// Number of invalid files considered.
    pub invalid_files: usize,
    /// Total number of evaluated reports (`pass + fail`).
    pub total_reports: usize,
    /// Computed fail ratio.
    pub fail_ratio: f64,
    /// Computed invalid-file ratio.
    pub invalid_file_ratio: f64,
    /// Policy threshold used for minimum pass reports.
    pub min_pass_reports: usize,
    /// Policy threshold used for maximum fail ratio.
    pub max_fail_ratio: f64,
    /// Policy threshold used for maximum invalid-file ratio.
    pub max_invalid_file_ratio: f64,
    /// Deterministic reason codes for failures.
    pub reason_codes: Vec<String>,
}

impl BenchmarkArtifactGateSummaryReportManifestQualityDecision {
    /// Projects the manifest-quality decision into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "pass": self.pass,
            "pass_reports": self.pass_reports,
            "fail_reports": self.fail_reports,
            "invalid_files": self.invalid_files,
            "total_reports": self.total_reports,
            "fail_ratio": self.fail_ratio,
            "invalid_file_ratio": self.invalid_file_ratio,
            "min_pass_reports": self.min_pass_reports,
            "max_fail_ratio": self.max_fail_ratio,
            "max_invalid_file_ratio": self.max_invalid_file_ratio,
            "reason_codes": self.reason_codes,
        })
    }
}

/// Combined summary gate report manifest + manifest-quality decision report.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkArtifactGateSummaryReportManifestReport {
    /// Source summary gate report manifest produced by directory scan.
    pub manifest: BenchmarkArtifactGateSummaryReportManifest,
    /// Deterministic quality decision derived from the manifest.
    pub quality: BenchmarkArtifactGateSummaryReportManifestQualityDecision,
}

impl BenchmarkArtifactGateSummaryReportManifestReport {
    /// Projects the combined report into machine-readable JSON.
    pub fn to_json_value(&self) -> Value {
        json!({
            "manifest": self.manifest.to_json_value(),
            "quality": self.quality.to_json_value(),
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

/// Builds a deterministic M24 RL gate evidence bundle.
#[instrument(skip(input))]
pub fn build_m24_rl_gate_evidence_bundle(
    input: M24RLGateEvidenceBundleInput,
) -> Result<M24RLGateEvidenceBundle> {
    let M24RLGateEvidenceBundleInput {
        generated_at_epoch_ms,
        benchmark,
        safety,
        operations,
        runbooks,
    } = input;

    if benchmark.report_ref.trim().is_empty() {
        bail!("m24 benchmark report_ref must not be blank");
    }
    if !safety.observed_regression.is_finite() {
        bail!("m24 safety observed_regression must be finite");
    }
    if !safety.max_allowed_regression.is_finite() || safety.max_allowed_regression < 0.0 {
        bail!("m24 safety max_allowed_regression must be finite and non-negative");
    }
    if safety.report_ref.trim().is_empty() {
        bail!("m24 safety report_ref must not be blank");
    }
    if operations.recovery_log_ref.trim().is_empty() {
        bail!("m24 operations recovery_log_ref must not be blank");
    }
    if runbooks.operator_runbook_ref.trim().is_empty() {
        bail!("m24 runbooks operator_runbook_ref must not be blank");
    }
    if runbooks.incident_playbook_ref.trim().is_empty() {
        bail!("m24 runbooks incident_playbook_ref must not be blank");
    }

    let operations_pass = operations.pause_resume_proven
        && operations.rollback_proven
        && operations.recovery_drill_proven;

    Ok(M24RLGateEvidenceBundle {
        schema_version: M24RLGateEvidenceBundle::SCHEMA_VERSION_V1,
        generated_at_epoch_ms,
        benchmark: M24RLGateEvidenceBenchmark {
            pass: benchmark.pass,
            pass_reports: benchmark.pass_reports,
            fail_reports: benchmark.fail_reports,
            invalid_files: benchmark.invalid_files,
            reason_codes: benchmark.reason_codes,
            report_ref: benchmark.report_ref,
        },
        safety: M24RLGateEvidenceSafety {
            pass: safety.pass,
            observed_regression: safety.observed_regression,
            max_allowed_regression: safety.max_allowed_regression,
            reason_codes: safety.reason_codes,
            report_ref: safety.report_ref,
        },
        operations: M24RLGateEvidenceOperations {
            pass: operations_pass,
            pause_resume_proven: operations.pause_resume_proven,
            rollback_proven: operations.rollback_proven,
            recovery_drill_proven: operations.recovery_drill_proven,
            recovery_log_ref: operations.recovery_log_ref,
        },
        runbooks: M24RLGateEvidenceRunbooks {
            operator_runbook_ref: runbooks.operator_runbook_ref,
            incident_playbook_ref: runbooks.incident_playbook_ref,
        },
    })
}

/// Persists an M24 RL gate evidence bundle to a deterministic JSON file.
#[instrument(skip(bundle, output_dir))]
pub fn export_m24_rl_gate_evidence_bundle(
    bundle: &M24RLGateEvidenceBundle,
    output_dir: impl AsRef<Path>,
) -> Result<BenchmarkArtifactExportSummary> {
    let output_dir = output_dir.as_ref();

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "m24 rl gate evidence export destination is not a directory: {}",
            output_dir.display()
        );
    }

    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create m24 rl gate evidence output directory {}",
            output_dir.display()
        )
    })?;

    let path = output_dir.join(deterministic_m24_rl_gate_evidence_bundle_file_name(bundle));
    let payload = serde_json::to_vec_pretty(&bundle.to_json_value())?;
    std::fs::write(&path, &payload)
        .with_context(|| format!("failed to write m24 rl gate evidence {}", path.display()))?;

    Ok(BenchmarkArtifactExportSummary {
        path,
        bytes_written: payload.len(),
    })
}

/// Loads and validates an exported M24 RL gate evidence bundle JSON file.
#[instrument(skip(path))]
pub fn validate_exported_m24_rl_gate_evidence_bundle(path: impl AsRef<Path>) -> Result<Value> {
    const REQUIRED_KEYS: [&str; 4] = ["benchmark", "safety", "operations", "runbooks"];

    let path = path.as_ref();
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read m24 rl gate evidence bundle {}",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse m24 rl gate evidence bundle {}",
            path.display()
        )
    })?;

    let Value::Object(object) = &value else {
        bail!("m24 rl gate evidence bundle must be a top-level JSON object");
    };

    for key in REQUIRED_KEYS {
        match object.get(key) {
            Some(Value::Object(_)) => {}
            Some(_) => bail!("m24 rl gate evidence bundle key '{key}' must be an object"),
            None => bail!("m24 rl gate evidence bundle missing required key: {key}"),
        }
    }

    Ok(value)
}

/// Evaluates M24 exit readiness from a deterministic evidence bundle.
#[instrument(skip(bundle))]
pub fn evaluate_m24_rl_gate_exit(bundle: &M24RLGateEvidenceBundle) -> M24RLGateExitDecision {
    let mut reason_codes = Vec::new();

    if !bundle.benchmark.pass {
        reason_codes.push("benchmark_failed".to_string());
    }
    if !bundle.safety.pass {
        reason_codes.push("safety_failed".to_string());
    }
    if !bundle.operations.pass {
        reason_codes.push("operations_failed".to_string());
    }

    let operator_runbook_present = !bundle.runbooks.operator_runbook_ref.trim().is_empty();
    let incident_playbook_present = !bundle.runbooks.incident_playbook_ref.trim().is_empty();
    if !operator_runbook_present {
        reason_codes.push("operator_runbook_missing".to_string());
    }
    if !incident_playbook_present {
        reason_codes.push("incident_playbook_missing".to_string());
    }

    M24RLGateExitDecision {
        pass: reason_codes.is_empty(),
        benchmark_pass: bundle.benchmark.pass,
        safety_pass: bundle.safety.pass,
        operations_pass: bundle.operations.pass,
        runbooks_present: operator_runbook_present && incident_playbook_present,
        reason_codes,
    }
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

/// Evaluates manifest counters against a deterministic quality policy.
#[instrument(skip(manifest, policy))]
pub fn evaluate_benchmark_manifest_quality(
    manifest: &BenchmarkArtifactManifestQualityInput,
    policy: &BenchmarkArtifactManifestQualityPolicy,
) -> Result<BenchmarkArtifactManifestQualityDecision> {
    if !policy.max_invalid_ratio.is_finite() || !(0.0..=1.0).contains(&policy.max_invalid_ratio) {
        bail!("max_invalid_ratio must be finite and in [0.0, 1.0]");
    }

    let scanned_entries = manifest.valid_entries + manifest.invalid_entries;
    let invalid_ratio = if scanned_entries == 0 {
        0.0
    } else {
        manifest.invalid_entries as f64 / scanned_entries as f64
    };

    let mut reason_codes = Vec::new();
    if manifest.valid_entries == 0 {
        reason_codes.push("no_valid_artifacts".to_string());
    } else if manifest.valid_entries < policy.min_valid_entries {
        reason_codes.push("below_min_valid_entries".to_string());
    }
    if invalid_ratio > policy.max_invalid_ratio {
        reason_codes.push("invalid_ratio_exceeded".to_string());
    }

    Ok(BenchmarkArtifactManifestQualityDecision {
        pass: reason_codes.is_empty(),
        valid_entries: manifest.valid_entries,
        invalid_entries: manifest.invalid_entries,
        scanned_entries,
        invalid_ratio,
        min_valid_entries: policy.min_valid_entries,
        max_invalid_ratio: policy.max_invalid_ratio,
        reason_codes,
    })
}

/// Builds a combined quality gate report from a scanned artifact manifest.
#[instrument(skip(manifest, policy))]
pub fn build_benchmark_artifact_gate_report(
    manifest: &BenchmarkArtifactManifest,
    policy: &BenchmarkArtifactManifestQualityPolicy,
) -> Result<BenchmarkArtifactGateReport> {
    let quality_input = BenchmarkArtifactManifestQualityInput {
        valid_entries: manifest.valid_entries.len(),
        invalid_entries: manifest.invalid_files.len(),
    };
    let quality = evaluate_benchmark_manifest_quality(&quality_input, policy)?;
    Ok(BenchmarkArtifactGateReport {
        manifest: manifest.clone(),
        quality,
    })
}

/// Persists a benchmark artifact gate report to a deterministic JSON file.
#[instrument(skip(report, output_dir))]
pub fn export_benchmark_artifact_gate_report(
    report: &BenchmarkArtifactGateReport,
    output_dir: impl AsRef<Path>,
) -> Result<BenchmarkArtifactExportSummary> {
    let output_dir = output_dir.as_ref();

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "benchmark gate report export destination is not a directory: {}",
            output_dir.display()
        );
    }

    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create benchmark gate report output directory {}",
            output_dir.display()
        )
    })?;

    let path = output_dir.join(deterministic_gate_report_file_name(report));
    let payload = serde_json::to_vec_pretty(&report.to_json_value())?;
    std::fs::write(&path, &payload)
        .with_context(|| format!("failed to write benchmark gate report {}", path.display()))?;

    Ok(BenchmarkArtifactExportSummary {
        path,
        bytes_written: payload.len(),
    })
}

/// Loads and validates an exported benchmark artifact gate report JSON file.
#[instrument(skip(path))]
pub fn validate_exported_benchmark_artifact_gate_report(path: impl AsRef<Path>) -> Result<Value> {
    const REQUIRED_KEYS: [&str; 2] = ["manifest", "quality"];

    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read benchmark gate report {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse benchmark gate report {}", path.display()))?;

    let Value::Object(object) = &value else {
        bail!("benchmark gate report must be a top-level JSON object");
    };

    for key in REQUIRED_KEYS {
        match object.get(key) {
            Some(Value::Object(_)) => {}
            Some(_) => bail!("benchmark gate report key '{key}' must be an object"),
            None => bail!("benchmark gate report missing required key: {key}"),
        }
    }

    Ok(value)
}

/// Builds a deterministic summary manifest from exported gate report files.
#[instrument(skip(directory))]
pub fn build_benchmark_artifact_gate_report_summary_manifest(
    directory: impl AsRef<Path>,
) -> Result<BenchmarkArtifactGateReportSummaryManifest> {
    let directory = directory.as_ref();
    if !directory.exists() {
        bail!(
            "benchmark gate report summary missing directory: {}",
            directory.display()
        );
    }
    if !directory.is_dir() {
        bail!(
            "benchmark gate report summary path is not a directory: {}",
            directory.display()
        );
    }

    let mut json_paths = Vec::new();
    for entry in std::fs::read_dir(directory).with_context(|| {
        format!(
            "failed to scan benchmark gate report directory {}",
            directory.display()
        )
    })? {
        let path = entry
            .with_context(|| {
                format!(
                    "failed to read benchmark gate report directory entry in {}",
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
    let mut entries = Vec::new();
    let mut invalid_files = Vec::new();
    for path in json_paths {
        match parse_gate_report_summary_entry(&path) {
            Ok(entry) => entries.push(entry),
            Err(error) => invalid_files.push(BenchmarkArtifactGateReportSummaryInvalidFile {
                path,
                reason: error.to_string(),
            }),
        }
    }

    let pass_entries = entries.iter().filter(|entry| entry.pass).count();
    let fail_entries = entries.len().saturating_sub(pass_entries);

    Ok(BenchmarkArtifactGateReportSummaryManifest {
        schema_version: BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1,
        directory: directory.to_path_buf(),
        scanned_json_files,
        entries,
        invalid_files,
        pass_entries,
        fail_entries,
    })
}

/// Evaluates a gate report summary manifest against a deterministic quality policy.
#[instrument(skip(summary, policy))]
pub fn evaluate_benchmark_gate_report_summary_quality(
    summary: &BenchmarkArtifactGateReportSummaryManifest,
    policy: &BenchmarkArtifactGateReportSummaryQualityPolicy,
) -> Result<BenchmarkArtifactGateReportSummaryQualityDecision> {
    if !policy.max_fail_ratio.is_finite() || !(0.0..=1.0).contains(&policy.max_fail_ratio) {
        bail!("max_fail_ratio must be finite and in [0.0, 1.0]");
    }
    if !policy.max_invalid_file_ratio.is_finite()
        || !(0.0..=1.0).contains(&policy.max_invalid_file_ratio)
    {
        bail!("max_invalid_file_ratio must be finite and in [0.0, 1.0]");
    }

    let pass_entries = summary.pass_entries;
    let fail_entries = summary.fail_entries;
    let invalid_files = summary.invalid_files.len();
    let total_entries = pass_entries + fail_entries;

    let fail_ratio = if total_entries == 0 {
        0.0
    } else {
        fail_entries as f64 / total_entries as f64
    };
    let invalid_file_ratio = if summary.scanned_json_files == 0 {
        0.0
    } else {
        invalid_files as f64 / summary.scanned_json_files as f64
    };

    let mut reason_codes = Vec::new();
    if pass_entries < policy.min_pass_entries {
        reason_codes.push("below_min_pass_entries".to_string());
    }
    if fail_ratio > policy.max_fail_ratio {
        reason_codes.push("fail_ratio_exceeded".to_string());
    }
    if invalid_file_ratio > policy.max_invalid_file_ratio {
        reason_codes.push("invalid_file_ratio_exceeded".to_string());
    }

    Ok(BenchmarkArtifactGateReportSummaryQualityDecision {
        pass: reason_codes.is_empty(),
        pass_entries,
        fail_entries,
        invalid_files,
        total_entries,
        fail_ratio,
        invalid_file_ratio,
        min_pass_entries: policy.min_pass_entries,
        max_fail_ratio: policy.max_fail_ratio,
        max_invalid_file_ratio: policy.max_invalid_file_ratio,
        reason_codes,
    })
}

/// Builds a combined summary-quality gate report from a summary manifest.
#[instrument(skip(summary, policy))]
pub fn build_benchmark_artifact_gate_summary_report(
    summary: &BenchmarkArtifactGateReportSummaryManifest,
    policy: &BenchmarkArtifactGateReportSummaryQualityPolicy,
) -> Result<BenchmarkArtifactGateSummaryReport> {
    let quality = evaluate_benchmark_gate_report_summary_quality(summary, policy)?;
    Ok(BenchmarkArtifactGateSummaryReport {
        summary: summary.clone(),
        quality,
    })
}

/// Persists a summary gate report to a deterministic JSON file.
#[instrument(skip(report, output_dir))]
pub fn export_benchmark_artifact_gate_summary_report(
    report: &BenchmarkArtifactGateSummaryReport,
    output_dir: impl AsRef<Path>,
) -> Result<BenchmarkArtifactExportSummary> {
    let output_dir = output_dir.as_ref();

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "benchmark summary gate report export destination is not a directory: {}",
            output_dir.display()
        );
    }

    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create benchmark summary gate report output directory {}",
            output_dir.display()
        )
    })?;

    let path = output_dir.join(deterministic_summary_gate_report_file_name(report));
    let payload = serde_json::to_vec_pretty(&report.to_json_value())?;
    std::fs::write(&path, &payload).with_context(|| {
        format!(
            "failed to write benchmark summary gate report {}",
            path.display()
        )
    })?;

    Ok(BenchmarkArtifactExportSummary {
        path,
        bytes_written: payload.len(),
    })
}

/// Loads and validates an exported summary gate report JSON file.
#[instrument(skip(path))]
pub fn validate_exported_benchmark_artifact_gate_summary_report(
    path: impl AsRef<Path>,
) -> Result<Value> {
    const REQUIRED_KEYS: [&str; 2] = ["summary", "quality"];

    let path = path.as_ref();
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read benchmark summary gate report {}",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse benchmark summary gate report {}",
            path.display()
        )
    })?;

    let Value::Object(object) = &value else {
        bail!("benchmark summary gate report must be a top-level JSON object");
    };

    for key in REQUIRED_KEYS {
        match object.get(key) {
            Some(Value::Object(_)) => {}
            Some(_) => bail!("benchmark summary gate report key '{key}' must be an object"),
            None => bail!("benchmark summary gate report missing required key: {key}"),
        }
    }

    Ok(value)
}

/// Builds a deterministic manifest from exported summary gate report files.
#[instrument(skip(directory))]
pub fn build_benchmark_artifact_gate_summary_report_manifest(
    directory: impl AsRef<Path>,
) -> Result<BenchmarkArtifactGateSummaryReportManifest> {
    let directory = directory.as_ref();
    if !directory.exists() {
        bail!(
            "benchmark summary gate report manifest missing directory: {}",
            directory.display()
        );
    }
    if !directory.is_dir() {
        bail!(
            "benchmark summary gate report manifest path is not a directory: {}",
            directory.display()
        );
    }

    let mut json_paths = Vec::new();
    for entry in std::fs::read_dir(directory).with_context(|| {
        format!(
            "failed to scan benchmark summary gate report directory {}",
            directory.display()
        )
    })? {
        let path = entry
            .with_context(|| {
                format!(
                    "failed to read benchmark summary gate report directory entry in {}",
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
    let mut entries = Vec::new();
    let mut invalid_files = Vec::new();
    for path in json_paths {
        match parse_gate_summary_report_manifest_entry(&path) {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                invalid_files.push(BenchmarkArtifactGateSummaryReportManifestInvalidFile {
                    path,
                    reason: error.to_string(),
                })
            }
        }
    }

    let pass_reports = entries.iter().filter(|entry| entry.pass).count();
    let fail_reports = entries.len().saturating_sub(pass_reports);

    Ok(BenchmarkArtifactGateSummaryReportManifest {
        schema_version: BenchmarkEvaluationArtifact::SCHEMA_VERSION_V1,
        directory: directory.to_path_buf(),
        scanned_json_files,
        entries,
        invalid_files,
        pass_reports,
        fail_reports,
    })
}

/// Evaluates a summary gate report manifest against a deterministic quality policy.
#[instrument(skip(manifest, policy))]
pub fn evaluate_benchmark_gate_summary_report_manifest_quality(
    manifest: &BenchmarkArtifactGateSummaryReportManifest,
    policy: &BenchmarkArtifactGateSummaryReportManifestQualityPolicy,
) -> Result<BenchmarkArtifactGateSummaryReportManifestQualityDecision> {
    if !policy.max_fail_ratio.is_finite() || !(0.0..=1.0).contains(&policy.max_fail_ratio) {
        bail!("max_fail_ratio must be finite and in [0.0, 1.0]");
    }
    if !policy.max_invalid_file_ratio.is_finite()
        || !(0.0..=1.0).contains(&policy.max_invalid_file_ratio)
    {
        bail!("max_invalid_file_ratio must be finite and in [0.0, 1.0]");
    }

    let pass_reports = manifest.pass_reports;
    let fail_reports = manifest.fail_reports;
    let invalid_files = manifest.invalid_files.len();
    let total_reports = pass_reports + fail_reports;

    let fail_ratio = if total_reports == 0 {
        0.0
    } else {
        fail_reports as f64 / total_reports as f64
    };
    let invalid_file_ratio = if manifest.scanned_json_files == 0 {
        0.0
    } else {
        invalid_files as f64 / manifest.scanned_json_files as f64
    };

    let mut reason_codes = Vec::new();
    if pass_reports < policy.min_pass_reports {
        reason_codes.push("below_min_pass_reports".to_string());
    }
    if fail_ratio > policy.max_fail_ratio {
        reason_codes.push("fail_ratio_exceeded".to_string());
    }
    if invalid_file_ratio > policy.max_invalid_file_ratio {
        reason_codes.push("invalid_file_ratio_exceeded".to_string());
    }

    Ok(BenchmarkArtifactGateSummaryReportManifestQualityDecision {
        pass: reason_codes.is_empty(),
        pass_reports,
        fail_reports,
        invalid_files,
        total_reports,
        fail_ratio,
        invalid_file_ratio,
        min_pass_reports: policy.min_pass_reports,
        max_fail_ratio: policy.max_fail_ratio,
        max_invalid_file_ratio: policy.max_invalid_file_ratio,
        reason_codes,
    })
}

/// Builds a combined summary gate manifest report from manifest + quality policy.
#[instrument(skip(manifest, policy))]
pub fn build_benchmark_artifact_gate_summary_report_manifest_report(
    manifest: &BenchmarkArtifactGateSummaryReportManifest,
    policy: &BenchmarkArtifactGateSummaryReportManifestQualityPolicy,
) -> Result<BenchmarkArtifactGateSummaryReportManifestReport> {
    let quality = evaluate_benchmark_gate_summary_report_manifest_quality(manifest, policy)?;
    Ok(BenchmarkArtifactGateSummaryReportManifestReport {
        manifest: manifest.clone(),
        quality,
    })
}

/// Persists a combined summary gate manifest report to a deterministic JSON file.
#[instrument(skip(report, output_dir))]
pub fn export_benchmark_artifact_gate_summary_report_manifest_report(
    report: &BenchmarkArtifactGateSummaryReportManifestReport,
    output_dir: impl AsRef<Path>,
) -> Result<BenchmarkArtifactExportSummary> {
    let output_dir = output_dir.as_ref();

    if output_dir.exists() && !output_dir.is_dir() {
        bail!(
            "benchmark summary manifest report export destination is not a directory: {}",
            output_dir.display()
        );
    }

    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create benchmark summary manifest report output directory {}",
            output_dir.display()
        )
    })?;

    let path = output_dir.join(deterministic_summary_manifest_report_file_name(report));
    let payload = serde_json::to_vec_pretty(&report.to_json_value())?;
    std::fs::write(&path, &payload).with_context(|| {
        format!(
            "failed to write benchmark summary manifest report {}",
            path.display()
        )
    })?;

    Ok(BenchmarkArtifactExportSummary {
        path,
        bytes_written: payload.len(),
    })
}

/// Loads and validates an exported combined summary gate manifest report JSON file.
#[instrument(skip(path))]
pub fn validate_exported_benchmark_artifact_gate_summary_report_manifest_report(
    path: impl AsRef<Path>,
) -> Result<Value> {
    const REQUIRED_KEYS: [&str; 2] = ["manifest", "quality"];

    let path = path.as_ref();
    let raw = std::fs::read_to_string(path).with_context(|| {
        format!(
            "failed to read benchmark summary manifest report {}",
            path.display()
        )
    })?;
    let value: Value = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse benchmark summary manifest report {}",
            path.display()
        )
    })?;

    let Value::Object(object) = &value else {
        bail!("benchmark summary manifest report must be a top-level JSON object");
    };

    for key in REQUIRED_KEYS {
        match object.get(key) {
            Some(Value::Object(_)) => {}
            Some(_) => bail!("benchmark summary manifest report key '{key}' must be an object"),
            None => bail!("benchmark summary manifest report missing required key: {key}"),
        }
    }

    Ok(value)
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

fn parse_gate_report_summary_entry(path: &Path) -> Result<BenchmarkArtifactGateReportSummaryEntry> {
    let value = validate_exported_benchmark_artifact_gate_report(path)?;
    let Value::Object(root) = value else {
        bail!("benchmark gate report must be a top-level JSON object");
    };
    let Some(Value::Object(quality)) = root.get("quality") else {
        bail!("benchmark gate report missing required key: quality");
    };

    Ok(BenchmarkArtifactGateReportSummaryEntry {
        path: path.to_path_buf(),
        pass: gate_report_required_bool(quality, "pass")?,
        valid_entries: usize::try_from(gate_report_required_u64(quality, "valid_entries")?)?,
        invalid_entries: usize::try_from(gate_report_required_u64(quality, "invalid_entries")?)?,
        scanned_entries: usize::try_from(gate_report_required_u64(quality, "scanned_entries")?)?,
        invalid_ratio: gate_report_required_f64(quality, "invalid_ratio")?,
        reason_codes: gate_report_required_string_vec(quality, "reason_codes")?,
    })
}

fn parse_gate_summary_report_manifest_entry(
    path: &Path,
) -> Result<BenchmarkArtifactGateSummaryReportManifestEntry> {
    let value = validate_exported_benchmark_artifact_gate_summary_report(path)?;
    let Value::Object(root) = value else {
        bail!("benchmark summary gate report must be a top-level JSON object");
    };
    let Some(Value::Object(quality)) = root.get("quality") else {
        bail!("benchmark summary gate report missing required key: quality");
    };

    Ok(BenchmarkArtifactGateSummaryReportManifestEntry {
        path: path.to_path_buf(),
        pass: gate_report_required_bool(quality, "pass")?,
        pass_reports: usize::try_from(gate_report_required_u64(quality, "pass_entries")?)?,
        fail_reports: usize::try_from(gate_report_required_u64(quality, "fail_entries")?)?,
        invalid_files: usize::try_from(gate_report_required_u64(quality, "invalid_files")?)?,
        total_reports: usize::try_from(gate_report_required_u64(quality, "total_entries")?)?,
        fail_ratio: gate_report_required_f64(quality, "fail_ratio")?,
        invalid_file_ratio: gate_report_required_f64(quality, "invalid_file_ratio")?,
        reason_codes: gate_report_required_string_vec(quality, "reason_codes")?,
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

fn gate_report_required_bool(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<bool> {
    match object.get(key).and_then(Value::as_bool) {
        Some(value) => Ok(value),
        None if object.contains_key(key) => {
            bail!("benchmark gate report key '{key}' must be a boolean")
        }
        None => bail!("benchmark gate report missing required key: {key}"),
    }
}

fn gate_report_required_u64(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<u64> {
    match object.get(key).and_then(Value::as_u64) {
        Some(value) => Ok(value),
        None if object.contains_key(key) => {
            bail!("benchmark gate report key '{key}' must be an unsigned integer")
        }
        None => bail!("benchmark gate report missing required key: {key}"),
    }
}

fn gate_report_required_f64(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<f64> {
    match object.get(key).and_then(Value::as_f64) {
        Some(value) if value.is_finite() => Ok(value),
        Some(_) => bail!("benchmark gate report key '{key}' must be finite"),
        None if object.contains_key(key) => {
            bail!("benchmark gate report key '{key}' must be a number")
        }
        None => bail!("benchmark gate report missing required key: {key}"),
    }
}

fn gate_report_required_string_vec(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<Vec<String>> {
    let Some(value) = object.get(key) else {
        bail!("benchmark gate report missing required key: {key}");
    };

    let Value::Array(items) = value else {
        bail!("benchmark gate report key '{key}' must be an array");
    };

    let mut output = Vec::with_capacity(items.len());
    for item in items {
        let Value::String(code) = item else {
            bail!("benchmark gate report key '{key}' must contain only strings");
        };
        output.push(code.clone());
    }
    Ok(output)
}

fn deterministic_m24_rl_gate_evidence_bundle_file_name(bundle: &M24RLGateEvidenceBundle) -> String {
    let benchmark_pass = if bundle.benchmark.pass { 1 } else { 0 };
    let safety_pass = if bundle.safety.pass { 1 } else { 0 };
    let operations_pass = if bundle.operations.pass { 1 } else { 0 };
    format!(
        "m24-rl-gate-evidence-bundle-v{}-benchmark-pass-{}-safety-pass-{}-operations-pass-{}-{}.json",
        bundle.schema_version,
        benchmark_pass,
        safety_pass,
        operations_pass,
        bundle.generated_at_epoch_ms
    )
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

fn deterministic_gate_report_file_name(report: &BenchmarkArtifactGateReport) -> String {
    format!(
        "benchmark-artifact-gate-report-v{}-valid-{}-invalid-{}.json",
        report.manifest.schema_version,
        report.manifest.valid_entries.len(),
        report.manifest.invalid_files.len()
    )
}

fn deterministic_summary_gate_report_file_name(
    report: &BenchmarkArtifactGateSummaryReport,
) -> String {
    format!(
        "benchmark-artifact-gate-summary-report-v{}-pass-{}-fail-{}-invalid-{}.json",
        report.summary.schema_version,
        report.summary.pass_entries,
        report.summary.fail_entries,
        report.summary.invalid_files.len()
    )
}

fn deterministic_summary_manifest_report_file_name(
    report: &BenchmarkArtifactGateSummaryReportManifestReport,
) -> String {
    format!(
        "benchmark-artifact-gate-summary-manifest-report-v{}-pass-{}-fail-{}-invalid-{}.json",
        report.manifest.schema_version,
        report.manifest.pass_reports,
        report.manifest.fail_reports,
        report.manifest.invalid_files.len()
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
mod tests;
