//! State-file persistence for issue bridge checkpoints and run metadata.

use std::{
    collections::{BTreeMap, HashSet},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    current_unix_timestamp_ms, issue_shared_session_id, write_text_atomic, TransportHealthSnapshot,
    GITHUB_STATE_SCHEMA_VERSION,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubIssuesBridgeState {
    schema_version: u32,
    #[serde(default)]
    last_issue_scan_at: Option<String>,
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    issue_sessions: BTreeMap<String, GithubIssueChatSessionState>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct GithubIssueChatSessionState {
    pub(super) session_id: String,
    #[serde(default)]
    pub(super) last_comment_id: Option<u64>,
    #[serde(default)]
    pub(super) last_run_id: Option<String>,
    #[serde(default)]
    pub(super) active_run_id: Option<String>,
    #[serde(default)]
    pub(super) last_event_key: Option<String>,
    #[serde(default)]
    pub(super) last_event_kind: Option<String>,
    #[serde(default)]
    pub(super) last_actor_login: Option<String>,
    #[serde(default)]
    pub(super) last_reason_code: Option<String>,
    #[serde(default)]
    pub(super) last_processed_unix_ms: Option<u64>,
    #[serde(default)]
    pub(super) total_processed_events: u64,
    #[serde(default)]
    pub(super) total_duplicate_events: u64,
    #[serde(default)]
    pub(super) total_failed_events: u64,
    #[serde(default)]
    pub(super) total_denied_events: u64,
    #[serde(default)]
    pub(super) total_runs_started: u64,
    #[serde(default)]
    pub(super) total_runs_completed: u64,
    #[serde(default)]
    pub(super) total_runs_failed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IssueEventOutcome {
    Processed,
    Denied,
    Failed,
}

impl Default for GithubIssuesBridgeState {
    fn default() -> Self {
        Self {
            schema_version: GITHUB_STATE_SCHEMA_VERSION,
            last_issue_scan_at: None,
            processed_event_keys: Vec::new(),
            issue_sessions: BTreeMap::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub(super) struct GithubIssuesBridgeStateStore {
    path: PathBuf,
    cap: usize,
    state: GithubIssuesBridgeState,
    processed_index: HashSet<String>,
}

impl GithubIssuesBridgeStateStore {
    /// Loads persisted bridge state and rebuilds in-memory de-duplication indexes.
    pub(super) fn load(path: PathBuf, cap: usize) -> Result<Self> {
        let mut state = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read state file {}", path.display()))?;
            match serde_json::from_str::<GithubIssuesBridgeState>(&raw) {
                Ok(state) => state,
                Err(error) => {
                    eprintln!(
                        "failed to parse github issues bridge state file {}: {} (starting fresh)",
                        path.display(),
                        error
                    );
                    GithubIssuesBridgeState::default()
                }
            }
        } else {
            GithubIssuesBridgeState::default()
        };

        if state.schema_version != GITHUB_STATE_SCHEMA_VERSION {
            eprintln!(
                "unsupported github issues bridge state schema: expected {}, found {} (starting fresh)",
                GITHUB_STATE_SCHEMA_VERSION,
                state.schema_version
            );
            state = GithubIssuesBridgeState::default();
        }

        let cap = cap.max(1);
        if state.processed_event_keys.len() > cap {
            let keep_from = state.processed_event_keys.len() - cap;
            state.processed_event_keys = state.processed_event_keys[keep_from..].to_vec();
        }
        let processed_index = state
            .processed_event_keys
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        Ok(Self {
            path,
            cap,
            state,
            processed_index,
        })
    }

    pub(super) fn contains(&self, key: &str) -> bool {
        self.processed_index.contains(key)
    }

    pub(super) fn mark_processed(&mut self, key: &str) -> bool {
        if self.processed_index.contains(key) {
            return false;
        }
        self.state.processed_event_keys.push(key.to_string());
        self.processed_index.insert(key.to_string());
        while self.state.processed_event_keys.len() > self.cap {
            let removed = self.state.processed_event_keys.remove(0);
            self.processed_index.remove(&removed);
        }
        true
    }

    pub(super) fn processed_event_tail(&self, limit: usize) -> Vec<String> {
        if limit == 0 || self.state.processed_event_keys.is_empty() {
            return Vec::new();
        }
        let total = self.state.processed_event_keys.len();
        let start = total.saturating_sub(limit);
        self.state.processed_event_keys[start..].to_vec()
    }

    pub(super) fn processed_event_cap(&self) -> usize {
        self.cap
    }

    pub(super) fn issue_session(&self, issue_number: u64) -> Option<&GithubIssueChatSessionState> {
        self.state.issue_sessions.get(&issue_number.to_string())
    }

    fn issue_session_mut(&mut self, issue_number: u64) -> &mut GithubIssueChatSessionState {
        self.state
            .issue_sessions
            .entry(issue_number.to_string())
            .or_insert_with(|| GithubIssueChatSessionState {
                session_id: issue_shared_session_id(issue_number),
                last_comment_id: None,
                last_run_id: None,
                active_run_id: None,
                last_event_key: None,
                last_event_kind: None,
                last_actor_login: None,
                last_reason_code: None,
                last_processed_unix_ms: None,
                total_processed_events: 0,
                total_duplicate_events: 0,
                total_failed_events: 0,
                total_denied_events: 0,
                total_runs_started: 0,
                total_runs_completed: 0,
                total_runs_failed: 0,
            })
    }

    pub(super) fn update_issue_session(
        &mut self,
        issue_number: u64,
        session_id: String,
        last_comment_id: Option<u64>,
        last_run_id: Option<String>,
    ) -> bool {
        let key = issue_number.to_string();
        let entry =
            self.state
                .issue_sessions
                .entry(key)
                .or_insert_with(|| GithubIssueChatSessionState {
                    session_id: session_id.clone(),
                    last_comment_id: None,
                    last_run_id: None,
                    active_run_id: None,
                    last_event_key: None,
                    last_event_kind: None,
                    last_actor_login: None,
                    last_reason_code: None,
                    last_processed_unix_ms: None,
                    total_processed_events: 0,
                    total_duplicate_events: 0,
                    total_failed_events: 0,
                    total_denied_events: 0,
                    total_runs_started: 0,
                    total_runs_completed: 0,
                    total_runs_failed: 0,
                });
        let mut changed = false;
        if entry.session_id != session_id {
            entry.session_id = session_id;
            changed = true;
        }
        if let Some(comment_id) = last_comment_id {
            if entry.last_comment_id != Some(comment_id) {
                entry.last_comment_id = Some(comment_id);
                changed = true;
            }
        }
        if let Some(run_id) = last_run_id {
            if entry.last_run_id.as_deref() != Some(run_id.as_str()) {
                entry.last_run_id = Some(run_id);
                changed = true;
            }
        }
        changed
    }

    pub(super) fn record_issue_duplicate_event(
        &mut self,
        issue_number: u64,
        event_key: &str,
        event_kind: &str,
        actor_login: &str,
    ) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.last_event_key.as_deref() != Some(event_key) {
            entry.last_event_key = Some(event_key.to_string());
        }
        if entry.last_event_kind.as_deref() != Some(event_kind) {
            entry.last_event_kind = Some(event_kind.to_string());
        }
        if entry.last_actor_login.as_deref() != Some(actor_login) {
            entry.last_actor_login = Some(actor_login.to_string());
        }
        if entry.last_reason_code.as_deref() != Some("duplicate_event") {
            entry.last_reason_code = Some("duplicate_event".to_string());
        }
        let processed_unix_ms = current_unix_timestamp_ms();
        if entry.last_processed_unix_ms != Some(processed_unix_ms) {
            entry.last_processed_unix_ms = Some(processed_unix_ms);
        }
        entry.total_duplicate_events = entry.total_duplicate_events.saturating_add(1);
        true
    }

    pub(super) fn record_issue_event_outcome(
        &mut self,
        issue_number: u64,
        event_key: &str,
        event_kind: &str,
        actor_login: &str,
        outcome: IssueEventOutcome,
        reason_code: Option<&str>,
    ) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.last_event_key.as_deref() != Some(event_key) {
            entry.last_event_key = Some(event_key.to_string());
        }
        if entry.last_event_kind.as_deref() != Some(event_kind) {
            entry.last_event_kind = Some(event_kind.to_string());
        }
        if entry.last_actor_login.as_deref() != Some(actor_login) {
            entry.last_actor_login = Some(actor_login.to_string());
        }
        if let Some(reason_code) = reason_code {
            if entry.last_reason_code.as_deref() != Some(reason_code) {
                entry.last_reason_code = Some(reason_code.to_string());
            }
        }
        let processed_unix_ms = current_unix_timestamp_ms();
        if entry.last_processed_unix_ms != Some(processed_unix_ms) {
            entry.last_processed_unix_ms = Some(processed_unix_ms);
        }
        entry.total_processed_events = entry.total_processed_events.saturating_add(1);
        match outcome {
            IssueEventOutcome::Processed => {}
            IssueEventOutcome::Denied => {
                entry.total_denied_events = entry.total_denied_events.saturating_add(1);
            }
            IssueEventOutcome::Failed => {
                entry.total_failed_events = entry.total_failed_events.saturating_add(1);
            }
        }
        true
    }

    pub(super) fn record_issue_run_started(&mut self, issue_number: u64, run_id: &str) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.active_run_id.as_deref() != Some(run_id) {
            entry.active_run_id = Some(run_id.to_string());
        }
        if entry.last_run_id.as_deref() != Some(run_id) {
            entry.last_run_id = Some(run_id.to_string());
        }
        entry.total_runs_started = entry.total_runs_started.saturating_add(1);
        true
    }

    pub(super) fn record_issue_run_finished(
        &mut self,
        issue_number: u64,
        run_id: &str,
        failed: bool,
    ) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.last_run_id.as_deref() != Some(run_id) {
            entry.last_run_id = Some(run_id.to_string());
        }
        if entry.active_run_id.is_some() {
            entry.active_run_id = None;
        }
        entry.total_runs_completed = entry.total_runs_completed.saturating_add(1);
        if failed {
            entry.total_runs_failed = entry.total_runs_failed.saturating_add(1);
        }
        true
    }

    pub(super) fn clear_issue_session(&mut self, issue_number: u64) -> bool {
        self.state
            .issue_sessions
            .remove(&issue_number.to_string())
            .is_some()
    }

    pub(super) fn last_issue_scan_at(&self) -> Option<&str> {
        self.state.last_issue_scan_at.as_deref()
    }

    pub(super) fn update_last_issue_scan_at(&mut self, value: Option<String>) -> bool {
        if self.state.last_issue_scan_at == value {
            return false;
        }
        self.state.last_issue_scan_at = value;
        true
    }

    pub(super) fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    pub(super) fn update_transport_health(&mut self, value: TransportHealthSnapshot) -> bool {
        if self.state.health == value {
            return false;
        }
        self.state.health = value;
        true
    }

    pub(super) fn save(&self) -> Result<()> {
        let mut payload =
            serde_json::to_string_pretty(&self.state).context("failed to serialize state")?;
        payload.push('\n');
        write_text_atomic(&self.path, &payload)
            .with_context(|| format!("failed to write state file {}", self.path.display()))?;
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct JsonlEventLog {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
}

impl JsonlEventLog {
    pub(super) fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub(super) fn append(&self, value: &Value) -> Result<()> {
        let line = serde_json::to_string(value).context("failed to encode log event")?;
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("event log mutex is poisoned"))?;
        writeln!(file, "{line}")
            .with_context(|| format!("failed to append to {}", self.path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush {}", self.path.display()))?;
        Ok(())
    }
}
