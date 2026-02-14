//! Span collection and reward emission for training rollouts.

use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tau_agent_core::AgentEvent;
use tau_training_store::{StoreResult, TrainingStore};
use tau_training_types::{Reward, TrainingSpan};

/// Thread-safe tracer used during rollout execution.
#[derive(Debug, Clone)]
pub struct TrainingTracer {
    rollout_id: String,
    attempt_id: String,
    trace_id: String,
    inner: Arc<Mutex<TrainingTracerInner>>,
}

#[derive(Debug, Default)]
struct TrainingTracerInner {
    open_spans: HashMap<String, OpenSpan>,
    managed_keys: HashMap<String, String>,
    completed_spans: Vec<TrainingSpan>,
    next_sequence_id: u64,
}

#[derive(Debug)]
struct OpenSpan {
    sequence_id: u64,
    span_id: String,
    parent_id: Option<String>,
    name: String,
    attributes: HashMap<String, Value>,
    start_time: chrono::DateTime<Utc>,
}

/// RAII guard that closes a span when dropped.
#[derive(Debug)]
pub struct SpanGuard {
    tracer: TrainingTracer,
    span_id: Option<String>,
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if let Some(span_id) = self.span_id.take() {
            self.tracer.end_span(&span_id, HashMap::new());
        }
    }
}

impl TrainingTracer {
    /// Creates a tracer for a rollout attempt.
    pub fn new(rollout_id: impl Into<String>, attempt_id: impl Into<String>) -> Self {
        Self {
            rollout_id: rollout_id.into(),
            attempt_id: attempt_id.into(),
            trace_id: next_id("trace"),
            inner: Arc::new(Mutex::new(TrainingTracerInner::default())),
        }
    }

    /// Starts an operation span that closes on guard drop.
    pub fn operation(&self, name: impl Into<String>) -> SpanGuard {
        let span_id = self.start_span(name.into(), None, HashMap::new());
        SpanGuard {
            tracer: self.clone(),
            span_id: Some(span_id),
        }
    }

    /// Emits a scalar reward span.
    pub fn emit_reward(&self, reward: Reward) {
        let reward_name = reward.name;
        let reward_value = reward.value;
        let mut attrs = HashMap::new();
        attrs.insert("reward_name".to_string(), json!(reward_name));
        attrs.insert("reward_value".to_string(), json!(reward_value));
        attrs.insert(
            "rewards".to_string(),
            json!([{
                "name": reward_name,
                "value": reward_value,
            }]),
        );
        self.instant_span("reward.emit".to_string(), attrs);
    }

    /// Captures an agent runtime event as one or more spans.
    pub fn on_agent_event(&self, event: &AgentEvent) {
        match event {
            AgentEvent::AgentStart => {
                self.instant_span("agent.start".to_string(), HashMap::new());
            }
            AgentEvent::AgentEnd { new_messages } => {
                self.instant_span(
                    "agent.end".to_string(),
                    HashMap::from([("new_messages".to_string(), json!(new_messages))]),
                );
            }
            AgentEvent::TurnStart { turn } => {
                self.start_managed_span(
                    format!("turn:{turn}"),
                    format!("agent.turn.{turn}"),
                    HashMap::from([("turn".to_string(), json!(turn))]),
                );
            }
            AgentEvent::TurnEnd {
                turn,
                tool_results,
                request_duration_ms,
                usage,
                finish_reason,
            } => {
                self.end_managed_span(
                    &format!("turn:{turn}"),
                    HashMap::from([
                        ("tool_results".to_string(), json!(tool_results)),
                        (
                            "request_duration_ms".to_string(),
                            json!(request_duration_ms),
                        ),
                        (
                            "usage".to_string(),
                            json!({
                                "input_tokens": usage.input_tokens,
                                "output_tokens": usage.output_tokens,
                                "total_tokens": usage.total_tokens,
                            }),
                        ),
                        ("finish_reason".to_string(), json!(finish_reason)),
                    ]),
                );
            }
            AgentEvent::MessageAdded { message } => {
                self.instant_span(
                    "message.added".to_string(),
                    HashMap::from([
                        (
                            "role".to_string(),
                            json!(format!("{:?}", message.role).to_lowercase()),
                        ),
                        ("text".to_string(), json!(message.text_content())),
                    ]),
                );
            }
            AgentEvent::ToolExecutionStart {
                tool_call_id,
                tool_name,
                arguments,
            } => {
                self.start_managed_span(
                    format!("tool:{tool_call_id}"),
                    format!("tool.{tool_name}"),
                    HashMap::from([
                        ("tool_call_id".to_string(), json!(tool_call_id)),
                        ("tool_name".to_string(), json!(tool_name)),
                        ("arguments".to_string(), arguments.clone()),
                    ]),
                );
            }
            AgentEvent::ToolExecutionEnd {
                tool_call_id,
                tool_name,
                result,
            } => {
                self.end_managed_span(
                    &format!("tool:{tool_call_id}"),
                    HashMap::from([
                        ("tool_call_id".to_string(), json!(tool_call_id)),
                        ("tool_name".to_string(), json!(tool_name)),
                        ("is_error".to_string(), json!(result.is_error)),
                        ("content".to_string(), result.content.clone()),
                    ]),
                );
            }
            AgentEvent::ReplanTriggered { turn, reason } => {
                self.instant_span(
                    "agent.replan_triggered".to_string(),
                    HashMap::from([
                        ("turn".to_string(), json!(turn)),
                        ("reason".to_string(), json!(reason)),
                    ]),
                );
            }
            AgentEvent::CostUpdated {
                turn,
                turn_cost_usd,
                cumulative_cost_usd,
                budget_usd,
            } => {
                self.instant_span(
                    "agent.cost_updated".to_string(),
                    HashMap::from([
                        ("turn".to_string(), json!(turn)),
                        ("turn_cost_usd".to_string(), json!(turn_cost_usd)),
                        (
                            "cumulative_cost_usd".to_string(),
                            json!(cumulative_cost_usd),
                        ),
                        ("budget_usd".to_string(), json!(budget_usd)),
                    ]),
                );
            }
            AgentEvent::CostBudgetAlert {
                turn,
                threshold_percent,
                cumulative_cost_usd,
                budget_usd,
            } => {
                self.instant_span(
                    "agent.cost_budget_alert".to_string(),
                    HashMap::from([
                        ("turn".to_string(), json!(turn)),
                        ("threshold_percent".to_string(), json!(threshold_percent)),
                        (
                            "cumulative_cost_usd".to_string(),
                            json!(cumulative_cost_usd),
                        ),
                        ("budget_usd".to_string(), json!(budget_usd)),
                    ]),
                );
            }
            AgentEvent::SafetyPolicyApplied {
                stage,
                mode,
                blocked,
                matched_rules,
                reason_codes,
            } => {
                self.instant_span(
                    "agent.safety_policy_applied".to_string(),
                    HashMap::from([
                        ("stage".to_string(), json!(stage.as_str())),
                        ("mode".to_string(), json!(mode)),
                        ("blocked".to_string(), json!(blocked)),
                        ("matched_rules".to_string(), json!(matched_rules)),
                        ("reason_codes".to_string(), json!(reason_codes)),
                    ]),
                );
            }
        }
    }

    /// Flushes completed spans to the backing store.
    pub async fn flush(&self, store: &(dyn TrainingStore + Send + Sync)) -> StoreResult<usize> {
        let spans = {
            let mut inner = self.inner.lock().expect("training tracer mutex poisoned");
            let now = Utc::now();

            let open_ids: Vec<String> = inner.open_spans.keys().cloned().collect();
            for span_id in open_ids {
                if let Some(open) = inner.open_spans.remove(&span_id) {
                    let span = TrainingSpan {
                        rollout_id: self.rollout_id.clone(),
                        attempt_id: self.attempt_id.clone(),
                        sequence_id: open.sequence_id,
                        trace_id: self.trace_id.clone(),
                        span_id: open.span_id,
                        parent_id: open.parent_id,
                        name: open.name,
                        attributes: open.attributes,
                        events: Vec::new(),
                        start_time: open.start_time,
                        end_time: Some(now),
                    };
                    inner.completed_spans.push(span);
                }
            }
            inner.managed_keys.clear();

            let mut spans = Vec::new();
            std::mem::swap(&mut spans, &mut inner.completed_spans);
            spans.sort_by_key(|span| span.sequence_id);
            spans
        };

        if spans.is_empty() {
            return Ok(0);
        }

        let count = spans.len();
        store.add_spans(spans).await?;
        Ok(count)
    }

    /// Returns an in-memory snapshot of completed spans.
    pub fn completed_spans(&self) -> Vec<TrainingSpan> {
        let inner = self.inner.lock().expect("training tracer mutex poisoned");
        let mut spans = inner.completed_spans.clone();
        spans.sort_by_key(|span| span.sequence_id);
        spans
    }

    fn start_managed_span(&self, key: String, name: String, attributes: HashMap<String, Value>) {
        let span_id = self.start_span(name, None, attributes);
        let mut inner = self.inner.lock().expect("training tracer mutex poisoned");
        inner.managed_keys.insert(key, span_id);
    }

    fn end_managed_span(&self, key: &str, extra_attributes: HashMap<String, Value>) {
        let span_id = {
            let mut inner = self.inner.lock().expect("training tracer mutex poisoned");
            inner.managed_keys.remove(key)
        };
        if let Some(span_id) = span_id {
            self.end_span(&span_id, extra_attributes);
        } else {
            self.instant_span(format!("{key}.end_without_start"), extra_attributes);
        }
    }

    fn start_span(
        &self,
        name: String,
        parent_id: Option<String>,
        attributes: HashMap<String, Value>,
    ) -> String {
        let mut inner = self.inner.lock().expect("training tracer mutex poisoned");
        inner.next_sequence_id += 1;
        let sequence_id = inner.next_sequence_id;
        let span_id = next_id("span");
        inner.open_spans.insert(
            span_id.clone(),
            OpenSpan {
                sequence_id,
                span_id: span_id.clone(),
                parent_id,
                name,
                attributes,
                start_time: Utc::now(),
            },
        );
        span_id
    }

    fn end_span(&self, span_id: &str, extra_attributes: HashMap<String, Value>) {
        let mut inner = self.inner.lock().expect("training tracer mutex poisoned");
        let Some(mut open) = inner.open_spans.remove(span_id) else {
            return;
        };

        for (key, value) in extra_attributes {
            open.attributes.insert(key, value);
        }

        let span = TrainingSpan {
            rollout_id: self.rollout_id.clone(),
            attempt_id: self.attempt_id.clone(),
            sequence_id: open.sequence_id,
            trace_id: self.trace_id.clone(),
            span_id: open.span_id,
            parent_id: open.parent_id,
            name: open.name,
            attributes: open.attributes,
            events: Vec::new(),
            start_time: open.start_time,
            end_time: Some(Utc::now()),
        };
        inner.completed_spans.push(span);
    }

    fn instant_span(&self, name: String, attributes: HashMap<String, Value>) {
        let mut inner = self.inner.lock().expect("training tracer mutex poisoned");
        inner.next_sequence_id += 1;
        let sequence_id = inner.next_sequence_id;
        let now = Utc::now();

        inner.completed_spans.push(TrainingSpan {
            rollout_id: self.rollout_id.clone(),
            attempt_id: self.attempt_id.clone(),
            sequence_id,
            trace_id: self.trace_id.clone(),
            span_id: next_id("span"),
            parent_id: None,
            name,
            attributes,
            events: Vec::new(),
            start_time: now,
            end_time: Some(now),
        });
    }
}

fn next_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let value = COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_ns = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    format!("{prefix}-{now_ns}-{value}")
}

#[cfg(test)]
mod tests {
    use super::TrainingTracer;
    use serde_json::json;
    use tau_agent_core::{AgentEvent, SafetyMode, SafetyStage};
    use tau_ai::ChatUsage;
    use tau_training_store::{InMemoryTrainingStore, TrainingStore};
    use tau_training_types::Reward;

    #[test]
    fn operation_guard_closes_span() {
        let tracer = TrainingTracer::new("r-1", "a-1");
        {
            let _guard = tracer.operation("step");
        }

        let spans = tracer.completed_spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "step");
        assert!(spans[0].end_time.is_some());
    }

    #[test]
    fn reward_emits_span() {
        let tracer = TrainingTracer::new("r-1", "a-1");
        tracer.emit_reward(Reward::new("exact_match", 1.0));

        let spans = tracer.completed_spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "reward.emit");
        assert_eq!(spans[0].attributes.get("reward_value"), Some(&json!(1.0)));
    }

    #[tokio::test]
    async fn maps_agent_events_and_flushes_to_store() {
        let tracer = TrainingTracer::new("r-1", "a-1");
        tracer.on_agent_event(&AgentEvent::TurnStart { turn: 1 });
        tracer.on_agent_event(&AgentEvent::TurnEnd {
            turn: 1,
            tool_results: 0,
            request_duration_ms: 15,
            usage: ChatUsage::default(),
            finish_reason: Some("stop".to_string()),
        });

        let store = InMemoryTrainingStore::new();
        let count = tracer.flush(&store).await.expect("flush");
        assert_eq!(count, 1);

        let spans = store.query_spans("r-1", Some("a-1")).await.expect("query");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "agent.turn.1");
    }

    #[test]
    fn maps_safety_policy_events_into_spans() {
        let tracer = TrainingTracer::new("r-1", "a-1");
        tracer.on_agent_event(&AgentEvent::SafetyPolicyApplied {
            stage: SafetyStage::InboundMessage,
            mode: SafetyMode::Redact,
            blocked: false,
            matched_rules: vec!["literal.ignore_previous_instructions".to_string()],
            reason_codes: vec!["prompt_injection.ignore_instructions".to_string()],
        });
        let spans = tracer.completed_spans();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "agent.safety_policy_applied");
        assert_eq!(
            spans[0].attributes.get("stage"),
            Some(&json!("inbound_message"))
        );
        assert_eq!(spans[0].attributes.get("mode"), Some(&json!("redact")));
    }
}
