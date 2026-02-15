use anyhow::{anyhow, bail, Result};
use std::collections::BTreeSet;
use tau_training_store::TrainingStore;
use tau_training_types::{EpisodeTrajectory, RolloutQuery};

use crate::{SpansToTrajectories, TraceAdapter, TrajectoryWindowPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrajectoryCollectionSkip {
    pub rollout_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrajectoryCollectionBatch {
    pub rollout_ids: Vec<String>,
    pub total_spans: usize,
    pub trajectories: Vec<EpisodeTrajectory>,
    pub skipped_rollouts: Vec<TrajectoryCollectionSkip>,
}

pub async fn collect_trajectory_batch(
    store: &dyn TrainingStore,
    rollout_ids: &[String],
    window_policy: Option<TrajectoryWindowPolicy>,
) -> Result<TrajectoryCollectionBatch> {
    let normalized_rollout_ids: Vec<String> = rollout_ids
        .iter()
        .map(|rollout_id| rollout_id.trim())
        .filter(|rollout_id| !rollout_id.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if normalized_rollout_ids.is_empty() {
        bail!("trajectory collection requires at least one rollout id");
    }

    let rollouts = store
        .query_rollouts(RolloutQuery {
            ids: Some(normalized_rollout_ids.clone()),
            ..RolloutQuery::default()
        })
        .await?;
    let known_rollout_ids = rollouts
        .iter()
        .map(|rollout| rollout.rollout_id.as_str())
        .collect::<BTreeSet<_>>();
    let missing_rollout_ids: Vec<String> = normalized_rollout_ids
        .iter()
        .filter(|rollout_id| !known_rollout_ids.contains(rollout_id.as_str()))
        .cloned()
        .collect();
    if !missing_rollout_ids.is_empty() {
        bail!(
            "unknown rollout ids for trajectory collection: {}",
            missing_rollout_ids.join(", ")
        );
    }

    let adapter = window_policy.map_or_else(SpansToTrajectories::default, |policy| {
        SpansToTrajectories::with_window_policy(policy)
    });
    let mut total_spans = 0usize;
    let mut trajectories = Vec::new();
    let mut skipped_rollouts = Vec::new();
    for rollout_id in &normalized_rollout_ids {
        let spans = store.query_spans(rollout_id, None).await?;
        total_spans += spans.len();
        if spans.is_empty() {
            skipped_rollouts.push(TrajectoryCollectionSkip {
                rollout_id: rollout_id.clone(),
                reason: "no spans".to_string(),
            });
            continue;
        }
        let adapted = adapter.adapt(&spans).map_err(|error| {
            anyhow!("trajectory collection adaptation failed for rollout '{rollout_id}': {error}")
        })?;
        if adapted.is_empty() {
            skipped_rollouts.push(TrajectoryCollectionSkip {
                rollout_id: rollout_id.clone(),
                reason: "no trajectories".to_string(),
            });
            continue;
        }
        trajectories.extend(adapted);
    }

    Ok(TrajectoryCollectionBatch {
        rollout_ids: normalized_rollout_ids,
        total_spans,
        trajectories,
        skipped_rollouts,
    })
}

#[cfg(test)]
mod tests {
    use super::collect_trajectory_batch;
    use crate::{TrajectoryPaddingMode, TrajectoryWindowPolicy};
    use serde_json::json;
    use tau_training_store::{InMemoryTrainingStore, TrainingStore};
    use tau_training_types::{Rollout, RolloutMode, TrainingSpan};

    fn sample_span(
        rollout_id: &str,
        attempt_id: &str,
        sequence_id: u64,
        reward: f64,
    ) -> TrainingSpan {
        let mut span = TrainingSpan::new(
            rollout_id,
            attempt_id,
            sequence_id,
            format!("trace-{rollout_id}"),
            format!("span-{rollout_id}-{sequence_id}"),
            None,
            "agent.turn",
        );
        span.attributes
            .insert("observation".to_string(), json!({ "step": sequence_id }));
        span.attributes
            .insert("action".to_string(), json!({ "tool": "search" }));
        span.attributes.insert("reward".to_string(), json!(reward));
        span.end_time = Some(span.start_time);
        span
    }

    #[tokio::test]
    async fn spec_1964_c01_collects_single_rollout_into_deterministic_batch() {
        let store: Box<dyn TrainingStore> = Box::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-collect-1",
                json!({"prompt": "collect"}),
                Some(RolloutMode::Train),
            ))
            .await
            .expect("enqueue");
        store
            .add_span(sample_span("r-collect-1", "r-collect-1:attempt-1", 1, 0.5))
            .await
            .expect("add span");

        let batch = collect_trajectory_batch(store.as_ref(), &[String::from("r-collect-1")], None)
            .await
            .expect("batch");
        assert_eq!(batch.rollout_ids, vec!["r-collect-1".to_string()]);
        assert_eq!(batch.total_spans, 1);
        assert_eq!(batch.trajectories.len(), 1);
        assert!(batch.skipped_rollouts.is_empty());
    }

    #[tokio::test]
    async fn spec_1964_c02_collects_multi_attempt_retry_trajectories() {
        let store: Box<dyn TrainingStore> = Box::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-collect-chaos-1",
                json!({"prompt": "collect-chaos"}),
                Some(RolloutMode::Train),
            ))
            .await
            .expect("enqueue");
        store
            .add_span(sample_span(
                "r-collect-chaos-1",
                "r-collect-chaos-1:attempt-1",
                1,
                0.2,
            ))
            .await
            .expect("add span");
        store
            .add_span(sample_span(
                "r-collect-chaos-1",
                "r-collect-chaos-1:attempt-2",
                1,
                0.8,
            ))
            .await
            .expect("add span");

        let batch =
            collect_trajectory_batch(store.as_ref(), &[String::from("r-collect-chaos-1")], None)
                .await
                .expect("batch");
        assert_eq!(batch.trajectories.len(), 2);
        assert_eq!(batch.total_spans, 2);
    }

    #[tokio::test]
    async fn spec_1964_c03_applies_window_policy_during_collection() {
        let store: Box<dyn TrainingStore> = Box::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-collect-window-1",
                json!({"prompt": "collect-window"}),
                Some(RolloutMode::Train),
            ))
            .await
            .expect("enqueue");
        for sequence_id in 1..=5 {
            store
                .add_span(sample_span(
                    "r-collect-window-1",
                    "r-collect-window-1:attempt-1",
                    sequence_id,
                    sequence_id as f64 / 10.0,
                ))
                .await
                .expect("add span");
        }

        let batch = collect_trajectory_batch(
            store.as_ref(),
            &[String::from("r-collect-window-1")],
            Some(TrajectoryWindowPolicy {
                window_size: 3,
                padding_mode: TrajectoryPaddingMode::Disabled,
            }),
        )
        .await
        .expect("batch");
        assert_eq!(batch.trajectories.len(), 1);
        assert_eq!(batch.trajectories[0].steps.len(), 3);
    }

    #[tokio::test]
    async fn spec_1964_c04_rejects_unknown_rollout_ids() {
        let store: Box<dyn TrainingStore> = Box::new(InMemoryTrainingStore::new());
        let error = collect_trajectory_batch(store.as_ref(), &[String::from("r-unknown")], None)
            .await
            .expect_err("unknown rollout should fail");
        assert!(error
            .to_string()
            .contains("unknown rollout ids for trajectory collection"));
    }
}
