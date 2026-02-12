//! Core library surface for the crates crate.
use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use tau_ai::Message;

mod session_commands;
mod session_graph_commands;
mod session_integrity;
mod session_locking;
mod session_navigation_commands;
mod session_runtime_commands;
mod session_runtime_helpers;
mod session_storage;
#[cfg(test)]
mod tests;

use session_integrity::{
    collect_lineage_ids, has_cycle, merge_entries_with_remap, validation_report_for_entries,
};
use session_locking::acquire_lock;
use session_storage::{read_session_entries, write_session_entries_atomic};

const SESSION_SCHEMA_VERSION: u32 = 1;
const DEFAULT_LOCK_WAIT_MS: u64 = 5_000;
const DEFAULT_LOCK_STALE_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Public struct `SessionEntry` used across Tau components.
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
/// Public struct `RepairReport` used across Tau components.
pub struct RepairReport {
    pub removed_duplicates: usize,
    pub duplicate_ids: Vec<u64>,
    pub removed_invalid_parent: usize,
    pub invalid_parent_ids: Vec<u64>,
    pub removed_cycles: usize,
    pub cycle_ids: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Public struct `CompactReport` used across Tau components.
pub struct CompactReport {
    pub removed_entries: usize,
    pub retained_entries: usize,
    pub head_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Public struct `SessionValidationReport` used across Tau components.
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
/// Enumerates supported `SessionImportMode` values.
pub enum SessionImportMode {
    Merge,
    Replace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `ImportReport` used across Tau components.
pub struct ImportReport {
    pub imported_entries: usize,
    pub remapped_entries: usize,
    pub remapped_ids: Vec<(u64, u64)>,
    pub replaced_entries: usize,
    pub resulting_entries: usize,
    pub active_head: Option<u64>,
}

#[derive(Debug)]
/// Public struct `SessionStore` used across Tau components.
pub struct SessionStore {
    path: PathBuf,
    entries: Vec<SessionEntry>,
    next_id: u64,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
}

#[derive(Debug)]
/// Public struct `SessionRuntime` used across Tau components.
pub struct SessionRuntime {
    pub store: SessionStore,
    pub active_head: Option<u64>,
}

impl SessionStore {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create session directory {}", parent.display())
                })?;
            }
        }
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

pub use session_commands::*;
pub use session_graph_commands::*;
pub use session_navigation_commands::*;
pub use session_runtime_commands::*;
pub use session_runtime_helpers::*;
