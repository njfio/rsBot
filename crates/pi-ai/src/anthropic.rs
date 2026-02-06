use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
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
pub struct AnthropicConfig {
    pub api_base: String,
    pub api_key: String,
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
        let body = build_messages_request_body(&request);
        let url = self.messages_url();

        for attempt in 0..=MAX_RETRIES {
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
                    let raw = response.text().await?;
                    if status.is_success() {
                        return parse_messages_response(&raw);
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        anthropic::{build_messages_request_body, parse_messages_response},
        ChatRequest, ContentBlock, Message, ToolDefinition,
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
}
