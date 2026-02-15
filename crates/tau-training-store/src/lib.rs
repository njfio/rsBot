//! Training store abstractions and in-memory backend.

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Notify, RwLock};

mod sqlite;

pub use sqlite::SqliteTrainingStore;
pub use tau_training_types::{
    Attempt, AttemptStatus, ResourcesUpdate, Rollout, RolloutQuery, RolloutStatus, TrainingSpan,
    WorkerState,
};

/// Result type for training store operations.
pub type StoreResult<T> = Result<T, TrainingStoreError>;

/// Errors returned by store implementations.
#[derive(Debug, Error)]
pub enum TrainingStoreError {
    #[error("rollout '{0}' already exists")]
    RolloutAlreadyExists(String),
    #[error("rollout '{0}' not found")]
    RolloutNotFound(String),
    #[error("attempt '{0}' not found")]
    AttemptNotFound(String),
    #[error("worker '{0}' not found")]
    WorkerNotFound(String),
    #[error("invalid rollout status transition: {from:?} -> {to:?}")]
    InvalidRolloutTransition {
        from: RolloutStatus,
        to: RolloutStatus,
    },
    #[error("invalid attempt status transition: {from:?} -> {to:?}")]
    InvalidAttemptTransition {
        from: AttemptStatus,
        to: AttemptStatus,
    },
    #[error("invalid persisted value for '{field}': {value}")]
    InvalidPersistedValue { field: &'static str, value: String },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Chrono(#[from] chrono::ParseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result of atomically dequeuing a rollout for execution.
#[derive(Debug, Clone)]
pub struct DequeuedRollout {
    pub rollout: Rollout,
    pub attempt: Attempt,
}

/// Async store contract used by runners and algorithms.
#[async_trait]
pub trait TrainingStore: Send + Sync {
    async fn enqueue_rollout(&self, rollout: Rollout) -> StoreResult<()>;
    async fn dequeue_rollout(&self, worker_id: &str) -> StoreResult<Option<DequeuedRollout>>;
    async fn update_rollout_status(
        &self,
        rollout_id: &str,
        status: RolloutStatus,
    ) -> StoreResult<()>;
    async fn cancel_rollout(&self, rollout_id: &str) -> StoreResult<()>;

    async fn add_span(&self, span: TrainingSpan) -> StoreResult<()>;
    async fn add_spans(&self, spans: Vec<TrainingSpan>) -> StoreResult<()> {
        for span in spans {
            self.add_span(span).await?;
        }
        Ok(())
    }
    async fn query_spans(
        &self,
        rollout_id: &str,
        attempt_id: Option<&str>,
    ) -> StoreResult<Vec<TrainingSpan>>;
    async fn get_next_span_sequence_id(
        &self,
        rollout_id: &str,
        attempt_id: &str,
    ) -> StoreResult<u64>;

    async fn update_resources(
        &self,
        resources: HashMap<String, Value>,
    ) -> StoreResult<ResourcesUpdate>;
    async fn get_latest_resources(&self) -> StoreResult<Option<ResourcesUpdate>>;
    async fn get_resources_by_id(&self, resources_id: &str)
        -> StoreResult<Option<ResourcesUpdate>>;

    async fn query_rollouts(&self, query: RolloutQuery) -> StoreResult<Vec<Rollout>>;
    async fn wait_for_rollouts(
        &self,
        statuses: &[RolloutStatus],
        timeout: Duration,
    ) -> StoreResult<Vec<Rollout>>;

    async fn register_worker(&self, worker_id: &str) -> StoreResult<WorkerState>;
    async fn update_worker_heartbeat(
        &self,
        worker_id: &str,
        active_rollout_id: Option<String>,
        active_attempt_id: Option<String>,
    ) -> StoreResult<()>;
    async fn reassign_timed_out_rollouts(
        &self,
        heartbeat_timeout: Duration,
    ) -> StoreResult<Vec<String>>;
    async fn query_workers(&self) -> StoreResult<Vec<WorkerState>>;

    async fn update_attempt_status(
        &self,
        attempt_id: &str,
        status: AttemptStatus,
        error_message: Option<String>,
    ) -> StoreResult<()>;
    async fn get_attempt(&self, attempt_id: &str) -> StoreResult<Option<Attempt>>;
}

/// In-memory implementation for tests and local experimentation.
#[derive(Debug, Default)]
pub struct InMemoryTrainingStore {
    inner: RwLock<StoreInner>,
    notify: Notify,
}

#[derive(Debug, Default)]
struct StoreInner {
    rollouts: HashMap<String, Rollout>,
    queue: VecDeque<String>,
    attempts: HashMap<String, Attempt>,
    attempt_ids_by_rollout: HashMap<String, Vec<String>>,
    spans: HashMap<(String, String), Vec<TrainingSpan>>,
    resources: Vec<ResourcesUpdate>,
    workers: HashMap<String, WorkerState>,
}

impl InMemoryTrainingStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TrainingStore for InMemoryTrainingStore {
    async fn enqueue_rollout(&self, mut rollout: Rollout) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        if inner.rollouts.contains_key(&rollout.rollout_id) {
            return Err(TrainingStoreError::RolloutAlreadyExists(
                rollout.rollout_id.clone(),
            ));
        }

        rollout.status = RolloutStatus::Queuing;
        rollout.assigned_worker_id = None;
        rollout.start_time = None;
        rollout.end_time = None;
        inner.queue.push_back(rollout.rollout_id.clone());
        inner.rollouts.insert(rollout.rollout_id.clone(), rollout);
        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn dequeue_rollout(&self, worker_id: &str) -> StoreResult<Option<DequeuedRollout>> {
        let mut inner = self.inner.write().await;

        while let Some(rollout_id) = inner.queue.pop_front() {
            let (rollout, rollout_id_owned, sequence_id, attempt_id) = {
                let Some(rollout) = inner.rollouts.get_mut(&rollout_id) else {
                    continue;
                };

                if !matches!(
                    rollout.status,
                    RolloutStatus::Queuing | RolloutStatus::Requeuing
                ) {
                    continue;
                }

                let from = rollout.status;
                let to = RolloutStatus::Running;
                if !from.can_transition_to(to) {
                    return Err(TrainingStoreError::InvalidRolloutTransition { from, to });
                }

                rollout.status = RolloutStatus::Running;
                rollout.assigned_worker_id = Some(worker_id.to_string());
                if rollout.start_time.is_none() {
                    rollout.start_time = Some(Utc::now());
                }
                rollout.attempt_count += 1;

                let sequence_id = rollout.attempt_count;
                let rollout_id_owned = rollout.rollout_id.clone();
                let attempt_id = format!("{}:attempt-{}", rollout_id_owned, sequence_id);

                (rollout.clone(), rollout_id_owned, sequence_id, attempt_id)
            };

            let mut attempt = Attempt::new(
                attempt_id.clone(),
                rollout_id_owned.clone(),
                sequence_id,
                worker_id,
            );
            attempt.status = AttemptStatus::Running;

            inner
                .attempt_ids_by_rollout
                .entry(rollout_id_owned.clone())
                .or_default()
                .push(attempt_id.clone());
            inner.attempts.insert(attempt_id.clone(), attempt.clone());

            let now = Utc::now();
            inner
                .workers
                .entry(worker_id.to_string())
                .and_modify(|worker| {
                    worker.last_heartbeat_at = now;
                    worker.active_rollout_id = Some(rollout_id_owned.clone());
                    worker.active_attempt_id = Some(attempt_id.clone());
                })
                .or_insert_with(|| WorkerState {
                    worker_id: worker_id.to_string(),
                    registered_at: now,
                    last_heartbeat_at: now,
                    active_rollout_id: Some(rollout_id_owned),
                    active_attempt_id: Some(attempt_id.clone()),
                });

            let result = DequeuedRollout { rollout, attempt };

            drop(inner);
            self.notify.notify_waiters();
            return Ok(Some(result));
        }

        Ok(None)
    }

    async fn update_rollout_status(
        &self,
        rollout_id: &str,
        status: RolloutStatus,
    ) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        let rollout = inner
            .rollouts
            .get_mut(rollout_id)
            .ok_or_else(|| TrainingStoreError::RolloutNotFound(rollout_id.to_string()))?;

        let from = rollout.status;
        if !from.can_transition_to(status) {
            return Err(TrainingStoreError::InvalidRolloutTransition { from, to: status });
        }

        rollout.status = status;
        if status.is_terminal() {
            rollout.end_time = Some(Utc::now());
        }

        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn cancel_rollout(&self, rollout_id: &str) -> StoreResult<()> {
        self.update_rollout_status(rollout_id, RolloutStatus::Cancelled)
            .await
    }

    async fn add_span(&self, span: TrainingSpan) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        let key = (span.rollout_id.clone(), span.attempt_id.clone());
        let entry = inner.spans.entry(key).or_default();
        entry.push(span);
        entry.sort_by_key(|item| item.sequence_id);
        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn query_spans(
        &self,
        rollout_id: &str,
        attempt_id: Option<&str>,
    ) -> StoreResult<Vec<TrainingSpan>> {
        let inner = self.inner.read().await;

        if let Some(attempt_id) = attempt_id {
            let spans = inner
                .spans
                .get(&(rollout_id.to_string(), attempt_id.to_string()))
                .cloned()
                .unwrap_or_default();
            return Ok(spans);
        }

        let mut spans: Vec<TrainingSpan> = inner
            .spans
            .iter()
            .filter(|((span_rollout_id, _), _)| span_rollout_id == rollout_id)
            .flat_map(|(_, value)| value.iter().cloned())
            .collect();
        spans.sort_by_key(|item| item.sequence_id);
        Ok(spans)
    }

    async fn get_next_span_sequence_id(
        &self,
        rollout_id: &str,
        attempt_id: &str,
    ) -> StoreResult<u64> {
        let inner = self.inner.read().await;
        let last = inner
            .spans
            .get(&(rollout_id.to_string(), attempt_id.to_string()))
            .and_then(|items| items.iter().map(|item| item.sequence_id).max())
            .unwrap_or(0);
        Ok(last + 1)
    }

    async fn update_resources(
        &self,
        resources: HashMap<String, Value>,
    ) -> StoreResult<ResourcesUpdate> {
        let mut inner = self.inner.write().await;

        for update in &mut inner.resources {
            update.is_latest = false;
        }

        let version = inner.resources.len() as u64 + 1;
        let update = ResourcesUpdate {
            resources_id: format!("resources-{version}"),
            version,
            resources,
            created_time: Utc::now(),
            is_latest: true,
        };
        inner.resources.push(update.clone());

        drop(inner);
        self.notify.notify_waiters();
        Ok(update)
    }

    async fn get_latest_resources(&self) -> StoreResult<Option<ResourcesUpdate>> {
        let inner = self.inner.read().await;
        Ok(inner
            .resources
            .iter()
            .rev()
            .find(|item| item.is_latest)
            .cloned())
    }

    async fn get_resources_by_id(
        &self,
        resources_id: &str,
    ) -> StoreResult<Option<ResourcesUpdate>> {
        let inner = self.inner.read().await;
        Ok(inner
            .resources
            .iter()
            .find(|item| item.resources_id == resources_id)
            .cloned())
    }

    async fn query_rollouts(&self, query: RolloutQuery) -> StoreResult<Vec<Rollout>> {
        let inner = self.inner.read().await;
        let mut rollouts: Vec<Rollout> = inner
            .rollouts
            .values()
            .filter(|rollout| {
                query
                    .statuses
                    .as_ref()
                    .is_none_or(|statuses| statuses.contains(&rollout.status))
            })
            .filter(|rollout| query.mode.is_none_or(|mode| rollout.mode == Some(mode)))
            .filter(|rollout| {
                query
                    .ids
                    .as_ref()
                    .is_none_or(|ids| ids.iter().any(|id| id == &rollout.rollout_id))
            })
            .cloned()
            .collect();

        rollouts.sort_by(|left, right| left.rollout_id.cmp(&right.rollout_id));

        let start = query.offset.min(rollouts.len());
        let mut sliced = rollouts.split_off(start);
        if let Some(limit) = query.limit {
            sliced.truncate(limit);
        }
        Ok(sliced)
    }

    async fn wait_for_rollouts(
        &self,
        statuses: &[RolloutStatus],
        timeout: Duration,
    ) -> StoreResult<Vec<Rollout>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }

        let deadline = Instant::now() + timeout;
        loop {
            let matches = {
                let inner = self.inner.read().await;
                let mut rollouts: Vec<Rollout> = inner
                    .rollouts
                    .values()
                    .filter(|rollout| statuses.contains(&rollout.status))
                    .cloned()
                    .collect();
                rollouts.sort_by(|left, right| left.rollout_id.cmp(&right.rollout_id));
                rollouts
            };

            if !matches.is_empty() {
                return Ok(matches);
            }

            let now = Instant::now();
            if now >= deadline {
                return Ok(Vec::new());
            }

            let remaining = deadline.saturating_duration_since(now);
            if tokio::time::timeout(remaining, self.notify.notified())
                .await
                .is_err()
            {
                return Ok(Vec::new());
            }
        }
    }

    async fn register_worker(&self, worker_id: &str) -> StoreResult<WorkerState> {
        let mut inner = self.inner.write().await;
        let now = Utc::now();

        let worker = inner
            .workers
            .entry(worker_id.to_string())
            .and_modify(|worker| worker.last_heartbeat_at = now)
            .or_insert_with(|| WorkerState {
                worker_id: worker_id.to_string(),
                registered_at: now,
                last_heartbeat_at: now,
                active_rollout_id: None,
                active_attempt_id: None,
            })
            .clone();

        drop(inner);
        self.notify.notify_waiters();
        Ok(worker)
    }

    async fn update_worker_heartbeat(
        &self,
        worker_id: &str,
        active_rollout_id: Option<String>,
        active_attempt_id: Option<String>,
    ) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        let worker = inner
            .workers
            .get_mut(worker_id)
            .ok_or_else(|| TrainingStoreError::WorkerNotFound(worker_id.to_string()))?;

        worker.last_heartbeat_at = Utc::now();
        worker.active_rollout_id = active_rollout_id;
        worker.active_attempt_id = active_attempt_id;

        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn reassign_timed_out_rollouts(
        &self,
        heartbeat_timeout: Duration,
    ) -> StoreResult<Vec<String>> {
        let mut inner = self.inner.write().await;
        let now = Utc::now();
        let mut requeued_rollout_ids = Vec::new();

        let worker_ids = inner.workers.keys().cloned().collect::<Vec<_>>();
        for worker_id in worker_ids {
            let (Some(active_rollout_id), Some(active_attempt_id)) = inner
                .workers
                .get(&worker_id)
                .map(|worker| {
                    (
                        worker.active_rollout_id.clone(),
                        worker.active_attempt_id.clone(),
                    )
                })
                .unwrap_or((None, None))
            else {
                continue;
            };

            let Some(attempt) = inner.attempts.get_mut(&active_attempt_id) else {
                if let Some(worker) = inner.workers.get_mut(&worker_id) {
                    worker.active_rollout_id = None;
                    worker.active_attempt_id = None;
                    worker.last_heartbeat_at = now;
                }
                continue;
            };

            if attempt.status != AttemptStatus::Running {
                continue;
            }

            let elapsed = now
                .signed_duration_since(attempt.last_heartbeat_at)
                .to_std()
                .unwrap_or_default();
            if elapsed <= heartbeat_timeout {
                continue;
            }

            let from_attempt = attempt.status;
            if !from_attempt.can_transition_to(AttemptStatus::Timeout) {
                return Err(TrainingStoreError::InvalidAttemptTransition {
                    from: from_attempt,
                    to: AttemptStatus::Timeout,
                });
            }
            attempt.status = AttemptStatus::Timeout;
            attempt.last_heartbeat_at = now;
            attempt.ended_at = Some(now);
            attempt.error_message = Some("worker heartbeat timeout".to_string());

            if let Some(rollout) = inner.rollouts.get_mut(&active_rollout_id) {
                let from_rollout = rollout.status;
                if from_rollout.can_transition_to(RolloutStatus::Requeuing) {
                    rollout.status = RolloutStatus::Requeuing;
                    rollout.assigned_worker_id = None;
                    if !inner
                        .queue
                        .iter()
                        .any(|queued_id| queued_id == &active_rollout_id)
                    {
                        inner.queue.push_back(active_rollout_id.clone());
                    }
                    if !requeued_rollout_ids.contains(&active_rollout_id) {
                        requeued_rollout_ids.push(active_rollout_id.clone());
                    }
                } else {
                    return Err(TrainingStoreError::InvalidRolloutTransition {
                        from: from_rollout,
                        to: RolloutStatus::Requeuing,
                    });
                }
            }

            if let Some(worker) = inner.workers.get_mut(&worker_id) {
                worker.last_heartbeat_at = now;
                worker.active_rollout_id = None;
                worker.active_attempt_id = None;
            }
        }

        drop(inner);
        if !requeued_rollout_ids.is_empty() {
            self.notify.notify_waiters();
        }
        Ok(requeued_rollout_ids)
    }

    async fn query_workers(&self) -> StoreResult<Vec<WorkerState>> {
        let inner = self.inner.read().await;
        let mut workers: Vec<WorkerState> = inner.workers.values().cloned().collect();
        workers.sort_by(|left, right| left.worker_id.cmp(&right.worker_id));
        Ok(workers)
    }

    async fn update_attempt_status(
        &self,
        attempt_id: &str,
        status: AttemptStatus,
        error_message: Option<String>,
    ) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        let attempt = inner
            .attempts
            .get_mut(attempt_id)
            .ok_or_else(|| TrainingStoreError::AttemptNotFound(attempt_id.to_string()))?;

        let from = attempt.status;
        if !from.can_transition_to(status) {
            return Err(TrainingStoreError::InvalidAttemptTransition { from, to: status });
        }

        attempt.status = status;
        attempt.last_heartbeat_at = Utc::now();
        if status.is_terminal() {
            attempt.ended_at = Some(Utc::now());
            attempt.error_message = error_message;
        }

        drop(inner);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn get_attempt(&self, attempt_id: &str) -> StoreResult<Option<Attempt>> {
        let inner = self.inner.read().await;
        Ok(inner.attempts.get(attempt_id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AttemptStatus, InMemoryTrainingStore, Rollout, RolloutQuery, RolloutStatus, TrainingSpan,
    };
    use crate::TrainingStore;
    use serde_json::json;
    use std::time::Duration;

    #[tokio::test]
    async fn enqueues_and_dequeues_rollouts() {
        let store = InMemoryTrainingStore::new();
        store
            .enqueue_rollout(Rollout::new("r-1", json!({ "prompt": "hello" }), None))
            .await
            .expect("enqueue");

        let dequeued = store
            .dequeue_rollout("worker-1")
            .await
            .expect("dequeue")
            .expect("item");

        assert_eq!(dequeued.rollout.rollout_id, "r-1");
        assert_eq!(dequeued.attempt.worker_id, "worker-1");

        store
            .update_rollout_status("r-1", RolloutStatus::Succeeded)
            .await
            .expect("status update");

        let rows = store
            .query_rollouts(RolloutQuery {
                statuses: Some(vec![RolloutStatus::Succeeded]),
                ..RolloutQuery::default()
            })
            .await
            .expect("query");
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn persists_and_queries_spans() {
        let store = InMemoryTrainingStore::new();
        let mut span = TrainingSpan::new(
            "r-1",
            "r-1:attempt-1",
            1,
            "trace-1",
            "span-1",
            None,
            "runner",
        );
        span.attributes.insert("k".to_string(), json!("v"));

        store.add_span(span).await.expect("add span");

        let spans = store.query_spans("r-1", None).await.expect("query spans");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "runner");
        assert_eq!(
            store
                .get_next_span_sequence_id("r-1", "r-1:attempt-1")
                .await
                .expect("next sequence"),
            2
        );
    }

    #[tokio::test]
    async fn wait_for_rollouts_wakes_after_enqueue() {
        let store = std::sync::Arc::new(InMemoryTrainingStore::new());
        let waiting = {
            let store = store.clone();
            tokio::spawn(async move {
                store
                    .wait_for_rollouts(&[RolloutStatus::Queuing], Duration::from_secs(2))
                    .await
                    .expect("wait")
            })
        };

        tokio::time::sleep(Duration::from_millis(40)).await;
        store
            .enqueue_rollout(Rollout::new("r-1", json!({ "prompt": "hello" }), None))
            .await
            .expect("enqueue");

        let found = waiting.await.expect("join");
        assert_eq!(found.len(), 1);
    }

    #[tokio::test]
    async fn updates_resources_with_incrementing_versions() {
        let store = InMemoryTrainingStore::new();
        let first = store
            .update_resources(HashMap::from([(String::from("system_prompt"), json!("A"))]))
            .await
            .expect("first update");
        let second = store
            .update_resources(HashMap::from([(String::from("system_prompt"), json!("B"))]))
            .await
            .expect("second update");

        assert_eq!(first.version, 1);
        assert_eq!(second.version, 2);

        let latest = store
            .get_latest_resources()
            .await
            .expect("latest")
            .expect("resource");
        assert_eq!(latest.resources_id, second.resources_id);
    }

    #[tokio::test]
    async fn integration_reassigns_timed_out_worker_and_preserves_spans() {
        let store = InMemoryTrainingStore::new();
        store
            .enqueue_rollout(Rollout::new(
                "r-chaos-1",
                json!({ "prompt": "hello" }),
                None,
            ))
            .await
            .expect("enqueue");

        let first = store
            .dequeue_rollout("worker-a")
            .await
            .expect("dequeue first")
            .expect("first attempt");
        store
            .add_span(TrainingSpan::new(
                "r-chaos-1",
                first.attempt.attempt_id.clone(),
                1,
                "trace-1",
                "span-attempt-1",
                None,
                "runner.execute",
            ))
            .await
            .expect("add first attempt span");

        tokio::time::sleep(Duration::from_millis(30)).await;
        let requeued = store
            .reassign_timed_out_rollouts(Duration::from_millis(5))
            .await
            .expect("reassign timed out");
        assert_eq!(requeued, vec!["r-chaos-1".to_string()]);

        let first_attempt = store
            .get_attempt(&first.attempt.attempt_id)
            .await
            .expect("get first attempt")
            .expect("first attempt exists");
        assert_eq!(first_attempt.status, AttemptStatus::Timeout);

        let second = store
            .dequeue_rollout("worker-b")
            .await
            .expect("dequeue second")
            .expect("second attempt");
        assert_eq!(second.rollout.rollout_id, "r-chaos-1");
        assert_eq!(second.attempt.sequence_id, 2);

        store
            .add_span(TrainingSpan::new(
                "r-chaos-1",
                second.attempt.attempt_id.clone(),
                1,
                "trace-2",
                "span-attempt-2",
                None,
                "runner.execute",
            ))
            .await
            .expect("add second attempt span");

        let all_spans = store
            .query_spans("r-chaos-1", None)
            .await
            .expect("query all spans");
        assert_eq!(all_spans.len(), 2);
    }

    use std::collections::HashMap;
}
