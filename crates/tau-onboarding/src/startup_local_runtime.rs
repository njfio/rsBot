use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::Value;
use tau_agent_core::{Agent, AgentEvent};
use tau_core::current_unix_timestamp_ms;

const EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptRuntimeMode {
    None,
    Prompt(String),
    PlanFirstPrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalRuntimeEntryMode {
    Interactive,
    CommandFile(PathBuf),
    Prompt(String),
    PlanFirstPrompt(String),
}

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

pub fn resolve_prompt_runtime_mode(
    prompt: Option<String>,
    plan_first_mode: bool,
) -> PromptRuntimeMode {
    match prompt {
        Some(prompt) if plan_first_mode => PromptRuntimeMode::PlanFirstPrompt(prompt),
        Some(prompt) => PromptRuntimeMode::Prompt(prompt),
        None => PromptRuntimeMode::None,
    }
}

pub fn resolve_local_runtime_entry_mode(
    prompt: Option<String>,
    plan_first_mode: bool,
    command_file: Option<&Path>,
) -> LocalRuntimeEntryMode {
    match resolve_prompt_runtime_mode(prompt, plan_first_mode) {
        PromptRuntimeMode::PlanFirstPrompt(prompt) => {
            LocalRuntimeEntryMode::PlanFirstPrompt(prompt)
        }
        PromptRuntimeMode::Prompt(prompt) => LocalRuntimeEntryMode::Prompt(prompt),
        PromptRuntimeMode::None => command_file
            .map(|path| LocalRuntimeEntryMode::CommandFile(path.to_path_buf()))
            .unwrap_or(LocalRuntimeEntryMode::Interactive),
    }
}

pub fn resolve_orchestrator_route_table<T, F>(
    route_table_path: Option<&Path>,
    load_route_table: F,
) -> Result<T>
where
    T: Default,
    F: FnOnce(&Path) -> Result<T>,
{
    if let Some(path) = route_table_path {
        load_route_table(path)
    } else {
        Ok(T::default())
    }
}

pub fn resolve_extension_runtime_registrations<T, FDiscover, FEmpty>(
    enabled: bool,
    root: &Path,
    discover: FDiscover,
    empty: FEmpty,
) -> T
where
    FDiscover: FnOnce(&Path) -> T,
    FEmpty: FnOnce(&Path) -> T,
{
    if enabled {
        discover(root)
    } else {
        empty(root)
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
        register_runtime_extension_tool_hook_subscriber, resolve_extension_runtime_registrations,
        resolve_local_runtime_entry_mode, resolve_orchestrator_route_table,
        resolve_prompt_runtime_mode, LocalRuntimeEntryMode, PromptRuntimeMode,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
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
    fn unit_resolve_orchestrator_route_table_returns_default_when_unset() {
        let table: Vec<String> =
            resolve_orchestrator_route_table::<Vec<String>, _>(None, |_path| {
                panic!("loader should not be called when route table path is unset")
            })
            .expect("default table");
        assert!(table.is_empty());
    }

    #[test]
    fn functional_resolve_orchestrator_route_table_uses_loader_when_path_is_set() {
        let loaded =
            resolve_orchestrator_route_table(Some(Path::new("/tmp/route-table.json")), |path| {
                Ok(vec![path.display().to_string()])
            })
            .expect("loaded table");
        assert_eq!(loaded, vec!["/tmp/route-table.json".to_string()]);
    }

    #[test]
    fn integration_resolve_extension_runtime_registrations_uses_discover_when_enabled() {
        let root = PathBuf::from("/tmp/extensions");
        let result = resolve_extension_runtime_registrations(
            true,
            &root,
            |path| vec![format!("discover:{}", path.display())],
            |_path| vec!["empty".to_string()],
        );
        assert_eq!(result, vec!["discover:/tmp/extensions".to_string()]);
    }

    #[test]
    fn regression_resolve_extension_runtime_registrations_uses_empty_when_disabled() {
        let discover_called = AtomicBool::new(false);
        let root = PathBuf::from("/tmp/extensions");
        let result = resolve_extension_runtime_registrations(
            false,
            &root,
            |_path| {
                discover_called.store(true, Ordering::Relaxed);
                vec!["discover".to_string()]
            },
            |_path| vec!["empty".to_string()],
        );
        assert_eq!(result, vec!["empty".to_string()]);
        assert!(!discover_called.load(Ordering::Relaxed));
    }

    #[test]
    fn unit_resolve_prompt_runtime_mode_defaults_to_none() {
        assert_eq!(
            resolve_prompt_runtime_mode(None, false),
            PromptRuntimeMode::None
        );
    }

    #[test]
    fn functional_resolve_prompt_runtime_mode_selects_prompt_mode() {
        assert_eq!(
            resolve_prompt_runtime_mode(Some("hello".to_string()), false),
            PromptRuntimeMode::Prompt("hello".to_string())
        );
    }

    #[test]
    fn integration_resolve_prompt_runtime_mode_selects_plan_first_prompt_mode() {
        assert_eq!(
            resolve_prompt_runtime_mode(Some("hello".to_string()), true),
            PromptRuntimeMode::PlanFirstPrompt("hello".to_string())
        );
    }

    #[test]
    fn regression_resolve_prompt_runtime_mode_preserves_whitespace_prompt_text() {
        assert_eq!(
            resolve_prompt_runtime_mode(Some("  keep me  ".to_string()), true),
            PromptRuntimeMode::PlanFirstPrompt("  keep me  ".to_string())
        );
    }

    #[test]
    fn unit_resolve_local_runtime_entry_mode_defaults_to_interactive() {
        assert_eq!(
            resolve_local_runtime_entry_mode(None, false, None),
            LocalRuntimeEntryMode::Interactive
        );
    }

    #[test]
    fn functional_resolve_local_runtime_entry_mode_prefers_prompt_over_command_file() {
        assert_eq!(
            resolve_local_runtime_entry_mode(
                Some("prompt text".to_string()),
                false,
                Some(Path::new("commands.txt")),
            ),
            LocalRuntimeEntryMode::Prompt("prompt text".to_string())
        );
    }

    #[test]
    fn integration_resolve_local_runtime_entry_mode_selects_command_file_without_prompt() {
        assert_eq!(
            resolve_local_runtime_entry_mode(None, false, Some(Path::new("commands.txt"))),
            LocalRuntimeEntryMode::CommandFile(PathBuf::from("commands.txt"))
        );
    }

    #[test]
    fn regression_resolve_local_runtime_entry_mode_selects_plan_first_prompt() {
        assert_eq!(
            resolve_local_runtime_entry_mode(Some("plan text".to_string()), true, None),
            LocalRuntimeEntryMode::PlanFirstPrompt("plan text".to_string())
        );
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
