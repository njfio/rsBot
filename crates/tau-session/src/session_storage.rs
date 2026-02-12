//! Session JSONL persistence helpers with atomic write semantics.
use super::*;

pub(super) fn read_session_entries(path: &Path) -> Result<Vec<SessionEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)
        .with_context(|| format!("failed to open session file {}", path.display()))?;
    let reader = BufReader::new(file);

    let mut entries = Vec::new();
    let mut meta_seen = false;

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("failed to read line {} from {}", index + 1, path.display())
        })?;

        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<SessionRecord>(&line) {
            Ok(SessionRecord::Meta(meta)) => {
                meta_seen = true;
                if meta.schema_version > SESSION_SCHEMA_VERSION {
                    bail!(
                        "unsupported session schema version {} in {} (supported up to {})",
                        meta.schema_version,
                        path.display(),
                        SESSION_SCHEMA_VERSION
                    );
                }
            }
            Ok(SessionRecord::Entry(entry)) => entries.push(entry),
            Err(_) => {
                // Backward compatibility: old files with plain SessionEntry lines.
                let entry = serde_json::from_str::<SessionEntry>(&line).with_context(|| {
                    format!(
                        "failed to parse session line {} in {}",
                        index + 1,
                        path.display()
                    )
                })?;
                entries.push(entry);
            }
        }
    }

    if !meta_seen && !entries.is_empty() {
        // Legacy format is accepted; no-op here.
    }

    Ok(entries)
}

pub(super) fn write_session_entries_atomic(path: &Path, entries: &[SessionEntry]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create session directory {}", parent.display())
            })?;
        }
    }

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "session".to_string());

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_file_name = format!(".{file_name}.{timestamp}.tmp");
    let temp_path = path.with_file_name(temp_file_name);

    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| format!("failed to open temp file {}", temp_path.display()))?;

        let meta = SessionRecord::Meta(SessionMetaRecord {
            schema_version: SESSION_SCHEMA_VERSION,
        });
        writeln!(file, "{}", serde_json::to_string(&meta)?)
            .with_context(|| format!("failed to write meta to {}", temp_path.display()))?;

        for entry in entries {
            let line = serde_json::to_string(&SessionRecord::Entry(entry.clone()))?;
            writeln!(file, "{line}").with_context(|| {
                format!("failed to write session entry to {}", temp_path.display())
            })?;
        }

        file.sync_all()
            .with_context(|| format!("failed to sync temp file {}", temp_path.display()))?;
    }

    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to atomically replace session file {} with {}",
            path.display(),
            temp_path.display()
        )
    })?;

    Ok(())
}
