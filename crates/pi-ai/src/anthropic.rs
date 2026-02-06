use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::sleep;

use crate::{
    retry::{
        is_retryable_http_error, new_request_id, next_backoff_ms_with_jitter,
        retry_budget_allows_delay, should_retry_status,
    },
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole, PiAiError,
    StreamDeltaHandler, ToolDefinition,
};

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_base: String,
    pub api_key: String,
    pub request_timeout_ms: u64,
    pub max_retries: usize,
    pub retry_budget_ms: u64,
    pub retry_jitter: bool,
}

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    client: reqwest::Client,
    config: AnthropicConfig,
}

impl AnthropicClient {
    pub fn new(config: AnthropicConfig) -> Result<Self, PiAiError> {
        if config.api_key.trim().is_empty() {
            return Err(PiAiError::MissingApiKey);
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(config.api_key.trim())
                .map_err(|e| PiAiError::InvalidResponse(format!("invalid API key header: {e}")))?,
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_millis(
                config.request_timeout_ms.max(1),
            ))
            .build()?;

        Ok(Self { client, config })
    }

    fn messages_url(&self) -> String {
        let base = self.config.api_base.trim_end_matches('/');
        if base.ends_with("/messages") {
            return base.to_string();
        }

        format!("{base}/messages")
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, PiAiError> {
        self.complete_with_mode(request, None).await
    }

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        self.complete_with_mode(request, on_delta).await
    }
}

impl AnthropicClient {
    async fn complete_with_mode(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        let mut body = build_messages_request_body(&request);
        let stream_mode = on_delta.is_some();
        if stream_mode {
            body["stream"] = json!(true);
        }
        let url = self.messages_url();
        let started = std::time::Instant::now();
        let max_retries = self.config.max_retries;

        for attempt in 0..=max_retries {
            let request_id = new_request_id();
            let response = self
                .client
                .post(&url)
                .header("x-pi-request-id", request_id)
                .header("x-pi-retry-attempt", attempt.to_string())
                .json(&body)
                .send()
                .await;

            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        if let Some(delta_handler) = on_delta.clone() {
                            let is_event_stream = response
                                .headers()
                                .get(CONTENT_TYPE)
                                .and_then(|value| value.to_str().ok())
                                .map(|value| {
                                    value.to_ascii_lowercase().contains("text/event-stream")
                                })
                                .unwrap_or(false);
                            if is_event_stream {
                                return parse_messages_stream_response(response, delta_handler)
                                    .await;
                            }

                            let raw = response.text().await?;
                            let parsed = parse_messages_response(&raw)?;
                            let text = parsed.message.text_content();
                            if !text.is_empty() {
                                delta_handler(text);
                            }
                            return Ok(parsed);
                        }

                        let raw = response.text().await?;
                        return parse_messages_response(&raw);
                    }

                    let raw = response.text().await?;
                    if attempt < max_retries && should_retry_status(status.as_u16()) {
                        let backoff_ms =
                            next_backoff_ms_with_jitter(attempt, self.config.retry_jitter);
                        let elapsed_ms = started.elapsed().as_millis() as u64;
                        if retry_budget_allows_delay(
                            elapsed_ms,
                            backoff_ms,
                            self.config.retry_budget_ms,
                        ) {
                            sleep(std::time::Duration::from_millis(backoff_ms)).await;
                            continue;
                        }
                    }

                    return Err(PiAiError::HttpStatus {
                        status: status.as_u16(),
                        body: raw,
                    });
                }
                Err(error) => {
                    if attempt < max_retries && is_retryable_http_error(&error) {
                        let backoff_ms =
                            next_backoff_ms_with_jitter(attempt, self.config.retry_jitter);
                        let elapsed_ms = started.elapsed().as_millis() as u64;
                        if retry_budget_allows_delay(
                            elapsed_ms,
                            backoff_ms,
                            self.config.retry_budget_ms,
                        ) {
                            sleep(std::time::Duration::from_millis(backoff_ms)).await;
                            continue;
                        }
                    }
                    return Err(PiAiError::Http(error));
                }
            }
        }

        Err(PiAiError::InvalidResponse(
            "request retry loop terminated unexpectedly".to_string(),
        ))
    }
}

fn build_messages_request_body(request: &ChatRequest) -> Value {
    let system = extract_system_text(&request.messages);
    let messages = to_anthropic_messages(&request.messages);

    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens.unwrap_or(1024),
    });

    if !system.is_empty() {
        body["system"] = json!(system);
    }

    if !request.tools.is_empty() {
        body["tools"] = to_anthropic_tools(&request.tools);
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }

    body
}

fn extract_system_text(messages: &[Message]) -> String {
    messages
        .iter()
        .filter(|message| message.role == MessageRole::System)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn to_anthropic_tools(tools: &[ToolDefinition]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                })
            })
            .collect(),
    )
}

fn to_anthropic_messages(messages: &[Message]) -> Value {
    Value::Array(
        messages
            .iter()
            .filter_map(|message| match message.role {
                MessageRole::System => None,
                MessageRole::User => {
                    let text = message.text_content();
                    if text.trim().is_empty() {
                        None
                    } else {
                        Some(json!({
                            "role": "user",
                            "content": [{
                                "type": "text",
                                "text": text,
                            }]
                        }))
                    }
                }
                MessageRole::Assistant => {
                    let mut parts = Vec::new();
                    for block in &message.content {
                        match block {
                            ContentBlock::Text { text } => {
                                if !text.trim().is_empty() {
                                    parts.push(json!({
                                        "type": "text",
                                        "text": text,
                                    }));
                                }
                            }
                            ContentBlock::ToolCall {
                                id,
                                name,
                                arguments,
                            } => {
                                parts.push(json!({
                                    "type": "tool_use",
                                    "id": id,
                                    "name": name,
                                    "input": arguments,
                                }));
                            }
                        }
                    }

                    if parts.is_empty() {
                        None
                    } else {
                        Some(json!({
                            "role": "assistant",
                            "content": parts,
                        }))
                    }
                }
                MessageRole::Tool => {
                    if let Some(tool_call_id) = message.tool_call_id.as_deref() {
                        Some(json!({
                            "role": "user",
                            "content": [{
                                "type": "tool_result",
                                "tool_use_id": tool_call_id,
                                "content": message.text_content(),
                                "is_error": message.is_error,
                            }]
                        }))
                    } else {
                        Some(json!({
                            "role": "user",
                            "content": [{
                                "type": "text",
                                "text": "invalid tool result message: missing tool_call_id",
                            }]
                        }))
                    }
                }
            })
            .collect(),
    )
}

fn parse_messages_response(raw: &str) -> Result<ChatResponse, PiAiError> {
    let parsed: AnthropicMessageResponse = serde_json::from_str(raw)?;

    let mut blocks = Vec::new();
    for part in parsed.content {
        match part {
            AnthropicContent::Text { text, .. } => {
                if !text.trim().is_empty() {
                    blocks.push(ContentBlock::Text { text });
                }
            }
            AnthropicContent::ToolUse {
                id, name, input, ..
            } => {
                blocks.push(ContentBlock::ToolCall {
                    id,
                    name,
                    arguments: input,
                });
            }
            AnthropicContent::Other => {}
        }
    }

    let usage = parsed
        .usage
        .map(|usage| ChatUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.input_tokens + usage.output_tokens,
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        message: Message::assistant_blocks(blocks),
        finish_reason: parsed.stop_reason,
        usage,
    })
}

async fn parse_messages_stream_response(
    response: reqwest::Response,
    on_delta: StreamDeltaHandler,
) -> Result<ChatResponse, PiAiError> {
    let mut stream = response.bytes_stream();
    let mut line_buffer = String::new();
    let mut current_event: Option<String> = None;
    let mut current_data = String::new();

    let mut text = String::new();
    let mut tool_calls: Vec<AnthropicToolUseAccumulator> = Vec::new();
    let mut finish_reason = None;
    let mut usage = ChatUsage::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let fragment = std::str::from_utf8(chunk.as_ref()).map_err(|error| {
            PiAiError::InvalidResponse(format!(
                "invalid UTF-8 in Anthropic streaming response: {error}"
            ))
        })?;
        line_buffer.push_str(fragment);

        while let Some(pos) = line_buffer.find('\n') {
            let line = line_buffer[..pos].trim_end_matches('\r').to_string();
            line_buffer.drain(..=pos);

            if line.is_empty() {
                apply_anthropic_stream_event(
                    current_event.take(),
                    current_data.trim(),
                    &on_delta,
                    &mut text,
                    &mut tool_calls,
                    &mut finish_reason,
                    &mut usage,
                )?;
                current_data.clear();
                continue;
            }

            if let Some(event) = line.strip_prefix("event:") {
                current_event = Some(event.trim().to_string());
                continue;
            }

            if let Some(data) = line.strip_prefix("data:") {
                if !current_data.is_empty() {
                    current_data.push('\n');
                }
                current_data.push_str(data.trim());
            }
        }
    }

    if !current_data.trim().is_empty() {
        apply_anthropic_stream_event(
            current_event.take(),
            current_data.trim(),
            &on_delta,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )?;
    }

    Ok(finalize_anthropic_stream_response(
        text,
        tool_calls,
        finish_reason,
        usage,
    ))
}

fn apply_anthropic_stream_event(
    event: Option<String>,
    data: &str,
    on_delta: &StreamDeltaHandler,
    text: &mut String,
    tool_calls: &mut Vec<AnthropicToolUseAccumulator>,
    finish_reason: &mut Option<String>,
    usage: &mut ChatUsage,
) -> Result<(), PiAiError> {
    if data.is_empty() {
        return Ok(());
    }

    let payload: Value = serde_json::from_str(data).map_err(|error| {
        PiAiError::InvalidResponse(format!("failed to parse Anthropic stream chunk: {error}"))
    })?;
    let payload_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let event_type = event.as_deref().unwrap_or_default();

    if payload_type == "error" || event_type == "error" {
        let error_message = payload
            .get("error")
            .and_then(Value::as_object)
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("anthropic stream returned error event");
        return Err(PiAiError::InvalidResponse(error_message.to_string()));
    }

    match payload_type {
        "message_start" => {
            if let Some(input_tokens) = payload
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("usage"))
                .and_then(Value::as_object)
                .and_then(|usage| usage.get("input_tokens"))
                .and_then(Value::as_u64)
            {
                usage.input_tokens = input_tokens;
                usage.total_tokens = usage.input_tokens + usage.output_tokens;
            }
        }
        "content_block_start" => {
            let Some(index) = payload.get("index").and_then(Value::as_u64) else {
                return Ok(());
            };
            let index = index as usize;
            if tool_calls.len() <= index {
                tool_calls.resize_with(index + 1, AnthropicToolUseAccumulator::default);
            }

            let block = payload
                .get("content_block")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                if let Some(id) = block.get("id").and_then(Value::as_str) {
                    tool_calls[index].id = id.to_string();
                }
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    tool_calls[index].name = name.to_string();
                }
                if let Some(input) = block.get("input") {
                    tool_calls[index].input = Some(input.clone());
                }
            }
        }
        "content_block_delta" => {
            let index = payload
                .get("index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(0);
            if tool_calls.len() <= index {
                tool_calls.resize_with(index + 1, AnthropicToolUseAccumulator::default);
            }

            let delta = payload
                .get("delta")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            match delta
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "text_delta" => {
                    if let Some(delta_text) = delta.get("text").and_then(Value::as_str) {
                        if !delta_text.is_empty() {
                            text.push_str(delta_text);
                            on_delta(delta_text.to_string());
                        }
                    }
                }
                "input_json_delta" => {
                    if let Some(partial_json) = delta.get("partial_json").and_then(Value::as_str) {
                        tool_calls[index].partial_json.push_str(partial_json);
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(reason) = payload
                .get("delta")
                .and_then(Value::as_object)
                .and_then(|delta| delta.get("stop_reason"))
                .and_then(Value::as_str)
            {
                *finish_reason = Some(reason.to_string());
            }

            if let Some(output_tokens) = payload
                .get("usage")
                .and_then(Value::as_object)
                .and_then(|usage| usage.get("output_tokens"))
                .and_then(Value::as_u64)
            {
                usage.output_tokens = output_tokens;
                usage.total_tokens = usage.input_tokens + usage.output_tokens;
            }
        }
        _ => {}
    }

    Ok(())
}

fn finalize_anthropic_stream_response(
    text: String,
    tool_calls: Vec<AnthropicToolUseAccumulator>,
    finish_reason: Option<String>,
    usage: ChatUsage,
) -> ChatResponse {
    let mut blocks = Vec::new();
    if !text.trim().is_empty() {
        blocks.push(ContentBlock::Text { text });
    }

    for (index, tool_call) in tool_calls.into_iter().enumerate() {
        if tool_call.name.trim().is_empty() {
            continue;
        }

        let id = if tool_call.id.trim().is_empty() {
            format!("anthropic_tool_{}", index + 1)
        } else {
            tool_call.id
        };
        let arguments = if let Some(input) = tool_call.input {
            input
        } else if !tool_call.partial_json.trim().is_empty() {
            match serde_json::from_str::<Value>(&tool_call.partial_json) {
                Ok(value) => value,
                Err(_) => Value::String(tool_call.partial_json),
            }
        } else {
            json!({})
        };

        blocks.push(ContentBlock::ToolCall {
            id,
            name: tool_call.name,
            arguments,
        });
    }

    ChatResponse {
        message: Message::assistant_blocks(blocks),
        finish_reason,
        usage,
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContent>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Debug, Default)]
struct AnthropicToolUseAccumulator {
    id: String,
    name: String,
    input: Option<Value>,
    partial_json: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    use super::{
        apply_anthropic_stream_event, build_messages_request_body,
        finalize_anthropic_stream_response, parse_messages_response,
    };
    use crate::{ChatRequest, ContentBlock, Message, ToolDefinition};

    #[test]
    fn serializes_tool_messages() {
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                Message::system("You are helpful"),
                Message::user("Read file"),
                Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "toolu_1".to_string(),
                    name: "read".to_string(),
                    arguments: json!({ "path": "README.md" }),
                }]),
                Message::tool_result("toolu_1", "read", "done", false),
            ],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read file".to_string(),
                parameters: json!({"type":"object"}),
            }],
            max_tokens: Some(512),
            temperature: Some(0.0),
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(body["tools"][0]["name"], "read");
        assert_eq!(body["system"], "You are helpful");
    }

    #[test]
    fn parses_tool_use_response() {
        let raw = r#"{
            "content": [
                {"type":"text","text":"Working..."},
                {"type":"tool_use","id":"toolu_1","name":"read","input":{"path":"README.md"}}
            ],
            "stop_reason":"tool_use",
            "usage":{"input_tokens":10,"output_tokens":3}
        }"#;

        let response = parse_messages_response(raw).expect("response should parse");
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
        assert_eq!(response.usage.total_tokens, 13);
    }

    #[test]
    fn functional_stream_event_parsing_emits_text_and_tool_payload() {
        let streamed = Arc::new(Mutex::new(String::new()));
        let sink_streamed = streamed.clone();
        let sink: crate::StreamDeltaHandler = Arc::new(move |delta: String| {
            sink_streamed
                .lock()
                .expect("stream lock")
                .push_str(delta.as_str());
        });

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;
        let mut usage = crate::ChatUsage::default();

        apply_anthropic_stream_event(
            Some("message_start".to_string()),
            r#"{"type":"message_start","message":{"usage":{"input_tokens":8}}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("message_start parses");
        apply_anthropic_stream_event(
            Some("content_block_delta".to_string()),
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("text delta parses");
        apply_anthropic_stream_event(
            Some("content_block_start".to_string()),
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"read","input":{"path":"README.md"}}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("tool start parses");
        apply_anthropic_stream_event(
            Some("message_delta".to_string()),
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("message_delta parses");

        assert_eq!(text, "Hi");
        assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hi");
        assert_eq!(finish_reason.as_deref(), Some("tool_use"));
        assert_eq!(usage.total_tokens, 13);
        assert_eq!(tool_calls.len(), 2);

        let response = finalize_anthropic_stream_response(text, tool_calls, finish_reason, usage);
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(
            response.message.tool_calls()[0].arguments,
            json!({"path":"README.md"})
        );
    }

    #[test]
    fn regression_stream_event_parsing_surfaces_error_events() {
        let sink: crate::StreamDeltaHandler = Arc::new(|_delta: String| {});
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;
        let mut usage = crate::ChatUsage::default();

        let error = apply_anthropic_stream_event(
            Some("error".to_string()),
            r#"{"type":"error","error":{"message":"rate limited"}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect_err("error events should fail");

        assert!(error.to_string().contains("rate limited"));
    }
}
