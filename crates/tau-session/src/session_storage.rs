//! Session persistence helpers with JSONL and SQLite backends.
use super::*;
use rusqlite::{params, Connection};
use std::env;

pub(super) struct ResolvedSessionBackend {
    pub backend: SessionStorageBackend,
    pub reason_code: String,
}

pub(super) fn resolve_session_backend(path: &Path) -> Result<ResolvedSessionBackend> {
    let env_value = env::var(SESSION_BACKEND_ENV).ok();
    if let Some(raw) = env_value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let normalized = raw.to_ascii_lowercase();
        match normalized.as_str() {
            "auto" => return infer_session_backend(path),
            "jsonl" => {
                return Ok(ResolvedSessionBackend {
                    backend: SessionStorageBackend::Jsonl,
                    reason_code: "session_backend_env_jsonl".to_string(),
                });
            }
            "sqlite" => {
                return Ok(ResolvedSessionBackend {
                    backend: SessionStorageBackend::Sqlite,
                    reason_code: "session_backend_env_sqlite".to_string(),
                });
            }
            "postgres" => {
                let dsn = env::var(SESSION_POSTGRES_DSN_ENV).unwrap_or_default();
                if dsn.trim().is_empty() {
                    bail!(
                        "{}=postgres requires non-empty {}",
                        SESSION_BACKEND_ENV,
                        SESSION_POSTGRES_DSN_ENV
                    );
                }
                return Ok(ResolvedSessionBackend {
                    backend: SessionStorageBackend::Postgres,
                    reason_code: "session_backend_env_postgres".to_string(),
                });
            }
            _ => {
                bail!(
                    "unsupported {} value '{}' (expected auto|jsonl|sqlite|postgres)",
                    SESSION_BACKEND_ENV,
                    raw
                );
            }
        }
    }

    infer_session_backend(path)
}

fn infer_session_backend(path: &Path) -> Result<ResolvedSessionBackend> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if matches!(extension.as_deref(), Some("sqlite" | "db")) {
        return Ok(ResolvedSessionBackend {
            backend: SessionStorageBackend::Sqlite,
            reason_code: "session_backend_path_sqlite".to_string(),
        });
    }
    if matches!(extension.as_deref(), Some("jsonl")) {
        return Ok(ResolvedSessionBackend {
            backend: SessionStorageBackend::Jsonl,
            reason_code: "session_backend_path_jsonl".to_string(),
        });
    }

    if path.exists() {
        if is_sqlite_file(path)? {
            return Ok(ResolvedSessionBackend {
                backend: SessionStorageBackend::Sqlite,
                reason_code: "session_backend_existing_sqlite".to_string(),
            });
        }
        return Ok(ResolvedSessionBackend {
            backend: SessionStorageBackend::Jsonl,
            reason_code: "session_backend_existing_legacy_file".to_string(),
        });
    }

    Ok(ResolvedSessionBackend {
        backend: SessionStorageBackend::Jsonl,
        reason_code: "session_backend_default_jsonl".to_string(),
    })
}

pub(super) fn maybe_import_legacy_jsonl_into_sqlite(
    path: &Path,
    backend: SessionStorageBackend,
) -> Result<usize> {
    if backend != SessionStorageBackend::Sqlite {
        return Ok(0);
    }

    let legacy_jsonl_path = path.with_extension("jsonl");
    if legacy_jsonl_path == path || !legacy_jsonl_path.exists() {
        return Ok(0);
    }

    let connection = open_session_sqlite_connection(path)?;
    initialize_session_sqlite_schema(&connection)?;
    let existing_count: u64 = connection
        .query_row("SELECT COUNT(1) FROM session_entries", [], |row| row.get(0))
        .context("failed to inspect sqlite session entry count")?;
    if existing_count > 0 {
        return Ok(0);
    }

    let entries = read_session_entries_jsonl(&legacy_jsonl_path)?;
    if entries.is_empty() {
        return Ok(0);
    }

    write_session_entries_sqlite(path, &entries)?;
    Ok(entries.len())
}

pub(super) fn read_session_entries(
    path: &Path,
    backend: SessionStorageBackend,
) -> Result<Vec<SessionEntry>> {
    match backend {
        SessionStorageBackend::Jsonl => read_session_entries_jsonl(path),
        SessionStorageBackend::Sqlite => read_session_entries_sqlite(path),
        SessionStorageBackend::Postgres => bail!(
            "session postgres backend is scaffolded but not implemented; set {}=jsonl or sqlite",
            SESSION_BACKEND_ENV
        ),
    }
}

pub(super) fn write_session_entries_atomic(
    path: &Path,
    entries: &[SessionEntry],
    backend: SessionStorageBackend,
) -> Result<()> {
    match backend {
        SessionStorageBackend::Jsonl => write_session_entries_atomic_jsonl(path, entries),
        SessionStorageBackend::Sqlite => write_session_entries_sqlite(path, entries),
        SessionStorageBackend::Postgres => bail!(
            "session postgres backend is scaffolded but not implemented; set {}=jsonl or sqlite",
            SESSION_BACKEND_ENV
        ),
    }
}

fn read_session_entries_jsonl(path: &Path) -> Result<Vec<SessionEntry>> {
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

fn write_session_entries_atomic_jsonl(path: &Path, entries: &[SessionEntry]) -> Result<()> {
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

fn read_session_entries_sqlite(path: &Path) -> Result<Vec<SessionEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let connection = open_session_sqlite_connection(path)?;
    initialize_session_sqlite_schema(&connection)?;
    let mut statement = connection.prepare(
        r#"
        SELECT id, parent_id, message_json
        FROM session_entries
        ORDER BY id ASC
        "#,
    )?;
    let mut rows = statement.query([])?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next()? {
        let id: u64 = row.get(0)?;
        let parent_id: Option<u64> = row.get(1)?;
        let message_json: String = row.get(2)?;
        let message = serde_json::from_str::<Message>(&message_json).with_context(|| {
            format!(
                "failed to decode session message for id {} in {}",
                id,
                path.display()
            )
        })?;
        entries.push(SessionEntry {
            id,
            parent_id,
            message,
        });
    }
    Ok(entries)
}

fn write_session_entries_sqlite(path: &Path, entries: &[SessionEntry]) -> Result<()> {
    let mut connection = open_session_sqlite_connection(path)?;
    initialize_session_sqlite_schema(&connection)?;
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM session_entries", [])?;
    for entry in entries {
        let message_json =
            serde_json::to_string(&entry.message).context("failed to encode session message")?;
        transaction.execute(
            r#"
            INSERT INTO session_entries (id, parent_id, message_json)
            VALUES (?1, ?2, ?3)
            "#,
            params![entry.id, entry.parent_id, message_json],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn open_session_sqlite_connection(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create session directory {}", parent.display())
            })?;
        }
    }
    let connection = Connection::open(path)
        .with_context(|| format!("failed to open sqlite session store {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        "#,
    )?;
    Ok(connection)
}

fn initialize_session_sqlite_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS session_entries (
            id INTEGER PRIMARY KEY,
            parent_id INTEGER NULL,
            message_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_session_entries_parent_id
            ON session_entries(parent_id);
        "#,
    )?;
    Ok(())
}

fn is_sqlite_file(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to inspect session file {}", path.display()))?;
    let mut prefix = [0u8; 16];
    let read = std::io::Read::read(&mut file, &mut prefix)?;
    if read < 16 {
        return Ok(false);
    }
    Ok(&prefix == b"SQLite format 3\0")
}
