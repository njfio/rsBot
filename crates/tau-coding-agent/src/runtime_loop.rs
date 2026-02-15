use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
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
use tau_agent_core::{Agent, AgentError, CooperativeCancellationToken};
use tau_ai::StreamDeltaHandler;
use tau_cli::{Cli, CliOrchestratorMode};
use tau_core::current_unix_timestamp_ms;
use tau_extensions::{apply_extension_message_transforms, dispatch_extension_runtime_hook};
use tau_onboarding::startup_resolution::ensure_non_empty_text;
use tau_session::SessionRuntime;
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
use crate::runtime_types::{CommandExecutionContext, RenderOptions};

const EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;
const REPL_PROMPT: &str = "tau> ";
const REPL_CONTINUATION_PROMPT: &str = "...> ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeExtensionHooksConfig {
    pub(crate) enabled: bool,
    pub(crate) root: PathBuf,
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
    pub(crate) command_context: CommandExecutionContext<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractiveLoopControl {
    Continue,
    Exit,
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

pub(crate) async fn run_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<()> {
    let status = run_prompt_with_runtime_hooks(
        agent,
        session_runtime,
        prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        render_options,
        extension_runtime_hooks,
    )
    .await?;
    report_prompt_status(status);
    Ok(())
}

pub(crate) async fn run_interactive(
    mut agent: Agent,
    mut session_runtime: Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
) -> Result<()> {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        run_interactive_tty(&mut agent, &mut session_runtime, config).await
    } else {
        run_interactive_stdin(&mut agent, &mut session_runtime, config).await
    }
}

async fn run_interactive_stdin(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
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
) -> Result<()> {
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
        config.extension_runtime_hooks,
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

async fn run_prompt_with_runtime_hooks<F>(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    cancellation_signal: F,
    render_options: RenderOptions,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<PromptRunStatus>
where
    F: Future,
{
    let effective_prompt = apply_runtime_message_transform(extension_runtime_hooks, prompt);
    dispatch_runtime_hook(
        extension_runtime_hooks,
        "run-start",
        build_runtime_hook_payload("run-start", &effective_prompt, turn_timeout_ms, None, None),
    );

    let result = run_prompt_with_cancellation(
        agent,
        session_runtime,
        &effective_prompt,
        turn_timeout_ms,
        cancellation_signal,
        render_options,
    )
    .await;

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
        "schema_version={schema_version};preset={preset};bash_profile={bash_profile};os_sandbox_mode={os_sandbox_mode};os_sandbox_policy_mode={os_sandbox_policy_mode};bash_dry_run={bash_dry_run};enforce_regular_files={enforce_regular_files};max_command_length={max_command_length};max_command_output_bytes={max_command_output_bytes};http_timeout_ms={http_timeout_ms};http_max_response_bytes={http_max_response_bytes};http_max_redirects={http_max_redirects};http_allow_http={http_allow_http};http_allow_private_network={http_allow_private_network};allowed_roots={};allowed_commands={};extension_runtime_hooks_enabled={}",
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
    persist_messages(session_runtime, &new_messages)?;
    print_assistant_messages(
        &new_messages,
        render_options,
        streamed_output.load(Ordering::Relaxed),
    );
    Ok(PromptRunStatus::Completed)
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
        apply_runtime_message_transform, build_runtime_hook_payload,
        render_orchestrator_policy_inheritance_context, resolve_repl_history_path,
        ReplCommandCompleter, ReplMultilineState, RuntimeExtensionHooksConfig,
        RuntimeHookRunStatus,
    };
    use tempfile::tempdir;

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
            "schema_version=1;preset=balanced;bash_profile=balanced;os_sandbox_mode=off;os_sandbox_policy_mode=best-effort;bash_dry_run=false;enforce_regular_files=true;max_command_length=4096;max_command_output_bytes=16000;http_timeout_ms=20000;http_max_response_bytes=256000;http_max_redirects=5;http_allow_http=false;http_allow_private_network=false;allowed_roots=1;allowed_commands=2;extension_runtime_hooks_enabled=true"
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
}
