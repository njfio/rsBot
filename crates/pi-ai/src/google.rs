use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::time::sleep;

use crate::{
    retry::{
        is_retryable_http_error, new_request_id, next_backoff_ms, should_retry_status, MAX_RETRIES,
    },
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole, PiAiError,
    ToolDefinition,
};

#[derive(Debug, Clone)]
pub struct GoogleConfig {
    pub api_base: String,
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct GoogleClient {
    client: reqwest::Client,
    config: GoogleConfig,
}

impl GoogleClient {
    pub fn new(config: GoogleConfig) -> Result<Self, PiAiError> {
        if config.api_key.trim().is_empty() {
            return Err(PiAiError::MissingApiKey);
        }

        Ok(Self {
            client: reqwest::Client::new(),
            config,
        })
    }

    fn generate_content_url(&self, model: &str) -> String {
        let base = self.config.api_base.trim_end_matches('/');
        if base.contains(":generateContent") {
            return base.replace("{model}", model);
        }

        format!("{base}/models/{model}:generateContent")
    }
}

#[async_trait]
impl LlmClient for GoogleClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, PiAiError> {
        let body = build_generate_content_body(&request);
        let url = self.generate_content_url(&request.model);

        for attempt in 0..=MAX_RETRIES {
            let request_id = new_request_id();
            let response = self
                .client
                .post(&url)
                .header("x-pi-request-id", request_id)
                .header("x-pi-retry-attempt", attempt.to_string())
                .query(&[("key", self.config.api_key.as_str())])
                .json(&body)
                .send()
                .await;

            match response {
                Ok(response) => {
                    let status = response.status();
                    let raw = response.text().await?;
                    if status.is_success() {
                        return parse_generate_content_response(&raw);
                    }

                    if attempt < MAX_RETRIES && should_retry_status(status.as_u16()) {
                        sleep(std::time::Duration::from_millis(next_backoff_ms(attempt))).await;
                        continue;
                    }

                    return Err(PiAiError::HttpStatus {
                        status: status.as_u16(),
                        body: raw,
                    });
                }
                Err(error) => {
                    if attempt < MAX_RETRIES && is_retryable_http_error(&error) {
                        sleep(std::time::Duration::from_millis(next_backoff_ms(attempt))).await;
                        continue;
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
    }

    if request.temperature.is_some() || request.max_tokens.is_some() {
        let mut generation_config = json!({});
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
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
    })
}

fn to_google_contents(messages: &[Message]) -> Value {
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
                            "parts": [{ "text": text }],
                        }))
                    }
                }
                MessageRole::Assistant => {
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
                            } => {
                                parts.push(json!({
                                    "functionCall": {
                                        "name": name,
                                        "args": arguments,
                                    }
                                }));
                            }
                        }
                    }

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
                                    "content": message.text_content(),
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

fn parse_generate_content_response(raw: &str) -> Result<ChatResponse, PiAiError> {
    let parsed: GenerateContentResponse = serde_json::from_str(raw)?;
    let candidate = parsed
        .candidates
        .and_then(|mut candidates| candidates.drain(..).next())
        .ok_or_else(|| {
            PiAiError::InvalidResponse("response contained no candidates".to_string())
        })?;

    let parts = candidate
        .content
        .and_then(|content| content.parts)
        .unwrap_or_default();
    let mut blocks = Vec::new();

    for (index, part) in parts.into_iter().enumerate() {
        if let Some(text) = part.text {
            if !text.trim().is_empty() {
                blocks.push(ContentBlock::Text { text });
            }
        }

        if let Some(function_call) = part.function_call {
            blocks.push(ContentBlock::ToolCall {
                id: format!("google_call_{}", index + 1),
                name: function_call.name,
                arguments: function_call.args.unwrap_or_else(|| json!({})),
            });
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
}

#[derive(Debug, Deserialize)]
struct GenerateContentFunctionCall {
    name: String,
    args: Option<Value>,
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

    use crate::{
        google::{build_generate_content_body, parse_generate_content_response},
        ChatRequest, ContentBlock, Message, ToolDefinition,
    };

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
}
