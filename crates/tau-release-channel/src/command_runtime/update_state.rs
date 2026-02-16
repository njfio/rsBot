use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_core::write_text_atomic;

use super::{ReleaseChannel, RELEASE_UPDATE_STATE_SCHEMA_VERSION};

/// Persisted release update plan/apply state record stored beside channel config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReleaseUpdateStateFile {
    pub(super) schema_version: u32,
    pub(super) channel: ReleaseChannel,
    pub(super) current_version: String,
    pub(super) target_version: String,
    pub(super) action: String,
    pub(super) dry_run: bool,
    pub(super) lookup_source: String,
    pub(super) guard_code: String,
    pub(super) guard_reason: String,
    pub(super) planned_at_unix_ms: u64,
    pub(super) apply_attempts: u64,
    pub(super) last_apply_unix_ms: Option<u64>,
    pub(super) last_apply_status: Option<String>,
    pub(super) last_apply_target: Option<String>,
    pub(super) rollback_channel: Option<ReleaseChannel>,
    pub(super) rollback_version: Option<String>,
}

/// Load release update state file and enforce supported schema version.
pub(super) fn load_release_update_state_file(
    path: &Path,
) -> Result<Option<ReleaseUpdateStateFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read release update state {}", path.display()))?;
    let parsed = serde_json::from_str::<ReleaseUpdateStateFile>(&raw)
        .with_context(|| format!("failed to parse release update state {}", path.display()))?;
    if parsed.schema_version != RELEASE_UPDATE_STATE_SCHEMA_VERSION {
        bail!(
            "unsupported release update state schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            RELEASE_UPDATE_STATE_SCHEMA_VERSION
        );
    }
    Ok(Some(parsed))
}

/// Save release update state file atomically with trailing newline.
pub(super) fn save_release_update_state_file(
    path: &Path,
    state: &ReleaseUpdateStateFile,
) -> Result<()> {
    let mut encoded =
        serde_json::to_string_pretty(state).context("failed to encode release update state")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "release update state path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create release update state directory {}",
            parent.display()
        )
    })?;
    write_text_atomic(path, &encoded)
}
