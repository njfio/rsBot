//! Rollout worker runtime for training jobs.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tau_agent_core::Agent;
use tau_ai::MessageRole;
use tau_training_store::{DequeuedRollout, TrainingStore};
use tau_training_tracer::TrainingTracer;
use tau_training_types::{
    AttemptStatus, ResourcesUpdate, Reward, Rollout, RolloutStatus, TrainingSpan,
};
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
    pub reassignment_interval: Duration,
    pub worker_timeout: Duration,
    pub transient_error_backoff_initial: Duration,
    pub transient_error_backoff_max: Duration,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            worker_id: "training-worker-1".to_string(),
            poll_interval: Duration::from_millis(75),
            heartbeat_interval: Duration::from_secs(1),
            reassignment_interval: Duration::from_millis(250),
            worker_timeout: Duration::from_secs(3),
            transient_error_backoff_initial: Duration::from_millis(25),
            transient_error_backoff_max: Duration::from_millis(500),
        }
    }
}

impl RunnerConfig {
    /// Validates runner timing and retry-backoff configuration.
    pub fn validate(&self) -> Result<()> {
        if self.transient_error_backoff_initial.is_zero() {
            anyhow::bail!("transient_error_backoff_initial must be greater than 0");
        }
        if self.transient_error_backoff_max.is_zero() {
            anyhow::bail!("transient_error_backoff_max must be greater than 0");
        }
        if self.transient_error_backoff_max < self.transient_error_backoff_initial {
            anyhow::bail!("transient_error_backoff_max must be >= transient_error_backoff_initial");
        }
        Ok(())
    }
}

fn compute_poll_retry_delay(failure_count: u32, initial: Duration, max: Duration) -> Duration {
    let mut delay = initial;
    for _ in 1..failure_count {
        delay = delay.saturating_mul(2);
        if delay >= max {
            return max;
        }
    }
    std::cmp::min(delay, max)
}

/// Per-attempt persistence details for rollout integrity audits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptPersistenceAudit {
    pub attempt_id: String,
    pub status: Option<AttemptStatus>,
    pub span_count: usize,
}

/// Deterministic rollout persistence integrity report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutPersistenceAuditReport {
    pub rollout_id: String,
    pub rollout_status: RolloutStatus,
    pub expected_attempt_count: u32,
    pub attempts: Vec<AttemptPersistenceAudit>,
    pub gap_reasons: Vec<String>,
    pub has_persistence_gaps: bool,
}

/// Audits rollout persistence integrity across attempts/spans using a store snapshot.
pub async fn audit_rollout_persistence(
    store: &dyn TrainingStore,
    rollout_id: &str,
) -> Result<RolloutPersistenceAuditReport> {
    let rollouts = store
        .query_rollouts(tau_training_types::RolloutQuery {
            ids: Some(vec![rollout_id.to_string()]),
            ..tau_training_types::RolloutQuery::default()
        })
        .await?;

    let Some(rollout) = rollouts.first() else {
        anyhow::bail!("rollout '{rollout_id}' not found for persistence audit");
    };

    let mut attempts = Vec::new();
    let mut gap_reasons = Vec::new();
    for attempt_sequence in 1..=rollout.attempt_count {
        let attempt_id = format!("{rollout_id}:attempt-{attempt_sequence}");
        let attempt = store.get_attempt(&attempt_id).await?;
        let spans = store.query_spans(rollout_id, Some(&attempt_id)).await?;
        let status = attempt.as_ref().map(|record| record.status);

        if attempt.is_none() {
            gap_reasons.push(format!("missing attempt record: {attempt_id}"));
        } else if status.is_some_and(AttemptStatus::is_terminal) && spans.is_empty() {
            gap_reasons.push(format!("missing terminal attempt spans: {attempt_id}"));
        }

        attempts.push(AttemptPersistenceAudit {
            attempt_id,
            status,
            span_count: spans.len(),
        });
    }

    Ok(RolloutPersistenceAuditReport {
        rollout_id: rollout_id.to_string(),
        rollout_status: rollout.status,
        expected_attempt_count: rollout.attempt_count,
        attempts,
        has_persistence_gaps: !gap_reasons.is_empty(),
        gap_reasons,
    })
}

/// Configures reward shaping from safety-policy events.
#[derive(Debug, Clone, PartialEq)]
pub struct SafetyRewardPolicy {
    /// Penalty applied when a safety event was explicitly blocked.
    pub blocked_penalty: f64,
    /// Penalty for reason codes that do not match a configured family/override.
    pub default_reason_code_penalty: f64,
    /// Penalty for `prompt_injection.*` reason codes.
    pub prompt_injection_penalty: f64,
    /// Penalty for `secret_leak.*` reason codes.
    pub secret_leak_penalty: f64,
    /// Per-reason-code penalty overrides.
    pub reason_code_penalties: HashMap<String, f64>,
    /// Exact reason codes that trigger hard gating.
    pub hard_gate_reason_codes: HashSet<String>,
    /// Prefixes that trigger hard gating.
    pub hard_gate_reason_prefixes: Vec<String>,
    /// Upper bound applied to positive rewards when hard gate triggers.
    pub hard_gate_reward_ceiling: f64,
    /// Additional penalty applied when hard gate triggers.
    pub hard_gate_penalty: f64,
    /// Treat blocked safety events as hard-gate signals.
    pub blocked_event_triggers_hard_gate: bool,
    /// Reject the rollout on hard-gate instead of only clamping rewards.
    pub reject_rollout_on_hard_gate: bool,
}

impl Default for SafetyRewardPolicy {
    fn default() -> Self {
        Self {
            blocked_penalty: 1.0,
            default_reason_code_penalty: 0.25,
            prompt_injection_penalty: 1.0,
            secret_leak_penalty: 1.5,
            reason_code_penalties: HashMap::new(),
            hard_gate_reason_codes: HashSet::from([
                "prompt_injection.system_prompt_exfiltration".to_string(),
                "secret_leak.redaction_failed".to_string(),
            ]),
            hard_gate_reason_prefixes: vec!["secret_leak.".to_string()],
            hard_gate_reward_ceiling: 0.0,
            hard_gate_penalty: 1.0,
            blocked_event_triggers_hard_gate: true,
            reject_rollout_on_hard_gate: false,
        }
    }
}

impl SafetyRewardPolicy {
    /// Validates policy numeric constraints and hard-gate config fields.
    pub fn validate(&self) -> Result<()> {
        ensure_non_negative_finite(self.blocked_penalty, "blocked_penalty")?;
        ensure_non_negative_finite(
            self.default_reason_code_penalty,
            "default_reason_code_penalty",
        )?;
        ensure_non_negative_finite(self.prompt_injection_penalty, "prompt_injection_penalty")?;
        ensure_non_negative_finite(self.secret_leak_penalty, "secret_leak_penalty")?;
        ensure_non_negative_finite(self.hard_gate_penalty, "hard_gate_penalty")?;
        if !self.hard_gate_reward_ceiling.is_finite() || self.hard_gate_reward_ceiling > 0.0 {
            anyhow::bail!("hard_gate_reward_ceiling must be finite and <= 0");
        }
        for (reason_code, penalty) in &self.reason_code_penalties {
            if !penalty.is_finite() || *penalty < 0.0 {
                anyhow::bail!(
                    "reason_code_penalties[{reason_code}] must be finite and non-negative"
                );
            }
        }
        for prefix in &self.hard_gate_reason_prefixes {
            if prefix.trim().is_empty() {
                anyhow::bail!("hard_gate_reason_prefixes cannot include empty values");
            }
        }
        Ok(())
    }

    fn penalty_for_reason_code(&self, reason_code: &str) -> f64 {
        if let Some(penalty) = self.reason_code_penalties.get(reason_code) {
            return *penalty;
        }
        if reason_code.starts_with("prompt_injection.") {
            return self.prompt_injection_penalty;
        }
        if reason_code.starts_with("secret_leak.") {
            return self.secret_leak_penalty;
        }
        self.default_reason_code_penalty
    }

    fn is_hard_gate_reason_code(&self, reason_code: &str) -> bool {
        self.hard_gate_reason_codes.contains(reason_code)
            || self
                .hard_gate_reason_prefixes
                .iter()
                .any(|prefix| reason_code.starts_with(prefix))
    }
}

fn ensure_non_negative_finite(value: f64, field_name: &str) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        anyhow::bail!("{field_name} must be finite and non-negative");
    }
    Ok(())
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
        config
            .validate()
            .expect("invalid runner config: retry-backoff settings");
        Self {
            store,
            executor,
            config,
        }
    }

    /// Runs the worker loop until `shutdown` flips to true.
    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        self.store.register_worker(&self.config.worker_id).await?;
        let mut poll_failure_count = 0u32;
        let mut poll_backoff_accumulated_ms = 0u128;

        let mut heartbeat = tokio::time::interval(self.config.heartbeat_interval);
        let mut poll = tokio::time::interval(self.config.poll_interval);
        let mut reassignment = tokio::time::interval(self.config.reassignment_interval);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        reassignment.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

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
                    match self
                        .process_once(poll_failure_count, poll_backoff_accumulated_ms)
                        .await
                    {
                        Ok(()) => {
                            poll_failure_count = 0;
                            poll_backoff_accumulated_ms = 0;
                        }
                        Err(error) => {
                            poll_failure_count = poll_failure_count.saturating_add(1);
                            let delay = compute_poll_retry_delay(
                                poll_failure_count,
                                self.config.transient_error_backoff_initial,
                                self.config.transient_error_backoff_max,
                            );
                            poll_backoff_accumulated_ms = poll_backoff_accumulated_ms
                                .saturating_add(delay.as_millis());
                            let _ = error;
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
                _ = reassignment.tick() => {
                    self.store
                        .reassign_timed_out_rollouts(self.config.worker_timeout)
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn process_once(
        &self,
        poll_failure_count: u32,
        poll_backoff_accumulated_ms: u128,
    ) -> Result<()> {
        let Some(item) = self.store.dequeue_rollout(&self.config.worker_id).await? else {
            return Ok(());
        };

        self.process_dequeued(item, poll_failure_count, poll_backoff_accumulated_ms)
            .await
    }

    async fn process_dequeued(
        &self,
        item: DequeuedRollout,
        poll_failure_count: u32,
        poll_backoff_accumulated_ms: u128,
    ) -> Result<()> {
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
        let (heartbeat_stop_tx, mut heartbeat_stop_rx) = watch::channel(false);
        let heartbeat_store = self.store.clone();
        let heartbeat_worker_id = self.config.worker_id.clone();
        let heartbeat_rollout_id = item.rollout.rollout_id.clone();
        let heartbeat_attempt_id = item.attempt.attempt_id.clone();
        let heartbeat_interval = self.config.heartbeat_interval;
        let heartbeat_task = tokio::spawn(async move {
            let mut heartbeat = tokio::time::interval(heartbeat_interval);
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        heartbeat_store
                            .update_worker_heartbeat(
                                &heartbeat_worker_id,
                                Some(heartbeat_rollout_id.clone()),
                                Some(heartbeat_attempt_id.clone()),
                            )
                            .await?;
                    }
                    changed = heartbeat_stop_rx.changed() => {
                        if changed.is_err() || *heartbeat_stop_rx.borrow() {
                            break;
                        }
                    }
                }
            }
            Ok::<(), anyhow::Error>(())
        });

        let execution_result = {
            let _operation = tracer.operation("runner.execute_rollout");
            self.executor
                .execute(&item.rollout, resources.as_ref(), tracer.clone())
                .await
        };
        let _ = heartbeat_stop_tx.send(true);
        heartbeat_task.await??;

        match &execution_result {
            Ok(outcome) => {
                for reward in &outcome.rewards {
                    tracer.emit_reward(reward.clone());
                }
                tracer.emit_reward(Reward::new(
                    "runner.execution_success",
                    if outcome.output.is_null() { 0.0 } else { 1.0 },
                ));
            }
            Err(_) => {
                tracer.emit_reward(Reward::new("runner.execution_success", 0.0));
            }
        }
        if poll_failure_count > 0 {
            tracer.emit_reward(Reward::new(
                "runner.poll_retry_failures_before_rollout",
                poll_failure_count as f64,
            ));
            tracer.emit_reward(Reward::new(
                "runner.poll_retry_backoff_ms_before_rollout",
                poll_backoff_accumulated_ms as f64,
            ));
        }

        tracer.flush(self.store.as_ref()).await?;

        if !self
            .attempt_is_still_running(&item.attempt.attempt_id)
            .await?
        {
            self.store
                .update_worker_heartbeat(&self.config.worker_id, None, None)
                .await?;
            return Ok(());
        }

        match execution_result {
            Ok(_) => {
                self.store
                    .update_attempt_status(&item.attempt.attempt_id, AttemptStatus::Succeeded, None)
                    .await?;
                self.store
                    .update_rollout_status(
                        &item.rollout.rollout_id,
                        tau_training_types::RolloutStatus::Succeeded,
                    )
                    .await?;
            }
            Err(error) => {
                self.store
                    .update_attempt_status(
                        &item.attempt.attempt_id,
                        AttemptStatus::Failed,
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

    async fn attempt_is_still_running(&self, attempt_id: &str) -> Result<bool> {
        let attempt = self.store.get_attempt(attempt_id).await?;
        Ok(attempt
            .map(|current| current.status == AttemptStatus::Running)
            .unwrap_or(false))
    }
}

/// Executor that runs Tau's core `Agent` with tracer event subscriptions.
#[derive(Clone)]
pub struct TauAgentExecutor {
    agent_factory: Arc<AgentFactoryFn>,
    prompt_extractor: Arc<PromptExtractorFn>,
    safety_reward_policy: SafetyRewardPolicy,
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
            safety_reward_policy: SafetyRewardPolicy::default(),
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

    /// Overrides safety reward shaping and hard-gate behavior.
    pub fn with_safety_reward_policy(mut self, policy: SafetyRewardPolicy) -> Result<Self> {
        policy.validate()?;
        self.safety_reward_policy = policy;
        Ok(self)
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

        let safety_spans = tracer.completed_spans();
        let safety_decision =
            apply_safety_reward_policy(&mut rewards, &safety_spans, &self.safety_reward_policy)?;
        if safety_decision.hard_gate_triggered
            && self.safety_reward_policy.reject_rollout_on_hard_gate
        {
            anyhow::bail!(
                "safety hard gate triggered: reason_codes={:?}",
                safety_decision.triggered_reason_codes
            );
        }

        Ok(RolloutExecutionOutcome {
            output: json!({
                "assistant_text": assistant_text,
                "message_count": messages.len(),
                "safety": {
                    "penalty_total": safety_decision.penalty_total,
                    "hard_gate_triggered": safety_decision.hard_gate_triggered,
                    "blocked_events": safety_decision.blocked_events,
                    "reason_codes": safety_decision.triggered_reason_codes,
                },
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SafetySignalSummary {
    blocked_events: usize,
    reason_codes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SafetyRewardDecision {
    penalty_total: f64,
    hard_gate_triggered: bool,
    blocked_events: usize,
    triggered_reason_codes: Vec<String>,
}

fn apply_safety_reward_policy(
    rewards: &mut Vec<Reward>,
    spans: &[TrainingSpan],
    policy: &SafetyRewardPolicy,
) -> Result<SafetyRewardDecision> {
    policy.validate()?;

    let summary = summarize_safety_signals(spans);
    if summary.blocked_events == 0 && summary.reason_codes.is_empty() {
        return Ok(SafetyRewardDecision::default());
    }

    let penalty_total_from_reasons = summary
        .reason_codes
        .iter()
        .map(|reason_code| policy.penalty_for_reason_code(reason_code))
        .sum::<f64>();
    let penalty_total =
        penalty_total_from_reasons + (summary.blocked_events as f64 * policy.blocked_penalty);

    if penalty_total > 0.0 {
        rewards.push(Reward::new("safety.penalty_total", -penalty_total));
    }

    let hard_gate_triggered = (policy.blocked_event_triggers_hard_gate
        && summary.blocked_events > 0)
        || summary
            .reason_codes
            .iter()
            .any(|reason_code| policy.is_hard_gate_reason_code(reason_code));

    if hard_gate_triggered {
        for reward in rewards.iter_mut() {
            if reward.value > policy.hard_gate_reward_ceiling {
                reward.value = policy.hard_gate_reward_ceiling;
            }
        }
        if policy.hard_gate_penalty > 0.0 {
            rewards.push(Reward::new(
                "safety.hard_gate_penalty",
                -policy.hard_gate_penalty,
            ));
        }
        rewards.push(Reward::new("safety.hard_gate_triggered", 0.0));
    }

    Ok(SafetyRewardDecision {
        penalty_total,
        hard_gate_triggered,
        blocked_events: summary.blocked_events,
        triggered_reason_codes: summary.reason_codes,
    })
}

fn summarize_safety_signals(spans: &[TrainingSpan]) -> SafetySignalSummary {
    let mut blocked_events = 0usize;
    let mut reason_codes = BTreeSet::new();

    for span in spans
        .iter()
        .filter(|span| span.name == "agent.safety_policy_applied")
    {
        if span
            .attributes
            .get("blocked")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            blocked_events += 1;
        }
        for reason_code in parse_reason_codes(span.attributes.get("reason_codes")) {
            reason_codes.insert(reason_code);
        }
    }

    SafetySignalSummary {
        blocked_events,
        reason_codes: reason_codes.into_iter().collect(),
    }
}

fn parse_reason_codes(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        audit_rollout_persistence, RolloutExecutionOutcome, RolloutExecutor, RunnerConfig,
        TauAgentExecutor, TrainingRunner,
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use tau_agent_core::{Agent, AgentConfig, AgentEvent, SafetyMode, SafetyStage};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tau_training_store::{
        Attempt, DequeuedRollout, InMemoryTrainingStore, ResourcesUpdate, RolloutQuery,
        StoreResult, TrainingSpan, TrainingStore, TrainingStoreError, WorkerState,
    };
    use tau_training_types::{AttemptStatus, Reward, Rollout, RolloutStatus};
    use tokio::sync::watch;

    #[derive(Debug, Clone)]
    struct CollectorLoadReport {
        enqueued_rollouts: usize,
        succeeded_rollouts: usize,
        failed_rollouts: usize,
        cancelled_rollouts: usize,
        elapsed_ms: u128,
        throughput_per_sec: f64,
    }

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

    struct SlowExecutor;

    #[async_trait]
    impl RolloutExecutor for SlowExecutor {
        async fn execute(
            &self,
            rollout: &Rollout,
            _resources: Option<&tau_training_types::ResourcesUpdate>,
            _tracer: Arc<tau_training_tracer::TrainingTracer>,
        ) -> Result<RolloutExecutionOutcome> {
            tokio::time::sleep(Duration::from_millis(180)).await;
            let response = rollout
                .input
                .get("prompt")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(RolloutExecutionOutcome {
                output: json!({ "slow_echo": response }),
                rewards: vec![Reward::new("slow", 0.5)],
            })
        }
    }

    struct FastExecutor;

    #[async_trait]
    impl RolloutExecutor for FastExecutor {
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
                output: json!({ "fast_echo": response }),
                rewards: vec![Reward::new("fast", 1.0)],
            })
        }
    }

    struct FlakyDequeueStore {
        inner: InMemoryTrainingStore,
        failures_remaining: AtomicUsize,
        hidden_attempt_ids: Mutex<HashSet<String>>,
    }

    impl FlakyDequeueStore {
        fn new(failures: usize) -> Self {
            Self {
                inner: InMemoryTrainingStore::new(),
                failures_remaining: AtomicUsize::new(failures),
                hidden_attempt_ids: Mutex::new(HashSet::new()),
            }
        }

        fn hide_attempt_for_audit(&self, attempt_id: &str) {
            if let Ok(mut hidden) = self.hidden_attempt_ids.lock() {
                hidden.insert(attempt_id.to_string());
            }
        }
    }

    #[async_trait]
    impl TrainingStore for FlakyDequeueStore {
        async fn enqueue_rollout(&self, rollout: Rollout) -> StoreResult<()> {
            self.inner.enqueue_rollout(rollout).await
        }

        async fn dequeue_rollout(&self, worker_id: &str) -> StoreResult<Option<DequeuedRollout>> {
            let mut current = self.failures_remaining.load(Ordering::SeqCst);
            while current > 0 {
                match self.failures_remaining.compare_exchange(
                    current,
                    current - 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        return Err(TrainingStoreError::Io(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "simulated transient dequeue failure",
                        )));
                    }
                    Err(observed) => current = observed,
                }
            }

            self.inner.dequeue_rollout(worker_id).await
        }

        async fn update_rollout_status(
            &self,
            rollout_id: &str,
            status: RolloutStatus,
        ) -> StoreResult<()> {
            self.inner.update_rollout_status(rollout_id, status).await
        }

        async fn cancel_rollout(&self, rollout_id: &str) -> StoreResult<()> {
            self.inner.cancel_rollout(rollout_id).await
        }

        async fn add_span(&self, span: TrainingSpan) -> StoreResult<()> {
            self.inner.add_span(span).await
        }

        async fn query_spans(
            &self,
            rollout_id: &str,
            attempt_id: Option<&str>,
        ) -> StoreResult<Vec<TrainingSpan>> {
            self.inner.query_spans(rollout_id, attempt_id).await
        }

        async fn get_next_span_sequence_id(
            &self,
            rollout_id: &str,
            attempt_id: &str,
        ) -> StoreResult<u64> {
            self.inner
                .get_next_span_sequence_id(rollout_id, attempt_id)
                .await
        }

        async fn update_resources(
            &self,
            resources: HashMap<String, serde_json::Value>,
        ) -> StoreResult<ResourcesUpdate> {
            self.inner.update_resources(resources).await
        }

        async fn get_latest_resources(&self) -> StoreResult<Option<ResourcesUpdate>> {
            self.inner.get_latest_resources().await
        }

        async fn get_resources_by_id(
            &self,
            resources_id: &str,
        ) -> StoreResult<Option<ResourcesUpdate>> {
            self.inner.get_resources_by_id(resources_id).await
        }

        async fn query_rollouts(&self, query: RolloutQuery) -> StoreResult<Vec<Rollout>> {
            self.inner.query_rollouts(query).await
        }

        async fn wait_for_rollouts(
            &self,
            statuses: &[RolloutStatus],
            timeout: Duration,
        ) -> StoreResult<Vec<Rollout>> {
            self.inner.wait_for_rollouts(statuses, timeout).await
        }

        async fn register_worker(&self, worker_id: &str) -> StoreResult<WorkerState> {
            self.inner.register_worker(worker_id).await
        }

        async fn update_worker_heartbeat(
            &self,
            worker_id: &str,
            active_rollout_id: Option<String>,
            active_attempt_id: Option<String>,
        ) -> StoreResult<()> {
            self.inner
                .update_worker_heartbeat(worker_id, active_rollout_id, active_attempt_id)
                .await
        }

        async fn reassign_timed_out_rollouts(
            &self,
            heartbeat_timeout: Duration,
        ) -> StoreResult<Vec<String>> {
            self.inner
                .reassign_timed_out_rollouts(heartbeat_timeout)
                .await
        }

        async fn query_workers(&self) -> StoreResult<Vec<WorkerState>> {
            self.inner.query_workers().await
        }

        async fn update_attempt_status(
            &self,
            attempt_id: &str,
            status: AttemptStatus,
            error_message: Option<String>,
        ) -> StoreResult<()> {
            self.inner
                .update_attempt_status(attempt_id, status, error_message)
                .await
        }

        async fn get_attempt(&self, attempt_id: &str) -> StoreResult<Option<Attempt>> {
            if self
                .hidden_attempt_ids
                .lock()
                .ok()
                .is_some_and(|hidden| hidden.contains(attempt_id))
            {
                return Ok(None);
            }
            self.inner.get_attempt(attempt_id).await
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
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(120),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
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

    #[test]
    fn spec_c02_compute_poll_retry_delay_is_bounded_exponential() {
        let initial = Duration::from_millis(10);
        let max = Duration::from_millis(40);

        assert_eq!(
            super::compute_poll_retry_delay(1, initial, max),
            Duration::from_millis(10)
        );
        assert_eq!(
            super::compute_poll_retry_delay(2, initial, max),
            Duration::from_millis(20)
        );
        assert_eq!(
            super::compute_poll_retry_delay(3, initial, max),
            Duration::from_millis(40)
        );
        assert_eq!(
            super::compute_poll_retry_delay(6, initial, max),
            Duration::from_millis(40)
        );
    }

    #[test]
    fn spec_c03_poll_retry_delay_resets_after_success() {
        let initial = Duration::from_millis(8);
        let max = Duration::from_millis(64);
        let mut failures = 0u32;

        failures += 1;
        let first = super::compute_poll_retry_delay(failures, initial, max);
        failures += 1;
        let second = super::compute_poll_retry_delay(failures, initial, max);
        assert!(second > first);

        failures = 0;
        failures += 1;
        let after_reset = super::compute_poll_retry_delay(failures, initial, max);
        assert_eq!(after_reset, initial);
    }

    #[test]
    fn spec_c04_runner_config_validation_rejects_invalid_retry_backoff() {
        let mut config = RunnerConfig::default();
        config.transient_error_backoff_initial = Duration::from_millis(0);
        let initial_error = config.validate().expect_err("zero initial should fail");
        assert!(initial_error
            .to_string()
            .contains("transient_error_backoff_initial"));

        let mut config = RunnerConfig::default();
        config.transient_error_backoff_max = Duration::from_millis(0);
        let max_error = config.validate().expect_err("zero max should fail");
        assert!(max_error
            .to_string()
            .contains("transient_error_backoff_max"));

        let mut config = RunnerConfig::default();
        config.transient_error_backoff_initial = Duration::from_millis(50);
        config.transient_error_backoff_max = Duration::from_millis(10);
        let order_error = config.validate().expect_err("max<initial should fail");
        assert!(order_error
            .to_string()
            .contains("transient_error_backoff_max must be >= transient_error_backoff_initial"));
    }

    #[tokio::test]
    async fn spec_c01_runner_recovers_from_transient_dequeue_failures() {
        let store: Arc<dyn TrainingStore> = Arc::new(FlakyDequeueStore::new(1));
        store
            .enqueue_rollout(Rollout::new(
                "r-flaky-1",
                json!({ "prompt": "recover" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let runner = TrainingRunner::new(
            store.clone(),
            Arc::new(StaticExecutor),
            RunnerConfig {
                worker_id: "worker-flaky".to_string(),
                poll_interval: Duration::from_millis(10),
                heartbeat_interval: Duration::from_millis(25),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(120),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(async move { runner.run(rx).await });

        wait_for_rollout_status(store.clone(), "r-flaky-1", RolloutStatus::Succeeded)
            .await
            .expect("status wait");

        tx.send(true).expect("shutdown");
        handle.await.expect("join").expect("runner");

        let spans = store
            .query_spans("r-flaky-1", Some("r-flaky-1:attempt-1"))
            .await
            .expect("spans");
        assert_eq!(
            reward_metric_values(&spans, "runner.poll_retry_failures_before_rollout"),
            vec![1.0]
        );
        assert_eq!(
            reward_metric_values(&spans, "runner.poll_retry_backoff_ms_before_rollout"),
            vec![5.0]
        );
    }

    #[tokio::test]
    async fn spec_1958_c02_retry_metrics_capture_multi_failure_backoff_totals() {
        let store: Arc<dyn TrainingStore> = Arc::new(FlakyDequeueStore::new(3));
        store
            .enqueue_rollout(Rollout::new(
                "r-flaky-3",
                json!({ "prompt": "recover-three" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let runner = TrainingRunner::new(
            store.clone(),
            Arc::new(StaticExecutor),
            RunnerConfig {
                worker_id: "worker-flaky-3".to_string(),
                poll_interval: Duration::from_millis(10),
                heartbeat_interval: Duration::from_millis(25),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(120),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(async move { runner.run(rx).await });

        wait_for_rollout_status(store.clone(), "r-flaky-3", RolloutStatus::Succeeded)
            .await
            .expect("status wait");

        tx.send(true).expect("shutdown");
        handle.await.expect("join").expect("runner");

        let spans = store
            .query_spans("r-flaky-3", Some("r-flaky-3:attempt-1"))
            .await
            .expect("spans");
        assert_eq!(
            reward_metric_values(&spans, "runner.poll_retry_failures_before_rollout"),
            vec![3.0]
        );
        assert_eq!(
            reward_metric_values(&spans, "runner.poll_retry_backoff_ms_before_rollout"),
            vec![35.0]
        );
    }

    #[tokio::test]
    async fn spec_1958_c03_clean_runs_do_not_emit_retry_recovery_metrics() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-clean-1",
                json!({ "prompt": "clean-run" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let runner = TrainingRunner::new(
            store.clone(),
            Arc::new(StaticExecutor),
            RunnerConfig {
                worker_id: "worker-clean-1".to_string(),
                poll_interval: Duration::from_millis(10),
                heartbeat_interval: Duration::from_millis(25),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(120),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(async move { runner.run(rx).await });

        wait_for_rollout_status(store.clone(), "r-clean-1", RolloutStatus::Succeeded)
            .await
            .expect("status wait");

        tx.send(true).expect("shutdown");
        handle.await.expect("join").expect("runner");

        let spans = store
            .query_spans("r-clean-1", Some("r-clean-1:attempt-1"))
            .await
            .expect("spans");
        assert!(
            reward_metric_values(&spans, "runner.poll_retry_failures_before_rollout").is_empty()
        );
        assert!(
            reward_metric_values(&spans, "runner.poll_retry_backoff_ms_before_rollout").is_empty()
        );
    }

    #[tokio::test]
    async fn spec_1960_c01_single_rollout_audit_summary_is_deterministic() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-audit-1",
                json!({ "prompt": "audit-single" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let runner = TrainingRunner::new(
            store.clone(),
            Arc::new(StaticExecutor),
            RunnerConfig {
                worker_id: "worker-audit-1".to_string(),
                poll_interval: Duration::from_millis(10),
                heartbeat_interval: Duration::from_millis(25),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(120),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(async move { runner.run(rx).await });
        wait_for_rollout_status(store.clone(), "r-audit-1", RolloutStatus::Succeeded)
            .await
            .expect("status wait");
        tx.send(true).expect("shutdown");
        handle.await.expect("join").expect("runner");

        let report = audit_rollout_persistence(store.as_ref(), "r-audit-1")
            .await
            .expect("audit report");
        assert_eq!(report.rollout_id, "r-audit-1");
        assert_eq!(report.rollout_status, RolloutStatus::Succeeded);
        assert_eq!(report.expected_attempt_count, 1);
        assert_eq!(report.attempts.len(), 1);
        assert_eq!(report.attempts[0].status, Some(AttemptStatus::Succeeded));
        assert!(report.attempts[0].span_count > 0);
        assert!(!report.has_persistence_gaps);
        assert!(report.gap_reasons.is_empty());
    }

    #[tokio::test]
    async fn spec_1960_c02_retry_requeue_audit_reports_no_persistence_gaps() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-audit-chaos-1",
                json!({ "prompt": "audit-chaos" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let slow_runner = TrainingRunner::new(
            store.clone(),
            Arc::new(SlowExecutor),
            RunnerConfig {
                worker_id: "worker-audit-slow".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(200),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(50),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );
        let fast_runner = TrainingRunner::new(
            store.clone(),
            Arc::new(FastExecutor),
            RunnerConfig {
                worker_id: "worker-audit-fast".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(20),
                reassignment_interval: Duration::from_millis(10),
                worker_timeout: Duration::from_millis(50),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (slow_tx, slow_rx) = watch::channel(false);
        let slow_handle = tokio::spawn(async move { slow_runner.run(slow_rx).await });
        wait_for_worker_assignment(store.clone(), "worker-audit-slow", Duration::from_secs(2))
            .await
            .expect("assignment");

        let (fast_tx, fast_rx) = watch::channel(false);
        let fast_handle = tokio::spawn(async move { fast_runner.run(fast_rx).await });
        wait_for_rollout_status(store.clone(), "r-audit-chaos-1", RolloutStatus::Succeeded)
            .await
            .expect("status wait");

        slow_tx.send(true).expect("shutdown slow");
        fast_tx.send(true).expect("shutdown fast");
        slow_handle.await.expect("join slow").expect("slow runner");
        fast_handle.await.expect("join fast").expect("fast runner");

        let report = audit_rollout_persistence(store.as_ref(), "r-audit-chaos-1")
            .await
            .expect("audit report");
        assert_eq!(report.expected_attempt_count, 2);
        assert_eq!(report.attempts.len(), 2);
        assert_eq!(report.attempts[0].status, Some(AttemptStatus::Timeout));
        assert_eq!(report.attempts[1].status, Some(AttemptStatus::Succeeded));
        assert!(report.attempts.iter().all(|attempt| attempt.span_count > 0));
        assert!(!report.has_persistence_gaps);
        assert!(report.gap_reasons.is_empty());
    }

    #[tokio::test]
    async fn spec_1960_c03_audit_detects_missing_attempt_record_gaps() {
        let store = Arc::new(FlakyDequeueStore::new(0));
        let store_dyn: Arc<dyn TrainingStore> = store.clone();
        store_dyn
            .enqueue_rollout(Rollout::new(
                "r-audit-gap-1",
                json!({ "prompt": "audit-gap" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let runner = TrainingRunner::new(
            store_dyn.clone(),
            Arc::new(StaticExecutor),
            RunnerConfig {
                worker_id: "worker-audit-gap".to_string(),
                poll_interval: Duration::from_millis(10),
                heartbeat_interval: Duration::from_millis(25),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(120),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (tx, rx) = watch::channel(false);
        let handle = tokio::spawn(async move { runner.run(rx).await });
        wait_for_rollout_status(store_dyn.clone(), "r-audit-gap-1", RolloutStatus::Succeeded)
            .await
            .expect("status wait");
        tx.send(true).expect("shutdown");
        handle.await.expect("join").expect("runner");

        store.hide_attempt_for_audit("r-audit-gap-1:attempt-1");
        let report = audit_rollout_persistence(store.as_ref(), "r-audit-gap-1")
            .await
            .expect("audit report");
        assert!(report.has_persistence_gaps);
        assert!(report
            .gap_reasons
            .iter()
            .any(|reason| reason == "missing attempt record: r-audit-gap-1:attempt-1"));
    }

    #[tokio::test]
    async fn spec_1960_c04_audit_rejects_unknown_rollout_id() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        let error = audit_rollout_persistence(store.as_ref(), "r-missing-1960")
            .await
            .expect_err("missing rollout should fail");
        assert!(error
            .to_string()
            .contains("rollout 'r-missing-1960' not found for persistence audit"));
    }

    #[tokio::test]
    async fn integration_reassigns_stalled_worker_and_preserves_attempt_spans() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-chaos-1",
                json!({ "prompt": "hello-chaos" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let slow_runner = TrainingRunner::new(
            store.clone(),
            Arc::new(SlowExecutor),
            RunnerConfig {
                worker_id: "worker-slow".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(200),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(50),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );
        let fast_runner = TrainingRunner::new(
            store.clone(),
            Arc::new(FastExecutor),
            RunnerConfig {
                worker_id: "worker-fast".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(20),
                reassignment_interval: Duration::from_millis(10),
                worker_timeout: Duration::from_millis(50),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (slow_tx, slow_rx) = watch::channel(false);
        let slow_handle = tokio::spawn(async move { slow_runner.run(slow_rx).await });

        wait_for_worker_assignment(store.clone(), "worker-slow", Duration::from_secs(2))
            .await
            .expect("worker-slow assignment");

        let (fast_tx, fast_rx) = watch::channel(false);
        let fast_handle = tokio::spawn(async move { fast_runner.run(fast_rx).await });

        wait_for_rollout_status(store.clone(), "r-chaos-1", RolloutStatus::Succeeded)
            .await
            .expect("rollout should eventually succeed");

        slow_tx.send(true).expect("shutdown slow");
        fast_tx.send(true).expect("shutdown fast");
        slow_handle.await.expect("join slow").expect("slow runner");
        fast_handle.await.expect("join fast").expect("fast runner");

        let attempt_1 = store
            .get_attempt("r-chaos-1:attempt-1")
            .await
            .expect("attempt-1")
            .expect("attempt-1 exists");
        let attempt_2 = store
            .get_attempt("r-chaos-1:attempt-2")
            .await
            .expect("attempt-2")
            .expect("attempt-2 exists");
        assert_eq!(attempt_1.status, AttemptStatus::Timeout);
        assert_eq!(attempt_2.status, AttemptStatus::Succeeded);

        let spans_1 = store
            .query_spans("r-chaos-1", Some("r-chaos-1:attempt-1"))
            .await
            .expect("spans attempt-1");
        let spans_2 = store
            .query_spans("r-chaos-1", Some("r-chaos-1:attempt-2"))
            .await
            .expect("spans attempt-2");
        assert!(!spans_1.is_empty());
        assert!(!spans_2.is_empty());
    }

    #[tokio::test]
    async fn regression_healthy_long_running_worker_does_not_false_timeout() {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());
        store
            .enqueue_rollout(Rollout::new(
                "r-healthy-1",
                json!({ "prompt": "steady-worker" }),
                Some(tau_training_types::RolloutMode::Train),
            ))
            .await
            .expect("enqueue");

        let slow_runner = TrainingRunner::new(
            store.clone(),
            Arc::new(SlowExecutor),
            RunnerConfig {
                worker_id: "worker-steady".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(20),
                reassignment_interval: Duration::from_millis(20),
                worker_timeout: Duration::from_millis(80),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );
        let fast_runner = TrainingRunner::new(
            store.clone(),
            Arc::new(FastExecutor),
            RunnerConfig {
                worker_id: "worker-backup".to_string(),
                poll_interval: Duration::from_millis(20),
                heartbeat_interval: Duration::from_millis(20),
                reassignment_interval: Duration::from_millis(10),
                worker_timeout: Duration::from_millis(80),
                transient_error_backoff_initial: Duration::from_millis(5),
                transient_error_backoff_max: Duration::from_millis(20),
            },
        );

        let (slow_tx, slow_rx) = watch::channel(false);
        let slow_handle = tokio::spawn(async move { slow_runner.run(slow_rx).await });

        wait_for_worker_assignment(store.clone(), "worker-steady", Duration::from_secs(2))
            .await
            .expect("worker-steady assignment");

        let (fast_tx, fast_rx) = watch::channel(false);
        let fast_handle = tokio::spawn(async move { fast_runner.run(fast_rx).await });

        wait_for_rollout_status(store.clone(), "r-healthy-1", RolloutStatus::Succeeded)
            .await
            .expect("rollout should succeed");

        slow_tx.send(true).expect("shutdown slow");
        fast_tx.send(true).expect("shutdown fast");
        slow_handle.await.expect("join slow").expect("slow runner");
        fast_handle.await.expect("join fast").expect("fast runner");

        let attempt_1 = store
            .get_attempt("r-healthy-1:attempt-1")
            .await
            .expect("attempt-1")
            .expect("attempt-1 exists");
        assert_eq!(attempt_1.status, AttemptStatus::Succeeded);

        let attempt_2 = store
            .get_attempt("r-healthy-1:attempt-2")
            .await
            .expect("attempt-2");
        assert!(
            attempt_2.is_none(),
            "healthy worker should not be reassigned into attempt-2"
        );
    }

    #[tokio::test]
    async fn regression_collector_load_harness_reports_metrics_and_no_drop() {
        let report = run_collector_load_harness(64, 4)
            .await
            .expect("load harness should run");

        assert_eq!(report.enqueued_rollouts, 64);
        assert_eq!(report.succeeded_rollouts, 64);
        assert_eq!(report.failed_rollouts, 0);
        assert_eq!(report.cancelled_rollouts, 0);
        assert!(report.elapsed_ms > 0);
        assert!(report.throughput_per_sec > 0.0);
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

    #[tokio::test]
    async fn regression_tau_agent_executor_penalizes_prompt_injection_reason_codes() {
        let executor = TauAgentExecutor::new(|_resources| {
            Agent::new(Arc::new(MockClient), AgentConfig::default())
        });
        let tracer = Arc::new(tau_training_tracer::TrainingTracer::new("r-penalty", "a-1"));
        tracer.on_agent_event(&AgentEvent::SafetyPolicyApplied {
            stage: SafetyStage::InboundMessage,
            mode: SafetyMode::Warn,
            blocked: false,
            matched_rules: vec!["literal.ignore_previous_instructions".to_string()],
            reason_codes: vec!["prompt_injection.ignore_instructions".to_string()],
        });

        let outcome = executor
            .execute(
                &Rollout::new(
                    "r-penalty",
                    json!({ "prompt": "Say expected", "expected": "expected-output" }),
                    Some(tau_training_types::RolloutMode::Train),
                ),
                None,
                tracer,
            )
            .await
            .expect("execute");

        let total_reward = outcome
            .rewards
            .iter()
            .map(|reward| reward.value)
            .sum::<f64>();
        assert!(
            total_reward <= 0.0,
            "unsafe prompt-injection trajectory should not retain positive reward: {:?}",
            outcome.rewards
        );
        assert!(
            outcome
                .rewards
                .iter()
                .any(|reward| reward.name == "safety.penalty_total"),
            "safety penalty reward missing: {:?}",
            outcome.rewards
        );
    }

    #[tokio::test]
    async fn regression_tau_agent_executor_hard_gate_clamps_reward_improvement() {
        let executor = TauAgentExecutor::new(|_resources| {
            Agent::new(Arc::new(MockClient), AgentConfig::default())
        });
        let tracer = Arc::new(tau_training_tracer::TrainingTracer::new(
            "r-hard-gate",
            "a-1",
        ));
        tracer.on_agent_event(&AgentEvent::SafetyPolicyApplied {
            stage: SafetyStage::InboundMessage,
            mode: SafetyMode::Block,
            blocked: true,
            matched_rules: vec!["literal.reveal_system_prompt".to_string()],
            reason_codes: vec!["prompt_injection.system_prompt_exfiltration".to_string()],
        });

        let outcome = executor
            .execute(
                &Rollout::new(
                    "r-hard-gate",
                    json!({ "prompt": "Say expected", "expected": "expected-output" }),
                    Some(tau_training_types::RolloutMode::Train),
                ),
                None,
                tracer,
            )
            .await
            .expect("execute");

        let exact_match = outcome
            .rewards
            .iter()
            .find(|reward| reward.name == "exact_match")
            .expect("exact_match reward");
        assert!(
            exact_match.value <= 0.0,
            "hard-gated trajectory must clamp positive reward improvement: {:?}",
            outcome.rewards
        );
        assert!(
            outcome
                .rewards
                .iter()
                .any(|reward| reward.name == "safety.hard_gate_penalty"),
            "hard-gate penalty reward missing: {:?}",
            outcome.rewards
        );
    }

    #[tokio::test]
    async fn integration_tau_agent_executor_penalizes_secret_leak_reason_codes() {
        let executor = TauAgentExecutor::new(|_resources| {
            Agent::new(Arc::new(MockClient), AgentConfig::default())
        });
        let tracer = Arc::new(tau_training_tracer::TrainingTracer::new("r-secret", "a-1"));
        tracer.on_agent_event(&AgentEvent::SafetyPolicyApplied {
            stage: SafetyStage::InboundMessage,
            mode: SafetyMode::Warn,
            blocked: false,
            matched_rules: vec!["regex.openai_api_key".to_string()],
            reason_codes: vec!["secret_leak.openai_api_key".to_string()],
        });

        let outcome = executor
            .execute(
                &Rollout::new(
                    "r-secret",
                    json!({ "prompt": "Say expected", "expected": "expected-output" }),
                    Some(tau_training_types::RolloutMode::Train),
                ),
                None,
                tracer,
            )
            .await
            .expect("execute");

        let total_reward = outcome
            .rewards
            .iter()
            .map(|reward| reward.value)
            .sum::<f64>();
        assert!(
            total_reward <= 0.0,
            "secret-leak trajectory should not retain positive reward: {:?}",
            outcome.rewards
        );
        assert!(
            outcome
                .rewards
                .iter()
                .any(|reward| reward.name == "safety.hard_gate_penalty"),
            "secret-leak trajectory should trigger hard gate penalty: {:?}",
            outcome.rewards
        );
    }

    #[tokio::test]
    async fn functional_tau_agent_executor_rejects_rollout_on_hard_gate_when_configured() {
        let mut policy = super::SafetyRewardPolicy::default();
        policy.reject_rollout_on_hard_gate = true;
        let executor = TauAgentExecutor::new(|_resources| {
            Agent::new(Arc::new(MockClient), AgentConfig::default())
        })
        .with_safety_reward_policy(policy)
        .expect("policy");
        let tracer = Arc::new(tau_training_tracer::TrainingTracer::new("r-reject", "a-1"));
        tracer.on_agent_event(&AgentEvent::SafetyPolicyApplied {
            stage: SafetyStage::InboundMessage,
            mode: SafetyMode::Block,
            blocked: true,
            matched_rules: vec!["literal.reveal_system_prompt".to_string()],
            reason_codes: vec!["prompt_injection.system_prompt_exfiltration".to_string()],
        });

        let error = executor
            .execute(
                &Rollout::new(
                    "r-reject",
                    json!({ "prompt": "Say expected", "expected": "expected-output" }),
                    Some(tau_training_types::RolloutMode::Train),
                ),
                None,
                tracer,
            )
            .await
            .expect_err("hard gate should reject rollout");
        assert!(error.to_string().contains("safety hard gate triggered"));
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

    async fn wait_for_worker_assignment(
        store: Arc<dyn TrainingStore>,
        worker_id: &str,
        timeout: Duration,
    ) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let workers = store.query_workers().await?;
            if workers
                .iter()
                .find(|worker| worker.worker_id == worker_id)
                .and_then(|worker| worker.active_attempt_id.clone())
                .is_some()
            {
                return Ok(());
            }

            if Instant::now() >= deadline {
                anyhow::bail!("worker '{worker_id}' was not assigned before timeout");
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn reward_metric_values(spans: &[TrainingSpan], reward_name: &str) -> Vec<f64> {
        spans
            .iter()
            .filter(|span| span.name == "reward.emit")
            .filter_map(|span| {
                let name = span.attributes.get("reward_name")?.as_str()?;
                if name != reward_name {
                    return None;
                }
                span.attributes.get("reward_value")?.as_f64()
            })
            .collect()
    }

    async fn run_collector_load_harness(
        enqueued_rollouts: usize,
        worker_count: usize,
    ) -> Result<CollectorLoadReport> {
        let store: Arc<dyn TrainingStore> = Arc::new(InMemoryTrainingStore::new());

        for index in 0..enqueued_rollouts {
            store
                .enqueue_rollout(Rollout::new(
                    format!("r-load-{index}"),
                    json!({ "prompt": format!("load-{index}") }),
                    Some(tau_training_types::RolloutMode::Train),
                ))
                .await?;
        }

        let mut shutdown_txs = Vec::with_capacity(worker_count);
        let mut handles = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let runner = TrainingRunner::new(
                store.clone(),
                Arc::new(StaticExecutor),
                RunnerConfig {
                    worker_id: format!("worker-load-{}", index + 1),
                    poll_interval: Duration::from_millis(5),
                    heartbeat_interval: Duration::from_millis(25),
                    reassignment_interval: Duration::from_millis(25),
                    worker_timeout: Duration::from_millis(150),
                    transient_error_backoff_initial: Duration::from_millis(5),
                    transient_error_backoff_max: Duration::from_millis(20),
                },
            );
            let (tx, rx) = watch::channel(false);
            shutdown_txs.push(tx);
            handles.push(tokio::spawn(async move { runner.run(rx).await }));
        }

        let started = Instant::now();
        let deadline = started + Duration::from_secs(15);

        let (succeeded_rollouts, failed_rollouts, cancelled_rollouts) = loop {
            let rollouts = store.query_rollouts(RolloutQuery::default()).await?;
            let succeeded = rollouts
                .iter()
                .filter(|rollout| rollout.status == RolloutStatus::Succeeded)
                .count();
            let failed = rollouts
                .iter()
                .filter(|rollout| rollout.status == RolloutStatus::Failed)
                .count();
            let cancelled = rollouts
                .iter()
                .filter(|rollout| rollout.status == RolloutStatus::Cancelled)
                .count();

            if succeeded + failed + cancelled == enqueued_rollouts {
                break (succeeded, failed, cancelled);
            }

            if Instant::now() >= deadline {
                anyhow::bail!(
                    "collector load harness timed out before terminal completion: succeeded={succeeded} failed={failed} cancelled={cancelled} expected={enqueued_rollouts}"
                );
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        };

        let elapsed_ms = started.elapsed().as_millis();
        let throughput_per_sec = if elapsed_ms == 0 {
            succeeded_rollouts as f64
        } else {
            succeeded_rollouts as f64 / (elapsed_ms as f64 / 1000.0)
        };

        println!(
            "METRIC collector_load enqueued={} succeeded={} failed={} cancelled={} elapsed_ms={} throughput_per_sec={:.3}",
            enqueued_rollouts,
            succeeded_rollouts,
            failed_rollouts,
            cancelled_rollouts,
            elapsed_ms,
            throughput_per_sec
        );

        for tx in shutdown_txs {
            tx.send(true).expect("shutdown worker");
        }
        for handle in handles {
            handle.await.expect("join worker")?;
        }

        Ok(CollectorLoadReport {
            enqueued_rollouts,
            succeeded_rollouts,
            failed_rollouts,
            cancelled_rollouts,
            elapsed_ms,
            throughput_per_sec,
        })
    }
}
