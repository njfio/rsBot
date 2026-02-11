use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, ModelRef};
use tau_cli::{Cli, CliOrchestratorMode};
use tau_core::current_unix_timestamp_ms;
use tau_diagnostics::{build_doctor_command_config, DoctorCommandConfig};
use tau_provider::AuthCommandConfig;
use tau_tools::tools::{register_builtin_tools, ToolPolicy};

use crate::startup_config::{build_auth_command_config, build_profile_defaults, ProfileDefaults};

const EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

pub fn build_local_runtime_agent(
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    max_turns: usize,
    tool_policy: ToolPolicy,
) -> Agent {
    let mut agent = Agent::new(
        client,
        AgentConfig {
            model: model_ref.model.clone(),
            system_prompt: system_prompt.to_string(),
            max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );
    register_builtin_tools(&mut agent, tool_policy);
    agent
}

pub fn build_local_runtime_doctor_config(
    cli: &Cli,
    model_ref: &ModelRef,
    fallback_model_refs: &[ModelRef],
    skills_dir: &Path,
    skills_lock_path: &Path,
) -> DoctorCommandConfig {
    let mut doctor_config =
        build_doctor_command_config(cli, model_ref, fallback_model_refs, skills_lock_path);
    doctor_config.skills_dir = skills_dir.to_path_buf();
    doctor_config.skills_lock_path = skills_lock_path.to_path_buf();
    doctor_config
}

#[derive(Debug, Clone)]
pub struct LocalRuntimeCommandDefaults {
    pub profile_defaults: ProfileDefaults,
    pub auth_command_config: AuthCommandConfig,
    pub doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalRuntimeInteractiveDefaults {
    pub turn_timeout_ms: u64,
    pub orchestrator_mode: CliOrchestratorMode,
    pub orchestrator_max_plan_steps: usize,
    pub orchestrator_max_delegated_steps: usize,
    pub orchestrator_max_executor_response_chars: usize,
    pub orchestrator_max_delegated_step_response_chars: usize,
    pub orchestrator_max_delegated_total_response_chars: usize,
    pub orchestrator_delegate_steps: bool,
}

pub fn build_local_runtime_interactive_defaults(cli: &Cli) -> LocalRuntimeInteractiveDefaults {
    LocalRuntimeInteractiveDefaults {
        turn_timeout_ms: cli.turn_timeout_ms,
        orchestrator_mode: cli.orchestrator_mode,
        orchestrator_max_plan_steps: cli.orchestrator_max_plan_steps,
        orchestrator_max_delegated_steps: cli.orchestrator_max_delegated_steps,
        orchestrator_max_executor_response_chars: cli.orchestrator_max_executor_response_chars,
        orchestrator_max_delegated_step_response_chars: cli
            .orchestrator_max_delegated_step_response_chars,
        orchestrator_max_delegated_total_response_chars: cli
            .orchestrator_max_delegated_total_response_chars,
        orchestrator_delegate_steps: cli.orchestrator_delegate_steps,
    }
}

pub fn build_local_runtime_command_defaults(
    cli: &Cli,
    model_ref: &ModelRef,
    fallback_model_refs: &[ModelRef],
    skills_dir: &Path,
    skills_lock_path: &Path,
) -> LocalRuntimeCommandDefaults {
    LocalRuntimeCommandDefaults {
        profile_defaults: build_profile_defaults(cli),
        auth_command_config: build_auth_command_config(cli),
        doctor_config: build_local_runtime_doctor_config(
            cli,
            model_ref,
            fallback_model_refs,
            skills_dir,
            skills_lock_path,
        ),
    }
}

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
pub enum PromptOrCommandFileEntryOutcome {
    PromptHandled,
    CommandFile(PathBuf),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBootstrapOutcome<TSession, TMessage> {
    pub runtime: TSession,
    pub lineage: Vec<TMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRuntimeExtensionBootstrap<TOrchestratorRouteTable, TExtensionRuntimeRegistrations> {
    pub orchestrator_route_table: TOrchestratorRouteTable,
    pub orchestrator_route_trace_log: Option<PathBuf>,
    pub extension_runtime_registrations: TExtensionRuntimeRegistrations,
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

pub fn resolve_command_file_entry_path(entry_mode: &LocalRuntimeEntryMode) -> Option<&Path> {
    match entry_mode {
        LocalRuntimeEntryMode::CommandFile(path) => Some(path.as_path()),
        LocalRuntimeEntryMode::Interactive
        | LocalRuntimeEntryMode::Prompt(_)
        | LocalRuntimeEntryMode::PlanFirstPrompt(_) => None,
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

pub fn execute_command_file_entry_mode<FRun>(
    entry_mode: &LocalRuntimeEntryMode,
    run_command_file: FRun,
) -> Result<bool>
where
    FRun: FnOnce(&Path) -> Result<()>,
{
    let Some(command_file_path) = resolve_command_file_entry_path(entry_mode) else {
        return Ok(false);
    };
    run_command_file(command_file_path)?;
    Ok(true)
}

pub async fn execute_prompt_or_command_file_entry_mode<FRunPrompt, FutPrompt>(
    entry_mode: &LocalRuntimeEntryMode,
    run_prompt_mode: FRunPrompt,
) -> Result<PromptOrCommandFileEntryOutcome>
where
    FRunPrompt: FnOnce(PromptEntryRuntimeMode) -> FutPrompt,
    FutPrompt: Future<Output = Result<()>>,
{
    if execute_prompt_entry_mode(entry_mode, run_prompt_mode).await? {
        return Ok(PromptOrCommandFileEntryOutcome::PromptHandled);
    }
    if let Some(path) = resolve_command_file_entry_path(entry_mode) {
        return Ok(PromptOrCommandFileEntryOutcome::CommandFile(
            path.to_path_buf(),
        ));
    }
    Ok(PromptOrCommandFileEntryOutcome::None)
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

pub fn resolve_session_runtime_from_cli<TSession, TMessage, FInit, FReplace>(
    cli: &Cli,
    system_prompt: &str,
    initialize_session: FInit,
    replace_messages: FReplace,
) -> Result<Option<TSession>>
where
    FInit: FnOnce(
        &Path,
        u64,
        u64,
        Option<u64>,
        &str,
    ) -> Result<SessionBootstrapOutcome<TSession, TMessage>>,
    FReplace: FnOnce(Vec<TMessage>),
{
    resolve_session_runtime(
        cli.no_session,
        || {
            initialize_session(
                &cli.session,
                cli.session_lock_wait_ms,
                cli.session_lock_stale_ms,
                cli.branch_from,
                system_prompt,
            )
        },
        replace_messages,
    )
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

pub fn build_local_runtime_extension_bootstrap<
    TOrchestratorRouteTable,
    TExtensionRuntimeRegistrations,
    FLoadRouteTable,
    FDiscoverRegistrations,
    FBuildEmptyRegistrations,
>(
    cli: &Cli,
    extension_runtime_hooks_enabled: bool,
    extension_runtime_root: &Path,
    load_route_table: FLoadRouteTable,
    discover_registrations: FDiscoverRegistrations,
    build_empty_registrations: FBuildEmptyRegistrations,
) -> Result<LocalRuntimeExtensionBootstrap<TOrchestratorRouteTable, TExtensionRuntimeRegistrations>>
where
    TOrchestratorRouteTable: Default,
    FLoadRouteTable: FnOnce(&Path) -> Result<TOrchestratorRouteTable>,
    FDiscoverRegistrations: FnOnce(&Path) -> TExtensionRuntimeRegistrations,
    FBuildEmptyRegistrations: FnOnce(&Path) -> TExtensionRuntimeRegistrations,
{
    let orchestrator_route_table = resolve_orchestrator_route_table(
        cli.orchestrator_route_table.as_deref(),
        load_route_table,
    )?;
    let extension_runtime_registrations = resolve_extension_runtime_registrations(
        extension_runtime_hooks_enabled,
        extension_runtime_root,
        discover_registrations,
        build_empty_registrations,
    );
    Ok(LocalRuntimeExtensionBootstrap {
        orchestrator_route_table,
        orchestrator_route_trace_log: cli.telemetry_log.clone(),
        extension_runtime_registrations,
    })
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

#[derive(Debug, Clone)]
pub struct RuntimeExtensionPipelineConfig<'a, T> {
    pub enabled: bool,
    pub root: PathBuf,
    pub registered_tools: &'a [T],
    pub diagnostics: &'a [String],
}

pub fn register_runtime_extension_pipeline<T, FRegister, FReport, FDispatch>(
    agent: &mut Agent,
    config: RuntimeExtensionPipelineConfig<'_, T>,
    register_tools: FRegister,
    report_diagnostic: FReport,
    dispatch_hook: FDispatch,
) where
    FRegister: FnOnce(&mut Agent, &[T]),
    FReport: FnMut(&str),
    FDispatch: Fn(&Path, &'static str, &Value) -> Vec<String> + Send + Sync + 'static,
{
    register_runtime_extension_tools(
        agent,
        config.registered_tools,
        config.diagnostics,
        register_tools,
        report_diagnostic,
    );
    register_runtime_extension_tool_hook_subscriber(
        agent,
        config.enabled,
        config.root,
        dispatch_hook,
    );
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

pub fn register_runtime_event_reporter_if_configured<TReporter, FOpen, FReport, FEmit, E>(
    agent: &mut Agent,
    path: Option<PathBuf>,
    open_reporter: FOpen,
    report_event: FReport,
    emit_error: FEmit,
) -> Result<bool>
where
    TReporter: Send + Sync + 'static,
    FOpen: FnOnce(PathBuf) -> Result<TReporter>,
    FReport: Fn(&TReporter, &AgentEvent) -> std::result::Result<(), E> + Send + Sync + 'static,
    FEmit: Fn(&str) + Send + Sync + 'static,
    E: std::fmt::Display,
{
    let Some(path) = path else {
        return Ok(false);
    };
    let reporter = open_reporter(path)?;
    register_runtime_event_reporter_subscriber(
        agent,
        move |event| report_event(&reporter, event),
        emit_error,
    );
    Ok(true)
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
        build_local_runtime_agent, build_local_runtime_command_defaults,
        build_local_runtime_doctor_config, build_local_runtime_extension_bootstrap,
        build_local_runtime_interactive_defaults, execute_command_file_entry_mode,
        execute_prompt_entry_mode, execute_prompt_or_command_file_entry_mode,
        extension_tool_hook_diagnostics, extension_tool_hook_dispatch,
        register_runtime_event_reporter_if_configured, register_runtime_event_reporter_subscriber,
        register_runtime_extension_pipeline, register_runtime_extension_tool_hook_subscriber,
        register_runtime_extension_tools, register_runtime_json_event_subscriber,
        resolve_command_file_entry_path, resolve_extension_runtime_registrations,
        resolve_local_runtime_entry_mode, resolve_orchestrator_route_table,
        resolve_prompt_entry_runtime_mode, resolve_prompt_runtime_mode, resolve_session_runtime,
        resolve_session_runtime_from_cli, LocalRuntimeEntryMode, LocalRuntimeInteractiveDefaults,
        PromptEntryRuntimeMode, PromptOrCommandFileEntryOutcome, PromptRuntimeMode,
        RuntimeExtensionPipelineConfig, SessionBootstrapOutcome,
    };
    use async_trait::async_trait;
    use clap::Parser;
    use serde_json::Value;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use tau_agent_core::{
        Agent, AgentConfig, AgentError, AgentEvent, AgentTool, ToolExecutionResult,
    };
    use tau_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, ModelRef,
        TauAiError, ToolDefinition,
    };
    use tau_cli::{Cli, CliOrchestratorMode};
    use tau_tools::tools::ToolPolicy;
    use tokio::sync::Mutex as AsyncMutex;

    struct QueueClient {
        responses: AsyncMutex<VecDeque<ChatResponse>>,
    }

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
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
    fn unit_build_local_runtime_agent_preserves_system_prompt_message() {
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client = Arc::new(QueueClient {
            responses: AsyncMutex::new(VecDeque::new()),
        });
        let agent = build_local_runtime_agent(
            client,
            &model_ref,
            "system prompt",
            4,
            ToolPolicy::new(vec![std::env::temp_dir()]),
        );
        assert_eq!(agent.messages().len(), 1);
        assert_eq!(agent.messages()[0].text_content(), "system prompt");
    }

    #[tokio::test]
    async fn functional_build_local_runtime_agent_registers_builtin_tools_with_model_identity() {
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let captured_request = Arc::new(Mutex::new(None));
        let client = Arc::new(RecordingRequestClient {
            captured_request: captured_request.clone(),
            response: ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        });
        let mut agent = build_local_runtime_agent(
            client,
            &model_ref,
            "system prompt",
            4,
            ToolPolicy::new(vec![std::env::temp_dir()]),
        );
        agent.prompt("hello").await.expect("prompt succeeds");
        let request = captured_request
            .lock()
            .expect("captured request lock")
            .clone()
            .expect("captured request");
        assert_eq!(request.model, "gpt-4o-mini");
        assert!(
            request.tools.iter().any(|tool| tool.name == "read"),
            "expected built-in read tool to be registered"
        );
    }

    #[tokio::test]
    async fn integration_build_local_runtime_agent_respects_max_turns_limit() {
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client = Arc::new(QueueClient {
            responses: AsyncMutex::new(VecDeque::new()),
        });
        let mut agent = build_local_runtime_agent(
            client,
            &model_ref,
            "system prompt",
            0,
            ToolPolicy::new(vec![std::env::temp_dir()]),
        );
        let error = agent
            .prompt("hello")
            .await
            .expect_err("max turns should fail");
        assert!(matches!(error, AgentError::MaxTurnsExceeded(0)));
    }

    #[test]
    fn regression_build_local_runtime_agent_skips_empty_system_prompt_message() {
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let client = Arc::new(QueueClient {
            responses: AsyncMutex::new(VecDeque::new()),
        });
        let agent = build_local_runtime_agent(
            client,
            &model_ref,
            "   ",
            4,
            ToolPolicy::new(vec![std::env::temp_dir()]),
        );
        assert!(agent.messages().is_empty());
    }

    struct RecordingRequestClient {
        captured_request: Arc<Mutex<Option<ChatRequest>>>,
        response: ChatResponse,
    }

    #[async_trait]
    impl LlmClient for RecordingRequestClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            *self.captured_request.lock().expect("capture lock") = Some(request);
            Ok(self.response.clone())
        }
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
    fn unit_build_local_runtime_extension_bootstrap_preserves_trace_log_default_route() {
        let mut cli = parse_cli_with_stack();
        cli.telemetry_log = Some(PathBuf::from("logs/telemetry.ndjson"));
        let extension_root = Path::new("/tmp/extensions");

        let bootstrap =
            build_local_runtime_extension_bootstrap::<Vec<String>, Vec<String>, _, _, _>(
                &cli,
                false,
                extension_root,
                |_path| panic!("route loader should not be called without route table path"),
                |_path| panic!("discover should not be called when runtime hooks are disabled"),
                |path| vec![format!("empty:{}", path.display())],
            )
            .expect("bootstrap");

        assert!(bootstrap.orchestrator_route_table.is_empty());
        assert_eq!(
            bootstrap.orchestrator_route_trace_log,
            Some(PathBuf::from("logs/telemetry.ndjson"))
        );
        assert_eq!(
            bootstrap.extension_runtime_registrations,
            vec!["empty:/tmp/extensions".to_string()]
        );
    }

    #[test]
    fn functional_build_local_runtime_extension_bootstrap_uses_loader_and_discovery_when_enabled() {
        let mut cli = parse_cli_with_stack();
        cli.orchestrator_route_table = Some(PathBuf::from("/tmp/route-table.json"));
        cli.telemetry_log = Some(PathBuf::from("/tmp/trace.ndjson"));
        let extension_root = Path::new("/tmp/extensions");

        let bootstrap = build_local_runtime_extension_bootstrap(
            &cli,
            true,
            extension_root,
            |path| Ok(vec![format!("route:{}", path.display())]),
            |path| vec![format!("discover:{}", path.display())],
            |_path| panic!("empty builder should not be used when runtime hooks are enabled"),
        )
        .expect("bootstrap");

        assert_eq!(
            bootstrap.orchestrator_route_table,
            vec!["route:/tmp/route-table.json".to_string()]
        );
        assert_eq!(
            bootstrap.orchestrator_route_trace_log,
            Some(PathBuf::from("/tmp/trace.ndjson"))
        );
        assert_eq!(
            bootstrap.extension_runtime_registrations,
            vec!["discover:/tmp/extensions".to_string()]
        );
    }

    #[test]
    fn integration_build_local_runtime_extension_bootstrap_uses_empty_when_hooks_disabled() {
        let cli = parse_cli_with_stack();
        let discover_called = AtomicBool::new(false);
        let extension_root = Path::new("/tmp/extensions");

        let bootstrap =
            build_local_runtime_extension_bootstrap::<Vec<String>, Vec<String>, _, _, _>(
                &cli,
                false,
                extension_root,
                |_path| panic!("route loader should not run when route table path is absent"),
                |_path| {
                    discover_called.store(true, Ordering::Relaxed);
                    vec!["discover".to_string()]
                },
                |_path| vec!["empty".to_string()],
            )
            .expect("bootstrap");

        assert!(!discover_called.load(Ordering::Relaxed));
        assert_eq!(
            bootstrap.extension_runtime_registrations,
            vec!["empty".to_string()]
        );
    }

    #[test]
    fn regression_build_local_runtime_extension_bootstrap_propagates_loader_errors() {
        let mut cli = parse_cli_with_stack();
        cli.orchestrator_route_table = Some(PathBuf::from("/tmp/route-table.json"));
        let extension_root = Path::new("/tmp/extensions");

        let error = build_local_runtime_extension_bootstrap::<Vec<String>, Vec<String>, _, _, _>(
            &cli,
            true,
            extension_root,
            |_path| Err(anyhow::anyhow!("route loader failed")),
            |_path| vec!["discover".to_string()],
            |_path| vec!["empty".to_string()],
        )
        .expect_err("loader error should propagate");

        assert!(error.to_string().contains("route loader failed"));
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
    fn unit_resolve_command_file_entry_path_returns_none_for_interactive() {
        assert_eq!(
            resolve_command_file_entry_path(&LocalRuntimeEntryMode::Interactive),
            None
        );
    }

    #[test]
    fn functional_execute_command_file_entry_mode_runs_callback_for_command_file_path() {
        let mode = LocalRuntimeEntryMode::CommandFile(PathBuf::from("commands.txt"));
        let captured = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
        let captured_sink = Arc::clone(&captured);

        let handled = execute_command_file_entry_mode(&mode, |path| {
            captured_sink
                .lock()
                .expect("lock captured paths")
                .push(path.to_path_buf());
            Ok(())
        })
        .expect("command-file dispatch should succeed");

        assert!(handled);
        let captured = captured.lock().expect("lock captured paths");
        assert_eq!(captured.as_slice(), &[PathBuf::from("commands.txt")]);
    }

    #[test]
    fn integration_execute_command_file_entry_mode_returns_false_for_prompt_entries() {
        let prompt_mode = LocalRuntimeEntryMode::Prompt("prompt text".to_string());
        let plan_first_mode = LocalRuntimeEntryMode::PlanFirstPrompt("prompt text".to_string());

        let prompt_handled = execute_command_file_entry_mode(&prompt_mode, |_| {
            panic!("prompt mode should not run command file callback")
        })
        .expect("prompt mode dispatch result");
        let plan_first_handled = execute_command_file_entry_mode(&plan_first_mode, |_| {
            panic!("plan-first prompt mode should not run command file callback")
        })
        .expect("plan-first mode dispatch result");

        assert!(!prompt_handled);
        assert!(!plan_first_handled);
    }

    #[test]
    fn regression_execute_command_file_entry_mode_propagates_callback_errors() {
        let mode = LocalRuntimeEntryMode::CommandFile(PathBuf::from("commands.txt"));

        let error = execute_command_file_entry_mode(&mode, |_path| {
            Err(anyhow::anyhow!("forced command-file dispatch failure"))
        })
        .expect_err("callback failures should propagate");

        assert!(
            error
                .to_string()
                .contains("forced command-file dispatch failure"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn unit_execute_prompt_or_command_file_entry_mode_returns_none_for_interactive() {
        let mode = LocalRuntimeEntryMode::Interactive;
        let outcome = execute_prompt_or_command_file_entry_mode(&mode, |_| async {
            panic!("interactive mode should not execute prompt callback")
        })
        .await
        .expect("interactive dispatch should succeed");

        assert_eq!(outcome, PromptOrCommandFileEntryOutcome::None);
    }

    #[tokio::test]
    async fn functional_execute_prompt_or_command_file_entry_mode_reports_prompt_handled() {
        let mode = LocalRuntimeEntryMode::Prompt("prompt text".to_string());
        let outcome = execute_prompt_or_command_file_entry_mode(&mode, |prompt_mode| async move {
            assert_eq!(
                prompt_mode,
                PromptEntryRuntimeMode::Prompt("prompt text".to_string())
            );
            Ok(())
        })
        .await
        .expect("prompt dispatch should succeed");

        assert_eq!(outcome, PromptOrCommandFileEntryOutcome::PromptHandled);
    }

    #[tokio::test]
    async fn integration_execute_prompt_or_command_file_entry_mode_returns_command_path() {
        let mode = LocalRuntimeEntryMode::CommandFile(PathBuf::from("commands.txt"));
        let outcome = execute_prompt_or_command_file_entry_mode(&mode, |_| async {
            panic!("command-file mode should not execute prompt callback")
        })
        .await
        .expect("command path resolution should succeed");

        assert_eq!(
            outcome,
            PromptOrCommandFileEntryOutcome::CommandFile(PathBuf::from("commands.txt"))
        );
    }

    #[tokio::test]
    async fn regression_execute_prompt_or_command_file_entry_mode_propagates_prompt_errors() {
        let mode = LocalRuntimeEntryMode::Prompt("prompt text".to_string());
        let error = execute_prompt_or_command_file_entry_mode(&mode, |_prompt_mode| async {
            Err(anyhow::anyhow!("forced merged dispatch failure"))
        })
        .await
        .expect_err("prompt callback errors should propagate");

        assert!(
            error.to_string().contains("forced merged dispatch failure"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn unit_build_local_runtime_doctor_config_uses_runtime_skills_paths() {
        let cli = parse_cli_with_stack();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let doctor_config = build_local_runtime_doctor_config(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert_eq!(doctor_config.skills_dir, PathBuf::from("runtime-skills"));
        assert_eq!(
            doctor_config.skills_lock_path,
            PathBuf::from("runtime-skills.lock.json")
        );
    }

    #[test]
    fn functional_build_local_runtime_doctor_config_keeps_primary_model_identity() {
        let cli = parse_cli_with_stack();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let doctor_config = build_local_runtime_doctor_config(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert_eq!(doctor_config.model, "openai/gpt-4o-mini");
    }

    #[test]
    fn integration_build_local_runtime_doctor_config_includes_fallback_providers() {
        let cli = parse_cli_with_stack();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let fallback_model_refs = vec![
            ModelRef::parse("anthropic/claude-sonnet-4").expect("fallback model"),
            ModelRef::parse("openai/gpt-4o-mini").expect("duplicate provider fallback model"),
        ];
        let doctor_config = build_local_runtime_doctor_config(
            &cli,
            &model_ref,
            &fallback_model_refs,
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        let providers = doctor_config
            .provider_keys
            .iter()
            .map(|entry| entry.provider.as_str())
            .collect::<Vec<_>>();
        assert!(providers.contains(&"openai"));
        assert!(providers.contains(&"anthropic"));
    }

    #[test]
    fn regression_build_local_runtime_doctor_config_respects_no_session_flag() {
        let mut cli = parse_cli_with_stack();
        cli.no_session = true;
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let doctor_config = build_local_runtime_doctor_config(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert!(!doctor_config.session_enabled);
    }

    #[test]
    fn unit_build_local_runtime_command_defaults_keeps_runtime_skills_paths() {
        let cli = parse_cli_with_stack();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");
        let defaults = build_local_runtime_command_defaults(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert_eq!(
            defaults.doctor_config.skills_dir,
            PathBuf::from("runtime-skills")
        );
        assert_eq!(
            defaults.doctor_config.skills_lock_path,
            PathBuf::from("runtime-skills.lock.json")
        );
    }

    #[test]
    fn functional_build_local_runtime_command_defaults_builds_profile_defaults() {
        let mut cli = parse_cli_with_stack();
        cli.no_session = true;
        cli.model = "openai/gpt-5-mini".to_string();
        cli.fallback_model = vec!["anthropic/claude-sonnet-4".to_string()];
        let model_ref = ModelRef::parse("openai/gpt-5-mini").expect("model ref");

        let defaults = build_local_runtime_command_defaults(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert_eq!(defaults.profile_defaults.model, "openai/gpt-5-mini");
        assert_eq!(
            defaults.profile_defaults.fallback_models,
            vec!["anthropic/claude-sonnet-4".to_string()]
        );
        assert!(!defaults.profile_defaults.session.enabled);
    }

    #[test]
    fn integration_build_local_runtime_command_defaults_builds_auth_config() {
        let mut cli = parse_cli_with_stack();
        cli.provider_subscription_strict = true;
        cli.openai_codex_backend = false;
        cli.openai_codex_cli = "tau-codex".to_string();
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

        let defaults = build_local_runtime_command_defaults(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert!(defaults.auth_command_config.provider_subscription_strict);
        assert!(!defaults.auth_command_config.openai_codex_backend);
        assert_eq!(defaults.auth_command_config.openai_codex_cli, "tau-codex");
    }

    #[test]
    fn regression_build_local_runtime_command_defaults_respects_no_session_for_doctor() {
        let mut cli = parse_cli_with_stack();
        cli.no_session = true;
        let model_ref = ModelRef::parse("openai/gpt-4o-mini").expect("model ref");

        let defaults = build_local_runtime_command_defaults(
            &cli,
            &model_ref,
            &[],
            Path::new("runtime-skills"),
            Path::new("runtime-skills.lock.json"),
        );

        assert!(!defaults.doctor_config.session_enabled);
    }

    #[test]
    fn unit_build_local_runtime_interactive_defaults_preserves_timeout_and_mode() {
        let mut cli = parse_cli_with_stack();
        cli.turn_timeout_ms = 11_000;
        cli.orchestrator_mode = CliOrchestratorMode::PlanFirst;

        let defaults = build_local_runtime_interactive_defaults(&cli);

        assert_eq!(defaults.turn_timeout_ms, 11_000);
        assert_eq!(defaults.orchestrator_mode, CliOrchestratorMode::PlanFirst);
    }

    #[test]
    fn functional_build_local_runtime_interactive_defaults_preserves_plan_and_delegate_budgets() {
        let mut cli = parse_cli_with_stack();
        cli.orchestrator_max_plan_steps = 7;
        cli.orchestrator_max_delegated_steps = 13;
        cli.orchestrator_delegate_steps = true;

        let defaults = build_local_runtime_interactive_defaults(&cli);

        assert_eq!(defaults.orchestrator_max_plan_steps, 7);
        assert_eq!(defaults.orchestrator_max_delegated_steps, 13);
        assert!(defaults.orchestrator_delegate_steps);
    }

    #[test]
    fn integration_build_local_runtime_interactive_defaults_preserves_response_budgets() {
        let mut cli = parse_cli_with_stack();
        cli.orchestrator_max_executor_response_chars = 2222;
        cli.orchestrator_max_delegated_step_response_chars = 3333;
        cli.orchestrator_max_delegated_total_response_chars = 4444;

        let defaults = build_local_runtime_interactive_defaults(&cli);

        assert_eq!(defaults.orchestrator_max_executor_response_chars, 2222);
        assert_eq!(
            defaults.orchestrator_max_delegated_step_response_chars,
            3333
        );
        assert_eq!(
            defaults.orchestrator_max_delegated_total_response_chars,
            4444
        );
    }

    #[test]
    fn regression_build_local_runtime_interactive_defaults_uses_default_cli_values() {
        let cli = parse_cli_with_stack();
        let defaults = build_local_runtime_interactive_defaults(&cli);
        let expected = LocalRuntimeInteractiveDefaults {
            turn_timeout_ms: cli.turn_timeout_ms,
            orchestrator_mode: cli.orchestrator_mode,
            orchestrator_max_plan_steps: cli.orchestrator_max_plan_steps,
            orchestrator_max_delegated_steps: cli.orchestrator_max_delegated_steps,
            orchestrator_max_executor_response_chars: cli.orchestrator_max_executor_response_chars,
            orchestrator_max_delegated_step_response_chars: cli
                .orchestrator_max_delegated_step_response_chars,
            orchestrator_max_delegated_total_response_chars: cli
                .orchestrator_max_delegated_total_response_chars,
            orchestrator_delegate_steps: cli.orchestrator_delegate_steps,
        };

        assert_eq!(defaults, expected);
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
    fn unit_resolve_session_runtime_from_cli_no_session_skips_initialization() {
        let mut cli = parse_cli_with_stack();
        cli.no_session = true;
        let init_called = AtomicBool::new(false);

        let runtime = resolve_session_runtime_from_cli::<u64, String, _, _>(
            &cli,
            "system prompt",
            |_session_path, _lock_wait_ms, _lock_stale_ms, _branch_from, _system_prompt| {
                init_called.store(true, Ordering::Relaxed);
                Ok(SessionBootstrapOutcome {
                    runtime: 7,
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
    fn functional_resolve_session_runtime_from_cli_forwards_cli_bootstrap_inputs() {
        let mut cli = parse_cli_with_stack();
        cli.no_session = false;
        cli.session = PathBuf::from("custom-session.json");
        cli.session_lock_wait_ms = 222;
        cli.session_lock_stale_ms = 333;
        cli.branch_from = Some(55);
        let replayed = Arc::new(Mutex::new(Vec::<String>::new()));
        let replayed_sink = Arc::clone(&replayed);

        let runtime = resolve_session_runtime_from_cli(
            &cli,
            "boot system prompt",
            |session_path, lock_wait_ms, lock_stale_ms, branch_from, system_prompt| {
                assert_eq!(session_path, Path::new("custom-session.json"));
                assert_eq!(lock_wait_ms, 222);
                assert_eq!(lock_stale_ms, 333);
                assert_eq!(branch_from, Some(55));
                assert_eq!(system_prompt, "boot system prompt");
                Ok(SessionBootstrapOutcome {
                    runtime: "runtime".to_string(),
                    lineage: vec!["lineage-message".to_string()],
                })
            },
            move |lineage| {
                replayed_sink.lock().expect("replay lock").extend(lineage);
            },
        )
        .expect("session resolution should succeed");

        assert_eq!(runtime.as_deref(), Some("runtime"));
        assert_eq!(
            replayed.lock().expect("replay lock").as_slice(),
            ["lineage-message"]
        );
    }

    #[test]
    fn integration_resolve_session_runtime_from_cli_returns_runtime_when_lineage_empty() {
        let mut cli = parse_cli_with_stack();
        cli.no_session = false;
        let replay_called = AtomicBool::new(false);

        let runtime = resolve_session_runtime_from_cli(
            &cli,
            "boot system prompt",
            |_session_path, _lock_wait_ms, _lock_stale_ms, _branch_from, _system_prompt| {
                Ok(SessionBootstrapOutcome {
                    runtime: 99usize,
                    lineage: Vec::<String>::new(),
                })
            },
            |_lineage| replay_called.store(true, Ordering::Relaxed),
        )
        .expect("session resolution should succeed");

        assert_eq!(runtime, Some(99));
        assert!(!replay_called.load(Ordering::Relaxed));
    }

    #[test]
    fn regression_resolve_session_runtime_from_cli_propagates_initializer_error() {
        let cli = parse_cli_with_stack();

        let error = resolve_session_runtime_from_cli::<u8, String, _, _>(
            &cli,
            "system prompt",
            |_session_path, _lock_wait_ms, _lock_stale_ms, _branch_from, _system_prompt| {
                Err(anyhow::anyhow!("session bootstrap failed"))
            },
            |_lineage| panic!("lineage replay should not run when init fails"),
        )
        .expect_err("initializer failure should be propagated");

        assert!(error.to_string().contains("session bootstrap failed"));
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
    async fn unit_register_runtime_extension_pipeline_disabled_still_registers_and_reports() {
        let mut agent = build_tool_loop_agent();
        let captured_diagnostics = Arc::new(Mutex::new(Vec::<String>::new()));
        let diagnostics_sink = Arc::clone(&captured_diagnostics);
        let captured_hooks = Arc::new(Mutex::new(Vec::<String>::new()));
        let hooks_sink = Arc::clone(&captured_hooks);

        register_runtime_extension_pipeline(
            &mut agent,
            RuntimeExtensionPipelineConfig {
                enabled: false,
                root: PathBuf::from("/tmp/extensions"),
                registered_tools: &["tool-a".to_string()],
                diagnostics: &["diag-a".to_string(), "diag-b".to_string()],
            },
            |_agent, _registered_tools| {},
            move |diagnostic| {
                diagnostics_sink
                    .lock()
                    .expect("capture lock")
                    .push(diagnostic.to_string());
            },
            move |_root, hook, _payload| {
                hooks_sink
                    .lock()
                    .expect("capture lock")
                    .push(hook.to_string());
                Vec::new()
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert_eq!(
            captured_diagnostics
                .lock()
                .expect("capture lock")
                .as_slice(),
            ["diag-a", "diag-b"]
        );
        assert!(captured_hooks.lock().expect("capture lock").is_empty());
    }

    #[tokio::test]
    async fn functional_register_runtime_extension_pipeline_enabled_dispatches_hooks() {
        let mut agent = build_tool_loop_agent();
        let captured_hooks = Arc::new(Mutex::new(Vec::<String>::new()));
        let hooks_sink = Arc::clone(&captured_hooks);

        register_runtime_extension_pipeline(
            &mut agent,
            RuntimeExtensionPipelineConfig {
                enabled: true,
                root: PathBuf::from("/tmp/extensions"),
                registered_tools: &[],
                diagnostics: &[],
            },
            |_agent, _registered_tools: &[String]| {},
            |_diagnostic| {},
            move |_root, hook, _payload| {
                hooks_sink
                    .lock()
                    .expect("capture lock")
                    .push(hook.to_string());
                Vec::new()
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert_eq!(
            captured_hooks.lock().expect("capture lock").as_slice(),
            ["pre-tool-call", "post-tool-call"]
        );
    }

    #[tokio::test]
    async fn integration_register_runtime_extension_pipeline_reports_hook_diagnostics() {
        let mut agent = build_tool_loop_agent();
        let captured_diagnostics = Arc::new(Mutex::new(Vec::<String>::new()));
        let diagnostics_sink = Arc::clone(&captured_diagnostics);
        let captured_hooks = Arc::new(Mutex::new(Vec::<String>::new()));
        let hooks_sink = Arc::clone(&captured_hooks);

        register_runtime_extension_pipeline(
            &mut agent,
            RuntimeExtensionPipelineConfig {
                enabled: true,
                root: PathBuf::from("/tmp/extensions"),
                registered_tools: &[],
                diagnostics: &["manifest-diag".to_string()],
            },
            |_agent, _registered_tools: &[String]| {},
            move |diagnostic| {
                diagnostics_sink
                    .lock()
                    .expect("capture lock")
                    .push(diagnostic.to_string());
            },
            move |_root, hook, _payload| {
                hooks_sink
                    .lock()
                    .expect("capture lock")
                    .push(hook.to_string());
                vec![format!("hook-{hook}")]
            },
        );

        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert_eq!(
            captured_diagnostics
                .lock()
                .expect("capture lock")
                .as_slice(),
            ["manifest-diag"]
        );
        assert_eq!(
            captured_hooks.lock().expect("capture lock").as_slice(),
            ["pre-tool-call", "post-tool-call"]
        );
    }

    #[tokio::test]
    async fn regression_register_runtime_extension_pipeline_ignores_non_tool_events_for_hooks() {
        let mut agent = Agent::new(
            Arc::new(QueueClient {
                responses: AsyncMutex::new(VecDeque::from([ChatResponse {
                    message: Message::assistant_text("done"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }])),
            }),
            AgentConfig::default(),
        );
        let captured_hooks = Arc::new(Mutex::new(Vec::<String>::new()));
        let hooks_sink = Arc::clone(&captured_hooks);

        register_runtime_extension_pipeline(
            &mut agent,
            RuntimeExtensionPipelineConfig {
                enabled: true,
                root: PathBuf::from("/tmp/extensions"),
                registered_tools: &[] as &[String],
                diagnostics: &[],
            },
            |_agent, _registered_tools| {},
            |_diagnostic| {},
            move |_root, hook, _payload| {
                hooks_sink
                    .lock()
                    .expect("capture lock")
                    .push(hook.to_string());
                Vec::new()
            },
        );

        let _ = agent.prompt("plain prompt").await.expect("prompt succeeds");
        assert!(captured_hooks.lock().expect("capture lock").is_empty());
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

    #[test]
    fn unit_register_runtime_event_reporter_if_configured_returns_false_when_path_missing() {
        let mut agent = build_tool_loop_agent();
        let open_called = Arc::new(AtomicBool::new(false));
        let open_called_sink = Arc::clone(&open_called);

        let registered = register_runtime_event_reporter_if_configured(
            &mut agent,
            None,
            move |_path| {
                open_called_sink.store(true, Ordering::Relaxed);
                Ok(())
            },
            |_reporter: &(), _event| Ok::<(), &'static str>(()),
            |_error| {},
        )
        .expect("optional reporter registration should succeed");

        assert!(!registered);
        assert!(!open_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn functional_register_runtime_event_reporter_if_configured_registers_and_reports_events()
    {
        let mut agent = build_tool_loop_agent();
        let observed_start_events = Arc::new(Mutex::new(0usize));
        let observed_start_events_sink = Arc::clone(&observed_start_events);

        let registered = register_runtime_event_reporter_if_configured(
            &mut agent,
            Some(PathBuf::from("telemetry.log")),
            move |_path| Ok(Arc::clone(&observed_start_events_sink)),
            |reporter, event| {
                if matches!(event, AgentEvent::ToolExecutionStart { .. }) {
                    let mut guard = reporter.lock().expect("reporter lock");
                    *guard += 1;
                }
                Ok::<(), &'static str>(())
            },
            |_error| {},
        )
        .expect("optional reporter registration should succeed");

        assert!(registered);
        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert!(*observed_start_events.lock().expect("reporter lock") > 0);
    }

    #[test]
    fn integration_register_runtime_event_reporter_if_configured_propagates_open_errors() {
        let mut agent = build_tool_loop_agent();

        let error = register_runtime_event_reporter_if_configured(
            &mut agent,
            Some(PathBuf::from("telemetry.log")),
            |_path| Err(anyhow::anyhow!("failed to open reporter")),
            |_reporter: &(), _event| Ok::<(), &'static str>(()),
            |_error| {},
        )
        .expect_err("open errors should propagate");

        assert!(
            error.to_string().contains("failed to open reporter"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn regression_register_runtime_event_reporter_if_configured_emits_report_errors() {
        let mut agent = build_tool_loop_agent();
        let captured_errors = Arc::new(Mutex::new(Vec::<String>::new()));
        let errors_sink = Arc::clone(&captured_errors);

        let registered = register_runtime_event_reporter_if_configured(
            &mut agent,
            Some(PathBuf::from("telemetry.log")),
            |_path| Ok(()),
            |_reporter: &(), _event| Err("forced configured reporter failure"),
            move |error| {
                errors_sink
                    .lock()
                    .expect("capture lock")
                    .push(error.to_string())
            },
        )
        .expect("optional reporter registration should succeed");

        assert!(registered);
        let _ = agent.prompt("run echo").await.expect("prompt succeeds");
        assert!(!captured_errors.lock().expect("capture lock").is_empty());
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
