//! Training algorithm abstractions and APO implementation.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tau_training_store::TrainingStore;
use tau_training_types::ResourcesUpdate;

mod adapters;
mod apo;
mod gae;
mod ppo;
mod safety_penalty_calibration;

pub use adapters::{
    SpansToMessages, SpansToTrajectories, SpansToTriplets, TraceAdapter, TrajectoryPaddingMode,
    TrajectoryWindowPolicy,
};
pub use apo::{ApoAlgorithm, ApoConfig, ApoTemplates, PromptEvaluator, VersionedPrompt};
pub use gae::{compute_gae_batch_from_slices, compute_gae_batch_from_trajectory, GaeConfig};
pub use ppo::{
    compute_ppo_loss, compute_ppo_update, PpoConfig, PpoLossBreakdown, PpoOptimizerStep, PpoSample,
    PpoUpdateSummary,
};
pub use safety_penalty_calibration::{
    calibrate_safety_penalty_grid, select_default_safety_penalty_coefficient,
    SafetyPenaltyCalibrationObservation, SafetyPenaltyCalibrationPolicy,
    SafetyPenaltyCalibrationReport, SafetyPenaltyCalibrationSelection,
};

/// Input/output example used by prompt-oriented training algorithms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptExample {
    pub input: String,
    pub expected: String,
}

impl PromptExample {
    /// Creates a prompt example from input and expected output text.
    pub fn new(input: impl Into<String>, expected: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            expected: expected.into(),
        }
    }
}

/// Context passed to algorithm executions.
pub struct AlgorithmContext {
    pub store: Arc<dyn TrainingStore>,
    pub seed_prompt: String,
    pub train_examples: Vec<PromptExample>,
    pub validation_examples: Vec<PromptExample>,
}

impl AlgorithmContext {
    /// Creates an algorithm context with explicit datasets.
    pub fn new(
        store: Arc<dyn TrainingStore>,
        seed_prompt: impl Into<String>,
        train_examples: Vec<PromptExample>,
        validation_examples: Vec<PromptExample>,
    ) -> Self {
        Self {
            store,
            seed_prompt: seed_prompt.into(),
            train_examples,
            validation_examples,
        }
    }
}

/// Execution summary returned by training algorithms.
#[derive(Debug, Clone)]
pub struct AlgorithmRunSummary {
    pub algorithm_name: String,
    pub rounds_completed: usize,
    pub best_prompt: Option<VersionedPrompt>,
    pub resource_updates: Vec<ResourcesUpdate>,
    pub beam_history: Vec<VersionedPrompt>,
}

/// Core algorithm trait used by the trainer stack.
#[async_trait]
pub trait Algorithm: Send + Sync {
    async fn run(&self, ctx: AlgorithmContext) -> Result<AlgorithmRunSummary>;
}
