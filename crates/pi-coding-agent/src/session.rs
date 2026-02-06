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
const LOCK_WAIT_MS: u64 = 5_000;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub removed_duplicates: usize,
    pub removed_invalid_parent: usize,
    pub removed_cycles: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactReport {
    pub removed_entries: usize,
    pub retained_entries: usize,
    pub head_id: Option<u64>,
}

#[derive(Debug)]
pub struct SessionStore {
    path: PathBuf,
    entries: Vec<SessionEntry>,
    next_id: u64,
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
        let _lock = acquire_lock(&lock_path, Duration::from_millis(LOCK_WAIT_MS))?;

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
        let Some(mut current_id) = head_id else {
            return Ok(Vec::new());
        };

        let mut ids = Vec::new();
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

            ids.push(entry.id);
            match entry.parent_id {
                Some(parent) => current_id = parent,
                None => break,
            }
        }

        ids.reverse();

        let messages = ids
            .into_iter()
            .map(|id| {
                self.entries
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.message.clone())
                    .ok_or_else(|| anyhow!("missing message for id {id}"))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(messages)
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
        let _lock = acquire_lock(&lock_path, Duration::from_millis(LOCK_WAIT_MS))?;

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

            if invalid_ids.is_empty() {
                break;
            }

            for id in invalid_ids {
                if id_to_entry.remove(&id).is_some() {
                    report.removed_invalid_parent += 1;
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
        for id in cycle_ids {
            if id_to_entry.remove(&id).is_some() {
                report.removed_cycles += 1;
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
        let _lock = acquire_lock(&lock_path, Duration::from_millis(LOCK_WAIT_MS))?;

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

fn acquire_lock(path: &Path, timeout: Duration) -> Result<LockGuard> {
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

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, path::PathBuf, sync::Arc, thread};

    use tempfile::tempdir;

    use super::{CompactReport, RepairReport, SessionEntry, SessionStore};

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
                removed_invalid_parent: 1,
                removed_cycles: 2,
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
                    let mut store = SessionStore::load(&path).expect("load worker");
                    let head = store.head_id();
                    store
                        .append_messages(
                            head,
                            &[pi_ai::Message::user(format!(
                                "worker-{worker_index}-append-{append_index}"
                            ))],
                        )
                        .expect("append worker");
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
    }

    #[test]
    fn regression_lock_timeout_when_lock_file_persists() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("timeout.jsonl");
        let lock_path = path.with_extension("lock");
        fs::write(&lock_path, "stale").expect("write lock");

        let mut store = SessionStore::load(&path).expect("load");
        let error = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect_err("append should time out when lock persists");
        assert!(error.to_string().contains("timed out acquiring lock"));

        fs::remove_file(&lock_path).expect("cleanup lock");
    }
}
