use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const DEFAULT_LOG_ROTATION_MAX_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_LOG_ROTATION_MAX_FILES: usize = 5;

/// Configuration for size-based log rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogRotationPolicy {
    pub max_bytes: u64,
    pub max_files: usize,
}

impl LogRotationPolicy {
    /// Build policy from env vars with safe defaults.
    pub fn from_env() -> Self {
        let max_bytes = std::env::var("TAU_LOG_ROTATION_MAX_BYTES")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_LOG_ROTATION_MAX_BYTES);
        let max_files = std::env::var("TAU_LOG_ROTATION_MAX_FILES")
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_LOG_ROTATION_MAX_FILES);
        Self {
            max_bytes,
            max_files,
        }
    }

    /// Returns true when size-based rotation is enabled.
    pub fn is_enabled(self) -> bool {
        self.max_bytes > 0 && self.max_files > 0
    }
}

/// Append one NDJSON line to `path`, applying size-based rotation policy.
pub fn append_line_with_rotation(path: &Path, line: &str, policy: LogRotationPolicy) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    if policy.is_enabled() && path.exists() {
        let current_size = std::fs::metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?
            .len();
        let incoming_size = line.len().saturating_add(1).try_into().unwrap_or(u64::MAX);
        if current_size.saturating_add(incoming_size) > policy.max_bytes {
            rotate_log_file(path, policy)?;
        }
    }

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn rotated_backup_path(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

fn rotate_log_file(path: &Path, policy: LogRotationPolicy) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !policy.is_enabled() {
        return Ok(());
    }

    if policy.max_files <= 1 {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to rotate {}", path.display()))?;
        return Ok(());
    }

    let max_backup_index = policy.max_files.saturating_sub(1);
    for index in (1..=max_backup_index).rev() {
        let source = if index == 1 {
            path.to_path_buf()
        } else {
            rotated_backup_path(path, index.saturating_sub(1))
        };
        if !source.exists() {
            continue;
        }
        let destination = rotated_backup_path(path, index);
        if destination.exists() {
            std::fs::remove_file(&destination).with_context(|| {
                format!("failed to replace rotated log {}", destination.display())
            })?;
        }
        std::fs::rename(&source, &destination).with_context(|| {
            format!(
                "failed to rotate {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{append_line_with_rotation, rotated_backup_path, LogRotationPolicy};

    fn read(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    #[test]
    fn spec_c01_append_rotation_rotates_when_size_threshold_exceeded() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("runtime-events.jsonl");
        let policy = LogRotationPolicy {
            max_bytes: 24,
            max_files: 3,
        };

        append_line_with_rotation(path.as_path(), r#"{"seq":1,"msg":"first"}"#, policy)
            .expect("append first");
        append_line_with_rotation(path.as_path(), r#"{"seq":2,"msg":"second"}"#, policy)
            .expect("append second");

        let first_backup = rotated_backup_path(path.as_path(), 1);
        assert!(
            first_backup.exists(),
            "expected first rotated backup to exist"
        );
        assert!(
            read(first_backup.as_path()).contains("\"seq\":1"),
            "backup should retain first record"
        );
        assert!(
            read(path.as_path()).contains("\"seq\":2"),
            "active log should contain second record after rotation"
        );
    }

    #[test]
    fn spec_c02_append_rotation_prunes_backups_to_max_files_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("runtime-events.jsonl");
        let policy = LogRotationPolicy {
            max_bytes: 18,
            max_files: 2,
        };

        for seq in 1..=6 {
            append_line_with_rotation(path.as_path(), &format!(r#"{{"seq":{seq}}}"#), policy)
                .expect("append line");
        }

        let backup_1 = rotated_backup_path(path.as_path(), 1);
        let backup_2 = rotated_backup_path(path.as_path(), 2);
        assert!(backup_1.exists(), "expected first backup to exist");
        assert!(
            !backup_2.exists(),
            "expected backups older than max_files retention to be pruned"
        );
    }

    #[test]
    fn spec_c03_log_rotation_policy_from_env_uses_valid_values_and_falls_back_on_invalid() {
        std::env::set_var("TAU_LOG_ROTATION_MAX_BYTES", "4096");
        std::env::set_var("TAU_LOG_ROTATION_MAX_FILES", "7");
        let parsed = LogRotationPolicy::from_env();
        assert_eq!(parsed.max_bytes, 4096);
        assert_eq!(parsed.max_files, 7);

        std::env::set_var("TAU_LOG_ROTATION_MAX_BYTES", "not-a-number");
        std::env::set_var("TAU_LOG_ROTATION_MAX_FILES", "0");
        let fallback = LogRotationPolicy::from_env();
        assert_eq!(fallback.max_bytes, 10 * 1024 * 1024);
        assert_eq!(fallback.max_files, 5);

        std::env::remove_var("TAU_LOG_ROTATION_MAX_BYTES");
        std::env::remove_var("TAU_LOG_ROTATION_MAX_FILES");
    }
}
