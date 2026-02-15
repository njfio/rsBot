//! Shared data types for Tau training pipelines.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Current schema version used by RL core payload types.
///
/// Migration guarantee:
/// - payloads that omit `schema_version` default to this version
/// - unknown versions fail closed during `validate()`
pub const RL_SCHEMA_VERSION_V1: u32 = 1;

fn default_rl_schema_version() -> u32 {
    RL_SCHEMA_VERSION_V1
}

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

/// Validation error for RL trajectory, advantage, and checkpoint payloads.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RlSchemaError {
    #[error("{type_name} unsupported schema version {found} (supported versions: {supported:?})")]
    UnsupportedSchemaVersion {
        type_name: &'static str,
        found: u32,
        supported: Vec<u32>,
    },
    #[error("{type_name} requires non-empty field '{field}'")]
    EmptyField {
        type_name: &'static str,
        field: &'static str,
    },
    #[error("{type_name} field '{field}' must be in range [0.0, 1.0], found {value}")]
    OutOfRange {
        type_name: &'static str,
        field: &'static str,
        value: f64,
    },
    #[error(
        "{type_name} length mismatch: '{left_field}'={left_len} does not match '{right_field}'={right_len}"
    )]
    LengthMismatch {
        type_name: &'static str,
        left_field: &'static str,
        left_len: usize,
        right_field: &'static str,
        right_len: usize,
    },
    #[error(
        "{type_name} expected step_index={expected} but found step_index={found} at position {position}"
    )]
    StepIndexMismatch {
        type_name: &'static str,
        expected: u32,
        found: u32,
        position: usize,
    },
    #[error("{type_name} field '{field}' must contain finite f64 values")]
    NonFinite {
        type_name: &'static str,
        field: &'static str,
    },
}

/// Cross-payload validation error for RL bundle conformance.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RlBundleError {
    #[error("trajectory payload failed schema validation: {source}")]
    TrajectorySchema { source: RlSchemaError },
    #[error("advantage payload failed schema validation: {source}")]
    AdvantageSchema { source: RlSchemaError },
    #[error("checkpoint payload failed schema validation: {source}")]
    CheckpointSchema { source: RlSchemaError },
    #[error(
        "trajectory_id mismatch: trajectory.trajectory_id='{trajectory_id}' advantages.trajectory_id='{advantages_trajectory_id}'"
    )]
    TrajectoryIdMismatch {
        trajectory_id: String,
        advantages_trajectory_id: String,
    },
    #[error(
        "trajectory/advantage length mismatch: trajectory.steps={step_count} advantages={advantage_count}"
    )]
    StepAdvantageLengthMismatch {
        step_count: usize,
        advantage_count: usize,
    },
    #[error(
        "checkpoint progression mismatch: checkpoint.global_step={global_step} trajectory.steps={step_count}"
    )]
    CheckpointGlobalStepMismatch { global_step: u64, step_count: usize },
}

/// Checkpoint lineage validation/query error.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CheckpointLineageError {
    #[error("checkpoint '{checkpoint_id}' failed schema validation: {source}")]
    CheckpointSchema {
        checkpoint_id: String,
        source: RlSchemaError,
    },
    #[error("duplicate checkpoint_id '{checkpoint_id}' in lineage input")]
    DuplicateCheckpointId { checkpoint_id: String },
    #[error("unknown leaf checkpoint '{checkpoint_id}'")]
    UnknownLeafCheckpoint { checkpoint_id: String },
    #[error(
        "missing parent checkpoint '{parent_checkpoint_id}' referenced by checkpoint '{checkpoint_id}'"
    )]
    MissingParentCheckpoint {
        checkpoint_id: String,
        parent_checkpoint_id: String,
    },
    #[error("lineage cycle detected while traversing checkpoint '{checkpoint_id}'")]
    LineageCycleDetected { checkpoint_id: String },
}

fn ensure_supported_rl_schema(type_name: &'static str, version: u32) -> Result<(), RlSchemaError> {
    if version == RL_SCHEMA_VERSION_V1 {
        return Ok(());
    }
    Err(RlSchemaError::UnsupportedSchemaVersion {
        type_name,
        found: version,
        supported: vec![RL_SCHEMA_VERSION_V1],
    })
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

/// Single state-action-reward transition used for RL policy updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrajectoryStep {
    /// Monotonic per-trajectory index.
    pub step_index: u32,
    /// Observation provided to the policy before action selection.
    pub observation: Value,
    /// Action selected by the policy.
    pub action: Value,
    /// Scalar reward received after executing the action.
    pub reward: f64,
    /// Marks terminal transition for an episode.
    pub done: bool,
    /// Optional policy log-probability of the action.
    pub logprob: Option<f64>,
    /// Optional value estimate at this step.
    pub value_estimate: Option<f64>,
    /// Optional RL-system metadata for this transition.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl TrajectoryStep {
    /// Creates a trajectory step with required RL transition fields.
    pub fn new(
        step_index: u32,
        observation: Value,
        action: Value,
        reward: f64,
        done: bool,
    ) -> Self {
        Self {
            step_index,
            observation,
            action,
            reward,
            done,
            logprob: None,
            value_estimate: None,
            metadata: HashMap::new(),
        }
    }

    /// Validates numeric fields for serialization and RL math safety.
    pub fn validate(&self) -> Result<(), RlSchemaError> {
        if !self.reward.is_finite() {
            return Err(RlSchemaError::NonFinite {
                type_name: "TrajectoryStep",
                field: "reward",
            });
        }
        if self.logprob.is_some_and(|value| !value.is_finite()) {
            return Err(RlSchemaError::NonFinite {
                type_name: "TrajectoryStep",
                field: "logprob",
            });
        }
        if self.value_estimate.is_some_and(|value| !value.is_finite()) {
            return Err(RlSchemaError::NonFinite {
                type_name: "TrajectoryStep",
                field: "value_estimate",
            });
        }
        Ok(())
    }
}

/// Full episode trajectory containing ordered RL transition steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpisodeTrajectory {
    /// Backward-compatible schema marker for serialization stability.
    #[serde(default = "default_rl_schema_version")]
    pub schema_version: u32,
    /// Stable unique identifier for this trajectory.
    pub trajectory_id: String,
    /// Optional rollout correlation id.
    pub rollout_id: Option<String>,
    /// Optional episode identifier from task/dataset.
    pub episode_id: Option<String>,
    /// Ordered state-action transitions.
    pub steps: Vec<TrajectoryStep>,
    /// Discount factor used when computing returns/advantages.
    pub discount_factor: f64,
    /// Total return for convenience indexing.
    pub total_return: f64,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Optional RL-system metadata.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl EpisodeTrajectory {
    /// Creates a trajectory with schema defaults and precomputed total return.
    pub fn new(
        trajectory_id: impl Into<String>,
        rollout_id: Option<String>,
        episode_id: Option<String>,
        steps: Vec<TrajectoryStep>,
    ) -> Self {
        let total_return = steps.iter().map(|step| step.reward).sum();
        Self {
            schema_version: RL_SCHEMA_VERSION_V1,
            trajectory_id: trajectory_id.into(),
            rollout_id,
            episode_id,
            steps,
            discount_factor: 0.99,
            total_return,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Validates schema version and transition ordering constraints.
    pub fn validate(&self) -> Result<(), RlSchemaError> {
        ensure_supported_rl_schema("EpisodeTrajectory", self.schema_version)?;
        if self.steps.is_empty() {
            return Err(RlSchemaError::EmptyField {
                type_name: "EpisodeTrajectory",
                field: "steps",
            });
        }
        if !(0.0..=1.0).contains(&self.discount_factor) {
            return Err(RlSchemaError::OutOfRange {
                type_name: "EpisodeTrajectory",
                field: "discount_factor",
                value: self.discount_factor,
            });
        }
        if !self.total_return.is_finite() {
            return Err(RlSchemaError::NonFinite {
                type_name: "EpisodeTrajectory",
                field: "total_return",
            });
        }
        for (position, step) in self.steps.iter().enumerate() {
            let expected = position as u32;
            if step.step_index != expected {
                return Err(RlSchemaError::StepIndexMismatch {
                    type_name: "EpisodeTrajectory",
                    expected,
                    found: step.step_index,
                    position,
                });
            }
            step.validate()?;
        }
        Ok(())
    }
}

/// Batch tensor-aligned arrays produced for policy optimization updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvantageBatch {
    /// Backward-compatible schema marker for serialization stability.
    #[serde(default = "default_rl_schema_version")]
    pub schema_version: u32,
    /// Stable identifier for this advantage batch.
    pub batch_id: String,
    /// Source trajectory id used to compute this batch.
    pub trajectory_id: String,
    /// Advantage values aligned to trajectory steps.
    pub advantages: Vec<f64>,
    /// Return targets aligned to trajectory steps.
    pub returns: Vec<f64>,
    /// Optional value targets aligned to trajectory steps.
    #[serde(default)]
    pub value_targets: Vec<f64>,
    /// Indicates whether advantages are normalized.
    pub normalized: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Optional RL-system metadata.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl AdvantageBatch {
    /// Creates an advantage batch with schema defaults.
    pub fn new(
        batch_id: impl Into<String>,
        trajectory_id: impl Into<String>,
        advantages: Vec<f64>,
        returns: Vec<f64>,
    ) -> Self {
        Self {
            schema_version: RL_SCHEMA_VERSION_V1,
            batch_id: batch_id.into(),
            trajectory_id: trajectory_id.into(),
            advantages,
            returns,
            value_targets: Vec::new(),
            normalized: false,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Validates schema version, vector alignment, and numeric safety.
    pub fn validate(&self) -> Result<(), RlSchemaError> {
        ensure_supported_rl_schema("AdvantageBatch", self.schema_version)?;
        if self.advantages.is_empty() {
            return Err(RlSchemaError::EmptyField {
                type_name: "AdvantageBatch",
                field: "advantages",
            });
        }
        if self.advantages.len() != self.returns.len() {
            return Err(RlSchemaError::LengthMismatch {
                type_name: "AdvantageBatch",
                left_field: "advantages",
                left_len: self.advantages.len(),
                right_field: "returns",
                right_len: self.returns.len(),
            });
        }
        if !self.value_targets.is_empty() && self.value_targets.len() != self.advantages.len() {
            return Err(RlSchemaError::LengthMismatch {
                type_name: "AdvantageBatch",
                left_field: "value_targets",
                left_len: self.value_targets.len(),
                right_field: "advantages",
                right_len: self.advantages.len(),
            });
        }
        if self.advantages.iter().any(|value| !value.is_finite()) {
            return Err(RlSchemaError::NonFinite {
                type_name: "AdvantageBatch",
                field: "advantages",
            });
        }
        if self.returns.iter().any(|value| !value.is_finite()) {
            return Err(RlSchemaError::NonFinite {
                type_name: "AdvantageBatch",
                field: "returns",
            });
        }
        if self.value_targets.iter().any(|value| !value.is_finite()) {
            return Err(RlSchemaError::NonFinite {
                type_name: "AdvantageBatch",
                field: "value_targets",
            });
        }
        Ok(())
    }
}

/// Versioned checkpoint descriptor for persisted RL policy artifacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointRecord {
    /// Backward-compatible schema marker for serialization stability.
    #[serde(default = "default_rl_schema_version")]
    pub schema_version: u32,
    /// Stable checkpoint identifier.
    pub checkpoint_id: String,
    /// RL algorithm name (for example ppo).
    pub algorithm: String,
    /// Policy version string.
    pub policy_version: String,
    /// Global optimizer/environment step at checkpoint time.
    pub global_step: u64,
    /// Number of completed episodes.
    pub episode_count: u64,
    /// Optional moving-average reward metric.
    pub mean_reward: Option<f64>,
    /// Optional URI to persisted model artifact.
    pub artifact_uri: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Optional RL-system metadata.
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl CheckpointRecord {
    /// Creates a checkpoint record with schema defaults.
    pub fn new(
        checkpoint_id: impl Into<String>,
        algorithm: impl Into<String>,
        policy_version: impl Into<String>,
        global_step: u64,
    ) -> Self {
        Self {
            schema_version: RL_SCHEMA_VERSION_V1,
            checkpoint_id: checkpoint_id.into(),
            algorithm: algorithm.into(),
            policy_version: policy_version.into(),
            global_step,
            episode_count: 0,
            mean_reward: None,
            artifact_uri: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Validates schema version and required record fields.
    pub fn validate(&self) -> Result<(), RlSchemaError> {
        ensure_supported_rl_schema("CheckpointRecord", self.schema_version)?;
        if self.algorithm.trim().is_empty() {
            return Err(RlSchemaError::EmptyField {
                type_name: "CheckpointRecord",
                field: "algorithm",
            });
        }
        if self.policy_version.trim().is_empty() {
            return Err(RlSchemaError::EmptyField {
                type_name: "CheckpointRecord",
                field: "policy_version",
            });
        }
        if self.mean_reward.is_some_and(|value| !value.is_finite()) {
            return Err(RlSchemaError::NonFinite {
                type_name: "CheckpointRecord",
                field: "mean_reward",
            });
        }
        Ok(())
    }
}

/// Combined RL payload bundle used for cross-object conformance checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RlPayloadBundle {
    pub trajectory: EpisodeTrajectory,
    pub advantages: AdvantageBatch,
    pub checkpoint: CheckpointRecord,
}

impl RlPayloadBundle {
    /// Creates a bundle from trajectory, advantage batch, and checkpoint record.
    pub fn new(
        trajectory: EpisodeTrajectory,
        advantages: AdvantageBatch,
        checkpoint: CheckpointRecord,
    ) -> Self {
        Self {
            trajectory,
            advantages,
            checkpoint,
        }
    }

    /// Validates component payload schemas and cross-payload consistency.
    pub fn validate(&self) -> Result<(), RlBundleError> {
        self.trajectory
            .validate()
            .map_err(|source| RlBundleError::TrajectorySchema { source })?;
        self.advantages
            .validate()
            .map_err(|source| RlBundleError::AdvantageSchema { source })?;
        self.checkpoint
            .validate()
            .map_err(|source| RlBundleError::CheckpointSchema { source })?;

        if self.trajectory.trajectory_id != self.advantages.trajectory_id {
            return Err(RlBundleError::TrajectoryIdMismatch {
                trajectory_id: self.trajectory.trajectory_id.clone(),
                advantages_trajectory_id: self.advantages.trajectory_id.clone(),
            });
        }

        let step_count = self.trajectory.steps.len();
        let advantage_count = self.advantages.advantages.len();
        if step_count != advantage_count {
            return Err(RlBundleError::StepAdvantageLengthMismatch {
                step_count,
                advantage_count,
            });
        }

        if self.checkpoint.global_step < step_count as u64 {
            return Err(RlBundleError::CheckpointGlobalStepMismatch {
                global_step: self.checkpoint.global_step,
                step_count,
            });
        }

        Ok(())
    }
}

/// Resolves checkpoint lineage from root to `leaf_checkpoint_id`.
///
/// Parent links are read from `CheckpointRecord.metadata["parent_checkpoint_id"]`
/// when present and non-empty.
pub fn resolve_checkpoint_lineage_path(
    records: &[CheckpointRecord],
    leaf_checkpoint_id: &str,
) -> Result<Vec<String>, CheckpointLineageError> {
    let mut by_id: HashMap<String, &CheckpointRecord> = HashMap::new();

    for record in records {
        record
            .validate()
            .map_err(|source| CheckpointLineageError::CheckpointSchema {
                checkpoint_id: record.checkpoint_id.clone(),
                source,
            })?;

        if by_id.insert(record.checkpoint_id.clone(), record).is_some() {
            return Err(CheckpointLineageError::DuplicateCheckpointId {
                checkpoint_id: record.checkpoint_id.clone(),
            });
        }
    }

    if !by_id.contains_key(leaf_checkpoint_id) {
        return Err(CheckpointLineageError::UnknownLeafCheckpoint {
            checkpoint_id: leaf_checkpoint_id.to_string(),
        });
    }

    let mut lineage_reversed = Vec::new();
    let mut visited = HashSet::new();
    let mut cursor = leaf_checkpoint_id.to_string();

    loop {
        if !visited.insert(cursor.clone()) {
            return Err(CheckpointLineageError::LineageCycleDetected {
                checkpoint_id: cursor,
            });
        }

        lineage_reversed.push(cursor.clone());
        let Some(record) = by_id.get(&cursor) else {
            return Err(CheckpointLineageError::UnknownLeafCheckpoint {
                checkpoint_id: cursor,
            });
        };

        let parent = record
            .metadata
            .get("parent_checkpoint_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let Some(parent_checkpoint_id) = parent else {
            break;
        };

        if !by_id.contains_key(&parent_checkpoint_id) {
            return Err(CheckpointLineageError::MissingParentCheckpoint {
                checkpoint_id: record.checkpoint_id.clone(),
                parent_checkpoint_id,
            });
        }
        cursor = parent_checkpoint_id;
    }

    lineage_reversed.reverse();
    Ok(lineage_reversed)
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
    use super::{
        resolve_checkpoint_lineage_path, AdvantageBatch, AttemptStatus, CheckpointRecord,
        EpisodeTrajectory, RlPayloadBundle, RolloutStatus, TrajectoryStep, RL_SCHEMA_VERSION_V1,
    };
    use serde_json::json;

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

    #[test]
    fn episode_trajectory_validation_requires_ordered_steps() {
        let mut trajectory = EpisodeTrajectory::new(
            "trajectory-1",
            Some("rollout-1".to_string()),
            Some("episode-1".to_string()),
            vec![
                TrajectoryStep::new(0, json!({"s": 0}), json!({"a": 0}), 1.0, false),
                TrajectoryStep::new(2, json!({"s": 1}), json!({"a": 1}), 1.0, true),
            ],
        );

        assert!(trajectory.validate().is_err());

        trajectory.steps[1].step_index = 1;
        assert!(trajectory.validate().is_ok());
    }

    #[test]
    fn advantage_batch_validation_requires_aligned_lengths() {
        let mut batch = AdvantageBatch::new("batch-1", "trajectory-1", vec![1.0, 0.5], vec![1.2]);
        assert!(batch.validate().is_err());

        batch.returns = vec![1.2, 0.6];
        batch.value_targets = vec![0.4, 0.2];
        assert!(batch.validate().is_ok());
    }

    #[test]
    fn checkpoint_record_validation_requires_required_fields() {
        let mut checkpoint = CheckpointRecord::new("checkpoint-1", "", "policy-v1", 10);
        assert!(checkpoint.validate().is_err());

        checkpoint.algorithm = "ppo".to_string();
        assert!(checkpoint.validate().is_ok());
    }

    #[test]
    fn regression_schema_version_defaults_for_legacy_payloads() {
        let payload = json!({
            "trajectory_id": "trajectory-legacy",
            "rollout_id": "rollout-1",
            "episode_id": "episode-1",
            "steps": [
                {
                    "step_index": 0,
                    "observation": { "state": 1 },
                    "action": { "tool": "step0" },
                    "reward": 0.5,
                    "done": false
                },
                {
                    "step_index": 1,
                    "observation": { "state": 2 },
                    "action": { "tool": "step1" },
                    "reward": 1.0,
                    "done": true
                }
            ],
            "discount_factor": 0.99,
            "total_return": 1.5,
            "created_at": "2026-02-15T00:00:00Z"
        });

        let decoded: EpisodeTrajectory =
            serde_json::from_value(payload).expect("decode trajectory");
        assert_eq!(decoded.schema_version, RL_SCHEMA_VERSION_V1);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn regression_advantage_batch_schema_version_defaults_for_legacy_payloads() {
        let payload = json!({
            "batch_id": "batch-legacy",
            "trajectory_id": "trajectory-legacy",
            "advantages": [0.5, 0.25],
            "returns": [0.6, 0.3],
            "normalized": false,
            "created_at": "2026-02-15T00:00:00Z"
        });

        let decoded: AdvantageBatch =
            serde_json::from_value(payload).expect("decode advantage batch");
        assert_eq!(decoded.schema_version, RL_SCHEMA_VERSION_V1);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn regression_checkpoint_schema_version_defaults_for_legacy_payloads() {
        let payload = json!({
            "checkpoint_id": "checkpoint-legacy",
            "algorithm": "ppo",
            "policy_version": "policy-v1",
            "global_step": 42,
            "episode_count": 3,
            "mean_reward": 1.25,
            "artifact_uri": "file:///tmp/ckpt.bin",
            "created_at": "2026-02-15T00:00:00Z"
        });

        let decoded: CheckpointRecord = serde_json::from_value(payload).expect("decode checkpoint");
        assert_eq!(decoded.schema_version, RL_SCHEMA_VERSION_V1);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn regression_unknown_schema_versions_fail_with_deterministic_reason() {
        let mut trajectory = EpisodeTrajectory::new(
            "trajectory-unknown-version",
            Some("rollout-1".to_string()),
            Some("episode-1".to_string()),
            vec![TrajectoryStep::new(
                0,
                json!({"state": 1}),
                json!({"action": "a"}),
                1.0,
                true,
            )],
        );
        trajectory.schema_version = 99;
        let trajectory_error = trajectory.validate().expect_err("unsupported schema");
        let trajectory_reason = trajectory_error.to_string();
        assert!(trajectory_reason.contains("EpisodeTrajectory"));
        assert!(trajectory_reason.contains("unsupported schema version"));

        let mut advantages = AdvantageBatch::new(
            "batch-unknown-version",
            "trajectory-1",
            vec![0.5],
            vec![1.0],
        );
        advantages.schema_version = 99;
        let advantage_error = advantages.validate().expect_err("unsupported schema");
        let advantage_reason = advantage_error.to_string();
        assert!(advantage_reason.contains("AdvantageBatch"));
        assert!(advantage_reason.contains("unsupported schema version"));

        let mut checkpoint =
            CheckpointRecord::new("checkpoint-unknown-version", "ppo", "policy-v1", 1);
        checkpoint.schema_version = 99;
        let checkpoint_error = checkpoint.validate().expect_err("unsupported schema");
        let checkpoint_reason = checkpoint_error.to_string();
        assert!(checkpoint_reason.contains("CheckpointRecord"));
        assert!(checkpoint_reason.contains("unsupported schema version"));
    }

    #[test]
    fn spec_c04_valid_rl_payload_bundle_passes_validation() {
        let bundle = valid_bundle_fixture();
        assert!(bundle.validate().is_ok());
    }

    #[test]
    fn regression_bundle_validation_rejects_trajectory_id_mismatch() {
        let mut bundle = valid_bundle_fixture();
        bundle.advantages.trajectory_id = "trajectory-other".to_string();

        let error = bundle
            .validate()
            .expect_err("trajectory id mismatch should fail");
        assert!(error.to_string().contains("trajectory_id"));
    }

    #[test]
    fn regression_bundle_validation_rejects_step_advantage_length_mismatch() {
        let mut bundle = valid_bundle_fixture();
        bundle.advantages.advantages.pop();
        bundle.advantages.returns.pop();
        bundle.advantages.value_targets.pop();

        let error = bundle
            .validate()
            .expect_err("step/advantage length mismatch should fail");
        assert!(error.to_string().contains("trajectory.steps"));
        assert!(error.to_string().contains("advantages"));
    }

    #[test]
    fn regression_bundle_validation_rejects_checkpoint_progression_mismatch() {
        let mut bundle = valid_bundle_fixture();
        bundle.checkpoint.global_step = 1;

        let error = bundle
            .validate()
            .expect_err("checkpoint progression mismatch should fail");
        assert!(error.to_string().contains("checkpoint.global_step"));
    }

    fn valid_bundle_fixture() -> RlPayloadBundle {
        let trajectory = EpisodeTrajectory::new(
            "trajectory-1",
            Some("rollout-1".to_string()),
            Some("episode-1".to_string()),
            vec![
                TrajectoryStep::new(0, json!({"state": 0}), json!({"action": "a0"}), 0.4, false),
                TrajectoryStep::new(1, json!({"state": 1}), json!({"action": "a1"}), 0.6, true),
            ],
        );
        let mut advantages =
            AdvantageBatch::new("batch-1", "trajectory-1", vec![0.4, 0.2], vec![0.8, 0.6]);
        advantages.value_targets = vec![0.5, 0.4];
        let checkpoint = CheckpointRecord::new("checkpoint-1", "ppo", "policy-v1", 2);

        RlPayloadBundle::new(trajectory, advantages, checkpoint)
    }

    #[test]
    fn spec_c01_resolve_checkpoint_lineage_path_root_to_leaf() {
        let records = vec![
            checkpoint_with_parent("checkpoint-root", None, 10),
            checkpoint_with_parent("checkpoint-mid", Some("checkpoint-root"), 20),
            checkpoint_with_parent("checkpoint-leaf", Some("checkpoint-mid"), 30),
        ];

        let path = resolve_checkpoint_lineage_path(&records, "checkpoint-leaf").expect("path");
        assert_eq!(
            path,
            vec![
                "checkpoint-root".to_string(),
                "checkpoint-mid".to_string(),
                "checkpoint-leaf".to_string(),
            ]
        );
    }

    #[test]
    fn spec_c04_reject_unknown_leaf_checkpoint() {
        let records = vec![checkpoint_with_parent("checkpoint-root", None, 10)];
        let error = resolve_checkpoint_lineage_path(&records, "checkpoint-unknown")
            .expect_err("unknown leaf should fail");
        assert!(error.to_string().contains("unknown leaf"));
    }

    #[test]
    fn spec_c02_reject_duplicate_checkpoint_ids() {
        let records = vec![
            checkpoint_with_parent("checkpoint-dup", None, 10),
            checkpoint_with_parent("checkpoint-dup", None, 20),
        ];
        let error = resolve_checkpoint_lineage_path(&records, "checkpoint-dup")
            .expect_err("duplicate ids should fail");
        assert!(error.to_string().contains("duplicate checkpoint_id"));
    }

    #[test]
    fn spec_c03_reject_missing_parent_checkpoint() {
        let records = vec![checkpoint_with_parent(
            "checkpoint-leaf",
            Some("checkpoint-missing"),
            30,
        )];
        let error = resolve_checkpoint_lineage_path(&records, "checkpoint-leaf")
            .expect_err("missing parent should fail");
        assert!(error.to_string().contains("missing parent"));
    }

    #[test]
    fn spec_c03_reject_lineage_cycle() {
        let records = vec![
            checkpoint_with_parent("checkpoint-a", Some("checkpoint-b"), 10),
            checkpoint_with_parent("checkpoint-b", Some("checkpoint-a"), 20),
        ];
        let error = resolve_checkpoint_lineage_path(&records, "checkpoint-a")
            .expect_err("lineage cycle should fail");
        assert!(error.to_string().contains("lineage cycle"));
    }

    fn checkpoint_with_parent(
        checkpoint_id: &str,
        parent_checkpoint_id: Option<&str>,
        global_step: u64,
    ) -> CheckpointRecord {
        let mut checkpoint = CheckpointRecord::new(checkpoint_id, "ppo", "policy-v1", global_step);
        if let Some(parent_checkpoint_id) = parent_checkpoint_id {
            checkpoint.metadata.insert(
                "parent_checkpoint_id".to_string(),
                json!(parent_checkpoint_id),
            );
        }
        checkpoint
    }
}
