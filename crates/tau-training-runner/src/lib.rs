//! Rollout worker runtime for training jobs.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tau_agent_core::Agent;
use tau_ai::MessageRole;
use tau_training_store::{DequeuedRollout, TrainingStore};
use tau_training_tracer::TrainingTracer;
use tau_training_types::{ResourcesUpdate, Reward, Rollout};
use tokio::sync::watch;

type AgentFactoryFn = dyn Fn(Option<&ResourcesUpdate>) -> Agent + Send + Sync;
type PromptExtractorFn = dyn Fn(&Value) -> Result<String> + Send + Sync;

/// Execution result emitted by a rollout executor.
#[derive(Debug, Clone)]
pub struct RolloutExecutionOutcome {
    pub output: Value,
    pub rewards: Vec<Reward>,
}

impl Default for RolloutExecutionOutcome {
    fn default() -> Self {
        Self {
            output: Value::Null,
            rewards: Vec::new(),
        }
    }
}

/// User-supplied execution strategy for rollout inputs.
#[async_trait]
pub trait RolloutExecutor: Send + Sync {
    async fn execute(
        &self,
        rollout: &Rollout,
        resources: Option<&ResourcesUpdate>,
        tracer: Arc<TrainingTracer>,
    ) -> Result<RolloutExecutionOutcome>;
}

/// Runtime configuration for a training worker.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub worker_id: String,
    pub poll_interval: Duration,
    pub heartbeat_interval: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            worker_id: "training-worker-1".to_string(),
            poll_interval: Duration::from_millis(75),
            heartbeat_interval: Duration::from_secs(1),
        }
    }
}

/// Polling worker that executes queued rollouts.
pub struct TrainingRunner {
    store: Arc<dyn TrainingStore>,
    executor: Arc<dyn RolloutExecutor>,
    config: RunnerConfig,
}

impl TrainingRunner {
    /// Creates a worker bound to a store/executor pair.
    pub fn new(
        store: Arc<dyn TrainingStore>,
        executor: Arc<dyn RolloutExecutor>,
        config: RunnerConfig,
    ) -> Self {
        Self {
            store,
            executor,
            config,
        }
    }

    /// Runs the worker loop until `shutdown` flips to true.
    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        self.store.register_worker(&self.config.worker_id).await?;

        let mut heartbeat = tokio::time::interval(self.config.heartbeat_interval);
        let mut poll = tokio::time::interval(self.config.poll_interval);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_ok() && *shutdown.borrow() {
                        break;
                    }
                    if changed.is_err() {
                        break;
                    }
                }
                _ = heartbeat.tick() => {
                    self.store
                        .update_worker_heartbeat(&self.config.worker_id, None, None)
                        .await?;
                }
                _ = poll.tick() => {
                    self.process_once().await?;
                }
            }
        }

        Ok(())
    }

    async fn process_once(&self) -> Result<()> {
        let Some(item) = self.store.dequeue_rollout(&self.config.worker_id).await? else {
            return Ok(());
        };

        self.process_dequeued(item).await
    }

    async fn process_dequeued(&self, item: DequeuedRollout) -> Result<()> {
        self.store
            .update_worker_heartbeat(
                &self.config.worker_id,
                Some(item.rollout.rollout_id.clone()),
                Some(item.attempt.attempt_id.clone()),
            )
            .await?;

        let tracer = Arc::new(TrainingTracer::new(
            item.rollout.rollout_id.clone(),
            item.attempt.attempt_id.clone(),
        ));
        let resources = self.store.get_latest_resources().await?;

        let execution_result = {
            let _operation = tracer.operation("runner.execute_rollout");
            self.executor
                .execute(&item.rollout, resources.as_ref(), tracer.clone())
                .await
        };

        match execution_result {
            Ok(outcome) => {
                for reward in outcome.rewards {
                    tracer.emit_reward(reward);
                }
                tracer.emit_reward(Reward::new(
                    "runner.execution_success",
                    if outcome.output.is_null() { 0.0 } else { 1.0 },
                ));

                tracer.flush(self.store.as_ref()).await?;
                self.store
                    .update_attempt_status(
                        &item.attempt.attempt_id,
                        tau_training_types::AttemptStatus::Succeeded,
                        None,
                    )
                    .await?;
                self.store
                    .update_rollout_status(
                        &item.rollout.rollout_id,
                        tau_training_types::RolloutStatus::Succeeded,
                    )
                    .await?;
            }
            Err(error) => {
                tracer.emit_reward(Reward::new("runner.execution_success", 0.0));
                tracer.flush(self.store.as_ref()).await?;
                self.store
                    .update_attempt_status(
                        &item.attempt.attempt_id,
                        tau_training_types::AttemptStatus::Failed,
                        Some(error.to_string()),
                    )
                    .await?;
                self.store
                    .update_rollout_status(
                        &item.rollout.rollout_id,
                        tau_training_types::RolloutStatus::Failed,
                    )
                    .await?;
            }
        }

        self.store
            .update_worker_heartbeat(&self.config.worker_id, None, None)
            .await?;

        Ok(())
    }
}

/// Executor that runs Tau's core `Agent` with tracer event subscriptions.
#[derive(Clone)]
pub struct TauAgentExecutor {
    agent_factory: Arc<AgentFactoryFn>,
    prompt_extractor: Arc<PromptExtractorFn>,
}

impl TauAgentExecutor {
    /// Creates an executor using a caller-provided `Agent` factory.
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(Option<&ResourcesUpdate>) -> Agent + Send + Sync + 'static,
    {
        Self {
            agent_factory: Arc::new(factory),
            prompt_extractor: Arc::new(default_prompt_extractor),
        }
    }

    /// Overrides prompt extraction logic for non-standard rollout input shapes.
    pub fn with_prompt_extractor<F>(mut self, extractor: F) -> Self
    where
        F: Fn(&Value) -> Result<String> + Send + Sync + 'static,
    {
        self.prompt_extractor = Arc::new(extractor);
        self
    }
}

#[async_trait]
impl RolloutExecutor for TauAgentExecutor {
    async fn execute(
        &self,
        rollout: &Rollout,
        resources: Option<&ResourcesUpdate>,
        tracer: Arc<TrainingTracer>,
    ) -> Result<RolloutExecutionOutcome> {
        let mut agent = (self.agent_factory)(resources);

        let tracer_for_events = tracer.clone();
        agent.subscribe(move |event| tracer_for_events.on_agent_event(event));

        let prompt = (self.prompt_extractor)(&rollout.input)?;
        let messages = agent.prompt(prompt).await?;

        let assistant_text = messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
            .map(|message| message.text_content())
            .unwrap_or_default();

        let mut rewards = Vec::new();
        if let Some(expected) = rollout.input.get("expected").and_then(Value::as_str) {
            let score = if assistant_text.trim() == expected.trim() {
                1.0
            } else {
                0.0
            };
            rewards.push(Reward::new("exact_match", score));
        }

        Ok(RolloutExecutionOutcome {
            output: json!({
                "assistant_text": assistant_text,
                "message_count": messages.len(),
            }),
            rewards,
        })
    }
}

fn default_prompt_extractor(input: &Value) -> Result<String> {
    if let Some(text) = input.as_str() {
        return Ok(text.to_string());
    }

    if let Some(prompt) = input.get("prompt").and_then(Value::as_str) {
        return Ok(prompt.to_string());
    }

    anyhow::bail!("rollout input must be a string or object with a string 'prompt' field")
}

#[cfg(test)]
mod tests {
    use super::{
        RolloutExecutionOutcome, RolloutExecutor, RunnerConfig, TauAgentExecutor, TrainingRunner,
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tau_agent_core::{Agent, AgentConfig};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tau_training_store::{InMemoryTrainingStore, RolloutQuery, TrainingStore};
    use tau_training_types::{Reward, Rollout, RolloutStatus};
    use tokio::sync::watch;

    struct StaticExecutor;

    #[async_trait]
    impl RolloutExecutor for StaticExecutor {
        async fn execute(
            &self,
            rollout: &Rollout,
            _resources: Option<&tau_training_types::ResourcesUpdate>,
            _tracer: Arc<tau_training_tracer::TrainingTracer>,
        ) -> Result<RolloutExecutionOutcome> {
            let response = rollout
                .input
                .get("prompt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(RolloutExecutionOutcome {
                output: json!({ "echo": response }),
                rewards: vec![Reward::new("static", 1.0)],
            })
        }
    }

    struct MockClient;

    #[async_trait]
    impl LlmClient for MockClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("expected-output"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    #[tokio::test]
    async fn runner_processes_rollout_and_persists_spans() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-1",
                json!({ "prompt": "hello" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let runner = TrainingRunner::new(
            store.clone(),
            Arc::new(StaticExecutor),
            RunnerConfig {
                worker_id: "worker-1".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(50),
            },
        );

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(async move { runner.run(rx).await });

        wait_for_rollout_status(store.clone(), "r-1", RolloutStatus::Succeeded)
            .await
            .expect("status wait");

        tx.send(true).expect("shutdown");
        handle.await.expect("join").expect("runner");

        let spans = store.query_spans("r-1", None).await.expect("spans");
        assert!(spans
            .iter()
            .any(|span| span.name == "runner.execute_rollout"));
    }

    #[tokio::test]
    async fn tau_agent_executor_extracts_exact_match_reward() {
        let executor = TauAgentExecutor::new(|_resources| {
            Agent::new(Arc::new(MockClient), AgentConfig::default())
        });
        let tracer = Arc::new(tau_training_tracer::TrainingTracer::new("r-1", "a-1"));

        let outcome = executor
            .execute(
                &Rollout::new(
                    "r-1",
                    json!({ "prompt": "Say expected", "expected": "expected-output" }),
                    Some(tau_training_types::RolloutMode::Train),
                ),
                None,
                tracer,
            )
            .await
            .expect("execute");

        assert!(outcome.rewards.iter().any(
            |reward| reward.name == "exact_match" && (reward.value - 1.0).abs() < f64::EPSILON
        ));
    }

    async fn wait_for_rollout_status(
        store: Arc<dyn TrainingStore>,
        rollout_id: &str,
        status: RolloutStatus,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let rows = store
                .query_rollouts(RolloutQuery {
                    ids: Some(vec![rollout_id.to_string()]),
                    ..RolloutQuery::default()
                })
                .await?;
            if rows.first().is_some_and(|item| item.status == status) {
                return Ok(());
            }

            if Instant::now() >= deadline {
                anyhow::bail!("timed out waiting for rollout {rollout_id} status {status:?}");
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }
}
