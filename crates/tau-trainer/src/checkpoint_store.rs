//! Policy checkpoint persistence and rollback-aware resume helpers.

use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::path::Path;
use tracing::instrument;

/// Current checkpoint schema version.
pub const CURRENT_CHECKPOINT_VERSION: u32 = 1;

/// Persisted checkpoint payload for policy/optimizer resume.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyCheckpoint {
    pub checkpoint_version: u32,
    pub run_id: String,
    pub policy_state: Value,
    pub optimizer_state: Value,
    pub global_step: u64,
    pub optimizer_step: u64,
    pub saved_at_unix_seconds: u64,
}

/// Source used for resume loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointSource {
    Primary,
    Fallback,
}

/// Outcome of checkpoint resume loading with diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumeCheckpointResult {
    pub checkpoint: PolicyCheckpoint,
    pub source: CheckpointSource,
    pub diagnostics: Vec<String>,
}

/// Saves a checkpoint to disk.
#[instrument(skip(checkpoint), fields(path = %path.display(), run_id = %checkpoint.run_id))]
pub fn save_policy_checkpoint(path: &Path, checkpoint: &PolicyCheckpoint) -> Result<()> {
    validate_checkpoint(checkpoint)?;
    let payload = checkpoint_to_value(checkpoint);
    let bytes = serde_json::to_vec_pretty(&payload).context("serialize checkpoint payload")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create checkpoint directory {}", parent.display()))?;
    }

    let temp_path = temporary_checkpoint_path(path);
    std::fs::write(&temp_path, bytes)
        .with_context(|| format!("write checkpoint temp file {}", temp_path.display()))?;

    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("remove existing checkpoint {}", path.display()))?;
    }
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "atomically replace checkpoint {} from {}",
            path.display(),
            temp_path.display()
        )
    })?;
    Ok(())
}

/// Loads a checkpoint from disk.
#[instrument(fields(path = %path.display()))]
pub fn load_policy_checkpoint(path: &Path) -> Result<PolicyCheckpoint> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read checkpoint {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse checkpoint JSON {}", path.display()))?;
    let checkpoint = checkpoint_from_value(&value)?;
    validate_checkpoint(&checkpoint)?;
    Ok(checkpoint)
}

/// Loads from primary checkpoint and falls back to a rollback checkpoint on
/// corruption/errors.
pub fn load_policy_checkpoint_with_rollback(
    primary_path: &Path,
    fallback_path: &Path,
) -> Result<ResumeCheckpointResult> {
    match load_policy_checkpoint(primary_path) {
        Ok(checkpoint) => Ok(ResumeCheckpointResult {
            checkpoint,
            source: CheckpointSource::Primary,
            diagnostics: Vec::new(),
        }),
        Err(primary_error) => match load_policy_checkpoint(fallback_path) {
            Ok(checkpoint) => Ok(ResumeCheckpointResult {
                checkpoint,
                source: CheckpointSource::Fallback,
                diagnostics: vec![format!("primary checkpoint load failed: {primary_error:#}")],
            }),
            Err(fallback_error) => bail!(
                "primary checkpoint load failed: {primary_error:#}; fallback checkpoint load failed: {fallback_error:#}"
            ),
        },
    }
}

/// Renders deterministic, operator-facing resume diagnostics for checkpoint
/// restore flows.
#[instrument(skip(result), fields(source = ?result.source, run_id = %result.checkpoint.run_id))]
pub fn render_resume_operator_diagnostics(result: &ResumeCheckpointResult) -> String {
    let mut lines = vec![format!(
        "checkpoint_resume source={} run_id={} global_step={} optimizer_step={}",
        source_label(result.source),
        result.checkpoint.run_id,
        result.checkpoint.global_step,
        result.checkpoint.optimizer_step
    )];

    for diagnostic in &result.diagnostics {
        lines.push(format!("checkpoint_resume diagnostic={diagnostic}"));
    }

    lines.join("\n")
}

fn checkpoint_to_value(checkpoint: &PolicyCheckpoint) -> Value {
    json!({
        "checkpoint_version": checkpoint.checkpoint_version,
        "run_id": checkpoint.run_id,
        "policy_state": checkpoint.policy_state,
        "optimizer_state": checkpoint.optimizer_state,
        "global_step": checkpoint.global_step,
        "optimizer_step": checkpoint.optimizer_step,
        "saved_at_unix_seconds": checkpoint.saved_at_unix_seconds
    })
}

fn checkpoint_from_value(value: &Value) -> Result<PolicyCheckpoint> {
    let object = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("checkpoint payload must be a JSON object"))?;

    let checkpoint_version = required_u64(object, "checkpoint_version")? as u32;
    if checkpoint_version != CURRENT_CHECKPOINT_VERSION {
        bail!(
            "unsupported checkpoint_version {checkpoint_version}; expected {CURRENT_CHECKPOINT_VERSION}"
        );
    }

    let run_id = required_string(object, "run_id")?;
    let policy_state = required_value(object, "policy_state")?;
    if !policy_state.is_object() {
        bail!("`policy_state` must be a JSON object");
    }
    let optimizer_state = required_value(object, "optimizer_state")?;
    if !optimizer_state.is_object() {
        bail!("`optimizer_state` must be a JSON object");
    }

    Ok(PolicyCheckpoint {
        checkpoint_version,
        run_id,
        policy_state,
        optimizer_state,
        global_step: required_u64(object, "global_step")?,
        optimizer_step: required_u64(object, "optimizer_step")?,
        saved_at_unix_seconds: required_u64(object, "saved_at_unix_seconds")?,
    })
}

fn required_value(object: &Map<String, Value>, field: &'static str) -> Result<Value> {
    object
        .get(field)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing required checkpoint field `{field}`"))
}

fn required_string(object: &Map<String, Value>, field: &'static str) -> Result<String> {
    let raw = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required checkpoint field `{field}`"))?;
    if raw.trim().is_empty() {
        bail!("checkpoint field `{field}` must not be empty");
    }
    Ok(raw.to_string())
}

fn required_u64(object: &Map<String, Value>, field: &'static str) -> Result<u64> {
    object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("missing required checkpoint field `{field}`"))
}

fn temporary_checkpoint_path(path: &Path) -> std::path::PathBuf {
    let mut temp_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "checkpoint".to_string());
    temp_name.push_str(".tmp");
    path.with_file_name(format!("{temp_name}.{}", std::process::id()))
}

fn validate_checkpoint(checkpoint: &PolicyCheckpoint) -> Result<()> {
    if checkpoint.checkpoint_version != CURRENT_CHECKPOINT_VERSION {
        bail!(
            "unsupported checkpoint_version {}; expected {}",
            checkpoint.checkpoint_version,
            CURRENT_CHECKPOINT_VERSION
        );
    }
    if checkpoint.run_id.trim().is_empty() {
        bail!("run_id must not be empty");
    }
    if !checkpoint.policy_state.is_object() {
        bail!("policy_state must be a JSON object");
    }
    if !checkpoint.optimizer_state.is_object() {
        bail!("optimizer_state must be a JSON object");
    }
    Ok(())
}

fn source_label(source: CheckpointSource) -> &'static str {
    match source {
        CheckpointSource::Primary => "primary",
        CheckpointSource::Fallback => "fallback",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        load_policy_checkpoint, load_policy_checkpoint_with_rollback,
        render_resume_operator_diagnostics, save_policy_checkpoint, CheckpointSource,
        PolicyCheckpoint, ResumeCheckpointResult,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "tau-trainer-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn make_checkpoint() -> PolicyCheckpoint {
        PolicyCheckpoint {
            checkpoint_version: 1,
            run_id: "run-1670".to_string(),
            policy_state: json!({
                "weights": [0.12, -0.8, 1.33],
                "temperature": 0.7
            }),
            optimizer_state: json!({
                "learning_rate": 0.0003,
                "momentum": [0.01, 0.02, 0.03]
            }),
            global_step: 42,
            optimizer_step: 19,
            saved_at_unix_seconds: 1_770_000_000,
        }
    }

    #[test]
    fn spec_c01_checkpoint_roundtrip_preserves_policy_and_optimizer_state() {
        let temp_dir = unique_temp_dir("checkpoint-roundtrip");
        let checkpoint_path = temp_dir.join("policy-checkpoint.json");
        let checkpoint = make_checkpoint();

        save_policy_checkpoint(&checkpoint_path, &checkpoint).expect("save checkpoint");
        let loaded = load_policy_checkpoint(&checkpoint_path).expect("load checkpoint");

        assert_eq!(loaded, checkpoint);
        std::fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn spec_c02_resume_uses_fallback_checkpoint_when_primary_is_corrupted() {
        let temp_dir = unique_temp_dir("checkpoint-fallback");
        let primary_path = temp_dir.join("primary.json");
        let fallback_path = temp_dir.join("fallback.json");

        std::fs::write(&primary_path, "{ this-is: not-json").expect("write corrupt primary");
        save_policy_checkpoint(&fallback_path, &make_checkpoint()).expect("save fallback");

        let resumed = load_policy_checkpoint_with_rollback(&primary_path, &fallback_path)
            .expect("resume with fallback");

        assert_eq!(resumed.source, CheckpointSource::Fallback);
        assert!(!resumed.diagnostics.is_empty());
        assert!(resumed
            .diagnostics
            .iter()
            .any(|line| line.contains("primary checkpoint load failed")));
        std::fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn spec_c03_loader_rejects_unsupported_versions_with_actionable_errors() {
        let temp_dir = unique_temp_dir("checkpoint-version");
        let checkpoint_path = temp_dir.join("unsupported-version.json");
        std::fs::write(
            &checkpoint_path,
            r#"{
                "checkpoint_version": 999,
                "run_id": "run-1670",
                "policy_state": { "weights": [1, 2, 3] },
                "optimizer_state": { "learning_rate": 0.001 },
                "global_step": 2,
                "optimizer_step": 1,
                "saved_at_unix_seconds": 1770000000
            }"#,
        )
        .expect("write unsupported checkpoint");

        let error = load_policy_checkpoint(&checkpoint_path).expect_err("unsupported version");
        assert!(error.to_string().contains("unsupported checkpoint_version"));
        std::fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn spec_1726_c01_rollback_reports_both_primary_and_fallback_corruption_errors() {
        let temp_dir = unique_temp_dir("checkpoint-dual-corruption");
        let primary_path = temp_dir.join("primary-corrupt.json");
        let fallback_path = temp_dir.join("fallback-corrupt.json");

        std::fs::write(&primary_path, "{not-json").expect("write primary corruption");
        std::fs::write(&fallback_path, "{also-not-json").expect("write fallback corruption");

        let error = load_policy_checkpoint_with_rollback(&primary_path, &fallback_path)
            .expect_err("dual corruption should fail");
        let message = error.to_string();
        assert!(message.contains("primary checkpoint load failed"));
        assert!(message.contains("fallback checkpoint load failed"));
        std::fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn spec_1726_c02_primary_checkpoint_is_preferred_when_both_are_valid() {
        let temp_dir = unique_temp_dir("checkpoint-primary-preferred");
        let primary_path = temp_dir.join("primary-valid.json");
        let fallback_path = temp_dir.join("fallback-valid.json");

        let mut primary = make_checkpoint();
        primary.global_step = 120;
        let mut fallback = make_checkpoint();
        fallback.global_step = 40;

        save_policy_checkpoint(&primary_path, &primary).expect("save primary");
        save_policy_checkpoint(&fallback_path, &fallback).expect("save fallback");

        let resumed = load_policy_checkpoint_with_rollback(&primary_path, &fallback_path)
            .expect("primary should be preferred");
        assert_eq!(resumed.source, CheckpointSource::Primary);
        assert!(resumed.diagnostics.is_empty());
        assert_eq!(resumed.checkpoint.global_step, 120);
        std::fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn spec_1726_c03_operator_diagnostics_include_source_run_and_steps() {
        let report = ResumeCheckpointResult {
            checkpoint: make_checkpoint(),
            source: CheckpointSource::Fallback,
            diagnostics: vec!["primary checkpoint load failed: parse error".to_string()],
        };

        let rendered = render_resume_operator_diagnostics(&report);
        assert!(rendered.contains("checkpoint_resume source=fallback"));
        assert!(rendered.contains("run_id=run-1670"));
        assert!(rendered.contains("global_step=42"));
        assert!(rendered.contains("optimizer_step=19"));
        assert!(rendered.contains("primary checkpoint load failed"));
    }
}
