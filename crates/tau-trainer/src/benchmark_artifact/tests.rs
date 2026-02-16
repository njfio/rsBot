use super::{
    build_benchmark_artifact_gate_report, build_benchmark_artifact_gate_report_summary_manifest,
    build_benchmark_artifact_gate_summary_report,
    build_benchmark_artifact_gate_summary_report_manifest,
    build_benchmark_artifact_gate_summary_report_manifest_report,
    build_benchmark_artifact_manifest, build_benchmark_evaluation_artifact,
    build_m24_rl_gate_evidence_bundle, evaluate_benchmark_gate_report_summary_quality,
    evaluate_benchmark_gate_summary_report_manifest_quality, evaluate_benchmark_manifest_quality,
    evaluate_m24_rl_gate_exit, export_benchmark_artifact_gate_report,
    export_benchmark_artifact_gate_summary_report,
    export_benchmark_artifact_gate_summary_report_manifest_report,
    export_benchmark_evaluation_artifact, export_m24_rl_gate_evidence_bundle,
    validate_exported_benchmark_artifact, validate_exported_benchmark_artifact_gate_report,
    validate_exported_benchmark_artifact_gate_summary_report,
    validate_exported_benchmark_artifact_gate_summary_report_manifest_report,
    validate_exported_m24_rl_gate_evidence_bundle, BenchmarkArtifactGateReportSummaryEntry,
    BenchmarkArtifactGateReportSummaryInvalidFile, BenchmarkArtifactGateReportSummaryManifest,
    BenchmarkArtifactGateReportSummaryQualityPolicy, BenchmarkArtifactGateSummaryReportManifest,
    BenchmarkArtifactGateSummaryReportManifestEntry,
    BenchmarkArtifactGateSummaryReportManifestInvalidFile,
    BenchmarkArtifactGateSummaryReportManifestQualityPolicy, BenchmarkArtifactManifestQualityInput,
    BenchmarkArtifactManifestQualityPolicy, BenchmarkEvaluationArtifactInput,
    M24RLGateEvidenceBenchmarkInput, M24RLGateEvidenceBundleInput,
    M24RLGateEvidenceOperationsInput, M24RLGateEvidenceRunbooksInput, M24RLGateEvidenceSafetyInput,
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
    let path = std::env::temp_dir().join(format!("tau-{prefix}-{}-{nanos}", std::process::id()));
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

    let validated =
        validate_exported_benchmark_artifact(&summary.path).expect("validate exported artifact");
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

    let error =
        validate_exported_benchmark_artifact(&artifact_path).expect_err("missing keys should fail");
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

#[test]
fn spec_1974_c01_quality_gate_passes_when_manifest_meets_thresholds() {
    let decision = evaluate_benchmark_manifest_quality(
        &BenchmarkArtifactManifestQualityInput {
            valid_entries: 3,
            invalid_entries: 0,
        },
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.25,
        },
    )
    .expect("decision");

    assert!(decision.pass);
    assert!(decision.reason_codes.is_empty());
    assert_eq!(decision.valid_entries, 3);
    assert_eq!(decision.invalid_entries, 0);
}

#[test]
fn spec_1974_c02_quality_gate_fails_with_no_valid_artifacts_reason() {
    let decision = evaluate_benchmark_manifest_quality(
        &BenchmarkArtifactManifestQualityInput {
            valid_entries: 0,
            invalid_entries: 2,
        },
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.80,
        },
    )
    .expect("decision");

    assert!(!decision.pass);
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "no_valid_artifacts"));
}

#[test]
fn spec_1974_c03_quality_gate_fails_when_invalid_ratio_exceeds_policy() {
    let decision = evaluate_benchmark_manifest_quality(
        &BenchmarkArtifactManifestQualityInput {
            valid_entries: 1,
            invalid_entries: 2,
        },
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.5,
        },
    )
    .expect("decision");

    assert!(!decision.pass);
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "invalid_ratio_exceeded"));
}

#[test]
fn spec_1974_c04_quality_gate_decision_json_is_machine_readable() {
    let decision = evaluate_benchmark_manifest_quality(
        &BenchmarkArtifactManifestQualityInput {
            valid_entries: 2,
            invalid_entries: 1,
        },
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.6,
        },
    )
    .expect("decision");

    let payload = decision.to_json_value();
    assert!(payload["pass"].is_boolean());
    assert!(payload["valid_entries"].as_u64().is_some());
    assert!(payload["invalid_entries"].as_u64().is_some());
    assert!(payload["invalid_ratio"].is_number());
    assert!(payload["reason_codes"].is_array());
}

#[test]
fn regression_quality_gate_handles_zero_scanned_without_division_errors() {
    let decision = evaluate_benchmark_manifest_quality(
        &BenchmarkArtifactManifestQualityInput {
            valid_entries: 0,
            invalid_entries: 0,
        },
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.1,
        },
    )
    .expect("decision");

    assert!(decision.invalid_ratio.is_finite());
    assert!(!decision.pass);
}

#[test]
fn spec_1976_c01_gate_report_contains_manifest_counters_and_quality_decision() {
    let output_dir = temp_output_dir("benchmark-gate-report-c01");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");

    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.5,
        },
    )
    .expect("build gate report");

    assert_eq!(report.manifest.scanned_json_files, 1);
    assert_eq!(report.manifest.valid_entries.len(), 1);
    assert!(report.quality.pass);

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1976_c02_gate_report_preserves_quality_reason_codes() {
    let output_dir = temp_output_dir("benchmark-gate-report-c02");
    fs::create_dir_all(&output_dir).expect("create output dir");
    fs::write(output_dir.join("broken.json"), "{ malformed").expect("write malformed artifact");

    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.5,
        },
    )
    .expect("build gate report");

    assert!(!report.quality.pass);
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "no_valid_artifacts"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1976_c03_gate_report_json_is_machine_readable_with_nested_sections() {
    let output_dir = temp_output_dir("benchmark-gate-report-c03");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");

    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build gate report");
    let payload = report.to_json_value();
    assert!(payload["manifest"].is_object());
    assert!(payload["quality"].is_object());
    assert!(payload["manifest"]["scanned_json_files"].as_u64().is_some());
    assert!(payload["quality"]["pass"].is_boolean());

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1976_c04_gate_report_rejects_invalid_quality_policy() {
    let output_dir = temp_output_dir("benchmark-gate-report-c04");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");
    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");

    let error = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 1.5,
        },
    )
    .expect_err("invalid policy should fail");
    assert!(error.to_string().contains("max_invalid_ratio"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn regression_gate_report_handles_zero_scan_manifest() {
    let output_dir = temp_output_dir("benchmark-gate-report-regression");
    fs::create_dir_all(&output_dir).expect("create output dir");
    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");

    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy {
            min_valid_entries: 1,
            max_invalid_ratio: 0.5,
        },
    )
    .expect("build gate report");
    assert_eq!(report.quality.invalid_ratio, 0.0);
    assert!(!report.quality.pass);

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1978_c01_gate_report_export_writes_deterministic_file_and_summary() {
    let output_dir = temp_output_dir("benchmark-gate-report-export-c01");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");
    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build gate report");

    let summary =
        export_benchmark_artifact_gate_report(&report, &output_dir).expect("export report");
    assert!(summary.path.exists());
    assert!(summary
        .path
        .ends_with("benchmark-artifact-gate-report-v1-valid-1-invalid-0.json"));
    assert!(summary.bytes_written > 0);

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1978_c02_gate_report_validator_accepts_exported_payload() {
    let output_dir = temp_output_dir("benchmark-gate-report-export-c02");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");
    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build gate report");
    let summary =
        export_benchmark_artifact_gate_report(&report, &output_dir).expect("export report");

    let value = validate_exported_benchmark_artifact_gate_report(&summary.path).expect("validate");
    assert!(value["manifest"].is_object());
    assert!(value["quality"].is_object());

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1978_c03_gate_report_validator_rejects_malformed_or_non_object_payloads() {
    let output_dir = temp_output_dir("benchmark-gate-report-export-c03");
    fs::create_dir_all(&output_dir).expect("create output dir");

    let malformed_path = output_dir.join("malformed.json");
    fs::write(&malformed_path, "{ malformed").expect("write malformed");
    let malformed_error = validate_exported_benchmark_artifact_gate_report(&malformed_path)
        .expect_err("malformed JSON must fail");
    assert!(malformed_error.to_string().contains("failed to parse"));

    let non_object_path = output_dir.join("non-object.json");
    fs::write(&non_object_path, "[]").expect("write array");
    let non_object_error = validate_exported_benchmark_artifact_gate_report(&non_object_path)
        .expect_err("non-object JSON must fail");
    assert!(non_object_error
        .to_string()
        .contains("top-level JSON object"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1978_c04_gate_report_export_rejects_file_destination() {
    let output_dir = temp_output_dir("benchmark-gate-report-export-c04");
    fs::create_dir_all(&output_dir).expect("create output dir");

    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &output_dir).expect("export artifact");
    let manifest = build_benchmark_artifact_manifest(&output_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build gate report");

    let not_a_directory = output_dir.join("not-a-directory");
    fs::write(&not_a_directory, "not-a-directory").expect("write blocking file");

    let error = export_benchmark_artifact_gate_report(&report, &not_a_directory)
        .expect_err("file destination should fail");
    assert!(error.to_string().contains("not a directory"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn regression_gate_report_validator_rejects_missing_sections() {
    let output_dir = temp_output_dir("benchmark-gate-report-export-regression");
    fs::create_dir_all(&output_dir).expect("create output dir");
    let payload_path = output_dir.join("missing-sections.json");
    fs::write(&payload_path, "{ \"manifest\": {} }").expect("write payload");

    let error = validate_exported_benchmark_artifact_gate_report(&payload_path)
        .expect_err("missing required sections should fail");
    assert!(error.to_string().contains("missing required key"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1980_c01_gate_report_summary_manifest_is_sorted_with_pass_fail_totals() {
    let summary_dir = temp_output_dir("benchmark-gate-report-summary-c01");
    fs::create_dir_all(&summary_dir).expect("create summary dir");

    let pass_artifact_dir = temp_output_dir("benchmark-gate-report-summary-c01-pass");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &pass_artifact_dir).expect("export");
    let pass_manifest =
        build_benchmark_artifact_manifest(&pass_artifact_dir).expect("build pass manifest");
    let pass_report = build_benchmark_artifact_gate_report(
        &pass_manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build pass report");
    export_benchmark_artifact_gate_report(&pass_report, &summary_dir).expect("export pass");

    let fail_artifact_dir = temp_output_dir("benchmark-gate-report-summary-c01-fail");
    fs::create_dir_all(&fail_artifact_dir).expect("create fail artifact dir");
    fs::write(fail_artifact_dir.join("broken.json"), "{ malformed")
        .expect("write malformed artifact");
    let fail_manifest =
        build_benchmark_artifact_manifest(&fail_artifact_dir).expect("build fail manifest");
    let fail_report = build_benchmark_artifact_gate_report(
        &fail_manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build fail report");
    export_benchmark_artifact_gate_report(&fail_report, &summary_dir).expect("export fail");

    let summary =
        build_benchmark_artifact_gate_report_summary_manifest(&summary_dir).expect("summary");
    assert_eq!(summary.entries.len(), 2);
    assert_eq!(summary.pass_entries, 1);
    assert_eq!(summary.fail_entries, 1);
    assert!(summary.entries[0].path <= summary.entries[1].path);

    fs::remove_dir_all(summary_dir).expect("cleanup");
    fs::remove_dir_all(pass_artifact_dir).expect("cleanup");
    fs::remove_dir_all(fail_artifact_dir).expect("cleanup");
}

#[test]
fn spec_1980_c02_gate_report_summary_manifest_records_invalid_files_without_abort() {
    let summary_dir = temp_output_dir("benchmark-gate-report-summary-c02");
    fs::create_dir_all(&summary_dir).expect("create summary dir");

    let artifact_dir = temp_output_dir("benchmark-gate-report-summary-c02-source");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &artifact_dir).expect("export");
    let manifest = build_benchmark_artifact_manifest(&artifact_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build report");
    export_benchmark_artifact_gate_report(&report, &summary_dir).expect("export report");
    fs::write(summary_dir.join("broken.json"), "{ malformed").expect("write malformed file");

    let summary =
        build_benchmark_artifact_gate_report_summary_manifest(&summary_dir).expect("summary");
    assert_eq!(summary.entries.len(), 1);
    assert_eq!(summary.invalid_files.len(), 1);
    assert!(summary.invalid_files[0]
        .reason
        .contains("failed to parse benchmark gate report"));

    fs::remove_dir_all(summary_dir).expect("cleanup");
    fs::remove_dir_all(artifact_dir).expect("cleanup");
}

#[test]
fn spec_1980_c03_gate_report_summary_manifest_json_is_machine_readable() {
    let summary_dir = temp_output_dir("benchmark-gate-report-summary-c03");
    fs::create_dir_all(&summary_dir).expect("create summary dir");

    let artifact_dir = temp_output_dir("benchmark-gate-report-summary-c03-source");
    let artifact = sample_artifact();
    export_benchmark_evaluation_artifact(&artifact, &artifact_dir).expect("export");
    let manifest = build_benchmark_artifact_manifest(&artifact_dir).expect("build manifest");
    let report = build_benchmark_artifact_gate_report(
        &manifest,
        &BenchmarkArtifactManifestQualityPolicy::default(),
    )
    .expect("build report");
    export_benchmark_artifact_gate_report(&report, &summary_dir).expect("export report");

    let summary =
        build_benchmark_artifact_gate_report_summary_manifest(&summary_dir).expect("summary");
    let payload = summary.to_json_value();
    assert!(payload["entries"].is_array());
    assert!(payload["invalid_files"].is_array());
    assert!(payload["pass_entries"].as_u64().is_some());
    assert!(payload["fail_entries"].as_u64().is_some());

    fs::remove_dir_all(summary_dir).expect("cleanup");
    fs::remove_dir_all(artifact_dir).expect("cleanup");
}

#[test]
fn spec_1980_c04_gate_report_summary_manifest_rejects_missing_directory() {
    let output_dir = temp_output_dir("benchmark-gate-report-summary-c04");
    let missing = output_dir.join("missing");
    let error = build_benchmark_artifact_gate_report_summary_manifest(&missing)
        .expect_err("missing directory should fail");
    assert!(error.to_string().contains("missing directory"));
}

#[test]
fn regression_gate_report_summary_manifest_ignores_non_json_files() {
    let summary_dir = temp_output_dir("benchmark-gate-report-summary-regression");
    fs::create_dir_all(&summary_dir).expect("create summary dir");
    fs::write(summary_dir.join("README.txt"), "ignore me").expect("write text file");

    let summary =
        build_benchmark_artifact_gate_report_summary_manifest(&summary_dir).expect("summary");
    assert_eq!(summary.entries.len(), 0);
    assert_eq!(summary.invalid_files.len(), 0);
    assert_eq!(summary.pass_entries, 0);
    assert_eq!(summary.fail_entries, 0);

    fs::remove_dir_all(summary_dir).expect("cleanup");
}

#[test]
fn spec_1982_c01_summary_quality_gate_passes_when_thresholds_met() {
    let summary = sample_summary_manifest(2, 0, 0);
    let decision = evaluate_benchmark_gate_report_summary_quality(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.2,
        },
    )
    .expect("quality decision");
    assert!(decision.pass);
    assert_eq!(decision.pass_entries, 2);
    assert_eq!(decision.fail_entries, 0);
    assert_eq!(decision.invalid_files, 0);
    assert!(decision.reason_codes.is_empty());
}

#[test]
fn spec_1982_c02_summary_quality_gate_emits_reason_codes_for_failures() {
    let summary = sample_summary_manifest(0, 2, 1);
    let decision = evaluate_benchmark_gate_report_summary_quality(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.20,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect("quality decision");
    assert!(!decision.pass);
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "below_min_pass_entries"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "fail_ratio_exceeded"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "invalid_file_ratio_exceeded"));
}

#[test]
fn spec_1982_c03_summary_quality_gate_json_is_machine_readable() {
    let summary = sample_summary_manifest(1, 0, 0);
    let decision = evaluate_benchmark_gate_report_summary_quality(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("quality decision");
    let payload = decision.to_json_value();
    assert!(payload["pass"].is_boolean());
    assert!(payload["pass_entries"].as_u64().is_some());
    assert!(payload["fail_entries"].as_u64().is_some());
    assert!(payload["invalid_file_ratio"].is_number());
    assert!(payload["reason_codes"].is_array());
}

#[test]
fn spec_1982_c04_summary_quality_gate_rejects_invalid_policy_ratios() {
    let summary = sample_summary_manifest(1, 0, 0);
    let error = evaluate_benchmark_gate_report_summary_quality(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 1.5,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect_err("invalid policy should fail");
    assert!(error.to_string().contains("max_fail_ratio"));
}

#[test]
fn regression_summary_quality_gate_handles_zero_total_entries() {
    let summary = sample_summary_manifest(0, 0, 0);
    let decision = evaluate_benchmark_gate_report_summary_quality(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.5,
        },
    )
    .expect("quality decision");
    assert!(decision.fail_ratio.is_finite());
    assert!(decision.invalid_file_ratio.is_finite());
    assert!(!decision.pass);
}

#[test]
fn spec_1984_c01_summary_gate_report_contains_counters_and_quality_decision() {
    let summary = sample_summary_manifest(2, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.5,
        },
    )
    .expect("build summary gate report");

    assert_eq!(report.summary.pass_entries, 2);
    assert_eq!(report.summary.fail_entries, 0);
    assert_eq!(report.summary.invalid_files.len(), 0);
    assert!(report.quality.pass);
}

#[test]
fn spec_1984_c02_summary_gate_report_preserves_quality_reason_codes() {
    let summary = sample_summary_manifest(0, 2, 1);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.10,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect("build summary gate report");

    assert!(!report.quality.pass);
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "below_min_pass_entries"));
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "fail_ratio_exceeded"));
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "invalid_file_ratio_exceeded"));
}

#[test]
fn spec_1984_c03_summary_gate_report_json_is_machine_readable_with_nested_sections() {
    let summary = sample_summary_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build summary gate report");
    let payload = report.to_json_value();
    assert!(payload["summary"].is_object());
    assert!(payload["quality"].is_object());
    assert!(payload["summary"]["pass_entries"].as_u64().is_some());
    assert!(payload["quality"]["pass"].is_boolean());
}

#[test]
fn spec_1984_c04_summary_gate_report_rejects_invalid_quality_policy() {
    let summary = sample_summary_manifest(1, 0, 0);
    let error = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 1.5,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect_err("invalid policy should fail");
    assert!(error.to_string().contains("max_fail_ratio"));
}

#[test]
fn regression_summary_gate_report_handles_zero_summary_entries() {
    let summary = sample_summary_manifest(0, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.5,
        },
    )
    .expect("build summary gate report");
    assert!(report.quality.fail_ratio.is_finite());
    assert!(report.quality.invalid_file_ratio.is_finite());
    assert!(!report.quality.pass);
}

#[test]
fn spec_1986_c01_summary_gate_report_export_writes_deterministic_file_and_summary() {
    let output_dir = temp_output_dir("benchmark-summary-gate-export-c01");
    let summary = sample_summary_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build summary gate report");

    let export =
        export_benchmark_artifact_gate_summary_report(&report, &output_dir).expect("export");
    assert!(export.path.exists());
    assert!(export
        .path
        .ends_with("benchmark-artifact-gate-summary-report-v1-pass-1-fail-0-invalid-0.json"));
    assert!(export.bytes_written > 0);

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1986_c02_summary_gate_report_validator_accepts_exported_payload() {
    let output_dir = temp_output_dir("benchmark-summary-gate-export-c02");
    let summary = sample_summary_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build summary gate report");
    let export =
        export_benchmark_artifact_gate_summary_report(&report, &output_dir).expect("export");

    let value =
        validate_exported_benchmark_artifact_gate_summary_report(&export.path).expect("parse");
    assert!(value["summary"].is_object());
    assert!(value["quality"].is_object());

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1986_c03_summary_gate_report_validator_rejects_malformed_or_non_object_payloads() {
    let output_dir = temp_output_dir("benchmark-summary-gate-export-c03");
    fs::create_dir_all(&output_dir).expect("create output dir");

    let malformed_path = output_dir.join("malformed.json");
    fs::write(&malformed_path, "{ malformed").expect("write malformed");
    let malformed_error = validate_exported_benchmark_artifact_gate_summary_report(&malformed_path)
        .expect_err("malformed JSON must fail");
    assert!(malformed_error.to_string().contains("failed to parse"));

    let non_object_path = output_dir.join("non-object.json");
    fs::write(&non_object_path, "[]").expect("write array");
    let non_object_error =
        validate_exported_benchmark_artifact_gate_summary_report(&non_object_path)
            .expect_err("non-object JSON must fail");
    assert!(non_object_error
        .to_string()
        .contains("top-level JSON object"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1986_c04_summary_gate_report_export_rejects_file_destination() {
    let output_dir = temp_output_dir("benchmark-summary-gate-export-c04");
    fs::create_dir_all(&output_dir).expect("create output dir");
    let summary = sample_summary_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build summary gate report");

    let file_destination = output_dir.join("not-a-dir");
    fs::write(&file_destination, "blocking file").expect("write file");
    let error = export_benchmark_artifact_gate_summary_report(&report, &file_destination)
        .expect_err("file destination should fail");
    assert!(error.to_string().contains("not a directory"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn regression_summary_gate_report_validator_rejects_missing_sections() {
    let output_dir = temp_output_dir("benchmark-summary-gate-export-regression");
    fs::create_dir_all(&output_dir).expect("create output dir");
    let payload_path = output_dir.join("missing-sections.json");
    fs::write(&payload_path, "{ \"summary\": {} }").expect("write payload");

    let error = validate_exported_benchmark_artifact_gate_summary_report(&payload_path)
        .expect_err("missing sections should fail");
    assert!(error.to_string().contains("missing required key"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1988_c01_summary_gate_report_manifest_is_sorted_with_pass_fail_totals() {
    let manifest_dir = temp_output_dir("benchmark-summary-gate-manifest-c01");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");

    let pass_summary = sample_summary_manifest(2, 0, 0);
    let pass_report = build_benchmark_artifact_gate_summary_report(
        &pass_summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build pass summary report");
    export_benchmark_artifact_gate_summary_report(&pass_report, &manifest_dir)
        .expect("export pass report");

    let fail_summary = sample_summary_manifest(0, 2, 1);
    let fail_report = build_benchmark_artifact_gate_summary_report(
        &fail_summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy {
            min_pass_entries: 1,
            max_fail_ratio: 0.0,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect("build fail summary report");
    export_benchmark_artifact_gate_summary_report(&fail_report, &manifest_dir)
        .expect("export fail report");

    let manifest =
        build_benchmark_artifact_gate_summary_report_manifest(&manifest_dir).expect("manifest");
    assert_eq!(manifest.entries.len(), 2);
    assert_eq!(manifest.pass_reports, 1);
    assert_eq!(manifest.fail_reports, 1);
    assert!(manifest.entries[0].path <= manifest.entries[1].path);

    fs::remove_dir_all(manifest_dir).expect("cleanup");
}

#[test]
fn spec_1988_c02_summary_gate_report_manifest_records_invalid_files_without_abort() {
    let manifest_dir = temp_output_dir("benchmark-summary-gate-manifest-c02");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");

    let summary = sample_summary_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build report");
    export_benchmark_artifact_gate_summary_report(&report, &manifest_dir).expect("export");
    fs::write(manifest_dir.join("broken.json"), "{ malformed").expect("write malformed file");

    let manifest =
        build_benchmark_artifact_gate_summary_report_manifest(&manifest_dir).expect("manifest");
    assert_eq!(manifest.entries.len(), 1);
    assert_eq!(manifest.invalid_files.len(), 1);
    assert!(manifest.invalid_files[0]
        .reason
        .contains("failed to parse benchmark summary gate report"));

    fs::remove_dir_all(manifest_dir).expect("cleanup");
}

#[test]
fn spec_1988_c03_summary_gate_report_manifest_json_is_machine_readable() {
    let manifest_dir = temp_output_dir("benchmark-summary-gate-manifest-c03");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");

    let summary = sample_summary_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report(
        &summary,
        &BenchmarkArtifactGateReportSummaryQualityPolicy::default(),
    )
    .expect("build report");
    export_benchmark_artifact_gate_summary_report(&report, &manifest_dir).expect("export");

    let manifest =
        build_benchmark_artifact_gate_summary_report_manifest(&manifest_dir).expect("manifest");
    let payload = manifest.to_json_value();
    assert!(payload["entries"].is_array());
    assert!(payload["invalid_files"].is_array());
    assert!(payload["pass_reports"].as_u64().is_some());
    assert!(payload["fail_reports"].as_u64().is_some());

    fs::remove_dir_all(manifest_dir).expect("cleanup");
}

#[test]
fn spec_1988_c04_summary_gate_report_manifest_rejects_missing_directory() {
    let output_dir = temp_output_dir("benchmark-summary-gate-manifest-c04");
    let missing = output_dir.join("missing");
    let error = build_benchmark_artifact_gate_summary_report_manifest(&missing)
        .expect_err("missing directory should fail");
    assert!(error.to_string().contains("missing directory"));
}

#[test]
fn regression_summary_gate_report_manifest_ignores_non_json_files() {
    let manifest_dir = temp_output_dir("benchmark-summary-gate-manifest-regression");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    fs::write(manifest_dir.join("README.txt"), "ignore me").expect("write text file");

    let manifest =
        build_benchmark_artifact_gate_summary_report_manifest(&manifest_dir).expect("manifest");
    assert_eq!(manifest.entries.len(), 0);
    assert_eq!(manifest.invalid_files.len(), 0);
    assert_eq!(manifest.pass_reports, 0);
    assert_eq!(manifest.fail_reports, 0);

    fs::remove_dir_all(manifest_dir).expect("cleanup");
}

#[test]
fn spec_1990_c01_manifest_quality_gate_passes_when_thresholds_met() {
    let manifest = sample_summary_gate_report_manifest(2, 0, 0);
    let decision = evaluate_benchmark_gate_summary_report_manifest_quality(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.2,
        },
    )
    .expect("quality decision");
    assert!(decision.pass);
    assert_eq!(decision.pass_reports, 2);
    assert_eq!(decision.fail_reports, 0);
    assert_eq!(decision.invalid_files, 0);
    assert!(decision.reason_codes.is_empty());
}

#[test]
fn spec_1990_c02_manifest_quality_gate_emits_reason_codes_for_failures() {
    let manifest = sample_summary_gate_report_manifest(0, 2, 1);
    let decision = evaluate_benchmark_gate_summary_report_manifest_quality(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 0.2,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect("quality decision");
    assert!(!decision.pass);
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "below_min_pass_reports"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "fail_ratio_exceeded"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "invalid_file_ratio_exceeded"));
}

#[test]
fn spec_1990_c03_manifest_quality_gate_json_is_machine_readable() {
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let decision = evaluate_benchmark_gate_summary_report_manifest_quality(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy::default(),
    )
    .expect("quality decision");
    let payload = decision.to_json_value();
    assert!(payload["pass"].is_boolean());
    assert!(payload["pass_reports"].as_u64().is_some());
    assert!(payload["fail_reports"].as_u64().is_some());
    assert!(payload["invalid_file_ratio"].is_number());
    assert!(payload["reason_codes"].is_array());
}

#[test]
fn spec_1990_c04_manifest_quality_gate_rejects_invalid_policy_ratios() {
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let error = evaluate_benchmark_gate_summary_report_manifest_quality(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 1.5,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect_err("invalid policy should fail");
    assert!(error.to_string().contains("max_fail_ratio"));
}

#[test]
fn regression_manifest_quality_gate_handles_zero_total_reports() {
    let manifest = sample_summary_gate_report_manifest(0, 0, 0);
    let decision = evaluate_benchmark_gate_summary_report_manifest_quality(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.5,
        },
    )
    .expect("quality decision");
    assert!(decision.fail_ratio.is_finite());
    assert!(decision.invalid_file_ratio.is_finite());
    assert!(!decision.pass);
}

#[test]
fn spec_1992_c01_manifest_report_contains_counters_and_quality_decision() {
    let manifest = sample_summary_gate_report_manifest(2, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.5,
        },
    )
    .expect("build manifest report");

    assert_eq!(report.manifest.pass_reports, 2);
    assert_eq!(report.manifest.fail_reports, 0);
    assert_eq!(report.manifest.invalid_files.len(), 0);
    assert!(report.quality.pass);
}

#[test]
fn spec_1992_c02_manifest_report_preserves_quality_reason_codes() {
    let manifest = sample_summary_gate_report_manifest(0, 2, 1);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 0.1,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect("build manifest report");

    assert!(!report.quality.pass);
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "below_min_pass_reports"));
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "fail_ratio_exceeded"));
    assert!(report
        .quality
        .reason_codes
        .iter()
        .any(|code| code == "invalid_file_ratio_exceeded"));
}

#[test]
fn spec_1992_c03_manifest_report_json_is_machine_readable_with_nested_sections() {
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy::default(),
    )
    .expect("build manifest report");
    let payload = report.to_json_value();
    assert!(payload["manifest"].is_object());
    assert!(payload["quality"].is_object());
    assert!(payload["manifest"]["pass_reports"].as_u64().is_some());
    assert!(payload["quality"]["pass"].is_boolean());
}

#[test]
fn spec_1992_c04_manifest_report_rejects_invalid_quality_policy() {
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let error = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 1.5,
            max_invalid_file_ratio: 0.0,
        },
    )
    .expect_err("invalid policy should fail");
    assert!(error.to_string().contains("max_fail_ratio"));
}

#[test]
fn regression_manifest_report_handles_zero_manifest_entries() {
    let manifest = sample_summary_gate_report_manifest(0, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy {
            min_pass_reports: 1,
            max_fail_ratio: 0.5,
            max_invalid_file_ratio: 0.5,
        },
    )
    .expect("build manifest report");
    assert!(report.quality.fail_ratio.is_finite());
    assert!(report.quality.invalid_file_ratio.is_finite());
    assert!(!report.quality.pass);
}

#[test]
fn spec_1994_c01_manifest_report_export_writes_deterministic_file_and_summary() {
    let output_dir = temp_output_dir("benchmark-summary-manifest-report-export-c01");
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy::default(),
    )
    .expect("build report");

    let export =
        export_benchmark_artifact_gate_summary_report_manifest_report(&report, &output_dir)
            .expect("export report");
    assert!(export.path.exists());
    assert!(export.path.ends_with(
        "benchmark-artifact-gate-summary-manifest-report-v1-pass-1-fail-0-invalid-0.json"
    ));
    assert!(export.bytes_written > 0);

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1994_c02_manifest_report_validator_accepts_exported_payload() {
    let output_dir = temp_output_dir("benchmark-summary-manifest-report-export-c02");
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy::default(),
    )
    .expect("build report");
    let export =
        export_benchmark_artifact_gate_summary_report_manifest_report(&report, &output_dir)
            .expect("export report");

    let value =
        validate_exported_benchmark_artifact_gate_summary_report_manifest_report(&export.path)
            .expect("validate report");
    assert!(value["manifest"].is_object());
    assert!(value["quality"].is_object());

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1994_c03_manifest_report_validator_rejects_malformed_or_non_object_payloads() {
    let output_dir = temp_output_dir("benchmark-summary-manifest-report-export-c03");
    fs::create_dir_all(&output_dir).expect("create output dir");

    let malformed_path = output_dir.join("malformed.json");
    fs::write(&malformed_path, "{ malformed").expect("write malformed file");
    let malformed_error =
        validate_exported_benchmark_artifact_gate_summary_report_manifest_report(&malformed_path)
            .expect_err("malformed JSON must fail");
    assert!(malformed_error.to_string().contains("failed to parse"));

    let non_object_path = output_dir.join("non-object.json");
    fs::write(&non_object_path, "[]").expect("write array file");
    let non_object_error =
        validate_exported_benchmark_artifact_gate_summary_report_manifest_report(&non_object_path)
            .expect_err("non-object JSON must fail");
    assert!(non_object_error
        .to_string()
        .contains("top-level JSON object"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1994_c04_manifest_report_export_rejects_file_destination() {
    let output_dir = temp_output_dir("benchmark-summary-manifest-report-export-c04");
    fs::create_dir_all(&output_dir).expect("create output dir");
    let manifest = sample_summary_gate_report_manifest(1, 0, 0);
    let report = build_benchmark_artifact_gate_summary_report_manifest_report(
        &manifest,
        &BenchmarkArtifactGateSummaryReportManifestQualityPolicy::default(),
    )
    .expect("build report");

    let file_destination = output_dir.join("not-a-dir");
    fs::write(&file_destination, "blocking file").expect("write blocking file");
    let error =
        export_benchmark_artifact_gate_summary_report_manifest_report(&report, &file_destination)
            .expect_err("file destination should fail");
    assert!(error.to_string().contains("not a directory"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn regression_manifest_report_validator_rejects_missing_sections() {
    let output_dir = temp_output_dir("benchmark-summary-manifest-report-export-regression");
    fs::create_dir_all(&output_dir).expect("create output dir");
    let payload_path = output_dir.join("missing-sections.json");
    fs::write(&payload_path, "{ \"manifest\": {} }").expect("write payload");

    let error =
        validate_exported_benchmark_artifact_gate_summary_report_manifest_report(&payload_path)
            .expect_err("missing sections should fail");
    assert!(error.to_string().contains("missing required key"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1996_c01_bundle_builder_preserves_deterministic_sections_and_pass_signals() {
    let input = sample_m24_bundle_input();
    let bundle = build_m24_rl_gate_evidence_bundle(input).expect("build bundle");

    assert_eq!(bundle.schema_version, 1);
    assert!(bundle.benchmark.pass);
    assert!(bundle.safety.pass);
    assert!(bundle.operations.pass);
    assert_eq!(
        bundle.runbooks.operator_runbook_ref,
        "docs/guides/training-ops.md"
    );

    let payload = bundle.to_json_value();
    assert!(payload["benchmark"].is_object());
    assert!(payload["safety"].is_object());
    assert!(payload["operations"].is_object());
    assert!(payload["runbooks"].is_object());
}

#[test]
fn spec_1996_c02_bundle_export_writes_deterministic_file_and_summary() {
    let output_dir = temp_output_dir("m24-rl-gate-evidence-export-c02");
    let bundle =
        build_m24_rl_gate_evidence_bundle(sample_m24_bundle_input()).expect("build bundle");
    let export = export_m24_rl_gate_evidence_bundle(&bundle, &output_dir).expect("export");

    assert!(export.path.exists());
    assert!(export.path.ends_with(
            "m24-rl-gate-evidence-bundle-v1-benchmark-pass-1-safety-pass-1-operations-pass-1-1706000010000.json"
        ));
    assert!(export.bytes_written > 0);

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1996_c03_bundle_validator_accepts_exported_payload() {
    let output_dir = temp_output_dir("m24-rl-gate-evidence-export-c03");
    let bundle =
        build_m24_rl_gate_evidence_bundle(sample_m24_bundle_input()).expect("build bundle");
    let export = export_m24_rl_gate_evidence_bundle(&bundle, &output_dir).expect("export");
    let value = validate_exported_m24_rl_gate_evidence_bundle(&export.path)
        .expect("validator should accept export");

    assert!(value["benchmark"].is_object());
    assert!(value["safety"].is_object());
    assert!(value["operations"].is_object());
    assert!(value["runbooks"].is_object());

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1996_c04_bundle_validator_rejects_malformed_or_non_object_payloads() {
    let output_dir = temp_output_dir("m24-rl-gate-evidence-export-c04");
    fs::create_dir_all(&output_dir).expect("create output dir");

    let malformed_path = output_dir.join("malformed.json");
    fs::write(&malformed_path, "{ malformed").expect("write malformed payload");
    let malformed_error = validate_exported_m24_rl_gate_evidence_bundle(&malformed_path)
        .expect_err("malformed payload should fail");
    assert!(malformed_error.to_string().contains("failed to parse"));

    let non_object_path = output_dir.join("non-object.json");
    fs::write(&non_object_path, "[]").expect("write non-object payload");
    let non_object_error = validate_exported_m24_rl_gate_evidence_bundle(&non_object_path)
        .expect_err("non-object payload should fail");
    assert!(non_object_error
        .to_string()
        .contains("top-level JSON object"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn regression_m24_bundle_validator_rejects_missing_sections() {
    let output_dir = temp_output_dir("m24-rl-gate-evidence-export-regression");
    fs::create_dir_all(&output_dir).expect("create output dir");

    let payload_path = output_dir.join("missing-sections.json");
    fs::write(
        &payload_path,
        "{ \"benchmark\": {}, \"safety\": {}, \"operations\": {} }",
    )
    .expect("write payload");
    let error = validate_exported_m24_rl_gate_evidence_bundle(&payload_path)
        .expect_err("missing runbooks section should fail");
    assert!(error.to_string().contains("missing required key"));

    fs::remove_dir_all(output_dir).expect("cleanup");
}

#[test]
fn spec_1998_c01_exit_gate_passes_when_bundle_sections_are_green() {
    let bundle =
        build_m24_rl_gate_evidence_bundle(sample_m24_bundle_input()).expect("build bundle");
    let decision = evaluate_m24_rl_gate_exit(&bundle);

    assert!(decision.pass);
    assert!(decision.reason_codes.is_empty());
}

#[test]
fn spec_1998_c02_exit_gate_emits_reason_codes_for_failing_sections() {
    let mut input = sample_m24_bundle_input();
    input.benchmark.pass = false;
    input.safety.pass = false;
    input.operations.rollback_proven = false;
    let bundle = build_m24_rl_gate_evidence_bundle(input).expect("build bundle");

    let decision = evaluate_m24_rl_gate_exit(&bundle);
    assert!(!decision.pass);
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "benchmark_failed"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "safety_failed"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "operations_failed"));
}

#[test]
fn spec_1998_c03_exit_gate_decision_json_is_machine_readable() {
    let bundle =
        build_m24_rl_gate_evidence_bundle(sample_m24_bundle_input()).expect("build bundle");
    let decision = evaluate_m24_rl_gate_exit(&bundle);
    let payload = decision.to_json_value();

    assert!(payload["pass"].is_boolean());
    assert!(payload["reason_codes"].is_array());
}

#[test]
fn spec_1998_c04_exit_gate_fails_closed_on_blank_runbook_refs() {
    let mut bundle =
        build_m24_rl_gate_evidence_bundle(sample_m24_bundle_input()).expect("build bundle");
    bundle.runbooks.operator_runbook_ref = " ".to_string();
    bundle.runbooks.incident_playbook_ref.clear();

    let decision = evaluate_m24_rl_gate_exit(&bundle);
    assert!(!decision.pass);
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "operator_runbook_missing"));
    assert!(decision
        .reason_codes
        .iter()
        .any(|code| code == "incident_playbook_missing"));
}

#[test]
fn regression_m24_exit_gate_detects_whitespace_only_runbook_refs() {
    let mut bundle =
        build_m24_rl_gate_evidence_bundle(sample_m24_bundle_input()).expect("build bundle");
    bundle.runbooks.operator_runbook_ref = "\n\t".to_string();
    bundle.runbooks.incident_playbook_ref = "    ".to_string();

    let decision = evaluate_m24_rl_gate_exit(&bundle);
    assert!(!decision.pass);
    assert_eq!(decision.reason_codes.len(), 2);
}

fn sample_m24_bundle_input() -> M24RLGateEvidenceBundleInput {
    M24RLGateEvidenceBundleInput {
        generated_at_epoch_ms: 1_706_000_010_000,
        benchmark: M24RLGateEvidenceBenchmarkInput {
            pass: true,
            pass_reports: 4,
            fail_reports: 0,
            invalid_files: 0,
            reason_codes: Vec::new(),
            report_ref: "artifacts/benchmark-artifact-gate-summary-manifest-report.json"
                .to_string(),
        },
        safety: M24RLGateEvidenceSafetyInput {
            pass: true,
            observed_regression: 0.01,
            max_allowed_regression: 0.05,
            reason_codes: Vec::new(),
            report_ref: "artifacts/safety-regression-report.json".to_string(),
        },
        operations: M24RLGateEvidenceOperationsInput {
            pause_resume_proven: true,
            rollback_proven: true,
            recovery_drill_proven: true,
            recovery_log_ref: "artifacts/recovery-drill.log".to_string(),
        },
        runbooks: M24RLGateEvidenceRunbooksInput {
            operator_runbook_ref: "docs/guides/training-ops.md".to_string(),
            incident_playbook_ref: "docs/guides/incident-playbook.md".to_string(),
        },
    }
}

fn sample_summary_manifest(
    pass_entries: usize,
    fail_entries: usize,
    invalid_files: usize,
) -> BenchmarkArtifactGateReportSummaryManifest {
    let mut entries = Vec::new();
    for idx in 0..pass_entries {
        entries.push(BenchmarkArtifactGateReportSummaryEntry {
            path: PathBuf::from(format!("pass-{idx}.json")),
            pass: true,
            valid_entries: 1,
            invalid_entries: 0,
            scanned_entries: 1,
            invalid_ratio: 0.0,
            reason_codes: Vec::new(),
        });
    }
    for idx in 0..fail_entries {
        entries.push(BenchmarkArtifactGateReportSummaryEntry {
            path: PathBuf::from(format!("fail-{idx}.json")),
            pass: false,
            valid_entries: 0,
            invalid_entries: 1,
            scanned_entries: 1,
            invalid_ratio: 1.0,
            reason_codes: vec!["no_valid_artifacts".to_string()],
        });
    }

    let mut invalid = Vec::new();
    for idx in 0..invalid_files {
        invalid.push(BenchmarkArtifactGateReportSummaryInvalidFile {
            path: PathBuf::from(format!("invalid-{idx}.json")),
            reason: "malformed gate report".to_string(),
        });
    }

    BenchmarkArtifactGateReportSummaryManifest {
        schema_version: 1,
        directory: PathBuf::from("summary"),
        scanned_json_files: entries.len() + invalid.len(),
        entries,
        invalid_files: invalid,
        pass_entries,
        fail_entries,
    }
}

fn sample_summary_gate_report_manifest(
    pass_reports: usize,
    fail_reports: usize,
    invalid_files: usize,
) -> BenchmarkArtifactGateSummaryReportManifest {
    let mut entries = Vec::new();
    for idx in 0..pass_reports {
        entries.push(BenchmarkArtifactGateSummaryReportManifestEntry {
            path: PathBuf::from(format!("pass-report-{idx}.json")),
            pass: true,
            pass_reports: 1,
            fail_reports: 0,
            invalid_files: 0,
            total_reports: 1,
            fail_ratio: 0.0,
            invalid_file_ratio: 0.0,
            reason_codes: Vec::new(),
        });
    }
    for idx in 0..fail_reports {
        entries.push(BenchmarkArtifactGateSummaryReportManifestEntry {
            path: PathBuf::from(format!("fail-report-{idx}.json")),
            pass: false,
            pass_reports: 0,
            fail_reports: 1,
            invalid_files: 0,
            total_reports: 1,
            fail_ratio: 1.0,
            invalid_file_ratio: 0.0,
            reason_codes: vec!["below_min_pass_entries".to_string()],
        });
    }

    let mut invalid = Vec::new();
    for idx in 0..invalid_files {
        invalid.push(BenchmarkArtifactGateSummaryReportManifestInvalidFile {
            path: PathBuf::from(format!("invalid-report-{idx}.json")),
            reason: "malformed summary gate report".to_string(),
        });
    }

    BenchmarkArtifactGateSummaryReportManifest {
        schema_version: 1,
        directory: PathBuf::from("summary-report-manifest"),
        scanned_json_files: entries.len() + invalid.len(),
        entries,
        invalid_files: invalid,
        pass_reports,
        fail_reports,
    }
}
