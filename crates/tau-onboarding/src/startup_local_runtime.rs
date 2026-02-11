use std::future::Future;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptEntryRuntimeMode {
    Prompt(String),
    PlanFirstPrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBootstrapOutcome<TSession, TMessage> {
    pub runtime: TSession,
    pub lineage: Vec<TMessage>,
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

pub fn resolve_prompt_entry_runtime_mode(
    entry_mode: &LocalRuntimeEntryMode,
) -> Option<PromptEntryRuntimeMode> {
    match entry_mode {
        LocalRuntimeEntryMode::Prompt(prompt) => {
            Some(PromptEntryRuntimeMode::Prompt(prompt.clone()))
        }
        LocalRuntimeEntryMode::PlanFirstPrompt(prompt) => {
            Some(PromptEntryRuntimeMode::PlanFirstPrompt(prompt.clone()))
        }
        LocalRuntimeEntryMode::Interactive | LocalRuntimeEntryMode::CommandFile(_) => None,
    }
}

pub async fn execute_prompt_entry_mode<FRun, Fut>(
    entry_mode: &LocalRuntimeEntryMode,
    run_prompt_mode: FRun,
) -> Result<bool>
where
    FRun: FnOnce(PromptEntryRuntimeMode) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let Some(prompt_mode) = resolve_prompt_entry_runtime_mode(entry_mode) else {
        return Ok(false);
    };
    run_prompt_mode(prompt_mode).await?;
    Ok(true)
}

pub fn resolve_session_runtime<TSession, TMessage, FInit, FReplace>(
    no_session: bool,
    initialize_session: FInit,
    replace_messages: FReplace,
) -> Result<Option<TSession>>
where
    FInit: FnOnce() -> Result<SessionBootstrapOutcome<TSession, TMessage>>,
    FReplace: FnOnce(Vec<TMessage>),
{
    if no_session {
        return Ok(None);
    }

    let outcome = initialize_session()?;
    if !outcome.lineage.is_empty() {
        replace_messages(outcome.lineage);
    }
    Ok(Some(outcome.runtime))
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

pub fn register_runtime_extension_tools<T, FRegister, FReport>(
    agent: &mut Agent,
    registered_tools: &[T],
    diagnostics: &[String],
    register_tools: FRegister,
    mut report_diagnostic: FReport,
) where
    FRegister: FnOnce(&mut Agent, &[T]),
    FReport: FnMut(&str),
{
    register_tools(agent, registered_tools);
    for diagnostic in diagnostics {
        report_diagnostic(diagnostic);
    }
}

pub fn register_runtime_json_event_subscriber<FRender, FEmit>(
    agent: &mut Agent,
    enabled: bool,
    render_event: FRender,
    emit_json: FEmit,
) where
    FRender: Fn(&AgentEvent) -> Value + Send + Sync + 'static,
    FEmit: Fn(&Value) + Send + Sync + 'static,
{
    if !enabled {
        return;
    }

    agent.subscribe(move |event| {
        let value = render_event(event);
        emit_json(&value);
    });
}

pub fn register_runtime_event_reporter_subscriber<FReport, FEmit, E>(
    agent: &mut Agent,
    report_event: FReport,
    emit_error: FEmit,
) where
    FReport: Fn(&AgentEvent) -> std::result::Result<(), E> + Send + Sync + 'static,
    FEmit: Fn(&str) + Send + Sync + 'static,
    E: std::fmt::Display,
{
    agent.subscribe(move |event| {
        if let Err(error) = report_event(event) {
            let message = error.to_string();
            emit_error(&message);
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
        execute_prompt_entry_mode, extension_tool_hook_diagnostics, extension_tool_hook_dispatch,
        register_runtime_event_reporter_subscriber,
        register_runtime_extension_tool_hook_subscriber, register_runtime_extension_tools,
        register_runtime_json_event_subscriber, resolve_extension_runtime_registrations,
        resolve_local_runtime_entry_mode, resolve_orchestrator_route_table,
        resolve_prompt_entry_runtime_mode, resolve_prompt_runtime_mode, resolve_session_runtime,
        LocalRuntimeEntryMode, PromptEntryRuntimeMode, PromptRuntimeMode, SessionBootstrapOutcome,
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
    fn unit_resolve_prompt_entry_runtime_mode_returns_none_for_interactive() {
        assert_eq!(
            resolve_prompt_entry_runtime_mode(&LocalRuntimeEntryMode::Interactive),
            None
        );
    }

    #[tokio::test]
    async fn functional_execute_prompt_entry_mode_runs_prompt_variant() {
        let mode = LocalRuntimeEntryMode::Prompt("prompt text".to_string());
        let handled = execute_prompt_entry_mode(&mode, |prompt_mode| async move {
            assert_eq!(
                prompt_mode,
                PromptEntryRuntimeMode::Prompt("prompt text".to_string())
            );
            Ok(())
        })
        .await
        .expect("prompt dispatch should succeed");
        assert!(handled);
    }

    #[tokio::test]
    async fn integration_execute_prompt_entry_mode_runs_plan_first_variant() {
        let mode = LocalRuntimeEntryMode::PlanFirstPrompt("plan text".to_string());
        let handled = execute_prompt_entry_mode(&mode, |prompt_mode| async move {
            assert_eq!(
                prompt_mode,
                PromptEntryRuntimeMode::PlanFirstPrompt("plan text".to_string())
            );
            Ok(())
        })
        .await
        .expect("plan-first dispatch should succeed");
        assert!(handled);
    }

    #[tokio::test]
    async fn regression_execute_prompt_entry_mode_propagates_callback_errors() {
        let mode = LocalRuntimeEntryMode::Prompt("prompt text".to_string());
        let error = execute_prompt_entry_mode(&mode, |_prompt_mode| async {
            Err(anyhow::anyhow!("forced prompt dispatch failure"))
        })
        .await
        .expect_err("callback failures should propagate");
        assert!(
            error.to_string().contains("forced prompt dispatch failure"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn unit_resolve_session_runtime_no_session_skips_initialization() {
        let init_called = AtomicBool::new(false);
        let runtime = resolve_session_runtime::<u64, String, _, _>(
            true,
            || {
                init_called.store(true, Ordering::Relaxed);
                Ok(SessionBootstrapOutcome {
                    runtime: 42,
                    lineage: Vec::new(),
                })
            },
            |_lineage| panic!("lineage replay should not run when no-session is enabled"),
        )
        .expect("session resolution should succeed");
        assert_eq!(runtime, None);
        assert!(!init_called.load(Ordering::Relaxed));
    }

    #[test]
    fn functional_resolve_session_runtime_initializes_without_lineage_replay() {
        let replay_called = AtomicBool::new(false);
        let runtime = resolve_session_runtime(
            false,
            || {
                Ok(SessionBootstrapOutcome {
                    runtime: "runtime".to_string(),
                    lineage: Vec::<String>::new(),
                })
            },
            |_lineage| replay_called.store(true, Ordering::Relaxed),
        )
        .expect("session resolution should succeed");
        assert_eq!(runtime.as_deref(), Some("runtime"));
        assert!(!replay_called.load(Ordering::Relaxed));
    }

    #[test]
    fn integration_resolve_session_runtime_replays_non_empty_lineage() {
        let replayed = Arc::new(Mutex::new(Vec::<String>::new()));
        let replayed_sink = Arc::clone(&replayed);

        let runtime = resolve_session_runtime(
            false,
            || {
                Ok(SessionBootstrapOutcome {
                    runtime: 7usize,
                    lineage: vec!["msg-a".to_string(), "msg-b".to_string()],
                })
            },
            move |lineage| {
                replayed_sink.lock().expect("replay lock").extend(lineage);
            },
        )
        .expect("session resolution should succeed");

        assert_eq!(runtime, Some(7));
        assert_eq!(
            replayed.lock().expect("replay lock").as_slice(),
            ["msg-a", "msg-b"]
        );
    }

    #[test]
    fn regression_resolve_session_runtime_propagates_initializer_error() {
        let error = resolve_session_runtime::<u8, String, _, _>(
            false,
            || Err(anyhow::anyhow!("initializer failed")),
            |_lineage| panic!("lineage replay should not run when init fails"),
        )
        .expect_err("initializer failure should be propagated");

        assert!(error.to_string().contains("initializer failed"));
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

    #[test]
    fn unit_register_runtime_extension_tools_invokes_tool_registration_with_payload() {
        let mut agent = build_tool_loop_agent();
        let captured_tools = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&captured_tools);
        let tools = vec!["tool-a".to_string(), "tool-b".to_string()];

        register_runtime_extension_tools(
            &mut agent,
            &tools,
            &[],
            move |_agent, registered_tools| {
                sink.lock()
                    .expect("capture lock")
                    .extend(registered_tools.iter().cloned());
            },
            |_diagnostic| {},
        );

        assert_eq!(
            captured_tools.lock().expect("capture lock").as_slice(),
            ["tool-a", "tool-b"]
        );
    }

    #[test]
    fn functional_register_runtime_extension_tools_reports_all_diagnostics_in_order() {
        let mut agent = build_tool_loop_agent();
        let captured_diagnostics = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&captured_diagnostics);
        let diagnostics = vec!["first".to_string(), "second".to_string()];

        register_runtime_extension_tools(
            &mut agent,
            &[],
            &diagnostics,
            |_agent, _registered_tools: &[String]| {},
            move |diagnostic| {
                sink.lock()
                    .expect("capture lock")
                    .push(diagnostic.to_string())
            },
        );

        assert_eq!(
            captured_diagnostics
                .lock()
                .expect("capture lock")
                .as_slice(),
            ["first", "second"]
        );
    }

    #[test]
    fn integration_register_runtime_extension_tools_registers_and_reports_together() {
        let mut agent = build_tool_loop_agent();
        let captured_tools = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured_diagnostics = Arc::new(Mutex::new(Vec::<String>::new()));
        let tools_sink = Arc::clone(&captured_tools);
        let diagnostics_sink = Arc::clone(&captured_diagnostics);
        let tools = vec!["tool-x".to_string()];
        let diagnostics = vec!["diag-x".to_string()];

        register_runtime_extension_tools(
            &mut agent,
            &tools,
            &diagnostics,
            move |_agent, registered_tools| {
                tools_sink
                    .lock()
                    .expect("capture lock")
                    .extend(registered_tools.iter().cloned());
            },
            move |diagnostic| {
                diagnostics_sink
                    .lock()
                    .expect("capture lock")
                    .push(diagnostic.to_string());
            },
        );

        assert_eq!(
            captured_tools.lock().expect("capture lock").as_slice(),
            ["tool-x"]
        );
        assert_eq!(
            captured_diagnostics
                .lock()
                .expect("capture lock")
                .as_slice(),
            ["diag-x"]
        );
    }

    #[test]
    fn regression_register_runtime_extension_tools_handles_empty_inputs() {
        let mut agent = build_tool_loop_agent();
        let diagnostics_count = Arc::new(Mutex::new(0usize));
        let diagnostics_sink = Arc::clone(&diagnostics_count);

        register_runtime_extension_tools(
            &mut agent,
            &Vec::<String>::new(),
            &[],
            |_agent, _registered_tools| {},
            move |_diagnostic| {
                let mut guard = diagnostics_sink.lock().expect("capture lock");
                *guard += 1;
            },
        );

        assert_eq!(*diagnostics_count.lock().expect("capture lock"), 0);
    }

    #[tokio::test]
    async fn unit_register_runtime_json_event_subscriber_disabled_noops() {
        let mut agent = build_tool_loop_agent();
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&captured);

        register_runtime_json_event_subscriber(
            &mut agent,
            false,
            |_event| serde_json::json!({"kind": "ignored"}),
            move |value| {
                sink.lock()
                    .expect("capture lock")
                    .push(value["kind"].as_str().unwrap_or("missing").to_string());
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert!(captured.lock().expect("capture lock").is_empty());
    }

    #[tokio::test]
    async fn functional_register_runtime_json_event_subscriber_emits_rendered_values() {
        let mut agent = build_tool_loop_agent();
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&captured);

        register_runtime_json_event_subscriber(
            &mut agent,
            true,
            |event| match event {
                AgentEvent::ToolExecutionStart { .. } => serde_json::json!({"kind":"start"}),
                AgentEvent::ToolExecutionEnd { .. } => serde_json::json!({"kind":"end"}),
                _ => serde_json::json!({"kind":"other"}),
            },
            move |value| {
                sink.lock()
                    .expect("capture lock")
                    .push(value["kind"].as_str().unwrap_or("missing").to_string());
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        let events = captured.lock().expect("capture lock").clone();
        assert!(events.iter().any(|kind| kind == "start"));
        assert!(events.iter().any(|kind| kind == "end"));
    }

    #[tokio::test]
    async fn integration_register_runtime_event_reporter_subscriber_captures_report_errors() {
        let mut agent = build_tool_loop_agent();
        let captured_errors = Arc::new(Mutex::new(Vec::<String>::new()));
        let errors_sink = Arc::clone(&captured_errors);

        register_runtime_event_reporter_subscriber(
            &mut agent,
            |event| match event {
                AgentEvent::ToolExecutionStart { .. } => Err("start failed"),
                _ => Ok(()),
            },
            move |error| {
                errors_sink
                    .lock()
                    .expect("capture lock")
                    .push(error.to_string())
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert!(captured_errors
            .lock()
            .expect("capture lock")
            .iter()
            .any(|message| message == "start failed"));
    }

    #[tokio::test]
    async fn regression_register_runtime_event_reporter_subscriber_does_not_interrupt_prompt() {
        let mut agent = build_tool_loop_agent();
        let captured_errors = Arc::new(Mutex::new(Vec::<String>::new()));
        let errors_sink = Arc::clone(&captured_errors);

        register_runtime_event_reporter_subscriber(
            &mut agent,
            |_event| Err("forced reporter failure"),
            move |error| {
                errors_sink
                    .lock()
                    .expect("capture lock")
                    .push(error.to_string())
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert!(!captured_errors.lock().expect("capture lock").is_empty());
    }
}
