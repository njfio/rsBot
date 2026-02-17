use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::sleep;

use crate::{
    retry::{
        is_retryable_http_error, new_request_id, parse_retry_after_ms, provider_retry_delay_ms,
        retry_budget_allows_delay, should_retry_status,
    },
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, MediaSource, Message,
    MessageRole, StreamDeltaHandler, TauAiError, ToolChoice, ToolDefinition,
};

#[derive(Debug, Clone)]
/// Public struct `AnthropicConfig` used across Tau components.
pub struct AnthropicConfig {
    pub api_base: String,
    pub api_key: String,
    pub request_timeout_ms: u64,
    pub max_retries: usize,
    pub retry_budget_ms: u64,
    pub retry_jitter: bool,
}

#[derive(Debug, Clone)]
/// Public struct `AnthropicClient` used across Tau components.
pub struct AnthropicClient {
    client: reqwest::Client,
    config: AnthropicConfig,
}

impl AnthropicClient {
    pub fn new(config: AnthropicConfig) -> Result<Self, TauAiError> {
        if config.api_key.trim().is_empty() {
            return Err(TauAiError::MissingApiKey);
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(config.api_key.trim())
                .map_err(|e| TauAiError::InvalidResponse(format!("invalid API key header: {e}")))?,
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
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        self.complete_with_mode(request, None).await
    }

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        self.complete_with_mode(request, on_delta).await
    }
}

impl AnthropicClient {
    async fn complete_with_mode(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
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
                .header("x-tau-request-id", request_id)
                .header("x-tau-retry-attempt", attempt.to_string())
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

                    let retry_after_ms = parse_retry_after_ms(response.headers());
                    let raw = response.text().await?;
                    if attempt < max_retries && should_retry_status(status.as_u16()) {
                        let backoff_ms = provider_retry_delay_ms(
                            attempt,
                            self.config.retry_jitter,
                            retry_after_ms,
                        );
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

                    return Err(TauAiError::HttpStatus {
                        status: status.as_u16(),
                        body: raw,
                    });
                }
                Err(error) => {
                    if attempt < max_retries && is_retryable_http_error(&error) {
                        let backoff_ms =
                            provider_retry_delay_ms(attempt, self.config.retry_jitter, None);
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
                    return Err(TauAiError::Http(error));
                }
            }
        }

        Err(TauAiError::InvalidResponse(
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
        "max_tokens": request
            .max_tokens
            .unwrap_or_else(|| default_max_tokens_for_model(&request.model)),
    });

    if request.json_mode {
        let mut system_segments = vec![
            "Respond with valid JSON only. Do not include markdown code fences or commentary."
                .to_string(),
        ];
        if !system.is_empty() {
            system_segments.push(system);
        }
        set_anthropic_system_prompt(
            &mut body,
            system_segments.join("\n\n"),
            &request.prompt_cache,
        );
    } else if !system.is_empty() {
        set_anthropic_system_prompt(&mut body, system, &request.prompt_cache);
    }

    if !request.tools.is_empty() {
        body["tools"] = to_anthropic_tools(&request.tools);
        if let Some(tool_choice) = request
            .tool_choice
            .as_ref()
            .and_then(to_anthropic_tool_choice)
        {
            body["tool_choice"] = tool_choice;
        }
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }

    body
}

fn set_anthropic_system_prompt(
    body: &mut Value,
    system_prompt: String,
    prompt_cache: &crate::PromptCacheConfig,
) {
    if !prompt_cache.enabled {
        body["system"] = json!(system_prompt);
        return;
    }

    let mut system_block = json!({
        "type": "text",
        "text": system_prompt,
        "cache_control": {
            "type": "ephemeral",
        }
    });
    if let Some(retention) = prompt_cache.retention.as_ref() {
        if !retention.trim().is_empty() {
            system_block["cache_control"]["ttl"] = json!(retention);
        }
    }
    body["system"] = json!([system_block]);
}

fn default_max_tokens_for_model(model: &str) -> u32 {
    match model {
        "claude-opus-4-6" => 128_000,
        "claude-opus-4-5-20251101"
        | "claude-haiku-4-5-20251001"
        | "claude-sonnet-4-5-20250929"
        | "claude-sonnet-4-20250514"
        | "claude-3-7-sonnet-20250219" => 64_000,
        "claude-opus-4-1-20250805" | "claude-opus-4-20250514" => 32_000,
        _ => 4_096,
    }
}

fn to_anthropic_tool_choice(tool_choice: &ToolChoice) -> Option<Value> {
    match tool_choice {
        ToolChoice::Auto => Some(json!({ "type": "auto" })),
        ToolChoice::None => None,
        ToolChoice::Required => Some(json!({ "type": "any" })),
        ToolChoice::Tool { name } => Some(json!({
            "type": "tool",
            "name": name,
        })),
    }
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
                    let parts = to_anthropic_content_parts(message, false);
                    if parts.is_empty() {
                        None
                    } else {
                        Some(json!({
                            "role": "user",
                            "content": parts,
                        }))
                    }
                }
                MessageRole::Assistant => {
                    let parts = to_anthropic_content_parts(message, true);
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

fn to_anthropic_content_parts(message: &Message, allow_tool_calls: bool) -> Vec<Value> {
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
            } if allow_tool_calls => {
                parts.push(json!({
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": arguments,
                }));
            }
            ContentBlock::ToolCall { .. } => {}
            ContentBlock::Image { source } => match source {
                MediaSource::Url { url } => {
                    parts.push(json!({
                        "type": "text",
                        "text": format!("[tau-image:url:{url}]"),
                    }));
                }
                MediaSource::Base64 { mime_type, data } => {
                    parts.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": mime_type,
                            "data": data,
                        }
                    }));
                }
            },
            ContentBlock::Audio { source } => {
                parts.push(json!({
                    "type": "text",
                    "text": format!("[tau-audio:{}]", media_source_descriptor(source)),
                }));
            }
        }
    }

    parts
}

fn media_source_descriptor(source: &MediaSource) -> String {
    match source {
        MediaSource::Url { url } => format!("url:{url}"),
        MediaSource::Base64 { mime_type, data } => {
            format!("base64:{mime_type}:{}bytes", data.len())
        }
    }
}

fn parse_messages_response(raw: &str) -> Result<ChatResponse, TauAiError> {
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
            AnthropicContent::Image {
                source:
                    AnthropicMediaSource::Base64 {
                        media_type, data, ..
                    },
            } => {
                blocks.push(ContentBlock::Image {
                    source: MediaSource::Base64 {
                        mime_type: media_type,
                        data,
                    },
                });
            }
            AnthropicContent::Image {
                source: AnthropicMediaSource::Url { url, .. },
            } => {
                blocks.push(ContentBlock::Image {
                    source: MediaSource::Url { url },
                });
            }
            AnthropicContent::Image {
                source: AnthropicMediaSource::Other,
            } => {}
            AnthropicContent::Audio {
                source:
                    AnthropicMediaSource::Base64 {
                        media_type, data, ..
                    },
            } => {
                blocks.push(ContentBlock::Audio {
                    source: MediaSource::Base64 {
                        mime_type: media_type,
                        data,
                    },
                });
            }
            AnthropicContent::Audio {
                source: AnthropicMediaSource::Url { url, .. },
            } => {
                blocks.push(ContentBlock::Audio {
                    source: MediaSource::Url { url },
                });
            }
            AnthropicContent::Audio {
                source: AnthropicMediaSource::Other,
            } => {}
            AnthropicContent::Other => {}
        }
    }

    let usage = parsed
        .usage
        .map(|usage| ChatUsage {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.input_tokens + usage.output_tokens,
            cached_input_tokens: usage.cache_read_input_tokens.unwrap_or_default(),
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
) -> Result<ChatResponse, TauAiError> {
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
            TauAiError::InvalidResponse(format!(
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
) -> Result<(), TauAiError> {
    if data.is_empty() {
        return Ok(());
    }

    let payload: Value = serde_json::from_str(data).map_err(|error| {
        TauAiError::InvalidResponse(format!("failed to parse Anthropic stream chunk: {error}"))
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
        return Err(TauAiError::InvalidResponse(error_message.to_string()));
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
            if let Some(cache_read_input_tokens) = payload
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("usage"))
                .and_then(Value::as_object)
                .and_then(|usage| usage.get("cache_read_input_tokens"))
                .and_then(Value::as_u64)
            {
                usage.cached_input_tokens = cache_read_input_tokens;
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
            if let Some(cache_read_input_tokens) = payload
                .get("usage")
                .and_then(Value::as_object)
                .and_then(|usage| usage.get("cache_read_input_tokens"))
                .and_then(Value::as_u64)
            {
                usage.cached_input_tokens = cache_read_input_tokens;
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
    #[serde(rename = "image")]
    Image { source: AnthropicMediaSource },
    #[serde(rename = "audio")]
    Audio { source: AnthropicMediaSource },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicMediaSource {
    #[serde(rename = "base64")]
    Base64 {
        #[serde(rename = "media_type")]
        media_type: String,
        data: String,
    },
    #[serde(rename = "url")]
    Url { url: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: Option<u64>,
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
    use crate::{
        ChatRequest, ContentBlock, Message, MessageRole, PromptCacheConfig, ToolChoice,
        ToolDefinition,
    };

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
            tool_choice: Some(ToolChoice::Required),
            json_mode: true,
            max_tokens: Some(512),
            temperature: Some(0.0),
            prompt_cache: Default::default(),
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(body["tools"][0]["name"], "read");
        assert_eq!(body["tool_choice"]["type"], "any");
        let system = body["system"]
            .as_str()
            .expect("json mode system prompt should be a string");
        assert!(system.contains("Respond with valid JSON only"));
        assert!(system.contains("You are helpful"));
    }

    #[test]
    fn functional_serializes_named_tool_choice_for_anthropic() {
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read file".to_string(),
                parameters: json!({"type":"object"}),
            }],
            tool_choice: Some(ToolChoice::Tool {
                name: "read".to_string(),
            }),
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: PromptCacheConfig::default(),
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["tool_choice"]["type"], "tool");
        assert_eq!(body["tool_choice"]["name"], "read");
    }

    #[test]
    fn spec_c02_anthropic_serializes_system_cache_control_when_enabled() {
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![
                Message::system("Stable operating policy"),
                Message::user("hello"),
            ],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: PromptCacheConfig {
                enabled: true,
                cache_key: None,
                retention: Some("5m".to_string()),
                google_cached_content: None,
            },
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["system"][0]["type"], "text");
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn regression_none_tool_choice_is_not_serialized_for_anthropic() {
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read file".to_string(),
                parameters: json!({"type":"object"}),
            }],
            tool_choice: Some(ToolChoice::None),
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: Default::default(),
        };

        let body = build_messages_request_body(&request);
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn regression_anthropic_default_max_tokens_uses_model_specific_limit() {
        let request = ChatRequest {
            model: "claude-opus-4-6".to_string(),
            messages: vec![Message::user("generate a multi-file project with tools")],
            tools: vec![ToolDefinition {
                name: "write".to_string(),
                description: "Write content to a file".to_string(),
                parameters: json!({
                    "type": "object",
                    "required": ["path", "content"],
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    }
                }),
            }],
            tool_choice: Some(ToolChoice::Auto),
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: Default::default(),
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["max_tokens"], 128_000);
    }

    #[test]
    fn regression_anthropic_default_max_tokens_falls_back_for_unknown_models() {
        let request = ChatRequest {
            model: "claude-unknown-model".to_string(),
            messages: vec![Message::user("hello")],
            tools: Vec::new(),
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: Default::default(),
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["max_tokens"], 4_096);
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
    fn spec_c05_anthropic_parses_cache_read_tokens_from_usage() {
        let raw = r#"{
            "content": [
                {"type":"text","text":"cached"}
            ],
            "stop_reason":"end_turn",
            "usage":{"input_tokens":200,"output_tokens":20,"cache_read_input_tokens":160}
        }"#;

        let response = parse_messages_response(raw).expect("response should parse");
        assert_eq!(response.usage.input_tokens, 200);
        assert_eq!(response.usage.cached_input_tokens, 160);
        assert_eq!(response.usage.total_tokens, 220);
    }

    #[test]
    fn unit_serializes_multimodal_blocks_for_anthropic() {
        let request = ChatRequest {
            model: "claude-sonnet-4-20250514".to_string(),
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![
                    ContentBlock::text("describe"),
                    ContentBlock::image_base64("image/png", "aW1hZ2VEYXRh"),
                    ContentBlock::audio_base64("audio/wav", "YXVkaW9EYXRh"),
                ],
                tool_call_id: None,
                tool_name: None,
                is_error: false,
            }],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: Default::default(),
        };

        let body = build_messages_request_body(&request);
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][1]["type"], "image");
        assert_eq!(
            body["messages"][0]["content"][1]["source"]["media_type"],
            "image/png"
        );
        assert_eq!(body["messages"][0]["content"][2]["type"], "text");
        assert!(body["messages"][0]["content"][2]["text"]
            .as_str()
            .expect("audio fallback marker should be a string")
            .contains("[tau-audio:"));
    }

    #[test]
    fn functional_parses_multimodal_response_for_anthropic() {
        let raw = r#"{
            "content": [
                {"type":"text","text":"Working..."},
                {"type":"image","source":{"type":"url","url":"https://example.com/cat.png"}},
                {"type":"audio","source":{"type":"base64","media_type":"audio/wav","data":"YXVkaW9EYXRh"}}
            ],
            "stop_reason":"end_turn",
            "usage":{"input_tokens":10,"output_tokens":3}
        }"#;

        let response = parse_messages_response(raw).expect("response should parse");
        assert_eq!(response.message.text_content(), "Working...");
        assert!(response
            .message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::Image { .. })));
        assert!(response
            .message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::Audio { .. })));
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
