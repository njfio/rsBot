use std::{
    io::{BufRead, Write},
    path::{Component, Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use pi_ai::Message;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{current_unix_timestamp_ms, write_text_atomic};

const CHANNEL_STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChannelStoreMeta {
    schema_version: u32,
    transport: String,
    channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChannelLogEntry {
    pub timestamp_unix_ms: u64,
    pub direction: String,
    pub event_key: Option<String>,
    pub source: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChannelContextEntry {
    pub timestamp_unix_ms: u64,
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChannelRef {
    pub transport: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChannelInspectReport {
    pub transport: String,
    pub channel_id: String,
    pub channel_dir: PathBuf,
    pub log_records: usize,
    pub context_records: usize,
    pub invalid_log_lines: usize,
    pub invalid_context_lines: usize,
    pub artifact_records: usize,
    pub invalid_artifact_lines: usize,
    pub active_artifacts: usize,
    pub expired_artifacts: usize,
    pub memory_exists: bool,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChannelRepairReport {
    pub log_removed_lines: usize,
    pub context_removed_lines: usize,
    pub artifact_expired_removed: usize,
    pub artifact_invalid_removed: usize,
    pub log_backup_path: Option<PathBuf>,
    pub context_backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ChannelArtifactRecord {
    pub id: String,
    pub run_id: String,
    pub artifact_type: String,
    pub visibility: String,
    pub relative_path: String,
    pub bytes: u64,
    pub checksum_sha256: String,
    pub created_unix_ms: u64,
    pub expires_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct ChannelArtifactLoadReport {
    pub records: Vec<ChannelArtifactRecord>,
    pub invalid_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub(crate) struct ChannelArtifactPurgeReport {
    pub expired_removed: usize,
    pub invalid_removed: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct ChannelStore {
    base_dir: PathBuf,
    transport: String,
    channel_id: String,
}

#[allow(dead_code)]
impl ChannelStore {
    pub(crate) fn open(base_dir: &Path, transport: &str, channel_id: &str) -> Result<Self> {
        let transport = transport.trim();
        let channel_id = channel_id.trim();
        if transport.is_empty() || channel_id.is_empty() {
            bail!("channel store transport and channel id must be non-empty");
        }

        let store = Self {
            base_dir: base_dir.to_path_buf(),
            transport: transport.to_string(),
            channel_id: channel_id.to_string(),
        };
        store.ensure_layout()?;
        Ok(store)
    }

    pub(crate) fn parse_channel_ref(raw: &str) -> Result<ChannelRef> {
        let trimmed = raw.trim();
        let (transport, channel_id) = trimmed
            .split_once('/')
            .ok_or_else(|| anyhow!("invalid channel ref '{raw}', expected transport/channel_id"))?;
        let transport = transport.trim();
        let channel_id = channel_id.trim();
        if transport.is_empty() || channel_id.is_empty() {
            bail!("invalid channel ref '{raw}', expected transport/channel_id");
        }
        Ok(ChannelRef {
            transport: transport.to_string(),
            channel_id: channel_id.to_string(),
        })
    }

    pub(crate) fn channel_dir(&self) -> PathBuf {
        self.base_dir
            .join("channels")
            .join(sanitize_for_path(&self.transport))
            .join(sanitize_for_path(&self.channel_id))
    }

    pub(crate) fn session_path(&self) -> PathBuf {
        self.channel_dir().join("session.jsonl")
    }

    pub(crate) fn log_path(&self) -> PathBuf {
        self.channel_dir().join("log.jsonl")
    }

    pub(crate) fn context_path(&self) -> PathBuf {
        self.channel_dir().join("context.jsonl")
    }

    pub(crate) fn memory_path(&self) -> PathBuf {
        self.channel_dir().join("MEMORY.md")
    }

    pub(crate) fn attachments_dir(&self) -> PathBuf {
        self.channel_dir().join("attachments")
    }

    pub(crate) fn artifacts_dir(&self) -> PathBuf {
        self.channel_dir().join("artifacts")
    }

    pub(crate) fn artifact_index_path(&self) -> PathBuf {
        self.artifacts_dir().join("index.jsonl")
    }

    pub(crate) fn append_log_entry(&self, entry: &ChannelLogEntry) -> Result<()> {
        append_jsonl_line(&self.log_path(), entry)
    }

    pub(crate) fn append_context_entry(&self, entry: &ChannelContextEntry) -> Result<()> {
        append_jsonl_line(&self.context_path(), entry)
    }

    pub(crate) fn load_log_entries(&self) -> Result<Vec<ChannelLogEntry>> {
        read_jsonl_records(&self.log_path())
    }

    pub(crate) fn load_context_entries(&self) -> Result<Vec<ChannelContextEntry>> {
        read_jsonl_records(&self.context_path())
    }

    pub(crate) fn load_memory(&self) -> Result<Option<String>> {
        let path = self.memory_path();
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if raw.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(raw))
    }

    pub(crate) fn write_memory(&self, content: &str) -> Result<()> {
        let mut payload = content.to_string();
        if !payload.ends_with('\n') {
            payload.push('\n');
        }
        write_text_atomic(&self.memory_path(), &payload)
            .with_context(|| format!("failed to write {}", self.memory_path().display()))
    }

    pub(crate) fn sync_context_from_messages(&self, messages: &[Message]) -> Result<usize> {
        let mut entries = Vec::new();
        for message in messages {
            let text = message.text_content();
            if text.trim().is_empty() {
                continue;
            }
            entries.push(ChannelContextEntry {
                timestamp_unix_ms: current_unix_timestamp_ms(),
                role: format!("{:?}", message.role).to_lowercase(),
                text,
            });
        }
        write_jsonl_records(&self.context_path(), &entries)?;
        Ok(entries.len())
    }

    pub(crate) fn compact_context(&self, max_records: usize) -> Result<usize> {
        let max_records = max_records.max(1);
        let entries = self.load_context_entries()?;
        if entries.len() <= max_records {
            return Ok(entries.len());
        }
        let keep_from = entries.len() - max_records;
        let compacted = entries[keep_from..].to_vec();
        write_jsonl_records(&self.context_path(), &compacted)?;
        Ok(compacted.len())
    }

    pub(crate) fn inspect(&self) -> Result<ChannelInspectReport> {
        let (log_records, invalid_log_lines) = inspect_jsonl_file(&self.log_path())?;
        let (context_records, invalid_context_lines) = inspect_jsonl_file(&self.context_path())?;
        let now_unix_ms = current_unix_timestamp_ms();
        let artifact_loaded = self.load_artifact_records_tolerant()?;
        let artifact_records = artifact_loaded.records.len();
        let expired_artifacts = artifact_loaded
            .records
            .iter()
            .filter(|record| is_artifact_expired(record, now_unix_ms))
            .count();
        let active_artifacts = artifact_records.saturating_sub(expired_artifacts);
        let memory_path = self.memory_path();
        let memory_exists = memory_path.exists();
        let memory_bytes = if memory_exists {
            std::fs::metadata(&memory_path)
                .with_context(|| format!("failed to stat {}", memory_path.display()))?
                .len()
        } else {
            0
        };

        Ok(ChannelInspectReport {
            transport: self.transport.clone(),
            channel_id: self.channel_id.clone(),
            channel_dir: self.channel_dir(),
            log_records,
            context_records,
            invalid_log_lines,
            invalid_context_lines,
            artifact_records,
            invalid_artifact_lines: artifact_loaded.invalid_lines,
            active_artifacts,
            expired_artifacts,
            memory_exists,
            memory_bytes,
        })
    }

    pub(crate) fn repair(&self) -> Result<ChannelRepairReport> {
        let (log_removed, log_backup_path) = repair_jsonl_file(&self.log_path())?;
        let (context_removed, context_backup_path) = repair_jsonl_file(&self.context_path())?;
        let artifact_purge = self.purge_expired_artifacts(current_unix_timestamp_ms())?;
        Ok(ChannelRepairReport {
            log_removed_lines: log_removed,
            context_removed_lines: context_removed,
            artifact_expired_removed: artifact_purge.expired_removed,
            artifact_invalid_removed: artifact_purge.invalid_removed,
            log_backup_path,
            context_backup_path,
        })
    }

    pub(crate) fn write_text_artifact(
        &self,
        run_id: &str,
        artifact_type: &str,
        visibility: &str,
        retention_days: Option<u64>,
        extension: &str,
        content: &str,
    ) -> Result<ChannelArtifactRecord> {
        let run_id = run_id.trim();
        let artifact_type = artifact_type.trim();
        let visibility = visibility.trim();
        if run_id.is_empty() || artifact_type.is_empty() || visibility.is_empty() {
            bail!("artifact run_id, artifact_type, and visibility must be non-empty");
        }
        let extension = extension.trim().trim_start_matches('.');
        if extension.is_empty() {
            bail!("artifact extension must be non-empty");
        }

        let created_unix_ms = current_unix_timestamp_ms();
        let hash_seed = format!(
            "{run_id}:{artifact_type}:{created_unix_ms}:{}",
            self.channel_id
        );
        let artifact_id = format!(
            "artifact-{}-{}",
            created_unix_ms,
            short_hash(hash_seed.as_bytes())
        );
        let run_key = sanitize_for_path(run_id);
        let type_key = sanitize_for_path(artifact_type);
        let ext_key = sanitize_for_path(extension);
        let relative_path = format!("artifacts/{run_key}/{type_key}-{artifact_id}.{ext_key}");
        let absolute_path = resolve_safe_channel_path(&self.channel_dir(), &relative_path)?;
        if let Some(parent) = absolute_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }

        let mut payload = content.to_string();
        if !payload.ends_with('\n') {
            payload.push('\n');
        }
        write_text_atomic(&absolute_path, &payload)
            .with_context(|| format!("failed to write {}", absolute_path.display()))?;
        let checksum_sha256 = sha256_hex(payload.as_bytes());
        let bytes = payload.len() as u64;
        let expires_unix_ms = retention_days
            .map(|days| days.saturating_mul(86_400_000))
            .map(|ttl| created_unix_ms.saturating_add(ttl));
        let record = ChannelArtifactRecord {
            id: artifact_id,
            run_id: run_id.to_string(),
            artifact_type: artifact_type.to_string(),
            visibility: visibility.to_string(),
            relative_path,
            bytes,
            checksum_sha256,
            created_unix_ms,
            expires_unix_ms,
        };
        append_jsonl_line(&self.artifact_index_path(), &record)?;
        Ok(record)
    }

    pub(crate) fn load_artifact_records(&self) -> Result<Vec<ChannelArtifactRecord>> {
        let path = self.artifact_index_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_jsonl_records(&path)
    }

    pub(crate) fn load_artifact_records_tolerant(&self) -> Result<ChannelArtifactLoadReport> {
        let path = self.artifact_index_path();
        if !path.exists() {
            return Ok(ChannelArtifactLoadReport::default());
        }
        let file = std::fs::File::open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        let reader = std::io::BufReader::new(file);

        let mut report = ChannelArtifactLoadReport::default();
        for line_result in reader.lines() {
            let line = line_result.with_context(|| format!("failed reading {}", path.display()))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<ChannelArtifactRecord>(trimmed) {
                Ok(record) => report.records.push(record),
                Err(_) => report.invalid_lines = report.invalid_lines.saturating_add(1),
            }
        }
        Ok(report)
    }

    pub(crate) fn list_active_artifacts(
        &self,
        now_unix_ms: u64,
    ) -> Result<Vec<ChannelArtifactRecord>> {
        let mut records = self
            .load_artifact_records_tolerant()?
            .records
            .into_iter()
            .filter(|record| !is_artifact_expired(record, now_unix_ms))
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(records)
    }

    pub(crate) fn purge_expired_artifacts(
        &self,
        now_unix_ms: u64,
    ) -> Result<ChannelArtifactPurgeReport> {
        let loaded = self.load_artifact_records_tolerant()?;
        let mut keep = Vec::new();
        let mut expired_removed = 0_usize;
        for record in loaded.records {
            if is_artifact_expired(&record, now_unix_ms) {
                let artifact_path =
                    resolve_safe_channel_path(&self.channel_dir(), &record.relative_path)?;
                if artifact_path.exists() {
                    std::fs::remove_file(&artifact_path)
                        .with_context(|| format!("failed to remove {}", artifact_path.display()))?;
                }
                expired_removed = expired_removed.saturating_add(1);
            } else {
                keep.push(record);
            }
        }

        write_jsonl_records(&self.artifact_index_path(), &keep)?;
        Ok(ChannelArtifactPurgeReport {
            expired_removed,
            invalid_removed: loaded.invalid_lines,
        })
    }

    fn ensure_layout(&self) -> Result<()> {
        let channel_dir = self.channel_dir();
        std::fs::create_dir_all(&channel_dir)
            .with_context(|| format!("failed to create {}", channel_dir.display()))?;
        std::fs::create_dir_all(self.attachments_dir())
            .with_context(|| format!("failed to create {}", self.attachments_dir().display()))?;
        std::fs::create_dir_all(self.artifacts_dir())
            .with_context(|| format!("failed to create {}", self.artifacts_dir().display()))?;

        for path in [
            self.log_path(),
            self.context_path(),
            self.artifact_index_path(),
        ] {
            if !path.exists() {
                std::fs::write(&path, "")
                    .with_context(|| format!("failed to initialize {}", path.display()))?;
            }
        }

        let meta_path = channel_dir.join("schema.json");
        if meta_path.exists() {
            let raw = std::fs::read_to_string(&meta_path)
                .with_context(|| format!("failed to read {}", meta_path.display()))?;
            let meta = serde_json::from_str::<ChannelStoreMeta>(&raw)
                .with_context(|| format!("failed to parse {}", meta_path.display()))?;
            if meta.schema_version != CHANNEL_STORE_SCHEMA_VERSION {
                bail!(
                    "unsupported channel store schema: expected {}, found {}",
                    CHANNEL_STORE_SCHEMA_VERSION,
                    meta.schema_version
                );
            }
            if meta.transport != self.transport || meta.channel_id != self.channel_id {
                bail!(
                    "channel store schema mismatch for {}",
                    channel_dir.display()
                );
            }
            return Ok(());
        }

        let mut payload = serde_json::to_string_pretty(&ChannelStoreMeta {
            schema_version: CHANNEL_STORE_SCHEMA_VERSION,
            transport: self.transport.clone(),
            channel_id: self.channel_id.clone(),
        })
        .context("failed to encode channel store schema")?;
        payload.push('\n');
        write_text_atomic(&meta_path, &payload)
            .with_context(|| format!("failed to write {}", meta_path.display()))
    }
}

fn append_jsonl_line<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let line = serde_json::to_string(value).context("failed to encode jsonl record")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append to {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn write_jsonl_records<T>(path: &Path, entries: &[T]) -> Result<()>
where
    T: Serialize,
{
    let mut payload = String::new();
    for entry in entries {
        payload.push_str(&serde_json::to_string(entry).context("failed to encode jsonl record")?);
        payload.push('\n');
    }
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[allow(dead_code)]
fn read_jsonl_records<T>(path: &Path) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut rows = Vec::new();
    for (index, line_result) in reader.lines().enumerate() {
        let line_no = index + 1;
        let line = line_result
            .with_context(|| format!("failed reading line {} from {}", line_no, path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<T>(trimmed).with_context(|| {
            format!("failed parsing JSON line {} in {}", line_no, path.display())
        })?;
        rows.push(parsed);
    }
    Ok(rows)
}

fn inspect_jsonl_file(path: &Path) -> Result<(usize, usize)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut valid = 0_usize;
    let mut invalid = 0_usize;
    for line_result in reader.lines() {
        let line = line_result.with_context(|| format!("failed reading {}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if serde_json::from_str::<Value>(trimmed).is_ok() {
            valid = valid.saturating_add(1);
        } else {
            invalid = invalid.saturating_add(1);
        }
    }
    Ok((valid, invalid))
}

fn repair_jsonl_file(path: &Path) -> Result<(usize, Option<PathBuf>)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut valid_lines = Vec::new();
    let mut invalid_lines = Vec::new();
    for line_result in reader.lines() {
        let line = line_result.with_context(|| format!("failed reading {}", path.display()))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if serde_json::from_str::<Value>(trimmed).is_ok() {
            valid_lines.push(trimmed.to_string());
        } else {
            invalid_lines.push(line);
        }
    }

    if invalid_lines.is_empty() {
        return Ok((0, None));
    }

    let backup_path = path.with_extension(format!("{}.corrupt", current_unix_timestamp_ms()));
    let mut backup_content = String::new();
    for line in &invalid_lines {
        backup_content.push_str(line);
        backup_content.push('\n');
    }
    write_text_atomic(&backup_path, &backup_content)
        .with_context(|| format!("failed to write {}", backup_path.display()))?;

    let mut repaired_content = String::new();
    for line in &valid_lines {
        repaired_content.push_str(line);
        repaired_content.push('\n');
    }
    write_text_atomic(path, &repaired_content)
        .with_context(|| format!("failed to write repaired {}", path.display()))?;

    Ok((invalid_lines.len(), Some(backup_path)))
}

fn sanitize_for_path(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "channel".to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_safe_channel_path(channel_dir: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        bail!("artifact relative path must be non-absolute");
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("artifact relative path must not traverse parent directories");
    }
    Ok(channel_dir.join(relative))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn short_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3]
    )
}

fn is_artifact_expired(record: &ChannelArtifactRecord, now_unix_ms: u64) -> bool {
    record
        .expires_unix_ms
        .map(|expiry| expiry <= now_unix_ms)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        sanitize_for_path, ChannelContextEntry, ChannelLogEntry, ChannelStore, ChannelStoreMeta,
        CHANNEL_STORE_SCHEMA_VERSION,
    };
    use pi_ai::Message;

    #[test]
    fn unit_parse_channel_ref_and_path_resolution_are_stable() {
        let parsed =
            ChannelStore::parse_channel_ref("github/owner/repo#1").expect("parse channel ref");
        assert_eq!(parsed.transport, "github");
        assert_eq!(parsed.channel_id, "owner/repo#1");

        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "owner/repo#1").expect("open store");
        let dir = store.channel_dir();
        assert!(dir.ends_with("channels/github/owner_repo_1"));
        assert!(store.session_path().ends_with("session.jsonl"));
    }

    #[test]
    fn functional_append_load_and_memory_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "slack", "C123").expect("open store");

        store
            .append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms: 10,
                direction: "inbound".to_string(),
                event_key: Some("e1".to_string()),
                source: "slack".to_string(),
                payload: json!({"text": "hello"}),
            })
            .expect("append log");
        store
            .append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms: 11,
                role: "user".to_string(),
                text: "hello".to_string(),
            })
            .expect("append context");
        store
            .write_memory("Remember this channel preference")
            .expect("write memory");

        let logs = store.load_log_entries().expect("load logs");
        assert_eq!(logs.len(), 1);
        let context = store.load_context_entries().expect("load context");
        assert_eq!(context.len(), 1);
        let memory = store.load_memory().expect("load memory");
        assert!(memory
            .expect("memory should exist")
            .contains("channel preference"));
    }

    #[test]
    fn functional_sync_and_compact_context_from_messages() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "issue-9").expect("open store");
        let messages = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant_text("a1"),
            Message::user("u2"),
            Message::assistant_text("a2"),
        ];

        let count = store
            .sync_context_from_messages(&messages)
            .expect("sync context");
        assert_eq!(count, 5);
        let compacted = store.compact_context(3).expect("compact context");
        assert_eq!(compacted, 3);
        let entries = store.load_context_entries().expect("load entries");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "a1");
    }

    #[test]
    fn functional_artifact_roundtrip_create_and_list_active_records() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "issue-44").expect("open store");

        let artifact = store
            .write_text_artifact(
                "run-1",
                "github-reply",
                "private",
                Some(7),
                "md",
                "# reply\nhello",
            )
            .expect("write artifact");
        assert_eq!(artifact.run_id, "run-1");
        assert!(artifact.relative_path.starts_with("artifacts/run-1/"));
        assert!(artifact.checksum_sha256.len() >= 64);
        assert!(store.channel_dir().join(&artifact.relative_path).exists());

        let strict = store.load_artifact_records().expect("load strict records");
        assert_eq!(strict.len(), 1);
        assert_eq!(strict[0].id, artifact.id);

        let active = store
            .list_active_artifacts(artifact.created_unix_ms)
            .expect("list active");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, artifact.id);
    }

    #[test]
    fn integration_inspect_and_repair_detects_corrupted_jsonl() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "slack", "D111").expect("open store");

        let mut raw = String::new();
        raw.push_str("{\"ok\":true}\n");
        raw.push_str("this is bad json\n");
        std::fs::write(store.log_path(), raw).expect("seed invalid log");

        let report = store.inspect().expect("inspect");
        assert_eq!(report.log_records, 1);
        assert_eq!(report.invalid_log_lines, 1);
        assert_eq!(report.artifact_records, 0);
        assert_eq!(report.invalid_artifact_lines, 0);

        let repaired = store.repair().expect("repair");
        assert_eq!(repaired.log_removed_lines, 1);
        assert_eq!(repaired.artifact_expired_removed, 0);
        assert_eq!(repaired.artifact_invalid_removed, 0);
        assert!(repaired.log_backup_path.is_some());

        let repaired_report = store.inspect().expect("inspect after repair");
        assert_eq!(repaired_report.invalid_log_lines, 0);
        assert_eq!(repaired_report.log_records, 1);
    }

    #[test]
    fn regression_parse_ref_and_schema_mismatch_are_rejected() {
        let error = ChannelStore::parse_channel_ref("missing-separator")
            .expect_err("invalid ref should fail");
        assert!(error.to_string().contains("transport/channel_id"));

        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "issue-1").expect("open store");
        let schema_path = store.channel_dir().join("schema.json");
        let mut payload = serde_json::to_string_pretty(&ChannelStoreMeta {
            schema_version: CHANNEL_STORE_SCHEMA_VERSION + 1,
            transport: "github".to_string(),
            channel_id: "issue-1".to_string(),
        })
        .expect("serialize schema");
        payload.push('\n');
        std::fs::write(&schema_path, payload).expect("overwrite schema");

        let error = ChannelStore::open(temp.path(), "github", "issue-1")
            .expect_err("schema mismatch should fail");
        assert!(error
            .to_string()
            .contains("unsupported channel store schema"));
    }

    #[test]
    fn regression_sanitize_for_path_normalizes_special_characters() {
        assert_eq!(sanitize_for_path("owner/repo#1"), "owner_repo_1");
        assert_eq!(sanitize_for_path("***"), "channel");
        assert_eq!(sanitize_for_path("good.name"), "good.name");
    }

    #[test]
    fn integration_transport_shared_layout_paths_are_deterministic() {
        let temp = tempdir().expect("tempdir");
        let github =
            ChannelStore::open(temp.path(), "github", "owner/repo#9").expect("open github");
        let slack = ChannelStore::open(temp.path(), "slack", "C123").expect("open slack");

        assert_ne!(github.channel_dir(), slack.channel_dir());
        assert!(github.attachments_dir().ends_with(Path::new("attachments")));
        assert!(slack.artifacts_dir().ends_with(Path::new("artifacts")));
    }

    #[test]
    fn integration_reopen_preserves_channel_records_across_restarts() {
        let temp = tempdir().expect("tempdir");
        {
            let store = ChannelStore::open(temp.path(), "github", "issue-21").expect("open");
            store
                .append_log_entry(&ChannelLogEntry {
                    timestamp_unix_ms: 1,
                    direction: "inbound".to_string(),
                    event_key: Some("e1".to_string()),
                    source: "github".to_string(),
                    payload: json!({"body":"first"}),
                })
                .expect("append log");
            store
                .sync_context_from_messages(&[Message::assistant_text("persist me")])
                .expect("sync context");
        }

        let reopened = ChannelStore::open(temp.path(), "github", "issue-21").expect("reopen");
        let inspect = reopened.inspect().expect("inspect reopened");
        assert_eq!(inspect.log_records, 1);
        assert_eq!(inspect.context_records, 1);
    }

    #[test]
    fn integration_purge_expired_artifacts_rewrites_index_and_removes_files() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "issue-52").expect("open store");

        let expired = store
            .write_text_artifact(
                "run-expired",
                "github-reply",
                "private",
                Some(0),
                "md",
                "expired body",
            )
            .expect("write expired");
        let retained = store
            .write_text_artifact(
                "run-active",
                "github-reply",
                "private",
                Some(30),
                "md",
                "active body",
            )
            .expect("write retained");

        let purge = store
            .purge_expired_artifacts(expired.created_unix_ms.saturating_add(1))
            .expect("purge");
        assert_eq!(purge.expired_removed, 1);
        assert_eq!(purge.invalid_removed, 0);
        assert!(!store.channel_dir().join(&expired.relative_path).exists());
        assert!(store.channel_dir().join(&retained.relative_path).exists());

        let active = store
            .list_active_artifacts(expired.created_unix_ms.saturating_add(1))
            .expect("active records");
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, retained.id);
    }

    #[test]
    fn integration_inspect_and_repair_report_artifact_health_counters() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "issue-72").expect("open store");
        let expired = store
            .write_text_artifact(
                "run-expired",
                "github-reply",
                "private",
                Some(0),
                "md",
                "expired",
            )
            .expect("write expired artifact");
        store
            .write_text_artifact(
                "run-active",
                "github-reply",
                "private",
                Some(30),
                "md",
                "active",
            )
            .expect("write active artifact");
        let mut seeded = std::fs::read_to_string(store.artifact_index_path()).expect("read index");
        seeded.push_str("invalid-json-line\n");
        std::fs::write(store.artifact_index_path(), seeded).expect("seed invalid artifact line");

        let inspect_before = store.inspect().expect("inspect before repair");
        assert_eq!(inspect_before.artifact_records, 2);
        assert_eq!(inspect_before.invalid_artifact_lines, 1);
        assert_eq!(inspect_before.expired_artifacts, 1);
        assert_eq!(inspect_before.active_artifacts, 1);

        let repaired = store.repair().expect("repair");
        assert_eq!(repaired.artifact_expired_removed, 1);
        assert_eq!(repaired.artifact_invalid_removed, 1);
        assert!(!store.channel_dir().join(expired.relative_path).exists());

        let inspect_after = store.inspect().expect("inspect after repair");
        assert_eq!(inspect_after.artifact_records, 1);
        assert_eq!(inspect_after.invalid_artifact_lines, 0);
        assert_eq!(inspect_after.expired_artifacts, 0);
        assert_eq!(inspect_after.active_artifacts, 1);
    }

    #[test]
    fn regression_open_creates_schema_for_legacy_layout_without_schema_file() {
        let temp = tempdir().expect("tempdir");
        let legacy_dir = temp.path().join("channels/github/legacy_issue");
        std::fs::create_dir_all(&legacy_dir).expect("create legacy dir");
        std::fs::write(legacy_dir.join("log.jsonl"), "{\"ok\":true}\n").expect("write log");
        std::fs::write(legacy_dir.join("context.jsonl"), "").expect("write context");
        std::fs::create_dir_all(legacy_dir.join("attachments")).expect("create attachments");
        std::fs::create_dir_all(legacy_dir.join("artifacts")).expect("create artifacts");

        let store =
            ChannelStore::open(temp.path(), "github", "legacy_issue").expect("open legacy layout");
        let schema = std::fs::read_to_string(store.channel_dir().join("schema.json"))
            .expect("schema created");
        assert!(schema.contains("\"schema_version\""));
        let inspect = store.inspect().expect("inspect legacy");
        assert_eq!(inspect.log_records, 1);
        assert_eq!(inspect.invalid_log_lines, 0);
    }

    #[test]
    fn regression_artifact_tolerant_loader_skips_invalid_lines_without_failure() {
        let temp = tempdir().expect("tempdir");
        let store = ChannelStore::open(temp.path(), "github", "issue-61").expect("open store");
        let artifact = store
            .write_text_artifact("run-1", "github-reply", "private", Some(7), "md", "ok")
            .expect("write valid artifact");
        let mut seeded = std::fs::read_to_string(store.artifact_index_path()).expect("read index");
        seeded.push_str("not-json\n");
        std::fs::write(store.artifact_index_path(), seeded).expect("seed invalid line");

        let loaded = store
            .load_artifact_records_tolerant()
            .expect("tolerant load should pass");
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(loaded.records[0].id, artifact.id);
        assert_eq!(loaded.invalid_lines, 1);
    }
}
