use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
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

const DEFAULT_OPENROUTER_X_TITLE: &str = "tau-rs";

fn non_empty_env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn is_openrouter_route(api_base: &str) -> bool {
    let normalized = api_base.trim().trim_end_matches('/').to_ascii_lowercase();
    if normalized.contains("openrouter.ai") {
        return true;
    }

    let Some(configured_base) = non_empty_env_var("TAU_OPENROUTER_API_BASE") else {
        return false;
    };
    configured_base
        .trim_end_matches('/')
        .eq_ignore_ascii_case(normalized.as_str())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
/// Enumerates supported `OpenAiAuthScheme` values.
pub enum OpenAiAuthScheme {
    #[default]
    Bearer,
    ApiKeyHeader,
}

#[derive(Debug, Clone)]
/// Public struct `OpenAiConfig` used across Tau components.
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
/// Public struct `OpenAiClient` used across Tau components.
pub struct OpenAiClient {
    client: reqwest::Client,
    config: OpenAiConfig,
}

impl OpenAiClient {
    pub fn new(config: OpenAiConfig) -> Result<Self, TauAiError> {
        if config.api_key.trim().is_empty() {
            return Err(TauAiError::MissingApiKey);
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        match config.auth_scheme {
            OpenAiAuthScheme::Bearer => {
                let bearer = format!("Bearer {}", config.api_key.trim());
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&bearer).map_err(|e| {
                        TauAiError::InvalidResponse(format!("invalid API key header: {e}"))
                    })?,
                );
            }
            OpenAiAuthScheme::ApiKeyHeader => {
                headers.insert(
                    "api-key",
                    HeaderValue::from_str(config.api_key.trim()).map_err(|e| {
                        TauAiError::InvalidResponse(format!("invalid API key header: {e}"))
                    })?,
                );
            }
        }

        if is_openrouter_route(&config.api_base) {
            let title = non_empty_env_var("TAU_OPENROUTER_X_TITLE")
                .unwrap_or_else(|| DEFAULT_OPENROUTER_X_TITLE.to_string());
            headers.insert(
                "X-Title",
                HeaderValue::from_str(&title).map_err(|e| {
                    TauAiError::InvalidResponse(format!("invalid OpenRouter X-Title header: {e}"))
                })?,
            );
            if let Some(referer) = non_empty_env_var("TAU_OPENROUTER_HTTP_REFERER") {
                headers.insert(
                    "HTTP-Referer",
                    HeaderValue::from_str(&referer).map_err(|e| {
                        TauAiError::InvalidResponse(format!(
                            "invalid OpenRouter HTTP-Referer header: {e}"
                        ))
                    })?,
                );
            }
        }

        if let Some(org) = &config.organization {
            headers.insert(
                "OpenAI-Organization",
                HeaderValue::from_str(org).map_err(|e| {
                    TauAiError::InvalidResponse(format!("invalid organization header: {e}"))
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

    fn responses_url(&self) -> String {
        let base = self.config.api_base.trim_end_matches('/');
        if base.ends_with("/responses") {
            return base.to_string();
        }
        if let Some(prefix) = base.strip_suffix("/chat/completions") {
            return format!("{prefix}/responses");
        }

        format!("{base}/responses")
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
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

impl OpenAiClient {
    async fn complete_with_mode(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        if model_prefers_responses_api(&request.model) {
            match self
                .complete_via_responses(&request, on_delta.clone())
                .await
            {
                Ok(response) => return Ok(response),
                Err(error) if should_fallback_responses_error_to_chat(&error) => {
                    return self.complete_via_chat(&request, on_delta).await;
                }
                Err(error) => return Err(error),
            }
        }

        match self.complete_via_chat(&request, on_delta.clone()).await {
            Ok(response) => Ok(response),
            Err(error) if should_fallback_chat_error_to_responses(&error) => {
                self.complete_via_responses(&request, on_delta).await
            }
            Err(error) => Err(error),
        }
    }

    async fn complete_via_chat(
        &self,
        request: &ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        let mut body = build_chat_request_body(request)?;
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
                .header("x-tau-request-id", request_id)
                .header("x-tau-retry-attempt", attempt.to_string());
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

    async fn complete_via_responses(
        &self,
        request: &ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        let body = build_responses_request_body(request)?;
        let url = self.responses_url();
        let started = std::time::Instant::now();
        let max_retries = self.config.max_retries;

        for attempt in 0..=max_retries {
            let request_id = new_request_id();
            let mut request_builder = self
                .client
                .post(&url)
                .header("x-tau-request-id", request_id)
                .header("x-tau-retry-attempt", attempt.to_string());
            if let Some(api_version) = self.config.api_version.as_deref() {
                request_builder = request_builder.query(&[("api-version", api_version)]);
            }
            let response = request_builder.json(&body).send().await;

            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let raw = response.text().await?;
                        let parsed = parse_responses_api_response(&raw)?;
                        if let Some(delta_handler) = on_delta.clone() {
                            let text = parsed.message.text_content();
                            if !text.is_empty() {
                                delta_handler(text);
                            }
                        }
                        return Ok(parsed);
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

fn build_chat_request_body(request: &ChatRequest) -> Result<Value, TauAiError> {
    let messages = to_openai_messages(&request.messages)?;
    let mut body = json!({
        "model": request.model,
        "messages": messages,
    });

    if !request.tools.is_empty() {
        body["tools"] = to_openai_tools(&request.tools);
    }

    if let Some(tool_choice) = request.tool_choice.as_ref() {
        if !request.tools.is_empty() || matches!(tool_choice, ToolChoice::None) {
            body["tool_choice"] = to_openai_tool_choice(tool_choice);
        }
    }

    if request.json_mode {
        body["response_format"] = json!({
            "type": "json_object",
        });
    }

    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }

    apply_openai_prompt_cache_fields(&mut body, request);

    Ok(body)
}

fn build_responses_request_body(request: &ChatRequest) -> Result<Value, TauAiError> {
    let input = to_openai_responses_input(&request.messages)?;
    let mut body = json!({
        "model": request.model,
        "input": input,
    });

    if !request.tools.is_empty() {
        body["tools"] = to_openai_responses_tools(&request.tools);
    }

    if let Some(tool_choice) = request.tool_choice.as_ref() {
        if !request.tools.is_empty() || matches!(tool_choice, ToolChoice::None) {
            body["tool_choice"] = to_openai_responses_tool_choice(tool_choice);
        }
    }

    if request.json_mode {
        body["text"] = json!({
            "format": {
                "type": "json_object",
            }
        });
    }

    if let Some(max_tokens) = request.max_tokens {
        body["max_output_tokens"] = json!(max_tokens);
    }

    apply_openai_prompt_cache_fields(&mut body, request);

    Ok(body)
}

fn apply_openai_prompt_cache_fields(body: &mut Value, request: &ChatRequest) {
    if !request.prompt_cache.enabled {
        return;
    }
    if let Some(cache_key) = request.prompt_cache.cache_key.as_ref() {
        if !cache_key.trim().is_empty() {
            body["prompt_cache_key"] = json!(cache_key);
        }
    }
    if let Some(retention) = request.prompt_cache.retention.as_ref() {
        if !retention.trim().is_empty() {
            body["prompt_cache_retention"] = json!(retention);
        }
    }
}

fn model_prefers_responses_api(model: &str) -> bool {
    model.to_ascii_lowercase().contains("codex")
}

fn should_fallback_chat_error_to_responses(error: &TauAiError) -> bool {
    let TauAiError::HttpStatus { status, body } = error else {
        return false;
    };
    if !matches!(status, 400 | 404 | 422) {
        return false;
    }

    let normalized = body.to_ascii_lowercase();
    normalized.contains("not supported in the v1/chat/completions endpoint")
        || normalized.contains("use this model with the responses api")
        || (normalized.contains("chat/completions") && normalized.contains("responses"))
}

fn should_fallback_responses_error_to_chat(error: &TauAiError) -> bool {
    let TauAiError::HttpStatus { status, body } = error else {
        return false;
    };
    if !matches!(status, 400 | 404 | 405) {
        return false;
    }

    let normalized = body.to_ascii_lowercase();
    (normalized.contains("/responses")
        && (normalized.contains("no route")
            || normalized.contains("not found")
            || normalized.contains("unsupported")
            || normalized.contains("unknown")))
        || normalized.contains("unknown url")
}

fn to_openai_tool_choice(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Tool { name } => json!({
            "type": "function",
            "function": {
                "name": name,
            }
        }),
    }
}

fn to_openai_responses_tool_choice(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => json!("auto"),
        ToolChoice::None => json!("none"),
        ToolChoice::Required => json!("required"),
        ToolChoice::Tool { name } => json!({
            "type": "function",
            "name": name,
        }),
    }
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

fn to_openai_responses_tools(tools: &[ToolDefinition]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                })
            })
            .collect(),
    )
}

fn to_openai_responses_input(messages: &[Message]) -> Result<Value, TauAiError> {
    let mut input_items = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::System | MessageRole::User | MessageRole::Assistant => {
                let text = flatten_message_with_media_markers(message);
                if !text.trim().is_empty() {
                    input_items.push(json!({
                        "role": to_openai_role_name(message.role),
                        "content": text,
                    }));
                }

                if matches!(message.role, MessageRole::Assistant) {
                    for tool_call in message.tool_calls() {
                        input_items.push(json!({
                            "type": "function_call",
                            "call_id": tool_call.id,
                            "name": tool_call.name,
                            "arguments": stringify_tool_arguments(&tool_call.arguments),
                        }));
                    }
                }
            }
            MessageRole::Tool => {
                let Some(tool_call_id) = message.tool_call_id.as_deref() else {
                    return Err(TauAiError::InvalidResponse(
                        "tool message is missing tool_call_id".to_string(),
                    ));
                };

                let output = flatten_message_with_media_markers(message);
                input_items.push(json!({
                    "type": "function_call_output",
                    "call_id": tool_call_id,
                    "output": output,
                }));
            }
        }
    }

    if input_items.is_empty() {
        return Ok(Value::String(String::new()));
    }

    Ok(Value::Array(input_items))
}

fn to_openai_messages(messages: &[Message]) -> Result<Vec<Value>, TauAiError> {
    let mut serialized = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::System => serialized.push(json!({
                "role": "system",
                "content": flatten_message_with_media_markers(message),
            })),
            MessageRole::User => serialized.push(json!({
                "role": "user",
                "content": to_openai_user_content(message),
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

                let text = flatten_message_with_media_markers(message);
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
                    return Err(TauAiError::InvalidResponse(
                        "tool message is missing tool_call_id".to_string(),
                    ));
                };

                let mut tool_message = json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": flatten_message_with_media_markers(message),
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

fn to_openai_user_content(message: &Message) -> Value {
    let has_non_text_block = message
        .content
        .iter()
        .any(|block| !matches!(block, ContentBlock::Text { .. }));
    if !has_non_text_block {
        return Value::String(message.text_content());
    }

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
            ContentBlock::Image { source } => {
                parts.push(to_openai_image_part(source));
            }
            ContentBlock::Audio { source } => {
                parts.push(json!({
                    "type": "text",
                    "text": format!("[tau-audio:{}]", media_source_descriptor(source)),
                }));
            }
            ContentBlock::ToolCall { .. } => {}
        }
    }

    if parts.is_empty() {
        Value::String(String::new())
    } else {
        Value::Array(parts)
    }
}

fn to_openai_role_name(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn stringify_tool_arguments(arguments: &Value) -> String {
    match arguments {
        Value::String(value) => value.clone(),
        value => value.to_string(),
    }
}

fn to_openai_image_part(source: &MediaSource) -> Value {
    match source {
        MediaSource::Url { url } => json!({
            "type": "image_url",
            "image_url": { "url": url },
        }),
        MediaSource::Base64 { mime_type, data } => {
            let data_url = format!("data:{mime_type};base64,{data}");
            json!({
                "type": "image_url",
                "image_url": { "url": data_url },
            })
        }
    }
}

fn flatten_message_with_media_markers(message: &Message) -> String {
    let mut parts = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => {
                if !text.trim().is_empty() {
                    parts.push(text.clone());
                }
            }
            ContentBlock::ToolCall { .. } => {}
            ContentBlock::Image { source } => {
                parts.push(format!("[tau-image:{}]", media_source_descriptor(source)));
            }
            ContentBlock::Audio { source } => {
                parts.push(format!("[tau-audio:{}]", media_source_descriptor(source)));
            }
        }
    }
    parts.join("\n")
}

fn media_source_descriptor(source: &MediaSource) -> String {
    match source {
        MediaSource::Url { url } => format!("url:{url}"),
        MediaSource::Base64 { mime_type, data } => {
            format!("base64:{mime_type}:{}bytes", data.len())
        }
    }
}

fn parse_chat_response(raw: &str) -> Result<ChatResponse, TauAiError> {
    let parsed: OpenAiChatResponse = serde_json::from_str(raw)?;
    let choice =
        parsed.choices.into_iter().next().ok_or_else(|| {
            TauAiError::InvalidResponse("response contained no choices".to_string())
        })?;

    let mut content = parse_openai_content_blocks(&choice.message.content);

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
            cached_input_tokens: usage
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or_default(),
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        message,
        finish_reason: choice.finish_reason,
        usage,
    })
}

fn parse_responses_api_response(raw: &str) -> Result<ChatResponse, TauAiError> {
    let parsed: OpenAiResponsesResponse = serde_json::from_str(raw)?;
    let mut content = Vec::new();

    if let Some(output_items) = parsed.output {
        for output_item in output_items {
            match output_item.item_type.as_deref() {
                Some("message") => {
                    content.extend(parse_openai_content_blocks(&output_item.content));
                }
                Some("function_call") => {
                    let Some(name) = output_item.name else {
                        continue;
                    };
                    let id = output_item
                        .call_id
                        .or(output_item.id)
                        .unwrap_or_else(|| "response_function_call".to_string());
                    let arguments = parse_tool_call_arguments(output_item.arguments.as_deref());
                    content.push(ContentBlock::ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
                _ => {}
            }
        }
    }

    if !content
        .iter()
        .any(|block| matches!(block, ContentBlock::Text { .. }))
    {
        if let Some(output_text) = parsed.output_text {
            if !output_text.trim().is_empty() {
                content.insert(0, ContentBlock::Text { text: output_text });
            }
        }
    }

    let usage = parsed
        .usage
        .map(|usage| {
            let input_tokens = usage
                .input_tokens
                .or(usage.prompt_tokens)
                .unwrap_or_default();
            let output_tokens = usage
                .output_tokens
                .or(usage.completion_tokens)
                .unwrap_or_default();
            let total_tokens = usage.total_tokens.unwrap_or(input_tokens + output_tokens);
            ChatUsage {
                input_tokens,
                output_tokens,
                total_tokens,
                cached_input_tokens: usage
                    .input_tokens_details
                    .as_ref()
                    .and_then(|details| details.cached_tokens)
                    .unwrap_or_default(),
            }
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        message: Message {
            role: MessageRole::Assistant,
            content,
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        },
        finish_reason: parsed.status,
        usage,
    })
}

fn parse_tool_call_arguments(arguments: Option<&str>) -> Value {
    let Some(arguments) = arguments else {
        return Value::Null;
    };

    match serde_json::from_str::<Value>(arguments) {
        Ok(value) => value,
        Err(_) => Value::String(arguments.to_string()),
    }
}

async fn parse_chat_stream_response(
    response: reqwest::Response,
    on_delta: StreamDeltaHandler,
) -> Result<ChatResponse, TauAiError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut finish_reason = None;
    let mut text = String::new();
    let mut tool_calls: Vec<OpenAiToolCallAccumulator> = Vec::new();
    let mut usage = ChatUsage::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let fragment = std::str::from_utf8(chunk.as_ref()).map_err(|error| {
            TauAiError::InvalidResponse(format!("invalid UTF-8 in streaming response: {error}"))
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
) -> Result<(), TauAiError> {
    let chunk: OpenAiStreamChunk = serde_json::from_str(data).map_err(|error| {
        TauAiError::InvalidResponse(format!("failed to parse OpenAI stream chunk: {error}"))
    })?;

    if let Some(chunk_usage) = chunk.usage {
        usage.input_tokens = chunk_usage.prompt_tokens;
        usage.output_tokens = chunk_usage.completion_tokens;
        usage.total_tokens = chunk_usage.total_tokens;
        usage.cached_input_tokens = chunk_usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|details| details.cached_tokens)
            .unwrap_or_default();
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

fn parse_openai_content_blocks(content: &Option<Value>) -> Vec<ContentBlock> {
    match content {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(text)) => {
            if text.trim().is_empty() {
                Vec::new()
            } else {
                vec![ContentBlock::Text { text: text.clone() }]
            }
        }
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.as_object())
            .filter_map(parse_openai_array_part)
            .collect(),
        Some(other) => {
            let rendered = match other {
                Value::Number(number) => number.to_string(),
                Value::Bool(flag) => flag.to_string(),
                Value::Object(_) | Value::Array(_) => other.to_string(),
                Value::String(text) => text.clone(),
                Value::Null => String::new(),
            };
            if rendered.trim().is_empty() {
                Vec::new()
            } else {
                vec![ContentBlock::Text { text: rendered }]
            }
        }
    }
}

fn parse_openai_array_part(part: &serde_json::Map<String, Value>) -> Option<ContentBlock> {
    match part.get("type").and_then(Value::as_str).unwrap_or("text") {
        "text" | "output_text" | "input_text" => {
            let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
            if text.trim().is_empty() {
                None
            } else {
                Some(ContentBlock::Text {
                    text: text.to_string(),
                })
            }
        }
        "image_url" | "input_image" => parse_openai_image_from_part(part),
        "input_audio" => parse_openai_audio_from_part(part),
        _ => {
            let text = part.get("text").and_then(Value::as_str).unwrap_or_default();
            if text.trim().is_empty() {
                None
            } else {
                Some(ContentBlock::Text {
                    text: text.to_string(),
                })
            }
        }
    }
}

fn parse_openai_image_from_part(part: &serde_json::Map<String, Value>) -> Option<ContentBlock> {
    if let Some(url) = part
        .get("image_url")
        .and_then(Value::as_object)
        .and_then(|image| image.get("url"))
        .and_then(Value::as_str)
    {
        return Some(ContentBlock::Image {
            source: MediaSource::Url {
                url: url.to_string(),
            },
        });
    }

    if let Some(url) = part.get("image_url").and_then(Value::as_str) {
        return Some(ContentBlock::Image {
            source: MediaSource::Url {
                url: url.to_string(),
            },
        });
    }

    None
}

fn parse_openai_audio_from_part(part: &serde_json::Map<String, Value>) -> Option<ContentBlock> {
    let audio = part
        .get("input_audio")
        .or_else(|| part.get("audio"))
        .and_then(Value::as_object)?;

    if let Some(url) = audio
        .get("url")
        .or_else(|| audio.get("audio_url"))
        .and_then(Value::as_str)
    {
        return Some(ContentBlock::Audio {
            source: MediaSource::Url {
                url: url.to_string(),
            },
        });
    }

    if let Some(data) = audio
        .get("data")
        .or_else(|| audio.get("audio_data"))
        .and_then(Value::as_str)
    {
        let mime_type = audio
            .get("mime_type")
            .or_else(|| audio.get("format"))
            .and_then(Value::as_str)
            .unwrap_or("audio/wav");
        return Some(ContentBlock::Audio {
            source: MediaSource::Base64 {
                mime_type: mime_type.to_string(),
                data: data.to_string(),
            },
        });
    }

    None
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsesResponse {
    status: Option<String>,
    output: Option<Vec<OpenAiResponsesOutputItem>>,
    output_text: Option<String>,
    usage: Option<OpenAiResponsesUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsesOutputItem {
    #[serde(rename = "type")]
    item_type: Option<String>,
    id: Option<String>,
    name: Option<String>,
    call_id: Option<String>,
    arguments: Option<String>,
    content: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponsesUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    input_tokens_details: Option<OpenAiInputTokenDetails>,
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
    prompt_tokens_details: Option<OpenAiInputTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct OpenAiInputTokenDetails {
    cached_tokens: Option<u64>,
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
        apply_stream_data, build_chat_request_body, build_responses_request_body,
        finalize_stream_response, parse_chat_response, parse_responses_api_response,
    };
    use crate::{
        ChatRequest, ContentBlock, Message, MessageRole, PromptCacheConfig, ToolChoice,
        ToolDefinition,
    };

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
            tool_choice: Some(ToolChoice::Required),
            json_mode: true,
            max_tokens: Some(512),
            temperature: Some(0.0),
            prompt_cache: Default::default(),
        };

        let body = build_chat_request_body(&request).expect("request body must serialize");
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["name"],
            "read"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["tools"][0]["function"]["name"], "read");
        assert_eq!(body["tool_choice"], json!("required"));
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn functional_serializes_named_tool_choice() {
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read a file".to_string(),
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

        let body = build_chat_request_body(&request).expect("request body must serialize");
        assert_eq!(body["tool_choice"]["type"], "function");
        assert_eq!(body["tool_choice"]["function"]["name"], "read");
    }

    #[test]
    fn spec_c01_openai_serializes_prompt_cache_key_when_enabled() {
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: PromptCacheConfig {
                enabled: true,
                cache_key: Some("session:turn-prefix".to_string()),
                retention: None,
                google_cached_content: None,
            },
        };

        let body = build_chat_request_body(&request).expect("request body must serialize");
        assert_eq!(body["prompt_cache_key"], "session:turn-prefix");
    }

    #[test]
    fn regression_omits_non_none_tool_choice_when_tools_are_absent() {
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: Some(ToolChoice::Auto),
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: Default::default(),
        };

        let body = build_chat_request_body(&request).expect("request body must serialize");
        assert!(body.get("tool_choice").is_none());
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
    fn spec_c04_openai_parses_cached_prompt_tokens_from_usage() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "content": "cached reply",
                    "tool_calls": null
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 120,
                "completion_tokens": 12,
                "total_tokens": 132,
                "prompt_tokens_details": {
                    "cached_tokens": 96
                }
            }
        }"#;

        let response = parse_chat_response(raw).expect("response must parse");
        assert_eq!(response.usage.input_tokens, 120);
        assert_eq!(response.usage.cached_input_tokens, 96);
        assert_eq!(response.usage.total_tokens, 132);
    }

    #[test]
    fn unit_serializes_user_multimodal_parts_for_openai() {
        let multimodal_message = Message {
            role: MessageRole::User,
            content: vec![
                ContentBlock::text("Inspect this"),
                ContentBlock::image_url("https://example.com/cat.png"),
                ContentBlock::audio_base64("audio/wav", "UklGRiQAAABXQVZF"),
            ],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        };
        let request = ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![multimodal_message],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: None,
            temperature: None,
            prompt_cache: Default::default(),
        };

        let body = build_chat_request_body(&request).expect("request body must serialize");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][0]["content"][1]["type"], "image_url");
        assert_eq!(
            body["messages"][0]["content"][1]["image_url"]["url"],
            "https://example.com/cat.png"
        );
        assert_eq!(body["messages"][0]["content"][2]["type"], "text");
        assert!(body["messages"][0]["content"][2]["text"]
            .as_str()
            .expect("audio fallback marker should be a string")
            .contains("[tau-audio:"));
    }

    #[test]
    fn functional_parses_multimodal_content_blocks_from_response() {
        let raw = r#"{
            "choices": [{
                "message": {
                    "content": [
                        {"type":"text","text":"caption"},
                        {"type":"image_url","image_url":{"url":"https://example.com/cat.png"}},
                        {"type":"input_audio","input_audio":{"format":"audio/mpeg","data":"QUJDREVGRw=="}}
                    ],
                    "tool_calls": null
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 4,
                "total_tokens": 14
            }
        }"#;

        let response = parse_chat_response(raw).expect("response must parse");
        assert_eq!(response.message.text_content(), "caption");
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

    #[test]
    fn unit_parses_responses_api_text_and_function_call_output() {
        let raw = r#"{
            "status": "completed",
            "output": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type":"output_text","text":"call tool"}]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file",
                    "arguments": "{\"path\":\"README.md\"}"
                }
            ],
            "usage": {
                "input_tokens": 9,
                "output_tokens": 4,
                "total_tokens": 13
            }
        }"#;

        let response = parse_responses_api_response(raw).expect("responses payload should parse");
        assert_eq!(response.message.text_content(), "call tool");
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(response.message.tool_calls()[0].name, "read_file");
        assert_eq!(
            response.message.tool_calls()[0].arguments,
            json!({"path":"README.md"})
        );
        assert_eq!(response.finish_reason.as_deref(), Some("completed"));
        assert_eq!(response.usage.total_tokens, 13);
    }

    #[test]
    fn conformance_builds_responses_body_with_function_call_output_items() {
        let request = ChatRequest {
            model: "gpt-5.2-codex".to_string(),
            messages: vec![
                Message::system("You are helpful"),
                Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call_1".to_string(),
                    name: "read_file".to_string(),
                    arguments: json!({"path":"README.md"}),
                }]),
                Message::tool_result("call_1", "read_file", "contents", false),
            ],
            tools: vec![ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a file".to_string(),
                parameters: json!({"type":"object"}),
            }],
            tool_choice: Some(ToolChoice::Auto),
            json_mode: false,
            max_tokens: Some(64),
            temperature: Some(0.0),
            prompt_cache: Default::default(),
        };

        let body =
            build_responses_request_body(&request).expect("responses request body must serialize");
        assert_eq!(body["model"], "gpt-5.2-codex");
        assert_eq!(body["input"][0]["role"], "system");
        assert_eq!(body["input"][1]["type"], "function_call");
        assert_eq!(body["input"][1]["name"], "read_file");
        assert_eq!(body["input"][2]["type"], "function_call_output");
        assert_eq!(body["input"][2]["call_id"], "call_1");
        assert_eq!(body["tools"][0]["name"], "read_file");
    }
}
