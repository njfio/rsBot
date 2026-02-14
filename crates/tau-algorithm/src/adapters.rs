use anyhow::Result;
use tau_ai::{Message, MessageRole};
use tau_training_types::{TrainingSpan, Triplet};

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

fn extract_reward_value(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(serde_json::Value::as_f64)
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
    use super::{SpansToMessages, SpansToTriplets, TraceAdapter};
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
}
