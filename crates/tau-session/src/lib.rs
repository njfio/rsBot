//! Session storage, lineage, and runtime session command utilities for Tau.
//!
//! Implements session persistence, branch/merge/search/stat tooling, and
//! session graph export helpers shared by local and bridge runtimes.

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
mod session_store_runtime;
#[cfg(test)]
mod tests;

use session_integrity::{
    collect_lineage_ids, has_cycle, merge_entries_with_remap, validation_report_for_entries,
};
use session_locking::acquire_lock;
use session_storage::{
    maybe_import_legacy_jsonl_into_sqlite, read_session_entries, resolve_session_backend,
    write_session_entries_atomic,
};

const SESSION_SCHEMA_VERSION: u32 = 1;
const DEFAULT_LOCK_WAIT_MS: u64 = 5_000;
const DEFAULT_LOCK_STALE_MS: u64 = 30_000;
const SESSION_BACKEND_ENV: &str = "TAU_SESSION_BACKEND";
const SESSION_POSTGRES_DSN_ENV: &str = "TAU_SESSION_POSTGRES_DSN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
/// Enumerates supported `SessionStorageBackend` values.
pub enum SessionStorageBackend {
    Jsonl,
    Sqlite,
    Postgres,
}

impl SessionStorageBackend {
    pub fn label(self) -> &'static str {
        match self {
            SessionStorageBackend::Jsonl => "jsonl",
            SessionStorageBackend::Sqlite => "sqlite",
            SessionStorageBackend::Postgres => "postgres",
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `SessionMergeStrategy` values.
pub enum SessionMergeStrategy {
    Append,
    Squash,
    FastForward,
}

impl SessionMergeStrategy {
    pub fn label(self) -> &'static str {
        match self {
            SessionMergeStrategy::Append => "append",
            SessionMergeStrategy::Squash => "squash",
            SessionMergeStrategy::FastForward => "fast-forward",
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `BranchMergeReport` used across Tau components.
pub struct BranchMergeReport {
    pub source_head: u64,
    pub target_head: u64,
    pub strategy: SessionMergeStrategy,
    pub common_ancestor: Option<u64>,
    pub appended_entries: usize,
    pub merged_head: u64,
}

#[derive(Debug)]
/// Public struct `SessionStore` used across Tau components.
pub struct SessionStore {
    path: PathBuf,
    backend: SessionStorageBackend,
    backend_reason_code: String,
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

pub use session_commands::*;
pub use session_graph_commands::*;
pub use session_navigation_commands::*;
pub use session_runtime_commands::*;
pub use session_runtime_helpers::*;
