//! External coding-agent bridge/session-pool runtime staging contracts.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCodingAgentBridgeConfig {
    pub inactivity_timeout_ms: u64,
    pub max_active_sessions: usize,
    pub max_events_per_session: usize,
}

impl Default for ExternalCodingAgentBridgeConfig {
    fn default() -> Self {
        Self {
            inactivity_timeout_ms: 10 * 60 * 1_000,
            max_active_sessions: 16,
            max_events_per_session: 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalCodingAgentSessionStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCodingAgentSessionSnapshot {
    pub session_id: String,
    pub workspace_id: String,
    pub status: ExternalCodingAgentSessionStatus,
    pub started_unix_ms: u64,
    pub last_activity_unix_ms: u64,
    pub queued_followups: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCodingAgentProgressEvent {
    pub sequence_id: u64,
    pub event_type: String,
    pub message: String,
    pub timestamp_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalCodingAgentBridgeError {
    InvalidWorkspaceId,
    InvalidMessage,
    SessionNotFound(String),
    SessionLimitReached { limit: usize },
}

impl std::fmt::Display for ExternalCodingAgentBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWorkspaceId => write!(f, "workspace id must be non-empty"),
            Self::InvalidMessage => write!(f, "message must be non-empty"),
            Self::SessionNotFound(session_id) => {
                write!(f, "session '{session_id}' was not found")
            }
            Self::SessionLimitReached { limit } => {
                write!(f, "max active sessions limit reached ({limit})")
            }
        }
    }
}

impl std::error::Error for ExternalCodingAgentBridgeError {}

#[derive(Debug, Clone)]
pub struct ExternalCodingAgentBridge {
    inner: Arc<Mutex<ExternalCodingAgentBridgeState>>,
}

#[derive(Debug)]
struct ExternalCodingAgentSessionRecord {
    snapshot: ExternalCodingAgentSessionSnapshot,
    events: VecDeque<ExternalCodingAgentProgressEvent>,
    queued_followups: VecDeque<String>,
}

#[derive(Debug)]
struct ExternalCodingAgentBridgeState {
    config: ExternalCodingAgentBridgeConfig,
    next_session_sequence: u64,
    next_event_sequence: u64,
    sessions: HashMap<String, ExternalCodingAgentSessionRecord>,
    workspace_to_session: HashMap<String, String>,
}

impl ExternalCodingAgentBridge {
    pub fn new(config: ExternalCodingAgentBridgeConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ExternalCodingAgentBridgeState {
                config,
                next_session_sequence: 0,
                next_event_sequence: 0,
                sessions: HashMap::new(),
                workspace_to_session: HashMap::new(),
            })),
        }
    }

    pub fn open_or_reuse_session(
        &self,
        workspace_id: &str,
    ) -> Result<ExternalCodingAgentSessionSnapshot, ExternalCodingAgentBridgeError> {
        let normalized_workspace = workspace_id.trim();
        if normalized_workspace.is_empty() {
            return Err(ExternalCodingAgentBridgeError::InvalidWorkspaceId);
        }
        let mut state = lock_or_recover(&self.inner);
        if let Some(existing_session_id) = state.workspace_to_session.get(normalized_workspace) {
            if let Some(existing) = state.sessions.get(existing_session_id) {
                if existing.snapshot.status == ExternalCodingAgentSessionStatus::Running {
                    return Ok(existing.snapshot.clone());
                }
            }
            let stale_session_id = existing_session_id.clone();
            state.sessions.remove(stale_session_id.as_str());
            state.workspace_to_session.remove(normalized_workspace);
        }
        if state.sessions.len() >= state.config.max_active_sessions {
            return Err(ExternalCodingAgentBridgeError::SessionLimitReached {
                limit: state.config.max_active_sessions,
            });
        }

        state.next_session_sequence = state.next_session_sequence.saturating_add(1);
        let session_id = format!("external-session-{}", state.next_session_sequence);
        let now_unix_ms = current_unix_timestamp_ms();
        let snapshot = ExternalCodingAgentSessionSnapshot {
            session_id: session_id.clone(),
            workspace_id: normalized_workspace.to_string(),
            status: ExternalCodingAgentSessionStatus::Running,
            started_unix_ms: now_unix_ms,
            last_activity_unix_ms: now_unix_ms,
            queued_followups: 0,
        };
        state.sessions.insert(
            session_id.clone(),
            ExternalCodingAgentSessionRecord {
                snapshot: snapshot.clone(),
                events: VecDeque::new(),
                queued_followups: VecDeque::new(),
            },
        );
        state
            .workspace_to_session
            .insert(normalized_workspace.to_string(), session_id);
        Ok(snapshot)
    }

    pub fn active_session_count(&self) -> usize {
        let state = lock_or_recover(&self.inner);
        state.sessions.len()
    }

    pub fn snapshot(&self, session_id: &str) -> Option<ExternalCodingAgentSessionSnapshot> {
        let state = lock_or_recover(&self.inner);
        state
            .sessions
            .get(session_id)
            .map(|value| value.snapshot.clone())
    }

    pub fn append_progress(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<ExternalCodingAgentProgressEvent, ExternalCodingAgentBridgeError> {
        self.append_event(session_id, "progress", message, false)
    }

    pub fn queue_followup(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<ExternalCodingAgentProgressEvent, ExternalCodingAgentBridgeError> {
        self.append_event(session_id, "followup", message, true)
    }

    pub fn poll_events(
        &self,
        session_id: &str,
        after_sequence_id: Option<u64>,
        limit: usize,
    ) -> Result<Vec<ExternalCodingAgentProgressEvent>, ExternalCodingAgentBridgeError> {
        let state = lock_or_recover(&self.inner);
        let Some(record) = state.sessions.get(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        let lower_bound = after_sequence_id.unwrap_or(0);
        let capped_limit = limit.max(1).min(state.config.max_events_per_session.max(1));
        Ok(record
            .events
            .iter()
            .filter(|event| event.sequence_id > lower_bound)
            .take(capped_limit)
            .cloned()
            .collect())
    }

    pub fn take_followups(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<String>, ExternalCodingAgentBridgeError> {
        let mut state = lock_or_recover(&self.inner);
        let Some(record) = state.sessions.get_mut(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        let capped_limit = limit.max(1).min(record.queued_followups.len().max(1));
        let mut drained = Vec::new();
        for _ in 0..capped_limit {
            let Some(value) = record.queued_followups.pop_front() else {
                break;
            };
            drained.push(value);
        }
        record.snapshot.queued_followups = record.queued_followups.len();
        Ok(drained)
    }

    pub fn mark_completed(
        &self,
        session_id: &str,
    ) -> Result<ExternalCodingAgentSessionSnapshot, ExternalCodingAgentBridgeError> {
        self.mark_terminal(session_id, ExternalCodingAgentSessionStatus::Completed)
    }

    pub fn mark_failed(
        &self,
        session_id: &str,
    ) -> Result<ExternalCodingAgentSessionSnapshot, ExternalCodingAgentBridgeError> {
        self.mark_terminal(session_id, ExternalCodingAgentSessionStatus::Failed)
    }

    pub fn close_session(
        &self,
        session_id: &str,
    ) -> Result<ExternalCodingAgentSessionSnapshot, ExternalCodingAgentBridgeError> {
        self.mark_terminal(session_id, ExternalCodingAgentSessionStatus::Closed)?;
        let mut state = lock_or_recover(&self.inner);
        let Some(record) = state.sessions.remove(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        state
            .workspace_to_session
            .remove(record.snapshot.workspace_id.as_str());
        Ok(record.snapshot)
    }

    pub fn reap_inactive_sessions(
        &self,
        now_unix_ms: u64,
    ) -> Vec<ExternalCodingAgentSessionSnapshot> {
        let mut state = lock_or_recover(&self.inner);
        let mut stale_ids = Vec::new();
        let timeout = state.config.inactivity_timeout_ms;
        for (session_id, record) in &state.sessions {
            if record.snapshot.status != ExternalCodingAgentSessionStatus::Running {
                continue;
            }
            if now_unix_ms.saturating_sub(record.snapshot.last_activity_unix_ms) > timeout {
                stale_ids.push(session_id.clone());
            }
        }

        let mut reaped = Vec::new();
        for stale_id in stale_ids {
            if let Some(mut record) = state.sessions.remove(stale_id.as_str()) {
                record.snapshot.status = ExternalCodingAgentSessionStatus::TimedOut;
                record.snapshot.last_activity_unix_ms = now_unix_ms;
                state
                    .workspace_to_session
                    .remove(record.snapshot.workspace_id.as_str());
                reaped.push(record.snapshot);
            }
        }
        reaped.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        reaped
    }

    fn append_event(
        &self,
        session_id: &str,
        event_type: &str,
        message: &str,
        queue_followup: bool,
    ) -> Result<ExternalCodingAgentProgressEvent, ExternalCodingAgentBridgeError> {
        let normalized_message = message.trim();
        if normalized_message.is_empty() {
            return Err(ExternalCodingAgentBridgeError::InvalidMessage);
        }
        let mut state = lock_or_recover(&self.inner);
        let now_unix_ms = current_unix_timestamp_ms();
        state.next_event_sequence = state.next_event_sequence.saturating_add(1);
        let event = ExternalCodingAgentProgressEvent {
            sequence_id: state.next_event_sequence,
            event_type: event_type.to_string(),
            message: normalized_message.to_string(),
            timestamp_unix_ms: now_unix_ms,
        };

        let max_events = state.config.max_events_per_session.max(1);
        let Some(record) = state.sessions.get_mut(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        record.events.push_back(event.clone());
        while record.events.len() > max_events {
            record.events.pop_front();
        }
        if queue_followup {
            record
                .queued_followups
                .push_back(normalized_message.to_string());
            record.snapshot.queued_followups = record.queued_followups.len();
        }
        record.snapshot.last_activity_unix_ms = now_unix_ms;
        Ok(event)
    }

    fn mark_terminal(
        &self,
        session_id: &str,
        status: ExternalCodingAgentSessionStatus,
    ) -> Result<ExternalCodingAgentSessionSnapshot, ExternalCodingAgentBridgeError> {
        let mut state = lock_or_recover(&self.inner);
        let Some(record) = state.sessions.get_mut(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        record.snapshot.status = status;
        record.snapshot.last_activity_unix_ms = current_unix_timestamp_ms();
        Ok(record.snapshot.clone())
    }
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn current_unix_timestamp_ms() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_c01_session_pool_reuses_workspace_session_and_tracks_lifecycle() {
        let bridge = ExternalCodingAgentBridge::new(ExternalCodingAgentBridgeConfig::default());

        let first = bridge
            .open_or_reuse_session("workspace-a")
            .expect("initial session open");
        assert_eq!(first.workspace_id, "workspace-a");
        assert_eq!(first.status, ExternalCodingAgentSessionStatus::Running);

        let reused = bridge
            .open_or_reuse_session("workspace-a")
            .expect("workspace session reuse");
        assert_eq!(reused.session_id, first.session_id);
        assert_eq!(bridge.active_session_count(), 1);

        let completed = bridge
            .mark_completed(first.session_id.as_str())
            .expect("mark session completed");
        assert_eq!(
            completed.status,
            ExternalCodingAgentSessionStatus::Completed
        );
        let completed_snapshot = bridge
            .snapshot(first.session_id.as_str())
            .expect("snapshot exists");
        assert_eq!(
            completed_snapshot.status,
            ExternalCodingAgentSessionStatus::Completed
        );

        let closed = bridge
            .close_session(first.session_id.as_str())
            .expect("close completed session");
        assert_eq!(closed.status, ExternalCodingAgentSessionStatus::Closed);
        assert_eq!(bridge.active_session_count(), 0);
    }

    #[test]
    fn spec_c01_reopen_after_terminal_state_creates_new_running_session() {
        let bridge = ExternalCodingAgentBridge::new(ExternalCodingAgentBridgeConfig::default());
        let first = bridge
            .open_or_reuse_session("workspace-reopen")
            .expect("open initial session");

        bridge
            .mark_completed(first.session_id.as_str())
            .expect("complete initial session");

        let reopened = bridge
            .open_or_reuse_session("workspace-reopen")
            .expect("open replacement session");
        assert_ne!(reopened.session_id, first.session_id);
        assert_eq!(reopened.status, ExternalCodingAgentSessionStatus::Running);
    }

    #[test]
    fn spec_c02_progress_and_followups_emit_monotonic_events() {
        let bridge = ExternalCodingAgentBridge::new(ExternalCodingAgentBridgeConfig::default());
        let session = bridge
            .open_or_reuse_session("workspace-progress")
            .expect("open progress session");

        bridge
            .append_progress(session.session_id.as_str(), "spawned subprocess")
            .expect("append progress");
        bridge
            .queue_followup(session.session_id.as_str(), "add repo context")
            .expect("queue follow-up");

        let events = bridge
            .poll_events(session.session_id.as_str(), None, 16)
            .expect("poll events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence_id, 1);
        assert_eq!(events[1].sequence_id, 2);
        assert_eq!(events[0].event_type, "progress");
        assert_eq!(events[1].event_type, "followup");

        let followups = bridge
            .take_followups(session.session_id.as_str(), 16)
            .expect("drain followups");
        assert_eq!(followups, vec!["add repo context".to_string()]);

        let snapshot = bridge
            .snapshot(session.session_id.as_str())
            .expect("snapshot after followup drain");
        assert_eq!(snapshot.queued_followups, 0);

        let replay = bridge
            .poll_events(session.session_id.as_str(), Some(1), 16)
            .expect("poll replay after first event");
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].sequence_id, 2);
    }

    #[test]
    fn spec_c03_regression_inactivity_reaper_times_out_stale_sessions() {
        let config = ExternalCodingAgentBridgeConfig {
            inactivity_timeout_ms: 1_000,
            max_active_sessions: 8,
            max_events_per_session: 64,
        };
        let bridge = ExternalCodingAgentBridge::new(config);
        let session = bridge
            .open_or_reuse_session("workspace-timeout")
            .expect("open timeout session");

        let reaped = bridge.reap_inactive_sessions(session.last_activity_unix_ms + 1_001);
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].session_id, session.session_id);
        assert_eq!(reaped[0].status, ExternalCodingAgentSessionStatus::TimedOut);
        assert_eq!(bridge.active_session_count(), 0);
    }
}
