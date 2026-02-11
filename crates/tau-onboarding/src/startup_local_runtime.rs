use std::path::{Path, PathBuf};

use serde_json::Value;
use tau_agent_core::{Agent, AgentEvent};
use tau_core::current_unix_timestamp_ms;

const EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

pub fn extension_tool_hook_dispatch(event: &AgentEvent) -> Option<(&'static str, Value)> {
    match event {
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => Some((
            "pre-tool-call",
            extension_tool_hook_payload(
                "pre-tool-call",
                serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "arguments": arguments,
                }),
            ),
        )),
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => Some((
            "post-tool-call",
            extension_tool_hook_payload(
                "post-tool-call",
                serde_json::json!({
                    "tool_call_id": tool_call_id,
                    "tool_name": tool_name,
                    "result": {
                        "is_error": result.is_error,
                        "content": result.content,
                    },
                }),
            ),
        )),
        _ => None,
    }
}

pub fn extension_tool_hook_diagnostics<F>(
    event: &AgentEvent,
    root: &Path,
    dispatch_hook: &F,
) -> Vec<String>
where
    F: Fn(&Path, &'static str, &Value) -> Vec<String>,
{
    let Some((hook, payload)) = extension_tool_hook_dispatch(event) else {
        return Vec::new();
    };
    dispatch_hook(root, hook, &payload)
}

pub fn register_runtime_extension_tool_hook_subscriber<F>(
    agent: &mut Agent,
    enabled: bool,
    root: PathBuf,
    dispatch_hook: F,
) where
    F: Fn(&Path, &'static str, &Value) -> Vec<String> + Send + Sync + 'static,
{
    if !enabled {
        return;
    }

    agent.subscribe(move |event| {
        let diagnostics = extension_tool_hook_diagnostics(event, &root, &dispatch_hook);
        for diagnostic in diagnostics {
            eprintln!("{diagnostic}");
        }
    });
}

fn extension_tool_hook_payload(hook: &str, data: Value) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema_version".to_string(),
        serde_json::Value::Number(EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION.into()),
    );
    payload.insert(
        "hook".to_string(),
        serde_json::Value::String(hook.to_string()),
    );
    payload.insert(
        "emitted_at_ms".to_string(),
        serde_json::Value::Number(current_unix_timestamp_ms().into()),
    );
    payload.insert("data".to_string(), data.clone());
    if let Some(object) = data.as_object() {
        for (key, value) in object {
            payload.insert(key.clone(), value.clone());
        }
    }
    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use super::{
        extension_tool_hook_diagnostics, extension_tool_hook_dispatch,
        register_runtime_extension_tool_hook_subscriber,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tau_agent_core::{Agent, AgentConfig, AgentEvent, AgentTool, ToolExecutionResult};
    use tau_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, TauAiError,
        ToolDefinition,
    };
    use tokio::sync::Mutex as AsyncMutex;

    struct QueueClient {
        responses: AsyncMutex<VecDeque<ChatResponse>>,
    }

    #[async_trait]
    impl LlmClient for QueueClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            let mut responses = self.responses.lock().await;
            responses.pop_front().ok_or_else(|| {
                TauAiError::InvalidResponse("queue client has no responses".to_string())
            })
        }
    }

    struct EchoTool;

    #[async_trait]
    impl AgentTool for EchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".to_string(),
                description: "echo tool for tests".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"}
                    },
                    "required": ["text"],
                    "additionalProperties": false
                }),
            }
        }

        async fn execute(&self, arguments: Value) -> ToolExecutionResult {
            ToolExecutionResult::ok(serde_json::json!({ "echo": arguments["text"] }))
        }
    }

    fn build_tool_loop_agent() -> Agent {
        let responses = VecDeque::from(vec![
            ChatResponse {
                message: Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call-1".to_string(),
                    name: "echo".to_string(),
                    arguments: serde_json::json!({ "text": "hello" }),
                }]),
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ]);
        let mut agent = Agent::new(
            Arc::new(QueueClient {
                responses: AsyncMutex::new(responses),
            }),
            AgentConfig::default(),
        );
        agent.register_tool(EchoTool);
        agent
    }

    #[test]
    fn unit_extension_tool_hook_dispatch_maps_start_event_payload() {
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::json!({"path":"README.md"}),
        };
        let (hook, payload) = extension_tool_hook_dispatch(&event).expect("dispatch payload");
        assert_eq!(hook, "pre-tool-call");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "pre-tool-call");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["tool_call_id"], "call-1");
        assert_eq!(payload["data"]["tool_name"], "read");
        assert_eq!(payload["data"]["arguments"]["path"], "README.md");
    }

    #[test]
    fn functional_extension_tool_hook_dispatch_maps_end_event_payload() {
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({"content":"hello"})),
        };
        let (hook, payload) = extension_tool_hook_dispatch(&event).expect("dispatch payload");
        assert_eq!(hook, "post-tool-call");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "post-tool-call");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["tool_call_id"], "call-1");
        assert_eq!(payload["data"]["tool_name"], "read");
        assert_eq!(payload["data"]["result"]["is_error"], false);
        assert_eq!(payload["data"]["result"]["content"]["content"], "hello");
    }

    #[test]
    fn regression_extension_tool_hook_dispatch_ignores_non_tool_events() {
        let event = AgentEvent::AgentStart;
        assert!(extension_tool_hook_dispatch(&event).is_none());
    }

    #[test]
    fn functional_extension_tool_hook_diagnostics_routes_dispatch_payload() {
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::json!({"path":"README.md"}),
        };

        let diagnostics = extension_tool_hook_diagnostics(
            &event,
            Path::new("/tmp/extensions"),
            &|root, hook, payload| {
                assert_eq!(root, Path::new("/tmp/extensions"));
                assert_eq!(hook, "pre-tool-call");
                assert_eq!(payload["tool_name"], "read");
                vec!["ok".to_string()]
            },
        );

        assert_eq!(diagnostics, vec!["ok".to_string()]);
    }

    #[tokio::test]
    async fn integration_register_runtime_extension_tool_hook_subscriber_dispatches_hooks() {
        let mut agent = build_tool_loop_agent();
        let extension_root = Path::new("/tmp/extensions").to_path_buf();
        let captured = Arc::new(Mutex::new(Vec::<(String, String, Value)>::new()));
        let sink = Arc::clone(&captured);

        register_runtime_extension_tool_hook_subscriber(
            &mut agent,
            true,
            extension_root,
            move |root, hook, payload| {
                sink.lock().expect("capture lock").push((
                    root.display().to_string(),
                    hook.to_string(),
                    payload.clone(),
                ));
                Vec::new()
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        let rows = captured.lock().expect("capture lock");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "/tmp/extensions");
        assert_eq!(rows[0].1, "pre-tool-call");
        assert_eq!(rows[1].1, "post-tool-call");
        assert_eq!(rows[0].2["data"]["tool_name"], "echo");
        assert_eq!(rows[1].2["data"]["tool_name"], "echo");
    }

    #[tokio::test]
    async fn regression_register_runtime_extension_tool_hook_subscriber_disabled_noops() {
        let mut agent = build_tool_loop_agent();
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&captured);

        register_runtime_extension_tool_hook_subscriber(
            &mut agent,
            false,
            Path::new("/tmp/extensions").to_path_buf(),
            move |_root, hook, _payload| {
                sink.lock().expect("capture lock").push(hook.to_string());
                Vec::new()
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert!(captured.lock().expect("capture lock").is_empty());
    }
}
