//! Session persistence helpers with JSONL and SQLite backends.
use super::*;
use postgres::{Client, NoTls};
use rusqlite::{params, Connection};
use std::env;

const SESSION_USAGE_SCHEMA_VERSION: u32 = 1;

pub(super) struct ResolvedSessionBackend {
    pub backend: SessionStorageBackend,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionUsageRecord {
    schema_version: u32,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
}

/// Resolve session storage backend from env override, path hints, and existing artifacts.
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

/// Import legacy JSONL session snapshot into SQLite backend when destination is empty.
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

/// Read session entries from selected backend implementation.
pub(super) fn read_session_entries(
    path: &Path,
    backend: SessionStorageBackend,
) -> Result<Vec<SessionEntry>> {
    match backend {
        SessionStorageBackend::Jsonl => read_session_entries_jsonl(path),
        SessionStorageBackend::Sqlite => read_session_entries_sqlite(path),
        SessionStorageBackend::Postgres => read_session_entries_postgres(path),
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
        SessionStorageBackend::Postgres => write_session_entries_postgres(path, entries),
    }
}

/// Read persisted usage summary for a session store.
pub(super) fn read_session_usage_summary(
    path: &Path,
    backend: SessionStorageBackend,
) -> Result<SessionUsageSummary> {
    match backend {
        SessionStorageBackend::Postgres => read_session_usage_summary_postgres(path),
        SessionStorageBackend::Jsonl | SessionStorageBackend::Sqlite => {
            read_session_usage_summary_json(path)
        }
    }
}

fn read_session_usage_summary_json(path: &Path) -> Result<SessionUsageSummary> {
    let usage_path = session_usage_path(path);
    if !usage_path.exists() {
        return Ok(SessionUsageSummary::default());
    }

    let raw = fs::read_to_string(&usage_path)
        .with_context(|| format!("failed to read session usage file {}", usage_path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(SessionUsageSummary::default());
    }

    match serde_json::from_str::<SessionUsageRecord>(trimmed) {
        Ok(record) => {
            if record.schema_version > SESSION_USAGE_SCHEMA_VERSION {
                bail!(
                    "unsupported session usage schema version {} in {} (supported up to {})",
                    record.schema_version,
                    usage_path.display(),
                    SESSION_USAGE_SCHEMA_VERSION
                );
            }
            Ok(SessionUsageSummary {
                input_tokens: record.input_tokens,
                output_tokens: record.output_tokens,
                total_tokens: record.total_tokens,
                estimated_cost_usd: record.estimated_cost_usd,
            })
        }
        Err(_) => {
            // Backward compatibility for schema-less files.
            serde_json::from_str::<SessionUsageSummary>(trimmed).with_context(|| {
                format!(
                    "failed to parse session usage summary in {}",
                    usage_path.display()
                )
            })
        }
    }
}

/// Persist usage summary atomically for a session store.
pub(super) fn write_session_usage_summary_atomic(
    path: &Path,
    usage: &SessionUsageSummary,
    backend: SessionStorageBackend,
) -> Result<()> {
    match backend {
        SessionStorageBackend::Postgres => write_session_usage_summary_postgres(path, usage),
        SessionStorageBackend::Jsonl | SessionStorageBackend::Sqlite => {
            write_session_usage_summary_atomic_json(path, usage)
        }
    }
}

fn write_session_usage_summary_atomic_json(path: &Path, usage: &SessionUsageSummary) -> Result<()> {
    let usage_path = session_usage_path(path);
    if let Some(parent) = usage_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create session usage directory {}",
                    parent.display()
                )
            })?;
        }
    }

    let file_name = usage_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "session.usage.json".to_string());
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_file_name = format!(".{file_name}.{timestamp}.tmp");
    let temp_path = usage_path.with_file_name(temp_file_name);

    let record = SessionUsageRecord {
        schema_version: SESSION_USAGE_SCHEMA_VERSION,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        estimated_cost_usd: usage.estimated_cost_usd,
    };
    let serialized =
        serde_json::to_string_pretty(&record).context("failed to encode session usage summary")?;
    fs::write(&temp_path, format!("{serialized}\n"))
        .with_context(|| format!("failed to write session usage temp {}", temp_path.display()))?;
    fs::rename(&temp_path, &usage_path).with_context(|| {
        format!(
            "failed to atomically replace session usage file {} with {}",
            usage_path.display(),
            temp_path.display()
        )
    })?;
    Ok(())
}

fn session_usage_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.usage.json", path.display()))
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

fn read_session_entries_postgres(path: &Path) -> Result<Vec<SessionEntry>> {
    let session_path_key = session_path_key(path)?;
    let mut client = open_session_postgres_client()?;
    let rows = client
        .query(
            r#"
            SELECT id, parent_id, message_json
            FROM tau_session_entries
            WHERE session_path_key = $1
            ORDER BY id ASC
            "#,
            &[&session_path_key],
        )
        .context("failed to query postgres session entries")?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let id_raw: i64 = row.get(0);
        let parent_raw: Option<i64> = row.get(1);
        let message_json: String = row.get(2);
        let id = u64::try_from(id_raw).with_context(|| {
            format!(
                "invalid postgres session id {} for key {}",
                id_raw, session_path_key
            )
        })?;
        let parent_id = match parent_raw {
            Some(value) => Some(u64::try_from(value).with_context(|| {
                format!(
                    "invalid postgres parent id {} for key {}",
                    value, session_path_key
                )
            })?),
            None => None,
        };
        let message = serde_json::from_str::<Message>(&message_json).with_context(|| {
            format!(
                "failed to decode postgres session message for id {} and key {}",
                id, session_path_key
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

fn write_session_entries_postgres(path: &Path, entries: &[SessionEntry]) -> Result<()> {
    let session_path_key = session_path_key(path)?;
    let mut client = open_session_postgres_client()?;
    let mut transaction = client
        .transaction()
        .context("failed to start postgres session transaction")?;
    transaction
        .execute(
            "DELETE FROM tau_session_entries WHERE session_path_key = $1",
            &[&session_path_key],
        )
        .context("failed to clear postgres session entries")?;
    for entry in entries {
        let id = i64::try_from(entry.id)
            .with_context(|| format!("session id {} exceeds postgres bigint", entry.id))?;
        let parent_id = match entry.parent_id {
            Some(value) => Some(
                i64::try_from(value)
                    .with_context(|| format!("parent id {} exceeds postgres bigint", value))?,
            ),
            None => None,
        };
        let message_json =
            serde_json::to_string(&entry.message).context("failed to encode session message")?;
        transaction
            .execute(
                r#"
                INSERT INTO tau_session_entries (session_path_key, id, parent_id, message_json)
                VALUES ($1, $2, $3, $4)
                "#,
                &[&session_path_key, &id, &parent_id, &message_json],
            )
            .context("failed to write postgres session entry")?;
    }
    transaction
        .commit()
        .context("failed to commit postgres session entries")?;
    Ok(())
}

fn read_session_usage_summary_postgres(path: &Path) -> Result<SessionUsageSummary> {
    let session_path_key = session_path_key(path)?;
    let mut client = open_session_postgres_client()?;
    let row = client
        .query_opt(
            r#"
            SELECT schema_version, input_tokens, output_tokens, total_tokens, estimated_cost_usd
            FROM tau_session_usage
            WHERE session_path_key = $1
            "#,
            &[&session_path_key],
        )
        .context("failed to query postgres session usage summary")?;

    let Some(row) = row else {
        return Ok(SessionUsageSummary::default());
    };

    let schema_version: i32 = row.get(0);
    if schema_version < 0 || schema_version as u32 > SESSION_USAGE_SCHEMA_VERSION {
        bail!(
            "unsupported postgres session usage schema version {} for key {} (supported up to {})",
            schema_version,
            session_path_key,
            SESSION_USAGE_SCHEMA_VERSION
        );
    }

    let input_tokens = u64::try_from(row.get::<_, i64>(1)).with_context(|| {
        format!(
            "invalid postgres input_tokens for session key {}",
            session_path_key
        )
    })?;
    let output_tokens = u64::try_from(row.get::<_, i64>(2)).with_context(|| {
        format!(
            "invalid postgres output_tokens for session key {}",
            session_path_key
        )
    })?;
    let total_tokens = u64::try_from(row.get::<_, i64>(3)).with_context(|| {
        format!(
            "invalid postgres total_tokens for session key {}",
            session_path_key
        )
    })?;
    let estimated_cost_usd: f64 = row.get(4);

    Ok(SessionUsageSummary {
        input_tokens,
        output_tokens,
        total_tokens,
        estimated_cost_usd,
    })
}

fn write_session_usage_summary_postgres(path: &Path, usage: &SessionUsageSummary) -> Result<()> {
    let session_path_key = session_path_key(path)?;
    let mut client = open_session_postgres_client()?;
    let input_tokens = i64::try_from(usage.input_tokens).with_context(|| {
        format!(
            "input_tokens {} exceeds postgres bigint",
            usage.input_tokens
        )
    })?;
    let output_tokens = i64::try_from(usage.output_tokens).with_context(|| {
        format!(
            "output_tokens {} exceeds postgres bigint",
            usage.output_tokens
        )
    })?;
    let total_tokens = i64::try_from(usage.total_tokens).with_context(|| {
        format!(
            "total_tokens {} exceeds postgres bigint",
            usage.total_tokens
        )
    })?;
    let schema_version = i32::try_from(SESSION_USAGE_SCHEMA_VERSION)
        .context("session usage schema version exceeds postgres integer")?;
    client
        .execute(
            r#"
            INSERT INTO tau_session_usage (
                session_path_key,
                schema_version,
                input_tokens,
                output_tokens,
                total_tokens,
                estimated_cost_usd
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (session_path_key) DO UPDATE SET
                schema_version = EXCLUDED.schema_version,
                input_tokens = EXCLUDED.input_tokens,
                output_tokens = EXCLUDED.output_tokens,
                total_tokens = EXCLUDED.total_tokens,
                estimated_cost_usd = EXCLUDED.estimated_cost_usd
            "#,
            &[
                &session_path_key,
                &schema_version,
                &input_tokens,
                &output_tokens,
                &total_tokens,
                &usage.estimated_cost_usd,
            ],
        )
        .context("failed to upsert postgres session usage summary")?;
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

fn open_session_postgres_client() -> Result<Client> {
    let dsn = env::var(SESSION_POSTGRES_DSN_ENV)
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!(
                "{}=postgres requires non-empty {}",
                SESSION_BACKEND_ENV,
                SESSION_POSTGRES_DSN_ENV
            )
        })?;
    let mut client = Client::connect(&dsn, NoTls)
        .context("failed to connect to postgres session store backend")?;
    initialize_session_postgres_schema(&mut client)?;
    Ok(client)
}

fn initialize_session_postgres_schema(client: &mut Client) -> Result<()> {
    client
        .batch_execute(
            r#"
            CREATE TABLE IF NOT EXISTS tau_session_entries (
                session_path_key TEXT NOT NULL,
                id BIGINT NOT NULL,
                parent_id BIGINT NULL,
                message_json TEXT NOT NULL,
                PRIMARY KEY (session_path_key, id)
            );
            CREATE INDEX IF NOT EXISTS idx_tau_session_entries_parent
                ON tau_session_entries(session_path_key, parent_id);
            CREATE TABLE IF NOT EXISTS tau_session_usage (
                session_path_key TEXT PRIMARY KEY,
                schema_version INTEGER NOT NULL,
                input_tokens BIGINT NOT NULL,
                output_tokens BIGINT NOT NULL,
                total_tokens BIGINT NOT NULL,
                estimated_cost_usd DOUBLE PRECISION NOT NULL
            );
            "#,
        )
        .context("failed to initialize postgres session schema")?;
    Ok(())
}

fn session_path_key(path: &Path) -> Result<String> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical.to_string_lossy().to_string());
    }
    if path.is_absolute() {
        return Ok(path.to_string_lossy().to_string());
    }
    let current_dir = std::env::current_dir().context("failed to resolve current directory")?;
    Ok(current_dir.join(path).to_string_lossy().to_string())
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
