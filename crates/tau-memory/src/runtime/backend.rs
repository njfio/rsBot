use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use super::{
    MemoryRelation, MemoryStorageBackend, ResolvedMemoryBackend, RuntimeMemoryRecord,
    MEMORY_BACKEND_ENV, MEMORY_RUNTIME_ENTRIES_FILE_NAME, MEMORY_RUNTIME_ENTRIES_SQLITE_FILE_NAME,
    MEMORY_STORAGE_REASON_DEFAULT_SQLITE, MEMORY_STORAGE_REASON_ENV_AUTO,
    MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK, MEMORY_STORAGE_REASON_ENV_JSONL,
    MEMORY_STORAGE_REASON_ENV_SQLITE, MEMORY_STORAGE_REASON_EXISTING_JSONL,
    MEMORY_STORAGE_REASON_EXISTING_SQLITE, MEMORY_STORAGE_REASON_PATH_JSONL,
    MEMORY_STORAGE_REASON_PATH_SQLITE,
};

/// Resolve memory storage backend from env override, path hints, and existing artifacts.
pub(super) fn resolve_memory_backend(root_dir: &Path) -> ResolvedMemoryBackend {
    let env_backend = std::env::var(MEMORY_BACKEND_ENV)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase());
    let env_backend = env_backend.as_deref().unwrap_or("auto");

    if env_backend != "auto" && env_backend != "jsonl" && env_backend != "sqlite" {
        let mut inferred = infer_memory_backend(root_dir);
        inferred.reason_code = MEMORY_STORAGE_REASON_ENV_INVALID_FALLBACK.to_string();
        return inferred;
    }

    if env_backend == "jsonl" {
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Jsonl,
            storage_path: Some(resolve_jsonl_path(root_dir)),
            reason_code: MEMORY_STORAGE_REASON_ENV_JSONL.to_string(),
        };
    }

    if env_backend == "sqlite" {
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Sqlite,
            storage_path: Some(resolve_sqlite_path(root_dir)),
            reason_code: MEMORY_STORAGE_REASON_ENV_SQLITE.to_string(),
        };
    }

    let mut inferred = infer_memory_backend(root_dir);
    if std::env::var(MEMORY_BACKEND_ENV).is_ok() {
        inferred.reason_code = MEMORY_STORAGE_REASON_ENV_AUTO.to_string();
    }
    inferred
}

fn infer_memory_backend(root_dir: &Path) -> ResolvedMemoryBackend {
    let extension = root_dir
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());

    if matches!(extension.as_deref(), Some("jsonl")) {
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Jsonl,
            storage_path: Some(root_dir.to_path_buf()),
            reason_code: MEMORY_STORAGE_REASON_PATH_JSONL.to_string(),
        };
    }
    if matches!(extension.as_deref(), Some("sqlite" | "db")) {
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Sqlite,
            storage_path: Some(root_dir.to_path_buf()),
            reason_code: MEMORY_STORAGE_REASON_PATH_SQLITE.to_string(),
        };
    }

    if root_dir.exists() && root_dir.is_file() {
        if looks_like_sqlite_file(root_dir) {
            return ResolvedMemoryBackend {
                backend: MemoryStorageBackend::Sqlite,
                storage_path: Some(root_dir.to_path_buf()),
                reason_code: MEMORY_STORAGE_REASON_EXISTING_SQLITE.to_string(),
            };
        }
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Jsonl,
            storage_path: Some(root_dir.to_path_buf()),
            reason_code: MEMORY_STORAGE_REASON_EXISTING_JSONL.to_string(),
        };
    }

    let sqlite_candidate = root_dir.join(MEMORY_RUNTIME_ENTRIES_SQLITE_FILE_NAME);
    if sqlite_candidate.exists() {
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Sqlite,
            storage_path: Some(sqlite_candidate),
            reason_code: MEMORY_STORAGE_REASON_EXISTING_SQLITE.to_string(),
        };
    }

    let jsonl_candidate = root_dir.join(MEMORY_RUNTIME_ENTRIES_FILE_NAME);
    if jsonl_candidate.exists() {
        return ResolvedMemoryBackend {
            backend: MemoryStorageBackend::Sqlite,
            storage_path: Some(resolve_sqlite_path(root_dir)),
            reason_code: MEMORY_STORAGE_REASON_EXISTING_JSONL.to_string(),
        };
    }

    ResolvedMemoryBackend {
        backend: MemoryStorageBackend::Sqlite,
        storage_path: Some(resolve_sqlite_path(root_dir)),
        reason_code: MEMORY_STORAGE_REASON_DEFAULT_SQLITE.to_string(),
    }
}

fn resolve_jsonl_path(root_dir: &Path) -> PathBuf {
    match root_dir
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jsonl") => root_dir.to_path_buf(),
        _ => root_dir.join(MEMORY_RUNTIME_ENTRIES_FILE_NAME),
    }
}

fn resolve_sqlite_path(root_dir: &Path) -> PathBuf {
    match root_dir
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("sqlite" | "db") => root_dir.to_path_buf(),
        _ => root_dir.join(MEMORY_RUNTIME_ENTRIES_SQLITE_FILE_NAME),
    }
}

pub(super) fn looks_like_sqlite_file(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut prefix = [0u8; 16];
    if std::io::Read::read(&mut file, &mut prefix).unwrap_or_default() < 16 {
        return false;
    }
    &prefix == b"SQLite format 3\0"
}

pub(super) fn append_record_jsonl(path: &Path, record: &RuntimeMemoryRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create memory store root {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open memory entries file {}", path.display()))?;
    let encoded = serde_json::to_string(record).context("failed to encode memory record")?;
    file.write_all(encoded.as_bytes())
        .with_context(|| format!("failed to write memory record to {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to write newline to {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush memory entries file {}", path.display()))?;
    Ok(())
}

pub(super) fn load_records_jsonl(path: &Path) -> Result<Vec<RuntimeMemoryRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open memory entries file {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "failed to read memory entries file {} at line {}",
                path.display(),
                index + 1
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<RuntimeMemoryRecord>(trimmed).with_context(|| {
            format!(
                "failed to parse memory entries file {} at line {}",
                path.display(),
                index + 1
            )
        })?;
        records.push(record);
    }
    Ok(records)
}

pub(super) fn append_record_sqlite(path: &Path, record: &RuntimeMemoryRecord) -> Result<()> {
    let mut connection = open_memory_sqlite_connection(path)?;
    initialize_memory_sqlite_schema(&connection)?;
    let transaction = connection.transaction()?;
    let encoded = serde_json::to_string(record).context("failed to encode memory record")?;
    transaction.execute(
        r#"
        INSERT INTO memory_records (memory_id, updated_unix_ms, record_json)
        VALUES (?1, ?2, ?3)
        "#,
        params![record.entry.memory_id, record.updated_unix_ms, encoded],
    )?;
    transaction.execute(
        r#"
        DELETE FROM memory_relations
        WHERE source_memory_id = ?1
        "#,
        params![record.entry.memory_id],
    )?;
    for relation in &record.relations {
        transaction.execute(
            r#"
            INSERT INTO memory_relations (
                source_memory_id,
                target_memory_id,
                relation_type,
                weight,
                effective_weight,
                updated_unix_ms
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(source_memory_id, target_memory_id, relation_type)
            DO UPDATE SET
                weight = excluded.weight,
                effective_weight = excluded.effective_weight,
                updated_unix_ms = excluded.updated_unix_ms
            "#,
            params![
                record.entry.memory_id,
                relation.target_id,
                relation.relation_type,
                relation.weight,
                relation.effective_weight,
                record.updated_unix_ms
            ],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

pub(super) fn load_records_sqlite(path: &Path) -> Result<Vec<RuntimeMemoryRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let connection = open_memory_sqlite_connection(path)?;
    initialize_memory_sqlite_schema(&connection)?;
    let mut statement = connection.prepare(
        r#"
        SELECT record_json
        FROM memory_records
        ORDER BY row_id ASC
        "#,
    )?;
    let mut rows = statement.query([])?;
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        let encoded: String = row.get(0)?;
        let record = serde_json::from_str::<RuntimeMemoryRecord>(&encoded)
            .context("failed to decode sqlite memory record")?;
        records.push(record);
    }
    Ok(records)
}

pub(super) fn load_relation_map_sqlite(
    path: &Path,
) -> Result<HashMap<String, Vec<MemoryRelation>>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let connection = open_memory_sqlite_connection(path)?;
    initialize_memory_sqlite_schema(&connection)?;
    let mut statement = connection.prepare(
        r#"
        SELECT
            source_memory_id,
            target_memory_id,
            relation_type,
            weight,
            effective_weight
        FROM memory_relations
        ORDER BY source_memory_id ASC, target_memory_id ASC, relation_type ASC
        "#,
    )?;
    let mut rows = statement.query([])?;
    let mut relation_map = HashMap::<String, Vec<MemoryRelation>>::new();
    while let Some(row) = rows.next()? {
        let source_memory_id: String = row.get(0)?;
        let relation = MemoryRelation {
            target_id: row.get(1)?,
            relation_type: row.get(2)?,
            weight: row.get(3)?,
            effective_weight: row.get(4)?,
        };
        relation_map
            .entry(source_memory_id)
            .or_default()
            .push(relation);
    }
    Ok(relation_map)
}

/// Open SQLite memory store connection with WAL pragmas and busy timeout.
pub(super) fn open_memory_sqlite_connection(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create memory store root {}", parent.display()))?;
    }
    let connection = Connection::open(path)
        .with_context(|| format!("failed to open sqlite memory store {}", path.display()))?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        "#,
    )?;
    Ok(connection)
}

/// Ensure SQLite memory schema and indexes exist before reads/writes.
pub(super) fn initialize_memory_sqlite_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS memory_records (
            row_id INTEGER PRIMARY KEY AUTOINCREMENT,
            memory_id TEXT NOT NULL,
            updated_unix_ms INTEGER NOT NULL,
            record_json TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_memory_records_memory_id_updated
            ON memory_records(memory_id, updated_unix_ms);
        CREATE TABLE IF NOT EXISTS memory_relations (
            row_id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_memory_id TEXT NOT NULL,
            target_memory_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            weight REAL NOT NULL,
            effective_weight REAL NOT NULL,
            updated_unix_ms INTEGER NOT NULL,
            UNIQUE(source_memory_id, target_memory_id, relation_type)
        );
        CREATE INDEX IF NOT EXISTS idx_memory_relations_source
            ON memory_relations(source_memory_id, updated_unix_ms);
        CREATE INDEX IF NOT EXISTS idx_memory_relations_target
            ON memory_relations(target_memory_id, updated_unix_ms);
        "#,
    )?;
    Ok(())
}
