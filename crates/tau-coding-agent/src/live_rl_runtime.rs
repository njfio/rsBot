//! Live RL runtime bridge for wiring agent decision traces into rollout updates.

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tau_agent_core::{Agent, AgentEvent};
use tau_ai::MessageRole;
use tau_algorithm::{
    collect_trajectory_batch, compute_gae_batch_from_slices, compute_ppo_update, GaeConfig,
    PpoConfig, PpoSample,
};
use tau_training_store::{
    Rollout, RolloutQuery, RolloutStatus, SqliteTrainingStore, TrainingSpan, TrainingStore,
};
use tokio::sync::Mutex;

const LIVE_RL_ENABLED_ENV: &str = "TAU_LIVE_RL_ENABLED";
const LIVE_RL_STORE_PATH_ENV: &str = "TAU_LIVE_RL_STORE_SQLITE";
const LIVE_RL_UPDATE_INTERVAL_ENV: &str = "TAU_LIVE_RL_UPDATE_INTERVAL";
const LIVE_RL_MAX_ROLLOUTS_ENV: &str = "TAU_LIVE_RL_MAX_ROLLOUTS_PER_UPDATE";
const LIVE_RL_MAX_FAILURE_STREAK_ENV: &str = "TAU_LIVE_RL_MAX_FAILURE_STREAK";
const LIVE_ROLLOUT_PREFIX: &str = "live-rl-rollout";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveRlRuntimeGate {
    Pass,
    Hold,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveRlOptimizerReport {
    pub executed: bool,
    pub trajectories: usize,
    pub samples: usize,
    pub mean_total_loss: Option<f64>,
    pub observed_approx_kl: Option<f64>,
    pub early_stop_triggered: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveRlRuntimeSnapshot {
    pub enabled: bool,
    pub store_path: PathBuf,
    pub gate: LiveRlRuntimeGate,
    pub completed_rollouts: usize,
    pub consecutive_failures: usize,
    pub last_error: Option<String>,
    pub last_optimizer_report: Option<LiveRlOptimizerReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveRlRuntimeConfig {
    pub enabled: bool,
    pub store_path: PathBuf,
    pub update_interval_rollouts: usize,
    pub max_rollouts_per_update: usize,
    pub max_failure_streak: usize,
}

impl LiveRlRuntimeConfig {
    pub(crate) fn from_env_map(
        env: &BTreeMap<String, String>,
        default_store_path: &Path,
    ) -> Result<Self> {
        let enabled = match env.get(LIVE_RL_ENABLED_ENV) {
            Some(raw) => parse_bool_env(raw).ok_or_else(|| {
                anyhow!("{LIVE_RL_ENABLED_ENV} must be one of 1,true,yes,on,0,false,no,off")
            })?,
            None => false,
        };

        let store_path = env
            .get(LIVE_RL_STORE_PATH_ENV)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_store_path.to_path_buf());

        let update_interval_rollouts = parse_positive_usize_env(
            env.get(LIVE_RL_UPDATE_INTERVAL_ENV).map(String::as_str),
            8,
            LIVE_RL_UPDATE_INTERVAL_ENV,
        )?;
        let max_rollouts_per_update = parse_positive_usize_env(
            env.get(LIVE_RL_MAX_ROLLOUTS_ENV).map(String::as_str),
            64,
            LIVE_RL_MAX_ROLLOUTS_ENV,
        )?;
        let max_failure_streak = parse_positive_usize_env(
            env.get(LIVE_RL_MAX_FAILURE_STREAK_ENV).map(String::as_str),
            3,
            LIVE_RL_MAX_FAILURE_STREAK_ENV,
        )?;

        Ok(Self {
            enabled,
            store_path,
            update_interval_rollouts,
            max_rollouts_per_update,
            max_failure_streak,
        })
    }
}

#[derive(Clone)]
pub(crate) struct LiveRlRuntimeBridge {
    inner: Arc<LiveRlRuntimeBridgeInner>,
}

struct LiveRlRuntimeBridgeInner {
    store: Arc<dyn TrainingStore + Send + Sync>,
    config: LiveRlRuntimeConfig,
    state: Mutex<LiveRlRuntimeState>,
}

#[derive(Debug)]
struct LiveRlRuntimeState {
    gate: LiveRlRuntimeGate,
    next_rollout_sequence: u64,
    completed_rollouts: usize,
    consecutive_failures: usize,
    last_error: Option<String>,
    last_optimizer_report: Option<LiveRlOptimizerReport>,
    active_run: Option<LiveRlActiveRun>,
}

#[derive(Debug, Clone)]
struct LiveRlActiveRun {
    rollout_id: String,
    attempt_id: String,
    prompt: Option<String>,
    assistant_reply: Option<String>,
    turns: u32,
    tool_errors: u32,
    safety_blocked: bool,
}

impl LiveRlRuntimeBridge {
    pub(crate) fn register_if_enabled(
        agent: &mut Agent,
        default_store_path: &Path,
    ) -> Result<Option<LiveRlRuntimeSnapshot>> {
        let env = std::env::vars().collect::<BTreeMap<_, _>>();
        let config = LiveRlRuntimeConfig::from_env_map(&env, default_store_path)
            .context("failed to resolve live RL runtime config")?;
        if !config.enabled {
            return Ok(None);
        }

        let sqlite_store = Arc::new(
            SqliteTrainingStore::new(config.store_path.as_path()).with_context(|| {
                format!(
                    "failed to initialize live RL training store at {}",
                    config.store_path.display()
                )
            })?,
        );

        let bridge = Self::new(sqlite_store, config);
        bridge.register(agent);
        Ok(Some(bridge.snapshot_blocking()))
    }

    fn new(store: Arc<dyn TrainingStore + Send + Sync>, config: LiveRlRuntimeConfig) -> Self {
        Self {
            inner: Arc::new(LiveRlRuntimeBridgeInner {
                store,
                config,
                state: Mutex::new(LiveRlRuntimeState {
                    gate: LiveRlRuntimeGate::Pass,
                    next_rollout_sequence: 0,
                    completed_rollouts: 0,
                    consecutive_failures: 0,
                    last_error: None,
                    last_optimizer_report: None,
                    active_run: None,
                }),
            }),
        }
    }

    fn register(&self, agent: &mut Agent) {
        let bridge = self.clone();
        agent.subscribe_async(move |event| {
            let bridge = bridge.clone();
            async move {
                bridge.handle_event(event).await;
            }
        });
    }

    pub(crate) async fn handle_event(&self, event: AgentEvent) {
        if !self.inner.config.enabled {
            return;
        }

        match event {
            AgentEvent::AgentStart => {
                self.handle_agent_start().await;
            }
            AgentEvent::AgentEnd { .. } => {
                self.handle_agent_end().await;
            }
            AgentEvent::MessageAdded { message } => {
                self.handle_message_event(message.role, message.text_content())
                    .await;
            }
            AgentEvent::ToolExecutionEnd { result, .. } => {
                if result.is_error {
                    let mut state = self.inner.state.lock().await;
                    if let Some(run) = state.active_run.as_mut() {
                        run.tool_errors = run.tool_errors.saturating_add(1);
                    }
                }
            }
            AgentEvent::TurnEnd { .. } => {
                let mut state = self.inner.state.lock().await;
                if let Some(run) = state.active_run.as_mut() {
                    run.turns = run.turns.saturating_add(1);
                }
            }
            AgentEvent::SafetyPolicyApplied { blocked, .. } => {
                if blocked {
                    let mut state = self.inner.state.lock().await;
                    if let Some(run) = state.active_run.as_mut() {
                        run.safety_blocked = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn snapshot_blocking(&self) -> LiveRlRuntimeSnapshot {
        let state = self.inner.state.blocking_lock();
        LiveRlRuntimeSnapshot {
            enabled: self.inner.config.enabled,
            store_path: self.inner.config.store_path.clone(),
            gate: state.gate,
            completed_rollouts: state.completed_rollouts,
            consecutive_failures: state.consecutive_failures,
            last_error: state.last_error.clone(),
            last_optimizer_report: state.last_optimizer_report.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) async fn snapshot(&self) -> LiveRlRuntimeSnapshot {
        let state = self.inner.state.lock().await;
        LiveRlRuntimeSnapshot {
            enabled: self.inner.config.enabled,
            store_path: self.inner.config.store_path.clone(),
            gate: state.gate,
            completed_rollouts: state.completed_rollouts,
            consecutive_failures: state.consecutive_failures,
            last_error: state.last_error.clone(),
            last_optimizer_report: state.last_optimizer_report.clone(),
        }
    }

    async fn handle_agent_start(&self) {
        let stale_run = {
            let mut state = self.inner.state.lock().await;
            let stale = state.active_run.take();
            if state.gate == LiveRlRuntimeGate::Hold {
                return;
            }
            state.next_rollout_sequence = state.next_rollout_sequence.saturating_add(1);
            let rollout_id = format!("{LIVE_ROLLOUT_PREFIX}-{:010}", state.next_rollout_sequence);
            let attempt_id = format!("{rollout_id}:attempt-live");
            state.active_run = Some(LiveRlActiveRun {
                rollout_id: rollout_id.clone(),
                attempt_id,
                prompt: None,
                assistant_reply: None,
                turns: 0,
                tool_errors: 0,
                safety_blocked: false,
            });
            stale
        };

        if let Some(stale) = stale_run {
            self.finalize_run(stale, RolloutStatus::Cancelled).await;
        }

        let active_rollout_id = {
            let state = self.inner.state.lock().await;
            state
                .active_run
                .as_ref()
                .map(|run| run.rollout_id.clone())
                .unwrap_or_default()
        };

        if active_rollout_id.is_empty() {
            return;
        }

        if let Err(error) = self.create_rollout(active_rollout_id.as_str()).await {
            self.clear_active_run(active_rollout_id.as_str()).await;
            self.register_failure(format!(
                "live RL rollout init failed for {active_rollout_id}: {error}"
            ))
            .await;
        }
    }

    async fn handle_agent_end(&self) {
        let active = {
            let mut state = self.inner.state.lock().await;
            state.active_run.take()
        };
        let Some(active) = active else {
            return;
        };
        self.finalize_run(active, RolloutStatus::Succeeded).await;
    }

    async fn handle_message_event(&self, role: MessageRole, text: String) {
        let normalized = text.trim();
        if normalized.is_empty() {
            return;
        }
        let mut state = self.inner.state.lock().await;
        let Some(run) = state.active_run.as_mut() else {
            return;
        };
        match role {
            MessageRole::User => {
                if run.prompt.is_none() {
                    run.prompt = Some(normalized.to_string());
                }
            }
            MessageRole::Assistant => {
                run.assistant_reply = Some(normalized.to_string());
            }
            _ => {}
        }
    }

    async fn create_rollout(&self, rollout_id: &str) -> Result<()> {
        let mut rollout = Rollout::new(
            rollout_id.to_string(),
            json!({
                "source": "live_rl_runtime",
                "kind": "live_agent_decision",
            }),
            None,
        );
        rollout
            .metadata
            .insert("source".to_string(), json!("live_rl_runtime"));
        self.inner
            .store
            .enqueue_rollout(rollout)
            .await
            .with_context(|| format!("failed to enqueue live rollout '{rollout_id}'"))?;
        self.inner
            .store
            .update_rollout_status(rollout_id, RolloutStatus::Running)
            .await
            .with_context(|| format!("failed to mark live rollout '{rollout_id}' running"))?;
        Ok(())
    }

    async fn finalize_run(&self, run: LiveRlActiveRun, status: RolloutStatus) {
        if status == RolloutStatus::Succeeded {
            let span = build_final_decision_span(&run);
            if let Err(error) = self.inner.store.add_span(span).await {
                self.register_failure(format!(
                    "live RL span persistence failed for {}: {error}",
                    run.rollout_id
                ))
                .await;
                return;
            }
        }

        if let Err(error) = self
            .inner
            .store
            .update_rollout_status(run.rollout_id.as_str(), status)
            .await
        {
            self.register_failure(format!(
                "live RL rollout status update failed for {}: {error}",
                run.rollout_id
            ))
            .await;
            return;
        }

        if status == RolloutStatus::Succeeded {
            let should_run_update = {
                let mut state = self.inner.state.lock().await;
                state.completed_rollouts = state.completed_rollouts.saturating_add(1);
                state.consecutive_failures = 0;
                state.last_error = None;
                let due =
                    state.completed_rollouts % self.inner.config.update_interval_rollouts == 0;
                if !due {
                    state.last_optimizer_report = None;
                }
                due
            };

            if should_run_update {
                if let Err(error) = self.run_optimizer_update().await {
                    self.register_failure(format!("live RL optimizer update failed: {error}"))
                        .await;
                }
            }
        }
    }

    async fn clear_active_run(&self, rollout_id: &str) {
        let mut state = self.inner.state.lock().await;
        if state
            .active_run
            .as_ref()
            .is_some_and(|run| run.rollout_id == rollout_id)
        {
            state.active_run = None;
        }
    }

    async fn run_optimizer_update(&self) -> Result<()> {
        let rollout_ids = self.collect_live_rollout_ids_for_update().await?;
        if rollout_ids.is_empty() {
            self.set_optimizer_report(LiveRlOptimizerReport {
                executed: false,
                trajectories: 0,
                samples: 0,
                mean_total_loss: None,
                observed_approx_kl: None,
                early_stop_triggered: false,
            })
            .await;
            return Ok(());
        }

        let trajectory_batch =
            collect_trajectory_batch(self.inner.store.as_ref(), &rollout_ids, None)
                .await
                .context("failed to collect live trajectories")?;
        if trajectory_batch.trajectories.is_empty() {
            self.set_optimizer_report(LiveRlOptimizerReport {
                executed: false,
                trajectories: 0,
                samples: 0,
                mean_total_loss: None,
                observed_approx_kl: None,
                early_stop_triggered: false,
            })
            .await;
            return Ok(());
        }

        let mut samples = Vec::new();
        let gae_config = GaeConfig::default();
        let ppo_config = PpoConfig::default();

        for trajectory in &trajectory_batch.trajectories {
            if trajectory.steps.is_empty() {
                continue;
            }

            let rewards = trajectory
                .steps
                .iter()
                .map(|step| step.reward)
                .collect::<Vec<_>>();
            let values = trajectory
                .steps
                .iter()
                .map(|step| step.value_estimate.unwrap_or(0.0))
                .collect::<Vec<_>>();
            let dones = trajectory
                .steps
                .iter()
                .map(|step| step.done)
                .collect::<Vec<_>>();

            let gae_batch = compute_gae_batch_from_slices(
                &gae_config,
                format!("live-gae-{}", trajectory.trajectory_id),
                trajectory.trajectory_id.clone(),
                &rewards,
                &values,
                &dones,
                0.0,
            )
            .with_context(|| {
                format!(
                    "failed to compute GAE batch for live trajectory '{}'",
                    trajectory.trajectory_id
                )
            })?;

            for (index, step) in trajectory.steps.iter().enumerate() {
                let logprob = step.logprob.unwrap_or(0.0);
                samples.push(PpoSample {
                    old_logprob: logprob,
                    new_logprob: logprob,
                    advantage: gae_batch.advantages[index],
                    return_value: gae_batch.returns[index],
                    value_prediction: values[index],
                    entropy: step
                        .metadata
                        .get("entropy")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                });
            }
        }

        if samples.is_empty() {
            self.set_optimizer_report(LiveRlOptimizerReport {
                executed: false,
                trajectories: trajectory_batch.trajectories.len(),
                samples: 0,
                mean_total_loss: None,
                observed_approx_kl: None,
                early_stop_triggered: false,
            })
            .await;
            return Ok(());
        }

        let update = compute_ppo_update(&ppo_config, &samples)
            .context("failed PPO update for live RL runtime")?;
        self.set_optimizer_report(LiveRlOptimizerReport {
            executed: true,
            trajectories: trajectory_batch.trajectories.len(),
            samples: samples.len(),
            mean_total_loss: Some(update.mean_loss.total_loss),
            observed_approx_kl: Some(update.observed_approx_kl),
            early_stop_triggered: update.early_stop_triggered,
        })
        .await;

        Ok(())
    }

    async fn collect_live_rollout_ids_for_update(&self) -> Result<Vec<String>> {
        let rollouts = self
            .inner
            .store
            .query_rollouts(RolloutQuery {
                statuses: Some(vec![RolloutStatus::Succeeded]),
                ..RolloutQuery::default()
            })
            .await
            .context("failed to query succeeded live rollouts")?;

        let mut rollout_ids = rollouts
            .into_iter()
            .filter(|rollout| rollout.rollout_id.starts_with(LIVE_ROLLOUT_PREFIX))
            .map(|rollout| rollout.rollout_id)
            .collect::<Vec<_>>();

        rollout_ids.sort();
        if rollout_ids.len() > self.inner.config.max_rollouts_per_update {
            let start = rollout_ids.len() - self.inner.config.max_rollouts_per_update;
            rollout_ids = rollout_ids[start..].to_vec();
        }

        Ok(rollout_ids)
    }

    async fn set_optimizer_report(&self, report: LiveRlOptimizerReport) {
        let mut state = self.inner.state.lock().await;
        state.last_optimizer_report = Some(report);
    }

    async fn register_failure(&self, message: String) {
        let mut state = self.inner.state.lock().await;
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_error = Some(message);
        if state.consecutive_failures >= self.inner.config.max_failure_streak {
            state.gate = LiveRlRuntimeGate::Hold;
        }
    }

    #[cfg(test)]
    pub(crate) fn for_tests(
        store: Arc<dyn TrainingStore + Send + Sync>,
        config: LiveRlRuntimeConfig,
    ) -> Self {
        Self::new(store, config)
    }

    #[cfg(test)]
    pub(crate) async fn record_failure_for_tests(&self, message: &str) {
        self.register_failure(message.to_string()).await;
    }
}

fn build_final_decision_span(run: &LiveRlActiveRun) -> TrainingSpan {
    let reward = compute_live_reward_breakdown(run);
    let mut span = TrainingSpan::new(
        run.rollout_id.as_str(),
        run.attempt_id.as_str(),
        1,
        format!("trace:{}", run.rollout_id),
        format!("span:{}:1", run.rollout_id),
        None,
        "live.agent.decision",
    );
    span.attributes.insert(
        "prompt".to_string(),
        json!(run.prompt.clone().unwrap_or_default()),
    );
    span.attributes.insert(
        "assistant_text".to_string(),
        json!(run.assistant_reply.clone().unwrap_or_default()),
    );
    span.attributes
        .insert("reward".to_string(), json!(reward.composite));
    span.attributes
        .insert("reward_completion".to_string(), json!(reward.completion));
    span.attributes
        .insert("reward_reliability".to_string(), json!(reward.reliability));
    span.attributes
        .insert("reward_safety".to_string(), json!(reward.safety));
    span.attributes
        .insert("reward_efficiency".to_string(), json!(reward.efficiency));
    span.attributes
        .insert("turns".to_string(), json!(run.turns));
    span.attributes
        .insert("tool_errors".to_string(), json!(run.tool_errors));
    span.attributes
        .insert("safety_blocked".to_string(), json!(run.safety_blocked));
    span.attributes.insert("done".to_string(), json!(true));
    span.end_time = Some(Utc::now());
    span
}

#[cfg(test)]
fn compute_live_reward(run: &LiveRlActiveRun) -> f64 {
    compute_live_reward_breakdown(run).composite
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct LiveRewardBreakdown {
    composite: f64,
    completion: f64,
    reliability: f64,
    safety: f64,
    efficiency: f64,
}

fn compute_live_reward_breakdown(run: &LiveRlActiveRun) -> LiveRewardBreakdown {
    let completion = if run
        .assistant_reply
        .as_ref()
        .is_some_and(|reply| !reply.trim().is_empty())
    {
        0.5
    } else {
        0.0
    };
    let reliability = -0.25 * f64::from(run.tool_errors.min(2));
    let efficiency = if run.turns <= 2 {
        0.5
    } else if run.turns <= 4 {
        0.25
    } else {
        0.0
    };
    let safety = if run.safety_blocked { -1.0 } else { 0.0 };

    if run.safety_blocked {
        return LiveRewardBreakdown {
            composite: -1.0,
            completion,
            reliability,
            safety,
            efficiency,
        };
    }

    let composite = (completion + reliability + efficiency).clamp(-1.0, 1.0);
    LiveRewardBreakdown {
        composite,
        completion,
        reliability,
        safety,
        efficiency,
    }
}

fn parse_bool_env(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_positive_usize_env(raw: Option<&str>, default: usize, key: &str) -> Result<usize> {
    let Some(raw) = raw else {
        return Ok(default);
    };
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Ok(default);
    }
    let value = normalized
        .parse::<usize>()
        .with_context(|| format!("{key} must be a positive integer"))?;
    if value == 0 {
        return Err(anyhow!("{key} must be greater than 0"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        compute_live_reward, LiveRlActiveRun, LiveRlRuntimeBridge, LiveRlRuntimeConfig,
        LiveRlRuntimeGate,
    };
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use tau_agent_core::AgentEvent;
    use tau_ai::Message;
    use tau_training_store::{InMemoryTrainingStore, RolloutQuery, RolloutStatus, TrainingStore};

    #[test]
    fn spec_c04_unit_live_rl_env_defaults_to_disabled() {
        let env = BTreeMap::new();
        let config = LiveRlRuntimeConfig::from_env_map(
            &env,
            std::path::Path::new(".tau/training/store.sqlite"),
        )
        .expect("config from env");
        assert!(!config.enabled);
        assert_eq!(config.update_interval_rollouts, 8);
        assert_eq!(config.max_rollouts_per_update, 64);
        assert_eq!(config.max_failure_streak, 3);
    }

    #[tokio::test]
    async fn spec_c01_functional_live_events_persist_rollout_and_span() {
        let store: Arc<dyn TrainingStore + Send + Sync> = Arc::new(InMemoryTrainingStore::new());
        let bridge = LiveRlRuntimeBridge::for_tests(
            store.clone(),
            LiveRlRuntimeConfig {
                enabled: true,
                store_path: ".tau/training/store.sqlite".into(),
                update_interval_rollouts: 8,
                max_rollouts_per_update: 32,
                max_failure_streak: 3,
            },
        );

        bridge.handle_event(AgentEvent::AgentStart).await;
        bridge
            .handle_event(AgentEvent::MessageAdded {
                message: Message::user("summarize latest deploy status"),
            })
            .await;
        bridge
            .handle_event(AgentEvent::MessageAdded {
                message: Message::assistant_text("Deploy completed with no failures."),
            })
            .await;
        bridge
            .handle_event(AgentEvent::AgentEnd { new_messages: 2 })
            .await;

        let rollouts = store
            .query_rollouts(RolloutQuery {
                statuses: Some(vec![RolloutStatus::Succeeded]),
                ..RolloutQuery::default()
            })
            .await
            .expect("query succeeded rollouts");
        assert_eq!(rollouts.len(), 1);
        assert!(rollouts[0].rollout_id.starts_with("live-rl-rollout"));

        let spans = store
            .query_spans(rollouts[0].rollout_id.as_str(), None)
            .await
            .expect("query spans");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "live.agent.decision");
        assert_eq!(spans[0].attributes["reward"], serde_json::json!(1.0));
    }

    #[tokio::test]
    async fn spec_c02_functional_optimizer_runs_on_update_interval() {
        let store: Arc<dyn TrainingStore + Send + Sync> = Arc::new(InMemoryTrainingStore::new());
        let bridge = LiveRlRuntimeBridge::for_tests(
            store,
            LiveRlRuntimeConfig {
                enabled: true,
                store_path: ".tau/training/store.sqlite".into(),
                update_interval_rollouts: 1,
                max_rollouts_per_update: 32,
                max_failure_streak: 3,
            },
        );

        bridge.handle_event(AgentEvent::AgentStart).await;
        bridge
            .handle_event(AgentEvent::MessageAdded {
                message: Message::user("draft release notes"),
            })
            .await;
        bridge
            .handle_event(AgentEvent::MessageAdded {
                message: Message::assistant_text("Release notes drafted."),
            })
            .await;
        bridge
            .handle_event(AgentEvent::AgentEnd { new_messages: 2 })
            .await;

        let snapshot = bridge.snapshot().await;
        let report = snapshot
            .last_optimizer_report
            .expect("optimizer report should be present");
        assert!(report.executed);
        assert!(report.samples > 0);
        assert_eq!(snapshot.completed_rollouts, 1);
    }

    #[tokio::test]
    async fn spec_c03_regression_failure_streak_holds_live_gate() {
        let store: Arc<dyn TrainingStore + Send + Sync> = Arc::new(InMemoryTrainingStore::new());
        let bridge = LiveRlRuntimeBridge::for_tests(
            store,
            LiveRlRuntimeConfig {
                enabled: true,
                store_path: ".tau/training/store.sqlite".into(),
                update_interval_rollouts: 4,
                max_rollouts_per_update: 32,
                max_failure_streak: 1,
            },
        );

        bridge.record_failure_for_tests("forced failure").await;
        let snapshot = bridge.snapshot().await;
        assert_eq!(snapshot.gate, LiveRlRuntimeGate::Hold);
        assert_eq!(snapshot.consecutive_failures, 1);
        assert_eq!(snapshot.last_error.as_deref(), Some("forced failure"));
    }

    #[test]
    fn spec_c05_unit_live_reward_breakdown_scores_deterministically() {
        let run = LiveRlActiveRun {
            rollout_id: "live-rl-rollout-1".to_string(),
            attempt_id: "live-rl-rollout-1:attempt-1".to_string(),
            prompt: Some("summarize release risks".to_string()),
            assistant_reply: Some("Release risks summarized.".to_string()),
            turns: 1,
            tool_errors: 0,
            safety_blocked: false,
        };
        assert_eq!(compute_live_reward(&run), 1.0);

        let noisy = LiveRlActiveRun {
            tool_errors: 2,
            ..run.clone()
        };
        assert_eq!(compute_live_reward(&noisy), 0.5);

        let no_reply = LiveRlActiveRun {
            assistant_reply: None,
            turns: 4,
            ..run.clone()
        };
        assert_eq!(compute_live_reward(&no_reply), 0.25);

        let blocked = LiveRlActiveRun {
            safety_blocked: true,
            ..run
        };
        assert_eq!(compute_live_reward(&blocked), -1.0);
    }

    #[tokio::test]
    async fn spec_c06_functional_live_rollout_span_persists_reward_breakdown() {
        let store: Arc<dyn TrainingStore + Send + Sync> = Arc::new(InMemoryTrainingStore::new());
        let bridge = LiveRlRuntimeBridge::for_tests(
            store.clone(),
            LiveRlRuntimeConfig {
                enabled: true,
                store_path: ".tau/training/store.sqlite".into(),
                update_interval_rollouts: 8,
                max_rollouts_per_update: 32,
                max_failure_streak: 3,
            },
        );

        bridge.handle_event(AgentEvent::AgentStart).await;
        bridge
            .handle_event(AgentEvent::MessageAdded {
                message: Message::user("summarize latest deploy status"),
            })
            .await;
        bridge
            .handle_event(AgentEvent::MessageAdded {
                message: Message::assistant_text("Deploy completed with no failures."),
            })
            .await;
        bridge
            .handle_event(AgentEvent::AgentEnd { new_messages: 2 })
            .await;

        let rollouts = store
            .query_rollouts(RolloutQuery {
                statuses: Some(vec![RolloutStatus::Succeeded]),
                ..RolloutQuery::default()
            })
            .await
            .expect("query succeeded rollouts");
        assert_eq!(rollouts.len(), 1);

        let spans = store
            .query_spans(rollouts[0].rollout_id.as_str(), None)
            .await
            .expect("query spans");
        assert_eq!(spans.len(), 1);
        let attrs = &spans[0].attributes;
        assert!(attrs.contains_key("reward_completion"));
        assert!(attrs.contains_key("reward_reliability"));
        assert!(attrs.contains_key("reward_safety"));
        assert!(attrs.contains_key("reward_efficiency"));
    }
}
