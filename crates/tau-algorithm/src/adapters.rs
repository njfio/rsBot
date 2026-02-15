use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tau_ai::{Message, MessageRole};
use tau_training_types::{EpisodeTrajectory, TrainingSpan, TrajectoryStep, Triplet};

/// Adapter trait converting raw spans into algorithm-specific structures.
pub trait TraceAdapter<T>: Send + Sync {
    fn adapt(&self, spans: &[TrainingSpan]) -> Result<T>;
}

/// Converts spans into (prompt, response, reward) triplets.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpansToTriplets;

impl TraceAdapter<Vec<Triplet>> for SpansToTriplets {
    fn adapt(&self, spans: &[TrainingSpan]) -> Result<Vec<Triplet>> {
        let mut ordered: Vec<&TrainingSpan> = spans.iter().collect();
        ordered.sort_by_key(|span| span.sequence_id);

        let reward_candidates: Vec<(u64, f64)> = ordered
            .iter()
            .filter_map(|span| {
                extract_reward_value(span.attributes.get("reward_value"))
                    .or_else(|| extract_reward_value(span.attributes.get("reward")))
                    .map(|reward| (span.sequence_id, reward))
            })
            .collect();

        let mut triplets = Vec::new();
        for span in ordered {
            let Some(prompt) = span
                .attributes
                .get("prompt")
                .or_else(|| span.attributes.get("input"))
                .cloned()
            else {
                continue;
            };

            let Some(response) = span
                .attributes
                .get("response")
                .or_else(|| span.attributes.get("assistant_text"))
                .or_else(|| span.attributes.get("output"))
                .cloned()
            else {
                continue;
            };

            let reward = extract_reward_value(
                span.attributes
                    .get("reward")
                    .or_else(|| span.attributes.get("reward_value")),
            )
            .or_else(|| {
                reward_candidates
                    .iter()
                    .find(|(sequence, _)| *sequence >= span.sequence_id)
                    .map(|(_, value)| *value)
            });

            triplets.push(Triplet {
                prompt,
                response,
                reward,
            });
        }

        Ok(triplets)
    }
}

/// Converts message-oriented spans into chat messages.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpansToMessages;

impl TraceAdapter<Vec<Message>> for SpansToMessages {
    fn adapt(&self, spans: &[TrainingSpan]) -> Result<Vec<Message>> {
        let mut ordered: Vec<&TrainingSpan> = spans.iter().collect();
        ordered.sort_by_key(|span| span.sequence_id);

        let messages = ordered
            .into_iter()
            .filter(|span| span.name == "message.added")
            .map(|span| {
                let role = span
                    .attributes
                    .get("role")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("assistant");
                let text = span
                    .attributes
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();

                match parse_role(role) {
                    MessageRole::System => Message::system(text),
                    MessageRole::User => Message::user(text),
                    MessageRole::Assistant => Message::assistant_text(text),
                    MessageRole::Tool => Message::tool_result(
                        span.attributes
                            .get("tool_call_id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("tool-call"),
                        span.attributes
                            .get("tool_name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("tool"),
                        text,
                        span.attributes
                            .get("is_error")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false),
                    ),
                }
            })
            .collect();

        Ok(messages)
    }
}

/// Converts spans into RL episode trajectories.
///
/// By default, trajectories retain the natural step count from input spans.
#[derive(Debug, Default, Clone, Copy)]
pub struct SpansToTrajectories {
    window_policy: Option<TrajectoryWindowPolicy>,
}

/// Optional sequence-window policy applied during span-to-trajectory adaptation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrajectoryWindowPolicy {
    /// Target number of steps kept/emitted per trajectory.
    pub window_size: usize,
    /// Padding strategy when observed steps are fewer than `window_size`.
    pub padding_mode: TrajectoryPaddingMode,
}

/// Padding mode for trajectory window adaptation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrajectoryPaddingMode {
    /// Keep shorter trajectories as-is after truncation rules are applied.
    Disabled,
    /// Right-pad deterministic synthetic steps until `window_size` is reached.
    PadRight,
}

impl TrajectoryWindowPolicy {
    fn validate(self) -> Result<()> {
        if self.window_size == 0 {
            bail!("trajectory window policy window_size must be greater than 0");
        }
        Ok(())
    }
}

impl SpansToTrajectories {
    /// Creates an adapter with an explicit windowing policy.
    pub fn with_window_policy(window_policy: TrajectoryWindowPolicy) -> Self {
        Self {
            window_policy: Some(window_policy),
        }
    }
}

impl TraceAdapter<Vec<EpisodeTrajectory>> for SpansToTrajectories {
    fn adapt(&self, spans: &[TrainingSpan]) -> Result<Vec<EpisodeTrajectory>> {
        if spans.is_empty() {
            bail!("trajectory adapter requires at least one span");
        }
        if let Some(policy) = self.window_policy {
            policy.validate()?;
        }

        let mut grouped: BTreeMap<(String, String), Vec<&TrainingSpan>> = BTreeMap::new();
        for span in spans {
            grouped
                .entry((span.rollout_id.clone(), span.attempt_id.clone()))
                .or_default()
                .push(span);
        }

        let mut trajectories = Vec::with_capacity(grouped.len());
        for ((rollout_id, attempt_id), mut grouped_spans) in grouped {
            grouped_spans.sort_by_key(|span| span.sequence_id);

            let mut steps = Vec::with_capacity(grouped_spans.len());
            let last_index = grouped_spans.len().saturating_sub(1);
            for (position, span) in grouped_spans.iter().enumerate() {
                let observation = extract_observation(span);
                let action = extract_action(span);
                let reward = extract_reward(span);
                let done = span
                    .attributes
                    .get("done")
                    .and_then(Value::as_bool)
                    .unwrap_or(position == last_index);

                let mut step =
                    TrajectoryStep::new(position as u32, observation, action, reward, done);
                step.logprob = extract_reward_value(
                    span.attributes
                        .get("logprob")
                        .or_else(|| span.attributes.get("action_logprob")),
                );
                step.value_estimate = extract_reward_value(
                    span.attributes
                        .get("value_estimate")
                        .or_else(|| span.attributes.get("value")),
                );
                step.metadata
                    .insert("span_name".to_string(), json!(span.name));
                step.metadata
                    .insert("span_id".to_string(), json!(span.span_id));
                step.metadata
                    .insert("trace_id".to_string(), json!(span.trace_id));
                step.metadata
                    .insert("sequence_id".to_string(), json!(span.sequence_id));
                step.metadata
                    .insert("attempt_id".to_string(), json!(attempt_id));
                steps.push(step);
            }
            if let Some(policy) = self.window_policy {
                apply_window_policy(&mut steps, policy);
            }

            let trajectory_id = format!("{rollout_id}::{attempt_id}");
            let mut trajectory = EpisodeTrajectory::new(
                trajectory_id,
                Some(rollout_id.clone()),
                Some(attempt_id.clone()),
                steps,
            );
            trajectory.metadata.insert(
                "adapter".to_string(),
                json!("tau-algorithm::SpansToTrajectories"),
            );
            trajectory
                .validate()
                .map_err(|error| anyhow!(
                    "trajectory adapter validation failed for rollout={rollout_id} attempt={attempt_id}: {error}"
                ))?;
            trajectories.push(trajectory);
        }

        Ok(trajectories)
    }
}

fn extract_reward_value(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(serde_json::Value::as_f64)
}

fn extract_value_by_keys(
    attributes: &std::collections::HashMap<String, Value>,
    keys: &[&str],
) -> Option<Value> {
    keys.iter().find_map(|key| attributes.get(*key)).cloned()
}

fn extract_observation(span: &TrainingSpan) -> Value {
    extract_value_by_keys(
        &span.attributes,
        &[
            "observation",
            "state",
            "prompt",
            "input",
            "tool_result",
            "tool_output",
        ],
    )
    .unwrap_or_else(|| {
        json!({
            "fallback": "observation",
            "span_name": span.name,
            "sequence_id": span.sequence_id,
        })
    })
}

fn extract_action(span: &TrainingSpan) -> Value {
    extract_value_by_keys(
        &span.attributes,
        &[
            "action",
            "tool_call",
            "response",
            "assistant_text",
            "output",
            "text",
            "tool_name",
        ],
    )
    .unwrap_or_else(|| {
        json!({
            "fallback": "action",
            "span_name": span.name,
            "sequence_id": span.sequence_id,
        })
    })
}

fn apply_window_policy(steps: &mut Vec<TrajectoryStep>, policy: TrajectoryWindowPolicy) {
    if steps.len() > policy.window_size {
        let to_drop = steps.len() - policy.window_size;
        steps.drain(0..to_drop);
    }

    if steps.len() < policy.window_size
        && matches!(policy.padding_mode, TrajectoryPaddingMode::PadRight)
    {
        let missing = policy.window_size - steps.len();
        for padding_index in 0..missing {
            let mut padded = TrajectoryStep::new(
                0,
                json!({"padding": "observation"}),
                json!({"padding": "action"}),
                0.0,
                false,
            );
            padded.metadata.insert("padded".to_string(), json!(true));
            padded
                .metadata
                .insert("padding_mode".to_string(), json!("right"));
            padded
                .metadata
                .insert("padding_index".to_string(), json!(padding_index));
            steps.push(padded);
        }
    }

    let last_index = steps.len().saturating_sub(1);
    for (index, step) in steps.iter_mut().enumerate() {
        step.step_index = index as u32;
        step.done = index == last_index;
    }
}

fn extract_reward(span: &TrainingSpan) -> f64 {
    extract_reward_value(
        span.attributes
            .get("reward")
            .or_else(|| span.attributes.get("reward_value")),
    )
    .unwrap_or(0.0)
}

fn parse_role(value: &str) -> MessageRole {
    match value {
        "system" => MessageRole::System,
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        "tool" => MessageRole::Tool,
        _ => MessageRole::Assistant,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SpansToMessages, SpansToTrajectories, SpansToTriplets, TraceAdapter, TrajectoryPaddingMode,
        TrajectoryWindowPolicy,
    };
    use serde_json::json;
    use std::collections::HashMap;
    use tau_training_types::TrainingSpan;

    fn span(
        sequence_id: u64,
        name: &str,
        attributes: HashMap<String, serde_json::Value>,
    ) -> TrainingSpan {
        let mut span = TrainingSpan::new(
            "rollout-1",
            "attempt-1",
            sequence_id,
            "trace-1",
            format!("span-{sequence_id}"),
            None,
            name,
        );
        span.attributes = attributes;
        span.end_time = Some(span.start_time);
        span
    }

    #[test]
    fn extracts_triplets_with_reward_fallback() {
        let spans = vec![
            span(
                1,
                "sample",
                HashMap::from([
                    ("prompt".to_string(), json!("question")),
                    ("response".to_string(), json!("answer")),
                ]),
            ),
            span(
                2,
                "reward.emit",
                HashMap::from([("reward_value".to_string(), json!(0.75))]),
            ),
        ];

        let triplets = SpansToTriplets.adapt(&spans).expect("adapt triplets");
        assert_eq!(triplets.len(), 1);
        assert_eq!(triplets[0].reward, Some(0.75));
    }

    #[test]
    fn maps_message_spans_to_tau_messages() {
        let spans = vec![span(
            1,
            "message.added",
            HashMap::from([
                ("role".to_string(), json!("user")),
                ("text".to_string(), json!("hello")),
            ]),
        )];

        let messages = SpansToMessages.adapt(&spans).expect("adapt messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text_content(), "hello");
    }

    #[test]
    fn adapts_spans_into_valid_episode_trajectories() {
        let spans = vec![
            span(
                1,
                "agent.turn.1",
                HashMap::from([
                    ("observation".to_string(), json!({"state": "s0"})),
                    ("action".to_string(), json!({"tool": "search"})),
                    ("reward".to_string(), json!(0.25)),
                ]),
            ),
            span(
                2,
                "agent.turn.2",
                HashMap::from([
                    ("observation".to_string(), json!({"state": "s1"})),
                    ("action".to_string(), json!({"tool": "summarize"})),
                    ("reward_value".to_string(), json!(0.75)),
                ]),
            ),
        ];

        let trajectories = SpansToTrajectories::default()
            .adapt(&spans)
            .expect("adapt trajectories");
        assert_eq!(trajectories.len(), 1);
        assert_eq!(trajectories[0].steps.len(), 2);
        assert_eq!(trajectories[0].steps[0].step_index, 0);
        assert_eq!(trajectories[0].steps[1].step_index, 1);
        assert!(!trajectories[0].steps[0].done);
        assert!(trajectories[0].steps[1].done);
        assert!(trajectories[0].validate().is_ok());
    }

    #[test]
    fn adapts_partial_telemetry_with_fallback_metadata() {
        let spans = vec![span(1, "message.added", HashMap::new())];

        let trajectories = SpansToTrajectories::default()
            .adapt(&spans)
            .expect("adapt trajectories");
        assert_eq!(trajectories.len(), 1);
        assert_eq!(trajectories[0].steps.len(), 1);
        let step = &trajectories[0].steps[0];
        assert!(step.observation.is_object());
        assert!(step.action.is_object());
        assert!(step.done);
        assert!(trajectories[0].validate().is_ok());
    }

    #[test]
    fn returns_deterministic_error_for_empty_input() {
        let error = SpansToTrajectories::default()
            .adapt(&[])
            .expect_err("empty spans should fail");
        assert_eq!(
            error.to_string(),
            "trajectory adapter requires at least one span"
        );
    }

    #[test]
    fn integration_tool_trace_fidelity_preserves_turn_semantics() {
        let spans = vec![
            span(
                1,
                "agent.turn.1",
                HashMap::from([
                    ("input".to_string(), json!({"user": "Find docs for Tau RL"})),
                    (
                        "tool_call".to_string(),
                        json!({"name": "search", "args": {"q": "Tau RL docs"}}),
                    ),
                    ("reward".to_string(), json!(0.1)),
                ]),
            ),
            span(
                2,
                "tool.result",
                HashMap::from([
                    (
                        "observation".to_string(),
                        json!({"tool_result": ["doc-a", "doc-b"]}),
                    ),
                    ("action".to_string(), json!({"name": "summarize"})),
                    ("reward_value".to_string(), json!(0.25)),
                ]),
            ),
            span(
                3,
                "agent.turn.2",
                HashMap::from([
                    (
                        "observation".to_string(),
                        json!({"summary_input": ["doc-a", "doc-b"]}),
                    ),
                    ("response".to_string(), json!("Tau RL pipeline summary...")),
                    ("reward".to_string(), json!(0.8)),
                    ("done".to_string(), json!(true)),
                ]),
            ),
        ];

        let trajectories = SpansToTrajectories::default()
            .adapt(&spans)
            .expect("adapt trajectories");
        assert_eq!(trajectories.len(), 1);
        let trajectory = &trajectories[0];
        assert_eq!(trajectory.steps.len(), 3);
        assert_eq!(trajectory.steps[0].step_index, 0);
        assert_eq!(trajectory.steps[1].step_index, 1);
        assert_eq!(trajectory.steps[2].step_index, 2);
        assert!(!trajectory.steps[0].done);
        assert!(!trajectory.steps[1].done);
        assert!(trajectory.steps[2].done);

        assert_eq!(
            trajectory.steps[0].action,
            json!({"name": "search", "args": {"q": "Tau RL docs"}})
        );
        assert_eq!(trajectory.steps[1].action, json!({"name": "summarize"}));
        assert_eq!(
            trajectory.steps[2].action,
            json!("Tau RL pipeline summary...")
        );
        assert!(trajectory.validate().is_ok());
    }

    #[test]
    fn spec_c01_spans_to_trajectories_default_behavior_is_compatible() {
        let spans = vec![
            span(
                1,
                "agent.turn.1",
                HashMap::from([
                    ("observation".to_string(), json!({"state": "s0"})),
                    ("action".to_string(), json!({"tool": "search"})),
                    ("reward".to_string(), json!(0.25)),
                ]),
            ),
            span(
                2,
                "agent.turn.2",
                HashMap::from([
                    ("observation".to_string(), json!({"state": "s1"})),
                    ("action".to_string(), json!({"tool": "summarize"})),
                    ("reward_value".to_string(), json!(0.75)),
                ]),
            ),
        ];

        let trajectories = SpansToTrajectories::default()
            .adapt(&spans)
            .expect("adapt trajectories");
        assert_eq!(trajectories.len(), 1);
        assert_eq!(trajectories[0].steps.len(), 2);
        assert!(trajectories[0].validate().is_ok());
    }

    #[test]
    fn spec_c02_spans_to_trajectories_truncates_to_tail_window() {
        let spans = vec![
            span(1, "s1", HashMap::from([("reward".to_string(), json!(0.1))])),
            span(2, "s2", HashMap::from([("reward".to_string(), json!(0.2))])),
            span(3, "s3", HashMap::from([("reward".to_string(), json!(0.3))])),
            span(4, "s4", HashMap::from([("reward".to_string(), json!(0.4))])),
            span(5, "s5", HashMap::from([("reward".to_string(), json!(0.5))])),
        ];

        let adapter = SpansToTrajectories::with_window_policy(TrajectoryWindowPolicy {
            window_size: 3,
            padding_mode: TrajectoryPaddingMode::Disabled,
        });
        let trajectories = adapter.adapt(&spans).expect("adapt trajectories");
        assert_eq!(trajectories.len(), 1);
        let steps = &trajectories[0].steps;
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].step_index, 0);
        assert_eq!(steps[1].step_index, 1);
        assert_eq!(steps[2].step_index, 2);
        assert_eq!(steps[0].reward, 0.3);
        assert_eq!(steps[1].reward, 0.4);
        assert_eq!(steps[2].reward, 0.5);
        assert!(!steps[0].done);
        assert!(!steps[1].done);
        assert!(steps[2].done);
        assert!(trajectories[0].validate().is_ok());
    }

    #[test]
    fn spec_c03_spans_to_trajectories_pads_to_fixed_window() {
        let spans = vec![span(
            1,
            "agent.turn.1",
            HashMap::from([
                ("observation".to_string(), json!({"state": "s0"})),
                ("action".to_string(), json!({"tool": "search"})),
                ("reward".to_string(), json!(1.0)),
            ]),
        )];

        let adapter = SpansToTrajectories::with_window_policy(TrajectoryWindowPolicy {
            window_size: 3,
            padding_mode: TrajectoryPaddingMode::PadRight,
        });
        let trajectories = adapter.adapt(&spans).expect("adapt trajectories");
        let steps = &trajectories[0].steps;
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].reward, 1.0);
        assert_eq!(steps[1].reward, 0.0);
        assert_eq!(steps[2].reward, 0.0);
        assert!(!steps[0].done);
        assert!(!steps[1].done);
        assert!(steps[2].done);
        assert_eq!(steps[1].metadata.get("padded"), Some(&json!(true)));
        assert_eq!(steps[2].metadata.get("padded"), Some(&json!(true)));
        assert!(trajectories[0].validate().is_ok());
    }

    #[test]
    fn spec_c04_spans_to_trajectories_rejects_zero_window_size() {
        let spans = vec![span(1, "sample", HashMap::new())];
        let adapter = SpansToTrajectories::with_window_policy(TrajectoryWindowPolicy {
            window_size: 0,
            padding_mode: TrajectoryPaddingMode::Disabled,
        });

        let error = adapter
            .adapt(&spans)
            .expect_err("invalid policy should fail");
        assert!(error
            .to_string()
            .contains("window_size must be greater than 0"));
    }
}
