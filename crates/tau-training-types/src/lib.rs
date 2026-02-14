//! Shared data types for Tau training pipelines.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Error returned when a status transition is invalid.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StatusTransitionError {
    #[error("invalid {kind} transition: {from:?} -> {to:?}")]
    Invalid {
        kind: &'static str,
        from: String,
        to: String,
    },
}

/// Lifecycle state for a rollout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RolloutStatus {
    #[default]
    Queuing,
    Preparing,
    Running,
    Failed,
    Succeeded,
    Cancelled,
    Requeuing,
}

impl RolloutStatus {
    /// Returns true when this status can transition to `next`.
    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }

        matches!(
            (self, next),
            (Self::Queuing, Self::Preparing)
                | (Self::Queuing, Self::Running)
                | (Self::Queuing, Self::Cancelled)
                | (Self::Queuing, Self::Requeuing)
                | (Self::Preparing, Self::Running)
                | (Self::Preparing, Self::Failed)
                | (Self::Preparing, Self::Cancelled)
                | (Self::Preparing, Self::Requeuing)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Cancelled)
                | (Self::Running, Self::Requeuing)
                | (Self::Failed, Self::Requeuing)
                | (Self::Failed, Self::Cancelled)
                | (Self::Requeuing, Self::Preparing)
                | (Self::Requeuing, Self::Running)
                | (Self::Requeuing, Self::Cancelled)
        )
    }

    /// Returns an error if transitioning to `next` is not allowed.
    pub fn ensure_transition(self, next: Self) -> Result<(), StatusTransitionError> {
        if self.can_transition_to(next) {
            return Ok(());
        }

        Err(StatusTransitionError::Invalid {
            kind: "rollout_status",
            from: format!("{self:?}"),
            to: format!("{next:?}"),
        })
    }

    /// Returns true when no further execution is expected.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Failed | Self::Succeeded | Self::Cancelled)
    }
}

/// Lifecycle state for a rollout attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Preparing,
    Running,
    Failed,
    Succeeded,
    Unresponsive,
    Timeout,
}

impl AttemptStatus {
    /// Returns true when this status can transition to `next`.
    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }

        matches!(
            (self, next),
            (Self::Preparing, Self::Running)
                | (Self::Preparing, Self::Failed)
                | (Self::Preparing, Self::Succeeded)
                | (Self::Preparing, Self::Timeout)
                | (Self::Preparing, Self::Unresponsive)
                | (Self::Running, Self::Failed)
                | (Self::Running, Self::Succeeded)
                | (Self::Running, Self::Timeout)
                | (Self::Running, Self::Unresponsive)
        )
    }

    /// Returns an error if transitioning to `next` is not allowed.
    pub fn ensure_transition(self, next: Self) -> Result<(), StatusTransitionError> {
        if self.can_transition_to(next) {
            return Ok(());
        }

        Err(StatusTransitionError::Invalid {
            kind: "attempt_status",
            from: format!("{self:?}"),
            to: format!("{next:?}"),
        })
    }

    /// Returns true when no further attempt work is expected.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Succeeded | Self::Unresponsive | Self::Timeout
        )
    }
}

/// Rollout mode used by training/eval workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RolloutMode {
    Train,
    Val,
    Test,
}

/// Configuration attached to a rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolloutConfig {
    /// Maximum number of attempts before the rollout is marked failed.
    pub max_attempts: u32,
    /// Arbitrary run-level settings.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl Default for RolloutConfig {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            metadata: HashMap::new(),
        }
    }
}

/// Work unit queued for a training runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rollout {
    pub rollout_id: String,
    pub input: Value,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub mode: Option<RolloutMode>,
    pub status: RolloutStatus,
    pub config: RolloutConfig,
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
    pub assigned_worker_id: Option<String>,
    pub attempt_count: u32,
}

impl Rollout {
    /// Creates a rollout in the queuing state.
    pub fn new(rollout_id: impl Into<String>, input: Value, mode: Option<RolloutMode>) -> Self {
        Self {
            rollout_id: rollout_id.into(),
            input,
            start_time: None,
            end_time: None,
            mode,
            status: RolloutStatus::Queuing,
            config: RolloutConfig::default(),
            metadata: HashMap::new(),
            assigned_worker_id: None,
            attempt_count: 0,
        }
    }
}

/// Single execution attempt of a rollout by a worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub attempt_id: String,
    pub rollout_id: String,
    pub sequence_id: u32,
    pub worker_id: String,
    pub status: AttemptStatus,
    pub started_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

impl Attempt {
    /// Creates a new attempt in the preparing state.
    pub fn new(
        attempt_id: impl Into<String>,
        rollout_id: impl Into<String>,
        sequence_id: u32,
        worker_id: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            attempt_id: attempt_id.into(),
            rollout_id: rollout_id.into(),
            sequence_id,
            worker_id: worker_id.into(),
            status: AttemptStatus::Preparing,
            started_at: now,
            last_heartbeat_at: now,
            ended_at: None,
            error_message: None,
        }
    }
}

/// Event attached to a training span.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub attributes: HashMap<String, Value>,
}

/// Structured execution span captured during rollout execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingSpan {
    pub rollout_id: String,
    pub attempt_id: String,
    pub sequence_id: u64,
    pub trace_id: String,
    pub span_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub attributes: HashMap<String, Value>,
    #[serde(default)]
    pub events: Vec<SpanEvent>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
}

impl TrainingSpan {
    /// Creates a new span with the current timestamp.
    pub fn new(
        rollout_id: impl Into<String>,
        attempt_id: impl Into<String>,
        sequence_id: u64,
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        parent_id: Option<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            rollout_id: rollout_id.into(),
            attempt_id: attempt_id.into(),
            sequence_id,
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_id,
            name: name.into(),
            attributes: HashMap::new(),
            events: Vec::new(),
            start_time: Utc::now(),
            end_time: None,
        }
    }
}

/// Scalar reward value emitted by a rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reward {
    pub name: String,
    pub value: f64,
}

impl Reward {
    /// Creates a scalar reward.
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

/// Immutable resource snapshot with version tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourcesUpdate {
    pub resources_id: String,
    pub version: u64,
    pub resources: HashMap<String, Value>,
    pub created_time: DateTime<Utc>,
    pub is_latest: bool,
}

/// Prompt/response/reward tuple extracted from spans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triplet {
    pub prompt: Value,
    pub response: Value,
    pub reward: Option<f64>,
}

/// Filter used when listing rollouts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RolloutQuery {
    pub statuses: Option<Vec<RolloutStatus>>,
    pub mode: Option<RolloutMode>,
    pub ids: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub offset: usize,
}

/// Worker registration and heartbeat state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerState {
    pub worker_id: String,
    pub registered_at: DateTime<Utc>,
    pub last_heartbeat_at: DateTime<Utc>,
    pub active_rollout_id: Option<String>,
    pub active_attempt_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{AttemptStatus, RolloutStatus};

    #[test]
    fn rollout_transitions_enforce_terminal_states() {
        assert!(RolloutStatus::Queuing.can_transition_to(RolloutStatus::Running));
        assert!(RolloutStatus::Running.can_transition_to(RolloutStatus::Succeeded));
        assert!(!RolloutStatus::Succeeded.can_transition_to(RolloutStatus::Running));
        assert!(!RolloutStatus::Cancelled.can_transition_to(RolloutStatus::Requeuing));
    }

    #[test]
    fn attempt_transitions_allow_runtime_outcomes() {
        assert!(AttemptStatus::Preparing.can_transition_to(AttemptStatus::Running));
        assert!(AttemptStatus::Running.can_transition_to(AttemptStatus::Succeeded));
        assert!(AttemptStatus::Running.can_transition_to(AttemptStatus::Timeout));
        assert!(!AttemptStatus::Succeeded.can_transition_to(AttemptStatus::Running));
    }
}
