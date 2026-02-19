//! External coding-agent bridge/session-pool runtime contracts.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const SUBPROCESS_STDOUT_EVENT_TYPE: &str = "subprocess.stdout";
const SUBPROCESS_STDERR_EVENT_TYPE: &str = "subprocess.stderr";
const SUBPROCESS_EXIT_EVENT_TYPE: &str = "subprocess.exit";
const SUBPROCESS_STDIN_ERROR_EVENT_TYPE: &str = "subprocess.stdin_error";
const SUBPROCESS_RUNTIME_ERROR_EVENT_TYPE: &str = "subprocess.runtime_error";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCodingAgentSubprocessConfig {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalCodingAgentBridgeConfig {
    pub inactivity_timeout_ms: u64,
    pub max_active_sessions: usize,
    pub max_events_per_session: usize,
    pub subprocess: Option<ExternalCodingAgentSubprocessConfig>,
}

impl Default for ExternalCodingAgentBridgeConfig {
    fn default() -> Self {
        Self {
            inactivity_timeout_ms: 10 * 60 * 1_000,
            max_active_sessions: 16,
            max_events_per_session: 256,
            subprocess: None,
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
    InvalidSubprocessConfig(String),
    SessionNotFound(String),
    SessionLimitReached { limit: usize },
    SubprocessSpawnFailed { workspace_id: String, error: String },
    SubprocessIoError { session_id: String, error: String },
}

impl std::fmt::Display for ExternalCodingAgentBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWorkspaceId => write!(f, "workspace id must be non-empty"),
            Self::InvalidMessage => write!(f, "message must be non-empty"),
            Self::InvalidSubprocessConfig(message) => {
                write!(f, "invalid subprocess configuration: {message}")
            }
            Self::SessionNotFound(session_id) => {
                write!(f, "session '{session_id}' was not found")
            }
            Self::SessionLimitReached { limit } => {
                write!(f, "max active sessions limit reached ({limit})")
            }
            Self::SubprocessSpawnFailed {
                workspace_id,
                error,
            } => {
                write!(
                    f,
                    "failed to spawn external coding-agent subprocess for workspace '{workspace_id}': {error}"
                )
            }
            Self::SubprocessIoError { session_id, error } => {
                write!(
                    f,
                    "external coding-agent subprocess I/O failed for session '{session_id}': {error}"
                )
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
    subprocess: Option<ExternalCodingAgentSubprocessHandle>,
}

#[derive(Debug)]
struct ExternalCodingAgentBridgeState {
    config: ExternalCodingAgentBridgeConfig,
    next_session_sequence: u64,
    next_event_sequence: u64,
    sessions: HashMap<String, ExternalCodingAgentSessionRecord>,
    workspace_to_session: HashMap<String, String>,
}

#[derive(Debug)]
struct ExternalCodingAgentSubprocessHandle {
    child: Child,
    stdin: Option<ChildStdin>,
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
        if let Some(existing_session_id) = state
            .workspace_to_session
            .get(normalized_workspace)
            .cloned()
        {
            sync_session_subprocess_state(&mut state, existing_session_id.as_str());
            if let Some(existing) = state.sessions.get(existing_session_id.as_str()) {
                if existing.snapshot.status == ExternalCodingAgentSessionStatus::Running {
                    return Ok(existing.snapshot.clone());
                }
            }
            if let Some(mut stale_record) = state.sessions.remove(existing_session_id.as_str()) {
                terminate_subprocess_handle(&mut stale_record.subprocess);
            }
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
        let subprocess_config = state.config.subprocess.clone();
        let subprocess = match subprocess_config.as_ref() {
            Some(config) => Some(spawn_subprocess_for_session(
                Arc::downgrade(&self.inner),
                config,
                session_id.as_str(),
                normalized_workspace,
            )?),
            None => None,
        };
        state.sessions.insert(
            session_id.clone(),
            ExternalCodingAgentSessionRecord {
                snapshot: snapshot.clone(),
                events: VecDeque::new(),
                queued_followups: VecDeque::new(),
                subprocess,
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
        let mut state = lock_or_recover(&self.inner);
        sync_session_subprocess_state(&mut state, session_id);
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
        let mut state = lock_or_recover(&self.inner);
        sync_session_subprocess_state(&mut state, session_id);
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
        sync_session_subprocess_state(&mut state, session_id);
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
        let mut state = lock_or_recover(&self.inner);
        sync_session_subprocess_state(&mut state, session_id);
        let Some(mut record) = state.sessions.remove(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        record.snapshot.status = ExternalCodingAgentSessionStatus::Closed;
        record.snapshot.last_activity_unix_ms = current_unix_timestamp_ms();
        terminate_subprocess_handle(&mut record.subprocess);
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
        for session_id in state.sessions.keys() {
            stale_ids.push(session_id.clone());
        }
        for session_id in stale_ids {
            sync_session_subprocess_state(&mut state, session_id.as_str());
        }

        let mut expired_ids = Vec::new();
        for (session_id, record) in &state.sessions {
            if record.snapshot.status != ExternalCodingAgentSessionStatus::Running {
                continue;
            }
            if now_unix_ms.saturating_sub(record.snapshot.last_activity_unix_ms) > timeout {
                expired_ids.push(session_id.clone());
            }
        }

        let mut reaped = Vec::new();
        for stale_id in expired_ids {
            if let Some(mut record) = state.sessions.remove(stale_id.as_str()) {
                record.snapshot.status = ExternalCodingAgentSessionStatus::TimedOut;
                record.snapshot.last_activity_unix_ms = now_unix_ms;
                terminate_subprocess_handle(&mut record.subprocess);
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
        sync_session_subprocess_state(&mut state, session_id);
        let event = append_event_to_session(
            &mut state,
            session_id,
            event_type,
            normalized_message,
            queue_followup,
        )?;

        if queue_followup {
            let mut subprocess_io_error = None;
            if let Some(record) = state.sessions.get_mut(session_id) {
                if let Some(subprocess) = record.subprocess.as_mut() {
                    if let Some(stdin) = subprocess.stdin.as_mut() {
                        if let Err(error) = writeln!(stdin, "{normalized_message}") {
                            subprocess_io_error = Some(format!(
                                "failed to write follow-up to subprocess stdin: {error}"
                            ));
                        } else if let Err(error) = stdin.flush() {
                            subprocess_io_error =
                                Some(format!("failed to flush subprocess stdin: {error}"));
                        }
                    } else {
                        subprocess_io_error = Some("subprocess stdin unavailable".to_string());
                    }
                }
            }

            if let Some(error_message) = subprocess_io_error {
                if let Some(record) = state.sessions.get_mut(session_id) {
                    record.snapshot.status = ExternalCodingAgentSessionStatus::Failed;
                    record.snapshot.last_activity_unix_ms = current_unix_timestamp_ms();
                    terminate_subprocess_handle(&mut record.subprocess);
                }
                let _ = append_event_to_session(
                    &mut state,
                    session_id,
                    SUBPROCESS_STDIN_ERROR_EVENT_TYPE,
                    error_message.as_str(),
                    false,
                );
                return Err(ExternalCodingAgentBridgeError::SubprocessIoError {
                    session_id: session_id.to_string(),
                    error: error_message,
                });
            }
        }

        Ok(event)
    }

    fn mark_terminal(
        &self,
        session_id: &str,
        status: ExternalCodingAgentSessionStatus,
    ) -> Result<ExternalCodingAgentSessionSnapshot, ExternalCodingAgentBridgeError> {
        let mut state = lock_or_recover(&self.inner);
        sync_session_subprocess_state(&mut state, session_id);
        let Some(record) = state.sessions.get_mut(session_id) else {
            return Err(ExternalCodingAgentBridgeError::SessionNotFound(
                session_id.to_string(),
            ));
        };
        record.snapshot.status = status;
        record.snapshot.last_activity_unix_ms = current_unix_timestamp_ms();
        terminate_subprocess_handle(&mut record.subprocess);
        Ok(record.snapshot.clone())
    }
}

fn spawn_subprocess_for_session(
    state_ref: Weak<Mutex<ExternalCodingAgentBridgeState>>,
    config: &ExternalCodingAgentSubprocessConfig,
    session_id: &str,
    workspace_id: &str,
) -> Result<ExternalCodingAgentSubprocessHandle, ExternalCodingAgentBridgeError> {
    let command_name = config.command.trim();
    if command_name.is_empty() {
        return Err(ExternalCodingAgentBridgeError::InvalidSubprocessConfig(
            "subprocess.command must be non-empty when subprocess mode is enabled".to_string(),
        ));
    }

    let mut command = Command::new(command_name);
    command.args(&config.args);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.env("TAU_WORKSPACE_ID", workspace_id);
    command.env("TAU_EXTERNAL_SESSION_ID", session_id);
    for (key, value) in &config.env {
        command.env(key, value);
    }

    let mut child =
        command.spawn().map_err(
            |error| ExternalCodingAgentBridgeError::SubprocessSpawnFailed {
                workspace_id: workspace_id.to_string(),
                error: error.to_string(),
            },
        )?;

    if let Some(stdout) = child.stdout.take() {
        spawn_subprocess_output_reader(
            state_ref.clone(),
            session_id.to_string(),
            SUBPROCESS_STDOUT_EVENT_TYPE.to_string(),
            stdout,
        );
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_subprocess_output_reader(
            state_ref,
            session_id.to_string(),
            SUBPROCESS_STDERR_EVENT_TYPE.to_string(),
            stderr,
        );
    }

    let stdin = child.stdin.take();
    Ok(ExternalCodingAgentSubprocessHandle { child, stdin })
}

fn spawn_subprocess_output_reader<R>(
    state_ref: Weak<Mutex<ExternalCodingAgentBridgeState>>,
    session_id: String,
    event_type: String,
    reader: R,
) where
    R: Read + Send + 'static,
{
    let _ = thread::Builder::new()
        .name(format!("tau-external-coding-agent-{event_type}"))
        .spawn(move || {
            let mut buffered = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match buffered.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Some(state_arc) = state_ref.upgrade() {
                            let mut state = lock_or_recover(&state_arc);
                            let _ = append_event_to_session(
                                &mut state,
                                session_id.as_str(),
                                event_type.as_str(),
                                trimmed,
                                false,
                            );
                        } else {
                            break;
                        }
                    }
                    Err(error) => {
                        if let Some(state_arc) = state_ref.upgrade() {
                            let mut state = lock_or_recover(&state_arc);
                            let _ = append_event_to_session(
                                &mut state,
                                session_id.as_str(),
                                SUBPROCESS_RUNTIME_ERROR_EVENT_TYPE,
                                format!("{event_type} reader failed: {error}").as_str(),
                                false,
                            );
                        }
                        break;
                    }
                }
            }
        });
}

fn sync_session_subprocess_state(state: &mut ExternalCodingAgentBridgeState, session_id: &str) {
    let mut transition: Option<(ExternalCodingAgentSessionStatus, String)> = None;
    if let Some(record) = state.sessions.get_mut(session_id) {
        if record.snapshot.status != ExternalCodingAgentSessionStatus::Running {
            if record.subprocess.is_some() {
                terminate_subprocess_handle(&mut record.subprocess);
            }
            return;
        }

        if let Some(subprocess) = record.subprocess.as_mut() {
            match subprocess.child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        transition = Some((
                            ExternalCodingAgentSessionStatus::Completed,
                            "subprocess exited successfully".to_string(),
                        ));
                    } else {
                        transition = Some((
                            ExternalCodingAgentSessionStatus::Failed,
                            format!("subprocess exited with status {status}"),
                        ));
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    transition = Some((
                        ExternalCodingAgentSessionStatus::Failed,
                        format!("subprocess runtime polling failed: {error}"),
                    ));
                }
            }
        }
    }

    if let Some((status, message)) = transition {
        if let Some(record) = state.sessions.get_mut(session_id) {
            record.snapshot.status = status;
            record.snapshot.last_activity_unix_ms = current_unix_timestamp_ms();
            terminate_subprocess_handle(&mut record.subprocess);
        }
        let _ = append_event_to_session(
            state,
            session_id,
            SUBPROCESS_EXIT_EVENT_TYPE,
            message.as_str(),
            false,
        );
    }
}

fn append_event_to_session(
    state: &mut ExternalCodingAgentBridgeState,
    session_id: &str,
    event_type: &str,
    message: &str,
    queue_followup: bool,
) -> Result<ExternalCodingAgentProgressEvent, ExternalCodingAgentBridgeError> {
    let normalized_message = message.trim();
    if normalized_message.is_empty() {
        return Err(ExternalCodingAgentBridgeError::InvalidMessage);
    }
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

fn terminate_subprocess_handle(subprocess: &mut Option<ExternalCodingAgentSubprocessHandle>) {
    if let Some(mut handle) = subprocess.take() {
        handle.stdin.take();
        let _ = handle.child.kill();
        let _ = handle.child.wait();
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
    #[cfg(unix)]
    use std::collections::BTreeMap;
    #[cfg(unix)]
    use std::process::Command;
    #[cfg(unix)]
    use std::time::{Duration, Instant};
    #[cfg(unix)]
    use tempfile::tempdir;

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
            subprocess: None,
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

    #[cfg(unix)]
    fn subprocess_test_config(
        pid_file: Option<&std::path::Path>,
    ) -> ExternalCodingAgentBridgeConfig {
        let mut env = BTreeMap::new();
        if let Some(path) = pid_file {
            env.insert("TAU_TEST_PID_FILE".to_string(), path.display().to_string());
        }
        ExternalCodingAgentBridgeConfig {
            inactivity_timeout_ms: 1_000,
            max_active_sessions: 8,
            max_events_per_session: 256,
            subprocess: Some(ExternalCodingAgentSubprocessConfig {
                command: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "if [ -n \"$TAU_TEST_PID_FILE\" ]; then echo $$ > \"$TAU_TEST_PID_FILE\"; fi; \
                     echo boot; \
                     while IFS= read -r line; do \
                       echo out:$line; \
                       echo err:$line 1>&2; \
                       if [ \"$line\" = \"__exit__\" ]; then exit 0; fi; \
                     done"
                        .to_string(),
                ],
                env,
            }),
        }
    }

    #[cfg(unix)]
    fn subprocess_long_running_config(
        pid_file: &std::path::Path,
    ) -> ExternalCodingAgentBridgeConfig {
        let mut env = BTreeMap::new();
        env.insert(
            "TAU_TEST_PID_FILE".to_string(),
            pid_file.display().to_string(),
        );
        ExternalCodingAgentBridgeConfig {
            inactivity_timeout_ms: 1_000,
            max_active_sessions: 8,
            max_events_per_session: 64,
            subprocess: Some(ExternalCodingAgentSubprocessConfig {
                command: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    "echo $$ > \"$TAU_TEST_PID_FILE\"; while true; do sleep 1; done".to_string(),
                ],
                env,
            }),
        }
    }

    #[cfg(unix)]
    fn wait_for_event_message(
        bridge: &ExternalCodingAgentBridge,
        session_id: &str,
        needle: &str,
    ) -> Vec<ExternalCodingAgentProgressEvent> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let events = bridge
                .poll_events(session_id, None, 256)
                .expect("poll subprocess events");
            if events.iter().any(|event| event.message.contains(needle)) {
                return events;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for event containing '{needle}'"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn wait_until<F>(timeout: Duration, predicate: F)
    where
        F: Fn() -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if predicate() {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for condition");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn process_is_running(pid: u32) -> bool {
        let probe = format!("kill -0 {pid} >/dev/null 2>&1");
        Command::new("/bin/sh")
            .args(["-c", probe.as_str()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(unix)]
    #[test]
    fn spec_2647_c01_c02_subprocess_launches_once_and_reuses_workspace_session() {
        let bridge = ExternalCodingAgentBridge::new(subprocess_test_config(None));
        let opened = bridge
            .open_or_reuse_session("workspace-subprocess")
            .expect("open subprocess session");
        assert_eq!(opened.status, ExternalCodingAgentSessionStatus::Running);

        let events = wait_for_event_message(&bridge, opened.session_id.as_str(), "boot");
        let boot_count = events
            .iter()
            .filter(|event| event.message.contains("boot"))
            .count();
        assert_eq!(boot_count, 1);

        let reused = bridge
            .open_or_reuse_session("workspace-subprocess")
            .expect("reuse subprocess session");
        assert_eq!(reused.session_id, opened.session_id);
        assert_eq!(bridge.active_session_count(), 1);

        bridge
            .close_session(opened.session_id.as_str())
            .expect("close subprocess session");
    }

    #[cfg(unix)]
    #[test]
    fn spec_2647_c03_c04_followup_forwards_to_stdin_and_streams_stdout_stderr_events() {
        let bridge = ExternalCodingAgentBridge::new(subprocess_test_config(None));
        let session = bridge
            .open_or_reuse_session("workspace-followup")
            .expect("open subprocess session");
        wait_for_event_message(&bridge, session.session_id.as_str(), "boot");

        bridge
            .queue_followup(session.session_id.as_str(), "hello-world")
            .expect("queue followup");
        let events =
            wait_for_event_message(&bridge, session.session_id.as_str(), "out:hello-world");
        assert!(events
            .iter()
            .any(|event| event.message.contains("err:hello-world")));

        let followups = bridge
            .take_followups(session.session_id.as_str(), 16)
            .expect("drain followups");
        assert_eq!(followups, vec!["hello-world".to_string()]);

        bridge
            .queue_followup(session.session_id.as_str(), "__exit__")
            .expect("queue exit followup");
        let _ = wait_for_event_message(&bridge, session.session_id.as_str(), "out:__exit__");

        bridge
            .close_session(session.session_id.as_str())
            .expect("close subprocess session");
    }

    #[cfg(unix)]
    #[test]
    fn spec_2647_c05_close_session_terminates_subprocess_worker() {
        let temp = tempdir().expect("tempdir");
        let pid_path = temp.path().join("external-worker.pid");
        let bridge =
            ExternalCodingAgentBridge::new(subprocess_test_config(Some(pid_path.as_path())));
        let session = bridge
            .open_or_reuse_session("workspace-close")
            .expect("open subprocess session");
        wait_until(Duration::from_secs(2), || pid_path.exists());

        let pid_raw = std::fs::read_to_string(pid_path.as_path()).expect("read pid file");
        let pid: u32 = pid_raw.trim().parse().expect("parse pid");
        assert!(process_is_running(pid));

        bridge
            .close_session(session.session_id.as_str())
            .expect("close subprocess session");

        wait_until(Duration::from_secs(2), || !process_is_running(pid));
    }

    #[cfg(unix)]
    #[test]
    fn spec_2647_c06_regression_reaper_terminates_stale_subprocess_worker() {
        let temp = tempdir().expect("tempdir");
        let pid_path = temp.path().join("external-worker-reaper.pid");
        let bridge =
            ExternalCodingAgentBridge::new(subprocess_long_running_config(pid_path.as_path()));
        let session = bridge
            .open_or_reuse_session("workspace-reaper")
            .expect("open subprocess session");
        wait_until(Duration::from_secs(2), || pid_path.exists());

        let pid_raw = std::fs::read_to_string(pid_path.as_path()).expect("read pid file");
        let pid: u32 = pid_raw.trim().parse().expect("parse pid");
        assert!(process_is_running(pid));

        let reaped = bridge.reap_inactive_sessions(session.last_activity_unix_ms + 1_001);
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].session_id, session.session_id);
        assert_eq!(reaped[0].status, ExternalCodingAgentSessionStatus::TimedOut);

        wait_until(Duration::from_secs(2), || !process_is_running(pid));
    }
}
