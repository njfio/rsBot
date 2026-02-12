//! Core runtime primitives for building tool-using LLM agents in Tau.
use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use jsonschema::validator_for;
use serde_json::{json, Value};
use tau_ai::{
    ChatRequest, ChatUsage, LlmClient, Message, MessageRole, StreamDeltaHandler, TauAiError,
    ToolCall, ToolDefinition,
};
use thiserror::Error;

/// Public struct `AgentConfig` used across Tau components.
///
/// # Examples
///
/// ```
/// use tau_agent_core::AgentConfig;
///
/// let config = AgentConfig {
///     model: "openai/gpt-4o-mini".to_string(),
///     max_turns: 12,
///     ..AgentConfig::default()
/// };
///
/// assert_eq!(config.max_turns, 12);
/// ```
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub max_parallel_tool_calls: usize,
    pub max_context_messages: Option<usize>,
    pub request_max_retries: usize,
    pub request_retry_initial_backoff_ms: u64,
    pub request_retry_max_backoff_ms: u64,
    pub request_timeout_ms: Option<u64>,
    pub tool_timeout_ms: Option<u64>,
    pub max_estimated_input_tokens: Option<u32>,
    pub max_estimated_total_tokens: Option<u32>,
    pub structured_output_max_retries: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            system_prompt: "You are a helpful coding assistant.".to_string(),
            max_turns: 8,
            temperature: Some(0.0),
            max_tokens: None,
            max_parallel_tool_calls: 4,
            max_context_messages: Some(256),
            request_max_retries: 2,
            request_retry_initial_backoff_ms: 200,
            request_retry_max_backoff_ms: 2_000,
            request_timeout_ms: Some(120_000),
            tool_timeout_ms: Some(120_000),
            max_estimated_input_tokens: Some(120_000),
            max_estimated_total_tokens: None,
            structured_output_max_retries: 1,
        }
    }
}

/// Public struct `ToolExecutionResult` used across Tau components.
///
/// # Examples
///
/// ```
/// use serde_json::json;
/// use tau_agent_core::ToolExecutionResult;
///
/// let ok = ToolExecutionResult::ok(json!({ "status": "ok" }));
/// assert!(!ok.is_error);
///
/// let err = ToolExecutionResult::error(json!("boom"));
/// assert!(err.is_error);
/// assert_eq!(err.as_text(), "boom");
/// ```
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub content: Value,
    pub is_error: bool,
}

impl ToolExecutionResult {
    /// Creates a successful tool result.
    pub fn ok(content: Value) -> Self {
        Self {
            content,
            is_error: false,
        }
    }

    /// Creates a failed tool result.
    pub fn error(content: Value) -> Self {
        Self {
            content,
            is_error: true,
        }
    }

    /// Converts the payload to text for insertion into a tool message.
    pub fn as_text(&self) -> String {
        match &self.content {
            Value::String(text) => text.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
        }
    }
}

/// Trait contract for `AgentTool` behavior.
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use serde_json::{json, Value};
/// use tau_agent_core::{AgentTool, ToolExecutionResult};
/// use tau_ai::ToolDefinition;
///
/// struct EchoTool;
///
/// #[async_trait]
/// impl AgentTool for EchoTool {
///     fn definition(&self) -> ToolDefinition {
///         ToolDefinition {
///             name: "echo".to_string(),
///             description: "Echoes a message".to_string(),
///             parameters: json!({
///                 "type": "object",
///                 "properties": {
///                     "message": { "type": "string" }
///                 }
///             }),
///         }
///     }
///
///     async fn execute(&self, arguments: Value) -> ToolExecutionResult {
///         ToolExecutionResult::ok(arguments)
///     }
/// }
///
/// let definition = EchoTool.definition();
/// assert_eq!(definition.name, "echo");
/// ```
#[async_trait]
pub trait AgentTool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, arguments: Value) -> ToolExecutionResult;
}

/// Enumerates supported `AgentEvent` values.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    AgentStart,
    AgentEnd {
        new_messages: usize,
    },
    TurnStart {
        turn: usize,
    },
    TurnEnd {
        turn: usize,
        tool_results: usize,
        request_duration_ms: u64,
        usage: ChatUsage,
        finish_reason: Option<String>,
    },
    MessageAdded {
        message: Message,
    },
    ToolExecutionStart {
        tool_call_id: String,
        tool_name: String,
        arguments: Value,
    },
    ToolExecutionEnd {
        tool_call_id: String,
        tool_name: String,
        result: ToolExecutionResult,
    },
}

/// Enumerates supported `AgentError` values.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Ai(#[from] TauAiError),
    #[error("agent exceeded max turns ({0})")]
    MaxTurnsExceeded(usize),
    #[error("model request timed out after {timeout_ms}ms on attempt {attempt}")]
    RequestTimeout { timeout_ms: u64, attempt: usize },
    #[error(
        "token budget exceeded: estimated_input_tokens={estimated_input_tokens}, max_input_tokens={max_input_tokens}, estimated_total_tokens={estimated_total_tokens}, max_total_tokens={max_total_tokens}"
    )]
    TokenBudgetExceeded {
        estimated_input_tokens: u32,
        max_input_tokens: u32,
        estimated_total_tokens: u32,
        max_total_tokens: u32,
    },
    #[error("structured output error: {0}")]
    StructuredOutput(String),
}

type EventHandler = Arc<dyn Fn(&AgentEvent) + Send + Sync>;
const CONTEXT_SUMMARY_PREFIX: &str = "[Tau context summary]";
const CONTEXT_SUMMARY_MAX_CHARS: usize = 1_200;
const CONTEXT_SUMMARY_SNIPPET_MAX_CHARS: usize = 160;
const CONTEXT_SUMMARY_MAX_EXCERPTS: usize = 6;

#[derive(Clone)]
struct RegisteredTool {
    definition: ToolDefinition,
    tool: Arc<dyn AgentTool>,
}

/// Public struct `Agent` used across Tau components.
///
/// # Examples
///
/// ```
/// use async_trait::async_trait;
/// use std::sync::Arc;
/// use tau_agent_core::{Agent, AgentConfig};
/// use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
///
/// struct StaticClient;
///
/// #[async_trait]
/// impl LlmClient for StaticClient {
///     async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
///         Ok(ChatResponse {
///             message: Message::assistant_text("ready"),
///             finish_reason: Some("stop".to_string()),
///             usage: ChatUsage::default(),
///         })
///     }
/// }
///
/// let agent = Agent::new(Arc::new(StaticClient), AgentConfig::default());
/// assert_eq!(agent.messages().len(), 1);
/// ```
#[derive(Clone)]
pub struct Agent {
    client: Arc<dyn LlmClient>,
    config: AgentConfig,
    messages: Vec<Message>,
    tools: HashMap<String, RegisteredTool>,
    handlers: Vec<EventHandler>,
}

impl Agent {
    /// Creates a new [`Agent`] with an initial system message when configured.
    pub fn new(client: Arc<dyn LlmClient>, config: AgentConfig) -> Self {
        let mut messages = Vec::new();
        if !config.system_prompt.trim().is_empty() {
            messages.push(Message::system(config.system_prompt.clone()));
        }

        Self {
            client,
            config,
            messages,
            tools: HashMap::new(),
            handlers: Vec::new(),
        }
    }

    /// Adds an event subscriber that receives runtime lifecycle callbacks.
    pub fn subscribe<F>(&mut self, handler: F)
    where
        F: Fn(&AgentEvent) + Send + Sync + 'static,
    {
        self.handlers.push(Arc::new(handler));
    }

    /// Registers a tool exposed to the language model.
    pub fn register_tool<T>(&mut self, tool: T)
    where
        T: AgentTool + 'static,
    {
        let definition = tool.definition();
        self.tools.insert(
            definition.name.clone(),
            RegisteredTool {
                definition,
                tool: Arc::new(tool),
            },
        );
    }

    /// Returns the current conversation history.
    ///
    /// # Examples
    ///
    /// ```
    /// use async_trait::async_trait;
    /// use std::sync::Arc;
    /// use tau_agent_core::{Agent, AgentConfig};
    /// use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    ///
    /// struct StaticClient;
    ///
    /// #[async_trait]
    /// impl LlmClient for StaticClient {
    ///     async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
    ///         Ok(ChatResponse {
    ///             message: Message::assistant_text("ok"),
    ///             finish_reason: Some("stop".to_string()),
    ///             usage: ChatUsage::default(),
    ///         })
    ///     }
    /// }
    ///
    /// let mut agent = Agent::new(Arc::new(StaticClient), AgentConfig::default());
    /// agent.append_message(Message::user("hi"));
    /// assert_eq!(agent.messages().len(), 2);
    /// ```
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Replaces the current conversation history with the provided messages.
    pub fn replace_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
    }

    /// Appends a message to the conversation history.
    pub fn append_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Clones the agent state for independent execution.
    pub fn fork(&self) -> Self {
        self.clone()
    }

    /// Executes multiple prompts in bounded parallel batches.
    pub async fn run_parallel_prompts<I, S>(
        &self,
        prompts: I,
        max_parallel_runs: usize,
    ) -> Vec<Result<Vec<Message>, AgentError>>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let indexed_prompts = prompts
            .into_iter()
            .enumerate()
            .map(|(index, prompt)| (index, prompt.into()))
            .collect::<Vec<_>>();
        if indexed_prompts.is_empty() {
            return Vec::new();
        }

        let max_parallel_runs = max_parallel_runs.max(1);
        let mut ordered = (0..indexed_prompts.len()).map(|_| None).collect::<Vec<_>>();

        for chunk in indexed_prompts.chunks(max_parallel_runs) {
            let mut handles = Vec::with_capacity(chunk.len());
            for (index, prompt) in chunk {
                let mut cloned = self.fork();
                let prompt = prompt.clone();
                let index = *index;
                let handle = tokio::spawn(async move { cloned.prompt(prompt).await });
                handles.push((index, handle));
            }

            for (index, handle) in handles {
                let result = match handle.await {
                    Ok(result) => result,
                    Err(error) => Err(AgentError::Ai(TauAiError::InvalidResponse(format!(
                        "parallel prompt at index {index} failed: {error}"
                    )))),
                };
                ordered[index] = Some(result);
            }
        }

        ordered
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                result.unwrap_or_else(|| {
                    Err(AgentError::Ai(TauAiError::InvalidResponse(format!(
                        "parallel prompt at index {index} did not complete"
                    ))))
                })
            })
            .collect()
    }

    /// Appends a user prompt and advances the agent until completion.
    pub async fn prompt(&mut self, text: impl Into<String>) -> Result<Vec<Message>, AgentError> {
        self.prompt_with_stream(text, None).await
    }

    /// Runs a prompt and validates assistant output against a JSON schema.
    pub async fn prompt_json(
        &mut self,
        text: impl Into<String>,
        schema: &Value,
    ) -> Result<Value, AgentError> {
        let new_messages = self.prompt(text).await?;
        self.parse_structured_output_with_retry(new_messages, schema)
            .await
    }

    /// Runs a prompt while optionally streaming text deltas.
    pub async fn prompt_with_stream(
        &mut self,
        text: impl Into<String>,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<Vec<Message>, AgentError> {
        let start_index = self.messages.len();
        let user_message = Message::user(text.into());
        self.messages.push(user_message.clone());
        self.emit(AgentEvent::MessageAdded {
            message: user_message,
        });
        let result = self.run_loop(start_index, on_delta).await;
        if result.is_ok() {
            self.compact_message_history();
        }
        result
    }

    /// Continues the current turn without adding a new user message.
    pub async fn continue_turn(&mut self) -> Result<Vec<Message>, AgentError> {
        self.continue_turn_with_stream(None).await
    }

    /// Continues the turn and parses the response as schema-validated JSON.
    pub async fn continue_turn_json(&mut self, schema: &Value) -> Result<Value, AgentError> {
        let new_messages = self.continue_turn().await?;
        self.parse_structured_output_with_retry(new_messages, schema)
            .await
    }

    /// Continues the current turn while optionally streaming text deltas.
    pub async fn continue_turn_with_stream(
        &mut self,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<Vec<Message>, AgentError> {
        let start_index = self.messages.len();
        let result = self.run_loop(start_index, on_delta).await;
        if result.is_ok() {
            self.compact_message_history();
        }
        result
    }

    fn emit(&self, event: AgentEvent) {
        for handler in &self.handlers {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&event)));
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| tool.definition.clone())
            .collect()
    }

    fn enforce_token_budget(&self, request: &ChatRequest) -> Result<(), AgentError> {
        let estimate = estimate_chat_request_tokens(request);
        let max_input_tokens = self.config.max_estimated_input_tokens.unwrap_or(u32::MAX);
        let max_total_tokens = self.config.max_estimated_total_tokens.unwrap_or(u32::MAX);

        if estimate.input_tokens > max_input_tokens || estimate.total_tokens > max_total_tokens {
            return Err(AgentError::TokenBudgetExceeded {
                estimated_input_tokens: estimate.input_tokens,
                max_input_tokens,
                estimated_total_tokens: estimate.total_tokens,
                max_total_tokens,
            });
        }

        Ok(())
    }

    async fn parse_structured_output_with_retry(
        &mut self,
        mut new_messages: Vec<Message>,
        schema: &Value,
    ) -> Result<Value, AgentError> {
        let max_retries = self.config.structured_output_max_retries;
        for attempt in 0..=max_retries {
            match parse_structured_output(&new_messages, schema) {
                Ok(value) => return Ok(value),
                Err(AgentError::StructuredOutput(error)) => {
                    if attempt >= max_retries {
                        return Err(AgentError::StructuredOutput(error));
                    }
                    let retry_prompt = build_structured_output_retry_prompt(schema, &error);
                    new_messages = self.prompt(retry_prompt).await?;
                }
                Err(other) => return Err(other),
            }
        }
        Err(AgentError::StructuredOutput(
            "structured output retry loop exhausted unexpectedly".to_string(),
        ))
    }

    async fn run_loop(
        &mut self,
        start_index: usize,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<Vec<Message>, AgentError> {
        self.emit(AgentEvent::AgentStart);

        for turn in 1..=self.config.max_turns {
            self.emit(AgentEvent::TurnStart { turn });

            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: self.request_messages(),
                tools: self.tool_definitions(),
                max_tokens: self.config.max_tokens,
                temperature: self.config.temperature,
            };
            self.enforce_token_budget(&request)?;

            let request_started = std::time::Instant::now();
            let response = self.complete_with_retry(request, on_delta.clone()).await?;
            let request_duration_ms = request_started.elapsed().as_millis() as u64;
            let finish_reason = response.finish_reason.clone();
            let usage = response.usage.clone();
            let assistant = response.message;
            self.messages.push(assistant.clone());
            self.emit(AgentEvent::MessageAdded {
                message: assistant.clone(),
            });

            let tool_calls = assistant.tool_calls();
            if tool_calls.is_empty() {
                self.emit(AgentEvent::TurnEnd {
                    turn,
                    tool_results: 0,
                    request_duration_ms,
                    usage,
                    finish_reason,
                });
                let new_messages = self.messages[start_index..].to_vec();
                self.emit(AgentEvent::AgentEnd {
                    new_messages: new_messages.len(),
                });
                return Ok(new_messages);
            }

            self.execute_tool_calls(tool_calls).await;

            self.emit(AgentEvent::TurnEnd {
                turn,
                tool_results: self
                    .messages
                    .iter()
                    .rev()
                    .take_while(|message| message.role == tau_ai::MessageRole::Tool)
                    .count(),
                request_duration_ms,
                usage,
                finish_reason,
            });
        }

        Err(AgentError::MaxTurnsExceeded(self.config.max_turns))
    }

    fn request_messages(&self) -> Vec<Message> {
        let Some(limit) = self.config.max_context_messages else {
            return self.messages.clone();
        };
        bounded_messages(&self.messages, limit)
    }

    fn compact_message_history(&mut self) {
        let Some(limit) = self.config.max_context_messages else {
            return;
        };
        if self.messages.len() <= limit {
            return;
        }
        self.messages = bounded_messages(&self.messages, limit);
    }

    async fn complete_with_retry(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<tau_ai::ChatResponse, AgentError> {
        let max_retries = self.config.request_max_retries;
        let mut attempt = 0usize;
        let mut backoff_ms = self.config.request_retry_initial_backoff_ms.max(1);
        let max_backoff_ms = self.config.request_retry_max_backoff_ms.max(backoff_ms);
        let request_timeout = timeout_duration_from_ms(self.config.request_timeout_ms);

        loop {
            let request_for_attempt = request.clone();
            let response_result = if let Some(timeout) = request_timeout {
                match tokio::time::timeout(
                    timeout,
                    self.client
                        .complete_with_stream(request_for_attempt, on_delta.clone()),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => {
                        let can_retry = attempt < max_retries && on_delta.is_none();
                        if !can_retry {
                            return Err(AgentError::RequestTimeout {
                                timeout_ms: timeout.as_millis() as u64,
                                attempt: attempt.saturating_add(1),
                            });
                        }
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                        backoff_ms = backoff_ms.saturating_mul(2).min(max_backoff_ms);
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                }
            } else {
                self.client
                    .complete_with_stream(request_for_attempt, on_delta.clone())
                    .await
            };

            match response_result {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let can_retry = attempt < max_retries
                        && on_delta.is_none()
                        && is_retryable_ai_error(&error);
                    if !can_retry {
                        return Err(AgentError::Ai(error));
                    }

                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms = backoff_ms.saturating_mul(2).min(max_backoff_ms);
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    async fn execute_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        let max_parallel = self.config.max_parallel_tool_calls.max(1);
        if max_parallel == 1 || tool_calls.len() <= 1 {
            for call in tool_calls {
                self.execute_tool_call(call).await;
            }
            return;
        }

        for chunk in tool_calls.chunks(max_parallel) {
            let mut handles = Vec::with_capacity(chunk.len());
            for call in chunk.iter().cloned() {
                self.emit(AgentEvent::ToolExecutionStart {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                let handle = self.spawn_tool_call_task(call.clone());
                handles.push((call, handle));
            }

            for (call, handle) in handles {
                let result = match handle.await {
                    Ok(result) => result,
                    Err(error) => ToolExecutionResult::error(json!({
                        "error": format!("tool '{}' execution task failed: {error}", call.name)
                    })),
                };
                self.record_tool_result(call, result);
            }
        }
    }

    fn spawn_tool_call_task(&self, call: ToolCall) -> tokio::task::JoinHandle<ToolExecutionResult> {
        let registered = self
            .tools
            .get(&call.name)
            .map(|tool| (tool.definition.clone(), Arc::clone(&tool.tool)));
        let tool_timeout = timeout_duration_from_ms(self.config.tool_timeout_ms);
        tokio::spawn(async move { execute_tool_call_inner(call, registered, tool_timeout).await })
    }

    async fn execute_tool_call(&mut self, call: ToolCall) {
        self.emit(AgentEvent::ToolExecutionStart {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        });

        let result = match self.spawn_tool_call_task(call.clone()).await {
            Ok(result) => result,
            Err(error) => ToolExecutionResult::error(json!({
                "error": format!("tool '{}' execution task failed: {error}", call.name)
            })),
        };
        self.record_tool_result(call, result);
    }

    fn record_tool_result(&mut self, call: ToolCall, result: ToolExecutionResult) {
        self.emit(AgentEvent::ToolExecutionEnd {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            result: result.clone(),
        });

        let tool_message =
            Message::tool_result(call.id, call.name, result.as_text(), result.is_error);
        self.messages.push(tool_message.clone());
        self.emit(AgentEvent::MessageAdded {
            message: tool_message,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChatRequestTokenEstimate {
    input_tokens: u32,
    total_tokens: u32,
}

fn estimate_chat_request_tokens(request: &ChatRequest) -> ChatRequestTokenEstimate {
    let message_tokens = request.messages.iter().fold(0u32, |acc, message| {
        acc.saturating_add(estimate_message_tokens(message))
    });
    let tool_tokens = request.tools.iter().fold(0u32, |acc, tool| {
        acc.saturating_add(estimate_tool_definition_tokens(tool))
    });
    let input_tokens = message_tokens.saturating_add(tool_tokens).saturating_add(2);
    let total_tokens = input_tokens.saturating_add(request.max_tokens.unwrap_or(0));

    ChatRequestTokenEstimate {
        input_tokens,
        total_tokens,
    }
}

fn estimate_message_tokens(message: &Message) -> u32 {
    let mut total = 4u32;
    for block in &message.content {
        match block {
            tau_ai::ContentBlock::Text { text } => {
                total = total.saturating_add(estimate_text_tokens(text));
            }
            tau_ai::ContentBlock::ToolCall {
                id,
                name,
                arguments,
            } => {
                total = total.saturating_add(estimate_text_tokens(id));
                total = total.saturating_add(estimate_text_tokens(name));
                total = total.saturating_add(estimate_json_tokens(arguments));
                total = total.saturating_add(4);
            }
        }
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        total = total.saturating_add(estimate_text_tokens(tool_call_id));
    }
    if let Some(tool_name) = &message.tool_name {
        total = total.saturating_add(estimate_text_tokens(tool_name));
    }
    total
}

fn estimate_tool_definition_tokens(definition: &ToolDefinition) -> u32 {
    let mut total = 12u32;
    total = total.saturating_add(estimate_text_tokens(&definition.name));
    total = total.saturating_add(estimate_text_tokens(&definition.description));
    total = total.saturating_add(estimate_json_tokens(&definition.parameters));
    total
}

fn estimate_json_tokens(value: &Value) -> u32 {
    let rendered = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    estimate_text_tokens(&rendered)
}

fn estimate_text_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    chars.saturating_add(3) / 4
}

fn bounded_messages(messages: &[Message], max_messages: usize) -> Vec<Message> {
    if max_messages == 0 || messages.len() <= max_messages {
        return messages.to_vec();
    }

    if max_messages < 3 {
        return bounded_messages_without_summary(messages, max_messages);
    }

    if matches!(
        messages.first().map(|message| message.role),
        Some(MessageRole::System)
    ) {
        let tail_keep = max_messages - 2;
        let tail_start = messages.len().saturating_sub(tail_keep);
        if tail_start <= 1 {
            return messages.to_vec();
        }

        let dropped = &messages[1..tail_start];
        if dropped.is_empty() {
            return bounded_messages_without_summary(messages, max_messages);
        }

        let mut bounded = Vec::with_capacity(max_messages);
        bounded.push(messages[0].clone());
        bounded.push(Message::system(summarize_dropped_messages(dropped)));
        bounded.extend_from_slice(&messages[tail_start..]);
        bounded
    } else {
        let tail_keep = max_messages - 1;
        let tail_start = messages.len().saturating_sub(tail_keep);
        if tail_start == 0 {
            return messages.to_vec();
        }

        let dropped = &messages[..tail_start];
        if dropped.is_empty() {
            return bounded_messages_without_summary(messages, max_messages);
        }

        let mut bounded = Vec::with_capacity(max_messages);
        bounded.push(Message::system(summarize_dropped_messages(dropped)));
        bounded.extend_from_slice(&messages[tail_start..]);
        bounded
    }
}

fn bounded_messages_without_summary(messages: &[Message], max_messages: usize) -> Vec<Message> {
    if max_messages == 0 || messages.len() <= max_messages {
        return messages.to_vec();
    }

    if max_messages > 1
        && matches!(
            messages.first().map(|message| message.role),
            Some(MessageRole::System)
        )
    {
        let tail_keep = max_messages - 1;
        let tail_start = messages.len().saturating_sub(tail_keep);
        if tail_start <= 1 {
            return messages.to_vec();
        }
        let mut bounded = Vec::with_capacity(max_messages);
        bounded.push(messages[0].clone());
        bounded.extend_from_slice(&messages[tail_start..]);
        bounded
    } else {
        messages[messages.len() - max_messages..].to_vec()
    }
}

fn summarize_dropped_messages(messages: &[Message]) -> String {
    let mut user_count = 0usize;
    let mut assistant_count = 0usize;
    let mut tool_count = 0usize;
    let mut system_count = 0usize;
    let mut excerpts = Vec::new();

    for message in messages {
        match message.role {
            MessageRole::User => user_count = user_count.saturating_add(1),
            MessageRole::Assistant => assistant_count = assistant_count.saturating_add(1),
            MessageRole::Tool => tool_count = tool_count.saturating_add(1),
            MessageRole::System => system_count = system_count.saturating_add(1),
        }

        let content = collapse_whitespace(&message.text_content());
        if content.is_empty() {
            continue;
        }
        if message.role == MessageRole::System && content.starts_with(CONTEXT_SUMMARY_PREFIX) {
            continue;
        }
        if excerpts.len() >= CONTEXT_SUMMARY_MAX_EXCERPTS {
            continue;
        }

        let preview = truncate_chars(&content, CONTEXT_SUMMARY_SNIPPET_MAX_CHARS);
        excerpts.push(format!("- {}: {}", role_label(message.role), preview));
    }

    let mut summary = format!(
        "{CONTEXT_SUMMARY_PREFIX}\n\
         summarized_messages={}; roles: user={}, assistant={}, tool={}, system={}.",
        messages.len(),
        user_count,
        assistant_count,
        tool_count,
        system_count
    );

    if !excerpts.is_empty() {
        let excerpt_block = excerpts.join("\n");
        summary.push_str("\nexcerpts:\n");
        summary.push_str(&excerpt_block);
    }

    truncate_chars(&summary, CONTEXT_SUMMARY_MAX_CHARS)
}

fn role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
        MessageRole::System => "system",
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let truncate_at = text
        .char_indices()
        .nth(max_chars - 1)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let mut truncated = text[..truncate_at].to_string();
    truncated.push('…');
    truncated
}

fn build_structured_output_retry_prompt(schema: &Value, error: &str) -> String {
    let schema_text = serde_json::to_string(schema).unwrap_or_else(|_| schema.to_string());
    format!(
        "Your previous response could not be accepted as structured JSON ({error}). \
Please reply with only valid JSON that matches this schema exactly:\n{schema_text}"
    )
}

fn parse_structured_output(messages: &[Message], schema: &Value) -> Result<Value, AgentError> {
    let assistant = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .ok_or_else(|| {
            AgentError::StructuredOutput(
                "assistant response missing for structured output".to_string(),
            )
        })?;
    let content = assistant.text_content();
    let value = extract_json_payload(&content).map_err(AgentError::StructuredOutput)?;
    validate_json_against_schema(schema, &value).map_err(AgentError::StructuredOutput)?;
    Ok(value)
}

fn extract_json_payload(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("assistant response was empty; expected JSON output".to_string());
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    let mut cursor = 0usize;
    while let Some(open_rel) = text[cursor..].find("```") {
        let open = cursor + open_rel;
        let after_open = &text[open + 3..];
        let header_end_rel = after_open.find('\n').unwrap_or(after_open.len());
        let header = after_open[..header_end_rel].trim();
        let block_start = if header_end_rel < after_open.len() {
            open + 3 + header_end_rel + 1
        } else {
            open + 3 + header_end_rel
        };
        let Some(close_rel) = text[block_start..].find("```") else {
            break;
        };
        let close = block_start + close_rel;
        cursor = close + 3;

        if !(header.is_empty() || header.eq_ignore_ascii_case("json")) {
            continue;
        }

        let block = text[block_start..close].trim();
        if block.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(block) {
            return Ok(value);
        }
    }

    Err("assistant response did not contain parseable JSON content".to_string())
}

fn validate_json_against_schema(schema: &Value, payload: &Value) -> Result<(), String> {
    let validator = validator_for(schema)
        .map_err(|error| format!("invalid structured output schema: {error}"))?;
    let mut errors = validator.iter_errors(payload);
    if let Some(first) = errors.next() {
        return Err(format!(
            "structured output schema validation failed: {first}"
        ));
    }
    Ok(())
}

async fn execute_tool_call_inner(
    call: ToolCall,
    registered: Option<(ToolDefinition, Arc<dyn AgentTool>)>,
    tool_timeout: Option<Duration>,
) -> ToolExecutionResult {
    if let Some((definition, tool)) = registered {
        if let Err(error) = validate_tool_arguments(&definition, &call.arguments) {
            return ToolExecutionResult::error(json!({ "error": error }));
        }
        if let Some(timeout) = tool_timeout {
            match tokio::time::timeout(timeout, tool.execute(call.arguments)).await {
                Ok(result) => result,
                Err(_) => ToolExecutionResult::error(json!({
                    "error": format!(
                        "tool '{}' timed out after {}ms",
                        definition.name,
                        timeout.as_millis()
                    )
                })),
            }
        } else {
            tool.execute(call.arguments).await
        }
    } else {
        ToolExecutionResult::error(json!({
            "error": format!("Tool '{}' is not registered", call.name)
        }))
    }
}

fn timeout_duration_from_ms(timeout_ms: Option<u64>) -> Option<Duration> {
    timeout_ms
        .filter(|timeout_ms| *timeout_ms > 0)
        .map(Duration::from_millis)
}

fn is_retryable_ai_error(error: &TauAiError) -> bool {
    match error {
        TauAiError::Http(http) => http.is_timeout() || http.is_connect(),
        TauAiError::HttpStatus { status, .. } => {
            *status == 408 || *status == 409 || *status == 425 || *status == 429 || *status >= 500
        }
        TauAiError::MissingApiKey | TauAiError::Serde(_) | TauAiError::InvalidResponse(_) => false,
    }
}

fn validate_tool_arguments(definition: &ToolDefinition, arguments: &Value) -> Result<(), String> {
    let validator = validator_for(&definition.parameters)
        .map_err(|error| format!("invalid JSON schema for '{}': {error}", definition.name))?;

    let mut errors = validator.iter_errors(arguments);
    if let Some(first) = errors.next() {
        return Err(format!(
            "invalid arguments for '{}': {}",
            definition.name, first
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::{Duration, Instant},
    };

    use async_trait::async_trait;
    use tau_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, Message, MessageRole, ToolDefinition,
    };
    use tokio::sync::Mutex as AsyncMutex;

    use crate::{
        bounded_messages, build_structured_output_retry_prompt, estimate_chat_request_tokens,
        extract_json_payload, truncate_chars, Agent, AgentConfig, AgentError, AgentEvent,
        AgentTool, ToolExecutionResult, CONTEXT_SUMMARY_MAX_CHARS, CONTEXT_SUMMARY_PREFIX,
    };

    struct MockClient {
        responses: AsyncMutex<VecDeque<ChatResponse>>,
    }

    #[async_trait]
    impl tau_ai::LlmClient for MockClient {
        async fn complete(
            &self,
            _request: ChatRequest,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            let mut responses = self.responses.lock().await;
            responses.pop_front().ok_or_else(|| {
                tau_ai::TauAiError::InvalidResponse("mock response queue is empty".to_string())
            })
        }
    }

    struct StreamingMockClient {
        response: ChatResponse,
        deltas: Vec<String>,
    }

    #[async_trait]
    impl tau_ai::LlmClient for StreamingMockClient {
        async fn complete(
            &self,
            _request: ChatRequest,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            Ok(self.response.clone())
        }

        async fn complete_with_stream(
            &self,
            _request: ChatRequest,
            on_delta: Option<tau_ai::StreamDeltaHandler>,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            if let Some(handler) = on_delta {
                for delta in &self.deltas {
                    handler(delta.clone());
                }
            }
            Ok(self.response.clone())
        }
    }

    struct CapturingMockClient {
        responses: AsyncMutex<VecDeque<ChatResponse>>,
        requests: AsyncMutex<Vec<ChatRequest>>,
    }

    #[async_trait]
    impl tau_ai::LlmClient for CapturingMockClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.requests.lock().await.push(request);
            let mut responses = self.responses.lock().await;
            responses.pop_front().ok_or_else(|| {
                tau_ai::TauAiError::InvalidResponse("mock response queue is empty".to_string())
            })
        }
    }

    struct RetryThenSuccessClient {
        remaining_failures: AsyncMutex<usize>,
        attempts: AsyncMutex<usize>,
        response: ChatResponse,
    }

    #[async_trait]
    impl tau_ai::LlmClient for RetryThenSuccessClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.complete_with_stream(request, None).await
        }

        async fn complete_with_stream(
            &self,
            _request: ChatRequest,
            _on_delta: Option<tau_ai::StreamDeltaHandler>,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            {
                let mut attempts = self.attempts.lock().await;
                *attempts = attempts.saturating_add(1);
            }
            let mut remaining = self.remaining_failures.lock().await;
            if *remaining > 0 {
                *remaining = remaining.saturating_sub(1);
                return Err(tau_ai::TauAiError::HttpStatus {
                    status: 503,
                    body: "service unavailable".to_string(),
                });
            }
            Ok(self.response.clone())
        }
    }

    struct TimeoutThenSuccessClient {
        delays_ms: AsyncMutex<VecDeque<u64>>,
        attempts: AsyncMutex<usize>,
        response: ChatResponse,
    }

    #[async_trait]
    impl tau_ai::LlmClient for TimeoutThenSuccessClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.complete_with_stream(request, None).await
        }

        async fn complete_with_stream(
            &self,
            _request: ChatRequest,
            _on_delta: Option<tau_ai::StreamDeltaHandler>,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            {
                let mut attempts = self.attempts.lock().await;
                *attempts = attempts.saturating_add(1);
            }
            let delay_ms = {
                let mut delays = self.delays_ms.lock().await;
                delays.pop_front().unwrap_or(0)
            };
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
            Ok(self.response.clone())
        }
    }

    struct EchoClient;

    #[async_trait]
    impl tau_ai::LlmClient for EchoClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            let prompt = last_user_prompt(&request);
            Ok(ChatResponse {
                message: Message::assistant_text(format!("echo:{prompt}")),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct DelayedEchoClient {
        delay_ms: u64,
    }

    #[async_trait]
    impl tau_ai::LlmClient for DelayedEchoClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            let prompt = last_user_prompt(&request);
            Ok(ChatResponse {
                message: Message::assistant_text(format!("echo:{prompt}")),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct SelectiveFailureEchoClient;

    #[async_trait]
    impl tau_ai::LlmClient for SelectiveFailureEchoClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            let prompt = last_user_prompt(&request);
            if prompt.contains("fail") {
                return Err(tau_ai::TauAiError::HttpStatus {
                    status: 503,
                    body: "forced failure".to_string(),
                });
            }
            Ok(ChatResponse {
                message: Message::assistant_text(format!("echo:{prompt}")),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    fn last_user_prompt(request: &ChatRequest) -> String {
        request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.text_content().to_string())
            .unwrap_or_default()
    }

    struct ReadTool;

    #[async_trait]
    impl AgentTool for ReadTool {
        fn definition(&self) -> tau_ai::ToolDefinition {
            tau_ai::ToolDefinition {
                name: "read".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }
        }

        async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
            let path = arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>");
            ToolExecutionResult::ok(serde_json::json!({ "content": format!("read:{path}") }))
        }
    }

    struct SlowReadTool {
        delay_ms: u64,
    }

    #[async_trait]
    impl AgentTool for SlowReadTool {
        fn definition(&self) -> tau_ai::ToolDefinition {
            tau_ai::ToolDefinition {
                name: "slow_read".to_string(),
                description: "Read with delay".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }
        }

        async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            let path = arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>");
            ToolExecutionResult::ok(serde_json::json!({ "content": format!("read:{path}") }))
        }
    }

    struct PanicTool;

    #[async_trait]
    impl AgentTool for PanicTool {
        fn definition(&self) -> tau_ai::ToolDefinition {
            tau_ai::ToolDefinition {
                name: "panic_tool".to_string(),
                description: "Always panics".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }),
            }
        }

        async fn execute(&self, _arguments: serde_json::Value) -> ToolExecutionResult {
            panic!("forced panic in tool");
        }
    }

    #[test]
    fn unit_agent_config_defaults_include_request_and_tool_timeouts() {
        let config = AgentConfig::default();
        assert_eq!(config.request_timeout_ms, Some(120_000));
        assert_eq!(config.tool_timeout_ms, Some(120_000));
        assert_eq!(config.max_estimated_input_tokens, Some(120_000));
        assert_eq!(config.max_estimated_total_tokens, None);
        assert_eq!(config.structured_output_max_retries, 1);
    }

    #[test]
    fn unit_estimate_chat_request_tokens_accounts_for_tools_and_max_tokens() {
        let request = ChatRequest {
            model: "openai/gpt-4o-mini".to_string(),
            messages: vec![
                Message::system("sys"),
                Message::user("hello world"),
                Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call-1".to_string(),
                    name: "read".to_string(),
                    arguments: serde_json::json!({ "path": "README.md" }),
                }]),
            ],
            tools: vec![ToolDefinition {
                name: "read".to_string(),
                description: "Read file contents".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }),
            }],
            max_tokens: Some(64),
            temperature: Some(0.0),
        };

        let estimate = estimate_chat_request_tokens(&request);
        assert!(estimate.input_tokens > 0);
        assert_eq!(
            estimate.total_tokens,
            estimate.input_tokens.saturating_add(64)
        );
    }

    #[tokio::test]
    async fn prompt_without_tools_completes_in_one_turn() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("Hello from model"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        let new_messages = agent.prompt("hi").await.expect("prompt should succeed");

        assert_eq!(new_messages.len(), 2);
        assert_eq!(new_messages[0].role, MessageRole::User);
        assert_eq!(new_messages[1].text_content(), "Hello from model");
    }

    #[tokio::test]
    async fn prompt_executes_tool_calls_and_continues() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);

        let second_assistant = Message::assistant_text("Done reading file");

        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(ReadTool);

        let new_messages = agent
            .prompt("Read README.md")
            .await
            .expect("prompt should succeed");

        assert_eq!(new_messages.len(), 4);
        assert_eq!(new_messages[0].role, MessageRole::User);
        assert_eq!(new_messages[1].role, MessageRole::Assistant);
        assert_eq!(new_messages[2].role, MessageRole::Tool);
        assert!(new_messages[2].text_content().contains("read:README.md"));
        assert_eq!(new_messages[3].text_content(), "Done reading file");
    }

    #[tokio::test]
    async fn emits_expected_event_sequence_for_tool_turn() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let second_assistant = Message::assistant_text("done");

        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(ReadTool);

        let events = Arc::new(Mutex::new(Vec::<String>::new()));
        let recorded = events.clone();
        agent.subscribe(move |event| {
            let label = match event {
                AgentEvent::MessageAdded { message } => format!("message:{:?}", message.role),
                AgentEvent::ToolExecutionStart { tool_name, .. } => {
                    format!("tool_start:{tool_name}")
                }
                AgentEvent::ToolExecutionEnd { tool_name, .. } => format!("tool_end:{tool_name}"),
                AgentEvent::TurnStart { turn } => format!("turn_start:{turn}"),
                AgentEvent::TurnEnd { turn, .. } => format!("turn_end:{turn}"),
                AgentEvent::AgentStart => "agent_start".to_string(),
                AgentEvent::AgentEnd { .. } => "agent_end".to_string(),
            };

            recorded
                .lock()
                .expect("event mutex should lock")
                .push(label);
        });

        let _ = agent.prompt("read").await.expect("prompt should succeed");

        let events = events.lock().expect("event mutex should lock").clone();
        assert_eq!(
            events,
            vec![
                "message:User",
                "agent_start",
                "turn_start:1",
                "message:Assistant",
                "tool_start:read",
                "tool_end:read",
                "message:Tool",
                "turn_end:1",
                "turn_start:2",
                "message:Assistant",
                "turn_end:2",
                "agent_end",
            ]
        );
    }

    #[test]
    fn unit_emit_isolates_panicking_handler_and_invokes_remaining_subscribers() {
        let mut agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
        let observed = Arc::new(AtomicUsize::new(0));

        agent.subscribe(|event| {
            if matches!(event, AgentEvent::AgentStart) {
                panic!("forced event handler panic");
            }
        });
        let observed_clone = observed.clone();
        agent.subscribe(move |_event| {
            observed_clone.fetch_add(1, Ordering::Relaxed);
        });

        agent.emit(AgentEvent::AgentStart);
        assert_eq!(observed.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn functional_prompt_completes_when_event_handler_panics() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        agent.subscribe(|event| {
            if matches!(event, AgentEvent::AgentStart) {
                panic!("panic in functional handler");
            }
        });

        let messages = agent
            .prompt("hello")
            .await
            .expect("panic in event handler should not abort prompt");
        assert_eq!(messages.last().expect("assistant").text_content(), "ok");
    }

    #[tokio::test]
    async fn integration_tool_turn_completes_when_handler_panics_on_tool_events() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let second_assistant = Message::assistant_text("done");
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(ReadTool);
        agent.subscribe(|event| {
            if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
                panic!("panic on tool start");
            }
        });

        let messages = agent
            .prompt("read")
            .await
            .expect("tool turn should survive panicking handler");
        assert!(
            messages
                .iter()
                .any(|message| message.role == MessageRole::Tool),
            "tool result should still be recorded"
        );
        assert_eq!(messages.last().expect("assistant").text_content(), "done");
    }

    #[tokio::test]
    async fn regression_panicking_handler_does_not_break_subsequent_prompts() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("first"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("second"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        let event_count = Arc::new(AtomicUsize::new(0));
        let event_count_clone = event_count.clone();
        agent.subscribe(move |_event| {
            event_count_clone.fetch_add(1, Ordering::Relaxed);
        });
        agent.subscribe(|event| {
            if matches!(event, AgentEvent::AgentStart) {
                panic!("panic every run");
            }
        });

        let first = agent
            .prompt("one")
            .await
            .expect("first prompt should succeed");
        let second = agent
            .prompt("two")
            .await
            .expect("second prompt should succeed");
        assert_eq!(first.last().expect("assistant").text_content(), "first");
        assert_eq!(second.last().expect("assistant").text_content(), "second");
        assert!(
            event_count.load(Ordering::Relaxed) > 0,
            "non-panicking handler should keep receiving events across runs"
        );
    }

    #[tokio::test]
    async fn turn_end_events_include_usage_finish_reason_and_request_duration() {
        let usage = ChatUsage {
            input_tokens: 3,
            output_tokens: 2,
            total_tokens: 5,
        };
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: usage.clone(),
            }])),
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        let turn_ends = Arc::new(Mutex::new(Vec::<(
            usize,
            usize,
            u64,
            ChatUsage,
            Option<String>,
        )>::new()));
        let captured = turn_ends.clone();
        agent.subscribe(move |event| {
            if let AgentEvent::TurnEnd {
                turn,
                tool_results,
                request_duration_ms,
                usage,
                finish_reason,
            } = event
            {
                captured.lock().expect("turn_end lock").push((
                    *turn,
                    *tool_results,
                    *request_duration_ms,
                    usage.clone(),
                    finish_reason.clone(),
                ));
            }
        });

        let _ = agent.prompt("hello").await.expect("prompt should succeed");

        let turn_ends = turn_ends.lock().expect("turn_end lock");
        assert_eq!(turn_ends.len(), 1);
        assert_eq!(turn_ends[0].0, 1);
        assert_eq!(turn_ends[0].1, 0);
        assert_eq!(turn_ends[0].3, usage);
        assert_eq!(turn_ends[0].4.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn returns_max_turns_exceeded_for_infinite_tool_loop() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let second_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_2".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);

        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(
            client,
            AgentConfig {
                max_turns: 2,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(ReadTool);

        let error = agent.prompt("loop").await.expect_err("must hit max turns");
        match error {
            AgentError::MaxTurnsExceeded(2) => {}
            other => panic!("expected AgentError::MaxTurnsExceeded(2), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_tool_arguments_via_json_schema() {
        let assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({}),
        }]);

        let final_assistant = Message::assistant_text("done");

        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: final_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(ReadTool);

        let messages = agent
            .prompt("read without args")
            .await
            .expect("prompt succeeds");
        let tool_message = messages
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("tool result must exist");
        assert!(tool_message.is_error);
        assert!(tool_message.text_content().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn integration_prompt_with_stream_emits_incremental_deltas() {
        let client = Arc::new(StreamingMockClient {
            response: ChatResponse {
                message: Message::assistant_text("Hello"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
            deltas: vec!["Hel".to_string(), "lo".to_string()],
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        let streamed = Arc::new(Mutex::new(String::new()));
        let sink_streamed = streamed.clone();
        let sink = Arc::new(move |delta: String| {
            sink_streamed
                .lock()
                .expect("stream lock")
                .push_str(delta.as_str());
        });

        let new_messages = agent
            .prompt_with_stream("hello", Some(sink))
            .await
            .expect("prompt should succeed");

        assert_eq!(
            new_messages
                .last()
                .expect("assistant message")
                .text_content(),
            "Hello"
        );
        assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hello");
    }

    #[tokio::test]
    async fn functional_request_timeout_fails_closed_for_slow_provider() {
        let mut agent = Agent::new(
            Arc::new(DelayedEchoClient { delay_ms: 80 }),
            AgentConfig {
                request_max_retries: 0,
                request_timeout_ms: Some(10),
                ..AgentConfig::default()
            },
        );

        let error = agent
            .prompt("timeout please")
            .await
            .expect_err("slow provider should time out");
        match error {
            AgentError::RequestTimeout {
                timeout_ms: 10,
                attempt: 1,
            } => {}
            other => panic!("expected request timeout on first attempt, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn functional_token_budget_exceeded_fails_before_request_dispatch() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("should-not-run"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });

        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                system_prompt: String::new(),
                max_estimated_input_tokens: Some(1),
                ..AgentConfig::default()
            },
        );
        let error = agent
            .prompt("this prompt should exceed budget")
            .await
            .expect_err("token budget should fail closed");
        assert!(matches!(error, AgentError::TokenBudgetExceeded { .. }));
        assert!(
            client.requests.lock().await.is_empty(),
            "request should not be dispatched when budget check fails"
        );
    }

    #[tokio::test]
    async fn integration_total_token_budget_enforces_max_tokens_headroom() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("should-not-run"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });

        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                system_prompt: String::new(),
                max_tokens: Some(64),
                max_estimated_input_tokens: Some(10_000),
                max_estimated_total_tokens: Some(30),
                ..AgentConfig::default()
            },
        );
        let error = agent
            .prompt("small prompt")
            .await
            .expect_err("max_tokens should count against total budget");
        match error {
            AgentError::TokenBudgetExceeded {
                max_total_tokens: 30,
                ..
            } => {}
            other => panic!("expected total token budget failure, got {other:?}"),
        }
        assert!(
            client.requests.lock().await.is_empty(),
            "request should not be dispatched when total budget is exceeded"
        );
    }

    #[test]
    fn unit_extract_json_payload_parses_plain_and_fenced_json() {
        let plain = extract_json_payload(r#"{"ok":true,"count":2}"#).expect("plain json");
        assert_eq!(plain["ok"], true);
        assert_eq!(plain["count"], 2);

        let fenced = extract_json_payload(
            "result follows\n```json\n{\"status\":\"pass\",\"items\":[1,2]}\n```\nthanks",
        )
        .expect("fenced json");
        assert_eq!(fenced["status"], "pass");
        assert_eq!(fenced["items"][1], 2);
    }

    #[test]
    fn unit_build_structured_output_retry_prompt_includes_error_and_schema() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["mode"]
        });
        let prompt =
            build_structured_output_retry_prompt(&schema, "did not contain parseable JSON");
        assert!(prompt.contains("did not contain parseable JSON"));
        assert!(prompt.contains("\"required\":[\"mode\"]"));
        assert!(prompt.contains("reply with only valid JSON"));
    }

    #[tokio::test]
    async fn functional_prompt_json_returns_validated_value() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("{\"tasks\":[\"a\",\"b\"],\"ok\":true}"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "ok": { "type": "boolean" }
            },
            "required": ["tasks", "ok"],
            "additionalProperties": false
        });

        let value = agent
            .prompt_json("return tasks", &schema)
            .await
            .expect("structured output should succeed");
        assert_eq!(value["ok"], true);
        assert_eq!(value["tasks"][0], "a");
    }

    #[tokio::test]
    async fn functional_prompt_json_retries_after_non_json_and_succeeds() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("not-json-response"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("{\"tasks\":[\"retry\"],\"ok\":true}"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                structured_output_max_retries: 1,
                ..AgentConfig::default()
            },
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "ok": { "type": "boolean" }
            },
            "required": ["tasks", "ok"],
            "additionalProperties": false
        });

        let value = agent
            .prompt_json("return tasks", &schema)
            .await
            .expect("structured output retry should recover");
        assert_eq!(value["ok"], true);
        assert_eq!(value["tasks"][0], "retry");

        let requests = client.requests.lock().await;
        assert_eq!(requests.len(), 2, "prompt_json should perform one retry");
        let retry_prompt = last_user_prompt(&requests[1]);
        assert!(retry_prompt.contains("could not be accepted as structured JSON"));
        assert!(retry_prompt.contains("\"tasks\""));
    }

    #[tokio::test]
    async fn integration_prompt_json_accepts_fenced_json_payload() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text(
                    "Here is the payload:\n```json\n{\"mode\":\"apply\",\"steps\":3}\n```",
                ),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "mode": { "type": "string" },
                "steps": { "type": "integer" }
            },
            "required": ["mode", "steps"]
        });

        let value = agent
            .prompt_json("return mode", &schema)
            .await
            .expect("fenced structured output should parse");
        assert_eq!(value["mode"], "apply");
        assert_eq!(value["steps"], 3);
    }

    #[tokio::test]
    async fn integration_continue_turn_json_retries_after_schema_failure_and_succeeds() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("{\"mode\":\"apply\"}"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("{\"mode\":\"apply\",\"steps\":2}"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                structured_output_max_retries: 1,
                ..AgentConfig::default()
            },
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "mode": { "type": "string" },
                "steps": { "type": "integer" }
            },
            "required": ["mode", "steps"]
        });

        let value = agent
            .continue_turn_json(&schema)
            .await
            .expect("continue_turn_json should recover via retry");
        assert_eq!(value["mode"], "apply");
        assert_eq!(value["steps"], 2);

        let requests = client.requests.lock().await;
        assert_eq!(
            requests.len(),
            2,
            "continue_turn_json should perform one retry request"
        );
        let retry_prompt = last_user_prompt(&requests[1]);
        assert!(retry_prompt.contains("schema validation failed"));
        assert!(retry_prompt.contains("\"steps\""));
    }

    #[tokio::test]
    async fn regression_prompt_json_fails_closed_on_non_json_response() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("not-json-response"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                structured_output_max_retries: 0,
                ..AgentConfig::default()
            },
        );
        let schema = serde_json::json!({ "type": "object" });

        let error = agent
            .prompt_json("return object", &schema)
            .await
            .expect_err("non-json output must fail");
        assert!(matches!(error, AgentError::StructuredOutput(_)));
        assert!(error.to_string().contains("did not contain parseable JSON"));
    }

    #[tokio::test]
    async fn regression_prompt_json_fails_closed_on_schema_mismatch() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("{\"ok\":true}"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                structured_output_max_retries: 0,
                ..AgentConfig::default()
            },
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" },
                "tasks": { "type": "array" }
            },
            "required": ["ok", "tasks"]
        });

        let error = agent
            .prompt_json("return object", &schema)
            .await
            .expect_err("schema mismatch must fail");
        assert!(matches!(error, AgentError::StructuredOutput(_)));
        assert!(error.to_string().contains("schema validation failed"));
    }

    #[tokio::test]
    async fn regression_prompt_json_retry_exhaustion_fails_closed() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("still-not-json"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("again-not-json"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                structured_output_max_retries: 1,
                ..AgentConfig::default()
            },
        );
        let schema = serde_json::json!({ "type": "object" });

        let error = agent
            .prompt_json("return object", &schema)
            .await
            .expect_err("non-json output must fail after retries are exhausted");
        assert!(matches!(error, AgentError::StructuredOutput(_)));
        assert!(error.to_string().contains("did not contain parseable JSON"));

        let requests = client.requests.lock().await;
        assert_eq!(requests.len(), 2, "expected one retry attempt");
    }

    #[tokio::test]
    async fn regression_continue_turn_json_fails_closed_when_assistant_lacks_json() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ack"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                structured_output_max_retries: 0,
                ..AgentConfig::default()
            },
        );
        let schema = serde_json::json!({ "type": "object" });

        let error = agent
            .continue_turn_json(&schema)
            .await
            .expect_err("missing json must fail");
        assert!(matches!(error, AgentError::StructuredOutput(_)));
    }

    #[tokio::test]
    async fn integration_parallel_tool_execution_runs_calls_concurrently_and_preserves_order() {
        let first_assistant = Message::assistant_blocks(vec![
            ContentBlock::ToolCall {
                id: "call_1".to_string(),
                name: "slow_read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
            },
            ContentBlock::ToolCall {
                id: "call_2".to_string(),
                name: "slow_read".to_string(),
                arguments: serde_json::json!({ "path": "b.txt" }),
            },
        ]);
        let second_assistant = Message::assistant_text("done");
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(
            client,
            AgentConfig {
                max_parallel_tool_calls: 2,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(SlowReadTool { delay_ms: 120 });

        let started = Instant::now();
        let messages = agent
            .prompt("read both")
            .await
            .expect("prompt should succeed");
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(230),
            "expected concurrent tool execution under 230ms, got {elapsed:?}"
        );

        let tool_messages = messages
            .iter()
            .filter(|message| message.role == MessageRole::Tool)
            .collect::<Vec<_>>();
        assert_eq!(tool_messages.len(), 2);
        assert!(tool_messages[0].text_content().contains("read:a.txt"));
        assert!(tool_messages[1].text_content().contains("read:b.txt"));
    }

    #[tokio::test]
    async fn regression_bug_1_max_parallel_tool_calls_zero_clamps_to_safe_serial_execution() {
        let first_assistant = Message::assistant_blocks(vec![
            ContentBlock::ToolCall {
                id: "call_1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
            },
            ContentBlock::ToolCall {
                id: "call_2".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({ "path": "b.txt" }),
            },
        ]);
        let second_assistant = Message::assistant_text("done");
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(
            client,
            AgentConfig {
                max_parallel_tool_calls: 0,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(ReadTool);

        let messages = agent
            .prompt("read both")
            .await
            .expect("zero parallel limit should be normalized to a safe value");
        let tool_messages = messages
            .iter()
            .filter(|message| message.role == MessageRole::Tool)
            .collect::<Vec<_>>();
        assert_eq!(tool_messages.len(), 2);
        assert!(tool_messages[0].text_content().contains("read:a.txt"));
        assert!(tool_messages[1].text_content().contains("read:b.txt"));
    }

    #[tokio::test]
    async fn functional_context_window_limits_request_messages_and_compacts_history() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });

        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                max_context_messages: Some(4),
                ..AgentConfig::default()
            },
        );
        agent.append_message(Message::user("u1"));
        agent.append_message(Message::assistant_text("a1"));
        agent.append_message(Message::user("u2"));
        agent.append_message(Message::assistant_text("a2"));
        agent.append_message(Message::user("u3"));

        let _ = agent.prompt("latest").await.expect("prompt should succeed");

        let requests = client.requests.lock().await;
        let first_request = requests.first().expect("request should be captured");
        assert_eq!(first_request.messages.len(), 4);
        assert_eq!(first_request.messages[0].role, MessageRole::System);
        assert_eq!(first_request.messages[1].role, MessageRole::System);
        assert!(first_request.messages[1]
            .text_content()
            .starts_with(CONTEXT_SUMMARY_PREFIX));
        assert_eq!(first_request.messages[2].text_content(), "u3");
        assert_eq!(first_request.messages[3].text_content(), "latest");
        assert!(
            agent.messages().len() <= 4,
            "history should be compacted to configured context window"
        );
    }

    #[test]
    fn unit_bounded_messages_inserts_summary_with_system_prompt() {
        let messages = vec![
            Message::system("sys"),
            Message::user("u1"),
            Message::assistant_text("a1"),
            Message::user("u2"),
            Message::assistant_text("a2"),
        ];

        let bounded = bounded_messages(&messages, 4);
        assert_eq!(bounded.len(), 4);
        assert_eq!(bounded[0].role, MessageRole::System);
        assert_eq!(bounded[1].role, MessageRole::System);
        assert!(bounded[1]
            .text_content()
            .starts_with(CONTEXT_SUMMARY_PREFIX));
        assert_eq!(bounded[2].text_content(), "u2");
        assert_eq!(bounded[3].text_content(), "a2");
    }

    #[test]
    fn regression_bounded_messages_inserts_summary_without_system_prompt() {
        let messages = vec![
            Message::user("u1"),
            Message::assistant_text("a1"),
            Message::user("u2"),
            Message::assistant_text("a2"),
        ];

        let bounded = bounded_messages(&messages, 3);
        assert_eq!(bounded.len(), 3);
        assert_eq!(bounded[0].role, MessageRole::System);
        assert!(bounded[0]
            .text_content()
            .starts_with(CONTEXT_SUMMARY_PREFIX));
        assert_eq!(bounded[1].text_content(), "u2");
        assert_eq!(bounded[2].text_content(), "a2");
    }

    #[test]
    fn regression_truncate_chars_preserves_utf8_and_appends_ellipsis() {
        let long = "alpha beta gamma delta epsilon zeta eta theta";
        let truncated = truncate_chars(long, 12);
        assert_eq!(truncated.chars().count(), 12);
        assert!(truncated.ends_with('…'));

        let long_unicode = "hello 👋 from τau runtime";
        let truncated_unicode = truncate_chars(long_unicode, 9);
        assert_eq!(truncated_unicode.chars().count(), 9);
        assert!(truncated_unicode.ends_with('…'));

        let very_long = "x".repeat(CONTEXT_SUMMARY_MAX_CHARS + 200);
        let clipped = truncate_chars(&very_long, CONTEXT_SUMMARY_MAX_CHARS);
        assert!(clipped.chars().count() <= CONTEXT_SUMMARY_MAX_CHARS);
    }

    #[tokio::test]
    async fn regression_retry_transient_request_failures_and_recover_response() {
        let client = Arc::new(RetryThenSuccessClient {
            remaining_failures: AsyncMutex::new(1),
            attempts: AsyncMutex::new(0),
            response: ChatResponse {
                message: Message::assistant_text("recovered"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                request_max_retries: 2,
                request_retry_initial_backoff_ms: 1,
                request_retry_max_backoff_ms: 2,
                ..AgentConfig::default()
            },
        );

        let messages = agent
            .prompt("retry please")
            .await
            .expect("prompt should recover");
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "recovered"
        );
        assert_eq!(*client.attempts.lock().await, 2);
    }

    #[tokio::test]
    async fn regression_request_timeout_retries_and_recovers_when_next_attempt_is_fast() {
        let client = Arc::new(TimeoutThenSuccessClient {
            delays_ms: AsyncMutex::new(VecDeque::from([40, 0])),
            attempts: AsyncMutex::new(0),
            response: ChatResponse {
                message: Message::assistant_text("timeout-recovered"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                request_max_retries: 1,
                request_retry_initial_backoff_ms: 1,
                request_retry_max_backoff_ms: 1,
                request_timeout_ms: Some(10),
                ..AgentConfig::default()
            },
        );

        let messages = agent
            .prompt("recover after timeout")
            .await
            .expect("second attempt should succeed");
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "timeout-recovered"
        );
        assert_eq!(*client.attempts.lock().await, 2);
    }

    #[tokio::test]
    async fn regression_token_budget_none_disables_estimation_gate() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                system_prompt: String::new(),
                max_estimated_input_tokens: None,
                max_estimated_total_tokens: None,
                ..AgentConfig::default()
            },
        );
        let oversized_prompt = "x".repeat(250_000);
        let messages = agent
            .prompt(oversized_prompt)
            .await
            .expect("token gate disabled should allow prompt");
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "ok"
        );
    }

    #[tokio::test]
    async fn regression_tool_panic_isolated_to_error_tool_result() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "panic_tool".to_string(),
            arguments: serde_json::json!({}),
        }]);
        let second_assistant = Message::assistant_text("continued");
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(PanicTool);

        let messages = agent.prompt("panic").await.expect("prompt should continue");
        let tool_message = messages
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("tool result should be present");
        assert!(tool_message.is_error);
        assert!(tool_message
            .text_content()
            .contains("execution task failed"));
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "continued"
        );
    }

    #[tokio::test]
    async fn integration_tool_timeout_returns_error_tool_message_and_continues_turn() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "slow_read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let second_assistant = Message::assistant_text("continued after timeout");
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                tool_timeout_ms: Some(10),
                ..AgentConfig::default()
            },
        );
        agent.register_tool(SlowReadTool { delay_ms: 75 });

        let messages = agent
            .prompt("slow read")
            .await
            .expect("prompt should continue after tool timeout");
        let tool_message = messages
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("tool result should be present");
        assert!(tool_message.is_error);
        assert!(tool_message.text_content().contains("timed out after 10ms"));
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "continued after timeout"
        );
    }

    #[tokio::test]
    async fn unit_agent_fork_clones_state_without_aliasing_messages() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let second_assistant = Message::assistant_text("done");
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: first_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: second_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let mut base = Agent::new(client, AgentConfig::default());
        base.register_tool(ReadTool);
        base.append_message(Message::user("seed message"));

        let mut fork = base.fork();
        let fork_messages = fork.prompt("read").await.expect("fork prompt");
        assert!(
            fork_messages
                .iter()
                .any(|message| message.role == MessageRole::Tool),
            "fork should inherit registered tools and execute tool calls"
        );
        assert_eq!(base.messages().len(), 2);
        assert_eq!(fork.messages().len(), 6);
    }

    #[tokio::test]
    async fn integration_run_parallel_prompts_executes_runs_concurrently_with_ordered_results() {
        let agent = Agent::new(
            Arc::new(DelayedEchoClient { delay_ms: 90 }),
            AgentConfig::default(),
        );

        let started = Instant::now();
        let results = agent
            .run_parallel_prompts(vec!["prompt-1", "prompt-2", "prompt-3", "prompt-4"], 4)
            .await;
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(260),
            "expected concurrent runs under 260ms, got {elapsed:?}"
        );
        assert_eq!(results.len(), 4);

        for (index, result) in results.into_iter().enumerate() {
            let messages = result.expect("parallel run should succeed");
            assert_eq!(messages[0].role, MessageRole::User);
            assert_eq!(
                messages.last().expect("assistant reply").text_content(),
                format!("echo:prompt-{}", index + 1)
            );
        }
    }

    #[tokio::test]
    async fn integration_bug_6_run_parallel_prompts_allows_zero_parallel_limit() {
        let agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
        let results = agent.run_parallel_prompts(vec!["p1", "p2", "p3"], 0).await;
        assert_eq!(results.len(), 3);
        for (index, result) in results.into_iter().enumerate() {
            let messages = result.expect("zero parallel limit should clamp to a valid value");
            assert_eq!(
                messages.last().expect("assistant reply").text_content(),
                format!("echo:p{}", index + 1)
            );
        }
    }

    #[tokio::test]
    async fn regression_run_parallel_prompts_isolates_failures_per_prompt() {
        let agent = Agent::new(
            Arc::new(SelectiveFailureEchoClient),
            AgentConfig {
                request_max_retries: 0,
                ..AgentConfig::default()
            },
        );

        let results = agent
            .run_parallel_prompts(vec!["ok-1", "fail-2", "ok-3"], 2)
            .await;

        assert_eq!(results.len(), 3);
        assert!(results[0].as_ref().is_ok());
        assert!(matches!(
            results[1],
            Err(AgentError::Ai(tau_ai::TauAiError::HttpStatus {
                status: 503,
                ..
            }))
        ));
        assert!(results[2].as_ref().is_ok());
    }

    #[tokio::test]
    async fn functional_run_parallel_prompts_returns_empty_for_empty_input() {
        let agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
        let results = agent
            .run_parallel_prompts(std::iter::empty::<&str>(), 4)
            .await;
        assert!(results.is_empty());
    }
}
