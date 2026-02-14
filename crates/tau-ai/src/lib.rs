//! Provider clients and shared AI transport types for Tau.
//!
//! Defines request/response schemas, model/provider abstractions, and retry
//! behavior used by OpenAI-, Anthropic-, and Google-compatible backends.

mod anthropic;
mod google;
mod openai;
mod provider;
mod retry;
mod types;

pub use anthropic::{AnthropicClient, AnthropicConfig};
pub use google::{GoogleClient, GoogleConfig};
pub use openai::{OpenAiAuthScheme, OpenAiClient, OpenAiConfig};
pub use provider::{ModelRef, ModelRefParseError, Provider};
pub use types::{
    ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, MediaSource, Message,
    MessageRole, StreamDeltaHandler, TauAiError, ToolCall, ToolChoice, ToolDefinition,
};
