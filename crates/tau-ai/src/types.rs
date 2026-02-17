use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MessageRole` values.
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `ToolCall` used across Tau components.
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Enumerates supported `ContentBlock` values.
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    Image {
        source: MediaSource,
    },
    Audio {
        source: MediaSource,
    },
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn tool_call(call: ToolCall) -> Self {
        Self::ToolCall {
            id: call.id,
            name: call.name,
            arguments: call.arguments,
        }
    }

    pub fn image_url(url: impl Into<String>) -> Self {
        Self::Image {
            source: MediaSource::Url { url: url.into() },
        }
    }

    pub fn image_base64(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::Image {
            source: MediaSource::Base64 {
                mime_type: mime_type.into(),
                data: data.into(),
            },
        }
    }

    pub fn audio_url(url: impl Into<String>) -> Self {
        Self::Audio {
            source: MediaSource::Url { url: url.into() },
        }
    }

    pub fn audio_base64(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self::Audio {
            source: MediaSource::Base64 {
                mime_type: mime_type.into(),
                data: data.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `Message` used across Tau components.
pub struct Message {
    pub role: MessageRole,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Enumerates supported media source payloads.
pub enum MediaSource {
    Url { url: String },
    Base64 { mime_type: String, data: String },
}

impl Message {
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: vec![ContentBlock::Text { text: text.into() }],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.into() }],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        }
    }

    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        }
    }

    pub fn assistant_blocks(content: Vec<ContentBlock>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        text: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: MessageRole::Tool,
            content: vec![ContentBlock::Text { text: text.into() }],
            tool_call_id: Some(tool_call_id.into()),
            tool_name: Some(tool_name.into()),
            is_error,
        }
    }

    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                ContentBlock::ToolCall { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::Audio { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn tool_calls(&self) -> Vec<ToolCall> {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                }),
                ContentBlock::Text { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::Audio { .. } => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `ToolDefinition` used across Tau components.
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Enumerates supported `ToolChoice` values.
pub enum ToolChoice {
    Auto,
    None,
    Required,
    Tool { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `ChatRequest` used across Tau components.
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub json_mode: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    #[serde(default)]
    pub prompt_cache: PromptCacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Provider-agnostic prompt caching controls.
pub struct PromptCacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_cached_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
/// Public struct `ChatUsage` used across Tau components.
pub struct ChatUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub cached_input_tokens: u64,
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `ChatResponse` used across Tau components.
pub struct ChatResponse {
    pub message: Message,
    pub finish_reason: Option<String>,
    pub usage: ChatUsage,
}

#[derive(Debug, Error)]
/// Enumerates supported `TauAiError` values.
pub enum TauAiError {
    #[error("missing API key")]
    MissingApiKey,
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("provider returned non-success status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

pub type StreamDeltaHandler = Arc<dyn Fn(String) + Send + Sync>;

#[async_trait]
/// Trait contract for `LlmClient` behavior.
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError>;

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, TauAiError> {
        let _ = on_delta;
        self.complete(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentBlock, MediaSource, Message, MessageRole};

    #[test]
    fn collects_text_content() {
        let message = Message {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "first".to_string(),
                },
                ContentBlock::ToolCall {
                    id: "1".to_string(),
                    name: "read".to_string(),
                    arguments: serde_json::json!({ "path": "README.md" }),
                },
                ContentBlock::Text {
                    text: "second".to_string(),
                },
                ContentBlock::Image {
                    source: MediaSource::Url {
                        url: "https://example.com/image.png".to_string(),
                    },
                },
            ],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        };

        assert_eq!(message.text_content(), "first\nsecond");
    }

    #[test]
    fn collects_tool_calls_ignoring_non_tool_blocks() {
        let message = Message {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::text("first"),
                ContentBlock::audio_base64("audio/wav", "ZGF0YQ=="),
                ContentBlock::tool_call(super::ToolCall {
                    id: "call_1".to_string(),
                    name: "read".to_string(),
                    arguments: serde_json::json!({ "path": "README.md" }),
                }),
            ],
            tool_call_id: None,
            tool_name: None,
            is_error: false,
        };

        let calls = message.tool_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
    }
}
