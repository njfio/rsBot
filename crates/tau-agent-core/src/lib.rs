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
use tau_memory::runtime::{cosine_similarity, embed_text_vector};
pub use tau_safety::{
    DefaultLeakDetector, DefaultSanitizer, LeakDetector, SafetyMode, SafetyPolicy, SafetyStage,
    Sanitizer,
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
    pub memory_embedding_model: Option<String>,
    pub memory_embedding_api_base: Option<String>,
    pub memory_embedding_api_key: Option<String>,
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
            memory_embedding_model: None,
            memory_embedding_api_base: None,
            memory_embedding_api_key: None,
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
///
/// # Examples
///
/// ```
/// use tau_agent_core::CooperativeCancellationToken;
///
/// let token = CooperativeCancellationToken::new();
/// assert!(!token.is_cancelled());
///
/// token.cancel();
/// assert!(token.is_cancelled());
/// ```
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
    SafetyPolicyApplied {
        stage: SafetyStage,
        mode: SafetyMode,
        blocked: bool,
        matched_rules: Vec<String>,
        reason_codes: Vec<String>,
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
    #[error("safety policy blocked {stage}: reason_codes={reason_codes:?}")]
    SafetyViolation {
        stage: String,
        reason_codes: Vec<String>,
    },
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
    #[error("direct message blocked by safety policy: reason_codes={reason_codes:?}")]
    SafetyViolation { reason_codes: Vec<String> },
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
    ///
    /// # Examples
    ///
    /// ```
    /// use tau_agent_core::AgentDirectMessagePolicy;
    ///
    /// let mut policy = AgentDirectMessagePolicy::default();
    /// policy.allow_bidirectional_route("planner", "executor");
    ///
    /// assert!(policy.allows("planner", "executor"));
    /// assert!(policy.allows("executor", "planner"));
    /// assert!(!policy.allows("planner", "reviewer"));
    /// ```
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
const TOOL_OUTPUT_BLOCKED_ERROR: &str = "tool output blocked by safety policy";
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

#[derive(Debug, Clone)]
struct SafetyInspection {
    mode: SafetyMode,
    redacted_text: String,
    matched_rules: Vec<String>,
    reason_codes: Vec<String>,
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
    safety_policy: SafetyPolicy,
    sanitizer: Arc<dyn Sanitizer>,
    leak_detector: Arc<dyn LeakDetector>,
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
            safety_policy: SafetyPolicy::default(),
            sanitizer: Arc::new(DefaultSanitizer::new()),
            leak_detector: Arc::new(DefaultLeakDetector::new()),
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
    ///
    /// # Examples
    ///
    /// ```
    /// use async_trait::async_trait;
    /// use serde_json::json;
    /// use std::sync::Arc;
    /// use tau_agent_core::{Agent, AgentConfig, AgentTool, ToolExecutionResult};
    /// use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, TauAiError, ToolDefinition};
    ///
    /// struct StaticClient;
    ///
    /// #[async_trait]
    /// impl LlmClient for StaticClient {
    ///     async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
    ///         Ok(ChatResponse {
    ///             message: tau_ai::Message::assistant_text("ok"),
    ///             finish_reason: Some("stop".to_string()),
    ///             usage: ChatUsage::default(),
    ///         })
    ///     }
    /// }
    ///
    /// struct EchoTool;
    ///
    /// #[async_trait]
    /// impl AgentTool for EchoTool {
    ///     fn definition(&self) -> ToolDefinition {
    ///         ToolDefinition {
    ///             name: "echo".to_string(),
    ///             description: "Echoes input text".to_string(),
    ///             parameters: json!({
    ///                 "type": "object",
    ///                 "properties": {
    ///                     "text": { "type": "string" }
    ///                 }
    ///             }),
    ///         }
    ///     }
    ///
    ///     async fn execute(&self, arguments: serde_json::Value) -> ToolExecutionResult {
    ///         ToolExecutionResult::ok(json!({ "echo": arguments }))
    ///     }
    /// }
    ///
    /// let mut agent = Agent::new(Arc::new(StaticClient), AgentConfig::default());
    /// agent.register_tool(EchoTool);
    ///
    /// assert!(agent.has_tool("echo"));
    /// assert_eq!(agent.registered_tool_names(), vec!["echo".to_string()]);
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use async_trait::async_trait;
    /// use std::sync::Arc;
    /// use tau_agent_core::{Agent, AgentConfig, AgentDirectMessagePolicy};
    /// use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, TauAiError};
    ///
    /// struct StaticClient;
    ///
    /// #[async_trait]
    /// impl LlmClient for StaticClient {
    ///     async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
    ///         Ok(ChatResponse {
    ///             message: tau_ai::Message::assistant_text("ok"),
    ///             finish_reason: Some("stop".to_string()),
    ///             usage: ChatUsage::default(),
    ///         })
    ///     }
    /// }
    ///
    /// let mut planner = Agent::new(Arc::new(StaticClient), AgentConfig::default());
    /// planner.set_agent_id("planner");
    ///
    /// let mut executor = Agent::new(Arc::new(StaticClient), AgentConfig::default());
    /// executor.set_agent_id("executor");
    ///
    /// let mut policy = AgentDirectMessagePolicy::default();
    /// policy.allow_route("planner", "executor");
    ///
    /// planner
    ///     .send_direct_message(&mut executor, "check module boundaries", &policy)
    ///     .expect("route is allowed");
    ///
    /// let latest = executor.messages().last().expect("direct message appended");
    /// assert!(latest.text_content().contains("check module boundaries"));
    /// ```
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
        let sanitized_content = self
            .sanitize_inbound_text(normalized_content)
            .map_err(|reason_codes| AgentDirectMessageError::SafetyViolation { reason_codes })?;
        let direct_message = Message::system(format!(
            "{} from={} to={}\n{}",
            DIRECT_MESSAGE_PREFIX, from_agent_id, to_agent_id, sanitized_content
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

    /// Replaces the runtime safety policy.
    pub fn set_safety_policy(&mut self, policy: SafetyPolicy) {
        self.safety_policy = policy;
    }

    /// Returns the active runtime safety policy.
    pub fn safety_policy(&self) -> &SafetyPolicy {
        &self.safety_policy
    }

    /// Replaces the sanitizer implementation.
    pub fn set_sanitizer(&mut self, sanitizer: Arc<dyn Sanitizer>) {
        self.sanitizer = sanitizer;
    }

    /// Replaces the leak detector implementation.
    pub fn set_leak_detector(&mut self, detector: Arc<dyn LeakDetector>) {
        self.leak_detector = detector;
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
        let text = self.sanitize_inbound_text(text).map_err(|reason_codes| {
            AgentError::SafetyViolation {
                stage: SafetyStage::InboundMessage.as_str().to_string(),
                reason_codes,
            }
        })?;
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

    fn prompt_safety_stage_enabled(&self, stage: SafetyStage) -> bool {
        if !self.safety_policy.enabled {
            return false;
        }
        match stage {
            SafetyStage::InboundMessage => self.safety_policy.apply_to_inbound_messages,
            SafetyStage::ToolOutput => self.safety_policy.apply_to_tool_outputs,
            SafetyStage::OutboundHttpPayload => false,
        }
    }

    fn leak_stage_enabled(&self, stage: SafetyStage) -> bool {
        if !self.safety_policy.secret_leak_detection_enabled {
            return false;
        }
        match stage {
            SafetyStage::InboundMessage => false,
            SafetyStage::ToolOutput => self.safety_policy.apply_to_tool_outputs,
            SafetyStage::OutboundHttpPayload => self.safety_policy.apply_to_outbound_http_payloads,
        }
    }

    fn inspect_prompt_safety_text(
        &self,
        stage: SafetyStage,
        text: &str,
    ) -> Option<SafetyInspection> {
        if !self.prompt_safety_stage_enabled(stage) || text.trim().is_empty() {
            return None;
        }
        let scan = self
            .sanitizer
            .scan(text, self.safety_policy.redaction_token.as_str());
        if !scan.has_matches() {
            return None;
        }
        let matched_rules = scan.matched_rule_ids();
        let reason_codes = scan.reason_codes();
        let mode = self.safety_policy.mode;
        self.emit(AgentEvent::SafetyPolicyApplied {
            stage,
            mode,
            blocked: matches!(mode, SafetyMode::Block),
            matched_rules: matched_rules.clone(),
            reason_codes: reason_codes.clone(),
        });
        Some(SafetyInspection {
            mode,
            redacted_text: scan.redacted_text,
            matched_rules,
            reason_codes,
        })
    }

    fn inspect_secret_leak_text(&self, stage: SafetyStage, text: &str) -> Option<SafetyInspection> {
        if !self.leak_stage_enabled(stage) || text.trim().is_empty() {
            return None;
        }
        let scan = self.leak_detector.scan(
            text,
            self.safety_policy.secret_leak_redaction_token.as_str(),
        );
        if !scan.has_matches() {
            return None;
        }
        let matched_rules = scan.matched_rule_ids();
        let reason_codes = scan.reason_codes();
        let mode = self.safety_policy.secret_leak_mode;
        self.emit(AgentEvent::SafetyPolicyApplied {
            stage,
            mode,
            blocked: matches!(mode, SafetyMode::Block),
            matched_rules: matched_rules.clone(),
            reason_codes: reason_codes.clone(),
        });
        Some(SafetyInspection {
            mode,
            redacted_text: scan.redacted_text,
            matched_rules,
            reason_codes,
        })
    }

    fn apply_safety_mode_to_text(
        text: String,
        inspection: &SafetyInspection,
    ) -> Result<String, Vec<String>> {
        match inspection.mode {
            SafetyMode::Warn => Ok(text),
            SafetyMode::Redact => Ok(inspection.redacted_text.clone()),
            SafetyMode::Block => Err(inspection.reason_codes.clone()),
        }
    }

    fn sanitize_inbound_text(&self, text: String) -> Result<String, Vec<String>> {
        let Some(inspection) = self.inspect_prompt_safety_text(SafetyStage::InboundMessage, &text)
        else {
            return Ok(text);
        };
        Self::apply_safety_mode_to_text(text, &inspection)
    }

    fn sanitize_outbound_http_request(&self, request: &mut ChatRequest) -> Result<(), AgentError> {
        let fail_closed_on_unscannable_payload = self
            .leak_stage_enabled(SafetyStage::OutboundHttpPayload)
            && self.safety_policy.secret_leak_mode == SafetyMode::Block;
        if fail_closed_on_unscannable_payload
            && matches!(request.temperature, Some(temperature) if !temperature.is_finite())
        {
            return Err(AgentError::SafetyViolation {
                stage: SafetyStage::OutboundHttpPayload.as_str().to_string(),
                reason_codes: vec!["secret_leak.payload_serialization_failed".to_string()],
            });
        }

        let payload = match serde_json::to_string(request) {
            Ok(payload) => payload,
            Err(_) => {
                if fail_closed_on_unscannable_payload {
                    return Err(AgentError::SafetyViolation {
                        stage: SafetyStage::OutboundHttpPayload.as_str().to_string(),
                        reason_codes: vec!["secret_leak.payload_serialization_failed".to_string()],
                    });
                }
                return Ok(());
            }
        };
        let Some(inspection) =
            self.inspect_secret_leak_text(SafetyStage::OutboundHttpPayload, &payload)
        else {
            return Ok(());
        };
        match inspection.mode {
            SafetyMode::Warn => Ok(()),
            SafetyMode::Redact => {
                let mut reason_codes = inspection.reason_codes.clone();
                let parsed = serde_json::from_str::<ChatRequest>(&inspection.redacted_text)
                    .map_err(|_| {
                        reason_codes.push("secret_leak.redaction_failed".to_string());
                        AgentError::SafetyViolation {
                            stage: SafetyStage::OutboundHttpPayload.as_str().to_string(),
                            reason_codes,
                        }
                    })?;
                *request = parsed;
                Ok(())
            }
            SafetyMode::Block => Err(AgentError::SafetyViolation {
                stage: SafetyStage::OutboundHttpPayload.as_str().to_string(),
                reason_codes: inspection.reason_codes,
            }),
        }
    }

    fn sanitize_tool_result(&self, result: ToolExecutionResult) -> ToolExecutionResult {
        let original_text = result.as_text();
        let mut text = original_text.clone();

        if let Some(inspection) = self.inspect_prompt_safety_text(SafetyStage::ToolOutput, &text) {
            match Self::apply_safety_mode_to_text(text, &inspection) {
                Ok(updated) => {
                    text = updated;
                }
                Err(reason_codes) => {
                    return ToolExecutionResult::error(json!({
                        "error": TOOL_OUTPUT_BLOCKED_ERROR,
                        "stage": SafetyStage::ToolOutput.as_str(),
                        "matched_rules": inspection.matched_rules,
                        "reason_codes": reason_codes,
                    }));
                }
            }
        }

        if let Some(inspection) = self.inspect_secret_leak_text(SafetyStage::ToolOutput, &text) {
            match Self::apply_safety_mode_to_text(text, &inspection) {
                Ok(updated) => {
                    text = updated;
                }
                Err(reason_codes) => {
                    return ToolExecutionResult::error(json!({
                        "error": TOOL_OUTPUT_BLOCKED_ERROR,
                        "stage": SafetyStage::ToolOutput.as_str(),
                        "matched_rules": inspection.matched_rules,
                        "reason_codes": reason_codes,
                    }));
                }
            }
        }

        if text != original_text {
            return ToolExecutionResult {
                content: Value::String(text),
                is_error: result.is_error,
            };
        }
        result
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
            let mut request = ChatRequest {
                model: self.config.model.clone(),
                messages: self.request_messages().await,
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
            self.sanitize_outbound_http_request(&mut request)?;
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

    async fn request_messages(&self) -> Vec<Message> {
        let Some(limit) = self.config.max_context_messages else {
            return self.messages.clone();
        };
        let mut messages = bounded_messages(&self.messages, limit);
        if self.config.memory_retrieval_limit == 0 || self.messages.len() <= limit {
            return messages;
        }
        let Some(recall) = self.build_memory_recall_message(limit).await else {
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

    async fn build_memory_recall_message(&self, context_limit: usize) -> Option<String> {
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
            &self.config,
        )
        .await;
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
        let result = self.sanitize_tool_result(result);
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

#[derive(Debug, Clone)]
struct MemoryEmbeddingApiConfig {
    model: String,
    api_base: String,
    api_key: String,
}

async fn retrieve_memory_matches(
    history: &[Message],
    query: &str,
    limit: usize,
    dimensions: usize,
    min_similarity: f32,
    config: &AgentConfig,
) -> Vec<MemoryRecallMatch> {
    if limit == 0 {
        return Vec::new();
    }
    let candidates = history
        .iter()
        .filter_map(|message| match message.role {
            MessageRole::User | MessageRole::Assistant => {
                let text = message.text_content();
                if text.trim().is_empty() {
                    None
                } else {
                    Some((message.role, text))
                }
            }
            MessageRole::Tool | MessageRole::System => None,
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Vec::new();
    }

    let api_embeddings = if let Some(api_config) = resolve_memory_embedding_api_config(config) {
        let mut inputs = Vec::with_capacity(candidates.len().saturating_add(1));
        inputs.push(query.to_string());
        inputs.extend(candidates.iter().map(|(_, text)| text.clone()));
        match embed_text_vectors_via_api(&inputs, dimensions, &api_config).await {
            Ok(vectors) if vectors.len() == inputs.len() => Some(vectors),
            _ => None,
        }
    } else {
        None
    };

    let (query_embedding, candidate_embeddings) = if let Some(vectors) = api_embeddings {
        let query_embedding = vectors.first().cloned().unwrap_or_default();
        let candidate_embeddings = vectors.into_iter().skip(1).collect::<Vec<_>>();
        (query_embedding, candidate_embeddings)
    } else {
        let query_embedding = embed_text_vector(query, dimensions);
        let candidate_embeddings = candidates
            .iter()
            .map(|(_, text)| embed_text_vector(text, dimensions))
            .collect::<Vec<_>>();
        (query_embedding, candidate_embeddings)
    };
    if query_embedding.iter().all(|component| *component == 0.0) {
        return Vec::new();
    }

    let mut matches = candidates
        .into_iter()
        .zip(candidate_embeddings.into_iter())
        .filter_map(|((role, text), candidate_embedding)| {
            let score = cosine_similarity(&query_embedding, &candidate_embedding);
            if score >= min_similarity {
                Some(MemoryRecallMatch { score, role, text })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    matches.sort_by(|left, right| right.score.total_cmp(&left.score));
    matches.truncate(limit);
    matches
}

fn resolve_memory_embedding_api_config(config: &AgentConfig) -> Option<MemoryEmbeddingApiConfig> {
    let model = config.memory_embedding_model.as_deref()?.trim();
    let api_key = config.memory_embedding_api_key.as_deref()?.trim();
    if model.is_empty() || api_key.is_empty() {
        return None;
    }
    let api_base = config
        .memory_embedding_api_base
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim()
        .trim_end_matches('/')
        .to_string();
    if api_base.is_empty() {
        return None;
    }
    Some(MemoryEmbeddingApiConfig {
        model: model.to_string(),
        api_base,
        api_key: api_key.to_string(),
    })
}

async fn embed_text_vectors_via_api(
    inputs: &[String],
    dimensions: usize,
    config: &MemoryEmbeddingApiConfig,
) -> Result<Vec<Vec<f32>>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/embeddings", config.api_base))
        .bearer_auth(config.api_key.as_str())
        .json(&json!({
            "model": config.model,
            "input": inputs,
        }))
        .send()
        .await
        .map_err(|error| format!("embedding request failed: {error}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "embedding request failed with status {}: {}",
            status.as_u16(),
            truncate_chars(&body, 240)
        ));
    }

    let payload = response
        .json::<Value>()
        .await
        .map_err(|error| format!("failed to parse embedding response json: {error}"))?;
    let data = payload
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "embedding response missing data array".to_string())?;
    if data.len() != inputs.len() {
        return Err(format!(
            "embedding response size mismatch: expected {}, got {}",
            inputs.len(),
            data.len()
        ));
    }

    let mut vectors = Vec::with_capacity(data.len());
    for item in data {
        let raw_embedding = item
            .get("embedding")
            .and_then(Value::as_array)
            .ok_or_else(|| "embedding item missing embedding array".to_string())?;
        let parsed = raw_embedding
            .iter()
            .map(|component| {
                component
                    .as_f64()
                    .map(|value| value as f32)
                    .ok_or_else(|| "embedding component must be numeric".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        vectors.push(resize_and_normalize_embedding(&parsed, dimensions));
    }

    Ok(vectors)
}

fn resize_and_normalize_embedding(values: &[f32], dimensions: usize) -> Vec<f32> {
    let dimensions = dimensions.max(1);
    let mut resized = vec![0.0f32; dimensions];
    for (index, value) in values.iter().enumerate() {
        let bucket = index % dimensions;
        resized[bucket] += *value;
    }

    let magnitude = resized
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if magnitude > 0.0 {
        for component in &mut resized {
            *component /= magnitude;
        }
    }
    resized
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
    use httpmock::prelude::*;
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
        AsyncEventDispatchMetrics, CooperativeCancellationToken, SafetyMode, SafetyPolicy,
        SafetyStage, StreamingRetryBufferState, ToolExecutionResult, CONTEXT_SUMMARY_MAX_CHARS,
        CONTEXT_SUMMARY_PREFIX, DIRECT_MESSAGE_PREFIX, MEMORY_RECALL_PREFIX,
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

    #[path = "config_and_direct_message.rs"]
    mod config_and_direct_message;

    #[path = "async_event_dispatch.rs"]
    mod async_event_dispatch;

    #[path = "streaming_and_budgets.rs"]
    mod streaming_and_budgets;

    #[path = "structured_output_and_parallel.rs"]
    mod structured_output_and_parallel;

    #[path = "safety_pipeline.rs"]
    mod safety_pipeline;
}
