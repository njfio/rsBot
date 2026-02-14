use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tau_agent_core::{Agent, AgentConfig, AgentEvent, SafetyMode, SafetyPolicy};
use tau_ai::{LlmClient, ModelRef};
use tau_cli::{Cli, CliOrchestratorMode};
use tau_core::current_unix_timestamp_ms;
use tau_diagnostics::{build_doctor_command_config, DoctorCommandConfig};
use tau_provider::AuthCommandConfig;
use tau_tools::tools::{register_builtin_tools, ToolPolicy};

use crate::startup_config::{build_auth_command_config, build_profile_defaults, ProfileDefaults};

const EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
/// Public struct `LocalRuntimeAgentSettings` used across Tau components.
pub struct LocalRuntimeAgentSettings {
    pub max_turns: usize,
    pub max_parallel_tool_calls: usize,
    pub max_context_messages: Option<usize>,
    pub request_max_retries: usize,
    pub request_retry_initial_backoff_ms: u64,
    pub request_retry_max_backoff_ms: u64,
    pub request_timeout_ms: Option<u64>,
    pub tool_timeout_ms: Option<u64>,
    pub model_input_cost_per_million: Option<f64>,
    pub model_output_cost_per_million: Option<f64>,
    pub cost_budget_usd: Option<f64>,
    pub cost_alert_thresholds_percent: Vec<u8>,
    pub prompt_sanitizer_enabled: bool,
    pub prompt_sanitizer_mode: SafetyMode,
    pub prompt_sanitizer_redaction_token: String,
}

pub fn build_local_runtime_agent(
    client: Arc<dyn LlmClient>,
    model_ref: &ModelRef,
    system_prompt: &str,
    settings: LocalRuntimeAgentSettings,
    tool_policy: ToolPolicy,
) -> Agent {
    let mut agent = Agent::new(
        client,
        AgentConfig {
            model: model_ref.model.clone(),
            system_prompt: system_prompt.to_string(),
            max_turns: settings.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            max_parallel_tool_calls: settings.max_parallel_tool_calls,
            max_context_messages: settings.max_context_messages,
            request_max_retries: settings.request_max_retries,
            request_retry_initial_backoff_ms: settings.request_retry_initial_backoff_ms,
            request_retry_max_backoff_ms: settings.request_retry_max_backoff_ms,
            request_timeout_ms: settings.request_timeout_ms,
            tool_timeout_ms: settings.tool_timeout_ms,
            model_input_cost_per_million: settings.model_input_cost_per_million,
            model_output_cost_per_million: settings.model_output_cost_per_million,
            cost_budget_usd: settings.cost_budget_usd,
            cost_alert_thresholds_percent: settings.cost_alert_thresholds_percent,
            ..AgentConfig::default()
        },
    );
    agent.set_safety_policy(SafetyPolicy {
        enabled: settings.prompt_sanitizer_enabled,
        mode: settings.prompt_sanitizer_mode,
        apply_to_inbound_messages: true,
        apply_to_tool_outputs: true,
        redaction_token: settings.prompt_sanitizer_redaction_token,
    });
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
/// Public struct `LocalRuntimeCommandDefaults` used across Tau components.
pub struct LocalRuntimeCommandDefaults {
    pub profile_defaults: ProfileDefaults,
    pub auth_command_config: AuthCommandConfig,
    pub doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `LocalRuntimeInteractiveDefaults` used across Tau components.
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
/// Enumerates supported `PromptRuntimeMode` values.
pub enum PromptRuntimeMode {
    None,
    Prompt(String),
    PlanFirstPrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `LocalRuntimeEntryMode` values.
pub enum LocalRuntimeEntryMode {
    Interactive,
    CommandFile(PathBuf),
    Prompt(String),
    PlanFirstPrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `PromptEntryRuntimeMode` values.
pub enum PromptEntryRuntimeMode {
    Prompt(String),
    PlanFirstPrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `PromptOrCommandFileEntryOutcome` values.
pub enum PromptOrCommandFileEntryOutcome {
    PromptHandled,
    CommandFile(PathBuf),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `PromptOrCommandFileEntryDispatch` values.
pub enum PromptOrCommandFileEntryDispatch {
    Prompt(PromptEntryRuntimeMode),
    CommandFile(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `LocalRuntimeEntryDispatch` values.
pub enum LocalRuntimeEntryDispatch {
    PlanFirstPrompt(String),
    Prompt(String),
    CommandFile(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `SessionBootstrapOutcome` used across Tau components.
pub struct SessionBootstrapOutcome<TSession, TMessage> {
    pub runtime: TSession,
    pub lineage: Vec<TMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `LocalRuntimeExtensionBootstrap` used across Tau components.
pub struct LocalRuntimeExtensionBootstrap<TOrchestratorRouteTable, TExtensionRuntimeRegistrations> {
    pub orchestrator_route_table: TOrchestratorRouteTable,
    pub orchestrator_route_trace_log: Option<PathBuf>,
    pub extension_runtime_registrations: TExtensionRuntimeRegistrations,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `LocalRuntimeExtensionHooksDefaults` used across Tau components.
pub struct LocalRuntimeExtensionHooksDefaults {
    pub enabled: bool,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `LocalRuntimeExtensionStartup` used across Tau components.
pub struct LocalRuntimeExtensionStartup<TOrchestratorRouteTable, TExtensionRuntimeRegistrations> {
    pub extension_hooks: LocalRuntimeExtensionHooksDefaults,
    pub bootstrap:
        LocalRuntimeExtensionBootstrap<TOrchestratorRouteTable, TExtensionRuntimeRegistrations>,
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

pub fn resolve_local_runtime_entry_mode_from_cli<FResolvePromptInput>(
    cli: &Cli,
    resolve_prompt_input: FResolvePromptInput,
) -> Result<LocalRuntimeEntryMode>
where
    FResolvePromptInput: FnOnce(&Cli) -> Result<Option<String>>,
{
    let interactive_defaults = build_local_runtime_interactive_defaults(cli);
    Ok(resolve_local_runtime_entry_mode(
        resolve_prompt_input(cli)?,
        interactive_defaults.orchestrator_mode == CliOrchestratorMode::PlanFirst,
        cli.command_file.as_deref(),
    ))
}

#[derive(Debug, Clone)]
/// Public struct `LocalRuntimeStartupResolution` used across Tau components.
pub struct LocalRuntimeStartupResolution {
    pub interactive_defaults: LocalRuntimeInteractiveDefaults,
    pub entry_mode: LocalRuntimeEntryMode,
    pub command_defaults: LocalRuntimeCommandDefaults,
}

pub fn resolve_local_runtime_startup_from_cli<FResolvePromptInput>(
    cli: &Cli,
    model_ref: &ModelRef,
    fallback_model_refs: &[ModelRef],
    skills_dir: &Path,
    skills_lock_path: &Path,
    resolve_prompt_input: FResolvePromptInput,
) -> Result<LocalRuntimeStartupResolution>
where
    FResolvePromptInput: FnOnce(&Cli) -> Result<Option<String>>,
{
    let interactive_defaults = build_local_runtime_interactive_defaults(cli);
    let entry_mode = resolve_local_runtime_entry_mode_from_cli(cli, resolve_prompt_input)?;
    let command_defaults = build_local_runtime_command_defaults(
        cli,
        model_ref,
        fallback_model_refs,
        skills_dir,
        skills_lock_path,
    );
    Ok(LocalRuntimeStartupResolution {
        interactive_defaults,
        entry_mode,
        command_defaults,
    })
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

pub async fn execute_prompt_or_command_file_entry_mode_with_dispatch<FRun, Fut>(
    entry_mode: &LocalRuntimeEntryMode,
    run_entry: FRun,
) -> Result<bool>
where
    FRun: FnOnce(PromptOrCommandFileEntryDispatch) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    if let Some(prompt_mode) = resolve_prompt_entry_runtime_mode(entry_mode) {
        run_entry(PromptOrCommandFileEntryDispatch::Prompt(prompt_mode)).await?;
        return Ok(true);
    }
    if let Some(path) = resolve_command_file_entry_path(entry_mode) {
        run_entry(PromptOrCommandFileEntryDispatch::CommandFile(
            path.to_path_buf(),
        ))
        .await?;
        return Ok(true);
    }
    Ok(false)
}

pub async fn execute_local_runtime_entry_mode_with_dispatch<FRun, Fut>(
    entry_mode: &LocalRuntimeEntryMode,
    run_entry: FRun,
) -> Result<bool>
where
    FRun: FnOnce(LocalRuntimeEntryDispatch) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    if let Some(prompt_mode) = resolve_prompt_entry_runtime_mode(entry_mode) {
        let dispatch = match prompt_mode {
            PromptEntryRuntimeMode::PlanFirstPrompt(prompt) => {
                LocalRuntimeEntryDispatch::PlanFirstPrompt(prompt)
            }
            PromptEntryRuntimeMode::Prompt(prompt) => LocalRuntimeEntryDispatch::Prompt(prompt),
        };
        run_entry(dispatch).await?;
        return Ok(true);
    }
    if let Some(path) = resolve_command_file_entry_path(entry_mode) {
        run_entry(LocalRuntimeEntryDispatch::CommandFile(path.to_path_buf())).await?;
        return Ok(true);
    }
    Ok(false)
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

pub fn build_local_runtime_extension_startup<
    TOrchestratorRouteTable,
    TExtensionRuntimeRegistrations,
    FLoadRouteTable,
    FDiscoverRegistrations,
    FBuildEmptyRegistrations,
>(
    cli: &Cli,
    load_route_table: FLoadRouteTable,
    discover_registrations: FDiscoverRegistrations,
    build_empty_registrations: FBuildEmptyRegistrations,
) -> Result<LocalRuntimeExtensionStartup<TOrchestratorRouteTable, TExtensionRuntimeRegistrations>>
where
    TOrchestratorRouteTable: Default,
    FLoadRouteTable: FnOnce(&Path) -> Result<TOrchestratorRouteTable>,
    FDiscoverRegistrations: FnOnce(&Path) -> TExtensionRuntimeRegistrations,
    FBuildEmptyRegistrations: FnOnce(&Path) -> TExtensionRuntimeRegistrations,
{
    let extension_hooks = LocalRuntimeExtensionHooksDefaults {
        enabled: cli.extension_runtime_hooks,
        root: cli.extension_runtime_root.clone(),
    };
    let bootstrap = build_local_runtime_extension_bootstrap(
        cli,
        extension_hooks.enabled,
        extension_hooks.root.as_path(),
        load_route_table,
        discover_registrations,
        build_empty_registrations,
    )?;
    Ok(LocalRuntimeExtensionStartup {
        extension_hooks,
        bootstrap,
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
/// Public struct `RuntimeExtensionPipelineConfig` used across Tau components.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `RuntimeEventReporterPairRegistration` used across Tau components.
pub struct RuntimeEventReporterPairRegistration {
    pub first_registered: bool,
    pub second_registered: bool,
}

/// Public struct `RuntimeEventReporterRegistrationConfig` used across Tau components.
pub struct RuntimeEventReporterRegistrationConfig<FOpen, FReport, FEmit> {
    pub path: Option<PathBuf>,
    pub open_reporter: FOpen,
    pub report_event: FReport,
    pub emit_error: FEmit,
}

pub fn register_runtime_event_reporter_pair_if_configured<
    TFirstReporter,
    TSecondReporter,
    FFirstOpen,
    FFirstReport,
    FFirstEmit,
    EFirst,
    FSecondOpen,
    FSecondReport,
    FSecondEmit,
    ESecond,
>(
    agent: &mut Agent,
    first: RuntimeEventReporterRegistrationConfig<FFirstOpen, FFirstReport, FFirstEmit>,
    second: RuntimeEventReporterRegistrationConfig<FSecondOpen, FSecondReport, FSecondEmit>,
) -> Result<RuntimeEventReporterPairRegistration>
where
    TFirstReporter: Send + Sync + 'static,
    TSecondReporter: Send + Sync + 'static,
    FFirstOpen: FnOnce(PathBuf) -> Result<TFirstReporter>,
    FFirstReport:
        Fn(&TFirstReporter, &AgentEvent) -> std::result::Result<(), EFirst> + Send + Sync + 'static,
    FFirstEmit: Fn(&str) + Send + Sync + 'static,
    EFirst: std::fmt::Display,
    FSecondOpen: FnOnce(PathBuf) -> Result<TSecondReporter>,
    FSecondReport: Fn(&TSecondReporter, &AgentEvent) -> std::result::Result<(), ESecond>
        + Send
        + Sync
        + 'static,
    FSecondEmit: Fn(&str) + Send + Sync + 'static,
    ESecond: std::fmt::Display,
{
    let RuntimeEventReporterRegistrationConfig {
        path: first_path,
        open_reporter: first_open_reporter,
        report_event: first_report_event,
        emit_error: first_emit_error,
    } = first;
    let RuntimeEventReporterRegistrationConfig {
        path: second_path,
        open_reporter: second_open_reporter,
        report_event: second_report_event,
        emit_error: second_emit_error,
    } = second;

    let first_registered = register_runtime_event_reporter_if_configured(
        agent,
        first_path,
        first_open_reporter,
        first_report_event,
        first_emit_error,
    )?;
    let second_registered = register_runtime_event_reporter_if_configured(
        agent,
        second_path,
        second_open_reporter,
        second_report_event,
        second_emit_error,
    )?;
    Ok(RuntimeEventReporterPairRegistration {
        first_registered,
        second_registered,
    })
}

pub fn register_runtime_observability_if_configured<
    TFirstReporter,
    TSecondReporter,
    FFirstOpen,
    FFirstReport,
    FFirstEmit,
    EFirst,
    FSecondOpen,
    FSecondReport,
    FSecondEmit,
    ESecond,
    FRenderEvent,
    FEmitJson,
>(
    agent: &mut Agent,
    first: RuntimeEventReporterRegistrationConfig<FFirstOpen, FFirstReport, FFirstEmit>,
    second: RuntimeEventReporterRegistrationConfig<FSecondOpen, FSecondReport, FSecondEmit>,
    json_events_enabled: bool,
    render_event: FRenderEvent,
    emit_json: FEmitJson,
) -> Result<RuntimeEventReporterPairRegistration>
where
    TFirstReporter: Send + Sync + 'static,
    TSecondReporter: Send + Sync + 'static,
    FFirstOpen: FnOnce(PathBuf) -> Result<TFirstReporter>,
    FFirstReport:
        Fn(&TFirstReporter, &AgentEvent) -> std::result::Result<(), EFirst> + Send + Sync + 'static,
    FFirstEmit: Fn(&str) + Send + Sync + 'static,
    EFirst: std::fmt::Display,
    FSecondOpen: FnOnce(PathBuf) -> Result<TSecondReporter>,
    FSecondReport: Fn(&TSecondReporter, &AgentEvent) -> std::result::Result<(), ESecond>
        + Send
        + Sync
        + 'static,
    FSecondEmit: Fn(&str) + Send + Sync + 'static,
    ESecond: std::fmt::Display,
    FRenderEvent: Fn(&AgentEvent) -> Value + Send + Sync + 'static,
    FEmitJson: Fn(&Value) + Send + Sync + 'static,
{
    let registration = register_runtime_event_reporter_pair_if_configured(agent, first, second)?;
    register_runtime_json_event_subscriber(agent, json_events_enabled, render_event, emit_json);
    Ok(registration)
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
mod tests;
