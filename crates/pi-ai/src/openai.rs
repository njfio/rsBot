use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
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
pub struct OpenAiConfig {
    pub api_base: String,
    pub api_key: String,
    pub organization: Option<String>,
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

        let bearer = format!("Bearer {}", config.api_key.trim());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer)
                .map_err(|e| PiAiError::InvalidResponse(format!("invalid API key header: {e}")))?,
        );

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
        let body = build_chat_request_body(&request)?;
        let url = self.chat_completions_url();

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
                        return parse_chat_response(&raw);
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{
        openai::{build_chat_request_body, parse_chat_response},
        ChatRequest, ContentBlock, Message, ToolDefinition,
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
}
