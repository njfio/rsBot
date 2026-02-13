//! Core runtime primitives for building tool-using LLM agents in Tau.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use jsonschema::validator_for;
use serde_json::{json, Value};
use tau_ai::{
    ChatRequest, ChatUsage, LlmClient, Message, MessageRole, StreamDeltaHandler, TauAiError,
    ToolCall, ToolChoice, ToolDefinition,
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
    pub agent_id: String,
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
    pub stream_retry_with_buffering: bool,
    pub request_timeout_ms: Option<u64>,
    pub tool_timeout_ms: Option<u64>,
    pub max_estimated_input_tokens: Option<u32>,
    pub max_estimated_total_tokens: Option<u32>,
    pub structured_output_max_retries: usize,
    pub react_max_replans_on_tool_failure: usize,
    pub memory_retrieval_limit: usize,
    pub memory_embedding_dimensions: usize,
    pub memory_min_similarity: f32,
    pub memory_max_chars_per_item: usize,
    pub response_cache_enabled: bool,
    pub response_cache_max_entries: usize,
    pub tool_result_cache_enabled: bool,
    pub tool_result_cache_max_entries: usize,
    pub model_input_cost_per_million: Option<f64>,
    pub model_output_cost_per_million: Option<f64>,
    pub cost_budget_usd: Option<f64>,
    pub cost_alert_thresholds_percent: Vec<u8>,
    pub async_event_queue_capacity: usize,
    pub async_event_handler_timeout_ms: Option<u64>,
    pub async_event_block_on_full: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: "tau-agent".to_string(),
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
            stream_retry_with_buffering: true,
            request_timeout_ms: Some(120_000),
            tool_timeout_ms: Some(120_000),
            max_estimated_input_tokens: Some(120_000),
            max_estimated_total_tokens: None,
            structured_output_max_retries: 1,
            react_max_replans_on_tool_failure: 1,
            memory_retrieval_limit: 3,
            memory_embedding_dimensions: 128,
            memory_min_similarity: 0.55,
            memory_max_chars_per_item: 180,
            response_cache_enabled: true,
            response_cache_max_entries: 128,
            tool_result_cache_enabled: true,
            tool_result_cache_max_entries: 256,
            model_input_cost_per_million: None,
            model_output_cost_per_million: None,
            cost_budget_usd: None,
            cost_alert_thresholds_percent: vec![80, 100],
            async_event_queue_capacity: 128,
            async_event_handler_timeout_ms: Some(5_000),
            async_event_block_on_full: false,
        }
    }
}

/// Public struct `AgentCostSnapshot` used across Tau components.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentCostSnapshot {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: f64,
    pub budget_usd: Option<f64>,
    pub budget_utilization: Option<f64>,
}

/// Public struct `AsyncEventDispatchMetrics` used across Tau components.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AsyncEventDispatchMetrics {
    pub enqueued: u64,
    pub dropped_full: u64,
    pub completed: u64,
    pub timed_out: u64,
    pub panicked: u64,
}

#[derive(Default)]
struct AsyncEventDispatchMetricsInner {
    enqueued: AtomicU64,
    dropped_full: AtomicU64,
    completed: AtomicU64,
    timed_out: AtomicU64,
    panicked: AtomicU64,
}

impl AsyncEventDispatchMetricsInner {
    fn snapshot(&self) -> AsyncEventDispatchMetrics {
        AsyncEventDispatchMetrics {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            dropped_full: self.dropped_full.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            timed_out: self.timed_out.load(Ordering::Relaxed),
            panicked: self.panicked.load(Ordering::Relaxed),
        }
    }
}

/// Cooperative cancellation token shared across runtime components.
#[derive(Debug, Clone, Default)]
pub struct CooperativeCancellationToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl CooperativeCancellationToken {
    /// Creates a new, not-yet-cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the token as cancelled and wakes pending waiters.
    pub fn cancel(&self) {
        let already_cancelled = self.cancelled.swap(true, Ordering::SeqCst);
        if !already_cancelled {
            self.notify.notify_waiters();
        }
    }

    /// Returns true when cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
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
    fn is_cacheable(&self) -> bool {
        false
    }
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
    ReplanTriggered {
        turn: usize,
        reason: String,
    },
    CostUpdated {
        turn: usize,
        turn_cost_usd: f64,
        cumulative_cost_usd: f64,
        budget_usd: Option<f64>,
    },
    CostBudgetAlert {
        turn: usize,
        threshold_percent: u8,
        cumulative_cost_usd: f64,
        budget_usd: f64,
    },
}

/// Enumerates supported `AgentError` values.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Ai(#[from] TauAiError),
    #[error("agent execution cancelled")]
    Cancelled,
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

/// Enumerates supported `AgentDirectMessageError` values.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentDirectMessageError {
    #[error(
        "direct message route from '{from_agent_id}' to '{to_agent_id}' is not allowed by policy"
    )]
    UnauthorizedRoute {
        from_agent_id: String,
        to_agent_id: String,
    },
    #[error("direct message content cannot be empty")]
    EmptyContent,
    #[error(
        "direct message content exceeds policy max chars (actual={actual_chars}, max={max_chars})"
    )]
    MessageTooLong {
        actual_chars: usize,
        max_chars: usize,
    },
}

/// Public struct `AgentDirectMessagePolicy` used across Tau components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDirectMessagePolicy {
    pub allow_self_messages: bool,
    pub max_message_chars: usize,
    allowed_routes: HashSet<(String, String)>,
}

impl Default for AgentDirectMessagePolicy {
    fn default() -> Self {
        Self {
            allow_self_messages: false,
            max_message_chars: 4_000,
            allowed_routes: HashSet::new(),
        }
    }
}

impl AgentDirectMessagePolicy {
    /// Adds a directed route permission (`from_agent_id` -> `to_agent_id`).
    pub fn allow_route(
        &mut self,
        from_agent_id: impl Into<String>,
        to_agent_id: impl Into<String>,
    ) {
        self.allowed_routes
            .insert((from_agent_id.into(), to_agent_id.into()));
    }

    /// Adds route permissions for both directions between `left_agent_id` and `right_agent_id`.
    pub fn allow_bidirectional_route(
        &mut self,
        left_agent_id: impl Into<String>,
        right_agent_id: impl Into<String>,
    ) {
        let left = left_agent_id.into();
        let right = right_agent_id.into();
        self.allow_route(left.clone(), right.clone());
        self.allow_route(right, left);
    }

    /// Returns true when the policy allows direct messages from `from_agent_id` to `to_agent_id`.
    pub fn allows(&self, from_agent_id: &str, to_agent_id: &str) -> bool {
        if from_agent_id == to_agent_id {
            return self.allow_self_messages;
        }
        self.allowed_routes
            .contains(&(from_agent_id.to_string(), to_agent_id.to_string()))
    }
}

type EventHandler = Arc<dyn Fn(&AgentEvent) + Send + Sync>;
type AsyncEventHandlerFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
type AsyncEventHandler = Arc<dyn Fn(AgentEvent) -> AsyncEventHandlerFuture + Send + Sync>;
type AsyncEventSender = std::sync::mpsc::SyncSender<AgentEvent>;
const CONTEXT_SUMMARY_PREFIX: &str = "[Tau context summary]";
const CONTEXT_SUMMARY_MAX_CHARS: usize = 1_200;
const CONTEXT_SUMMARY_SNIPPET_MAX_CHARS: usize = 160;
const CONTEXT_SUMMARY_MAX_EXCERPTS: usize = 6;
const MEMORY_RECALL_PREFIX: &str = "[Tau memory recall]";
const DIRECT_MESSAGE_PREFIX: &str = "[Tau direct message]";
const REPLAN_ON_TOOL_FAILURE_PROMPT: &str = "One or more tool calls failed. Replan and continue with an alternative approach using available tools. If no viable tool exists, explain what is missing and ask the user for clarification.";
const FAILURE_SIGNAL_PHRASES: &[&str] = &[
    "can't",
    "cannot",
    "could not",
    "couldn't",
    "unable",
    "failed",
    "failure",
    "error",
    "not available",
    "not possible",
    "do not have",
    "don't have",
    "no tool",
];

#[derive(Clone)]
struct RegisteredTool {
    definition: ToolDefinition,
    tool: Arc<dyn AgentTool>,
    cacheable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolExecutionStats {
    total: usize,
    errors: usize,
}

#[derive(Debug, Clone)]
struct MemoryRecallMatch {
    score: f32,
    role: MessageRole,
    text: String,
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
    agent_id: String,
    messages: Vec<Message>,
    tools: HashMap<String, RegisteredTool>,
    response_cache: HashMap<String, tau_ai::ChatResponse>,
    response_cache_order: VecDeque<String>,
    tool_result_cache: HashMap<String, ToolExecutionResult>,
    tool_result_cache_order: VecDeque<String>,
    handlers: Vec<EventHandler>,
    async_handlers: Vec<AsyncEventSender>,
    async_event_metrics: Arc<AsyncEventDispatchMetricsInner>,
    cancellation_token: Option<CooperativeCancellationToken>,
    cumulative_usage: ChatUsage,
    cumulative_cost_usd: f64,
    emitted_cost_alert_thresholds: HashSet<u8>,
}

impl Agent {
    /// Creates a new [`Agent`] with an initial system message when configured.
    pub fn new(client: Arc<dyn LlmClient>, config: AgentConfig) -> Self {
        let mut messages = Vec::new();
        if !config.system_prompt.trim().is_empty() {
            messages.push(Message::system(config.system_prompt.clone()));
        }
        let agent_id = config.agent_id.clone();

        Self {
            client,
            config,
            agent_id,
            messages,
            tools: HashMap::new(),
            response_cache: HashMap::new(),
            response_cache_order: VecDeque::new(),
            tool_result_cache: HashMap::new(),
            tool_result_cache_order: VecDeque::new(),
            handlers: Vec::new(),
            async_handlers: Vec::new(),
            async_event_metrics: Arc::new(AsyncEventDispatchMetricsInner::default()),
            cancellation_token: None,
            cumulative_usage: ChatUsage::default(),
            cumulative_cost_usd: 0.0,
            emitted_cost_alert_thresholds: HashSet::new(),
        }
    }

    /// Adds an event subscriber that receives runtime lifecycle callbacks.
    pub fn subscribe<F>(&mut self, handler: F)
    where
        F: Fn(&AgentEvent) + Send + Sync + 'static,
    {
        self.handlers.push(Arc::new(handler));
    }

    /// Adds an asynchronous event subscriber that runs in an isolated worker.
    ///
    /// Async handlers are dispatched through a bounded queue to provide backpressure.
    /// Panics and timeouts are isolated and reflected in [`Agent::async_event_metrics`].
    pub fn subscribe_async<F, Fut>(&mut self, handler: F)
    where
        F: Fn(AgentEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let queue_capacity = self.config.async_event_queue_capacity.max(1);
        let (sender, receiver) = std::sync::mpsc::sync_channel(queue_capacity);
        let timeout = timeout_duration_from_ms(self.config.async_event_handler_timeout_ms);
        let metrics = Arc::clone(&self.async_event_metrics);
        let boxed_handler: AsyncEventHandler =
            Arc::new(move |event: AgentEvent| Box::pin(handler(event)));
        spawn_async_event_handler_worker(receiver, boxed_handler, timeout, metrics);
        self.async_handlers.push(sender);
    }

    /// Returns aggregate async event dispatch metrics for this agent instance.
    pub fn async_event_metrics(&self) -> AsyncEventDispatchMetrics {
        self.async_event_metrics.snapshot()
    }

    /// Registers a tool exposed to the language model.
    pub fn register_tool<T>(&mut self, tool: T)
    where
        T: AgentTool + 'static,
    {
        let (_, replaced) = self.register_tool_internal(tool);
        if replaced.is_some() {
            self.clear_tool_result_cache();
        }
    }

    fn register_tool_internal<T>(&mut self, tool: T) -> (String, Option<RegisteredTool>)
    where
        T: AgentTool + 'static,
    {
        let cacheable = tool.is_cacheable();
        let definition = tool.definition();
        let name = definition.name.clone();
        let replaced = self.tools.insert(
            name.clone(),
            RegisteredTool {
                definition,
                tool: Arc::new(tool),
                cacheable,
            },
        );
        (name, replaced)
    }

    /// Returns true when a tool with `tool_name` is registered.
    pub fn has_tool(&self, tool_name: &str) -> bool {
        self.tools.contains_key(tool_name)
    }

    /// Returns sorted registered tool names.
    pub fn registered_tool_names(&self) -> Vec<String> {
        let mut names = self.tools.keys().cloned().collect::<Vec<_>>();
        names.sort();
        names
    }

    /// Unregisters a tool by name.
    pub fn unregister_tool(&mut self, tool_name: &str) -> bool {
        let removed = self.tools.remove(tool_name).is_some();
        if removed {
            self.clear_tool_result_cache();
        }
        removed
    }

    /// Removes all registered tools.
    pub fn clear_tools(&mut self) {
        if self.tools.is_empty() {
            return;
        }
        self.tools.clear();
        self.clear_tool_result_cache();
    }

    /// Returns this agent's identifier for policy checks and direct messaging.
    pub fn agent_id(&self) -> &str {
        self.agent_id.as_str()
    }

    /// Sets this agent's identifier when `agent_id` is non-empty after trimming.
    pub fn set_agent_id(&mut self, agent_id: impl Into<String>) {
        let normalized = agent_id.into().trim().to_string();
        if normalized.is_empty() {
            return;
        }
        self.agent_id = normalized;
    }

    /// Sends a direct message to `recipient` when allowed by `policy`.
    pub fn send_direct_message(
        &self,
        recipient: &mut Agent,
        content: &str,
        policy: &AgentDirectMessagePolicy,
    ) -> Result<(), AgentDirectMessageError> {
        recipient.receive_direct_message(self.agent_id(), content, policy)
    }

    /// Receives a direct message from another agent when allowed by `policy`.
    pub fn receive_direct_message(
        &mut self,
        from_agent_id: &str,
        content: &str,
        policy: &AgentDirectMessagePolicy,
    ) -> Result<(), AgentDirectMessageError> {
        let to_agent_id = self.agent_id();
        if !policy.allows(from_agent_id, to_agent_id) {
            return Err(AgentDirectMessageError::UnauthorizedRoute {
                from_agent_id: from_agent_id.to_string(),
                to_agent_id: to_agent_id.to_string(),
            });
        }
        let normalized_content =
            normalize_direct_message_content(content, policy.max_message_chars)?;
        let direct_message = Message::system(format!(
            "{} from={} to={}\n{}",
            DIRECT_MESSAGE_PREFIX, from_agent_id, to_agent_id, normalized_content
        ));
        self.messages.push(direct_message.clone());
        self.emit(AgentEvent::MessageAdded {
            message: direct_message,
        });
        Ok(())
    }

    /// Registers a temporary tool for the duration of `run` and restores prior state afterward.
    pub async fn with_scoped_tool<T, R>(
        &mut self,
        tool: T,
        run: impl for<'a> FnOnce(&'a mut Self) -> Pin<Box<dyn Future<Output = R> + 'a>>,
    ) -> R
    where
        T: AgentTool + 'static,
    {
        self.with_scoped_tools(std::iter::once(tool), run).await
    }

    /// Registers temporary tools for the duration of `run` and restores prior state afterward.
    pub async fn with_scoped_tools<T, I, R>(
        &mut self,
        tools: I,
        run: impl for<'a> FnOnce(&'a mut Self) -> Pin<Box<dyn Future<Output = R> + 'a>>,
    ) -> R
    where
        T: AgentTool + 'static,
        I: IntoIterator<Item = T>,
    {
        let mut replaced = Vec::new();
        for tool in tools {
            let (name, previous) = self.register_tool_internal(tool);
            replaced.push((name, previous));
        }
        self.clear_tool_result_cache();

        let result = run(self).await;

        for (name, previous) in replaced.into_iter().rev() {
            if let Some(previous) = previous {
                self.tools.insert(name, previous);
            } else {
                self.tools.remove(&name);
            }
        }
        self.clear_tool_result_cache();
        result
    }

    /// Installs or clears a cooperative cancellation token for subsequent runs.
    pub fn set_cancellation_token(&mut self, token: Option<CooperativeCancellationToken>) {
        self.cancellation_token = token;
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

    /// Returns cumulative token usage and estimated model spend for this agent.
    pub fn cost_snapshot(&self) -> AgentCostSnapshot {
        let budget_usd = self.config.cost_budget_usd.filter(|budget| *budget > 0.0);
        let budget_utilization = budget_usd.map(|budget| {
            if budget <= f64::EPSILON {
                0.0
            } else {
                self.cumulative_cost_usd / budget
            }
        });
        AgentCostSnapshot {
            input_tokens: self.cumulative_usage.input_tokens,
            output_tokens: self.cumulative_usage.output_tokens,
            total_tokens: self.cumulative_usage.total_tokens,
            estimated_cost_usd: self.cumulative_cost_usd,
            budget_usd,
            budget_utilization,
        }
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
        self.prompt_internal(text.into(), None, false).await
    }

    /// Runs a prompt and validates assistant output against a JSON schema.
    pub async fn prompt_json(
        &mut self,
        text: impl Into<String>,
        schema: &Value,
    ) -> Result<Value, AgentError> {
        let new_messages = self.prompt_internal(text.into(), None, true).await?;
        self.parse_structured_output_with_retry(new_messages, schema)
            .await
    }

    /// Runs a prompt while optionally streaming text deltas.
    pub async fn prompt_with_stream(
        &mut self,
        text: impl Into<String>,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<Vec<Message>, AgentError> {
        self.prompt_internal(text.into(), on_delta, false).await
    }

    async fn prompt_internal(
        &mut self,
        text: String,
        on_delta: Option<StreamDeltaHandler>,
        json_mode: bool,
    ) -> Result<Vec<Message>, AgentError> {
        let start_index = self.messages.len();
        let user_message = Message::user(text);
        self.messages.push(user_message.clone());
        self.emit(AgentEvent::MessageAdded {
            message: user_message,
        });
        let result = self.run_loop(start_index, on_delta, json_mode).await;
        if result.is_ok() {
            self.compact_message_history();
        }
        result
    }

    /// Continues the current turn without adding a new user message.
    pub async fn continue_turn(&mut self) -> Result<Vec<Message>, AgentError> {
        self.continue_turn_internal(None, false).await
    }

    /// Continues the turn and parses the response as schema-validated JSON.
    pub async fn continue_turn_json(&mut self, schema: &Value) -> Result<Value, AgentError> {
        let new_messages = self.continue_turn_internal(None, true).await?;
        self.parse_structured_output_with_retry(new_messages, schema)
            .await
    }

    /// Continues the current turn while optionally streaming text deltas.
    pub async fn continue_turn_with_stream(
        &mut self,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<Vec<Message>, AgentError> {
        self.continue_turn_internal(on_delta, false).await
    }

    async fn continue_turn_internal(
        &mut self,
        on_delta: Option<StreamDeltaHandler>,
        json_mode: bool,
    ) -> Result<Vec<Message>, AgentError> {
        let start_index = self.messages.len();
        let result = self.run_loop(start_index, on_delta, json_mode).await;
        if result.is_ok() {
            self.compact_message_history();
        }
        result
    }

    fn emit(&self, event: AgentEvent) {
        for handler in &self.handlers {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&event)));
        }
        for sender in &self.async_handlers {
            if self.config.async_event_block_on_full {
                match sender.send(event.clone()) {
                    Ok(()) => {
                        self.async_event_metrics
                            .enqueued
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        self.async_event_metrics
                            .dropped_full
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                continue;
            }

            match sender.try_send(event.clone()) {
                Ok(()) => {
                    self.async_event_metrics
                        .enqueued
                        .fetch_add(1, Ordering::Relaxed);
                }
                Err(std::sync::mpsc::TrySendError::Full(_))
                | Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    self.async_event_metrics
                        .dropped_full
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    fn cancellation_token(&self) -> Option<CooperativeCancellationToken> {
        self.cancellation_token.clone()
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .as_ref()
            .map(CooperativeCancellationToken::is_cancelled)
            .unwrap_or(false)
    }

    fn response_cache_eligible(
        &self,
        request: &ChatRequest,
        on_delta: &Option<StreamDeltaHandler>,
    ) -> bool {
        if !self.config.response_cache_enabled
            || self.config.response_cache_max_entries == 0
            || on_delta.is_some()
        {
            return false;
        }
        let temperature = request.temperature.unwrap_or(0.0);
        temperature.abs() <= f32::EPSILON
    }

    fn response_cache_key(request: &ChatRequest) -> Option<String> {
        serde_json::to_string(request).ok()
    }

    fn lookup_response_cache(
        &self,
        request: &ChatRequest,
        on_delta: &Option<StreamDeltaHandler>,
    ) -> Option<tau_ai::ChatResponse> {
        if !self.response_cache_eligible(request, on_delta) {
            return None;
        }
        let key = Self::response_cache_key(request)?;
        self.response_cache.get(&key).cloned()
    }

    fn store_response_cache(
        &mut self,
        request: &ChatRequest,
        on_delta: &Option<StreamDeltaHandler>,
        response: &tau_ai::ChatResponse,
    ) {
        if !self.response_cache_eligible(request, on_delta) {
            return;
        }
        let Some(key) = Self::response_cache_key(request) else {
            return;
        };
        cache_insert_with_limit(
            &mut self.response_cache,
            &mut self.response_cache_order,
            key,
            response.clone(),
            self.config.response_cache_max_entries,
        );
    }

    fn tool_result_cache_key(call: &ToolCall) -> Option<String> {
        serde_json::to_string(&json!({
            "name": call.name,
            "arguments": call.arguments,
        }))
        .ok()
    }

    fn lookup_tool_result_cache(&self, call: &ToolCall) -> Option<ToolExecutionResult> {
        if !self.config.tool_result_cache_enabled || self.config.tool_result_cache_max_entries == 0
        {
            return None;
        }
        let registered = self.tools.get(&call.name)?;
        if !registered.cacheable {
            return None;
        }
        let key = Self::tool_result_cache_key(call)?;
        self.tool_result_cache.get(&key).cloned()
    }

    fn store_tool_result_cache(&mut self, call: &ToolCall, result: &ToolExecutionResult) {
        if !self.config.tool_result_cache_enabled
            || self.config.tool_result_cache_max_entries == 0
            || result.is_error
        {
            return;
        }
        let Some(registered) = self.tools.get(&call.name) else {
            return;
        };
        if !registered.cacheable {
            return;
        }
        let Some(key) = Self::tool_result_cache_key(call) else {
            return;
        };
        cache_insert_with_limit(
            &mut self.tool_result_cache,
            &mut self.tool_result_cache_order,
            key,
            result.clone(),
            self.config.tool_result_cache_max_entries,
        );
    }

    fn clear_tool_result_cache(&mut self) {
        self.tool_result_cache.clear();
        self.tool_result_cache_order.clear();
    }

    fn accumulate_usage_and_emit_cost_events(&mut self, turn: usize, usage: &ChatUsage) {
        self.cumulative_usage.input_tokens = self
            .cumulative_usage
            .input_tokens
            .saturating_add(usage.input_tokens);
        self.cumulative_usage.output_tokens = self
            .cumulative_usage
            .output_tokens
            .saturating_add(usage.output_tokens);
        self.cumulative_usage.total_tokens = self
            .cumulative_usage
            .total_tokens
            .saturating_add(usage.total_tokens);

        let turn_cost_usd = estimate_usage_cost_usd(
            usage,
            self.config.model_input_cost_per_million,
            self.config.model_output_cost_per_million,
        );
        self.cumulative_cost_usd += turn_cost_usd;

        let budget_usd = self.config.cost_budget_usd.filter(|budget| *budget > 0.0);
        if self.config.model_input_cost_per_million.is_none()
            && self.config.model_output_cost_per_million.is_none()
            && budget_usd.is_none()
        {
            return;
        }

        self.emit(AgentEvent::CostUpdated {
            turn,
            turn_cost_usd,
            cumulative_cost_usd: self.cumulative_cost_usd,
            budget_usd,
        });

        let Some(budget_usd) = budget_usd else {
            return;
        };
        let utilization = if budget_usd <= f64::EPSILON {
            0.0
        } else {
            self.cumulative_cost_usd / budget_usd
        };
        let utilization_percent = utilization * 100.0;
        for threshold in normalize_cost_alert_thresholds(&self.config.cost_alert_thresholds_percent)
        {
            if utilization_percent >= f64::from(threshold)
                && self.emitted_cost_alert_thresholds.insert(threshold)
            {
                self.emit(AgentEvent::CostBudgetAlert {
                    turn,
                    threshold_percent: threshold,
                    cumulative_cost_usd: self.cumulative_cost_usd,
                    budget_usd,
                });
            }
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
                    new_messages = self.prompt_internal(retry_prompt, None, true).await?;
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
        json_mode: bool,
    ) -> Result<Vec<Message>, AgentError> {
        if self.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        self.emit(AgentEvent::AgentStart);
        let mut pending_replan_on_tool_failure = false;
        let mut replans_used = 0usize;

        for turn in 1..=self.config.max_turns {
            if self.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            self.emit(AgentEvent::TurnStart { turn });

            let tools = self.tool_definitions();
            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: self.request_messages(),
                tool_choice: if tools.is_empty() {
                    None
                } else {
                    Some(ToolChoice::Auto)
                },
                json_mode,
                tools,
                max_tokens: self.config.max_tokens,
                temperature: self.config.temperature,
            };
            self.enforce_token_budget(&request)?;

            let request_started = std::time::Instant::now();
            let response = if let Some(cached) = self.lookup_response_cache(&request, &on_delta) {
                cached
            } else {
                let response = self
                    .complete_with_retry(request.clone(), on_delta.clone())
                    .await?;
                self.store_response_cache(&request, &on_delta, &response);
                response
            };
            let request_duration_ms = request_started.elapsed().as_millis() as u64;
            let finish_reason = response.finish_reason.clone();
            let usage = response.usage.clone();
            let assistant = response.message;
            self.messages.push(assistant.clone());
            self.emit(AgentEvent::MessageAdded {
                message: assistant.clone(),
            });
            self.accumulate_usage_and_emit_cost_events(turn, &usage);

            let assistant_text = assistant.text_content();
            let tool_calls = assistant.tool_calls();
            if tool_calls.is_empty() {
                if pending_replan_on_tool_failure
                    && replans_used < self.config.react_max_replans_on_tool_failure
                    && assistant_text_suggests_failure(&assistant_text)
                {
                    self.emit(AgentEvent::ReplanTriggered {
                        turn,
                        reason:
                            "all tool calls in previous turn failed and assistant reported failure"
                                .to_string(),
                    });
                    let replan_message = Message::user(REPLAN_ON_TOOL_FAILURE_PROMPT);
                    self.messages.push(replan_message.clone());
                    self.emit(AgentEvent::MessageAdded {
                        message: replan_message,
                    });
                    replans_used = replans_used.saturating_add(1);
                    pending_replan_on_tool_failure = false;
                    self.emit(AgentEvent::TurnEnd {
                        turn,
                        tool_results: 0,
                        request_duration_ms,
                        usage,
                        finish_reason,
                    });
                    continue;
                }
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

            let tool_stats = self.execute_tool_calls(tool_calls).await?;
            pending_replan_on_tool_failure =
                tool_stats.total > 0 && tool_stats.errors == tool_stats.total;

            self.emit(AgentEvent::TurnEnd {
                turn,
                tool_results: tool_stats.total,
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
        let mut messages = bounded_messages(&self.messages, limit);
        if self.config.memory_retrieval_limit == 0 || self.messages.len() <= limit {
            return messages;
        }
        let Some(recall) = self.build_memory_recall_message(limit) else {
            return messages;
        };
        let insert_at = messages
            .iter()
            .take_while(|message| message.role == MessageRole::System)
            .count();
        messages.insert(insert_at, Message::system(recall));
        messages
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

    fn build_memory_recall_message(&self, context_limit: usize) -> Option<String> {
        let query = self
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.text_content())?;
        if query.trim().is_empty() {
            return None;
        }
        let dropped_cutoff = self.messages.len().saturating_sub(context_limit);
        if dropped_cutoff == 0 {
            return None;
        }
        let recalled = retrieve_memory_matches(
            &self.messages[..dropped_cutoff],
            &query,
            self.config.memory_retrieval_limit,
            self.config.memory_embedding_dimensions,
            self.config.memory_min_similarity,
        );
        if recalled.is_empty() {
            return None;
        }

        let mut lines = Vec::with_capacity(recalled.len().saturating_add(1));
        lines.push(MEMORY_RECALL_PREFIX.to_string());
        for memory in recalled {
            let collapsed = collapse_whitespace(&memory.text);
            let excerpt = truncate_chars(&collapsed, self.config.memory_max_chars_per_item);
            lines.push(format!(
                "- score={:.2} role={} text={}",
                memory.score,
                role_label(memory.role),
                excerpt
            ));
        }
        Some(lines.join("\n"))
    }

    async fn complete_with_retry(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<tau_ai::ChatResponse, AgentError> {
        if self.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        let max_retries = self.config.request_max_retries;
        let mut attempt = 0usize;
        let mut backoff_ms = self.config.request_retry_initial_backoff_ms.max(1);
        let max_backoff_ms = self.config.request_retry_max_backoff_ms.max(backoff_ms);
        let request_timeout = timeout_duration_from_ms(self.config.request_timeout_ms);
        let cancellation_token = self.cancellation_token();
        let stream_retry_enabled = on_delta.is_some() && self.config.stream_retry_with_buffering;
        let stream_buffer_state = if stream_retry_enabled {
            Some(Arc::new(Mutex::new(StreamingRetryBufferState::default())))
        } else {
            None
        };

        loop {
            if cancellation_token
                .as_ref()
                .map(CooperativeCancellationToken::is_cancelled)
                .unwrap_or(false)
            {
                return Err(AgentError::Cancelled);
            }

            let request_for_attempt = request.clone();
            let attempt_on_delta = if stream_retry_enabled {
                let user_handler = match on_delta.as_ref() {
                    Some(handler) => handler.clone(),
                    None => {
                        return Err(AgentError::Ai(TauAiError::InvalidResponse(
                            "stream retry invariant violated: missing delta handler".to_string(),
                        )));
                    }
                };
                let state: Arc<Mutex<StreamingRetryBufferState>> =
                    match stream_buffer_state.as_ref() {
                        Some(state) => Arc::clone(state),
                        None => {
                            return Err(AgentError::Ai(TauAiError::InvalidResponse(
                                "stream retry invariant violated: missing buffer state".to_string(),
                            )));
                        }
                    };
                lock_or_recover(state.as_ref()).reset_attempt();
                Some(Arc::new(move |delta: String| {
                    let replay = {
                        let mut guard = lock_or_recover(state.as_ref());
                        stream_retry_buffer_on_delta(&mut guard, delta.as_str())
                    };
                    if let Some(replayed_delta) = replay {
                        user_handler(replayed_delta);
                    }
                }) as StreamDeltaHandler)
            } else {
                on_delta.clone()
            };
            let client_call = self
                .client
                .complete_with_stream(request_for_attempt, attempt_on_delta);
            let response_result = if let Some(timeout) = request_timeout {
                let timed = if let Some(token) = cancellation_token.clone() {
                    tokio::select! {
                        _ = token.cancelled() => return Err(AgentError::Cancelled),
                        timed = tokio::time::timeout(timeout, client_call) => timed,
                    }
                } else {
                    tokio::time::timeout(timeout, client_call).await
                };

                match timed {
                    Ok(result) => result,
                    Err(_) => {
                        let can_retry =
                            attempt < max_retries && (on_delta.is_none() || stream_retry_enabled);
                        if !can_retry {
                            return Err(AgentError::RequestTimeout {
                                timeout_ms: timeout.as_millis() as u64,
                                attempt: attempt.saturating_add(1),
                            });
                        }
                        sleep_with_cancellation(
                            Duration::from_millis(backoff_ms),
                            cancellation_token.clone(),
                        )
                        .await?;
                        backoff_ms = backoff_ms.saturating_mul(2).min(max_backoff_ms);
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                }
            } else if let Some(token) = cancellation_token.clone() {
                tokio::select! {
                    _ = token.cancelled() => return Err(AgentError::Cancelled),
                    result = client_call => result,
                }
            } else {
                client_call.await
            };

            match response_result {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let can_retry = attempt < max_retries
                        && (on_delta.is_none() || stream_retry_enabled)
                        && is_retryable_ai_error(&error);
                    if !can_retry {
                        return Err(AgentError::Ai(error));
                    }

                    sleep_with_cancellation(
                        Duration::from_millis(backoff_ms),
                        cancellation_token.clone(),
                    )
                    .await?;
                    backoff_ms = backoff_ms.saturating_mul(2).min(max_backoff_ms);
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    async fn execute_tool_calls(
        &mut self,
        tool_calls: Vec<ToolCall>,
    ) -> Result<ToolExecutionStats, AgentError> {
        if self.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        let mut stats = ToolExecutionStats {
            total: 0,
            errors: 0,
        };
        let max_parallel = self.config.max_parallel_tool_calls.max(1);
        if max_parallel == 1 || tool_calls.len() <= 1 {
            for call in tool_calls {
                if self.is_cancelled() {
                    return Err(AgentError::Cancelled);
                }
                if self.execute_tool_call(call).await {
                    stats.errors = stats.errors.saturating_add(1);
                }
                stats.total = stats.total.saturating_add(1);
            }
            return Ok(stats);
        }

        enum PendingToolResult {
            Ready(ToolExecutionResult),
            Pending(tokio::task::JoinHandle<ToolExecutionResult>),
        }

        for chunk in tool_calls.chunks(max_parallel) {
            if self.is_cancelled() {
                return Err(AgentError::Cancelled);
            }
            let mut pending = Vec::with_capacity(chunk.len());
            for call in chunk.iter().cloned() {
                if self.is_cancelled() {
                    return Err(AgentError::Cancelled);
                }
                self.emit(AgentEvent::ToolExecutionStart {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    arguments: call.arguments.clone(),
                });
                if let Some(cached) = self.lookup_tool_result_cache(&call) {
                    pending.push((call, PendingToolResult::Ready(cached)));
                    continue;
                }
                let handle = self.spawn_tool_call_task(call.clone());
                pending.push((call, PendingToolResult::Pending(handle)));
            }

            for (call, execution) in pending {
                let result = match execution {
                    PendingToolResult::Ready(result) => result,
                    PendingToolResult::Pending(handle) => match handle.await {
                        Ok(result) => result,
                        Err(error) => ToolExecutionResult::error(json!({
                            "error": format!("tool '{}' execution task failed: {error}", call.name)
                        })),
                    },
                };
                self.store_tool_result_cache(&call, &result);
                let is_error = self.record_tool_result(call, result);
                if is_error {
                    stats.errors = stats.errors.saturating_add(1);
                }
                stats.total = stats.total.saturating_add(1);
            }
        }
        Ok(stats)
    }

    fn spawn_tool_call_task(&self, call: ToolCall) -> tokio::task::JoinHandle<ToolExecutionResult> {
        let registered = self
            .tools
            .get(&call.name)
            .map(|tool| (tool.definition.clone(), Arc::clone(&tool.tool)));
        let tool_timeout = timeout_duration_from_ms(self.config.tool_timeout_ms);
        let cancellation_token = self.cancellation_token();
        tokio::spawn(async move {
            execute_tool_call_inner(call, registered, tool_timeout, cancellation_token).await
        })
    }

    async fn execute_tool_call(&mut self, call: ToolCall) -> bool {
        if self.is_cancelled() {
            return true;
        }
        self.emit(AgentEvent::ToolExecutionStart {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        });

        let result = if let Some(cached) = self.lookup_tool_result_cache(&call) {
            cached
        } else {
            let result = match self.spawn_tool_call_task(call.clone()).await {
                Ok(result) => result,
                Err(error) => ToolExecutionResult::error(json!({
                    "error": format!("tool '{}' execution task failed: {error}", call.name)
                })),
            };
            self.store_tool_result_cache(&call, &result);
            result
        };
        self.record_tool_result(call, result)
    }

    fn record_tool_result(&mut self, call: ToolCall, result: ToolExecutionResult) -> bool {
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
        result.is_error
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
            tau_ai::ContentBlock::Image { source } => {
                total = total.saturating_add(estimate_media_source_tokens(source));
                total = total.saturating_add(8);
            }
            tau_ai::ContentBlock::Audio { source } => {
                total = total.saturating_add(estimate_media_source_tokens(source));
                total = total.saturating_add(8);
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

fn estimate_media_source_tokens(source: &tau_ai::MediaSource) -> u32 {
    match source {
        tau_ai::MediaSource::Url { url } => estimate_text_tokens(url),
        tau_ai::MediaSource::Base64 { mime_type, data } => estimate_text_tokens(mime_type)
            .saturating_add((data.len() as u32).saturating_div(3))
            .saturating_add(2),
    }
}

fn estimate_text_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    chars.saturating_add(3) / 4
}

fn estimate_usage_cost_usd(
    usage: &ChatUsage,
    input_cost_per_million: Option<f64>,
    output_cost_per_million: Option<f64>,
) -> f64 {
    let input = input_cost_per_million
        .unwrap_or(0.0)
        .max(0.0)
        .mul_add(usage.input_tokens as f64, 0.0)
        / 1_000_000.0;
    let output = output_cost_per_million
        .unwrap_or(0.0)
        .max(0.0)
        .mul_add(usage.output_tokens as f64, 0.0)
        / 1_000_000.0;
    input + output
}

fn normalize_cost_alert_thresholds(thresholds: &[u8]) -> Vec<u8> {
    let mut normalized = thresholds
        .iter()
        .copied()
        .filter(|threshold| (1..=100).contains(threshold))
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        normalized.push(100);
    }
    normalized.sort_unstable();
    normalized.dedup();
    normalized
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

fn assistant_text_suggests_failure(text: &str) -> bool {
    let normalized = collapse_whitespace(&text.to_lowercase());
    if normalized.trim().is_empty() {
        return true;
    }
    FAILURE_SIGNAL_PHRASES
        .iter()
        .any(|phrase| normalized.contains(phrase))
}

fn retrieve_memory_matches(
    history: &[Message],
    query: &str,
    limit: usize,
    dimensions: usize,
    min_similarity: f32,
) -> Vec<MemoryRecallMatch> {
    if limit == 0 {
        return Vec::new();
    }
    let query_embedding = embed_text_vector(query, dimensions);
    if query_embedding.iter().all(|component| *component == 0.0) {
        return Vec::new();
    }

    let mut matches = history
        .iter()
        .filter_map(|message| match message.role {
            MessageRole::User | MessageRole::Assistant => {
                let text = message.text_content();
                if text.trim().is_empty() {
                    return None;
                }
                let candidate_embedding = embed_text_vector(&text, dimensions);
                let score = cosine_similarity(&query_embedding, &candidate_embedding);
                if score >= min_similarity {
                    Some(MemoryRecallMatch {
                        score,
                        role: message.role,
                        text,
                    })
                } else {
                    None
                }
            }
            MessageRole::Tool | MessageRole::System => None,
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| right.score.total_cmp(&left.score));
    matches.truncate(limit);
    matches
}

fn embed_text_vector(text: &str, dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(1);
    let mut vector = vec![0.0f32; dimensions];
    for raw_token in text.split(|character: char| !character.is_alphanumeric()) {
        if raw_token.is_empty() {
            continue;
        }
        let token = raw_token.to_ascii_lowercase();
        let hash = fnv1a_hash(token.as_bytes());
        let index = (hash as usize) % dimensions;
        let sign = if (hash & 1) == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }

    let magnitude = vector
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if magnitude > 0.0 {
        for component in &mut vector {
            *component /= magnitude;
        }
    }
    vector
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() {
        return 0.0;
    }
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
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
    cancellation_token: Option<CooperativeCancellationToken>,
) -> ToolExecutionResult {
    if cancellation_token
        .as_ref()
        .map(CooperativeCancellationToken::is_cancelled)
        .unwrap_or(false)
    {
        return ToolExecutionResult::error(json!({
            "error": format!("tool '{}' cancelled before execution", call.name)
        }));
    }

    if let Some((definition, tool)) = registered {
        if let Err(error) = validate_tool_arguments(&definition, &call.arguments) {
            return ToolExecutionResult::error(json!({ "error": error }));
        }

        let tool_name = definition.name.clone();
        let execution = async move {
            if let Some(timeout) = tool_timeout {
                match tokio::time::timeout(timeout, tool.execute(call.arguments)).await {
                    Ok(result) => result,
                    Err(_) => ToolExecutionResult::error(json!({
                        "error": format!(
                            "tool '{}' timed out after {}ms",
                            tool_name,
                            timeout.as_millis()
                        )
                    })),
                }
            } else {
                tool.execute(call.arguments).await
            }
        };

        if let Some(token) = cancellation_token {
            tokio::select! {
                _ = token.cancelled() => ToolExecutionResult::error(json!({
                    "error": format!("tool '{}' cancelled", definition.name)
                })),
                result = execution => result,
            }
        } else {
            execution.await
        }
    } else {
        ToolExecutionResult::error(json!({
            "error": format!("Tool '{}' is not registered", call.name)
        }))
    }
}

async fn sleep_with_cancellation(
    delay: Duration,
    cancellation_token: Option<CooperativeCancellationToken>,
) -> Result<(), AgentError> {
    if let Some(token) = cancellation_token {
        tokio::select! {
            _ = token.cancelled() => Err(AgentError::Cancelled),
            _ = tokio::time::sleep(delay) => Ok(()),
        }
    } else {
        tokio::time::sleep(delay).await;
        Ok(())
    }
}

fn spawn_async_event_handler_worker(
    receiver: std::sync::mpsc::Receiver<AgentEvent>,
    handler: AsyncEventHandler,
    timeout: Option<Duration>,
    metrics: Arc<AsyncEventDispatchMetricsInner>,
) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };

        while let Ok(event) = receiver.recv() {
            let handler = Arc::clone(&handler);
            let metrics = Arc::clone(&metrics);
            runtime.block_on(async move {
                let mut task = tokio::spawn(async move { (handler)(event).await });
                if let Some(timeout) = timeout {
                    match tokio::time::timeout(timeout, &mut task).await {
                        Ok(Ok(())) => {
                            metrics.completed.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(Err(_)) => {
                            metrics.panicked.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            task.abort();
                            let _ = task.await;
                            metrics.timed_out.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                } else {
                    match task.await {
                        Ok(()) => {
                            metrics.completed.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            metrics.panicked.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
        }
    });
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn cache_insert_with_limit<T: Clone>(
    cache: &mut HashMap<String, T>,
    order: &mut VecDeque<String>,
    key: String,
    value: T,
    max_entries: usize,
) {
    if max_entries == 0 {
        return;
    }
    if let Some(position) = order.iter().position(|entry| entry == &key) {
        order.remove(position);
    }
    order.push_back(key.clone());
    cache.insert(key, value);

    while cache.len() > max_entries {
        let Some(oldest) = order.pop_front() else {
            break;
        };
        cache.remove(&oldest);
    }
}

#[derive(Default)]
struct StreamingRetryBufferState {
    delivered_output: String,
    attempt_output: String,
}

impl StreamingRetryBufferState {
    fn reset_attempt(&mut self) {
        self.attempt_output.clear();
    }
}

fn stream_retry_buffer_on_delta(
    state: &mut StreamingRetryBufferState,
    delta: &str,
) -> Option<String> {
    state.attempt_output.push_str(delta);
    if state.delivered_output.is_empty() {
        state.delivered_output.push_str(delta);
        return Some(delta.to_string());
    }

    if state.attempt_output.len() <= state.delivered_output.len() {
        if state.delivered_output.starts_with(&state.attempt_output) {
            return None;
        }
        state.delivered_output.push_str(delta);
        return Some(delta.to_string());
    }

    if state.attempt_output.starts_with(&state.delivered_output) {
        let replay = state
            .attempt_output
            .get(state.delivered_output.len()..)
            .unwrap_or_default()
            .to_string();
        if replay.is_empty() {
            return None;
        }
        state.delivered_output.push_str(&replay);
        return Some(replay);
    }

    state.delivered_output.push_str(delta);
    Some(delta.to_string())
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

fn normalize_direct_message_content(
    content: &str,
    max_message_chars: usize,
) -> Result<String, AgentDirectMessageError> {
    let normalized = content.trim();
    if normalized.is_empty() {
        return Err(AgentDirectMessageError::EmptyContent);
    }
    let actual_chars = normalized.chars().count();
    if actual_chars > max_message_chars {
        return Err(AgentDirectMessageError::MessageTooLong {
            actual_chars,
            max_chars: max_message_chars,
        });
    }
    Ok(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, VecDeque},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::{Duration, Instant},
    };

    use async_trait::async_trait;
    use tau_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, Message, MessageRole, ToolChoice,
        ToolDefinition,
    };
    use tokio::sync::Mutex as AsyncMutex;

    use crate::{
        assistant_text_suggests_failure, bounded_messages, build_structured_output_retry_prompt,
        cache_insert_with_limit, embed_text_vector, estimate_chat_request_tokens,
        estimate_usage_cost_usd, extract_json_payload, normalize_cost_alert_thresholds,
        retrieve_memory_matches, stream_retry_buffer_on_delta, truncate_chars, Agent, AgentConfig,
        AgentDirectMessageError, AgentDirectMessagePolicy, AgentError, AgentEvent, AgentTool,
        AsyncEventDispatchMetrics, CooperativeCancellationToken, StreamingRetryBufferState,
        ToolExecutionResult, CONTEXT_SUMMARY_MAX_CHARS, CONTEXT_SUMMARY_PREFIX,
        DIRECT_MESSAGE_PREFIX, MEMORY_RECALL_PREFIX,
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

    struct RetryingStreamingOutcome {
        deltas: Vec<String>,
        response: Result<ChatResponse, tau_ai::TauAiError>,
    }

    struct RetryingStreamingClient {
        outcomes: AsyncMutex<VecDeque<RetryingStreamingOutcome>>,
        attempts: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl tau_ai::LlmClient for RetryingStreamingClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.complete_with_stream(request, None).await
        }

        async fn complete_with_stream(
            &self,
            _request: ChatRequest,
            on_delta: Option<tau_ai::StreamDeltaHandler>,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            let outcome = self.outcomes.lock().await.pop_front().ok_or_else(|| {
                tau_ai::TauAiError::InvalidResponse("stream queue empty".to_string())
            })?;

            if let Some(handler) = on_delta {
                for delta in outcome.deltas {
                    handler(delta);
                }
            }

            outcome.response
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

    struct CountingStaticClient {
        calls: Arc<AtomicUsize>,
        response: ChatResponse,
    }

    #[async_trait]
    impl tau_ai::LlmClient for CountingStaticClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.complete_with_stream(request, None).await
        }

        async fn complete_with_stream(
            &self,
            _request: ChatRequest,
            _on_delta: Option<tau_ai::StreamDeltaHandler>,
        ) -> Result<ChatResponse, tau_ai::TauAiError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(self.response.clone())
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

    struct CountingReadTool {
        calls: Arc<AtomicUsize>,
        cacheable: bool,
    }

    #[async_trait]
    impl AgentTool for CountingReadTool {
        fn definition(&self) -> tau_ai::ToolDefinition {
            tau_ai::ToolDefinition {
                name: "counting_read".to_string(),
                description: "Reads and counts invocations".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }
        }

        fn is_cacheable(&self) -> bool {
            self.cacheable
        }

        async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let path = arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>");
            ToolExecutionResult::ok(serde_json::json!({ "content": format!("counting:{path}") }))
        }
    }

    struct CacheableVariantTool {
        label: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AgentTool for CacheableVariantTool {
        fn definition(&self) -> tau_ai::ToolDefinition {
            tau_ai::ToolDefinition {
                name: "cacheable_read".to_string(),
                description: "Cacheable read variant".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }),
            }
        }

        fn is_cacheable(&self) -> bool {
            true
        }

        async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let path = arguments
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>");
            ToolExecutionResult::ok(
                serde_json::json!({ "content": format!("{}:{path}", self.label) }),
            )
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
        assert_eq!(config.agent_id, "tau-agent");
        assert_eq!(config.request_timeout_ms, Some(120_000));
        assert_eq!(config.tool_timeout_ms, Some(120_000));
        assert!(config.stream_retry_with_buffering);
        assert_eq!(config.max_estimated_input_tokens, Some(120_000));
        assert_eq!(config.max_estimated_total_tokens, None);
        assert_eq!(config.structured_output_max_retries, 1);
        assert_eq!(config.react_max_replans_on_tool_failure, 1);
        assert_eq!(config.memory_retrieval_limit, 3);
        assert_eq!(config.memory_embedding_dimensions, 128);
        assert_eq!(config.memory_min_similarity, 0.55);
        assert_eq!(config.memory_max_chars_per_item, 180);
        assert!(config.response_cache_enabled);
        assert_eq!(config.response_cache_max_entries, 128);
        assert!(config.tool_result_cache_enabled);
        assert_eq!(config.tool_result_cache_max_entries, 256);
        assert_eq!(config.model_input_cost_per_million, None);
        assert_eq!(config.model_output_cost_per_million, None);
        assert_eq!(config.cost_budget_usd, None);
        assert_eq!(config.cost_alert_thresholds_percent, vec![80, 100]);
        assert_eq!(config.async_event_queue_capacity, 128);
        assert_eq!(config.async_event_handler_timeout_ms, Some(5_000));
        assert!(!config.async_event_block_on_full);
    }

    #[test]
    fn unit_cache_insert_with_limit_evicts_oldest_entries() {
        let mut cache = HashMap::new();
        let mut order = VecDeque::new();
        cache_insert_with_limit(&mut cache, &mut order, "a".to_string(), 1_u32, 2);
        cache_insert_with_limit(&mut cache, &mut order, "b".to_string(), 2_u32, 2);
        cache_insert_with_limit(&mut cache, &mut order, "c".to_string(), 3_u32, 2);

        assert_eq!(cache.len(), 2);
        assert!(!cache.contains_key("a"));
        assert_eq!(cache.get("b"), Some(&2));
        assert_eq!(cache.get("c"), Some(&3));
    }

    #[test]
    fn unit_stream_retry_buffer_on_delta_suppresses_replayed_prefix() {
        let mut state = StreamingRetryBufferState::default();
        assert_eq!(
            stream_retry_buffer_on_delta(&mut state, "Hel"),
            Some("Hel".to_string())
        );

        state.reset_attempt();
        assert_eq!(stream_retry_buffer_on_delta(&mut state, "Hel"), None);
        assert_eq!(
            stream_retry_buffer_on_delta(&mut state, "lo"),
            Some("lo".to_string())
        );
    }

    #[test]
    fn unit_dynamic_tool_registry_supports_presence_and_lifecycle_helpers() {
        let mut agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
        assert!(!agent.has_tool("read"));
        assert!(!agent.unregister_tool("read"));

        agent.register_tool(ReadTool);
        assert!(agent.has_tool("read"));
        assert_eq!(agent.registered_tool_names(), vec!["read".to_string()]);

        assert!(agent.unregister_tool("read"));
        assert!(!agent.has_tool("read"));

        agent.register_tool(ReadTool);
        agent.clear_tools();
        assert!(agent.registered_tool_names().is_empty());
    }

    #[test]
    fn unit_direct_message_policy_enforces_configured_routes() {
        let mut policy = AgentDirectMessagePolicy::default();
        assert!(!policy.allows("planner", "executor"));
        assert!(!policy.allows("planner", "planner"));

        policy.allow_route("planner", "executor");
        assert!(policy.allows("planner", "executor"));
        assert!(!policy.allows("executor", "planner"));

        policy.allow_bidirectional_route("reviewer", "executor");
        assert!(policy.allows("reviewer", "executor"));
        assert!(policy.allows("executor", "reviewer"));

        policy.allow_self_messages = true;
        assert!(policy.allows("planner", "planner"));
    }

    #[test]
    fn functional_send_direct_message_appends_system_message() {
        let sender = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "planner".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut recipient = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "executor".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut policy = AgentDirectMessagePolicy::default();
        policy.allow_route("planner", "executor");

        sender
            .send_direct_message(&mut recipient, "  review this step  ", &policy)
            .expect("direct message should be accepted");

        let direct_message = recipient
            .messages()
            .iter()
            .find(|message| {
                message.role == MessageRole::System
                    && message.text_content().starts_with(DIRECT_MESSAGE_PREFIX)
            })
            .expect("direct message should be appended as a system message");
        assert!(direct_message
            .text_content()
            .contains("from=planner to=executor"));
        assert!(direct_message.text_content().contains("review this step"));
    }

    #[tokio::test]
    async fn integration_direct_message_is_included_in_recipient_prompt_context() {
        let sender = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "planner".to_string(),
                ..AgentConfig::default()
            },
        );
        let recipient_client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ack"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut recipient = Agent::new(
            recipient_client.clone(),
            AgentConfig {
                agent_id: "executor".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut policy = AgentDirectMessagePolicy::default();
        policy.allow_route("planner", "executor");

        sender
            .send_direct_message(&mut recipient, "Focus on retry semantics", &policy)
            .expect("route should be authorized");
        let _ = recipient
            .prompt("continue")
            .await
            .expect("recipient prompt should succeed");

        let requests = recipient_client.requests.lock().await;
        let request = requests.first().expect("captured request");
        assert!(
            request.messages.iter().any(|message| {
                message.role == MessageRole::System
                    && message
                        .text_content()
                        .contains("[Tau direct message] from=planner to=executor")
            }),
            "direct message should be included in prompt context"
        );
    }

    #[test]
    fn regression_unauthorized_direct_message_fails_closed_without_mutation() {
        let sender = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "planner".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut recipient = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "executor".to_string(),
                ..AgentConfig::default()
            },
        );
        let policy = AgentDirectMessagePolicy::default();
        let baseline_count = recipient.messages().len();

        let error = sender
            .send_direct_message(&mut recipient, "unauthorized", &policy)
            .expect_err("unauthorized route must fail closed");
        assert!(matches!(
            error,
            AgentDirectMessageError::UnauthorizedRoute { .. }
        ));
        assert_eq!(recipient.messages().len(), baseline_count);
    }

    #[test]
    fn regression_direct_message_policy_enforces_max_message_chars() {
        let sender = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "planner".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut recipient = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                agent_id: "executor".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut policy = AgentDirectMessagePolicy::default();
        policy.allow_route("planner", "executor");
        policy.max_message_chars = 5;
        let baseline_count = recipient.messages().len();

        let error = sender
            .send_direct_message(&mut recipient, "message too long", &policy)
            .expect_err("oversized direct message must fail");
        assert!(matches!(
            error,
            AgentDirectMessageError::MessageTooLong { .. }
        ));
        assert_eq!(recipient.messages().len(), baseline_count);
    }

    #[tokio::test]
    async fn functional_with_scoped_tool_registers_within_scope_and_restores_after() {
        let mut agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
        assert!(!agent.has_tool("read"));

        let value = agent
            .with_scoped_tool(ReadTool, |agent| {
                Box::pin(async move {
                    assert!(agent.has_tool("read"));
                    assert_eq!(agent.registered_tool_names(), vec!["read".to_string()]);
                    42usize
                })
            })
            .await;

        assert_eq!(value, 42);
        assert!(!agent.has_tool("read"));
    }

    #[tokio::test]
    async fn unit_cooperative_cancellation_token_signals_waiters() {
        let token = CooperativeCancellationToken::new();
        let waiter = token.clone();
        let task = tokio::spawn(async move {
            waiter.cancelled().await;
            1usize
        });

        tokio::time::sleep(Duration::from_millis(5)).await;
        token.cancel();

        assert!(token.is_cancelled());
        assert_eq!(task.await.expect("waiter task should complete"), 1);
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
            tool_choice: Some(ToolChoice::Auto),
            json_mode: false,
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

    #[test]
    fn functional_estimate_chat_request_tokens_accounts_for_media_blocks() {
        let baseline = ChatRequest {
            model: "openai/gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            tool_choice: None,
            json_mode: false,
            max_tokens: Some(32),
            temperature: None,
        };
        let with_media = ChatRequest {
            model: "openai/gpt-4o-mini".to_string(),
            messages: vec![Message {
                role: MessageRole::User,
                content: vec![
                    ContentBlock::text("hello"),
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
            max_tokens: Some(32),
            temperature: None,
        };

        let baseline_estimate = estimate_chat_request_tokens(&baseline);
        let media_estimate = estimate_chat_request_tokens(&with_media);
        assert!(media_estimate.input_tokens > baseline_estimate.input_tokens);
        assert!(media_estimate.total_tokens > baseline_estimate.total_tokens);
    }

    #[tokio::test]
    async fn functional_prompt_returns_cancelled_when_token_is_pre_cancelled() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("should not be used"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(client.clone(), AgentConfig::default());
        let token = CooperativeCancellationToken::new();
        token.cancel();
        agent.set_cancellation_token(Some(token));

        let error = agent
            .prompt("hello")
            .await
            .expect_err("prompt should cancel");
        assert!(matches!(error, AgentError::Cancelled));
        assert_eq!(client.requests.lock().await.len(), 0);
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
    async fn functional_response_cache_reuses_model_response_for_identical_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Arc::new(CountingStaticClient {
            calls: calls.clone(),
            response: ChatResponse {
                message: Message::assistant_text("cached"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        });

        let mut agent = Agent::new(client, AgentConfig::default());
        agent.append_message(Message::user("cache me"));
        let baseline_messages = agent.messages.clone();
        let start_index = baseline_messages.len().saturating_sub(1);

        let _ = agent
            .run_loop(start_index, None, false)
            .await
            .expect("first run should succeed");
        agent.messages = baseline_messages;
        let _ = agent
            .run_loop(start_index, None, false)
            .await
            .expect("second run should succeed");

        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn regression_response_cache_disabled_dispatches_each_time() {
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Arc::new(CountingStaticClient {
            calls: calls.clone(),
            response: ChatResponse {
                message: Message::assistant_text("uncached"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                response_cache_enabled: false,
                ..AgentConfig::default()
            },
        );
        agent.append_message(Message::user("cache disabled"));
        let baseline_messages = agent.messages.clone();
        let start_index = baseline_messages.len().saturating_sub(1);

        let _ = agent
            .run_loop(start_index, None, false)
            .await
            .expect("first run should succeed");
        agent.messages = baseline_messages;
        let _ = agent
            .run_loop(start_index, None, false)
            .await
            .expect("second run should succeed");

        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn regression_streaming_requests_bypass_response_cache() {
        let calls = Arc::new(AtomicUsize::new(0));
        let client = Arc::new(CountingStaticClient {
            calls: calls.clone(),
            response: ChatResponse {
                message: Message::assistant_text("streamed"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        agent.append_message(Message::user("streaming cache bypass"));
        let baseline_messages = agent.messages.clone();
        let start_index = baseline_messages.len().saturating_sub(1);
        let sink = Arc::new(|_delta: String| {});

        let _ = agent
            .run_loop(start_index, Some(sink.clone()), false)
            .await
            .expect("first streamed run should succeed");
        agent.messages = baseline_messages;
        let _ = agent
            .run_loop(start_index, Some(sink), false)
            .await
            .expect("second streamed run should succeed");

        assert_eq!(calls.load(Ordering::Relaxed), 2);
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
    async fn integration_scoped_tool_lifecycle_supports_prompt_execution() {
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
        assert!(!agent.has_tool("read"));

        let messages = agent
            .with_scoped_tool(ReadTool, |agent| {
                Box::pin(async move { agent.prompt("read").await })
            })
            .await
            .expect("scoped tool prompt should succeed");

        assert!(
            messages
                .iter()
                .any(|message| message.role == MessageRole::Tool),
            "scoped tool should be available while running the closure"
        );
        assert!(!agent.has_tool("read"));
    }

    #[tokio::test]
    async fn regression_scoped_tool_restores_replaced_tool_and_avoids_stale_cache() {
        let make_tool_call = |id: &str| {
            Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: id.to_string(),
                name: "cacheable_read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
            }])
        };
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: make_tool_call("call_1"),
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("base pass 1"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: make_tool_call("call_2"),
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("scoped pass"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: make_tool_call("call_3"),
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: Message::assistant_text("base pass 2"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });

        let base_calls = Arc::new(AtomicUsize::new(0));
        let scoped_calls = Arc::new(AtomicUsize::new(0));
        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(CacheableVariantTool {
            label: "base",
            calls: base_calls.clone(),
        });

        let first = agent
            .prompt("first run")
            .await
            .expect("base tool run should succeed");
        let first_tool = first
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("first tool result");
        assert!(first_tool.text_content().contains("base:a.txt"));

        let second = agent
            .with_scoped_tool(
                CacheableVariantTool {
                    label: "scoped",
                    calls: scoped_calls.clone(),
                },
                |agent| Box::pin(async move { agent.prompt("second run").await }),
            )
            .await
            .expect("scoped tool run should succeed");
        let second_tool = second
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("second tool result");
        assert!(second_tool.text_content().contains("scoped:a.txt"));

        let third = agent
            .prompt("third run")
            .await
            .expect("restored base tool run should succeed");
        let third_tool = third
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("third tool result");
        assert!(third_tool.text_content().contains("base:a.txt"));

        assert_eq!(base_calls.load(Ordering::Relaxed), 2);
        assert_eq!(scoped_calls.load(Ordering::Relaxed), 1);
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
                AgentEvent::ReplanTriggered { turn, .. } => format!("replan:{turn}"),
                AgentEvent::CostUpdated { turn, .. } => format!("cost:{turn}"),
                AgentEvent::CostBudgetAlert {
                    threshold_percent, ..
                } => format!("cost_alert:{threshold_percent}"),
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

    #[test]
    fn unit_async_event_metrics_default_to_zero() {
        let agent = Agent::new(Arc::new(EchoClient), AgentConfig::default());
        assert_eq!(
            agent.async_event_metrics(),
            AsyncEventDispatchMetrics::default()
        );
    }

    #[tokio::test]
    async fn functional_async_subscriber_receives_events_and_records_metrics() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        let observed = Arc::new(AtomicUsize::new(0));
        let observed_clone = observed.clone();
        agent.subscribe_async(move |_event| {
            let observed_clone = observed_clone.clone();
            async move {
                observed_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        let _ = agent.prompt("hello").await.expect("prompt should succeed");
        let deadline = tokio::time::Instant::now() + Duration::from_millis(250);
        while tokio::time::Instant::now() < deadline {
            let metrics = agent.async_event_metrics();
            if observed.load(Ordering::Relaxed) > 0 && metrics.completed > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            observed.load(Ordering::Relaxed) > 0,
            "async handler should observe at least one event"
        );
        let metrics = agent.async_event_metrics();
        assert!(metrics.enqueued > 0);
        assert!(metrics.completed > 0);
        assert_eq!(metrics.dropped_full, 0);
    }

    #[tokio::test]
    async fn integration_async_subscriber_backpressure_drops_when_queue_is_full() {
        let mut agent = Agent::new(
            Arc::new(EchoClient),
            AgentConfig {
                async_event_queue_capacity: 1,
                async_event_block_on_full: false,
                async_event_handler_timeout_ms: None,
                ..AgentConfig::default()
            },
        );
        agent.subscribe_async(|_event| async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
        });

        for _ in 0..20 {
            agent.emit(AgentEvent::AgentStart);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        let metrics = agent.async_event_metrics();
        assert!(metrics.enqueued >= 1);
        assert!(metrics.dropped_full > 0);
    }

    #[tokio::test]
    async fn regression_async_subscriber_timeout_and_panic_are_isolated() {
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
                async_event_queue_capacity: 16,
                async_event_handler_timeout_ms: Some(20),
                ..AgentConfig::default()
            },
        );
        agent.subscribe_async(|_event| async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        agent.subscribe_async(|_event| async move {
            panic!("forced async handler panic");
        });

        let _ = agent
            .prompt("trigger async handlers")
            .await
            .expect("prompt should remain healthy");
        tokio::time::sleep(Duration::from_millis(250)).await;
        let metrics = agent.async_event_metrics();
        assert!(metrics.timed_out > 0);
        assert!(metrics.panicked > 0);
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
    async fn functional_streaming_retry_replays_buffer_without_duplicate_output() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let client = Arc::new(RetryingStreamingClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                RetryingStreamingOutcome {
                    deltas: vec!["Hel".to_string()],
                    response: Err(tau_ai::TauAiError::HttpStatus {
                        status: 503,
                        body: "transient".to_string(),
                    }),
                },
                RetryingStreamingOutcome {
                    deltas: vec!["Hello".to_string()],
                    response: Ok(ChatResponse {
                        message: Message::assistant_text("Hello"),
                        finish_reason: Some("stop".to_string()),
                        usage: ChatUsage::default(),
                    }),
                },
            ])),
            attempts: attempts.clone(),
        });

        let mut agent = Agent::new(
            client,
            AgentConfig {
                request_max_retries: 1,
                request_retry_initial_backoff_ms: 1,
                request_retry_max_backoff_ms: 1,
                stream_retry_with_buffering: true,
                ..AgentConfig::default()
            },
        );
        let streamed = Arc::new(Mutex::new(String::new()));
        let streamed_sink = streamed.clone();
        let sink = Arc::new(move |delta: String| {
            streamed_sink
                .lock()
                .expect("stream lock")
                .push_str(delta.as_str());
        });

        let messages = agent
            .prompt_with_stream("hello", Some(sink))
            .await
            .expect("retrying stream should succeed");

        assert_eq!(messages.last().expect("assistant").text_content(), "Hello");
        assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hello");
        assert_eq!(attempts.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn integration_streaming_retry_with_buffering_continues_tool_turns() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let first_turn_retry = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let final_assistant = Message::assistant_text("done");
        let client = Arc::new(RetryingStreamingClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                RetryingStreamingOutcome {
                    deltas: vec!["To".to_string()],
                    response: Err(tau_ai::TauAiError::HttpStatus {
                        status: 503,
                        body: "temporary".to_string(),
                    }),
                },
                RetryingStreamingOutcome {
                    deltas: vec!["Tool ".to_string()],
                    response: Ok(ChatResponse {
                        message: first_turn_retry,
                        finish_reason: Some("tool_calls".to_string()),
                        usage: ChatUsage::default(),
                    }),
                },
                RetryingStreamingOutcome {
                    deltas: vec!["done".to_string()],
                    response: Ok(ChatResponse {
                        message: final_assistant,
                        finish_reason: Some("stop".to_string()),
                        usage: ChatUsage::default(),
                    }),
                },
            ])),
            attempts: attempts.clone(),
        });

        let mut agent = Agent::new(
            client,
            AgentConfig {
                request_max_retries: 1,
                request_retry_initial_backoff_ms: 1,
                request_retry_max_backoff_ms: 1,
                stream_retry_with_buffering: true,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(ReadTool);

        let streamed = Arc::new(Mutex::new(String::new()));
        let streamed_sink = streamed.clone();
        let sink = Arc::new(move |delta: String| {
            streamed_sink
                .lock()
                .expect("stream lock")
                .push_str(delta.as_str());
        });

        let messages = agent
            .prompt_with_stream("read file", Some(sink))
            .await
            .expect("streaming retry with tools should succeed");

        assert_eq!(messages.last().expect("assistant").text_content(), "done");
        assert!(
            messages
                .iter()
                .any(|message| message.role == MessageRole::Tool),
            "tool turn should still execute after a retried streaming failure"
        );
        assert_eq!(streamed.lock().expect("stream lock").as_str(), "Tool done");
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn regression_streaming_retry_disabled_fails_without_retrying_stream() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let client = Arc::new(RetryingStreamingClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                RetryingStreamingOutcome {
                    deltas: vec!["Hel".to_string()],
                    response: Err(tau_ai::TauAiError::HttpStatus {
                        status: 503,
                        body: "temporary".to_string(),
                    }),
                },
                RetryingStreamingOutcome {
                    deltas: vec!["Hello".to_string()],
                    response: Ok(ChatResponse {
                        message: Message::assistant_text("Hello"),
                        finish_reason: Some("stop".to_string()),
                        usage: ChatUsage::default(),
                    }),
                },
            ])),
            attempts: attempts.clone(),
        });

        let mut agent = Agent::new(
            client,
            AgentConfig {
                request_max_retries: 1,
                request_retry_initial_backoff_ms: 1,
                request_retry_max_backoff_ms: 1,
                stream_retry_with_buffering: false,
                ..AgentConfig::default()
            },
        );
        let streamed = Arc::new(Mutex::new(String::new()));
        let streamed_sink = streamed.clone();
        let sink = Arc::new(move |delta: String| {
            streamed_sink
                .lock()
                .expect("stream lock")
                .push_str(delta.as_str());
        });

        let error = agent
            .prompt_with_stream("hello", Some(sink))
            .await
            .expect_err("disabled buffering should not retry streaming errors");
        assert!(matches!(
            error,
            AgentError::Ai(tau_ai::TauAiError::HttpStatus { status: 503, .. })
        ));
        assert_eq!(streamed.lock().expect("stream lock").as_str(), "Hel");
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
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
    fn unit_estimate_usage_cost_usd_applies_input_and_output_rates() {
        let usage = ChatUsage {
            input_tokens: 2_000,
            output_tokens: 500,
            total_tokens: 2_500,
        };
        let cost = estimate_usage_cost_usd(&usage, Some(1.5), Some(6.0));
        let expected = (2_000.0 * 1.5 + 500.0 * 6.0) / 1_000_000.0;
        assert!((cost - expected).abs() < 1e-12);
    }

    #[test]
    fn unit_normalize_cost_alert_thresholds_filters_invalid_and_deduplicates() {
        assert_eq!(
            normalize_cost_alert_thresholds(&[0, 80, 80, 120, 100]),
            vec![80, 100]
        );
        assert_eq!(normalize_cost_alert_thresholds(&[]), vec![100]);
    }

    #[tokio::test]
    async fn functional_prompt_emits_cost_update_event_when_model_pricing_present() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 200,
                    output_tokens: 100,
                    total_tokens: 300,
                },
            }])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                model_input_cost_per_million: Some(2.0),
                model_output_cost_per_million: Some(4.0),
                ..AgentConfig::default()
            },
        );
        let observed = Arc::new(Mutex::new(Vec::<(usize, f64, f64, Option<f64>)>::new()));
        let observed_clone = observed.clone();
        agent.subscribe(move |event| {
            if let AgentEvent::CostUpdated {
                turn,
                turn_cost_usd,
                cumulative_cost_usd,
                budget_usd,
            } = event
            {
                observed_clone.lock().expect("events lock").push((
                    *turn,
                    *turn_cost_usd,
                    *cumulative_cost_usd,
                    *budget_usd,
                ));
            }
        });

        let _ = agent
            .prompt("price this run")
            .await
            .expect("prompt should succeed");

        let snapshot = agent.cost_snapshot();
        let expected = (200.0 * 2.0 + 100.0 * 4.0) / 1_000_000.0;
        assert_eq!(snapshot.input_tokens, 200);
        assert_eq!(snapshot.output_tokens, 100);
        assert_eq!(snapshot.total_tokens, 300);
        assert!((snapshot.estimated_cost_usd - expected).abs() < 1e-12);
        assert_eq!(snapshot.budget_usd, None);
        assert_eq!(snapshot.budget_utilization, None);

        let events = observed.lock().expect("events lock").clone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, 1);
        assert!((events[0].1 - expected).abs() < 1e-12);
        assert!((events[0].2 - expected).abs() < 1e-12);
        assert_eq!(events[0].3, None);
    }

    #[tokio::test]
    async fn integration_budget_alerts_emit_once_per_threshold_across_multiple_prompts() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([
                ChatResponse {
                    message: Message::assistant_text("first"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage {
                        input_tokens: 80_000,
                        output_tokens: 0,
                        total_tokens: 80_000,
                    },
                },
                ChatResponse {
                    message: Message::assistant_text("second"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage {
                        input_tokens: 40_000,
                        output_tokens: 0,
                        total_tokens: 40_000,
                    },
                },
                ChatResponse {
                    message: Message::assistant_text("third"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage {
                        input_tokens: 40_000,
                        output_tokens: 0,
                        total_tokens: 40_000,
                    },
                },
            ])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                model_input_cost_per_million: Some(10.0),
                model_output_cost_per_million: Some(0.0),
                cost_budget_usd: Some(1.5),
                cost_alert_thresholds_percent: vec![50, 80, 100],
                ..AgentConfig::default()
            },
        );
        let thresholds = Arc::new(Mutex::new(Vec::<u8>::new()));
        let thresholds_clone = thresholds.clone();
        agent.subscribe(move |event| {
            if let AgentEvent::CostBudgetAlert {
                threshold_percent, ..
            } = event
            {
                thresholds_clone
                    .lock()
                    .expect("threshold lock")
                    .push(*threshold_percent);
            }
        });

        let _ = agent.prompt("step 1").await.expect("first prompt");
        let _ = agent.prompt("step 2").await.expect("second prompt");
        let _ = agent.prompt("step 3").await.expect("third prompt");

        let snapshot = agent.cost_snapshot();
        assert!((snapshot.estimated_cost_usd - 1.6).abs() < 1e-9);
        assert_eq!(snapshot.budget_usd, Some(1.5));
        let utilization = snapshot.budget_utilization.expect("utilization");
        assert!(utilization > 1.0);

        assert_eq!(
            thresholds.lock().expect("threshold lock").as_slice(),
            &[50, 80, 100]
        );
    }

    #[tokio::test]
    async fn regression_cost_budget_alert_threshold_normalization_avoids_duplicates() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 150_000,
                    output_tokens: 0,
                    total_tokens: 150_000,
                },
            }])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                model_input_cost_per_million: Some(10.0),
                cost_budget_usd: Some(1.0),
                cost_alert_thresholds_percent: vec![0, 80, 80, 120, 100],
                ..AgentConfig::default()
            },
        );
        let thresholds = Arc::new(Mutex::new(Vec::<u8>::new()));
        let thresholds_clone = thresholds.clone();
        agent.subscribe(move |event| {
            if let AgentEvent::CostBudgetAlert {
                threshold_percent, ..
            } = event
            {
                thresholds_clone
                    .lock()
                    .expect("threshold lock")
                    .push(*threshold_percent);
            }
        });

        let _ = agent
            .prompt("single run")
            .await
            .expect("prompt should succeed");
        assert_eq!(
            thresholds.lock().expect("threshold lock").as_slice(),
            &[80, 100]
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
    fn unit_assistant_text_suggests_failure_matches_common_markers() {
        assert!(assistant_text_suggests_failure(
            "Unable to continue after the error."
        ));
        assert!(assistant_text_suggests_failure(
            "I can't proceed with this tool."
        ));
        assert!(assistant_text_suggests_failure("   "));
        assert!(!assistant_text_suggests_failure("Completed successfully."));
    }

    #[test]
    fn unit_vector_retrieval_prefers_semantically_related_entries() {
        let history = vec![
            Message::user("rust tokio async runtime troubleshooting"),
            Message::assistant_text("pasta recipe with basil and tomato"),
        ];
        let matches = retrieve_memory_matches(&history, "tokio runtime async rust", 1, 64, 0.0);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].text.contains("tokio"));

        let query = embed_text_vector("tokio runtime async rust", 64);
        let related = embed_text_vector("rust tokio async runtime troubleshooting", 64);
        let unrelated = embed_text_vector("pasta recipe with basil and tomato", 64);
        let related_score = query
            .iter()
            .zip(&related)
            .map(|(left, right)| left * right)
            .sum::<f32>();
        let unrelated_score = query
            .iter()
            .zip(&unrelated)
            .map(|(left, right)| left * right)
            .sum::<f32>();
        assert!(related_score > unrelated_score);
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
    async fn functional_prompt_json_enables_provider_json_mode_on_requests() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("{\"ok\":true}"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(client.clone(), AgentConfig::default());
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "ok": { "type": "boolean" }
            },
            "required": ["ok"],
            "additionalProperties": false
        });

        let value = agent
            .prompt_json("return ok", &schema)
            .await
            .expect("structured output should succeed");
        assert_eq!(value["ok"], true);

        let requests = client.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].json_mode);
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
    async fn integration_requests_with_registered_tools_use_auto_tool_choice() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(client.clone(), AgentConfig::default());
        agent.register_tool(ReadTool);

        let _ = agent.prompt("hello").await.expect("prompt should succeed");

        let requests = client.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].tool_choice, Some(ToolChoice::Auto));
        assert!(!requests[0].json_mode);
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
    async fn regression_requests_without_tools_keep_tool_choice_unset() {
        let client = Arc::new(CapturingMockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(client.clone(), AgentConfig::default());

        let _ = agent.prompt("hello").await.expect("prompt should succeed");

        let requests = client.requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].tool_choice, None);
    }

    #[tokio::test]
    async fn integration_tool_execution_cancellation_propagates_as_agent_cancelled() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "slow_read".to_string(),
            arguments: serde_json::json!({
                "path": "README.md"
            }),
        }]);
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: first_assistant,
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        agent.register_tool(SlowReadTool { delay_ms: 500 });
        let token = CooperativeCancellationToken::new();
        agent.set_cancellation_token(Some(token.clone()));

        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            token.cancel();
        });

        let error = agent
            .prompt("read with cancellation")
            .await
            .expect_err("prompt should cancel cooperatively");
        assert!(
            matches!(error, AgentError::Cancelled),
            "expected AgentError::Cancelled, got {error:?}"
        );
        cancel_task.await.expect("cancel task should complete");
    }

    #[tokio::test]
    async fn regression_agent_can_continue_after_cancellation_token_is_cleared() {
        let client = Arc::new(MockClient {
            responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            }])),
        });
        let mut agent = Agent::new(client, AgentConfig::default());
        let token = CooperativeCancellationToken::new();
        token.cancel();
        agent.set_cancellation_token(Some(token));

        let error = agent
            .prompt("cancelled run")
            .await
            .expect_err("first prompt should be cancelled");
        assert!(matches!(error, AgentError::Cancelled));

        agent.set_cancellation_token(None);
        let new_messages = agent
            .prompt("second run")
            .await
            .expect("agent should continue once token is cleared");
        assert_eq!(
            new_messages
                .last()
                .expect("assistant response should exist")
                .text_content(),
            "ok"
        );
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
    async fn functional_replan_prompt_injected_after_failed_tool_and_failure_response() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({}),
        }]);
        let second_assistant =
            Message::assistant_text("I cannot continue because the tool failed.");
        let third_assistant = Message::assistant_text("recovered");
        let client = Arc::new(CapturingMockClient {
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
                ChatResponse {
                    message: third_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                react_max_replans_on_tool_failure: 1,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(ReadTool);
        let replan_count = Arc::new(AtomicUsize::new(0));
        let replan_count_sink = replan_count.clone();
        agent.subscribe(move |event| {
            if matches!(event, AgentEvent::ReplanTriggered { .. }) {
                replan_count_sink.fetch_add(1, Ordering::Relaxed);
            }
        });

        let messages = agent
            .prompt("read")
            .await
            .expect("replan flow should recover");
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "recovered"
        );
        assert_eq!(replan_count.load(Ordering::Relaxed), 1);

        let requests = client.requests.lock().await;
        assert_eq!(
            requests.len(),
            3,
            "expected replan to trigger an extra turn"
        );
        let replan_prompt = last_user_prompt(&requests[2]);
        assert!(replan_prompt.contains("One or more tool calls failed"));
    }

    #[tokio::test]
    async fn functional_request_messages_attach_memory_recall_for_relevant_history() {
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
                memory_retrieval_limit: 2,
                memory_embedding_dimensions: 64,
                memory_min_similarity: 0.2,
                ..AgentConfig::default()
            },
        );
        agent.append_message(Message::user(
            "postgres retry configuration for orders service",
        ));
        agent.append_message(Message::assistant_text(
            "increase postgres pool size for orders workloads",
        ));
        agent.append_message(Message::user("cache ttl cleanup"));
        agent.append_message(Message::assistant_text("set ttl to 15m"));

        let _ = agent
            .prompt("postgres orders service retry policy")
            .await
            .expect("prompt should succeed");

        let requests = client.requests.lock().await;
        let first_request = requests.first().expect("request should be captured");
        let recall = first_request
            .messages
            .iter()
            .find(|message| {
                message.role == MessageRole::System
                    && message.text_content().starts_with(MEMORY_RECALL_PREFIX)
            })
            .expect("memory recall system message should be attached");
        assert!(recall.text_content().contains("postgres"));
        assert!(recall.text_content().contains("orders"));
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
    async fn integration_parallel_tool_cache_reuses_results_across_chunks() {
        let first_assistant = Message::assistant_blocks(vec![
            ContentBlock::ToolCall {
                id: "call_1".to_string(),
                name: "counting_read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
            },
            ContentBlock::ToolCall {
                id: "call_2".to_string(),
                name: "counting_read".to_string(),
                arguments: serde_json::json!({ "path": "b.txt" }),
            },
            ContentBlock::ToolCall {
                id: "call_3".to_string(),
                name: "counting_read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
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
        let calls = Arc::new(AtomicUsize::new(0));
        let mut agent = Agent::new(
            client,
            AgentConfig {
                max_parallel_tool_calls: 2,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(CountingReadTool {
            calls: calls.clone(),
            cacheable: true,
        });

        let messages = agent
            .prompt("read all")
            .await
            .expect("prompt should succeed");
        let tool_messages = messages
            .iter()
            .filter(|message| message.role == MessageRole::Tool)
            .collect::<Vec<_>>();
        assert_eq!(tool_messages.len(), 3);
        assert!(tool_messages[0].text_content().contains("counting:a.txt"));
        assert!(tool_messages[1].text_content().contains("counting:b.txt"));
        assert!(tool_messages[2].text_content().contains("counting:a.txt"));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn regression_non_cacheable_tool_executes_each_identical_call() {
        let first_assistant = Message::assistant_blocks(vec![
            ContentBlock::ToolCall {
                id: "call_1".to_string(),
                name: "counting_read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
            },
            ContentBlock::ToolCall {
                id: "call_2".to_string(),
                name: "counting_read".to_string(),
                arguments: serde_json::json!({ "path": "a.txt" }),
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
        let calls = Arc::new(AtomicUsize::new(0));
        let mut agent = Agent::new(
            client,
            AgentConfig {
                max_parallel_tool_calls: 1,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(CountingReadTool {
            calls: calls.clone(),
            cacheable: false,
        });

        let _ = agent
            .prompt("read duplicate")
            .await
            .expect("prompt should succeed");
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn integration_replan_flow_can_recover_with_follow_up_tool_call() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({}),
        }]);
        let second_assistant = Message::assistant_text("Unable to continue after that tool error.");
        let third_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_2".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        }]);
        let fourth_assistant = Message::assistant_text("done after replan");
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
                ChatResponse {
                    message: third_assistant,
                    finish_reason: Some("tool_calls".to_string()),
                    usage: ChatUsage::default(),
                },
                ChatResponse {
                    message: fourth_assistant,
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                },
            ])),
        });
        let mut agent = Agent::new(
            client,
            AgentConfig {
                react_max_replans_on_tool_failure: 1,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(ReadTool);

        let messages = agent
            .prompt("read")
            .await
            .expect("replan flow should recover with second tool call");
        let tool_messages = messages
            .iter()
            .filter(|message| message.role == MessageRole::Tool)
            .collect::<Vec<_>>();
        assert_eq!(tool_messages.len(), 2);
        assert!(tool_messages[0].is_error);
        assert!(!tool_messages[1].is_error);
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "done after replan"
        );
    }

    #[tokio::test]
    async fn integration_memory_recall_ranks_relevant_entries_ahead_of_unrelated_entries() {
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
                max_context_messages: Some(2),
                memory_retrieval_limit: 1,
                memory_embedding_dimensions: 64,
                memory_min_similarity: 0.1,
                ..AgentConfig::default()
            },
        );
        agent.append_message(Message::user("rust tokio runtime diagnostics"));
        agent.append_message(Message::user("pasta recipe tomato basil"));
        agent.append_message(Message::assistant_text("acknowledged"));

        let _ = agent
            .prompt("tokio runtime troubleshooting")
            .await
            .expect("prompt should succeed");

        let requests = client.requests.lock().await;
        let first_request = requests.first().expect("request should be captured");
        let recall = first_request
            .messages
            .iter()
            .find(|message| {
                message.role == MessageRole::System
                    && message.text_content().starts_with(MEMORY_RECALL_PREFIX)
            })
            .expect("memory recall message should exist");
        assert!(recall.text_content().contains("tokio"));
        assert!(!recall.text_content().contains("pasta recipe"));
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
    async fn regression_no_replan_when_assistant_reports_success_after_tool_failure() {
        let first_assistant = Message::assistant_blocks(vec![ContentBlock::ToolCall {
            id: "call_1".to_string(),
            name: "read".to_string(),
            arguments: serde_json::json!({}),
        }]);
        let second_assistant = Message::assistant_text("done");
        let client = Arc::new(CapturingMockClient {
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
            requests: AsyncMutex::new(Vec::new()),
        });
        let mut agent = Agent::new(
            client.clone(),
            AgentConfig {
                react_max_replans_on_tool_failure: 1,
                ..AgentConfig::default()
            },
        );
        agent.register_tool(ReadTool);
        let replan_count = Arc::new(AtomicUsize::new(0));
        let replan_count_sink = replan_count.clone();
        agent.subscribe(move |event| {
            if matches!(event, AgentEvent::ReplanTriggered { .. }) {
                replan_count_sink.fetch_add(1, Ordering::Relaxed);
            }
        });

        let messages = agent
            .prompt("read")
            .await
            .expect("prompt should complete without forced replan");
        assert_eq!(
            messages.last().expect("assistant response").text_content(),
            "done"
        );
        assert_eq!(replan_count.load(Ordering::Relaxed), 0);
        assert_eq!(client.requests.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn regression_memory_recall_disabled_when_limit_is_zero() {
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
                max_context_messages: Some(2),
                memory_retrieval_limit: 0,
                ..AgentConfig::default()
            },
        );
        agent.append_message(Message::user("postgres connection issue"));
        agent.append_message(Message::assistant_text("ack"));
        agent.append_message(Message::user("retry strategy"));

        let _ = agent.prompt("postgres retry policy").await.expect("prompt");
        let requests = client.requests.lock().await;
        let first_request = requests.first().expect("request should be captured");
        assert!(first_request
            .messages
            .iter()
            .all(|message| !message.text_content().starts_with(MEMORY_RECALL_PREFIX)));
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
