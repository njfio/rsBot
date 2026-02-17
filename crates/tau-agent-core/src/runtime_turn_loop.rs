//! Turn-loop orchestration helpers for request shaping, retries, and structured output.

use std::time::Duration;

use jsonschema::validator_for;
use serde_json::Value;
use tau_ai::{ChatRequest, ChatUsage, Message, MessageRole, TauAiError, ToolDefinition};

use crate::{
    AgentError, CONTEXT_SUMMARY_MAX_CHARS, CONTEXT_SUMMARY_MAX_EXCERPTS, CONTEXT_SUMMARY_PREFIX,
    CONTEXT_SUMMARY_SNIPPET_MAX_CHARS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChatRequestTokenEstimate {
    pub(crate) input_tokens: u32,
    pub(crate) total_tokens: u32,
}

pub(crate) fn estimate_chat_request_tokens(request: &ChatRequest) -> ChatRequestTokenEstimate {
    let message_tokens = request.messages.iter().fold(0u32, |acc, message| {
        acc.saturating_add(estimate_message_tokens(message))
    });
    let tool_tokens = request.tools.iter().fold(0u32, |acc, tool| {
        acc.saturating_add(estimate_tool_definition_tokens(tool))
    });
    let input_tokens = message_tokens.saturating_add(tool_tokens).saturating_add(2);
    let total_tokens = input_tokens.saturating_add(request.max_tokens.unwrap_or(0));

    ChatRequestTokenEstimate {
        input_tokens,
        total_tokens,
    }
}

fn estimate_message_tokens(message: &Message) -> u32 {
    let mut total = 4u32;
    for block in &message.content {
        match block {
            tau_ai::ContentBlock::Text { text } => {
                total = total.saturating_add(estimate_text_tokens(text));
            }
            tau_ai::ContentBlock::ToolCall {
                id,
                name,
                arguments,
            } => {
                total = total.saturating_add(estimate_text_tokens(id));
                total = total.saturating_add(estimate_text_tokens(name));
                total = total.saturating_add(estimate_json_tokens(arguments));
                total = total.saturating_add(4);
            }
            tau_ai::ContentBlock::Image { source } => {
                total = total.saturating_add(estimate_media_source_tokens(source));
                total = total.saturating_add(8);
            }
            tau_ai::ContentBlock::Audio { source } => {
                total = total.saturating_add(estimate_media_source_tokens(source));
                total = total.saturating_add(8);
            }
        }
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        total = total.saturating_add(estimate_text_tokens(tool_call_id));
    }
    if let Some(tool_name) = &message.tool_name {
        total = total.saturating_add(estimate_text_tokens(tool_name));
    }
    total
}

fn estimate_tool_definition_tokens(definition: &ToolDefinition) -> u32 {
    let mut total = 12u32;
    total = total.saturating_add(estimate_text_tokens(&definition.name));
    total = total.saturating_add(estimate_text_tokens(&definition.description));
    total = total.saturating_add(estimate_json_tokens(&definition.parameters));
    total
}

fn estimate_json_tokens(value: &Value) -> u32 {
    let rendered = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    estimate_text_tokens(&rendered)
}

fn estimate_media_source_tokens(source: &tau_ai::MediaSource) -> u32 {
    match source {
        tau_ai::MediaSource::Url { url } => estimate_text_tokens(url),
        tau_ai::MediaSource::Base64 { mime_type, data } => estimate_text_tokens(mime_type)
            .saturating_add((data.len() as u32).saturating_div(3))
            .saturating_add(2),
    }
}

fn estimate_text_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    chars.saturating_add(3) / 4
}

pub(crate) fn estimate_usage_cost_usd(
    usage: &ChatUsage,
    input_cost_per_million: Option<f64>,
    cached_input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
) -> f64 {
    let cached_input_tokens = usage.cached_input_tokens.min(usage.input_tokens);
    let uncached_input_tokens = usage.input_tokens.saturating_sub(cached_input_tokens);
    let input = input_cost_per_million
        .unwrap_or(0.0)
        .max(0.0)
        .mul_add(uncached_input_tokens as f64, 0.0)
        / 1_000_000.0;
    let cached_input = cached_input_cost_per_million
        .unwrap_or_else(|| input_cost_per_million.unwrap_or(0.0))
        .max(0.0)
        .mul_add(cached_input_tokens as f64, 0.0)
        / 1_000_000.0;
    let output = output_cost_per_million
        .unwrap_or(0.0)
        .max(0.0)
        .mul_add(usage.output_tokens as f64, 0.0)
        / 1_000_000.0;
    input + cached_input + output
}

pub(crate) fn normalize_cost_alert_thresholds(thresholds: &[u8]) -> Vec<u8> {
    let mut normalized = thresholds
        .iter()
        .copied()
        .filter(|threshold| (1..=100).contains(threshold))
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        normalized.push(100);
    }
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

pub(crate) fn bounded_messages(messages: &[Message], max_messages: usize) -> Vec<Message> {
    if max_messages == 0 || messages.len() <= max_messages {
        return messages.to_vec();
    }

    if max_messages < 3 {
        return bounded_messages_without_summary(messages, max_messages);
    }

    if matches!(
        messages.first().map(|message| message.role),
        Some(MessageRole::System)
    ) {
        let tail_keep = max_messages - 2;
        let tail_start = messages.len().saturating_sub(tail_keep);
        if tail_start <= 1 {
            return messages.to_vec();
        }

        let dropped = &messages[1..tail_start];
        if dropped.is_empty() {
            return bounded_messages_without_summary(messages, max_messages);
        }

        let mut bounded = Vec::with_capacity(max_messages);
        bounded.push(messages[0].clone());
        bounded.push(Message::system(summarize_dropped_messages(dropped)));
        bounded.extend_from_slice(&messages[tail_start..]);
        bounded
    } else {
        let tail_keep = max_messages - 1;
        let tail_start = messages.len().saturating_sub(tail_keep);
        if tail_start == 0 {
            return messages.to_vec();
        }

        let dropped = &messages[..tail_start];
        if dropped.is_empty() {
            return bounded_messages_without_summary(messages, max_messages);
        }

        let mut bounded = Vec::with_capacity(max_messages);
        bounded.push(Message::system(summarize_dropped_messages(dropped)));
        bounded.extend_from_slice(&messages[tail_start..]);
        bounded
    }
}

fn bounded_messages_without_summary(messages: &[Message], max_messages: usize) -> Vec<Message> {
    if max_messages == 0 || messages.len() <= max_messages {
        return messages.to_vec();
    }

    if max_messages > 1
        && matches!(
            messages.first().map(|message| message.role),
            Some(MessageRole::System)
        )
    {
        let tail_keep = max_messages - 1;
        let tail_start = messages.len().saturating_sub(tail_keep);
        if tail_start <= 1 {
            return messages.to_vec();
        }
        let mut bounded = Vec::with_capacity(max_messages);
        bounded.push(messages[0].clone());
        bounded.extend_from_slice(&messages[tail_start..]);
        bounded
    } else {
        messages[messages.len() - max_messages..].to_vec()
    }
}

fn summarize_dropped_messages(messages: &[Message]) -> String {
    let mut user_count = 0usize;
    let mut assistant_count = 0usize;
    let mut tool_count = 0usize;
    let mut system_count = 0usize;
    let mut excerpts = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::User => user_count = user_count.saturating_add(1),
            MessageRole::Assistant => assistant_count = assistant_count.saturating_add(1),
            MessageRole::Tool => tool_count = tool_count.saturating_add(1),
            MessageRole::System => system_count = system_count.saturating_add(1),
        }

        let content = collapse_whitespace(&message.text_content());
        if content.is_empty() {
            continue;
        }
        if message.role == MessageRole::System && content.starts_with(CONTEXT_SUMMARY_PREFIX) {
            continue;
        }
        if excerpts.len() >= CONTEXT_SUMMARY_MAX_EXCERPTS {
            continue;
        }

        let preview = truncate_chars(&content, CONTEXT_SUMMARY_SNIPPET_MAX_CHARS);
        excerpts.push(format!("- {}: {}", role_label(message.role), preview));
    }

    let mut summary = format!(
        "{CONTEXT_SUMMARY_PREFIX}\n\
         summarized_messages={}; roles: user={}, assistant={}, tool={}, system={}.",
        messages.len(),
        user_count,
        assistant_count,
        tool_count,
        system_count
    );

    if !excerpts.is_empty() {
        let excerpt_block = excerpts.join("\n");
        summary.push_str("\nexcerpts:\n");
        summary.push_str(&excerpt_block);
    }

    truncate_chars(&summary, CONTEXT_SUMMARY_MAX_CHARS)
}

pub(crate) fn role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
        MessageRole::System => "system",
    }
}

pub(crate) fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn truncate_chars(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let truncate_at = text
        .char_indices()
        .nth(max_chars - 1)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let mut truncated = text[..truncate_at].to_string();
    truncated.push('…');
    truncated
}

pub(crate) fn build_structured_output_retry_prompt(schema: &Value, error: &str) -> String {
    let schema_text = serde_json::to_string(schema).unwrap_or_else(|_| schema.to_string());
    format!(
        "Your previous response could not be accepted as structured JSON ({error}). \
Please reply with only valid JSON that matches this schema exactly:\n{schema_text}"
    )
}

pub(crate) fn parse_structured_output(
    messages: &[Message],
    schema: &Value,
) -> Result<Value, AgentError> {
    let assistant = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .ok_or_else(|| {
            AgentError::StructuredOutput(
                "assistant response missing for structured output".to_string(),
            )
        })?;
    let content = assistant.text_content();
    let value = extract_json_payload(&content).map_err(AgentError::StructuredOutput)?;
    validate_json_against_schema(schema, &value).map_err(AgentError::StructuredOutput)?;
    Ok(value)
}

pub(crate) fn extract_json_payload(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("assistant response was empty; expected JSON output".to_string());
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    let mut cursor = 0usize;
    while let Some(open_rel) = text[cursor..].find("```") {
        let open = cursor + open_rel;
        let after_open = &text[open + 3..];
        let header_end_rel = after_open.find('\n').unwrap_or(after_open.len());
        let header = after_open[..header_end_rel].trim();
        let block_start = if header_end_rel < after_open.len() {
            open + 3 + header_end_rel + 1
        } else {
            open + 3 + header_end_rel
        };
        let Some(close_rel) = text[block_start..].find("```") else {
            break;
        };
        let close = block_start + close_rel;
        cursor = close + 3;

        if !(header.is_empty() || header.eq_ignore_ascii_case("json")) {
            continue;
        }

        let block = text[block_start..close].trim();
        if block.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(block) {
            return Ok(value);
        }
    }

    Err("assistant response did not contain parseable JSON content".to_string())
}

fn validate_json_against_schema(schema: &Value, payload: &Value) -> Result<(), String> {
    let validator = validator_for(schema)
        .map_err(|error| format!("invalid structured output schema: {error}"))?;
    let mut errors = validator.iter_errors(payload);
    if let Some(first) = errors.next() {
        return Err(format!(
            "structured output schema validation failed: {first}"
        ));
    }
    Ok(())
}
#[derive(Default)]
pub(crate) struct StreamingRetryBufferState {
    delivered_output: String,
    attempt_output: String,
}

impl StreamingRetryBufferState {
    pub(crate) fn reset_attempt(&mut self) {
        self.attempt_output.clear();
    }
}

pub(crate) fn stream_retry_buffer_on_delta(
    state: &mut StreamingRetryBufferState,
    delta: &str,
) -> Option<String> {
    state.attempt_output.push_str(delta);
    if state.delivered_output.is_empty() {
        state.delivered_output.push_str(delta);
        return Some(delta.to_string());
    }

    if state.attempt_output.len() <= state.delivered_output.len() {
        if state.delivered_output.starts_with(&state.attempt_output) {
            return None;
        }
        state.delivered_output.push_str(delta);
        return Some(delta.to_string());
    }

    if state.attempt_output.starts_with(&state.delivered_output) {
        let replay = state
            .attempt_output
            .get(state.delivered_output.len()..)
            .unwrap_or_default()
            .to_string();
        if replay.is_empty() {
            return None;
        }
        state.delivered_output.push_str(&replay);
        return Some(replay);
    }

    state.delivered_output.push_str(delta);
    Some(delta.to_string())
}

pub(crate) fn timeout_duration_from_ms(timeout_ms: Option<u64>) -> Option<Duration> {
    timeout_ms
        .filter(|timeout_ms| *timeout_ms > 0)
        .map(Duration::from_millis)
}

pub(crate) fn is_retryable_ai_error(error: &TauAiError) -> bool {
    match error {
        TauAiError::Http(http) => http.is_timeout() || http.is_connect(),
        TauAiError::HttpStatus { status, .. } => {
            *status == 408 || *status == 409 || *status == 425 || *status == 429 || *status >= 500
        }
        TauAiError::MissingApiKey | TauAiError::Serde(_) | TauAiError::InvalidResponse(_) => false,
    }
}
