use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenAiAuthScheme {
    #[default]
    Bearer,
    ApiKeyHeader,
}

#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    pub api_base: String,
    pub api_key: String,
    pub organization: Option<String>,
    pub request_timeout_ms: u64,
    pub max_retries: usize,
    pub retry_budget_ms: u64,
    pub retry_jitter: bool,
    pub auth_scheme: OpenAiAuthScheme,
    pub api_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    client: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiClient {
    pub fn new(config: OpenAiConfig) -> Result<Self, PiAiError> {
        if config.api_key.trim().is_empty() {
            return Err(PiAiError::MissingApiKey);
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        match config.auth_scheme {
            OpenAiAuthScheme::Bearer => {
                let bearer = format!("Bearer {}", config.api_key.trim());
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&bearer).map_err(|e| {
                        PiAiError::InvalidResponse(format!("invalid API key header: {e}"))
                    })?,
                );
            }
            OpenAiAuthScheme::ApiKeyHeader => {
                headers.insert(
                    "api-key",
                    HeaderValue::from_str(config.api_key.trim()).map_err(|e| {
                        PiAiError::InvalidResponse(format!("invalid API key header: {e}"))
                    })?,
                );
            }
        }

        if let Some(org) = &config.organization {
            headers.insert(
                "OpenAI-Organization",
                HeaderValue::from_str(org).map_err(|e| {
                    PiAiError::InvalidResponse(format!("invalid organization header: {e}"))
                })?,
            );
        }

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_millis(
                config.request_timeout_ms.max(1),
            ))
            .build()?;

        Ok(Self { client, config })
    }

    fn chat_completions_url(&self) -> String {
        let base = self.config.api_base.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            return base.to_string();
        }

        format!("{base}/chat/completions")
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
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

impl OpenAiClient {
    async fn complete_with_mode(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        let mut body = build_chat_request_body(&request)?;
        let stream_mode = on_delta.is_some();
        if stream_mode {
            body["stream"] = json!(true);
        }
        let url = self.chat_completions_url();
        let started = std::time::Instant::now();
        let max_retries = self.config.max_retries;

        for attempt in 0..=max_retries {
            let request_id = new_request_id();
            let mut request_builder = self
                .client
                .post(&url)
                .header("x-pi-request-id", request_id)
                .header("x-pi-retry-attempt", attempt.to_string());
            if let Some(api_version) = self.config.api_version.as_deref() {
                request_builder = request_builder.query(&[("api-version", api_version)]);
            }
            let response = request_builder.json(&body).send().await;

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
                                return parse_chat_stream_response(response, delta_handler).await;
                            }

                            let raw = response.text().await?;
                            let parsed = parse_chat_response(&raw)?;
                            let text = parsed.message.text_content();
                            if !text.is_empty() {
                                delta_handler(text);
                            }
                            return Ok(parsed);
                        }
                        let raw = response.text().await?;
                        return parse_chat_response(&raw);
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

fn build_chat_request_body(request: &ChatRequest) -> Result<Value, PiAiError> {
    let messages = to_openai_messages(&request.messages)?;
    let mut body = json!({
        "model": request.model,
        "messages": messages,
    });

    if !request.tools.is_empty() {
        body["tools"] = to_openai_tools(&request.tools);
    }

    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }

    Ok(body)
}

fn to_openai_tools(tools: &[ToolDefinition]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect(),
    )
}

fn to_openai_messages(messages: &[Message]) -> Result<Vec<Value>, PiAiError> {
    let mut serialized = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::System => serialized.push(json!({
                "role": "system",
                "content": message.text_content(),
            })),
            MessageRole::User => serialized.push(json!({
                "role": "user",
                "content": message.text_content(),
            })),
            MessageRole::Assistant => {
                let tool_calls: Vec<Value> = message
                    .tool_calls()
                    .into_iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments.to_string(),
                            }
                        })
                    })
                    .collect();

                let text = message.text_content();
                let content = if text.trim().is_empty() && !tool_calls.is_empty() {
                    Value::Null
                } else {
                    Value::String(text)
                };

                if tool_calls.is_empty() {
                    serialized.push(json!({
                        "role": "assistant",
                        "content": content,
                    }));
                } else {
                    serialized.push(json!({
                        "role": "assistant",
                        "content": content,
                        "tool_calls": tool_calls,
                    }));
                }
            }
            MessageRole::Tool => {
                let Some(tool_call_id) = message.tool_call_id.as_deref() else {
                    return Err(PiAiError::InvalidResponse(
                        "tool message is missing tool_call_id".to_string(),
                    ));
                };

                let mut tool_message = json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": message.text_content(),
                });

                if let Some(name) = &message.tool_name {
                    tool_message["name"] = Value::String(name.clone());
                }

                serialized.push(tool_message);
            }
        }
    }

    Ok(serialized)
}

fn parse_chat_response(raw: &str) -> Result<ChatResponse, PiAiError> {
    let parsed: OpenAiChatResponse = serde_json::from_str(raw)?;
    let choice =
        parsed.choices.into_iter().next().ok_or_else(|| {
            PiAiError::InvalidResponse("response contained no choices".to_string())
        })?;

    let text = extract_text(&choice.message.content);
    let mut content = Vec::new();
    if !text.trim().is_empty() {
        content.push(ContentBlock::Text { text });
    }

    if let Some(tool_calls) = choice.message.tool_calls {
        for tool_call in tool_calls {
            if tool_call.call_type != "function" {
                continue;
            }

            let arguments = match serde_json::from_str::<Value>(&tool_call.function.arguments) {
                Ok(value) => value,
                Err(_) => Value::String(tool_call.function.arguments),
            };

            content.push(ContentBlock::ToolCall {
                id: tool_call.id,
                name: tool_call.function.name,
                arguments,
            });
        }
    }

    let message = Message {
        role: MessageRole::Assistant,
        content,
        tool_call_id: None,
        tool_name: None,
        is_error: false,
    };

    let usage = parsed
        .usage
        .map(|usage| ChatUsage {
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        message,
        finish_reason: choice.finish_reason,
        usage,
    })
}

async fn parse_chat_stream_response(
    response: reqwest::Response,
    on_delta: StreamDeltaHandler,
) -> Result<ChatResponse, PiAiError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut finish_reason = None;
    let mut text = String::new();
    let mut tool_calls: Vec<OpenAiToolCallAccumulator> = Vec::new();
    let mut usage = ChatUsage::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let fragment = std::str::from_utf8(chunk.as_ref()).map_err(|error| {
            PiAiError::InvalidResponse(format!("invalid UTF-8 in streaming response: {error}"))
        })?;
        buffer.push_str(fragment);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer.drain(..=pos);
            if line.is_empty() {
                continue;
            }

            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    return Ok(finalize_stream_response(
                        text,
                        tool_calls,
                        finish_reason,
                        usage,
                    ));
                }

                apply_stream_data(
                    data,
                    &on_delta,
                    &mut text,
                    &mut tool_calls,
                    &mut finish_reason,
                    &mut usage,
                )?;
            }
        }
    }

    let trailing = buffer.trim();
    if !trailing.is_empty() {
        if let Some(data) = trailing.strip_prefix("data:") {
            let data = data.trim();
            if data != "[DONE]" {
                apply_stream_data(
                    data,
                    &on_delta,
                    &mut text,
                    &mut tool_calls,
                    &mut finish_reason,
                    &mut usage,
                )?;
            }
        }
    }

    Ok(finalize_stream_response(
        text,
        tool_calls,
        finish_reason,
        usage,
    ))
}

fn apply_stream_data(
    data: &str,
    on_delta: &StreamDeltaHandler,
    text: &mut String,
    tool_calls: &mut Vec<OpenAiToolCallAccumulator>,
    finish_reason: &mut Option<String>,
    usage: &mut ChatUsage,
) -> Result<(), PiAiError> {
    let chunk: OpenAiStreamChunk = serde_json::from_str(data).map_err(|error| {
        PiAiError::InvalidResponse(format!("failed to parse OpenAI stream chunk: {error}"))
    })?;

    if let Some(chunk_usage) = chunk.usage {
        usage.input_tokens = chunk_usage.prompt_tokens;
        usage.output_tokens = chunk_usage.completion_tokens;
        usage.total_tokens = chunk_usage.total_tokens;
    }

    for choice in chunk.choices {
        if let Some(reason) = choice.finish_reason {
            *finish_reason = Some(reason);
        }

        let Some(delta) = choice.delta else {
            continue;
        };

        if let Some(delta_text) = delta.content {
            if !delta_text.is_empty() {
                text.push_str(&delta_text);
                on_delta(delta_text);
            }
        }

        if let Some(delta_tool_calls) = delta.tool_calls {
            for delta_call in delta_tool_calls {
                let index = delta_call.index;
                if tool_calls.len() <= index {
                    tool_calls.resize_with(index + 1, OpenAiToolCallAccumulator::default);
                }

                let current = &mut tool_calls[index];
                if let Some(id) = delta_call.id {
                    if !id.is_empty() {
                        current.id = id;
                    }
                }
                if let Some(function) = delta_call.function {
                    if let Some(name) = function.name {
                        if !name.is_empty() {
                            current.name = name;
                        }
                    }
                    if let Some(arguments) = function.arguments {
                        current.arguments.push_str(&arguments);
                    }
                }
            }
        }
    }

    Ok(())
}

fn finalize_stream_response(
    text: String,
    tool_calls: Vec<OpenAiToolCallAccumulator>,
    finish_reason: Option<String>,
    usage: ChatUsage,
) -> ChatResponse {
    let mut content = Vec::new();
    if !text.trim().is_empty() {
        content.push(ContentBlock::Text { text });
    }

    for (index, tool_call) in tool_calls.into_iter().enumerate() {
        if tool_call.name.trim().is_empty() {
            continue;
        }

        let id = if tool_call.id.trim().is_empty() {
            format!("stream_tool_call_{}", index + 1)
        } else {
            tool_call.id
        };
        let arguments = match serde_json::from_str::<Value>(&tool_call.arguments) {
            Ok(value) => value,
            Err(_) => Value::String(tool_call.arguments),
        };
        content.push(ContentBlock::ToolCall {
            id,
            name: tool_call.name,
            arguments,
        });
    }

    ChatResponse {
        message: Message {
            role: MessageRole::Assistant,
            content,
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        },
        finish_reason,
        usage,
    }
}

fn extract_text(content: &Option<Value>) -> String {
    match content {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.as_object())
            .filter_map(|obj| {
                if obj.get("type").and_then(Value::as_str) == Some("text") {
                    obj.get("text")
                } else {
                    None
                }
            })
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(""),
        Some(other) => match other {
            Value::Number(number) => number.to_string(),
            Value::Bool(flag) => flag.to_string(),
            Value::Object(_) | Value::Array(_) => other.to_string(),
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
        },
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceMessage {
    content: Option<Value>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiStreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiStreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<OpenAiStreamFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct OpenAiToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    use super::{
        apply_stream_data, build_chat_request_body, finalize_stream_response, parse_chat_response,
    };
    use crate::{ChatRequest, ContentBlock, Message, ToolDefinition};

    #[test]
    fn serializes_assistant_tool_calls_for_openai() {
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![
                Message::system("You are helpful"),
                Message::user("read file"),
                Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call_1".to_string(),
                    name: "read".to_string(),
                    arguments: json!({ "path": "README.md" }),
                }]),
                Message::tool_result("call_1", "read", "hello", false),
            ],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read a file".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }],
            max_tokens: Some(512),
            temperature: Some(0.0),
        };

        let body = build_chat_request_body(&request).expect("request body must serialize");
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["name"],
            "read"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["tools"][0]["function"]["name"], "read");
    }

    #[test]
    fn parses_tool_calls_from_response() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "read",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 4,
                "total_tokens": 14
            }
        }"#;

        let response = parse_chat_response(raw).expect("response must parse");
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(response.usage.total_tokens, 14);
        assert_eq!(response.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn functional_stream_chunk_parsing_appends_deltas_and_tool_calls() {
        let emitted = Arc::new(Mutex::new(String::new()));
        let sink_emitted = emitted.clone();
        let sink: crate::StreamDeltaHandler = Arc::new(move |delta: String| {
            sink_emitted.lock().expect("delta lock").push_str(&delta);
        });
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;
        let mut usage = crate::ChatUsage::default();

        apply_stream_data(
            r#"{"choices":[{"delta":{"content":"Hel"}}]}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("first stream chunk parse");
        apply_stream_data(
            r#"{"choices":[{"delta":{"content":"lo","tool_calls":[{"index":0,"id":"call_1","function":{"name":"read","arguments":"{\"path\":\"README"}}]}}]}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("second stream chunk parse");
        apply_stream_data(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":".md\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":4,"completion_tokens":3,"total_tokens":7}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("third stream chunk parse");

        assert_eq!(text, "Hello");
        assert_eq!(emitted.lock().expect("delta lock").as_str(), "Hello");
        assert_eq!(finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(usage.total_tokens, 7);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "read");

        let response = finalize_stream_response(text, tool_calls, finish_reason, usage);
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(
            response.message.tool_calls()[0].arguments,
            json!({"path":"README.md"})
        );
    }

    #[test]
    fn regression_stream_chunk_parse_returns_actionable_error() {
        let sink: crate::StreamDeltaHandler = Arc::new(|_delta: String| {});
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;
        let mut usage = crate::ChatUsage::default();

        let error = apply_stream_data(
            r#"{"choices":[{"delta":{"content":"hi"}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect_err("invalid JSON should fail");

        assert!(error
            .to_string()
            .contains("failed to parse OpenAI stream chunk"));
    }
}
