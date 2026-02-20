//! Staged multi-process runtime contracts and supervisor primitives.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

/// Enumerates staged process roles for multi-process runtime migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcessType {
    Channel,
    Branch,
    Worker,
    Compactor,
    Cortex,
}

impl ProcessType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Branch => "branch",
            Self::Worker => "worker",
            Self::Compactor => "compactor",
            Self::Cortex => "cortex",
        }
    }
}

/// Runtime defaults for a staged process role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRuntimeProfile {
    pub process_type: ProcessType,
    pub system_prompt: String,
    pub max_turns: usize,
    pub max_context_messages: Option<usize>,
    pub tool_allowlist: Vec<String>,
}

impl ProcessRuntimeProfile {
    pub fn for_type(process_type: ProcessType) -> Self {
        match process_type {
            ProcessType::Channel => Self {
                process_type,
                system_prompt: "You are the channel coordinator process.".to_string(),
                max_turns: 8,
                max_context_messages: Some(256),
                tool_allowlist: vec![
                    "branch".to_string(),
                    "worker".to_string(),
                    "memory_search".to_string(),
                    "memory_write".to_string(),
                    "react".to_string(),
                    "send_file".to_string(),
                ],
            },
            ProcessType::Branch => Self {
                process_type,
                system_prompt: "You are the branch reasoning process.".to_string(),
                max_turns: 12,
                max_context_messages: Some(160),
                tool_allowlist: vec!["memory_search".to_string(), "memory_write".to_string()],
            },
            ProcessType::Worker => Self {
                process_type,
                system_prompt: "You are the worker execution process.".to_string(),
                max_turns: 25,
                max_context_messages: Some(96),
                tool_allowlist: vec!["memory_search".to_string(), "memory_write".to_string()],
            },
            ProcessType::Compactor => Self {
                process_type,
                system_prompt: "You are the context compactor process.".to_string(),
                max_turns: 4,
                max_context_messages: Some(128),
                tool_allowlist: vec!["memory_search".to_string(), "memory_write".to_string()],
            },
            ProcessType::Cortex => Self {
                process_type,
                system_prompt: "You are the cross-session cortex observer process.".to_string(),
                max_turns: 6,
                max_context_messages: Some(192),
                tool_allowlist: vec!["memory_search".to_string(), "memory_write".to_string()],
            },
        }
    }
}

/// Spawn-time process metadata for supervisor lifecycle tracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpawnSpec {
    pub process_id: String,
    pub process_type: ProcessType,
    pub parent_process_id: Option<String>,
    pub session_key: Option<String>,
    pub runtime_profile: ProcessRuntimeProfile,
}

impl ProcessSpawnSpec {
    pub fn new(process_id: impl Into<String>, process_type: ProcessType) -> Self {
        Self {
            process_id: process_id.into(),
            process_type,
            parent_process_id: None,
            session_key: None,
            runtime_profile: ProcessRuntimeProfile::for_type(process_type),
        }
    }

    pub fn with_parent_process_id(mut self, parent_process_id: impl Into<String>) -> Self {
        self.parent_process_id = Some(parent_process_id.into());
        self
    }

    pub fn with_session_key(mut self, session_key: impl Into<String>) -> Self {
        self.session_key = Some(session_key.into());
        self
    }

    pub fn with_runtime_profile(mut self, runtime_profile: ProcessRuntimeProfile) -> Self {
        self.runtime_profile = runtime_profile;
        self
    }
}

/// Lifecycle phases tracked by the staged process supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessLifecycleState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Immutable lifecycle snapshot emitted by `ProcessManager`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub process_id: String,
    pub process_type: ProcessType,
    pub parent_process_id: Option<String>,
    pub session_key: Option<String>,
    pub state: ProcessLifecycleState,
    pub started_unix_ms: Option<u64>,
    pub finished_unix_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProcessManagerError {
    #[error("process id must be non-empty")]
    MissingProcessId,
    #[error("process id '{0}' is already registered")]
    DuplicateProcessId(String),
}

#[derive(Debug, Clone, Default)]
pub struct ProcessManager {
    inner: Arc<ProcessManagerInner>,
}

#[derive(Debug, Default)]
struct ProcessManagerInner {
    snapshots: Mutex<HashMap<String, ProcessSnapshot>>,
}

impl ProcessManager {
    pub fn spawn_supervised<F, Fut>(
        &self,
        mut spec: ProcessSpawnSpec,
        runner: F,
    ) -> Result<tokio::task::JoinHandle<()>, ProcessManagerError>
    where
        F: FnOnce(ProcessSpawnSpec) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        spec.process_id = spec.process_id.trim().to_string();
        if spec.process_id.is_empty() {
            return Err(ProcessManagerError::MissingProcessId);
        }

        {
            let mut snapshots = lock_or_recover(&self.inner.snapshots);
            if snapshots.contains_key(spec.process_id.as_str()) {
                return Err(ProcessManagerError::DuplicateProcessId(spec.process_id));
            }
            snapshots.insert(
                spec.process_id.clone(),
                ProcessSnapshot {
                    process_id: spec.process_id.clone(),
                    process_type: spec.process_type,
                    parent_process_id: spec.parent_process_id.clone(),
                    session_key: spec.session_key.clone(),
                    state: ProcessLifecycleState::Running,
                    started_unix_ms: Some(current_unix_timestamp_ms()),
                    finished_unix_ms: None,
                    error: None,
                },
            );
        }

        let process_id = spec.process_id.clone();
        let manager = self.clone();
        Ok(tokio::spawn(async move {
            let result = runner(spec).await;
            manager.complete_process(process_id.as_str(), result);
        }))
    }

    pub fn snapshot(&self, process_id: &str) -> Option<ProcessSnapshot> {
        let snapshots = lock_or_recover(&self.inner.snapshots);
        snapshots.get(process_id).cloned()
    }

    pub fn snapshots(&self) -> Vec<ProcessSnapshot> {
        let snapshots = lock_or_recover(&self.inner.snapshots);
        let mut values = snapshots.values().cloned().collect::<Vec<_>>();
        values.sort_by(|left, right| left.process_id.cmp(&right.process_id));
        values
    }

    pub fn cancel(&self, process_id: &str) -> bool {
        let mut snapshots = lock_or_recover(&self.inner.snapshots);
        let Some(snapshot) = snapshots.get_mut(process_id) else {
            return false;
        };
        if matches!(
            snapshot.state,
            ProcessLifecycleState::Completed
                | ProcessLifecycleState::Failed
                | ProcessLifecycleState::Cancelled
        ) {
            return false;
        }
        snapshot.state = ProcessLifecycleState::Cancelled;
        snapshot.finished_unix_ms = Some(current_unix_timestamp_ms());
        snapshot.error = None;
        true
    }

    fn complete_process(&self, process_id: &str, result: Result<(), String>) {
        let mut snapshots = lock_or_recover(&self.inner.snapshots);
        let Some(snapshot) = snapshots.get_mut(process_id) else {
            return;
        };
        if snapshot.state == ProcessLifecycleState::Cancelled {
            return;
        }
        match result {
            Ok(()) => {
                snapshot.state = ProcessLifecycleState::Completed;
                snapshot.error = None;
            }
            Err(error) => {
                snapshot.state = ProcessLifecycleState::Failed;
                snapshot.error = Some(error);
            }
        }
        snapshot.finished_unix_ms = Some(current_unix_timestamp_ms());
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
