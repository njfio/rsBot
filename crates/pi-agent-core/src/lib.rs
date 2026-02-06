use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use jsonschema::validator_for;
use pi_ai::{
    ChatRequest, LlmClient, Message, PiAiError, StreamDeltaHandler, ToolCall, ToolDefinition,
};
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o-mini".to_string(),
            system_prompt: "You are a helpful coding assistant.".to_string(),
            max_turns: 8,
            temperature: Some(0.0),
            max_tokens: None,
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
    Ai(#[from] PiAiError),
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

        self.run_loop(start_index, on_delta).await
    }

    pub async fn continue_turn(&mut self) -> Result<Vec<Message>, AgentError> {
        self.continue_turn_with_stream(None).await
    }

    pub async fn continue_turn_with_stream(
        &mut self,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<Vec<Message>, AgentError> {
        let start_index = self.messages.len();
        self.run_loop(start_index, on_delta).await
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
                messages: self.messages.clone(),
                tools: self.tool_definitions(),
                max_tokens: self.config.max_tokens,
                temperature: self.config.temperature,
            };

            let response = self
                .client
                .complete_with_stream(request, on_delta.clone())
                .await?;
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
                });
                let new_messages = self.messages[start_index..].to_vec();
                self.emit(AgentEvent::AgentEnd {
                    new_messages: new_messages.len(),
                });
                return Ok(new_messages);
            }

            for call in tool_calls {
                self.execute_tool_call(call).await;
            }

            self.emit(AgentEvent::TurnEnd {
                turn,
                tool_results: self
                    .messages
                    .iter()
                    .rev()
                    .take_while(|message| message.role == pi_ai::MessageRole::Tool)
                    .count(),
            });
        }

        Err(AgentError::MaxTurnsExceeded(self.config.max_turns))
    }

    async fn execute_tool_call(&mut self, call: ToolCall) {
        self.emit(AgentEvent::ToolExecutionStart {
            tool_call_id: call.id.clone(),
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        });

        let result = if let Some(registered) = self.tools.get(&call.name) {
            if let Err(error) = validate_tool_arguments(&registered.definition, &call.arguments) {
                ToolExecutionResult::error(json!({ "error": error }))
            } else {
                registered.tool.execute(call.arguments).await
            }
        } else {
            ToolExecutionResult::error(json!({
                "error": format!("Tool '{}' is not registered", call.name)
            }))
        };

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
    };

    use async_trait::async_trait;
    use pi_ai::{ChatRequest, ChatResponse, ChatUsage, ContentBlock, Message, MessageRole};
    use tokio::sync::Mutex as AsyncMutex;

    use crate::{Agent, AgentConfig, AgentError, AgentEvent, AgentTool, ToolExecutionResult};

    struct MockClient {
        responses: AsyncMutex<VecDeque<ChatResponse>>,
    }

    #[async_trait]
    impl pi_ai::LlmClient for MockClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, pi_ai::PiAiError> {
            let mut responses = self.responses.lock().await;
            responses.pop_front().ok_or_else(|| {
                pi_ai::PiAiError::InvalidResponse("mock response queue is empty".to_string())
            })
        }
    }

    struct StreamingMockClient {
        response: ChatResponse,
        deltas: Vec<String>,
    }

    #[async_trait]
    impl pi_ai::LlmClient for StreamingMockClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, pi_ai::PiAiError> {
            Ok(self.response.clone())
        }

        async fn complete_with_stream(
            &self,
            _request: ChatRequest,
            on_delta: Option<pi_ai::StreamDeltaHandler>,
        ) -> Result<ChatResponse, pi_ai::PiAiError> {
            if let Some(handler) = on_delta {
                for delta in &self.deltas {
                    handler(delta.clone());
                }
            }
            Ok(self.response.clone())
        }
    }

    struct ReadTool;

    #[async_trait]
    impl AgentTool for ReadTool {
        fn definition(&self) -> pi_ai::ToolDefinition {
            pi_ai::ToolDefinition {
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
}
