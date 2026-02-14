use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::time_utils::current_unix_timestamp;

/// Writes text using a temp file + rename so readers never observe partial data.
pub fn write_text_atomic(path: &Path, content: &str) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("destination path cannot be empty");
    }
    if path.exists() && path.is_dir() {
        bail!("destination path '{}' is a directory", path.display());
    }

    let parent_dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent_dir)
        .with_context(|| format!("failed to create {}", parent_dir.display()))?;

    let temp_name = format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("session-graph"),
        std::process::id(),
        current_unix_timestamp()
    );
    let temp_path = parent_dir.join(temp_name);
    std::fs::write(&temp_path, content)
        .with_context(|| format!("failed to write temporary file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to rename temporary graph file {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}
