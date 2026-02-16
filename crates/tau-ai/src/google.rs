use async_trait::async_trait;
use futures_util::StreamExt;
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
/// Public struct `GoogleConfig` used across Tau components.
pub struct GoogleConfig {
    pub api_base: String,
    pub api_key: String,
    pub request_timeout_ms: u64,
    pub max_retries: usize,
    pub retry_budget_ms: u64,
    pub retry_jitter: bool,
}

#[derive(Debug, Clone)]
/// Public struct `GoogleClient` used across Tau components.
pub struct GoogleClient {
    client: reqwest::Client,
    config: GoogleConfig,
}

impl GoogleClient {
    pub fn new(config: GoogleConfig) -> Result<Self, TauAiError> {
        if config.api_key.trim().is_empty() {
            return Err(TauAiError::MissingApiKey);
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(
                config.request_timeout_ms.max(1),
            ))
            .build()?;

        Ok(Self { client, config })
    }

    fn generate_content_url(&self, model: &str) -> String {
        let base = self.config.api_base.trim_end_matches('/');
        if base.contains(":streamGenerateContent") {
            return base.replace("{model}", model);
        }
        if base.contains(":generateContent") {
            return base.replace("{model}", model);
        }

        format!("{base}/models/{model}:generateContent")
    }

    fn stream_generate_content_url(&self, model: &str) -> String {
        let base = self.config.api_base.trim_end_matches('/');
        if base.contains(":streamGenerateContent") {
            return base.replace("{model}", model);
        }
        if base.contains(":generateContent") {
            return base.replace(":generateContent", ":streamGenerateContent");
        }

        format!("{base}/models/{model}:streamGenerateContent")
    }
}

#[async_trait]
impl LlmClient for GoogleClient {
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

impl GoogleClient {
    async fn complete_with_mode(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        let body = build_generate_content_body(&request);
        let stream_mode = on_delta.is_some();
        let url = if stream_mode {
            self.stream_generate_content_url(&request.model)
        } else {
            self.generate_content_url(&request.model)
        };
        let started = std::time::Instant::now();
        let max_retries = self.config.max_retries;

        for attempt in 0..=max_retries {
            let request_id = new_request_id();
            let mut query = vec![("key", self.config.api_key.as_str())];
            if stream_mode {
                query.push(("alt", "sse"));
            }
            let response = self
                .client
                .post(&url)
                .header("x-tau-request-id", request_id)
                .header("x-tau-retry-attempt", attempt.to_string())
                .query(&query)
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
                                .get(reqwest::header::CONTENT_TYPE)
                                .and_then(|value| value.to_str().ok())
                                .map(|value| {
                                    value.to_ascii_lowercase().contains("text/event-stream")
                                })
                                .unwrap_or(false);
                            if is_event_stream {
                                return parse_generate_content_stream_response(
                                    response,
                                    delta_handler,
                                )
                                .await;
                            }

                            let raw = response.text().await?;
                            let parsed = parse_generate_content_response(&raw)?;
                            let text = parsed.message.text_content();
                            if !text.is_empty() {
                                delta_handler(text);
                            }
                            return Ok(parsed);
                        }

                        let raw = response.text().await?;
                        return parse_generate_content_response(&raw);
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

fn build_generate_content_body(request: &ChatRequest) -> Value {
    let system = extract_system_text(&request.messages);
    let contents = to_google_contents(&request.messages);

    let mut body = json!({
        "contents": contents,
    });

    if !system.is_empty() {
        body["systemInstruction"] = json!({
            "parts": [{ "text": system }],
        });
    }

    if !request.tools.is_empty() {
        body["tools"] = json!([{
            "functionDeclarations": request.tools.iter().map(to_google_function_declaration).collect::<Vec<_>>()
        }]);
        if let Some(tool_choice) = request.tool_choice.as_ref() {
            body["toolConfig"] = json!({
                "functionCallingConfig": to_google_function_calling_config(tool_choice),
            });
        }
    }

    if request.temperature.is_some() || request.max_tokens.is_some() || request.json_mode {
        let mut generation_config = json!({});
        if request.json_mode {
            generation_config["responseMimeType"] = json!("application/json");
        }
        if let Some(temperature) = request.temperature {
            generation_config["temperature"] = json!(temperature);
        }
        if let Some(max_tokens) = request.max_tokens {
            generation_config["maxOutputTokens"] = json!(max_tokens);
        }
        body["generationConfig"] = generation_config;
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

fn to_google_function_declaration(tool: &ToolDefinition) -> Value {
    let sanitized_parameters = sanitize_google_schema(&tool.parameters);
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": sanitized_parameters,
    })
}

fn sanitize_google_schema(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sanitized = serde_json::Map::new();
            for (key, nested) in map {
                if key == "additionalProperties" {
                    continue;
                }
                sanitized.insert(key.clone(), sanitize_google_schema(nested));
            }
            Value::Object(sanitized)
        }
        Value::Array(values) => Value::Array(values.iter().map(sanitize_google_schema).collect()),
        _ => value.clone(),
    }
}

fn to_google_function_calling_config(tool_choice: &ToolChoice) -> Value {
    match tool_choice {
        ToolChoice::Auto => json!({
            "mode": "AUTO",
        }),
        ToolChoice::None => json!({
            "mode": "NONE",
        }),
        ToolChoice::Required => json!({
            "mode": "ANY",
        }),
        ToolChoice::Tool { name } => json!({
            "mode": "ANY",
            "allowedFunctionNames": [name],
        }),
    }
}

fn to_google_contents(messages: &[Message]) -> Value {
    Value::Array(
        messages
            .iter()
            .filter_map(|message| match message.role {
                MessageRole::System => None,
                MessageRole::User => {
                    let parts = to_google_parts(message, false);
                    if parts.is_empty() {
                        None
                    } else {
                        Some(json!({
                            "role": "user",
                            "parts": parts,
                        }))
                    }
                }
                MessageRole::Assistant => {
                    let parts = to_google_parts(message, true);
                    if parts.is_empty() {
                        None
                    } else {
                        Some(json!({
                            "role": "model",
                            "parts": parts,
                        }))
                    }
                }
                MessageRole::Tool => {
                    let name = message
                        .tool_name
                        .as_deref()
                        .unwrap_or("unknown_tool")
                        .to_string();
                    let tool_call_id = message.tool_call_id.clone().unwrap_or_default();
                    Some(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": {
                                    "tool_call_id": tool_call_id,
                                    "content": flatten_message_with_media_markers(message),
                                    "is_error": message.is_error,
                                }
                            }
                        }]
                    }))
                }
            })
            .collect(),
    )
}

fn to_google_parts(message: &Message, allow_tool_calls: bool) -> Vec<Value> {
    let mut parts = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => {
                if !text.trim().is_empty() {
                    parts.push(json!({ "text": text }));
                }
            }
            ContentBlock::ToolCall {
                id: _,
                name,
                arguments,
            } if allow_tool_calls => {
                parts.push(json!({
                    "functionCall": {
                        "name": name,
                        "args": arguments,
                    }
                }));
            }
            ContentBlock::ToolCall { .. } => {}
            ContentBlock::Image { source } => {
                parts.push(to_google_media_part(source, "image"));
            }
            ContentBlock::Audio { source } => {
                parts.push(to_google_media_part(source, "audio"));
            }
        }
    }
    parts
}

fn to_google_media_part(source: &MediaSource, media_kind: &str) -> Value {
    match source {
        MediaSource::Url { url } => json!({
            "fileData": {
                "mimeType": default_mime_type(media_kind),
                "fileUri": url,
            }
        }),
        MediaSource::Base64 { mime_type, data } => json!({
            "inlineData": {
                "mimeType": mime_type,
                "data": data,
            }
        }),
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

fn default_mime_type(media_kind: &str) -> &'static str {
    if media_kind == "audio" {
        "audio/wav"
    } else {
        "image/png"
    }
}

fn parse_generate_content_response(raw: &str) -> Result<ChatResponse, TauAiError> {
    let parsed: GenerateContentResponse = serde_json::from_str(raw)?;
    let candidate = parsed
        .candidates
        .and_then(|mut candidates| candidates.drain(..).next())
        .ok_or_else(|| {
            TauAiError::InvalidResponse("response contained no candidates".to_string())
        })?;

    let parts = candidate
        .content
        .and_then(|content| content.parts)
        .unwrap_or_default();
    let mut blocks = Vec::new();

    for (index, part) in parts.into_iter().enumerate() {
        if let Some(text) = part.text.as_ref() {
            if !text.trim().is_empty() {
                blocks.push(ContentBlock::Text { text: text.clone() });
            }
        }

        if let Some(function_call) = part.function_call.as_ref() {
            blocks.push(ContentBlock::ToolCall {
                id: format!("google_call_{}", index + 1),
                name: function_call.name.clone(),
                arguments: function_call.args.clone().unwrap_or_else(|| json!({})),
            });
        }

        if let Some(media_block) = parse_google_media_part(&part) {
            blocks.push(media_block);
        }
    }

    let usage = parsed
        .usage_metadata
        .map(|usage| ChatUsage {
            input_tokens: usage.prompt_token_count.unwrap_or(0),
            output_tokens: usage.candidates_token_count.unwrap_or(0),
            total_tokens: usage.total_token_count.unwrap_or(0),
        })
        .unwrap_or_default();

    Ok(ChatResponse {
        message: Message::assistant_blocks(blocks),
        finish_reason: candidate.finish_reason,
        usage,
    })
}

async fn parse_generate_content_stream_response(
    response: reqwest::Response,
    on_delta: StreamDeltaHandler,
) -> Result<ChatResponse, TauAiError> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut finish_reason = None;
    let mut usage = ChatUsage::default();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let fragment = std::str::from_utf8(chunk.as_ref()).map_err(|error| {
            TauAiError::InvalidResponse(format!("invalid UTF-8 in Google stream response: {error}"))
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
                apply_google_stream_data(
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
    if let Some(data) = trailing.strip_prefix("data:") {
        apply_google_stream_data(
            data.trim(),
            &on_delta,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )?;
    }

    Ok(finalize_google_stream_response(
        text,
        tool_calls,
        finish_reason,
        usage,
    ))
}

fn apply_google_stream_data(
    data: &str,
    on_delta: &StreamDeltaHandler,
    text: &mut String,
    tool_calls: &mut Vec<ContentBlock>,
    finish_reason: &mut Option<String>,
    usage: &mut ChatUsage,
) -> Result<(), TauAiError> {
    if data.is_empty() {
        return Ok(());
    }

    let chunk: GenerateContentResponse = serde_json::from_str(data).map_err(|error| {
        TauAiError::InvalidResponse(format!("failed to parse Google stream chunk: {error}"))
    })?;
    if let Some(chunk_usage) = chunk.usage_metadata {
        usage.input_tokens = chunk_usage.prompt_token_count.unwrap_or(usage.input_tokens);
        usage.output_tokens = chunk_usage
            .candidates_token_count
            .unwrap_or(usage.output_tokens);
        usage.total_tokens = chunk_usage.total_token_count.unwrap_or(usage.total_tokens);
    }

    if let Some(candidates) = chunk.candidates {
        for candidate in candidates {
            if let Some(reason) = candidate.finish_reason {
                *finish_reason = Some(reason);
            }

            let Some(parts) = candidate.content.and_then(|content| content.parts) else {
                continue;
            };
            for part in parts {
                if let Some(delta_text) = part.text.as_ref() {
                    if !delta_text.is_empty() {
                        text.push_str(delta_text);
                        on_delta(delta_text.clone());
                    }
                }
                if let Some(function_call) = part.function_call.as_ref() {
                    tool_calls.push(ContentBlock::ToolCall {
                        id: format!("google_stream_call_{}", tool_calls.len() + 1),
                        name: function_call.name.clone(),
                        arguments: function_call.args.clone().unwrap_or_else(|| json!({})),
                    });
                }
                if let Some(media_block) = parse_google_media_part(&part) {
                    tool_calls.push(media_block);
                }
            }
        }
    }

    Ok(())
}

fn finalize_google_stream_response(
    text: String,
    tool_calls: Vec<ContentBlock>,
    finish_reason: Option<String>,
    usage: ChatUsage,
) -> ChatResponse {
    let mut blocks = Vec::new();
    if !text.trim().is_empty() {
        blocks.push(ContentBlock::Text { text });
    }
    blocks.extend(tool_calls);

    ChatResponse {
        message: Message::assistant_blocks(blocks),
        finish_reason,
        usage,
    }
}

fn parse_google_media_part(part: &GenerateContentPart) -> Option<ContentBlock> {
    if let Some(inline_data) = &part.inline_data {
        return Some(media_block_from_mime(
            &inline_data.mime_type,
            MediaSource::Base64 {
                mime_type: inline_data.mime_type.clone(),
                data: inline_data.data.clone(),
            },
        ));
    }

    if let Some(file_data) = &part.file_data {
        return Some(media_block_from_mime(
            &file_data.mime_type,
            MediaSource::Url {
                url: file_data.file_uri.clone(),
            },
        ));
    }

    None
}

fn media_block_from_mime(mime_type: &str, source: MediaSource) -> ContentBlock {
    if mime_type.to_ascii_lowercase().starts_with("audio/") {
        ContentBlock::Audio { source }
    } else {
        ContentBlock::Image { source }
    }
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    candidates: Option<Vec<GenerateContentCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GenerateContentUsage>,
}

#[derive(Debug, Deserialize)]
struct GenerateContentCandidate {
    content: Option<GenerateContentContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GenerateContentContent {
    parts: Option<Vec<GenerateContentPart>>,
}

#[derive(Debug, Deserialize)]
struct GenerateContentPart {
    text: Option<String>,
    #[serde(rename = "functionCall")]
    function_call: Option<GenerateContentFunctionCall>,
    #[serde(rename = "inlineData")]
    inline_data: Option<GenerateContentInlineData>,
    #[serde(rename = "fileData")]
    file_data: Option<GenerateContentFileData>,
}

#[derive(Debug, Deserialize)]
struct GenerateContentFunctionCall {
    name: String,
    args: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct GenerateContentInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[derive(Debug, Deserialize)]
struct GenerateContentFileData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "fileUri")]
    file_uri: String,
}

#[derive(Debug, Deserialize)]
struct GenerateContentUsage {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: Option<u64>,
    #[serde(rename = "totalTokenCount")]
    total_token_count: Option<u64>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    use super::{
        apply_google_stream_data, build_generate_content_body, finalize_google_stream_response,
        parse_generate_content_response,
    };
    use crate::{ChatRequest, ContentBlock, Message, MessageRole, ToolChoice, ToolDefinition};

    #[test]
    fn serializes_tool_calls_and_responses() {
        let request = ChatRequest {
            model: "gemini-2.5-pro".to_string(),
            messages: vec![
                Message::system("You are helpful"),
                Message::user("Read file"),
                Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call_1".to_string(),
                    name: "read".to_string(),
                    arguments: json!({ "path": "README.md" }),
                }]),
                Message::tool_result("call_1", "read", "done", false),
            ],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read file".to_string(),
                parameters: json!({"type":"object"}),
            }],
            tool_choice: Some(ToolChoice::Tool {
                name: "read".to_string(),
            }),
            json_mode: true,
            max_tokens: Some(256),
            temperature: Some(0.1),
        };

        let body = build_generate_content_body(&request);
        assert_eq!(
            body["contents"][1]["parts"][0]["functionCall"]["name"],
            "read"
        );
        assert_eq!(
            body["contents"][2]["parts"][0]["functionResponse"]["name"],
            "read"
        );
        assert_eq!(body["tools"][0]["functionDeclarations"][0]["name"], "read");
        assert_eq!(body["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
        assert_eq!(
            body["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"][0],
            "read"
        );
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
    }

    #[test]
    fn functional_google_tool_choice_none_serializes_to_mode_none() {
        let request = ChatRequest {
            model: "gemini-2.5-pro".to_string(),
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
        };

        let body = build_generate_content_body(&request);
        assert_eq!(body["toolConfig"]["functionCallingConfig"]["mode"], "NONE");
    }

    #[test]
    fn regression_google_json_mode_preserves_generation_overrides() {
        let request = ChatRequest {
            model: "gemini-2.5-pro".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: true,
            max_tokens: Some(64),
            temperature: Some(0.2),
        };

        let body = build_generate_content_body(&request);
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 64);
        let temperature = body["generationConfig"]["temperature"]
            .as_f64()
            .expect("temperature should serialize as f64");
        assert!((temperature - 0.2).abs() < 1e-6);
    }

    #[test]
    fn conformance_google_function_declaration_strips_additional_properties() {
        let request = ChatRequest {
            model: "gemini-2.5-pro".to_string(),
            messages: vec![Message::user("schema test")],
            tools: vec![ToolDefinition {
                name: "write_file".to_string(),
                description: "Writes a file".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "options": {
                            "type": "object",
                            "properties": {
                                "mode": { "type": "string" }
                            },
                            "additionalProperties": false
                        }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            }],
            tool_choice: Some(ToolChoice::Auto),
            json_mode: false,
            max_tokens: None,
            temperature: None,
        };

        let body = build_generate_content_body(&request);
        let parameters = &body["tools"][0]["functionDeclarations"][0]["parameters"];
        assert!(
            !parameters.to_string().contains("additionalProperties"),
            "google function schema should omit unsupported additionalProperties keys"
        );
        assert_eq!(parameters["type"], "object");
        assert_eq!(parameters["properties"]["path"]["type"], "string");
        assert_eq!(parameters["properties"]["options"]["type"], "object");
    }

    #[test]
    fn parses_function_call_from_response() {
        let raw = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Working"},
                        {"functionCall": {"name": "read", "args": {"path": "README.md"}}}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 8,
                "candidatesTokenCount": 4,
                "totalTokenCount": 12
            }
        }"#;

        let response = parse_generate_content_response(raw).expect("response must parse");
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(response.usage.total_tokens, 12);
    }

    #[test]
    fn unit_serializes_multimodal_parts_for_google() {
        let request = ChatRequest {
            model: "gemini-2.5-pro".to_string(),
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![
                    ContentBlock::text("describe"),
                    ContentBlock::image_base64("image/png", "aW1hZ2VEYXRh"),
                    ContentBlock::audio_url("https://example.com/audio.wav"),
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
        };

        let body = build_generate_content_body(&request);
        assert_eq!(body["contents"][0]["parts"][0]["text"], "describe");
        assert_eq!(
            body["contents"][0]["parts"][1]["inlineData"]["mimeType"],
            "image/png"
        );
        assert_eq!(
            body["contents"][0]["parts"][2]["fileData"]["fileUri"],
            "https://example.com/audio.wav"
        );
        assert_eq!(
            body["contents"][0]["parts"][2]["fileData"]["mimeType"],
            "audio/wav"
        );
    }

    #[test]
    fn functional_parses_multimodal_parts_from_google_response() {
        let raw = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Working"},
                        {"fileData": {"mimeType": "image/png", "fileUri": "https://example.com/cat.png"}},
                        {"inlineData": {"mimeType": "audio/mpeg", "data": "QUJDREVGRw=="}}
                    ]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 8,
                "candidatesTokenCount": 4,
                "totalTokenCount": 12
            }
        }"#;

        let response = parse_generate_content_response(raw).expect("response must parse");
        assert_eq!(response.message.text_content(), "Working");
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
    fn functional_google_stream_data_parses_text_and_function_calls() {
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

        apply_google_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"text":"Hel"}]}}]}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("first chunk parses");
        apply_google_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"text":"lo"},{"functionCall":{"name":"read","args":{"path":"README.md"}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":4,"totalTokenCount":9}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("second chunk parses");

        assert_eq!(text, "Hello");
        assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hello");
        assert_eq!(finish_reason.as_deref(), Some("STOP"));
        assert_eq!(usage.total_tokens, 9);
        assert_eq!(tool_calls.len(), 1);

        let response = finalize_google_stream_response(text, tool_calls, finish_reason, usage);
        assert_eq!(response.message.text_content(), "Hello");
        assert_eq!(response.message.tool_calls().len(), 1);
        assert_eq!(
            response.message.tool_calls()[0].arguments,
            json!({"path":"README.md"})
        );
    }

    #[test]
    fn regression_google_stream_data_parses_media_parts_without_error() {
        let sink: crate::StreamDeltaHandler = Arc::new(|_delta: String| {});
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;
        let mut usage = crate::ChatUsage::default();

        apply_google_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"fileData":{"mimeType":"image/png","fileUri":"https://example.com/cat.png"}}]},"finishReason":"STOP"}]}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect("media stream chunk parses");

        let response = finalize_google_stream_response(text, tool_calls, finish_reason, usage);
        assert!(response
            .message
            .content
            .iter()
            .any(|block| matches!(block, ContentBlock::Image { .. })));
    }

    #[test]
    fn regression_google_stream_data_surfaces_parse_errors() {
        let sink: crate::StreamDeltaHandler = Arc::new(|_delta: String| {});
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut finish_reason = None;
        let mut usage = crate::ChatUsage::default();

        let error = apply_google_stream_data(
            r#"{"candidates":[{"content":{"parts":[{"text":"Hel"}]}}"#,
            &sink,
            &mut text,
            &mut tool_calls,
            &mut finish_reason,
            &mut usage,
        )
        .expect_err("invalid stream payload should fail");
        assert!(error
            .to_string()
            .contains("failed to parse Google stream chunk"));
    }
}
