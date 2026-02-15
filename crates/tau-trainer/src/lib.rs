//! Top-level orchestrator for rollout-based training jobs.

pub mod benchmark_fixtures;
pub mod benchmark_significance;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tau_training_runner::{RolloutExecutor, RunnerConfig, TrainingRunner};
use tau_training_store::{RolloutQuery, TrainingStore};
use tau_training_types::{Rollout, RolloutMode, RolloutStatus};
use tokio::sync::watch;
use tokio::task::JoinSet;

/// Dataset abstraction consumed by the trainer.
#[async_trait]
pub trait Dataset: Send {
    /// Converts dataset rows to rollout units for the requested mode.
    async fn into_rollouts(self, mode: RolloutMode) -> Vec<Rollout>;
}

#[async_trait]
impl Dataset for Vec<Rollout> {
    async fn into_rollouts(self, mode: RolloutMode) -> Vec<Rollout> {
        self.into_iter()
            .map(|mut rollout| {
                rollout.mode = Some(mode);
                rollout.status = RolloutStatus::Queuing;
                rollout.assigned_worker_id = None;
                rollout.start_time = None;
                rollout.end_time = None;
                rollout
            })
            .collect()
    }
}

#[async_trait]
impl Dataset for Vec<Value> {
    async fn into_rollouts(self, mode: RolloutMode) -> Vec<Rollout> {
        self.into_iter()
            .enumerate()
            .map(|(index, input)| {
                Rollout::new(
                    format!("{}-{}-{}", mode_name(mode), index + 1, next_id()),
                    input,
                    Some(mode),
                )
            })
            .collect()
    }
}

/// Runtime settings for trainer orchestration.
#[derive(Debug, Clone)]
pub struct TrainerConfig {
    pub worker_count: usize,
    pub poll_interval: Duration,
    pub heartbeat_interval: Duration,
    pub completion_poll_interval: Duration,
    pub completion_timeout: Duration,
}

impl Default for TrainerConfig {
    fn default() -> Self {
        Self {
            worker_count: 2,
            poll_interval: Duration::from_millis(50),
            heartbeat_interval: Duration::from_secs(1),
            completion_poll_interval: Duration::from_millis(60),
            completion_timeout: Duration::from_secs(30),
        }
    }
}

/// Aggregate outcome from a trainer run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrainingSummary {
    pub total_rollouts: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub cancelled: usize,
}

/// High-level orchestrator that runs workers and waits for completion.
pub struct Trainer {
    store: Arc<dyn TrainingStore>,
    config: TrainerConfig,
}

impl Trainer {
    /// Creates a trainer using the provided store backend.
    pub fn new(store: Arc<dyn TrainingStore>, config: TrainerConfig) -> Self {
        Self { store, config }
    }

    /// Runs the baseline fixed-resource training loop.
    pub async fn fit<DTrain, DVal>(
        &self,
        executor: Arc<dyn RolloutExecutor>,
        train: Option<DTrain>,
        val: Option<DVal>,
    ) -> Result<TrainingSummary>
    where
        DTrain: Dataset,
        DVal: Dataset,
    {
        let mut rollouts = Vec::new();

        if let Some(train) = train {
            rollouts.extend(train.into_rollouts(RolloutMode::Train).await);
        }
        if let Some(val) = val {
            rollouts.extend(val.into_rollouts(RolloutMode::Val).await);
        }

        if rollouts.is_empty() {
            return Ok(TrainingSummary::default());
        }

        let rollout_ids: Vec<String> = rollouts
            .iter()
            .map(|item| item.rollout_id.clone())
            .collect();
        for rollout in rollouts {
            self.store.enqueue_rollout(rollout).await?;
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut workers = JoinSet::new();

        for index in 0..self.config.worker_count.max(1) {
            let runner = TrainingRunner::new(
                self.store.clone(),
                executor.clone(),
                RunnerConfig {
                    worker_id: format!("trainer-worker-{}", index + 1),
                    poll_interval: self.config.poll_interval,
                    heartbeat_interval: self.config.heartbeat_interval,
                    reassignment_interval: self.config.poll_interval,
                    worker_timeout: self.config.heartbeat_interval * 3,
                },
            );
            let runner_shutdown = shutdown_rx.clone();
            workers.spawn(async move { runner.run(runner_shutdown).await });
        }

        let summary = self.wait_for_completion(&rollout_ids).await?;

        shutdown_tx.send(true)?;
        while let Some(joined) = workers.join_next().await {
            joined??;
        }

        Ok(summary)
    }

    async fn wait_for_completion(&self, rollout_ids: &[String]) -> Result<TrainingSummary> {
        let deadline = Instant::now() + self.config.completion_timeout;

        loop {
            let rollouts = self
                .store
                .query_rollouts(RolloutQuery {
                    ids: Some(rollout_ids.to_vec()),
                    ..RolloutQuery::default()
                })
                .await?;

            if rollouts.len() == rollout_ids.len()
                && rollouts.iter().all(|item| item.status.is_terminal())
            {
                return Ok(summarize_rollouts(&rollouts));
            }

            if Instant::now() >= deadline {
                anyhow::bail!(
                    "trainer timeout waiting for rollouts to complete: {:?}",
                    summarize_rollouts(&rollouts)
                );
            }

            tokio::time::sleep(self.config.completion_poll_interval).await;
        }
    }
}

fn summarize_rollouts(rollouts: &[Rollout]) -> TrainingSummary {
    let total_rollouts = rollouts.len();
    let succeeded = rollouts
        .iter()
        .filter(|item| item.status == RolloutStatus::Succeeded)
        .count();
    let failed = rollouts
        .iter()
        .filter(|item| item.status == RolloutStatus::Failed)
        .count();
    let cancelled = rollouts
        .iter()
        .filter(|item| item.status == RolloutStatus::Cancelled)
        .count();

    TrainingSummary {
        total_rollouts,
        succeeded,
        failed,
        cancelled,
    }
}

fn mode_name(mode: RolloutMode) -> &'static str {
    match mode {
        RolloutMode::Train => "train",
        RolloutMode::Val => "val",
        RolloutMode::Test => "test",
    }
}

fn next_id() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::{Trainer, TrainerConfig};
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::sync::Arc;
    use std::time::Duration;
    use tau_training_runner::{RolloutExecutionOutcome, RolloutExecutor};
    use tau_training_store::{InMemoryTrainingStore, RolloutQuery, TrainingStore};
    use tau_training_types::{ResourcesUpdate, Reward};

    struct BaselineExecutor;

    #[async_trait]
    impl RolloutExecutor for BaselineExecutor {
        async fn execute(
            &self,
            rollout: &tau_training_types::Rollout,
            _resources: Option<&ResourcesUpdate>,
            _tracer: Arc<tau_training_tracer::TrainingTracer>,
        ) -> Result<RolloutExecutionOutcome> {
            let prompt = rollout
                .input
                .get("prompt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();

            Ok(RolloutExecutionOutcome {
                output: json!({ "echo": prompt }),
                rewards: vec![Reward::new("baseline", 1.0)],
            })
        }
    }

    #[tokio::test]
    async fn fit_runs_rollouts_with_two_workers() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        let trainer = Trainer::new(
            store.clone(),
            TrainerConfig {
                worker_count: 2,
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(30),
                completion_poll_interval: Duration::from_millis(25),
                completion_timeout: Duration::from_secs(10),
            },
        );

        let dataset = vec![
            json!({ "prompt": "task-1" }),
            json!({ "prompt": "task-2" }),
            json!({ "prompt": "task-3" }),
            json!({ "prompt": "task-4" }),
            json!({ "prompt": "task-5" }),
            json!({ "prompt": "task-6" }),
        ];

        let summary = trainer
            .fit(
                Arc::new(BaselineExecutor),
                Some(dataset),
                Option::<Vec<Value>>::None,
            )
            .await
            .expect("fit");

        assert_eq!(summary.total_rollouts, 6);
        assert_eq!(summary.succeeded, 6);

        let rollouts = store
            .query_rollouts(RolloutQuery::default())
            .await
            .expect("query rollouts");
        assert_eq!(rollouts.len(), 6);

        for rollout in rollouts {
            let spans = store
                .query_spans(&rollout.rollout_id, None)
                .await
                .expect("query spans");
            assert!(!spans.is_empty());
        }
    }
}
