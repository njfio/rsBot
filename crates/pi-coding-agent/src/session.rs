use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use pi_ai::Message;
use serde::{Deserialize, Serialize};

const SESSION_SCHEMA_VERSION: u32 = 1;
const DEFAULT_LOCK_WAIT_MS: u64 = 5_000;
const DEFAULT_LOCK_STALE_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionMetaRecord {
    schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "snake_case")]
enum SessionRecord {
    Meta(SessionMetaRecord),
    Entry(SessionEntry),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub removed_duplicates: usize,
    pub duplicate_ids: Vec<u64>,
    pub removed_invalid_parent: usize,
    pub invalid_parent_ids: Vec<u64>,
    pub removed_cycles: usize,
    pub cycle_ids: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactReport {
    pub removed_entries: usize,
    pub retained_entries: usize,
    pub head_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionValidationReport {
    pub entries: usize,
    pub duplicates: usize,
    pub invalid_parent: usize,
    pub cycles: usize,
}

impl SessionValidationReport {
    pub fn is_valid(&self) -> bool {
        self.duplicates == 0 && self.invalid_parent == 0 && self.cycles == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub imported_entries: usize,
    pub remapped_entries: usize,
    pub remapped_ids: Vec<(u64, u64)>,
    pub replaced_entries: usize,
    pub resulting_entries: usize,
    pub active_head: Option<u64>,
}

#[derive(Debug)]
pub struct SessionStore {
    path: PathBuf,
    entries: Vec<SessionEntry>,
    next_id: u64,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
}

impl SessionStore {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let entries = read_session_entries(&path)?;
        let next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        Ok(Self {
            path,
            entries,
            next_id,
            lock_wait_ms: DEFAULT_LOCK_WAIT_MS,
            lock_stale_ms: DEFAULT_LOCK_STALE_MS,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn entries(&self) -> &[SessionEntry] {
        &self.entries
    }

    pub fn head_id(&self) -> Option<u64> {
        self.entries.last().map(|entry| entry.id)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }

    pub fn set_lock_policy(&mut self, lock_wait_ms: u64, lock_stale_ms: u64) {
        self.lock_wait_ms = lock_wait_ms.max(1);
        self.lock_stale_ms = lock_stale_ms;
    }

    pub fn ensure_initialized(&mut self, system_prompt: &str) -> Result<Option<u64>> {
        if !self.entries.is_empty() {
            return Ok(self.head_id());
        }

        if system_prompt.trim().is_empty() {
            return Ok(None);
        }

        let system_message = Message::system(system_prompt);
        self.append_messages(None, &[system_message])
    }

    pub fn append_messages(
        &mut self,
        mut parent_id: Option<u64>,
        messages: &[Message],
    ) -> Result<Option<u64>> {
        if messages.is_empty() {
            return Ok(parent_id);
        }

        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let mut entries = read_session_entries(&self.path)?;
        let mut next_id = entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        if let Some(parent) = parent_id {
            if !entries.iter().any(|entry| entry.id == parent) {
                bail!("parent id {parent} does not exist in session");
            }
        }

        for message in messages {
            let entry = SessionEntry {
                id: next_id,
                parent_id,
                message: message.clone(),
            };
            next_id += 1;
            parent_id = Some(entry.id);
            entries.push(entry);
        }

        write_session_entries_atomic(&self.path, &entries)?;
        self.entries = entries;
        self.next_id = next_id;

        Ok(parent_id)
    }

    pub fn lineage_messages(&self, head_id: Option<u64>) -> Result<Vec<Message>> {
        Ok(self
            .lineage_entries(head_id)?
            .into_iter()
            .map(|entry| entry.message)
            .collect())
    }

    pub fn lineage_entries(&self, head_id: Option<u64>) -> Result<Vec<SessionEntry>> {
        let Some(mut current_id) = head_id else {
            return Ok(Vec::new());
        };

        let mut lineage = Vec::new();
        let mut visited = HashSet::new();

        loop {
            if !visited.insert(current_id) {
                bail!("detected a cycle while resolving session lineage at id {current_id}");
            }

            let entry = self
                .entries
                .iter()
                .find(|entry| entry.id == current_id)
                .ok_or_else(|| anyhow!("unknown session id {current_id}"))?;

            lineage.push(entry.clone());
            match entry.parent_id {
                Some(parent) => current_id = parent,
                None => break,
            }
        }
        lineage.reverse();
        Ok(lineage)
    }

    pub fn export_lineage(
        &self,
        head_id: Option<u64>,
        destination: impl AsRef<Path>,
    ) -> Result<usize> {
        let lineage = self.lineage_entries(head_id)?;
        write_session_entries_atomic(destination.as_ref(), &lineage)?;
        Ok(lineage.len())
    }

    pub fn export_lineage_jsonl(&self, head_id: Option<u64>) -> Result<String> {
        let lineage = self.lineage_entries(head_id)?;
        let mut lines = Vec::with_capacity(lineage.len() + 1);
        let meta = SessionRecord::Meta(SessionMetaRecord {
            schema_version: SESSION_SCHEMA_VERSION,
        });
        lines.push(serde_json::to_string(&meta)?);
        for entry in lineage {
            lines.push(serde_json::to_string(&SessionRecord::Entry(entry))?);
        }
        Ok(lines.join("\n"))
    }

    pub fn import_snapshot(
        &mut self,
        source: impl AsRef<Path>,
        mode: SessionImportMode,
    ) -> Result<ImportReport> {
        let source_path = source.as_ref();
        let imported_entries = read_session_entries(source_path)?;
        let source_report = validation_report_for_entries(&imported_entries);
        if !source_report.is_valid() {
            bail!(
                "import session validation failed: path={} entries={} duplicates={} invalid_parent={} cycles={}",
                source_path.display(),
                source_report.entries,
                source_report.duplicates,
                source_report.invalid_parent,
                source_report.cycles
            );
        }

        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let existing_entries = read_session_entries(&self.path)?;
        let report = match mode {
            SessionImportMode::Merge => {
                let (merged_entries, remapped_ids, active_head) =
                    merge_entries_with_remap(&existing_entries, &imported_entries)?;
                write_session_entries_atomic(&self.path, &merged_entries)?;
                self.entries = merged_entries;
                ImportReport {
                    imported_entries: imported_entries.len(),
                    remapped_entries: remapped_ids.len(),
                    remapped_ids,
                    replaced_entries: 0,
                    resulting_entries: self.entries.len(),
                    active_head,
                }
            }
            SessionImportMode::Replace => {
                write_session_entries_atomic(&self.path, &imported_entries)?;
                self.entries = imported_entries;
                ImportReport {
                    imported_entries: self.entries.len(),
                    remapped_entries: 0,
                    remapped_ids: Vec::new(),
                    replaced_entries: existing_entries.len(),
                    resulting_entries: self.entries.len(),
                    active_head: self.entries.last().map(|entry| entry.id),
                }
            }
        };

        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;
        Ok(report)
    }

    pub fn validation_report(&self) -> SessionValidationReport {
        validation_report_for_entries(&self.entries)
    }

    pub fn branch_tips(&self) -> Vec<&SessionEntry> {
        let mut parent_ids = HashSet::new();
        for entry in &self.entries {
            if let Some(parent_id) = entry.parent_id {
                parent_ids.insert(parent_id);
            }
        }

        let mut tips = self
            .entries
            .iter()
            .filter(|entry| !parent_ids.contains(&entry.id))
            .collect::<Vec<_>>();
        tips.sort_by_key(|entry| entry.id);
        tips
    }

    pub fn repair(&mut self) -> Result<RepairReport> {
        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let mut entries = read_session_entries(&self.path)?;
        entries.sort_by_key(|entry| entry.id);

        let mut report = RepairReport::default();
        let mut unique = Vec::new();
        let mut seen = HashSet::new();
        for entry in entries {
            if seen.insert(entry.id) {
                unique.push(entry);
            } else {
                report.removed_duplicates += 1;
                report.duplicate_ids.push(entry.id);
            }
        }

        let mut id_to_entry: HashMap<u64, SessionEntry> = unique
            .iter()
            .cloned()
            .map(|entry| (entry.id, entry))
            .collect();

        // Remove entries with missing parents.
        loop {
            let before = id_to_entry.len();
            let invalid_ids = id_to_entry
                .values()
                .filter_map(|entry| match entry.parent_id {
                    Some(parent_id) if !id_to_entry.contains_key(&parent_id) => Some(entry.id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut invalid_ids = invalid_ids;
            invalid_ids.sort_unstable();
            invalid_ids.dedup();

            if invalid_ids.is_empty() {
                break;
            }

            for id in invalid_ids {
                if id_to_entry.remove(&id).is_some() {
                    report.removed_invalid_parent += 1;
                    report.invalid_parent_ids.push(id);
                }
            }

            if id_to_entry.len() == before {
                break;
            }
        }

        // Remove cycles by walking each entry lineage.
        let cycle_ids = id_to_entry
            .keys()
            .copied()
            .filter(|id| has_cycle(*id, &id_to_entry))
            .collect::<Vec<_>>();
        let mut cycle_ids = cycle_ids;
        cycle_ids.sort_unstable();
        cycle_ids.dedup();
        for id in cycle_ids {
            if id_to_entry.remove(&id).is_some() {
                report.removed_cycles += 1;
                report.cycle_ids.push(id);
            }
        }

        let mut repaired = id_to_entry.into_values().collect::<Vec<_>>();
        repaired.sort_by_key(|entry| entry.id);

        write_session_entries_atomic(&self.path, &repaired)?;
        self.entries = repaired;
        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        Ok(report)
    }

    pub fn compact_to_lineage(&mut self, preferred_head_id: Option<u64>) -> Result<CompactReport> {
        let lock_path = self.lock_path();
        let _lock = acquire_lock(
            &lock_path,
            Duration::from_millis(self.lock_wait_ms),
            Duration::from_millis(self.lock_stale_ms),
        )?;

        let entries = read_session_entries(&self.path)?;
        if entries.is_empty() {
            self.entries = entries;
            self.next_id = 1;
            return Ok(CompactReport {
                removed_entries: 0,
                retained_entries: 0,
                head_id: None,
            });
        }

        let head_id = preferred_head_id.or_else(|| entries.last().map(|entry| entry.id));
        let Some(head_id) = head_id else {
            self.entries = entries;
            self.next_id = 1;
            return Ok(CompactReport {
                removed_entries: 0,
                retained_entries: 0,
                head_id: None,
            });
        };

        let lineage_ids = collect_lineage_ids(&entries, head_id)?;
        let compacted = entries
            .iter()
            .filter(|entry| lineage_ids.contains(&entry.id))
            .cloned()
            .collect::<Vec<_>>();

        let removed_entries = entries.len().saturating_sub(compacted.len());
        write_session_entries_atomic(&self.path, &compacted)?;
        self.entries = compacted;
        self.next_id = self.entries.iter().map(|entry| entry.id).max().unwrap_or(0) + 1;

        Ok(CompactReport {
            removed_entries,
            retained_entries: self.entries.len(),
            head_id: Some(head_id),
        })
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }
}

fn has_cycle(start_id: u64, entries: &HashMap<u64, SessionEntry>) -> bool {
    let mut visited = HashSet::new();
    let mut current = Some(start_id);

    while let Some(id) = current {
        if !visited.insert(id) {
            return true;
        }

        current = entries.get(&id).and_then(|entry| entry.parent_id);
    }

    false
}

fn collect_lineage_ids(entries: &[SessionEntry], head_id: u64) -> Result<HashSet<u64>> {
    let id_to_entry = entries
        .iter()
        .cloned()
        .map(|entry| (entry.id, entry))
        .collect::<HashMap<_, _>>();
    if !id_to_entry.contains_key(&head_id) {
        bail!("unknown session id {head_id}");
    }

    let mut lineage_ids = HashSet::new();
    let mut visited = HashSet::new();
    let mut current_id = head_id;
    loop {
        if !visited.insert(current_id) {
            bail!("detected a cycle while compacting session lineage at id {current_id}");
        }

        let entry = id_to_entry
            .get(&current_id)
            .ok_or_else(|| anyhow!("unknown session id {current_id}"))?;
        lineage_ids.insert(entry.id);

        match entry.parent_id {
            Some(parent_id) => {
                if !id_to_entry.contains_key(&parent_id) {
                    bail!("missing parent id {parent_id} while compacting");
                }
                current_id = parent_id;
            }
            None => break,
        }
    }

    Ok(lineage_ids)
}

fn validation_report_for_entries(entries: &[SessionEntry]) -> SessionValidationReport {
    let mut report = SessionValidationReport {
        entries: entries.len(),
        ..SessionValidationReport::default()
    };

    let mut seen = HashSet::new();
    for entry in entries {
        if !seen.insert(entry.id) {
            report.duplicates += 1;
        }
    }

    let id_to_entry = entries
        .iter()
        .cloned()
        .map(|entry| (entry.id, entry))
        .collect::<HashMap<_, _>>();

    for entry in entries {
        if let Some(parent_id) = entry.parent_id {
            if !id_to_entry.contains_key(&parent_id) {
                report.invalid_parent += 1;
            }
        }
    }

    let mut cycle_ids = HashSet::new();
    for entry in entries {
        if has_cycle(entry.id, &id_to_entry) {
            cycle_ids.insert(entry.id);
        }
    }
    report.cycles = cycle_ids.len();

    report
}

type MergeImportResult = (Vec<SessionEntry>, Vec<(u64, u64)>, Option<u64>);

fn merge_entries_with_remap(
    existing_entries: &[SessionEntry],
    imported_entries: &[SessionEntry],
) -> Result<MergeImportResult> {
    let mut merged = existing_entries.to_vec();
    if imported_entries.is_empty() {
        let active_head = merged.last().map(|entry| entry.id);
        return Ok((merged, Vec::new(), active_head));
    }

    let mut used_ids = existing_entries
        .iter()
        .map(|entry| entry.id)
        .collect::<HashSet<_>>();
    let mut next_id = used_ids.iter().max().copied().unwrap_or(0) + 1;
    let mut remapped_ids = Vec::new();
    let mut id_map = HashMap::with_capacity(imported_entries.len());

    for entry in imported_entries {
        let mapped_id = if used_ids.contains(&entry.id) {
            let replacement = next_id;
            next_id += 1;
            remapped_ids.push((entry.id, replacement));
            replacement
        } else {
            entry.id
        };
        used_ids.insert(mapped_id);
        id_map.insert(entry.id, mapped_id);
    }

    for entry in imported_entries {
        let mapped_id = *id_map
            .get(&entry.id)
            .ok_or_else(|| anyhow!("missing remap id for {}", entry.id))?;
        let mapped_parent_id = entry
            .parent_id
            .map(|parent_id| {
                id_map
                    .get(&parent_id)
                    .copied()
                    .ok_or_else(|| anyhow!("missing remap parent id for {}", parent_id))
            })
            .transpose()?;
        merged.push(SessionEntry {
            id: mapped_id,
            parent_id: mapped_parent_id,
            message: entry.message.clone(),
        });
    }

    let active_head = imported_entries
        .last()
        .and_then(|entry| id_map.get(&entry.id).copied());

    Ok((merged, remapped_ids, active_head))
}

fn read_session_entries(path: &Path) -> Result<Vec<SessionEntry>> {
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

fn write_session_entries_atomic(path: &Path, entries: &[SessionEntry]) -> Result<()> {
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

struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_lock(path: &Path, timeout: Duration, stale_after: Duration) -> Result<LockGuard> {
    let start = SystemTime::now();

    loop {
        match OpenOptions::new().create_new(true).write(true).open(path) {
            Ok(mut file) => {
                let pid = std::process::id();
                let _ = writeln!(file, "{pid}");
                return Ok(LockGuard {
                    path: path.to_path_buf(),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if stale_after > Duration::ZERO && reclaim_stale_lock(path, stale_after) {
                    continue;
                }
                let elapsed = SystemTime::now().duration_since(start).unwrap_or_default();
                if elapsed >= timeout {
                    bail!("timed out acquiring lock {}", path.display());
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                return Err(anyhow!(
                    "failed to acquire lock {}: {error}",
                    path.display()
                ));
            }
        }
    }
}

fn reclaim_stale_lock(path: &Path, stale_after: Duration) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(_) => return false,
    };
    let age = match SystemTime::now().duration_since(modified) {
        Ok(age) => age,
        Err(_) => Duration::ZERO,
    };
    if age < stale_after {
        return false;
    }

    fs::remove_file(path).is_ok()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, path::PathBuf, sync::Arc, thread, time::Duration};

    use tempfile::tempdir;

    use super::{
        CompactReport, RepairReport, SessionEntry, SessionImportMode, SessionRecord, SessionStore,
        SessionValidationReport,
    };

    #[test]
    fn appends_and_restores_lineage() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("hello"),
                    pi_ai::Message::assistant_text("hi"),
                ],
            )
            .expect("append");

        let lineage = store.lineage_messages(head).expect("lineage");
        assert_eq!(lineage.len(), 3);
        assert_eq!(lineage[0].text_content(), "sys");

        let reloaded = SessionStore::load(&path).expect("reload");
        assert_eq!(reloaded.entries().len(), 3);
    }

    #[test]
    fn supports_branching_from_older_id() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("q1"),
                    pi_ai::Message::assistant_text("a1"),
                    pi_ai::Message::user("q2"),
                    pi_ai::Message::assistant_text("a2"),
                ],
            )
            .expect("append");

        let branch_from = Some(head.expect("head") - 2);
        let branch_head = store
            .append_messages(
                branch_from,
                &[
                    pi_ai::Message::user("q2b"),
                    pi_ai::Message::assistant_text("a2b"),
                ],
            )
            .expect("append");

        let lineage = store.lineage_messages(branch_head).expect("lineage");
        let texts = lineage
            .iter()
            .map(|message| message.text_content())
            .collect::<Vec<_>>();
        assert_eq!(texts, vec!["sys", "q1", "a1", "q2b", "a2b"]);
        assert_eq!(store.branch_tips().len(), 2);
    }

    #[test]
    fn functional_export_lineage_writes_schema_valid_snapshot() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        let export = temp.path().join("export.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("q1"),
                    pi_ai::Message::assistant_text("a1"),
                ],
            )
            .expect("append");

        let exported = store
            .export_lineage(head, &export)
            .expect("lineage export should succeed");
        assert_eq!(exported, 3);

        let snapshot = SessionStore::load(&export).expect("load export");
        assert_eq!(snapshot.entries().len(), 3);
        assert_eq!(snapshot.entries()[0].message.text_content(), "sys");
        assert_eq!(snapshot.entries()[1].message.text_content(), "q1");
        assert_eq!(snapshot.entries()[2].message.text_content(), "a1");
    }

    #[test]
    fn unit_export_lineage_jsonl_includes_meta_and_entries() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        store
            .append_messages(head, &[pi_ai::Message::user("q1")])
            .expect("append");

        let snapshot = store
            .export_lineage_jsonl(store.head_id())
            .expect("export jsonl");
        let lines = snapshot.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        let meta: SessionRecord = serde_json::from_str(lines[0]).expect("meta");
        assert!(matches!(meta, SessionRecord::Meta(_)));
        let entry: SessionRecord = serde_json::from_str(lines[1]).expect("entry");
        assert!(matches!(entry, SessionRecord::Entry(_)));
    }

    #[test]
    fn unit_import_snapshot_merge_remaps_colliding_ids() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("target.jsonl");
        let source = temp.path().join("source.jsonl");

        let mut target_store = SessionStore::load(&target).expect("load target");
        let target_head = target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target root");
        target_store
            .append_messages(target_head, &[pi_ai::Message::user("target-user")])
            .expect("append target user");

        let mut source_store = SessionStore::load(&source).expect("load source");
        let source_head = source_store
            .append_messages(None, &[pi_ai::Message::system("import-root")])
            .expect("append source root");
        source_store
            .append_messages(
                source_head,
                &[pi_ai::Message::assistant_text("import-assistant")],
            )
            .expect("append source assistant");

        let report = target_store
            .import_snapshot(&source, SessionImportMode::Merge)
            .expect("merge import");

        assert_eq!(report.imported_entries, 2);
        assert_eq!(report.remapped_entries, 2);
        assert_eq!(report.remapped_ids, vec![(1, 3), (2, 4)]);
        assert_eq!(report.replaced_entries, 0);
        assert_eq!(report.resulting_entries, 4);
        assert_eq!(report.active_head, Some(4));

        let entries = target_store.entries();
        assert_eq!(entries[2].id, 3);
        assert_eq!(entries[2].parent_id, None);
        assert_eq!(entries[2].message.text_content(), "import-root");
        assert_eq!(entries[3].id, 4);
        assert_eq!(entries[3].parent_id, Some(3));
        assert_eq!(entries[3].message.text_content(), "import-assistant");
    }

    #[test]
    fn integration_export_import_roundtrip_merge_produces_valid_session_graph() {
        let temp = tempdir().expect("tempdir");
        let source_path = temp.path().join("roundtrip-source.jsonl");
        let target_path = temp.path().join("roundtrip-target.jsonl");
        let export_path = temp.path().join("roundtrip-export.jsonl");

        let mut source_store = SessionStore::load(&source_path).expect("load source");
        let source_head = source_store
            .append_messages(None, &[pi_ai::Message::system("source-root")])
            .expect("append source root");
        let source_head = source_store
            .append_messages(
                source_head,
                &[
                    pi_ai::Message::user("source-user"),
                    pi_ai::Message::assistant_text("source-assistant"),
                ],
            )
            .expect("append source branch");
        source_store
            .export_lineage(source_head, &export_path)
            .expect("export source lineage");

        let mut target_store = SessionStore::load(&target_path).expect("load target");
        target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target");

        let report = target_store
            .import_snapshot(&export_path, SessionImportMode::Merge)
            .expect("import merge");
        assert_eq!(report.imported_entries, 3);
        assert_eq!(report.remapped_entries, 3);
        assert_eq!(report.remapped_ids, vec![(1, 2), (2, 3), (3, 4)]);
        assert_eq!(report.resulting_entries, 4);
        assert!(target_store.validation_report().is_valid());

        let lineage = target_store
            .lineage_messages(report.active_head)
            .expect("lineage from imported head");
        assert_eq!(
            lineage
                .iter()
                .map(|message| message.text_content())
                .collect::<Vec<_>>(),
            vec!["source-root", "source-user", "source-assistant"]
        );
    }

    #[test]
    fn functional_export_import_roundtrip_replace_overwrites_target_graph() {
        let temp = tempdir().expect("tempdir");
        let source_path = temp.path().join("replace-roundtrip-source.jsonl");
        let target_path = temp.path().join("replace-roundtrip-target.jsonl");
        let export_path = temp.path().join("replace-roundtrip-export.jsonl");

        let mut source_store = SessionStore::load(&source_path).expect("load source");
        let source_head = source_store
            .append_messages(None, &[pi_ai::Message::system("source-root")])
            .expect("append source");
        source_store
            .append_messages(source_head, &[pi_ai::Message::user("source-user")])
            .expect("append source user");
        source_store
            .export_lineage(source_store.head_id(), &export_path)
            .expect("export source");

        let mut target_store = SessionStore::load(&target_path).expect("load target");
        let target_head = target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target");
        target_store
            .append_messages(
                target_head,
                &[pi_ai::Message::assistant_text("target-assistant")],
            )
            .expect("append target assistant");

        let report = target_store
            .import_snapshot(&export_path, SessionImportMode::Replace)
            .expect("replace import");
        assert_eq!(report.imported_entries, 2);
        assert_eq!(report.replaced_entries, 2);
        assert_eq!(report.remapped_entries, 0);
        assert!(report.remapped_ids.is_empty());
        assert_eq!(target_store.entries().len(), 2);
        assert_eq!(
            target_store.entries()[0].message.text_content(),
            "source-root"
        );
        assert_eq!(
            target_store.entries()[1].message.text_content(),
            "source-user"
        );
        assert!(target_store.validation_report().is_valid());
    }

    #[test]
    fn functional_import_snapshot_replace_overwrites_entries() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("replace-target.jsonl");
        let source = temp.path().join("replace-source.jsonl");

        let mut target_store = SessionStore::load(&target).expect("load target");
        let head = target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target root");
        target_store
            .append_messages(head, &[pi_ai::Message::user("target-user")])
            .expect("append target user");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":10,"parent_id":null,"message":pi_ai::Message::system("source-root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":11,"parent_id":10,"message":pi_ai::Message::user("source-user")}).to_string(),
        ]
        .join("\n");
        fs::write(&source, format!("{raw}\n")).expect("write source");

        let report = target_store
            .import_snapshot(&source, SessionImportMode::Replace)
            .expect("replace import");
        assert_eq!(report.imported_entries, 2);
        assert_eq!(report.remapped_entries, 0);
        assert!(report.remapped_ids.is_empty());
        assert_eq!(report.replaced_entries, 2);
        assert_eq!(report.resulting_entries, 2);
        assert_eq!(report.active_head, Some(11));
        assert_eq!(target_store.entries().len(), 2);
        assert_eq!(target_store.entries()[0].id, 10);
        assert_eq!(target_store.entries()[1].id, 11);

        let next = target_store
            .append_messages(
                target_store.head_id(),
                &[pi_ai::Message::assistant_text("next")],
            )
            .expect("append after replace");
        assert_eq!(next, Some(12));
    }

    #[test]
    fn integration_import_snapshot_replace_with_empty_source_clears_session() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("replace-empty-target.jsonl");
        let source = temp.path().join("replace-empty-source.jsonl");

        let mut target_store = SessionStore::load(&target).expect("load target");
        target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target");
        fs::write(
            &source,
            format!(
                "{}\n",
                serde_json::json!({"record_type":"meta","schema_version":1})
            ),
        )
        .expect("write empty snapshot");

        let report = target_store
            .import_snapshot(&source, SessionImportMode::Replace)
            .expect("replace import");
        assert_eq!(report.imported_entries, 0);
        assert_eq!(report.remapped_entries, 0);
        assert!(report.remapped_ids.is_empty());
        assert_eq!(report.replaced_entries, 1);
        assert_eq!(report.resulting_entries, 0);
        assert_eq!(report.active_head, None);
        assert!(target_store.entries().is_empty());
    }

    #[test]
    fn regression_import_snapshot_rejects_invalid_source_and_preserves_target() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("invalid-target.jsonl");
        let source = temp.path().join("invalid-source.jsonl");

        let mut target_store = SessionStore::load(&target).expect("load target");
        target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":pi_ai::Message::system("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
        fs::write(&source, format!("{raw}\n")).expect("write invalid source");

        let error = target_store
            .import_snapshot(&source, SessionImportMode::Merge)
            .expect_err("invalid import should fail");
        assert!(error
            .to_string()
            .contains("import session validation failed"));
        assert_eq!(target_store.entries().len(), 1);
        assert_eq!(
            target_store.entries()[0].message.text_content(),
            "target-root"
        );
    }

    #[test]
    fn unit_validation_report_detects_duplicates_invalid_parents_and_cycles() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("validate-invalid.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":pi_ai::Message::system("root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":99,"message":pi_ai::Message::user("dangling")}).to_string(),
            serde_json::json!({"record_type":"entry","id":3,"parent_id":4,"message":pi_ai::Message::user("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":4,"parent_id":3,"message":pi_ai::Message::user("cycle-b")}).to_string(),
            serde_json::json!({"record_type":"entry","id":6,"parent_id":1,"message":pi_ai::Message::user("duplicate-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":6,"parent_id":1,"message":pi_ai::Message::user("duplicate-b")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write invalid session");

        let store = SessionStore::load(&path).expect("load");
        let report = store.validation_report();
        assert_eq!(
            report,
            SessionValidationReport {
                entries: 6,
                duplicates: 1,
                invalid_parent: 1,
                cycles: 2,
            }
        );
        assert!(!report.is_valid());
    }

    #[test]
    fn regression_validation_report_for_valid_session_is_clean() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("validate-valid.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("q1"),
                    pi_ai::Message::assistant_text("a1"),
                ],
            )
            .expect("append");

        let report = store.validation_report();
        assert_eq!(
            report,
            SessionValidationReport {
                entries: 3,
                duplicates: 0,
                invalid_parent: 0,
                cycles: 0,
            }
        );
        assert!(report.is_valid());
    }

    #[test]
    fn append_rejects_unknown_parent_id() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let error = store
            .append_messages(Some(42), &[pi_ai::Message::user("hello")])
            .expect_err("must fail for unknown parent");

        assert!(error
            .to_string()
            .contains("parent id 42 does not exist in session"));
    }

    #[test]
    fn detects_cycles_in_session_lineage() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":pi_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::user("hello")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write session file");

        let store = SessionStore::load(&path).expect("load");
        let error = store
            .lineage_messages(Some(1))
            .expect_err("lineage should fail for cycle");
        assert!(error.to_string().contains("detected a cycle"));
    }

    #[test]
    fn writes_schema_meta_record() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");

        let raw = fs::read_to_string(&path).expect("session must exist");
        let first_line = raw.lines().next().expect("first line");
        assert!(first_line.contains("\"record_type\":\"meta\""));
        assert!(first_line.contains("\"schema_version\":1"));
    }

    #[test]
    fn loads_legacy_session_entry_lines_without_meta() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("legacy.jsonl");

        let legacy = SessionEntry {
            id: 1,
            parent_id: None,
            message: pi_ai::Message::system("legacy"),
        };
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string(&legacy).expect("serialize")),
        )
        .expect("write legacy");

        let store = SessionStore::load(&path).expect("load legacy");
        assert_eq!(store.entries().len(), 1);
        assert_eq!(store.entries()[0].message.text_content(), "legacy");
    }

    #[test]
    fn repair_removes_invalid_and_cycle_entries() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("repair.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":pi_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":99,"message":pi_ai::Message::user("dangling")}).to_string(),
            serde_json::json!({"record_type":"entry","id":3,"parent_id":4,"message":pi_ai::Message::user("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":4,"parent_id":3,"message":pi_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write malformed session");

        let mut store = SessionStore::load(&path).expect("load");
        let report = store.repair().expect("repair should succeed");

        assert_eq!(
            report,
            RepairReport {
                removed_duplicates: 0,
                duplicate_ids: Vec::new(),
                removed_invalid_parent: 1,
                invalid_parent_ids: vec![2],
                removed_cycles: 2,
                cycle_ids: vec![3, 4],
            }
        );
        assert_eq!(store.entries().len(), 1);
        assert_eq!(store.entries()[0].id, 1);
    }

    #[test]
    fn functional_compact_to_lineage_prunes_other_branches() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("compact.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("q1"),
                    pi_ai::Message::assistant_text("a1"),
                    pi_ai::Message::user("q2"),
                    pi_ai::Message::assistant_text("a2"),
                ],
            )
            .expect("append")
            .expect("head");

        store
            .append_messages(
                Some(head - 2),
                &[
                    pi_ai::Message::user("q2b"),
                    pi_ai::Message::assistant_text("a2b"),
                ],
            )
            .expect("append branch");

        let report = store.compact_to_lineage(Some(head)).expect("compact");
        assert_eq!(
            report,
            CompactReport {
                removed_entries: 2,
                retained_entries: 5,
                head_id: Some(head),
            }
        );
        assert_eq!(store.entries().len(), 5);
        assert_eq!(store.branch_tips().len(), 1);
        assert_eq!(store.branch_tips()[0].id, head);

        let reloaded = SessionStore::load(&path).expect("reload");
        assert_eq!(reloaded.entries().len(), 5);
        assert!(!reloaded.contains(head + 1));
        assert!(!reloaded.contains(head + 2));
    }

    #[test]
    fn integration_compact_then_append_preserves_next_id_progression() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("compact-append.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append")
            .expect("head");
        store
            .append_messages(Some(head), &[pi_ai::Message::user("main")])
            .expect("append");
        store
            .append_messages(Some(head), &[pi_ai::Message::user("branch")])
            .expect("append");

        store
            .compact_to_lineage(Some(head + 1))
            .expect("compact should succeed");
        let next_head = store
            .append_messages(
                store.head_id(),
                &[pi_ai::Message::assistant_text("after-compact")],
            )
            .expect("append after compact")
            .expect("next head");

        assert_eq!(next_head, 3);
    }

    #[test]
    fn regression_compact_to_lineage_errors_for_unknown_head() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("compact-unknown-head.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let error = store
            .compact_to_lineage(Some(999))
            .expect_err("unknown head should fail");
        assert!(error.to_string().contains("unknown session id 999"));
    }

    #[test]
    fn regression_compact_to_lineage_fails_on_cycle() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("compact-cycle.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":pi_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::user("hello")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write cycle session");

        let mut store = SessionStore::load(&path).expect("load");
        let error = store
            .compact_to_lineage(Some(1))
            .expect_err("cycle should fail");
        assert!(error.to_string().contains("detected a cycle"));
    }

    #[test]
    fn unit_compact_empty_store_is_noop() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("compact-empty.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let report = store.compact_to_lineage(None).expect("compact");
        assert_eq!(
            report,
            CompactReport {
                removed_entries: 0,
                retained_entries: 0,
                head_id: None,
            }
        );
        assert!(store.entries().is_empty());
    }

    #[test]
    fn concurrent_appends_are_serialized_with_locking() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("concurrent.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("init");

        let path1 = path.clone();
        let path2 = path.clone();

        let worker = |path: PathBuf, label: &'static str| {
            thread::spawn(move || {
                let mut store = SessionStore::load(&path).expect("load worker");
                let head = store.head_id();
                store
                    .append_messages(head, &[pi_ai::Message::user(label)])
                    .expect("append worker");
            })
        };

        let t1 = worker(path1, "a");
        let t2 = worker(path2, "b");
        t1.join().expect("thread 1");
        t2.join().expect("thread 2");

        let store = SessionStore::load(&path).expect("reload");
        assert_eq!(store.entries().len(), 3);
    }

    #[test]
    fn stress_parallel_appends_high_volume_remain_consistent() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("stress-concurrent.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("init");

        let workers = 8;
        let appends_per_worker = 25;
        let mut handles = Vec::new();
        for worker_index in 0..workers {
            let path = path.clone();
            handles.push(thread::spawn(move || {
                for append_index in 0..appends_per_worker {
                    let mut retries = 0usize;
                    loop {
                        let mut store = SessionStore::load(&path).expect("load worker");
                        let head = store.head_id();
                        match store.append_messages(
                            head,
                            &[pi_ai::Message::user(format!(
                                "worker-{worker_index}-append-{append_index}"
                            ))],
                        ) {
                            Ok(_) => break,
                            Err(error)
                                if error.to_string().contains("timed out acquiring lock") =>
                            {
                                retries += 1;
                                if retries >= 8 {
                                    panic!("append worker retries exhausted: {error}");
                                }
                                thread::sleep(Duration::from_millis(30));
                            }
                            Err(error) => panic!("append worker: {error}"),
                        }
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().expect("worker join");
        }

        let reloaded = SessionStore::load(&path).expect("reload");
        assert_eq!(reloaded.entries().len(), 1 + workers * appends_per_worker);
        let unique_ids = reloaded
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<HashSet<_>>();
        assert_eq!(unique_ids.len(), reloaded.entries().len());
    }

    #[test]
    fn repair_command_is_idempotent_for_valid_session() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("idempotent.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        store
            .append_messages(head, &[pi_ai::Message::user("hello")])
            .expect("append");

        let report = store.repair().expect("repair");
        assert_eq!(report, RepairReport::default());
        assert_eq!(store.entries().len(), 2);
    }

    #[test]
    fn lock_file_is_cleaned_up_after_append() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("lock-cleanup.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");

        let lock = path.with_extension("lock");
        assert!(!lock.exists());
    }

    #[test]
    fn session_file_remains_parseable_after_multiple_writes() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("atomic.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        for i in 0..10 {
            let head = store.head_id();
            store
                .append_messages(head, &[pi_ai::Message::user(format!("msg-{i}"))])
                .expect("append");
        }

        let reloaded = SessionStore::load(&path).expect("reload");
        assert_eq!(reloaded.entries().len(), 10);
    }

    #[test]
    fn regression_lineage_unknown_id_still_errors() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("unknown-id.jsonl");
        let store = SessionStore::load(&path).expect("load");

        let error = store
            .lineage_messages(Some(999))
            .expect_err("unknown id should fail");
        assert!(error.to_string().contains("unknown session id 999"));
    }

    #[test]
    fn functional_branch_tips_returns_all_leaf_nodes() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("tips.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append")
            .expect("head");

        let branch_a = store
            .append_messages(Some(head), &[pi_ai::Message::user("a")])
            .expect("append")
            .expect("branch a");
        let branch_b = store
            .append_messages(Some(head), &[pi_ai::Message::user("b")])
            .expect("append")
            .expect("branch b");

        let tips = store.branch_tips();
        let ids = tips.iter().map(|entry| entry.id).collect::<Vec<_>>();
        assert!(ids.contains(&branch_a));
        assert!(ids.contains(&branch_b));
    }

    #[test]
    fn integration_load_after_external_rewrite_keeps_consistency() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("external.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");

        let external_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":pi_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::user("external")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{external_raw}\n")).expect("external write");

        let mut reloaded = SessionStore::load(&path).expect("reload");
        let head = reloaded.head_id();
        reloaded
            .append_messages(head, &[pi_ai::Message::assistant_text("local")])
            .expect("append");

        let final_store = SessionStore::load(&path).expect("final load");
        assert_eq!(final_store.entries().len(), 3);
    }

    #[test]
    fn regression_repair_handles_duplicate_ids() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("dupe.jsonl");

        let duplicate_entry = SessionEntry {
            id: 1,
            parent_id: None,
            message: pi_ai::Message::system("sys"),
        };
        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":pi_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":pi_ai::Message::user("dupe")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write dupes");

        let mut store = SessionStore::load(&path).expect("load");
        let report = store.repair().expect("repair");
        assert_eq!(report.removed_duplicates, 1);
        assert_eq!(report.duplicate_ids, vec![1]);
        assert_eq!(store.entries().len(), 1);
        assert_eq!(store.entries()[0].id, duplicate_entry.id);
    }

    #[test]
    fn unit_append_messages_updates_internal_next_id() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("next-id.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(head, &[pi_ai::Message::user("u1")])
            .expect("append");
        store
            .append_messages(head, &[pi_ai::Message::assistant_text("a1")])
            .expect("append");

        assert_eq!(store.entries().last().map(|entry| entry.id), Some(3));
    }

    #[test]
    fn regression_repair_retains_root_nodes() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("roots.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":null,"message":pi_ai::Message::system("root1")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":null,"message":pi_ai::Message::system("root2")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write roots");

        let mut store = SessionStore::load(&path).expect("load");
        let report = store.repair().expect("repair");
        assert_eq!(report, RepairReport::default());
        assert_eq!(store.entries().len(), 2);
    }

    #[test]
    fn functional_lineage_for_none_head_returns_empty() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("none-head.jsonl");
        let store = SessionStore::load(&path).expect("load");

        let lineage = store.lineage_messages(None).expect("lineage");
        assert!(lineage.is_empty());
    }

    #[test]
    fn integration_parallel_stores_append_consistent_ids() {
        let temp = tempdir().expect("tempdir");
        let path = Arc::new(temp.path().join("parallel-ids.jsonl"));

        let mut store = SessionStore::load(&*path).expect("load");
        store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");

        let handles = (0..4)
            .map(|i| {
                let path = path.clone();
                thread::spawn(move || {
                    let mut store = SessionStore::load(&*path).expect("load thread");
                    let head = store.head_id();
                    store
                        .append_messages(head, &[pi_ai::Message::user(format!("u{i}"))])
                        .expect("append thread");
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().expect("thread join");
        }

        let store = SessionStore::load(&*path).expect("reload");
        let mut ids = store
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), store.entries().len());
    }

    #[test]
    fn regression_repair_report_counts_invalid_cycles() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("report-counts.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":pi_ai::Message::system("a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::system("b")}).to_string(),
        ]
        .join("\n");
        fs::write(&path, format!("{raw}\n")).expect("write cycles");

        let mut store = SessionStore::load(&path).expect("load");
        let report = store.repair().expect("repair");
        assert_eq!(report.removed_invalid_parent, 0);
        assert_eq!(report.removed_cycles, 2);
        assert_eq!(report.cycle_ids, vec![1, 2]);
    }

    #[test]
    fn regression_lock_timeout_when_lock_file_persists() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("timeout.jsonl");
        let lock_path = path.with_extension("lock");
        fs::write(&lock_path, "stale").expect("write lock");

        let mut store = SessionStore::load(&path).expect("load");
        store.set_lock_policy(150, 0);
        let error = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect_err("append should time out when lock persists");
        assert!(error.to_string().contains("timed out acquiring lock"));

        fs::remove_file(&lock_path).expect("cleanup lock");
    }

    #[test]
    fn functional_stale_lock_file_is_reclaimed_after_threshold() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("stale-reclaim.jsonl");
        let lock_path = path.with_extension("lock");
        fs::write(&lock_path, "stale").expect("write lock");
        thread::sleep(Duration::from_millis(30));

        let mut store = SessionStore::load(&path).expect("load");
        store.set_lock_policy(1_000, 10);
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append should reclaim stale lock");

        assert_eq!(head, Some(1));
        assert!(!lock_path.exists());
    }
}
