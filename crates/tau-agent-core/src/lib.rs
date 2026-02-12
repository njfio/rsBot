use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use jsonschema::validator_for;
use serde_json::{json, Value};
use tau_ai::{
    ChatRequest, ChatUsage, LlmClient, Message, MessageRole, StreamDeltaHandler, TauAiError,
    ToolCall, ToolDefinition,
};
use thiserror::Error;

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
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub content: Value,
    pub is_error: bool,
}

impl ToolExecutionResult {
    pub fn ok(content: Value) -> Self {
        Self {
            content,
            is_error: false,
        }
    }

    pub fn error(content: Value) -> Self {
        Self {
            content,
            is_error: true,
        }
    }

    pub fn as_text(&self) -> String {
        match &self.content {
            Value::String(text) => text.clone(),
            other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
        }
    }
}

#[async_trait]
pub trait AgentTool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, arguments: Value) -> ToolExecutionResult;
}

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

#[derive(Debug, Error)]
pub enum AgentError {
    #[error(transparent)]
    Ai(#[from] TauAiError),
    #[error("agent exceeded max turns ({0})")]
    MaxTurnsExceeded(usize),
}

type EventHandler = Arc<dyn Fn(&AgentEvent) + Send + Sync>;

struct RegisteredTool {
    definition: ToolDefinition,
    tool: Arc<dyn AgentTool>,
}

pub struct Agent {
    client: Arc<dyn LlmClient>,
    config: AgentConfig,
    messages: Vec<Message>,
    tools: HashMap<String, RegisteredTool>,
    handlers: Vec<EventHandler>,
}

impl Agent {
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

    pub fn subscribe<F>(&mut self, handler: F)
    where
        F: Fn(&AgentEvent) + Send + Sync + 'static,
    {
        self.handlers.push(Arc::new(handler));
    }

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

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn replace_messages(&mut self, messages: Vec<Message>) {
        self.messages = messages;
    }

    pub fn append_message(&mut self, message: Message) {
        self.messages.push(message);
    }

    pub async fn prompt(&mut self, text: impl Into<String>) -> Result<Vec<Message>, AgentError> {
        self.prompt_with_stream(text, None).await
    }

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

    pub async fn continue_turn(&mut self) -> Result<Vec<Message>, AgentError> {
        self.continue_turn_with_stream(None).await
    }

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
            handler(&event);
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| tool.definition.clone())
            .collect()
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
    ) -> Result<tau_ai::ChatResponse, TauAiError> {
        let max_retries = self.config.request_max_retries;
        let mut attempt = 0usize;
        let mut backoff_ms = self.config.request_retry_initial_backoff_ms.max(1);
        let max_backoff_ms = self.config.request_retry_max_backoff_ms.max(backoff_ms);

        loop {
            let request_for_attempt = request.clone();
            match self
                .client
                .complete_with_stream(request_for_attempt, on_delta.clone())
                .await
            {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let can_retry = attempt < max_retries
                        && on_delta.is_none()
                        && is_retryable_ai_error(&error);
                    if !can_retry {
                        return Err(error);
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
        tokio::spawn(async move { execute_tool_call_inner(call, registered).await })
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

fn bounded_messages(messages: &[Message], max_messages: usize) -> Vec<Message> {
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

async fn execute_tool_call_inner(
    call: ToolCall,
    registered: Option<(ToolDefinition, Arc<dyn AgentTool>)>,
) -> ToolExecutionResult {
    if let Some((definition, tool)) = registered {
        if let Err(error) = validate_tool_arguments(&definition, &call.arguments) {
            return ToolExecutionResult::error(json!({ "error": error }));
        }
        tool.execute(call.arguments).await
    } else {
        ToolExecutionResult::error(json!({
            "error": format!("Tool '{}' is not registered", call.name)
        }))
    }
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
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    use async_trait::async_trait;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, ContentBlock, Message, MessageRole};
    use tokio::sync::Mutex as AsyncMutex;

    use crate::{Agent, AgentConfig, AgentError, AgentEvent, AgentTool, ToolExecutionResult};

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
        assert_eq!(first_request.messages[1].text_content(), "a2");
        assert_eq!(first_request.messages[2].text_content(), "u3");
        assert_eq!(first_request.messages[3].text_content(), "latest");
        assert!(
            agent.messages().len() <= 4,
            "history should be compacted to configured context window"
        );
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
}
