//! Primary interactive/runtime execution loop for coding-agent.
//!
//! Coordinates turn processing, command dispatch, event emission, and transport
//! integration. Failure reasons are surfaced with stage-specific diagnostics.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use rustyline::{
    completion::{Completer, Pair},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    history::DefaultHistory,
    validate::Validator,
    Config as ReadlineConfig, Context as ReadlineContext, Editor, Helper,
};
use tau_agent_core::{Agent, AgentCostSnapshot, AgentError, CooperativeCancellationToken};
use tau_ai::StreamDeltaHandler;
use tau_cli::{Cli, CliOrchestratorMode};
use tau_core::current_unix_timestamp_ms;
use tau_extensions::{apply_extension_message_transforms, dispatch_extension_runtime_hook};
use tau_onboarding::startup_resolution::ensure_non_empty_text;
use tau_session::{SessionRuntime, SessionUsageSummary};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use crate::commands::{handle_command_with_session_import_mode, CommandAction, COMMAND_NAMES};
use crate::multi_agent_router::MultiAgentRouteTable;
use crate::orchestrator_bridge::{
    run_plan_first_prompt, run_plan_first_prompt_with_policy_context,
    run_plan_first_prompt_with_policy_context_and_routing, PlanFirstPromptPolicyRequest,
    PlanFirstPromptRequest, PlanFirstPromptRoutingRequest,
};
use crate::runtime_output::{persist_messages, print_assistant_messages};
use crate::runtime_prompt_template_bridge::RuntimePromptTemplateHotReloadBridgeHandle;
use crate::runtime_types::{CommandExecutionContext, ProfileDefaults, RenderOptions};

const EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;
const REPL_PROMPT: &str = "tau> ";
const REPL_CONTINUATION_PROMPT: &str = "...> ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptProcessType {
    Channel,
    Branch,
    Worker,
    Compactor,
    Cortex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptComplexity {
    Light,
    Standard,
    Heavy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeExtensionHooksConfig {
    pub(crate) enabled: bool,
    pub(crate) root: PathBuf,
}

#[derive(Clone, Copy)]
struct PromptRoutingRuntimeConfig<'a> {
    extension_runtime_hooks: &'a RuntimeExtensionHooksConfig,
    profile_defaults: Option<&'a ProfileDefaults>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeHookRunStatus {
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

impl RuntimeHookRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InteractiveRuntimeConfig<'a> {
    pub(crate) turn_timeout_ms: u64,
    pub(crate) render_options: RenderOptions,
    pub(crate) extension_runtime_hooks: &'a RuntimeExtensionHooksConfig,
    pub(crate) orchestrator_mode: CliOrchestratorMode,
    pub(crate) orchestrator_max_plan_steps: usize,
    pub(crate) orchestrator_max_delegated_steps: usize,
    pub(crate) orchestrator_max_executor_response_chars: usize,
    pub(crate) orchestrator_max_delegated_step_response_chars: usize,
    pub(crate) orchestrator_max_delegated_total_response_chars: usize,
    pub(crate) orchestrator_delegate_steps: bool,
    pub(crate) orchestrator_route_table: &'a MultiAgentRouteTable,
    pub(crate) orchestrator_route_trace_log: Option<&'a Path>,
    pub(crate) orchestrator_worker_skill_prompt: Option<&'a str>,
    pub(crate) command_context: CommandExecutionContext<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractiveLoopControl {
    Continue,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractiveIoMode {
    Tty,
    Stdin,
}

type InteractiveRunnerFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + 'a>>;

struct InteractiveRunnerContext<'a> {
    agent: &'a mut Agent,
    session_runtime: &'a mut Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'a>,
    prompt_template_bridge_handle: &'a mut RuntimePromptTemplateHotReloadBridgeHandle,
    cli: &'a Cli,
    skills_dir: &'a Path,
}

type InteractiveRunner =
    for<'a> fn(&'a mut InteractiveRunnerContext<'a>) -> InteractiveRunnerFuture<'a>;

fn resolve_interactive_io_mode(
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> InteractiveIoMode {
    if stdin_is_terminal && stdout_is_terminal {
        InteractiveIoMode::Tty
    } else {
        InteractiveIoMode::Stdin
    }
}

fn require_tty_streams(stdin_is_terminal: bool, stdout_is_terminal: bool) -> Result<()> {
    match resolve_interactive_io_mode(stdin_is_terminal, stdout_is_terminal) {
        InteractiveIoMode::Tty => Ok(()),
        InteractiveIoMode::Stdin => {
            bail!("interactive tty runtime requires terminal stdin/stdout")
        }
    }
}

#[derive(Debug, Default)]
struct ReplMultilineState {
    lines: Vec<String>,
}

impl ReplMultilineState {
    fn prompt(&self) -> &'static str {
        if self.lines.is_empty() {
            REPL_PROMPT
        } else {
            REPL_CONTINUATION_PROMPT
        }
    }

    fn push_line(&mut self, line: String) -> Option<String> {
        let continued = line.ends_with('\\') && !line.ends_with("\\\\");
        if continued {
            let mut trimmed = line;
            trimmed.pop();
            self.lines.push(trimmed);
            None
        } else {
            self.lines.push(line);
            let prompt = self.lines.join("\n");
            self.lines.clear();
            Some(prompt)
        }
    }

    fn has_pending(&self) -> bool {
        !self.lines.is_empty()
    }

    fn clear(&mut self) {
        self.lines.clear();
    }
}

#[derive(Debug)]
struct ReplCommandCompleter {
    commands: Vec<String>,
}

impl ReplCommandCompleter {
    fn new(commands: &[&str]) -> Self {
        Self {
            commands: commands
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
        }
    }

    fn complete_token(&self, token: &str) -> Vec<String> {
        if !token.starts_with('/') {
            return Vec::new();
        }
        self.commands
            .iter()
            .filter(|candidate| candidate.starts_with(token))
            .cloned()
            .collect()
    }
}

impl Helper for ReplCommandCompleter {}
impl Validator for ReplCommandCompleter {}
impl Highlighter for ReplCommandCompleter {}

impl Hinter for ReplCommandCompleter {
    type Hint = String;
}

impl Completer for ReplCommandCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &ReadlineContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let safe_pos = pos.min(line.len());
        let start = line[..safe_pos]
            .rfind(char::is_whitespace)
            .map_or(0, |index| index + 1);
        let token = &line[start..safe_pos];
        if !token.starts_with('/') {
            return Ok((safe_pos, Vec::new()));
        }

        let matches = self
            .complete_token(token)
            .into_iter()
            .map(|candidate| Pair {
                display: candidate.clone(),
                replacement: candidate,
            })
            .collect::<Vec<_>>();
        Ok((start, matches))
    }
}

pub(crate) async fn run_prompt_with_profile_routing(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
    profile_defaults: Option<&ProfileDefaults>,
) -> Result<()> {
    let status = run_prompt_with_runtime_hooks(
        agent,
        session_runtime,
        prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        render_options,
        PromptRoutingRuntimeConfig {
            extension_runtime_hooks,
            profile_defaults,
        },
    )
    .await?;
    report_prompt_status(status);
    Ok(())
}

pub(crate) async fn run_interactive(
    mut agent: Agent,
    mut session_runtime: Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
    prompt_template_bridge_handle: &mut RuntimePromptTemplateHotReloadBridgeHandle,
    cli: &Cli,
    skills_dir: &Path,
) -> Result<()> {
    let mode = resolve_interactive_io_mode(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    );
    let mut context = InteractiveRunnerContext {
        agent: &mut agent,
        session_runtime: &mut session_runtime,
        config,
        prompt_template_bridge_handle,
        cli,
        skills_dir,
    };
    run_interactive_with_runner(
        mode,
        &mut context,
        run_interactive_tty_runner,
        run_interactive_stdin_runner,
    )
    .await
}

fn run_interactive_stdin_runner<'a>(
    context: &'a mut InteractiveRunnerContext<'a>,
) -> InteractiveRunnerFuture<'a> {
    Box::pin(run_interactive_stdin(
        context.agent,
        context.session_runtime,
        context.config,
        context.prompt_template_bridge_handle,
        context.cli,
        context.skills_dir,
    ))
}

fn run_interactive_tty_runner<'a>(
    context: &'a mut InteractiveRunnerContext<'a>,
) -> InteractiveRunnerFuture<'a> {
    Box::pin(run_interactive_tty(
        context.agent,
        context.session_runtime,
        context.config,
        context.prompt_template_bridge_handle,
        context.cli,
        context.skills_dir,
    ))
}

async fn run_interactive_with_runner<'a>(
    mode: InteractiveIoMode,
    context: &'a mut InteractiveRunnerContext<'a>,
    tty_runner: InteractiveRunner,
    stdin_runner: InteractiveRunner,
) -> Result<()> {
    match mode {
        InteractiveIoMode::Tty => tty_runner(context).await,
        InteractiveIoMode::Stdin => stdin_runner(context).await,
    }
}

async fn run_interactive_stdin(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
    prompt_template_bridge_handle: &mut RuntimePromptTemplateHotReloadBridgeHandle,
    cli: &Cli,
    skills_dir: &Path,
) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        print!("{REPL_PROMPT}");
        std::io::stdout()
            .flush()
            .context("failed to flush stdout")?;

        let Some(line) = lines.next_line().await? else {
            break;
        };

        prompt_template_bridge_handle.apply_pending_update(agent, cli, skills_dir)?;
        match dispatch_interactive_turn(agent, session_runtime, config, &line).await? {
            InteractiveLoopControl::Continue => continue,
            InteractiveLoopControl::Exit => break,
        }
    }

    Ok(())
}

async fn run_interactive_tty(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
    prompt_template_bridge_handle: &mut RuntimePromptTemplateHotReloadBridgeHandle,
    cli: &Cli,
    skills_dir: &Path,
) -> Result<()> {
    require_tty_streams(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    )?;
    let history_path = default_repl_history_path(session_runtime.as_ref());
    let mut editor = build_repl_editor()?;
    load_repl_history(&mut editor, &history_path);
    let mut multiline = ReplMultilineState::default();

    loop {
        let prompt = multiline.prompt();
        let readline = tokio::task::block_in_place(|| editor.readline(prompt));
        let line = match readline {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                if multiline.has_pending() {
                    multiline.clear();
                    println!();
                    continue;
                }
                break;
            }
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(anyhow!("failed to read interactive input: {error}")),
        };

        let Some(input) = multiline.push_line(line) else {
            continue;
        };

        if input.trim().is_empty() {
            continue;
        }

        if matches!(editor.add_history_entry(input.as_str()), Ok(true)) {
            save_repl_history(&mut editor, &history_path);
        }

        prompt_template_bridge_handle.apply_pending_update(agent, cli, skills_dir)?;
        match dispatch_interactive_turn(agent, session_runtime, config, &input).await {
            Ok(InteractiveLoopControl::Continue) => continue,
            Ok(InteractiveLoopControl::Exit) => break,
            Err(error) => {
                report_interactive_turn_error(&error);
                continue;
            }
        }
    }

    save_repl_history(&mut editor, &history_path);
    Ok(())
}

async fn dispatch_interactive_turn(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
    input: &str,
) -> Result<InteractiveLoopControl> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(InteractiveLoopControl::Continue);
    }

    if trimmed.starts_with('/') {
        if handle_command_with_session_import_mode(
            trimmed,
            agent,
            session_runtime,
            config.command_context,
        )? == CommandAction::Exit
        {
            return Ok(InteractiveLoopControl::Exit);
        }
        return Ok(InteractiveLoopControl::Continue);
    }

    if config.orchestrator_mode == CliOrchestratorMode::PlanFirst {
        run_plan_first_prompt_with_runtime_hooks(
            agent,
            session_runtime,
            PlanFirstPromptRuntimeHooksConfig {
                prompt: input,
                turn_timeout_ms: config.turn_timeout_ms,
                render_options: config.render_options,
                orchestrator_max_plan_steps: config.orchestrator_max_plan_steps,
                orchestrator_max_delegated_steps: config.orchestrator_max_delegated_steps,
                orchestrator_max_executor_response_chars: config
                    .orchestrator_max_executor_response_chars,
                orchestrator_max_delegated_step_response_chars: config
                    .orchestrator_max_delegated_step_response_chars,
                orchestrator_max_delegated_total_response_chars: config
                    .orchestrator_max_delegated_total_response_chars,
                orchestrator_delegate_steps: config.orchestrator_delegate_steps,
                orchestrator_route_table: config.orchestrator_route_table,
                orchestrator_route_trace_log: config.orchestrator_route_trace_log,
                orchestrator_worker_skill_prompt: config.orchestrator_worker_skill_prompt,
                tool_policy_json: config.command_context.tool_policy_json,
                extension_runtime_hooks: config.extension_runtime_hooks,
            },
        )
        .await?;
        return Ok(InteractiveLoopControl::Continue);
    }

    let status = run_prompt_with_runtime_hooks(
        agent,
        session_runtime,
        input,
        config.turn_timeout_ms,
        tokio::signal::ctrl_c(),
        config.render_options,
        PromptRoutingRuntimeConfig {
            extension_runtime_hooks: config.extension_runtime_hooks,
            profile_defaults: Some(config.command_context.profile_defaults),
        },
    )
    .await?;
    report_prompt_status(status);
    Ok(InteractiveLoopControl::Continue)
}

fn build_repl_editor() -> Result<Editor<ReplCommandCompleter, DefaultHistory>> {
    let config = ReadlineConfig::builder().build();
    let mut editor = Editor::<ReplCommandCompleter, DefaultHistory>::with_config(config)
        .context("failed to initialize interactive editor")?;
    editor.set_helper(Some(ReplCommandCompleter::new(COMMAND_NAMES)));
    Ok(editor)
}

fn default_repl_history_path(session_runtime: Option<&SessionRuntime>) -> PathBuf {
    resolve_repl_history_path(session_runtime.map(|runtime| runtime.store.path()))
}

fn resolve_repl_history_path(session_path: Option<&Path>) -> PathBuf {
    session_path.map_or_else(
        || PathBuf::from(".tau/repl_history.txt"),
        |path| path.with_extension("history"),
    )
}

fn load_repl_history(editor: &mut Editor<ReplCommandCompleter, DefaultHistory>, path: &Path) {
    if let Err(error) = editor.load_history(path) {
        if !matches!(
            error,
            ReadlineError::Io(ref io_error) if io_error.kind() == std::io::ErrorKind::NotFound
        ) {
            eprintln!(
                "warning: failed to load REPL history from {}: {error}",
                path.display()
            );
        }
    }
}

fn save_repl_history(editor: &mut Editor<ReplCommandCompleter, DefaultHistory>, path: &Path) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "warning: failed to create REPL history directory {}: {error}",
                    parent.display()
                );
                return;
            }
        }
    }

    if let Err(error) = editor.save_history(path) {
        eprintln!(
            "warning: failed to persist REPL history to {}: {error}",
            path.display()
        );
    }
}

fn report_interactive_turn_error(error: &anyhow::Error) {
    eprintln!("interactive turn failed: {error:#}");
}

const CODING_TASK_KEYWORDS: &[&str] = &[
    "code",
    "coding",
    "implement",
    "implementation",
    "function",
    "method",
    "class",
    "module",
    "refactor",
    "debug",
    "fix",
    "bug",
    "test",
    "tests",
    "compile",
    "build",
    "rust",
    "typescript",
    "python",
];

const SUMMARIZATION_TASK_KEYWORDS: &[&str] = &[
    "summarize",
    "summary",
    "tldr",
    "recap",
    "overview",
    "condense",
];

const HEAVY_COMPLEXITY_KEYWORDS: &[&str] = &[
    "implement",
    "refactor",
    "debug",
    "fix",
    "migrate",
    "optimize",
    "design",
    "architecture",
    "integration",
    "benchmark",
    "security",
];

const STANDARD_COMPLEXITY_KEYWORDS: &[&str] = &[
    "explain",
    "analyze",
    "review",
    "compare",
    "plan",
    "document",
    "summarize",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptTaskType {
    Coding,
    Summarization,
}

fn contains_any_keyword(normalized_prompt: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| normalized_prompt.contains(keyword))
}

fn normalize_non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn infer_prompt_task_type(prompt: &str, complexity: PromptComplexity) -> Option<PromptTaskType> {
    let normalized_prompt = prompt.to_lowercase();
    let coding_match = contains_any_keyword(&normalized_prompt, CODING_TASK_KEYWORDS);
    let summarization_match = contains_any_keyword(&normalized_prompt, SUMMARIZATION_TASK_KEYWORDS);
    if coding_match {
        return Some(PromptTaskType::Coding);
    }
    if summarization_match {
        return Some(PromptTaskType::Summarization);
    }
    if complexity == PromptComplexity::Heavy {
        return Some(PromptTaskType::Coding);
    }
    None
}

fn routing_task_override(profile: &ProfileDefaults, task_type: PromptTaskType) -> Option<String> {
    let key = match task_type {
        PromptTaskType::Coding => "coding",
        PromptTaskType::Summarization => "summarization",
    };
    profile
        .routing
        .task_overrides
        .iter()
        .find_map(|(override_key, override_model)| {
            if override_key.trim().eq_ignore_ascii_case(key) {
                normalize_non_empty(override_model).map(ToOwned::to_owned)
            } else {
                None
            }
        })
}

fn process_type_routed_model(
    profile: &ProfileDefaults,
    process_type: PromptProcessType,
) -> Option<String> {
    let configured = match process_type {
        PromptProcessType::Channel => profile.routing.channel_model.as_deref(),
        PromptProcessType::Branch => profile.routing.branch_model.as_deref(),
        PromptProcessType::Worker => profile.routing.worker_model.as_deref(),
        PromptProcessType::Compactor => profile.routing.compactor_model.as_deref(),
        PromptProcessType::Cortex => profile.routing.cortex_model.as_deref(),
    };
    configured
        .and_then(normalize_non_empty)
        .map(ToOwned::to_owned)
}

pub(crate) fn classify_prompt_complexity(prompt: &str) -> PromptComplexity {
    let normalized_prompt = prompt.trim().to_lowercase();
    if normalized_prompt.is_empty() {
        return PromptComplexity::Light;
    }

    let char_count = normalized_prompt.chars().count();
    if contains_any_keyword(&normalized_prompt, HEAVY_COMPLEXITY_KEYWORDS) || char_count >= 320 {
        return PromptComplexity::Heavy;
    }
    if contains_any_keyword(&normalized_prompt, STANDARD_COMPLEXITY_KEYWORDS) || char_count >= 140 {
        return PromptComplexity::Standard;
    }
    PromptComplexity::Light
}

pub(crate) fn select_routed_dispatch_model(
    profile: &ProfileDefaults,
    process_type: PromptProcessType,
    prompt: &str,
) -> Option<String> {
    let complexity = classify_prompt_complexity(prompt);
    if let Some(task_type) = infer_prompt_task_type(prompt, complexity) {
        if let Some(task_override) = routing_task_override(profile, task_type) {
            return Some(task_override);
        }
    }
    process_type_routed_model(profile, process_type)
}

async fn run_prompt_with_runtime_hooks<F>(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    cancellation_signal: F,
    render_options: RenderOptions,
    routing_config: PromptRoutingRuntimeConfig<'_>,
) -> Result<PromptRunStatus>
where
    F: Future,
{
    let PromptRoutingRuntimeConfig {
        extension_runtime_hooks,
        profile_defaults,
    } = routing_config;
    let effective_prompt = apply_runtime_message_transform(extension_runtime_hooks, prompt);
    dispatch_runtime_hook(
        extension_runtime_hooks,
        "run-start",
        build_runtime_hook_payload("run-start", &effective_prompt, turn_timeout_ms, None, None),
    );

    let model_override = profile_defaults.and_then(|profile| {
        select_routed_dispatch_model(profile, PromptProcessType::Channel, &effective_prompt)
    });
    let previous_model = model_override
        .as_ref()
        .map(|selected_model| agent.swap_dispatch_model(selected_model.clone()));

    let result = run_prompt_with_cancellation(
        agent,
        session_runtime,
        &effective_prompt,
        turn_timeout_ms,
        cancellation_signal,
        render_options,
    )
    .await;
    if let Some(previous_model) = previous_model {
        agent.restore_dispatch_model(previous_model);
    }

    match result {
        Ok(status) => {
            let run_status = match status {
                PromptRunStatus::Completed => RuntimeHookRunStatus::Completed,
                PromptRunStatus::Cancelled => RuntimeHookRunStatus::Cancelled,
                PromptRunStatus::TimedOut => RuntimeHookRunStatus::TimedOut,
            };
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(run_status),
                    None,
                ),
            );
            Ok(status)
        }
        Err(error) => {
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(RuntimeHookRunStatus::Failed),
                    Some(error.to_string()),
                ),
            );
            Err(error)
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PlanFirstPromptRuntimeHooksConfig<'a> {
    pub(crate) prompt: &'a str,
    pub(crate) turn_timeout_ms: u64,
    pub(crate) render_options: RenderOptions,
    pub(crate) orchestrator_max_plan_steps: usize,
    pub(crate) orchestrator_max_delegated_steps: usize,
    pub(crate) orchestrator_max_executor_response_chars: usize,
    pub(crate) orchestrator_max_delegated_step_response_chars: usize,
    pub(crate) orchestrator_max_delegated_total_response_chars: usize,
    pub(crate) orchestrator_delegate_steps: bool,
    pub(crate) orchestrator_route_table: &'a MultiAgentRouteTable,
    pub(crate) orchestrator_route_trace_log: Option<&'a Path>,
    pub(crate) orchestrator_worker_skill_prompt: Option<&'a str>,
    pub(crate) tool_policy_json: &'a serde_json::Value,
    pub(crate) extension_runtime_hooks: &'a RuntimeExtensionHooksConfig,
}

pub(crate) async fn run_plan_first_prompt_with_runtime_hooks(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    config: PlanFirstPromptRuntimeHooksConfig<'_>,
) -> Result<()> {
    let PlanFirstPromptRuntimeHooksConfig {
        prompt,
        turn_timeout_ms,
        render_options,
        orchestrator_max_plan_steps,
        orchestrator_max_delegated_steps,
        orchestrator_max_executor_response_chars,
        orchestrator_max_delegated_step_response_chars,
        orchestrator_max_delegated_total_response_chars,
        orchestrator_delegate_steps,
        orchestrator_route_table,
        orchestrator_route_trace_log,
        orchestrator_worker_skill_prompt,
        tool_policy_json,
        extension_runtime_hooks,
    } = config;

    let effective_prompt = apply_runtime_message_transform(extension_runtime_hooks, prompt);
    dispatch_runtime_hook(
        extension_runtime_hooks,
        "run-start",
        build_runtime_hook_payload("run-start", &effective_prompt, turn_timeout_ms, None, None),
    );

    let policy_context = if orchestrator_delegate_steps {
        Some(
            render_orchestrator_policy_inheritance_context(
                tool_policy_json,
                extension_runtime_hooks,
            )
            .context(
                "plan-first orchestrator failed: delegated policy inheritance context build failed",
            )?,
        )
    } else {
        None
    };

    let uses_default_router = orchestrator_route_table == &MultiAgentRouteTable::default();
    let result = if uses_default_router && orchestrator_route_trace_log.is_none() {
        if let Some(policy_context) = policy_context.as_deref() {
            run_plan_first_prompt_with_policy_context(
                agent,
                session_runtime,
                PlanFirstPromptPolicyRequest {
                    user_prompt: &effective_prompt,
                    turn_timeout_ms,
                    render_options,
                    max_plan_steps: orchestrator_max_plan_steps,
                    max_delegated_steps: orchestrator_max_delegated_steps,
                    max_executor_response_chars: orchestrator_max_executor_response_chars,
                    max_delegated_step_response_chars:
                        orchestrator_max_delegated_step_response_chars,
                    max_delegated_total_response_chars:
                        orchestrator_max_delegated_total_response_chars,
                    delegate_steps: orchestrator_delegate_steps,
                    delegated_policy_context: Some(policy_context),
                    delegated_skill_context: orchestrator_worker_skill_prompt,
                },
            )
            .await
        } else {
            run_plan_first_prompt(
                agent,
                session_runtime,
                PlanFirstPromptRequest {
                    user_prompt: &effective_prompt,
                    turn_timeout_ms,
                    render_options,
                    max_plan_steps: orchestrator_max_plan_steps,
                    max_delegated_steps: orchestrator_max_delegated_steps,
                    max_executor_response_chars: orchestrator_max_executor_response_chars,
                    max_delegated_step_response_chars:
                        orchestrator_max_delegated_step_response_chars,
                    max_delegated_total_response_chars:
                        orchestrator_max_delegated_total_response_chars,
                    delegate_steps: orchestrator_delegate_steps,
                    delegated_skill_context: orchestrator_worker_skill_prompt,
                },
            )
            .await
        }
    } else {
        run_plan_first_prompt_with_policy_context_and_routing(
            agent,
            session_runtime,
            PlanFirstPromptRoutingRequest {
                user_prompt: &effective_prompt,
                turn_timeout_ms,
                render_options,
                max_plan_steps: orchestrator_max_plan_steps,
                max_delegated_steps: orchestrator_max_delegated_steps,
                max_executor_response_chars: orchestrator_max_executor_response_chars,
                max_delegated_step_response_chars: orchestrator_max_delegated_step_response_chars,
                max_delegated_total_response_chars: orchestrator_max_delegated_total_response_chars,
                delegate_steps: orchestrator_delegate_steps,
                delegated_policy_context: policy_context.as_deref(),
                delegated_skill_context: orchestrator_worker_skill_prompt,
                route_table: orchestrator_route_table,
                route_trace_log_path: orchestrator_route_trace_log,
            },
        )
        .await
    };

    match result {
        Ok(()) => {
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(RuntimeHookRunStatus::Completed),
                    None,
                ),
            );
            Ok(())
        }
        Err(error) => {
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(RuntimeHookRunStatus::Failed),
                    Some(error.to_string()),
                ),
            );
            Err(error)
        }
    }
}

fn apply_runtime_message_transform(config: &RuntimeExtensionHooksConfig, prompt: &str) -> String {
    if !config.enabled {
        return prompt.to_string();
    }
    let transform = apply_extension_message_transforms(&config.root, prompt);
    for diagnostic in transform.diagnostics {
        eprintln!("{diagnostic}");
    }
    transform.prompt
}

fn render_orchestrator_policy_inheritance_context(
    tool_policy_json: &serde_json::Value,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<String> {
    let object = tool_policy_json
        .as_object()
        .ok_or_else(|| anyhow!("tool policy JSON must be an object"))?;
    let schema_version = object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("tool policy JSON missing numeric field 'schema_version'"))?;
    let preset = object
        .get("preset")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'preset'"))?;
    let bash_profile = object
        .get("bash_profile")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'bash_profile'"))?;
    let os_sandbox_mode = object
        .get("os_sandbox_mode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'os_sandbox_mode'"))?;
    let os_sandbox_policy_mode = object
        .get("os_sandbox_policy_mode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'os_sandbox_policy_mode'"))?;
    let os_sandbox_docker_enabled = object
        .get("os_sandbox_docker_enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let bash_dry_run = object
        .get("bash_dry_run")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow!("tool policy JSON missing boolean field 'bash_dry_run'"))?;
    let enforce_regular_files = object
        .get("enforce_regular_files")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow!("tool policy JSON missing boolean field 'enforce_regular_files'"))?;
    let max_command_length = object
        .get("max_command_length")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("tool policy JSON missing numeric field 'max_command_length'"))?;
    let max_command_output_bytes = object
        .get("max_command_output_bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            anyhow!("tool policy JSON missing numeric field 'max_command_output_bytes'")
        })?;
    let http_timeout_ms = object
        .get("http_timeout_ms")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("tool policy JSON missing numeric field 'http_timeout_ms'"))?;
    let http_max_response_bytes = object
        .get("http_max_response_bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            anyhow!("tool policy JSON missing numeric field 'http_max_response_bytes'")
        })?;
    let http_max_redirects = object
        .get("http_max_redirects")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("tool policy JSON missing numeric field 'http_max_redirects'"))?;
    let http_allow_http = object
        .get("http_allow_http")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow!("tool policy JSON missing boolean field 'http_allow_http'"))?;
    let http_allow_private_network = object
        .get("http_allow_private_network")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            anyhow!("tool policy JSON missing boolean field 'http_allow_private_network'")
        })?;
    let allowed_roots = object
        .get("allowed_roots")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("tool policy JSON missing array field 'allowed_roots'"))?;
    let allowed_commands = object
        .get("allowed_commands")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("tool policy JSON missing array field 'allowed_commands'"))?;

    Ok(format!(
        "schema_version={schema_version};preset={preset};bash_profile={bash_profile};os_sandbox_mode={os_sandbox_mode};os_sandbox_policy_mode={os_sandbox_policy_mode};os_sandbox_docker_enabled={os_sandbox_docker_enabled};bash_dry_run={bash_dry_run};enforce_regular_files={enforce_regular_files};max_command_length={max_command_length};max_command_output_bytes={max_command_output_bytes};http_timeout_ms={http_timeout_ms};http_max_response_bytes={http_max_response_bytes};http_max_redirects={http_max_redirects};http_allow_http={http_allow_http};http_allow_private_network={http_allow_private_network};allowed_roots={};allowed_commands={};extension_runtime_hooks_enabled={}",
        allowed_roots.len(),
        allowed_commands.len(),
        extension_runtime_hooks.enabled,
    ))
}

fn dispatch_runtime_hook(
    config: &RuntimeExtensionHooksConfig,
    hook: &str,
    payload: serde_json::Value,
) {
    if !config.enabled {
        return;
    }
    let summary = dispatch_extension_runtime_hook(&config.root, hook, &payload);
    for diagnostic in summary.diagnostics {
        eprintln!("{diagnostic}");
    }
}

fn build_runtime_hook_payload(
    hook: &str,
    prompt: &str,
    turn_timeout_ms: u64,
    status: Option<RuntimeHookRunStatus>,
    error: Option<String>,
) -> serde_json::Value {
    let mut data = serde_json::Map::new();
    data.insert(
        "prompt".to_string(),
        serde_json::Value::String(prompt.to_string()),
    );
    data.insert(
        "turn_timeout_ms".to_string(),
        serde_json::Value::Number(turn_timeout_ms.into()),
    );
    if let Some(status) = status {
        data.insert(
            "status".to_string(),
            serde_json::Value::String(status.as_str().to_string()),
        );
    }
    if let Some(error) = error {
        data.insert("error".to_string(), serde_json::Value::String(error));
    }

    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema_version".to_string(),
        serde_json::Value::Number(EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION.into()),
    );
    payload.insert(
        "hook".to_string(),
        serde_json::Value::String(hook.to_string()),
    );
    payload.insert(
        "emitted_at_ms".to_string(),
        serde_json::Value::Number(current_unix_timestamp_ms().into()),
    );
    payload.insert("data".to_string(), serde_json::Value::Object(data.clone()));
    for (key, value) in data {
        payload.insert(key, value);
    }
    serde_json::Value::Object(payload)
}

pub(crate) async fn run_prompt_with_cancellation<F>(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    cancellation_signal: F,
    render_options: RenderOptions,
) -> Result<PromptRunStatus>
where
    F: Future,
{
    let checkpoint = agent.messages().to_vec();
    let pre_prompt_cost = agent.cost_snapshot();
    let cancellation_token = CooperativeCancellationToken::new();
    agent.set_cancellation_token(Some(cancellation_token.clone()));
    let streamed_output = Arc::new(AtomicBool::new(false));
    let (stream_delta_handler, mut stream_task) = if render_options.stream_output {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let streamed_output = streamed_output.clone();
        let stream_delay_ms = render_options.stream_delay_ms;
        let task = tokio::spawn(async move {
            while let Some(delta) = rx.recv().await {
                if delta.is_empty() {
                    continue;
                }
                streamed_output.store(true, Ordering::Relaxed);
                print!("{delta}");
                let _ = std::io::stdout().flush();
                if stream_delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(stream_delay_ms)).await;
                }
            }
        });
        (
            Some(Arc::new(move |delta: String| {
                let _ = tx.send(delta);
            }) as StreamDeltaHandler),
            Some(task),
        )
    } else {
        (None, None)
    };
    tokio::pin!(cancellation_signal);

    enum PromptOutcome<T> {
        Result(T),
        Cancelled,
        TimedOut,
    }

    let prompt_result = {
        let mut prompt_future =
            std::pin::pin!(agent.prompt_with_stream(prompt, stream_delta_handler.clone()));
        if turn_timeout_ms == 0 {
            tokio::select! {
                result = &mut prompt_future => PromptOutcome::Result(result),
                _ = &mut cancellation_signal => {
                    cancellation_token.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut prompt_future).await;
                    PromptOutcome::Cancelled
                },
            }
        } else {
            let timeout = tokio::time::sleep(Duration::from_millis(turn_timeout_ms));
            tokio::pin!(timeout);
            tokio::select! {
                result = &mut prompt_future => PromptOutcome::Result(result),
                _ = &mut cancellation_signal => {
                    cancellation_token.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut prompt_future).await;
                    PromptOutcome::Cancelled
                },
                _ = &mut timeout => {
                    cancellation_token.cancel();
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut prompt_future).await;
                    PromptOutcome::TimedOut
                },
            }
        }
    };
    agent.set_cancellation_token(None);

    drop(stream_delta_handler);
    if let Some(task) = stream_task.take() {
        let _ = tokio::time::timeout(Duration::from_secs(1), task).await;
    }

    let prompt_result = match prompt_result {
        PromptOutcome::Result(result) => result,
        PromptOutcome::Cancelled => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::Cancelled);
        }
        PromptOutcome::TimedOut => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::TimedOut);
        }
    };

    let new_messages = match prompt_result {
        Ok(messages) => messages,
        Err(AgentError::Cancelled) => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::Cancelled);
        }
        Err(error) => return Err(error.into()),
    };
    let post_prompt_cost = agent.cost_snapshot();
    persist_session_usage_delta(session_runtime, &pre_prompt_cost, &post_prompt_cost)?;
    persist_messages(session_runtime, &new_messages)?;
    print_assistant_messages(
        &new_messages,
        render_options,
        streamed_output.load(Ordering::Relaxed),
    );
    Ok(PromptRunStatus::Completed)
}

fn persist_session_usage_delta(
    session_runtime: &mut Option<SessionRuntime>,
    pre_prompt_cost: &AgentCostSnapshot,
    post_prompt_cost: &AgentCostSnapshot,
) -> Result<()> {
    let Some(runtime) = session_runtime.as_mut() else {
        return Ok(());
    };
    let delta = SessionUsageSummary {
        input_tokens: post_prompt_cost
            .input_tokens
            .saturating_sub(pre_prompt_cost.input_tokens),
        output_tokens: post_prompt_cost
            .output_tokens
            .saturating_sub(pre_prompt_cost.output_tokens),
        total_tokens: post_prompt_cost
            .total_tokens
            .saturating_sub(pre_prompt_cost.total_tokens),
        estimated_cost_usd: (post_prompt_cost.estimated_cost_usd
            - pre_prompt_cost.estimated_cost_usd)
            .max(0.0),
    };
    runtime.store.record_usage_delta(delta)
}

pub(crate) fn resolve_prompt_input(cli: &Cli) -> Result<Option<String>> {
    if let Some(prompt) = &cli.prompt {
        return Ok(Some(prompt.clone()));
    }

    if let Some(path) = cli.prompt_template_file.as_ref() {
        let template = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read prompt template file {}", path.display()))?;
        let template =
            ensure_non_empty_text(template, format!("prompt template file {}", path.display()))?;
        let vars = parse_prompt_template_vars(&cli.prompt_template_var)?;
        let rendered = render_prompt_template(&template, &vars)?;
        return Ok(Some(ensure_non_empty_text(
            rendered,
            format!("rendered prompt template {}", path.display()),
        )?));
    }

    let Some(path) = cli.prompt_file.as_ref() else {
        return Ok(None);
    };

    if path == std::path::Path::new("-") {
        let mut prompt = String::new();
        std::io::stdin()
            .read_to_string(&mut prompt)
            .context("failed to read prompt from stdin")?;
        return Ok(Some(ensure_non_empty_text(
            prompt,
            "stdin prompt".to_string(),
        )?));
    }

    let prompt = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read prompt file {}", path.display()))?;

    Ok(Some(ensure_non_empty_text(
        prompt,
        format!("prompt file {}", path.display()),
    )?))
}

fn parse_prompt_template_vars(raw_specs: &[String]) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();
    for raw_spec in raw_specs {
        let spec = raw_spec.trim();
        let (raw_key, value) = spec.split_once('=').ok_or_else(|| {
            anyhow!(
                "invalid --prompt-template-var '{}', expected key=value",
                raw_spec
            )
        })?;
        let key = raw_key.trim();
        if key.is_empty() {
            bail!(
                "invalid --prompt-template-var '{}', key must be non-empty",
                raw_spec
            );
        }
        if vars.contains_key(key) {
            bail!("duplicate --prompt-template-var key '{}'", key);
        }
        vars.insert(key.to_string(), value.to_string());
    }
    Ok(vars)
}

fn render_prompt_template(template: &str, vars: &BTreeMap<String, String>) -> Result<String> {
    let mut rendered = String::new();
    let mut used_keys = BTreeSet::new();
    let mut cursor = 0_usize;

    while let Some(start_rel) = template[cursor..].find("{{") {
        let start = cursor + start_rel;
        rendered.push_str(&template[cursor..start]);
        let placeholder_start = start + 2;
        let end_rel = template[placeholder_start..]
            .find("}}")
            .ok_or_else(|| anyhow!("prompt template contains unterminated placeholder"))?;
        let end = placeholder_start + end_rel;
        let key = template[placeholder_start..end].trim();
        if key.is_empty() {
            bail!("prompt template contains empty placeholder");
        }
        let value = vars.get(key).ok_or_else(|| {
            anyhow!(
                "prompt template placeholder '{}' is missing a --prompt-template-var value",
                key
            )
        })?;
        rendered.push_str(value);
        used_keys.insert(key.to_string());
        cursor = end + 2;
    }
    rendered.push_str(&template[cursor..]);

    let unused = vars
        .keys()
        .filter(|key| !used_keys.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    if !unused.is_empty() {
        bail!("unused --prompt-template-var keys: {}", unused.join(", "));
    }

    Ok(rendered)
}

fn report_prompt_status(status: PromptRunStatus) {
    if status == PromptRunStatus::Cancelled {
        println!("\nrequest cancelled\n");
    } else if status == PromptRunStatus::TimedOut {
        println!("\nrequest timed out\n");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_runtime_message_transform, build_runtime_hook_payload, classify_prompt_complexity,
        render_orchestrator_policy_inheritance_context, require_tty_streams,
        resolve_interactive_io_mode, resolve_repl_history_path, run_interactive_tty,
        run_interactive_with_runner, run_prompt_with_profile_routing, select_routed_dispatch_model,
        InteractiveIoMode, InteractiveRunnerContext, InteractiveRunnerFuture, PromptComplexity,
        PromptProcessType, ReplCommandCompleter, ReplMultilineState, RuntimeExtensionHooksConfig,
        RuntimeHookRunStatus,
    };
    use crate::runtime_prompt_template_bridge::start_runtime_prompt_template_hot_reload_bridge;
    use crate::runtime_types::{
        AuthCommandConfig, DoctorCommandConfig, DoctorMultiChannelReadinessConfig,
        ProfileAuthDefaults, ProfileDefaults, ProfileMcpDefaults, ProfilePolicyDefaults,
        ProfileRoutingDefaults, ProfileSessionDefaults, RenderOptions, SkillsSyncCommandConfig,
    };
    use crate::tests::test_cli;
    use crate::ModelCatalog;
    use async_trait::async_trait;
    use std::collections::{BTreeMap, VecDeque};
    use std::io::IsTerminal;
    use std::path::{Path, PathBuf};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use tau_agent_core::{Agent, AgentConfig};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, Provider, TauAiError};
    use tau_provider::CredentialStoreEncryptionMode;
    use tau_provider::ProviderAuthMethod;
    use tau_session::SessionImportMode;
    use tempfile::tempdir;
    use tokio::sync::Mutex as AsyncMutex;

    struct RecordingSequenceClient {
        outcomes: AsyncMutex<VecDeque<Result<ChatResponse, TauAiError>>>,
        recorded_models: Arc<AsyncMutex<Vec<String>>>,
    }

    #[async_trait]
    impl LlmClient for RecordingSequenceClient {
        async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.recorded_models.lock().await.push(request.model);
            let mut outcomes = self.outcomes.lock().await;
            outcomes.pop_front().unwrap_or_else(|| {
                Err(TauAiError::InvalidResponse(
                    "mock outcome queue is empty".to_string(),
                ))
            })
        }
    }

    fn test_render_options() -> RenderOptions {
        RenderOptions {
            stream_output: false,
            stream_delay_ms: 0,
        }
    }

    fn disabled_runtime_hooks() -> RuntimeExtensionHooksConfig {
        RuntimeExtensionHooksConfig {
            enabled: false,
            root: std::path::PathBuf::from(".tau/extensions"),
        }
    }

    fn sample_chat_response(text: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant_text(text),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }
    }

    fn sample_profile_defaults() -> ProfileDefaults {
        ProfileDefaults {
            model: "openai/gpt-4o-mini".to_string(),
            fallback_models: Vec::new(),
            session: ProfileSessionDefaults {
                enabled: true,
                path: Some(".tau/sessions/default.sqlite".to_string()),
                import_mode: "merge".to_string(),
            },
            policy: ProfilePolicyDefaults {
                tool_policy_preset: "balanced".to_string(),
                bash_profile: "balanced".to_string(),
                bash_dry_run: false,
                os_sandbox_mode: "off".to_string(),
                enforce_regular_files: true,
                bash_timeout_ms: 120_000,
                max_command_length: 8_192,
                max_tool_output_bytes: 262_144,
                max_file_read_bytes: 262_144,
                max_file_write_bytes: 262_144,
                allow_command_newlines: true,
                runtime_heartbeat_enabled: true,
                runtime_heartbeat_interval_ms: 5_000,
                runtime_heartbeat_state_path: ".tau/runtime-heartbeat/state.json".to_string(),
                runtime_self_repair_enabled: true,
                runtime_self_repair_timeout_ms: 300_000,
                runtime_self_repair_max_retries: 2,
                runtime_self_repair_tool_builds_dir: ".tau/tool-builds".to_string(),
                runtime_self_repair_orphan_max_age_seconds: 3_600,
                context_compaction_warn_threshold_percent: 80,
                context_compaction_aggressive_threshold_percent: 85,
                context_compaction_emergency_threshold_percent: 95,
                context_compaction_warn_retain_percent: 70,
                context_compaction_aggressive_retain_percent: 50,
                context_compaction_emergency_retain_percent: 50,
            },
            mcp: ProfileMcpDefaults::default(),
            auth: ProfileAuthDefaults::default(),
            routing: ProfileRoutingDefaults::default(),
        }
    }

    static TTY_RUNNER_CALLS: AtomicUsize = AtomicUsize::new(0);
    static STDIN_RUNNER_CALLS: AtomicUsize = AtomicUsize::new(0);
    static INTERACTIVE_RUNNER_TEST_LOCK: Mutex<()> = Mutex::new(());

    struct InteractiveRunnerHarness {
        tool_policy_json: serde_json::Value,
        profile_defaults: ProfileDefaults,
        skills_command_config: SkillsSyncCommandConfig,
        auth_command_config: AuthCommandConfig,
        model_catalog: ModelCatalog,
        route_table: tau_orchestrator::multi_agent_router::MultiAgentRouteTable,
    }

    impl InteractiveRunnerHarness {
        fn new() -> Self {
            let skills_dir = PathBuf::from(".tau/skills");
            let skills_lock_path = PathBuf::from(".tau/skills.lock.json");
            let doctor_config = DoctorCommandConfig {
                model: "openai/gpt-4o-mini".to_string(),
                provider_keys: Vec::new(),
                release_channel_path: PathBuf::from(".tau/release-channel.toml"),
                release_lookup_cache_path: PathBuf::from(".tau/release-channel-cache.json"),
                release_lookup_cache_ttl_ms: 300_000,
                browser_automation_playwright_cli: "playwright".to_string(),
                session_enabled: true,
                session_path: PathBuf::from(".tau/sessions/default.sqlite"),
                skills_dir: skills_dir.clone(),
                skills_lock_path: skills_lock_path.clone(),
                trust_root_path: None,
                multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
            };

            Self {
                tool_policy_json: serde_json::json!({
                    "schema_version": 1,
                    "preset": "balanced"
                }),
                profile_defaults: sample_profile_defaults(),
                skills_command_config: SkillsSyncCommandConfig {
                    skills_dir,
                    default_lock_path: skills_lock_path,
                    default_trust_root_path: None,
                    doctor_config,
                },
                auth_command_config: AuthCommandConfig {
                    credential_store: PathBuf::from(".tau/credentials.json"),
                    credential_store_key: None,
                    credential_store_encryption: CredentialStoreEncryptionMode::None,
                    api_key: None,
                    openai_api_key: None,
                    anthropic_api_key: None,
                    google_api_key: None,
                    openai_auth_mode: ProviderAuthMethod::ApiKey,
                    anthropic_auth_mode: ProviderAuthMethod::ApiKey,
                    google_auth_mode: ProviderAuthMethod::ApiKey,
                    provider_subscription_strict: false,
                    openai_codex_backend: false,
                    openai_codex_cli: "codex".to_string(),
                    anthropic_claude_backend: false,
                    anthropic_claude_cli: "claude".to_string(),
                    google_gemini_backend: false,
                    google_gemini_cli: "gemini".to_string(),
                    google_gcloud_cli: "gcloud".to_string(),
                },
                model_catalog: ModelCatalog::built_in(),
                route_table: tau_orchestrator::multi_agent_router::MultiAgentRouteTable::default(),
            }
        }

        fn runtime_config<'a>(
            &'a self,
            extension_runtime_hooks: &'a RuntimeExtensionHooksConfig,
        ) -> super::InteractiveRuntimeConfig<'a> {
            super::InteractiveRuntimeConfig {
                turn_timeout_ms: 0,
                render_options: test_render_options(),
                extension_runtime_hooks,
                orchestrator_mode: tau_cli::CliOrchestratorMode::Off,
                orchestrator_max_plan_steps: 4,
                orchestrator_max_delegated_steps: 4,
                orchestrator_max_executor_response_chars: 4_096,
                orchestrator_max_delegated_step_response_chars: 2_048,
                orchestrator_max_delegated_total_response_chars: 8_192,
                orchestrator_delegate_steps: true,
                orchestrator_route_table: &self.route_table,
                orchestrator_route_trace_log: None,
                orchestrator_worker_skill_prompt: None,
                command_context: crate::runtime_types::CommandExecutionContext {
                    tool_policy_json: &self.tool_policy_json,
                    session_import_mode: SessionImportMode::Merge,
                    profile_defaults: &self.profile_defaults,
                    skills_command_config: &self.skills_command_config,
                    auth_command_config: &self.auth_command_config,
                    model_catalog: &self.model_catalog,
                    extension_commands: &[],
                },
            }
        }
    }

    fn apply_workspace_paths(cli: &mut tau_cli::Cli, workspace: &Path) {
        let tau_root = workspace.join(".tau");
        cli.session = tau_root.join("sessions/default.sqlite");
        cli.credential_store = tau_root.join("credentials.json");
        cli.skills_dir = tau_root.join("skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
    }

    fn fake_tty_runner<'a>(
        _context: &'a mut InteractiveRunnerContext<'a>,
    ) -> InteractiveRunnerFuture<'a> {
        TTY_RUNNER_CALLS.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(()) })
    }

    fn fake_stdin_runner<'a>(
        _context: &'a mut InteractiveRunnerContext<'a>,
    ) -> InteractiveRunnerFuture<'a> {
        STDIN_RUNNER_CALLS.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(()) })
    }

    fn routed_profile_defaults() -> ProfileDefaults {
        let mut profile = sample_profile_defaults();
        profile.routing = ProfileRoutingDefaults {
            channel_model: Some("openai/gpt-4.1-mini".to_string()),
            branch_model: None,
            worker_model: None,
            compactor_model: None,
            cortex_model: None,
            task_overrides: BTreeMap::from([
                ("coding".to_string(), "openai/o3-mini".to_string()),
                (
                    "summarization".to_string(),
                    "openai/gpt-4o-mini".to_string(),
                ),
            ]),
        };
        profile
    }

    #[test]
    fn regression_2548_resolve_interactive_io_mode_requires_both_terminal_streams() {
        assert_eq!(
            resolve_interactive_io_mode(true, true),
            InteractiveIoMode::Tty
        );
        assert_eq!(
            resolve_interactive_io_mode(true, false),
            InteractiveIoMode::Stdin
        );
        assert_eq!(
            resolve_interactive_io_mode(false, true),
            InteractiveIoMode::Stdin
        );
        assert_eq!(
            resolve_interactive_io_mode(false, false),
            InteractiveIoMode::Stdin
        );
    }

    #[test]
    fn regression_2548_require_tty_streams_fails_closed_when_any_stream_is_not_terminal() {
        assert!(
            require_tty_streams(true, true).is_ok(),
            "both terminal streams should pass"
        );

        let stdout_non_terminal =
            require_tty_streams(true, false).expect_err("stdout non-terminal should fail");
        assert!(stdout_non_terminal
            .to_string()
            .contains("interactive tty runtime requires terminal stdin/stdout"));

        let stdin_non_terminal =
            require_tty_streams(false, true).expect_err("stdin non-terminal should fail");
        assert!(stdin_non_terminal
            .to_string()
            .contains("interactive tty runtime requires terminal stdin/stdout"));

        let both_non_terminal =
            require_tty_streams(false, false).expect_err("non-terminal streams should fail");
        assert!(both_non_terminal
            .to_string()
            .contains("interactive tty runtime requires terminal stdin/stdout"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_2548_run_interactive_with_runner_dispatches_tty_mode() {
        let _guard = INTERACTIVE_RUNNER_TEST_LOCK
            .lock()
            .expect("interactive runner test lock");
        TTY_RUNNER_CALLS.store(0, Ordering::Relaxed);
        STDIN_RUNNER_CALLS.store(0, Ordering::Relaxed);

        let hooks = disabled_runtime_hooks();
        let harness = InteractiveRunnerHarness::new();
        let config = harness.runtime_config(&hooks);
        let mut cli = test_cli();
        let temp = tempdir().expect("tempdir");
        apply_workspace_paths(&mut cli, temp.path());
        let mut bridge = start_runtime_prompt_template_hot_reload_bridge(&cli, "initial prompt")
            .expect("start bridge");
        let mut agent = Agent::new(
            Arc::new(RecordingSequenceClient {
                outcomes: AsyncMutex::new(VecDeque::new()),
                recorded_models: Arc::new(AsyncMutex::new(Vec::new())),
            }),
            AgentConfig::default(),
        );
        let mut session_runtime = None;
        let mut context = InteractiveRunnerContext {
            agent: &mut agent,
            session_runtime: &mut session_runtime,
            config,
            prompt_template_bridge_handle: &mut bridge,
            cli: &cli,
            skills_dir: &cli.skills_dir,
        };

        run_interactive_with_runner(
            InteractiveIoMode::Tty,
            &mut context,
            fake_tty_runner,
            fake_stdin_runner,
        )
        .await
        .expect("mode dispatch");
        assert_eq!(TTY_RUNNER_CALLS.load(Ordering::Relaxed), 1);
        assert_eq!(STDIN_RUNNER_CALLS.load(Ordering::Relaxed), 0);
        bridge.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_2548_run_interactive_with_runner_dispatches_stdin_mode() {
        let _guard = INTERACTIVE_RUNNER_TEST_LOCK
            .lock()
            .expect("interactive runner test lock");
        TTY_RUNNER_CALLS.store(0, Ordering::Relaxed);
        STDIN_RUNNER_CALLS.store(0, Ordering::Relaxed);

        let hooks = disabled_runtime_hooks();
        let harness = InteractiveRunnerHarness::new();
        let config = harness.runtime_config(&hooks);
        let mut cli = test_cli();
        let temp = tempdir().expect("tempdir");
        apply_workspace_paths(&mut cli, temp.path());
        let mut bridge = start_runtime_prompt_template_hot_reload_bridge(&cli, "initial prompt")
            .expect("start bridge");
        let mut agent = Agent::new(
            Arc::new(RecordingSequenceClient {
                outcomes: AsyncMutex::new(VecDeque::new()),
                recorded_models: Arc::new(AsyncMutex::new(Vec::new())),
            }),
            AgentConfig::default(),
        );
        let mut session_runtime = None;
        let mut context = InteractiveRunnerContext {
            agent: &mut agent,
            session_runtime: &mut session_runtime,
            config,
            prompt_template_bridge_handle: &mut bridge,
            cli: &cli,
            skills_dir: &cli.skills_dir,
        };

        run_interactive_with_runner(
            InteractiveIoMode::Stdin,
            &mut context,
            fake_tty_runner,
            fake_stdin_runner,
        )
        .await
        .expect("mode dispatch");
        assert_eq!(TTY_RUNNER_CALLS.load(Ordering::Relaxed), 0);
        assert_eq!(STDIN_RUNNER_CALLS.load(Ordering::Relaxed), 1);
        bridge.shutdown().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn regression_2548_run_interactive_tty_fails_closed_without_terminal() {
        if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
            return;
        }

        let hooks = disabled_runtime_hooks();
        let harness = InteractiveRunnerHarness::new();
        let config = harness.runtime_config(&hooks);
        let mut cli = test_cli();
        let temp = tempdir().expect("tempdir");
        apply_workspace_paths(&mut cli, temp.path());
        let mut bridge = start_runtime_prompt_template_hot_reload_bridge(&cli, "initial prompt")
            .expect("start bridge");
        let mut agent = Agent::new(
            Arc::new(RecordingSequenceClient {
                outcomes: AsyncMutex::new(VecDeque::new()),
                recorded_models: Arc::new(AsyncMutex::new(Vec::new())),
            }),
            AgentConfig::default(),
        );
        let mut session_runtime = None;

        let error = run_interactive_tty(
            &mut agent,
            &mut session_runtime,
            config,
            &mut bridge,
            &cli,
            &cli.skills_dir,
        )
        .await
        .expect_err("tty runner should fail closed when stdin/stdout are not terminals");
        assert!(error
            .to_string()
            .contains("interactive tty runtime requires terminal stdin/stdout"));
        bridge.shutdown().await;
    }

    #[test]
    fn unit_build_runtime_hook_payload_includes_status_and_error() {
        let payload = build_runtime_hook_payload(
            "run-end",
            "hello",
            5000,
            Some(RuntimeHookRunStatus::Failed),
            Some("network timeout".to_string()),
        );
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "run-end");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["prompt"], "hello");
        assert_eq!(payload["data"]["turn_timeout_ms"], 5000);
        assert_eq!(payload["data"]["status"], "failed");
        assert_eq!(payload["data"]["error"], "network timeout");
    }

    #[test]
    fn regression_build_runtime_hook_payload_omits_optional_fields_when_unset() {
        let payload = build_runtime_hook_payload("run-start", "hello", 0, None, None);
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "run-start");
        assert_eq!(payload["data"]["prompt"], "hello");
        assert_eq!(payload["data"]["turn_timeout_ms"], 0);
        assert!(payload["data"].get("status").is_none());
        assert!(payload["data"].get("error").is_none());
    }

    #[test]
    fn unit_apply_runtime_message_transform_disabled_returns_original_prompt() {
        let config = RuntimeExtensionHooksConfig {
            enabled: false,
            root: std::path::PathBuf::from(".tau/extensions"),
        };
        let transformed = apply_runtime_message_transform(&config, "hello");
        assert_eq!(transformed, "hello");
    }

    #[test]
    fn regression_apply_runtime_message_transform_missing_root_returns_original_prompt() {
        let temp = tempdir().expect("tempdir");
        let missing_root = temp.path().join("missing");
        let config = RuntimeExtensionHooksConfig {
            enabled: true,
            root: missing_root,
        };
        let transformed = apply_runtime_message_transform(&config, "hello");
        assert_eq!(transformed, "hello");
    }

    #[test]
    fn unit_render_orchestrator_policy_inheritance_context_is_deterministic() {
        let config = RuntimeExtensionHooksConfig {
            enabled: true,
            root: std::path::PathBuf::from(".tau/extensions"),
        };
        let tool_policy_json = serde_json::json!({
            "schema_version": 1,
            "preset": "balanced",
            "bash_profile": "balanced",
            "os_sandbox_mode": "off",
            "os_sandbox_policy_mode": "best-effort",
            "bash_dry_run": false,
            "enforce_regular_files": true,
            "max_command_length": 4096,
            "max_command_output_bytes": 16000,
            "http_timeout_ms": 20000,
            "http_max_response_bytes": 256000,
            "http_max_redirects": 5,
            "http_allow_http": false,
            "http_allow_private_network": false,
            "allowed_roots": ["/tmp/project"],
            "allowed_commands": ["cat", "ls"],
        });
        let context = render_orchestrator_policy_inheritance_context(&tool_policy_json, &config)
            .expect("policy context should render");
        assert_eq!(
            context,
            "schema_version=1;preset=balanced;bash_profile=balanced;os_sandbox_mode=off;os_sandbox_policy_mode=best-effort;os_sandbox_docker_enabled=false;bash_dry_run=false;enforce_regular_files=true;max_command_length=4096;max_command_output_bytes=16000;http_timeout_ms=20000;http_max_response_bytes=256000;http_max_redirects=5;http_allow_http=false;http_allow_private_network=false;allowed_roots=1;allowed_commands=2;extension_runtime_hooks_enabled=true"
        );
    }

    #[test]
    fn regression_render_orchestrator_policy_inheritance_context_fails_closed_on_invalid_payload() {
        let config = RuntimeExtensionHooksConfig {
            enabled: false,
            root: std::path::PathBuf::from(".tau/extensions"),
        };
        let invalid_tool_policy_json = serde_json::json!({
            "preset": "balanced",
            "allowed_roots": ["/tmp/project"],
            "allowed_commands": ["cat"],
        });
        let error =
            render_orchestrator_policy_inheritance_context(&invalid_tool_policy_json, &config)
                .expect_err("missing required fields should fail");
        assert!(error
            .to_string()
            .contains("missing numeric field 'schema_version'"));
    }

    #[test]
    fn unit_repl_multiline_state_handles_line_continuation() {
        let mut multiline = ReplMultilineState::default();
        assert_eq!(multiline.prompt(), "tau> ");

        assert_eq!(multiline.push_line("first line\\".to_string()), None);
        assert_eq!(multiline.prompt(), "...> ");
        let complete = multiline.push_line("second line".to_string());
        assert_eq!(complete.as_deref(), Some("first line\nsecond line"));
        assert_eq!(multiline.prompt(), "tau> ");
    }

    #[test]
    fn regression_repl_multiline_state_preserves_escaped_backslash() {
        let mut multiline = ReplMultilineState::default();
        let complete = multiline.push_line("path \\\\".to_string());
        assert_eq!(complete.as_deref(), Some("path \\\\"));
    }

    #[test]
    fn functional_repl_command_completer_matches_slash_commands() {
        let completer = ReplCommandCompleter::new(tau_ops::COMMAND_NAMES);
        let suggestions = completer.complete_token("/session");
        assert!(suggestions.contains(&"/session".to_string()));
        assert!(suggestions.contains(&"/session-import".to_string()));
        assert!(suggestions.contains(&"/session-search".to_string()));
    }

    #[test]
    fn unit_resolve_repl_history_path_defaults_under_tau() {
        let path = resolve_repl_history_path(None);
        assert_eq!(path, std::path::PathBuf::from(".tau/repl_history.txt"));
    }

    #[test]
    fn regression_resolve_repl_history_path_uses_session_file_stem() {
        let session_path = std::path::Path::new("/tmp/tau/sessions/default.jsonl");
        let history_path = resolve_repl_history_path(Some(session_path));
        assert_eq!(
            history_path,
            std::path::Path::new("/tmp/tau/sessions/default.history")
        );
    }

    #[test]
    fn spec_2536_c02_prompt_complexity_and_task_override_select_model() {
        let profile = routed_profile_defaults();

        assert_eq!(
            classify_prompt_complexity("write a Rust function and fix failing tests"),
            PromptComplexity::Heavy
        );
        let selected = select_routed_dispatch_model(
            &profile,
            PromptProcessType::Channel,
            "write a Rust function and fix failing tests",
        );
        assert_eq!(selected.as_deref(), Some("openai/o3-mini"));
    }

    #[test]
    fn regression_2536_select_model_falls_back_to_process_route_without_task_override() {
        let profile = routed_profile_defaults();
        let selected = select_routed_dispatch_model(
            &profile,
            PromptProcessType::Channel,
            "hello from channel",
        );
        assert_eq!(selected.as_deref(), Some("openai/gpt-4.1-mini"));
    }

    #[test]
    fn regression_2536_classify_prompt_complexity_respects_keyword_and_length_boundaries() {
        assert_eq!(classify_prompt_complexity("hello"), PromptComplexity::Light);
        assert_eq!(
            classify_prompt_complexity("explain this output"),
            PromptComplexity::Standard
        );
        assert_eq!(
            classify_prompt_complexity(&"a".repeat(140)),
            PromptComplexity::Standard
        );
        assert_eq!(
            classify_prompt_complexity(&"a".repeat(320)),
            PromptComplexity::Heavy
        );
    }

    #[tokio::test]
    async fn spec_2536_c03_dispatch_uses_scoped_model_override_and_restores_baseline() {
        let recorded_models = Arc::new(AsyncMutex::new(Vec::<String>::new()));
        let mut agent = Agent::new(
            Arc::new(RecordingSequenceClient {
                outcomes: AsyncMutex::new(VecDeque::from([
                    Ok(sample_chat_response("routed")),
                    Ok(sample_chat_response("baseline")),
                ])),
                recorded_models: recorded_models.clone(),
            }),
            AgentConfig {
                model: "openai/gpt-4o-mini".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut session_runtime = None;
        let runtime_hooks = disabled_runtime_hooks();
        let routed_profile = routed_profile_defaults();

        run_prompt_with_profile_routing(
            &mut agent,
            &mut session_runtime,
            "write a Rust function and fix failing tests",
            0,
            test_render_options(),
            &runtime_hooks,
            Some(&routed_profile),
        )
        .await
        .expect("routed prompt dispatch should succeed");

        run_prompt_with_profile_routing(
            &mut agent,
            &mut session_runtime,
            "hello",
            0,
            test_render_options(),
            &runtime_hooks,
            None,
        )
        .await
        .expect("baseline prompt dispatch should succeed");

        assert_eq!(
            recorded_models.lock().await.clone(),
            vec![
                "openai/o3-mini".to_string(),
                "openai/gpt-4o-mini".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn regression_2536_default_profile_without_routing_keeps_baseline_model() {
        let recorded_models = Arc::new(AsyncMutex::new(Vec::<String>::new()));
        let mut agent = Agent::new(
            Arc::new(RecordingSequenceClient {
                outcomes: AsyncMutex::new(VecDeque::from([Ok(sample_chat_response("baseline"))])),
                recorded_models: recorded_models.clone(),
            }),
            AgentConfig {
                model: "openai/gpt-4o-mini".to_string(),
                ..AgentConfig::default()
            },
        );
        let mut session_runtime = None;
        let runtime_hooks = disabled_runtime_hooks();
        let default_profile = sample_profile_defaults();

        run_prompt_with_profile_routing(
            &mut agent,
            &mut session_runtime,
            "write a Rust function and fix failing tests",
            0,
            test_render_options(),
            &runtime_hooks,
            Some(&default_profile),
        )
        .await
        .expect("default profile prompt dispatch should succeed");

        assert_eq!(
            recorded_models.lock().await.clone(),
            vec!["openai/gpt-4o-mini".to_string()]
        );
    }
}
