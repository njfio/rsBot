//! Core runtime primitives for building tool-using LLM agents in Tau.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use async_trait::async_trait;
use serde_json::{json, Value};
use tau_ai::{
    ChatRequest, ChatUsage, LlmClient, Message, MessageRole, StreamDeltaHandler, TauAiError,
    ToolCall, ToolChoice, ToolDefinition,
};
pub use tau_memory::runtime::{
    FileMemoryStore, MemoryLifecycleMaintenancePolicy, MemoryLifecycleMaintenanceResult,
};
pub use tau_safety::{
    default_safety_rule_set, scan_safety_rules, validate_safety_rule_set, DefaultLeakDetector,
    DefaultSanitizer, LeakDetector, SafetyMode, SafetyPolicy, SafetyRule, SafetyRuleMatcher,
    SafetyRuleSet, SafetyStage, Sanitizer,
};
use thiserror::Error;

mod process_types;
mod runtime_safety_memory;
mod runtime_startup;
mod runtime_tool_bridge;
mod runtime_turn_loop;

pub use process_types::{
    ProcessLifecycleState, ProcessManager, ProcessManagerError, ProcessRuntimeProfile,
    ProcessSnapshot, ProcessSpawnSpec, ProcessType,
};
pub(crate) use runtime_safety_memory::{assistant_text_suggests_failure, retrieve_memory_matches};
pub(crate) use runtime_startup::{
    cache_insert_with_limit, lock_or_recover, normalize_direct_message_content,
    sleep_with_cancellation, spawn_async_event_handler_worker,
};
pub(crate) use runtime_tool_bridge::execute_tool_call_inner;
#[cfg(test)]
pub(crate) use runtime_turn_loop::extract_json_payload;
pub(crate) use runtime_turn_loop::{
    bounded_messages, build_structured_output_retry_prompt, collapse_whitespace,
    compact_messages_for_tier, context_pressure_snapshot, estimate_chat_request_tokens,
    estimate_usage_cost_usd, is_retryable_ai_error, normalize_cost_alert_thresholds,
    parse_structured_output, role_label, stream_retry_buffer_on_delta, timeout_duration_from_ms,
    truncate_chars, ContextCompactionConfig, ContextCompactionTier, StreamingRetryBufferState,
};
#[cfg(test)]
pub(crate) use tau_memory::runtime::embed_text_vector;

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
    pub context_compaction_warn_threshold_percent: u8,
    pub context_compaction_aggressive_threshold_percent: u8,
    pub context_compaction_emergency_threshold_percent: u8,
    pub context_compaction_warn_retain_percent: u8,
    pub context_compaction_aggressive_retain_percent: u8,
    pub context_compaction_emergency_retain_percent: u8,
    pub structured_output_max_retries: usize,
    pub react_max_replans_on_tool_failure: usize,
    pub max_concurrent_branches_per_session: usize,
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
    pub model_cached_input_cost_per_million: Option<f64>,
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
            context_compaction_warn_threshold_percent: 80,
            context_compaction_aggressive_threshold_percent: 85,
            context_compaction_emergency_threshold_percent: 95,
            context_compaction_warn_retain_percent: 70,
            context_compaction_aggressive_retain_percent: 50,
            context_compaction_emergency_retain_percent: 50,
            structured_output_max_retries: 1,
            react_max_replans_on_tool_failure: 1,
            max_concurrent_branches_per_session: 2,
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
            model_cached_input_cost_per_million: None,
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

/// Extracts a successful skip-response directive reason from tool-result messages.
///
/// Returns `Some(reason)` when a successful `skip` tool result includes an explicit suppression
/// marker (`skip_response: true` or `action: "skip_response"`). When suppression is requested
/// without a reason, returns `Some(String::new())`.
pub fn extract_skip_response_reason(messages: &[Message]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        if message.role != MessageRole::Tool || message.is_error {
            return None;
        }
        if message.tool_name.as_deref() != Some("skip") {
            return None;
        }
        let text = message.text_content();
        if text.trim().is_empty() {
            return None;
        }
        let parsed = serde_json::from_str::<Value>(text.trim()).ok()?;
        parse_skip_response_reason_payload(&parsed)
    })
}

fn parse_skip_response_reason_payload(payload: &Value) -> Option<String> {
    let object = payload.as_object()?;
    let skip_response = object
        .get("skip_response")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let action_skip = object
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim() == "skip_response");
    if !skip_response && !action_skip {
        return None;
    }
    let reason = object
        .get("reason")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    Some(reason)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReactResponseDirective {
    pub(crate) emoji: String,
    pub(crate) message_id: Option<String>,
    pub(crate) reason_code: String,
}

pub(crate) fn extract_react_response_directive(
    messages: &[Message],
) -> Option<ReactResponseDirective> {
    messages.iter().rev().find_map(|message| {
        if message.role != MessageRole::Tool || message.is_error {
            return None;
        }
        if message.tool_name.as_deref() != Some("react") {
            return None;
        }
        let text = message.text_content();
        if text.trim().is_empty() {
            return None;
        }
        let parsed = serde_json::from_str::<Value>(text.trim()).ok()?;
        parse_react_response_directive_payload(&parsed)
    })
}

fn parse_react_response_directive_payload(payload: &Value) -> Option<ReactResponseDirective> {
    let object = payload.as_object()?;
    let react_response = object
        .get("react_response")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let action_react = object
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim() == "react_response");
    if !react_response && !action_react {
        return None;
    }
    let suppress_response = object
        .get("suppress_response")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !suppress_response {
        return None;
    }
    let emoji = object
        .get("emoji")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let message_id = object
        .get("message_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let reason_code = object
        .get("reason_code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("react_requested")
        .to_string();
    Some(ReactResponseDirective {
        emoji,
        message_id,
        reason_code,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SendFileResponseDirective {
    pub(crate) file_path: String,
    pub(crate) message: Option<String>,
    pub(crate) reason_code: String,
}

pub(crate) fn extract_send_file_response_directive(
    messages: &[Message],
) -> Option<SendFileResponseDirective> {
    messages.iter().rev().find_map(|message| {
        if message.role != MessageRole::Tool || message.is_error {
            return None;
        }
        if message.tool_name.as_deref() != Some("send_file") {
            return None;
        }
        let text = message.text_content();
        if text.trim().is_empty() {
            return None;
        }
        let parsed = serde_json::from_str::<Value>(text.trim()).ok()?;
        parse_send_file_response_directive_payload(&parsed)
    })
}

fn parse_send_file_response_directive_payload(
    payload: &Value,
) -> Option<SendFileResponseDirective> {
    let object = payload.as_object()?;
    let send_file_response = object
        .get("send_file_response")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let action_send_file = object
        .get("action")
        .and_then(Value::as_str)
        .is_some_and(|value| value.trim() == "send_file_response");
    if !send_file_response && !action_send_file {
        return None;
    }
    let suppress_response = object
        .get("suppress_response")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !suppress_response {
        return None;
    }
    let file_path = object
        .get("file_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let reason_code = object
        .get("reason_code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("send_file_requested")
        .to_string();
    Some(SendFileResponseDirective {
        file_path,
        message,
        reason_code,
    })
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
const CONTEXT_SUMMARY_LLM_BRIEF_PREFIX: &str = "llm_brief:";
const WARN_COMPACTION_LLM_BRIEF_MAX_CHARS: usize = 240;
const WARN_COMPACTION_LLM_SYSTEM_PROMPT: &str =
    "You summarize dropped chat context for compaction. Reply with one concise plain-text brief.";
const COMPACTION_ENTRY_PREFIX: &str = "[Tau compaction entry]";
const COMPACTION_MEMORY_SAVE_PREFIX: &str = "[Tau compaction memory save]";
const MEMORY_RECALL_PREFIX: &str = "[Tau memory recall]";
const DIRECT_MESSAGE_PREFIX: &str = "[Tau direct message]";
const REPLAN_ON_TOOL_FAILURE_PROMPT: &str = "One or more tool calls failed. Replan and continue with an alternative approach using available tools. If no viable tool exists, explain what is missing and ask the user for clarification.";
const TOOL_OUTPUT_BLOCKED_ERROR: &str = "tool output blocked by safety policy";
const BRANCH_CONCLUSION_MAX_CHARS: usize = 4_000;
const BRANCH_REASON_CODE_CREATED: &str = "session_branch_created";
const BRANCH_REASON_CODE_READY: &str = "branch_conclusion_ready";
const BRANCH_REASON_CODE_LIMIT_EXCEEDED: &str = "branch_concurrency_limit_exceeded";
const BRANCH_REASON_CODE_EXECUTION_FAILED: &str = "branch_execution_failed";
const BRANCH_REASON_CODE_PROMPT_MISSING: &str = "branch_prompt_missing";
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

struct PendingWarnCompaction {
    source_messages: Vec<Message>,
    receiver: tokio::sync::oneshot::Receiver<Vec<Message>>,
}

#[derive(Clone)]
struct ReadyWarnCompaction {
    source_messages: Vec<Message>,
    compacted_messages: Vec<Message>,
}

#[derive(Default)]
struct WarnCompactionState {
    pending: Option<PendingWarnCompaction>,
    ready: Option<ReadyWarnCompaction>,
}

struct BranchRunSlotGuard {
    active_branch_runs: Arc<AtomicUsize>,
}

impl Drop for BranchRunSlotGuard {
    fn drop(&mut self) {
        self.active_branch_runs.fetch_sub(1, Ordering::Release);
    }
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
    skip_response_reason: Option<String>,
    warn_compaction_state: Arc<Mutex<WarnCompactionState>>,
    active_branch_runs: Arc<AtomicUsize>,
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
            skip_response_reason: None,
            warn_compaction_state: Arc::new(Mutex::new(WarnCompactionState::default())),
            active_branch_runs: Arc::new(AtomicUsize::new(0)),
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

    /// Replaces the active dispatch model and returns the previous model value.
    pub fn swap_dispatch_model(&mut self, model: impl Into<String>) -> String {
        let normalized = model.into().trim().to_string();
        if normalized.is_empty() {
            return self.config.model.clone();
        }
        std::mem::replace(&mut self.config.model, normalized)
    }

    /// Restores the active dispatch model from a previously saved value.
    pub fn restore_dispatch_model(&mut self, previous_model: String) {
        if previous_model.trim().is_empty() {
            return;
        }
        self.config.model = previous_model;
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

    /// Replaces the leading startup system prompt while preserving conversation history.
    ///
    /// Returns `true` when the effective startup system prompt changed.
    pub fn replace_system_prompt(&mut self, system_prompt: impl Into<String>) -> bool {
        let system_prompt = system_prompt.into();

        if let Some(first) = self.messages.first_mut() {
            if first.role == MessageRole::System {
                if first.text_content() == system_prompt {
                    return false;
                }
                *first = Message::system(system_prompt);
                return true;
            }
        }

        self.messages.insert(0, Message::system(system_prompt));
        true
    }

    /// Appends a message to the conversation history.
    pub fn append_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    /// Clones the agent state for independent execution.
    pub fn fork(&self) -> Self {
        let mut cloned = self.clone();
        cloned.warn_compaction_state = Arc::new(Mutex::new(WarnCompactionState::default()));
        cloned
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

    fn max_concurrent_branches_per_session(&self) -> usize {
        self.config.max_concurrent_branches_per_session.max(1)
    }

    fn try_acquire_branch_run_slot(&self) -> Option<BranchRunSlotGuard> {
        let limit = self.max_concurrent_branches_per_session();
        loop {
            let current = self.active_branch_runs.load(Ordering::Acquire);
            if current >= limit {
                return None;
            }
            if self
                .active_branch_runs
                .compare_exchange(
                    current,
                    current.saturating_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                return Some(BranchRunSlotGuard {
                    active_branch_runs: Arc::clone(&self.active_branch_runs),
                });
            }
        }
    }

    fn branch_result_payload_base(result: &ToolExecutionResult) -> serde_json::Map<String, Value> {
        match &result.content {
            Value::Object(object) => object.clone(),
            other => {
                let mut payload = serde_json::Map::new();
                payload.insert("tool".to_string(), Value::String("branch".to_string()));
                payload.insert("branch_base_result".to_string(), other.clone());
                payload
            }
        }
    }

    fn branch_result_with_error(
        base_result: &ToolExecutionResult,
        reason_code: &str,
        error: String,
    ) -> ToolExecutionResult {
        let mut payload = Self::branch_result_payload_base(base_result);
        if let Some(existing_reason_code) = payload
            .get("reason_code")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            payload.insert(
                "branch_creation_reason_code".to_string(),
                Value::String(existing_reason_code.to_string()),
            );
        }
        payload.insert(
            "reason_code".to_string(),
            Value::String(reason_code.to_string()),
        );
        payload.insert("error".to_string(), Value::String(error.clone()));
        payload.insert(
            "branch_followup".to_string(),
            json!({
                "status": "error",
                "reason_code": reason_code,
                "error": error,
            }),
        );
        ToolExecutionResult::error(Value::Object(payload))
    }

    async fn maybe_execute_branch_followup(
        &self,
        call: &ToolCall,
        result: ToolExecutionResult,
        branch_slot_holders: &mut Vec<BranchRunSlotGuard>,
    ) -> ToolExecutionResult {
        if call.name != "branch" || result.is_error {
            return result;
        }

        let Some(slot) = self.try_acquire_branch_run_slot() else {
            return Self::branch_result_with_error(
                &result,
                BRANCH_REASON_CODE_LIMIT_EXCEEDED,
                format!(
                    "branch concurrency limit reached (active={} limit={})",
                    self.active_branch_runs.load(Ordering::Acquire),
                    self.max_concurrent_branches_per_session()
                ),
            );
        };
        branch_slot_holders.push(slot);
        self.execute_branch_followup(call, result).await
    }

    fn branch_followup_prompt(call: &ToolCall) -> Option<String> {
        call.arguments
            .get("prompt")
            .and_then(Value::as_str)
            .map(|prompt| prompt.trim().to_string())
            .filter(|prompt| !prompt.is_empty())
    }

    fn branch_followup_assistant_conclusion(messages: &[Message]) -> Option<String> {
        messages.iter().rev().find_map(|message| {
            if message.role != MessageRole::Assistant {
                return None;
            }
            let text = collapse_whitespace(&message.text_content());
            if text.trim().is_empty() {
                return None;
            }
            Some(truncate_chars(&text, BRANCH_CONCLUSION_MAX_CHARS))
        })
    }

    async fn execute_branch_followup(
        &self,
        call: &ToolCall,
        result: ToolExecutionResult,
    ) -> ToolExecutionResult {
        let Some(prompt) = Self::branch_followup_prompt(call) else {
            return Self::branch_result_with_error(
                &result,
                BRANCH_REASON_CODE_PROMPT_MISSING,
                "branch call arguments must include non-empty prompt".to_string(),
            );
        };

        let mut branch_agent = self.fork();
        branch_agent.set_agent_id(format!("{}::branch", self.agent_id()));
        branch_agent.skip_response_reason = None;
        branch_agent.clear_tool_result_cache();
        branch_agent
            .tools
            .retain(|tool_name, _| tool_name.starts_with("memory_"));

        let available_tools = branch_agent.registered_tool_names();
        let branch_messages = match Box::pin(branch_agent.prompt(prompt)).await {
            Ok(messages) => messages,
            Err(error) => {
                return Self::branch_result_with_error(
                    &result,
                    BRANCH_REASON_CODE_EXECUTION_FAILED,
                    format!("branch follow-up execution failed: {error}"),
                );
            }
        };

        let Some(branch_conclusion) = Self::branch_followup_assistant_conclusion(&branch_messages)
        else {
            return Self::branch_result_with_error(
                &result,
                BRANCH_REASON_CODE_EXECUTION_FAILED,
                "branch follow-up produced no assistant conclusion".to_string(),
            );
        };

        let mut payload = Self::branch_result_payload_base(&result);
        if payload
            .get("reason_code")
            .and_then(Value::as_str)
            .is_some_and(|value| value == BRANCH_REASON_CODE_CREATED)
        {
            payload.insert(
                "branch_creation_reason_code".to_string(),
                Value::String(BRANCH_REASON_CODE_CREATED.to_string()),
            );
        }
        payload.insert(
            "reason_code".to_string(),
            Value::String(BRANCH_REASON_CODE_READY.to_string()),
        );
        payload.insert(
            "branch_conclusion".to_string(),
            Value::String(branch_conclusion),
        );
        payload.insert(
            "branch_followup".to_string(),
            json!({
                "status": "completed",
                "tools_mode": "memory_only",
                "available_tools": available_tools,
                "branch_message_count": branch_messages.len(),
            }),
        );
        ToolExecutionResult::ok(Value::Object(payload))
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
            self.config.model_cached_input_cost_per_million,
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
        self.skip_response_reason = None;
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
                prompt_cache: tau_ai::PromptCacheConfig {
                    enabled: true,
                    cache_key: Some(self.config.agent_id.clone()),
                    retention: None,
                    google_cached_content: None,
                },
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

            if self.skip_response_reason.take().is_some() {
                self.emit(AgentEvent::TurnEnd {
                    turn,
                    tool_results: tool_stats.total,
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

    fn context_compaction_config(&self) -> ContextCompactionConfig {
        ContextCompactionConfig {
            max_input_tokens: self.config.max_estimated_input_tokens,
            warn_threshold_percent: self.config.context_compaction_warn_threshold_percent,
            aggressive_threshold_percent: self
                .config
                .context_compaction_aggressive_threshold_percent,
            emergency_threshold_percent: self.config.context_compaction_emergency_threshold_percent,
            warn_retain_percent: self.config.context_compaction_warn_retain_percent,
            aggressive_retain_percent: self.config.context_compaction_aggressive_retain_percent,
            emergency_retain_percent: self.config.context_compaction_emergency_retain_percent,
        }
    }

    fn poll_warn_compaction_state_locked(state: &mut WarnCompactionState) {
        let outcome = if let Some(pending) = state.pending.as_mut() {
            match pending.receiver.try_recv() {
                Ok(compacted_messages) => {
                    Some((pending.source_messages.clone(), Some(compacted_messages)))
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => Some((Vec::new(), None)),
            }
        } else {
            None
        };

        let Some((source_messages, compacted_messages)) = outcome else {
            return;
        };

        state.pending = None;
        if let Some(compacted_messages) = compacted_messages {
            state.ready = Some(ReadyWarnCompaction {
                source_messages,
                compacted_messages,
            });
        }
    }

    fn take_ready_warn_compaction(&self, messages: &[Message]) -> Option<Vec<Message>> {
        let mut state = lock_or_recover(self.warn_compaction_state.as_ref());
        Self::poll_warn_compaction_state_locked(&mut state);

        let ready = state.ready.take()?;
        if !messages.starts_with(&ready.source_messages) {
            return None;
        }

        let mut merged = ready.compacted_messages;
        merged.extend_from_slice(&messages[ready.source_messages.len()..]);
        Some(merged)
    }

    fn inject_warn_llm_brief_into_summary(summary: &str, llm_brief: &str) -> Option<String> {
        if !summary.starts_with(CONTEXT_SUMMARY_PREFIX) {
            return None;
        }
        if summary.lines().any(|line| {
            line.trim_start()
                .starts_with(CONTEXT_SUMMARY_LLM_BRIEF_PREFIX)
        }) {
            return Some(summary.to_string());
        }

        let brief = collapse_whitespace(llm_brief);
        if brief.is_empty() {
            return None;
        }
        let brief_line = format!(
            "{CONTEXT_SUMMARY_LLM_BRIEF_PREFIX} {}",
            truncate_chars(&brief, WARN_COMPACTION_LLM_BRIEF_MAX_CHARS)
        );
        let enriched = if let Some((head, excerpts)) = summary.split_once("\nexcerpts:\n") {
            format!("{head}\n{brief_line}\nexcerpts:\n{excerpts}")
        } else {
            format!("{summary}\n{brief_line}")
        };
        Some(truncate_chars(&enriched, CONTEXT_SUMMARY_MAX_CHARS))
    }

    async fn maybe_build_warn_llm_summary(
        client: Arc<dyn LlmClient>,
        model: String,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        request_timeout_ms: Option<u64>,
        agent_id: String,
        fallback_summary: String,
    ) -> Option<String> {
        let request = ChatRequest {
            model,
            messages: vec![
                Message::system(WARN_COMPACTION_LLM_SYSTEM_PROMPT),
                Message::user(format!(
                    "Refine this context compaction summary into one concise sentence:\n{}",
                    fallback_summary
                )),
            ],
            tool_choice: None,
            json_mode: false,
            tools: Vec::new(),
            max_tokens,
            temperature,
            prompt_cache: tau_ai::PromptCacheConfig {
                enabled: true,
                cache_key: Some(format!("{agent_id}:warn-compaction-llm-summary")),
                retention: None,
                google_cached_content: None,
            },
        };

        let response = if let Some(timeout) = timeout_duration_from_ms(request_timeout_ms) {
            match tokio::time::timeout(timeout, client.complete(request)).await {
                Ok(result) => result.ok()?,
                Err(_) => return None,
            }
        } else {
            client.complete(request).await.ok()?
        };
        let llm_brief = response.message.text_content();
        Self::inject_warn_llm_brief_into_summary(&fallback_summary, &llm_brief)
    }

    fn schedule_warn_compaction_if_needed(
        &self,
        messages: &[Message],
        compaction_config: ContextCompactionConfig,
    ) {
        let mut state = lock_or_recover(self.warn_compaction_state.as_ref());
        Self::poll_warn_compaction_state_locked(&mut state);

        if let Some(ready) = state.ready.as_ref() {
            if messages.starts_with(&ready.source_messages) {
                return;
            }
            state.ready = None;
        }

        if let Some(pending) = state.pending.as_ref() {
            if messages.starts_with(&pending.source_messages) {
                return;
            }
            state.pending = None;
        }

        let source_messages = messages.to_vec();
        let source_for_task = source_messages.clone();
        let client = Arc::clone(&self.client);
        let model = self.config.model.clone();
        let max_tokens = self.config.max_tokens;
        let temperature = self.config.temperature;
        let request_timeout_ms = self.config.request_timeout_ms;
        let agent_id = self.agent_id.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut compacted = compact_messages_for_tier(
                source_for_task.as_slice(),
                ContextCompactionTier::Warn,
                compaction_config,
            );
            if let Some(summary_index) = compacted.iter().position(|message| {
                message.role == MessageRole::System
                    && message.text_content().starts_with(CONTEXT_SUMMARY_PREFIX)
            }) {
                let fallback_summary = compacted[summary_index].text_content().to_string();
                if let Some(llm_summary) = Agent::maybe_build_warn_llm_summary(
                    client,
                    model,
                    max_tokens,
                    temperature,
                    request_timeout_ms,
                    agent_id,
                    fallback_summary,
                )
                .await
                {
                    compacted[summary_index] = Message::system(llm_summary);
                }
            }
            let _ = sender.send(compacted);
        });
        state.pending = Some(PendingWarnCompaction {
            source_messages,
            receiver,
        });
    }

    fn compaction_tier_label(tier: ContextCompactionTier) -> &'static str {
        match tier {
            ContextCompactionTier::Warn => "warn",
            ContextCompactionTier::Aggressive => "aggressive",
            ContextCompactionTier::Emergency => "emergency",
            ContextCompactionTier::None => "none",
        }
    }

    fn extract_memory_candidates_from_compaction_summary(
        summary: &str,
    ) -> Result<Vec<String>, &'static str> {
        let mut candidates = Vec::new();
        let mut in_excerpt_block = false;
        for line in summary.lines() {
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("excerpts:") {
                in_excerpt_block = true;
                continue;
            }
            if !in_excerpt_block {
                continue;
            }
            let Some(excerpt) = trimmed.strip_prefix("- ") else {
                continue;
            };
            let content = excerpt
                .split_once(':')
                .map(|(_, value)| value)
                .unwrap_or(excerpt);
            let collapsed = collapse_whitespace(content);
            if collapsed.is_empty() {
                continue;
            }
            candidates.push(collapsed);
            if candidates.len() >= CONTEXT_SUMMARY_MAX_EXCERPTS {
                break;
            }
        }
        if candidates.is_empty() {
            return Err("compaction summary did not contain excerpt candidates");
        }
        Ok(candidates)
    }

    fn append_system_artifact_if_new(&mut self, artifact: String) {
        let duplicate = self.messages.iter().rev().take(24).any(|message| {
            message.role == MessageRole::System && message.text_content() == artifact
        });
        if duplicate {
            return;
        }
        let message = Message::system(artifact);
        self.messages.push(message.clone());
        self.emit(AgentEvent::MessageAdded { message });
    }

    fn persist_compaction_artifacts(
        &mut self,
        tier: ContextCompactionTier,
        compacted_messages: &[Message],
    ) {
        if !matches!(
            tier,
            ContextCompactionTier::Warn | ContextCompactionTier::Aggressive
        ) {
            return;
        }

        let Some(summary) = compacted_messages.iter().find_map(|message| {
            (message.role == MessageRole::System
                && message.text_content().starts_with(CONTEXT_SUMMARY_PREFIX))
            .then(|| message.text_content().to_string())
        }) else {
            return;
        };

        let tier_label = Self::compaction_tier_label(tier);
        let compaction_entry = format!("{COMPACTION_ENTRY_PREFIX} tier={tier_label}\n{summary}");
        self.append_system_artifact_if_new(compaction_entry);

        let Ok(candidates) = Self::extract_memory_candidates_from_compaction_summary(&summary)
        else {
            return;
        };

        let mut lines = Vec::with_capacity(candidates.len().saturating_add(1));
        lines.push(format!("{COMPACTION_MEMORY_SAVE_PREFIX} tier={tier_label}"));
        for candidate in candidates {
            lines.push(format!(
                "- memory: {}",
                truncate_chars(&candidate, self.config.memory_max_chars_per_item)
            ));
        }
        self.append_system_artifact_if_new(lines.join("\n"));
    }

    async fn request_messages(&mut self) -> Vec<Message> {
        let context_limit = self.config.max_context_messages;
        let mut messages = if let Some(limit) = context_limit {
            bounded_messages(&self.messages, limit)
        } else {
            self.messages.clone()
        };
        let compaction_config = self.context_compaction_config();
        let pressure_estimate = estimate_chat_request_tokens(&ChatRequest {
            model: self.config.model.clone(),
            messages: messages.clone(),
            tool_choice: None,
            json_mode: false,
            tools: Vec::new(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            prompt_cache: tau_ai::PromptCacheConfig {
                enabled: true,
                cache_key: Some(self.config.agent_id.clone()),
                retention: None,
                google_cached_content: None,
            },
        });
        let pressure_snapshot =
            context_pressure_snapshot(pressure_estimate.input_tokens, compaction_config);
        match pressure_snapshot.tier {
            ContextCompactionTier::Warn => {
                if let Some(compacted) = self.take_ready_warn_compaction(&messages) {
                    messages = compacted;
                    self.persist_compaction_artifacts(ContextCompactionTier::Warn, &messages);
                } else {
                    self.schedule_warn_compaction_if_needed(&messages, compaction_config);
                }
            }
            ContextCompactionTier::Aggressive => {
                {
                    let mut state = lock_or_recover(self.warn_compaction_state.as_ref());
                    state.pending = None;
                    state.ready = None;
                }
                messages =
                    compact_messages_for_tier(&messages, pressure_snapshot.tier, compaction_config);
                self.persist_compaction_artifacts(ContextCompactionTier::Aggressive, &messages);
            }
            ContextCompactionTier::Emergency => {
                let mut state = lock_or_recover(self.warn_compaction_state.as_ref());
                state.pending = None;
                state.ready = None;
                messages =
                    compact_messages_for_tier(&messages, pressure_snapshot.tier, compaction_config);
            }
            ContextCompactionTier::None => {}
        }

        let Some(limit) = context_limit else {
            return messages;
        };

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
            let mut branch_slot_holders = Vec::new();
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
                let result = self
                    .maybe_execute_branch_followup(&call, result, &mut branch_slot_holders)
                    .await;
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
            match self.spawn_tool_call_task(call.clone()).await {
                Ok(result) => result,
                Err(error) => ToolExecutionResult::error(json!({
                    "error": format!("tool '{}' execution task failed: {error}", call.name)
                })),
            }
        };
        let mut branch_slot_holders = Vec::new();
        let result = self
            .maybe_execute_branch_followup(&call, result, &mut branch_slot_holders)
            .await;
        self.store_tool_result_cache(&call, &result);
        self.record_tool_result(call, result)
    }

    fn record_tool_result(&mut self, call: ToolCall, result: ToolExecutionResult) -> bool {
        let result = self.sanitize_tool_result(result);
        self.emit(AgentEvent::ToolExecutionEnd {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            result: result.clone(),
        });

        let tool_name = call.name.clone();
        let tool_message = Message::tool_result(
            call.id,
            tool_name.clone(),
            result.as_text(),
            result.is_error,
        );
        self.messages.push(tool_message.clone());
        self.emit(AgentEvent::MessageAdded {
            message: tool_message.clone(),
        });
        if tool_name == "skip" && !result.is_error {
            self.skip_response_reason =
                extract_skip_response_reason(std::slice::from_ref(&tool_message));
        } else if tool_name == "react"
            && !result.is_error
            && extract_react_response_directive(std::slice::from_ref(&tool_message)).is_some()
        {
            self.skip_response_reason = Some("react_requested".to_string());
        } else if tool_name == "send_file"
            && !result.is_error
            && extract_send_file_response_directive(std::slice::from_ref(&tool_message)).is_some()
        {
            self.skip_response_reason = Some("send_file_requested".to_string());
        }
        result.is_error
    }
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
        CONTEXT_SUMMARY_MAX_EXCERPTS, CONTEXT_SUMMARY_PREFIX, DIRECT_MESSAGE_PREFIX,
        MEMORY_RECALL_PREFIX,
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

    #[path = "process_architecture.rs"]
    mod process_architecture;
}
