mod session;
mod skills;
mod tools;

use std::{
    collections::HashMap,
    future::Future,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgAction, Parser, ValueEnum};
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::{
    AnthropicClient, AnthropicConfig, GoogleClient, GoogleConfig, LlmClient, Message, MessageRole,
    ModelRef, OpenAiClient, OpenAiConfig, Provider,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::session::{SessionImportMode, SessionStore};
use crate::skills::{
    augment_system_prompt, fetch_registry_manifest, install_remote_skills, install_skills,
    load_catalog, resolve_registry_skill_sources, resolve_remote_skill_sources,
    resolve_selected_skills, TrustedKey,
};
use crate::tools::{BashCommandProfile, OsSandboxMode, ToolPolicy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliBashProfile {
    Permissive,
    Balanced,
    Strict,
}

impl From<CliBashProfile> for BashCommandProfile {
    fn from(value: CliBashProfile) -> Self {
        match value {
            CliBashProfile::Permissive => BashCommandProfile::Permissive,
            CliBashProfile::Balanced => BashCommandProfile::Balanced,
            CliBashProfile::Strict => BashCommandProfile::Strict,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliOsSandboxMode {
    Off,
    Auto,
    Force,
}

impl From<CliOsSandboxMode> for OsSandboxMode {
    fn from(value: CliOsSandboxMode) -> Self {
        match value {
            CliOsSandboxMode::Off => OsSandboxMode::Off,
            CliOsSandboxMode::Auto => OsSandboxMode::Auto,
            CliOsSandboxMode::Force => OsSandboxMode::Force,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliSessionImportMode {
    Merge,
    Replace,
}

impl From<CliSessionImportMode> for SessionImportMode {
    fn from(value: CliSessionImportMode) -> Self {
        match value {
            CliSessionImportMode::Merge => SessionImportMode::Merge,
            CliSessionImportMode::Replace => SessionImportMode::Replace,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "pi-rs",
    about = "Pure Rust coding agent inspired by pi-mono",
    version
)]
struct Cli {
    #[arg(
        long,
        env = "PI_MODEL",
        default_value = "openai/gpt-4o-mini",
        help = "Model in provider/model format. Supported providers: openai, anthropic, google."
    )]
    model: String,

    #[arg(
        long,
        env = "PI_API_BASE",
        default_value = "https://api.openai.com/v1",
        help = "Base URL for OpenAI-compatible APIs"
    )]
    api_base: String,

    #[arg(
        long,
        env = "PI_ANTHROPIC_API_BASE",
        default_value = "https://api.anthropic.com/v1",
        help = "Base URL for Anthropic Messages API"
    )]
    anthropic_api_base: String,

    #[arg(
        long,
        env = "PI_GOOGLE_API_BASE",
        default_value = "https://generativelanguage.googleapis.com/v1beta",
        help = "Base URL for Google Gemini API"
    )]
    google_api_base: String,

    #[arg(
        long,
        env = "PI_API_KEY",
        hide_env_values = true,
        help = "Generic API key fallback"
    )]
    api_key: Option<String>,

    #[arg(
        long,
        env = "OPENAI_API_KEY",
        hide_env_values = true,
        help = "API key for OpenAI-compatible APIs"
    )]
    openai_api_key: Option<String>,

    #[arg(
        long,
        env = "ANTHROPIC_API_KEY",
        hide_env_values = true,
        help = "API key for Anthropic"
    )]
    anthropic_api_key: Option<String>,

    #[arg(
        long,
        env = "GEMINI_API_KEY",
        hide_env_values = true,
        help = "API key for Google Gemini"
    )]
    google_api_key: Option<String>,

    #[arg(
        long,
        env = "PI_SYSTEM_PROMPT",
        default_value = "You are a focused coding assistant. Prefer concrete steps and safe edits.",
        help = "System prompt"
    )]
    system_prompt: String,

    #[arg(
        long,
        env = "PI_SYSTEM_PROMPT_FILE",
        help = "Load system prompt from a UTF-8 text file (overrides --system-prompt)"
    )]
    system_prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_SKILLS_DIR",
        default_value = ".pi/skills",
        help = "Directory containing skill markdown files"
    )]
    skills_dir: PathBuf,

    #[arg(
        long = "skill",
        env = "PI_SKILL",
        value_delimiter = ',',
        help = "Skill name(s) to include in the system prompt"
    )]
    skills: Vec<String>,

    #[arg(
        long = "install-skill",
        env = "PI_INSTALL_SKILL",
        value_delimiter = ',',
        help = "Skill markdown file(s) to install into --skills-dir before startup"
    )]
    install_skill: Vec<PathBuf>,

    #[arg(
        long = "install-skill-url",
        env = "PI_INSTALL_SKILL_URL",
        value_delimiter = ',',
        help = "Skill URL(s) to install into --skills-dir before startup"
    )]
    install_skill_url: Vec<String>,

    #[arg(
        long = "install-skill-sha256",
        env = "PI_INSTALL_SKILL_SHA256",
        value_delimiter = ',',
        help = "Optional sha256 value(s) matching --install-skill-url entries"
    )]
    install_skill_sha256: Vec<String>,

    #[arg(
        long = "skill-registry-url",
        env = "PI_SKILL_REGISTRY_URL",
        help = "Remote registry manifest URL for skills"
    )]
    skill_registry_url: Option<String>,

    #[arg(
        long = "skill-registry-sha256",
        env = "PI_SKILL_REGISTRY_SHA256",
        help = "Optional sha256 checksum for the registry manifest"
    )]
    skill_registry_sha256: Option<String>,

    #[arg(
        long = "install-skill-from-registry",
        env = "PI_INSTALL_SKILL_FROM_REGISTRY",
        value_delimiter = ',',
        help = "Skill name(s) to install from the remote registry"
    )]
    install_skill_from_registry: Vec<String>,

    #[arg(
        long = "skill-trust-root",
        env = "PI_SKILL_TRUST_ROOT",
        value_delimiter = ',',
        help = "Trusted root key(s) for skill signature verification in key_id=base64_public_key format"
    )]
    skill_trust_root: Vec<String>,

    #[arg(
        long = "skill-trust-root-file",
        env = "PI_SKILL_TRUST_ROOT_FILE",
        help = "JSON file containing trusted root keys for skill signature verification"
    )]
    skill_trust_root_file: Option<PathBuf>,

    #[arg(
        long = "skill-trust-add",
        env = "PI_SKILL_TRUST_ADD",
        value_delimiter = ',',
        help = "Add or update trusted key(s) in --skill-trust-root-file (key_id=base64_public_key)"
    )]
    skill_trust_add: Vec<String>,

    #[arg(
        long = "skill-trust-revoke",
        env = "PI_SKILL_TRUST_REVOKE",
        value_delimiter = ',',
        help = "Revoke trusted key id(s) in --skill-trust-root-file"
    )]
    skill_trust_revoke: Vec<String>,

    #[arg(
        long = "skill-trust-rotate",
        env = "PI_SKILL_TRUST_ROTATE",
        value_delimiter = ',',
        help = "Rotate trusted key(s) in --skill-trust-root-file using old_id:new_id=base64_public_key"
    )]
    skill_trust_rotate: Vec<String>,

    #[arg(
        long = "require-signed-skills",
        env = "PI_REQUIRE_SIGNED_SKILLS",
        default_value_t = false,
        help = "Require selected registry skills to provide signature metadata and validate against trusted roots"
    )]
    require_signed_skills: bool,

    #[arg(long, env = "PI_MAX_TURNS", default_value_t = 8)]
    max_turns: usize,

    #[arg(
        long,
        env = "PI_REQUEST_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "HTTP request timeout for provider API calls in milliseconds"
    )]
    request_timeout_ms: u64,

    #[arg(
        long,
        env = "PI_TURN_TIMEOUT_MS",
        default_value_t = 0,
        help = "Optional per-prompt timeout in milliseconds (0 disables timeout)"
    )]
    turn_timeout_ms: u64,

    #[arg(long, help = "Print agent lifecycle events as JSON")]
    json_events: bool,

    #[arg(
        long,
        env = "PI_STREAM_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Render assistant text output token-by-token"
    )]
    stream_output: bool,

    #[arg(
        long,
        env = "PI_STREAM_DELAY_MS",
        default_value_t = 0,
        help = "Delay between streamed output chunks in milliseconds"
    )]
    stream_delay_ms: u64,

    #[arg(long, help = "Run one prompt and exit")]
    prompt: Option<String>,

    #[arg(
        long,
        env = "PI_PROMPT_FILE",
        conflicts_with = "prompt",
        help = "Read one prompt from a UTF-8 text file and exit"
    )]
    prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_SESSION",
        default_value = ".pi/sessions/default.jsonl",
        help = "Session JSONL file"
    )]
    session: PathBuf,

    #[arg(long, help = "Disable session persistence")]
    no_session: bool,

    #[arg(
        long,
        env = "PI_SESSION_VALIDATE",
        default_value_t = false,
        help = "Validate session graph integrity and exit"
    )]
    session_validate: bool,

    #[arg(
        long,
        env = "PI_SESSION_IMPORT_MODE",
        value_enum,
        default_value = "merge",
        help = "Import mode for /session-import: merge appends with id remapping, replace overwrites the current session"
    )]
    session_import_mode: CliSessionImportMode,

    #[arg(long, help = "Start from a specific session entry id")]
    branch_from: Option<u64>,

    #[arg(
        long,
        env = "PI_SESSION_LOCK_WAIT_MS",
        default_value_t = 5_000,
        help = "Maximum time to wait for acquiring the session lock in milliseconds"
    )]
    session_lock_wait_ms: u64,

    #[arg(
        long,
        env = "PI_SESSION_LOCK_STALE_MS",
        default_value_t = 30_000,
        help = "Lock-file age threshold in milliseconds before stale session locks are reclaimed (0 disables reclaim)"
    )]
    session_lock_stale_ms: u64,

    #[arg(
        long = "allow-path",
        env = "PI_ALLOW_PATH",
        value_delimiter = ',',
        help = "Allowed filesystem roots for read/write/edit/bash cwd (repeatable or comma-separated)"
    )]
    allow_path: Vec<PathBuf>,

    #[arg(
        long,
        env = "PI_BASH_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout for bash tool commands in milliseconds"
    )]
    bash_timeout_ms: u64,

    #[arg(
        long,
        env = "PI_MAX_TOOL_OUTPUT_BYTES",
        default_value_t = 16_000,
        help = "Maximum bytes returned from tool outputs (stdout/stderr)"
    )]
    max_tool_output_bytes: usize,

    #[arg(
        long,
        env = "PI_MAX_FILE_READ_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size read by the read tool"
    )]
    max_file_read_bytes: usize,

    #[arg(
        long,
        env = "PI_MAX_FILE_WRITE_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size written by write/edit tools"
    )]
    max_file_write_bytes: usize,

    #[arg(
        long,
        env = "PI_MAX_COMMAND_LENGTH",
        default_value_t = 4_096,
        help = "Maximum command length accepted by the bash tool"
    )]
    max_command_length: usize,

    #[arg(
        long,
        env = "PI_ALLOW_COMMAND_NEWLINES",
        default_value_t = false,
        help = "Allow newline characters in bash commands"
    )]
    allow_command_newlines: bool,

    #[arg(
        long,
        env = "PI_BASH_PROFILE",
        value_enum,
        default_value = "balanced",
        help = "Command execution profile for bash tool: permissive, balanced, or strict"
    )]
    bash_profile: CliBashProfile,

    #[arg(
        long = "allow-command",
        env = "PI_ALLOW_COMMAND",
        value_delimiter = ',',
        help = "Additional command executables/prefixes to allow (supports trailing '*' wildcards)"
    )]
    allow_command: Vec<String>,

    #[arg(
        long,
        env = "PI_PRINT_TOOL_POLICY",
        default_value_t = false,
        help = "Print effective tool policy JSON before executing prompts"
    )]
    print_tool_policy: bool,

    #[arg(
        long,
        env = "PI_TOOL_AUDIT_LOG",
        help = "Optional JSONL file path for tool execution audit events"
    )]
    tool_audit_log: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_OS_SANDBOX_MODE",
        value_enum,
        default_value = "off",
        help = "OS sandbox mode for bash tool: off, auto, or force"
    )]
    os_sandbox_mode: CliOsSandboxMode,

    #[arg(
        long = "os-sandbox-command",
        env = "PI_OS_SANDBOX_COMMAND",
        value_delimiter = ',',
        help = "Optional sandbox launcher command template tokens. Supports placeholders: {shell}, {command}, {cwd}"
    )]
    os_sandbox_command: Vec<String>,

    #[arg(
        long,
        env = "PI_ENFORCE_REGULAR_FILES",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Require read/edit targets and existing write targets to be regular files (reject symlink targets)"
    )]
    enforce_regular_files: bool,
}

#[derive(Debug)]
struct SessionRuntime {
    store: SessionStore,
    active_head: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandAction {
    Continue,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedCommand<'a> {
    name: &'a str,
    args: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CommandSpec {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
    details: &'static str,
    example: &'static str,
}

const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "/help",
        usage: "/help [command]",
        description: "Show command list or detailed command help",
        details: "Use '/help /command' (or '/help command') for command-specific guidance.",
        example: "/help /branch",
    },
    CommandSpec {
        name: "/session",
        usage: "/session",
        description: "Show session path, entry count, and active head id",
        details: "Read-only command; does not mutate session state.",
        example: "/session",
    },
    CommandSpec {
        name: "/session-export",
        usage: "/session-export <path>",
        description: "Export active lineage snapshot to a JSONL file",
        details: "Writes only the active lineage entries, including schema metadata.",
        example: "/session-export /tmp/session-snapshot.jsonl",
    },
    CommandSpec {
        name: "/session-import",
        usage: "/session-import <path>",
        description: "Import a lineage snapshot into the current session",
        details:
            "Uses --session-import-mode (merge or replace). Merge remaps colliding ids; replace overwrites current entries.",
        example: "/session-import /tmp/session-snapshot.jsonl",
    },
    CommandSpec {
        name: "/policy",
        usage: "/policy",
        description: "Print the effective tool policy JSON",
        details: "Useful for debugging allowlists, limits, and sandbox settings.",
        example: "/policy",
    },
    CommandSpec {
        name: "/branches",
        usage: "/branches",
        description: "List branch tips in the current session graph",
        details: "Each row includes entry id, parent id, and a short message summary.",
        example: "/branches",
    },
    CommandSpec {
        name: "/branch",
        usage: "/branch <id>",
        description: "Switch active branch head to a specific entry id",
        details: "Reloads the agent message context to the selected lineage.",
        example: "/branch 12",
    },
    CommandSpec {
        name: "/resume",
        usage: "/resume",
        description: "Jump back to the latest session head",
        details: "Resets active branch to current head and reloads lineage messages.",
        example: "/resume",
    },
    CommandSpec {
        name: "/session-repair",
        usage: "/session-repair",
        description: "Repair malformed session graphs",
        details: "Removes duplicate ids, invalid parent references, and cyclic lineage entries.",
        example: "/session-repair",
    },
    CommandSpec {
        name: "/session-compact",
        usage: "/session-compact",
        description: "Compact session to active lineage",
        details: "Prunes inactive branches and retains only entries reachable from active head.",
        example: "/session-compact",
    },
    CommandSpec {
        name: "/quit",
        usage: "/quit",
        description: "Exit interactive mode",
        details: "Alias: /exit",
        example: "/quit",
    },
];

const COMMAND_NAMES: &[&str] = &[
    "/help",
    "/session",
    "/session-export",
    "/session-import",
    "/policy",
    "/branches",
    "/branch",
    "/resume",
    "/session-repair",
    "/session-compact",
    "/quit",
    "/exit",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy)]
struct RenderOptions {
    stream_output: bool,
    stream_delay_ms: u64,
}

impl RenderOptions {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            stream_output: cli.stream_output,
            stream_delay_ms: cli.stream_delay_ms,
        }
    }
}

#[derive(Clone)]
struct ToolAuditLogger {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
    starts: Arc<Mutex<HashMap<String, Instant>>>,
}

impl ToolAuditLogger {
    fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create tool audit log directory {}",
                        parent.display()
                    )
                })?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open tool audit log {}", path.display()))?;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
            starts: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn log_event(&self, event: &AgentEvent) -> Result<()> {
        let payload = {
            let mut starts = self
                .starts
                .lock()
                .map_err(|_| anyhow!("tool audit state lock is poisoned"))?;
            tool_audit_event_json(event, &mut starts)
        };

        let Some(payload) = payload else {
            return Ok(());
        };
        let line = serde_json::to_string(&payload).context("failed to encode tool audit event")?;
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("tool audit file lock is poisoned"))?;
        writeln!(file, "{line}")
            .with_context(|| format!("failed to write tool audit log {}", self.path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush tool audit log {}", self.path.display()))?;
        Ok(())
    }
}

fn current_unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn tool_audit_event_json(
    event: &AgentEvent,
    starts: &mut HashMap<String, Instant>,
) -> Option<serde_json::Value> {
    match event {
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => {
            starts.insert(tool_call_id.clone(), Instant::now());
            Some(serde_json::json!({
                "timestamp_unix_ms": current_unix_timestamp_ms(),
                "event": "tool_execution_start",
                "tool_call_id": tool_call_id,
                "tool_name": tool_name,
                "arguments_bytes": arguments.to_string().len(),
            }))
        }
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => {
            let duration_ms = starts
                .remove(tool_call_id)
                .map(|started| started.elapsed().as_millis() as u64);
            Some(serde_json::json!({
                "timestamp_unix_ms": current_unix_timestamp_ms(),
                "event": "tool_execution_end",
                "tool_call_id": tool_call_id,
                "tool_name": tool_name,
                "duration_ms": duration_ms,
                "is_error": result.is_error,
                "result_bytes": result.as_text().len(),
            }))
        }
        _ => None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    if cli.session_validate {
        validate_session_file(&cli)?;
        return Ok(());
    }

    if cli.no_session && cli.branch_from.is_some() {
        bail!("--branch-from cannot be used together with --no-session");
    }

    let model_ref = ModelRef::parse(&cli.model)
        .map_err(|error| anyhow!("failed to parse --model '{}': {error}", cli.model))?;

    let client = build_client(&cli, model_ref.provider)
        .with_context(|| format!("failed to create {} client", model_ref.provider))?;
    if !cli.install_skill.is_empty() {
        let report = install_skills(&cli.install_skill, &cli.skills_dir)?;
        println!(
            "skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let remote_skill_sources =
        resolve_remote_skill_sources(&cli.install_skill_url, &cli.install_skill_sha256)?;
    if !remote_skill_sources.is_empty() {
        let report = install_remote_skills(&remote_skill_sources, &cli.skills_dir).await?;
        println!(
            "remote skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let trusted_skill_roots = resolve_skill_trust_roots(&cli)?;
    if !cli.install_skill_from_registry.is_empty() {
        let registry_url = cli.skill_registry_url.as_deref().ok_or_else(|| {
            anyhow!("--skill-registry-url is required when using --install-skill-from-registry")
        })?;
        let manifest =
            fetch_registry_manifest(registry_url, cli.skill_registry_sha256.as_deref()).await?;
        let sources = resolve_registry_skill_sources(
            &manifest,
            &cli.install_skill_from_registry,
            &trusted_skill_roots,
            cli.require_signed_skills,
        )?;
        let report = install_remote_skills(&sources, &cli.skills_dir).await?;
        println!(
            "registry skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let base_system_prompt = resolve_system_prompt(&cli)?;
    let catalog = load_catalog(&cli.skills_dir)
        .with_context(|| format!("failed to load skills from {}", cli.skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    let system_prompt = augment_system_prompt(&base_system_prompt, &selected_skills);

    let mut agent = Agent::new(
        client,
        AgentConfig {
            model: model_ref.model,
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );

    let tool_policy = build_tool_policy(&cli)?;
    let tool_policy_json = tool_policy_to_json(&tool_policy);
    if cli.print_tool_policy {
        println!("{tool_policy_json}");
    }
    tools::register_builtin_tools(&mut agent, tool_policy);
    if let Some(path) = cli.tool_audit_log.clone() {
        let logger = ToolAuditLogger::open(path)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("tool audit logger error: {error}");
            }
        });
    }
    let render_options = RenderOptions::from_cli(&cli);

    let mut session_runtime = if cli.no_session {
        None
    } else {
        Some(initialize_session(&mut agent, &cli, &system_prompt)?)
    };

    if cli.json_events {
        agent.subscribe(|event| {
            let value = event_to_json(event);
            println!("{value}");
        });
    }

    if let Some(prompt) = resolve_prompt_input(&cli)? {
        run_prompt(
            &mut agent,
            &mut session_runtime,
            &prompt,
            cli.turn_timeout_ms,
            render_options,
        )
        .await?;
        return Ok(());
    }

    run_interactive(
        agent,
        session_runtime,
        cli.turn_timeout_ms,
        render_options,
        cli.session_import_mode.into(),
        tool_policy_json,
    )
    .await
}

fn resolve_prompt_input(cli: &Cli) -> Result<Option<String>> {
    if let Some(prompt) = &cli.prompt {
        return Ok(Some(prompt.clone()));
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

fn resolve_system_prompt(cli: &Cli) -> Result<String> {
    let Some(path) = cli.system_prompt_file.as_ref() else {
        return Ok(cli.system_prompt.clone());
    };

    let system_prompt = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read system prompt file {}", path.display()))?;

    ensure_non_empty_text(
        system_prompt,
        format!("system prompt file {}", path.display()),
    )
}

fn ensure_non_empty_text(text: String, source: String) -> Result<String> {
    if text.trim().is_empty() {
        bail!("{source} is empty");
    }
    Ok(text)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrustedRootRecord {
    id: String,
    public_key: String,
    #[serde(default)]
    revoked: bool,
    expires_unix: Option<u64>,
    rotated_from: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum TrustedRootFileFormat {
    List(Vec<TrustedRootRecord>),
    Wrapped { roots: Vec<TrustedRootRecord> },
    Keys { keys: Vec<TrustedRootRecord> },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TrustMutationReport {
    added: usize,
    updated: usize,
    revoked: usize,
    rotated: usize,
}

fn resolve_skill_trust_roots(cli: &Cli) -> Result<Vec<TrustedKey>> {
    let has_store_mutation = !cli.skill_trust_add.is_empty()
        || !cli.skill_trust_revoke.is_empty()
        || !cli.skill_trust_rotate.is_empty();
    if has_store_mutation && cli.skill_trust_root_file.is_none() {
        bail!("--skill-trust-root-file is required when using trust lifecycle flags");
    }

    let mut roots = Vec::new();
    for raw in &cli.skill_trust_root {
        roots.push(parse_trusted_root_spec(raw)?);
    }

    if let Some(path) = &cli.skill_trust_root_file {
        let mut records = load_trust_root_records(path)?;
        if has_store_mutation {
            let report = apply_trust_root_mutations(&mut records, cli)?;
            save_trust_root_records(path, &records)?;
            println!(
                "skill trust store update: added={} updated={} revoked={} rotated={}",
                report.added, report.updated, report.revoked, report.rotated
            );
        }

        let now_unix = current_unix_timestamp();
        for item in records {
            if item.revoked || is_expired_unix(item.expires_unix, now_unix) {
                continue;
            }
            roots.push(TrustedKey {
                id: item.id,
                public_key: item.public_key,
            });
        }
    }

    Ok(roots)
}

fn parse_trusted_root_spec(raw: &str) -> Result<TrustedKey> {
    let (id, public_key) = raw
        .split_once('=')
        .ok_or_else(|| anyhow!("invalid --skill-trust-root '{raw}', expected key_id=base64_key"))?;
    let id = id.trim();
    let public_key = public_key.trim();
    if id.is_empty() || public_key.is_empty() {
        bail!("invalid --skill-trust-root '{raw}', expected key_id=base64_key");
    }
    Ok(TrustedKey {
        id: id.to_string(),
        public_key: public_key.to_string(),
    })
}

fn parse_trust_rotation_spec(raw: &str) -> Result<(String, TrustedKey)> {
    let (old_id, new_spec) = raw.split_once(':').ok_or_else(|| {
        anyhow!("invalid --skill-trust-rotate '{raw}', expected old_id:new_id=base64_key")
    })?;
    let old_id = old_id.trim();
    if old_id.is_empty() {
        bail!("invalid --skill-trust-rotate '{raw}', expected old_id:new_id=base64_key");
    }
    let new_key = parse_trusted_root_spec(new_spec)?;
    Ok((old_id.to_string(), new_key))
}

fn load_trust_root_records(path: &PathBuf) -> Result<Vec<TrustedRootRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = serde_json::from_str::<TrustedRootFileFormat>(&raw)
        .with_context(|| format!("failed to parse trusted root file {}", path.display()))?;

    let records = match parsed {
        TrustedRootFileFormat::List(items) => items,
        TrustedRootFileFormat::Wrapped { roots } => roots,
        TrustedRootFileFormat::Keys { keys } => keys,
    };

    Ok(records)
}

fn save_trust_root_records(path: &PathBuf, records: &[TrustedRootRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = serde_json::to_string_pretty(&TrustedRootFileFormat::Wrapped {
        roots: records.to_vec(),
    })
    .context("failed to serialize trusted root records")?;
    std::fs::write(path, payload).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn apply_trust_root_mutations(
    records: &mut Vec<TrustedRootRecord>,
    cli: &Cli,
) -> Result<TrustMutationReport> {
    let mut report = TrustMutationReport::default();

    for spec in &cli.skill_trust_add {
        let key = parse_trusted_root_spec(spec)?;
        if let Some(existing) = records.iter_mut().find(|record| record.id == key.id) {
            existing.public_key = key.public_key;
            existing.revoked = false;
            existing.rotated_from = None;
            report.updated += 1;
        } else {
            records.push(TrustedRootRecord {
                id: key.id,
                public_key: key.public_key,
                revoked: false,
                expires_unix: None,
                rotated_from: None,
            });
            report.added += 1;
        }
    }

    for id in &cli.skill_trust_revoke {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or_else(|| anyhow!("cannot revoke unknown trust key id '{}'", id))?;
        if !record.revoked {
            record.revoked = true;
            report.revoked += 1;
        }
    }

    for spec in &cli.skill_trust_rotate {
        let (old_id, new_key) = parse_trust_rotation_spec(spec)?;
        let old = records
            .iter_mut()
            .find(|record| record.id == old_id)
            .ok_or_else(|| anyhow!("cannot rotate unknown trust key id '{}'", old_id))?;
        old.revoked = true;

        if let Some(existing_new) = records.iter_mut().find(|record| record.id == new_key.id) {
            existing_new.public_key = new_key.public_key;
            existing_new.revoked = false;
            existing_new.rotated_from = Some(old_id.clone());
            report.updated += 1;
        } else {
            records.push(TrustedRootRecord {
                id: new_key.id,
                public_key: new_key.public_key,
                revoked: false,
                expires_unix: None,
                rotated_from: Some(old_id.clone()),
            });
            report.added += 1;
        }
        report.rotated += 1;
    }

    Ok(report)
}

fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_expired_unix(expires_unix: Option<u64>, now_unix: u64) -> bool {
    matches!(expires_unix, Some(value) if value <= now_unix)
}

fn validate_session_file(cli: &Cli) -> Result<()> {
    if cli.no_session {
        bail!("--session-validate cannot be used together with --no-session");
    }

    let store = SessionStore::load(&cli.session)?;
    let report = store.validation_report();
    println!(
        "session validation: path={} entries={} duplicates={} invalid_parent={} cycles={}",
        cli.session.display(),
        report.entries,
        report.duplicates,
        report.invalid_parent,
        report.cycles
    );
    if report.is_valid() {
        println!("session validation passed");
        Ok(())
    } else {
        bail!(
            "session validation failed: duplicates={} invalid_parent={} cycles={}",
            report.duplicates,
            report.invalid_parent,
            report.cycles
        );
    }
}

fn initialize_session(agent: &mut Agent, cli: &Cli, system_prompt: &str) -> Result<SessionRuntime> {
    let mut store = SessionStore::load(&cli.session)?;
    store.set_lock_policy(cli.session_lock_wait_ms.max(1), cli.session_lock_stale_ms);

    let mut active_head = store.ensure_initialized(system_prompt)?;
    if let Some(branch_id) = cli.branch_from {
        if !store.contains(branch_id) {
            bail!(
                "session {} does not contain entry id {}",
                store.path().display(),
                branch_id
            );
        }
        active_head = Some(branch_id);
    }

    let lineage = store.lineage_messages(active_head)?;
    if !lineage.is_empty() {
        agent.replace_messages(lineage);
    }

    Ok(SessionRuntime { store, active_head })
}

async fn run_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
) -> Result<()> {
    let status = run_prompt_with_cancellation(
        agent,
        session_runtime,
        prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        render_options,
    )
    .await?;
    if status == PromptRunStatus::Cancelled {
        println!("\nrequest cancelled\n");
    } else if status == PromptRunStatus::TimedOut {
        println!("\nrequest timed out\n");
    }
    Ok(())
}

async fn run_interactive(
    mut agent: Agent,
    mut session_runtime: Option<SessionRuntime>,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    session_import_mode: SessionImportMode,
    tool_policy_json: serde_json::Value,
) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        print!("pi> ");
        std::io::stdout()
            .flush()
            .context("failed to flush stdout")?;

        let Some(line) = lines.next_line().await? else {
            break;
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('/') {
            if handle_command_with_session_import_mode(
                trimmed,
                &mut agent,
                &mut session_runtime,
                &tool_policy_json,
                session_import_mode,
            )? == CommandAction::Exit
            {
                break;
            }
            continue;
        }

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut session_runtime,
            trimmed,
            turn_timeout_ms,
            tokio::signal::ctrl_c(),
            render_options,
        )
        .await?;
        if status == PromptRunStatus::Cancelled {
            println!("\nrequest cancelled\n");
        } else if status == PromptRunStatus::TimedOut {
            println!("\nrequest timed out\n");
        }
    }

    Ok(())
}

async fn run_prompt_with_cancellation<F>(
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
    tokio::pin!(cancellation_signal);

    enum PromptOutcome<T> {
        Result(T),
        Cancelled,
        TimedOut,
    }

    let prompt_result = if turn_timeout_ms == 0 {
        tokio::select! {
            result = agent.prompt(prompt) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
        }
    } else {
        let timeout = tokio::time::sleep(Duration::from_millis(turn_timeout_ms));
        tokio::pin!(timeout);
        tokio::select! {
            result = agent.prompt(prompt) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
            _ = &mut timeout => PromptOutcome::TimedOut,
        }
    };

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

    let new_messages = prompt_result?;
    persist_messages(session_runtime, &new_messages)?;
    print_assistant_messages(&new_messages, render_options);
    Ok(PromptRunStatus::Completed)
}

#[cfg(test)]
fn handle_command(
    command: &str,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    tool_policy_json: &serde_json::Value,
) -> Result<CommandAction> {
    handle_command_with_session_import_mode(
        command,
        agent,
        session_runtime,
        tool_policy_json,
        SessionImportMode::Merge,
    )
}

fn handle_command_with_session_import_mode(
    command: &str,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    tool_policy_json: &serde_json::Value,
    session_import_mode: SessionImportMode,
) -> Result<CommandAction> {
    let Some(parsed) = parse_command(command) else {
        println!("invalid command input: {command}");
        return Ok(CommandAction::Continue);
    };
    let command_name = canonical_command_name(parsed.name);
    let command_args = parsed.args;

    if command_name == "/quit" {
        return Ok(CommandAction::Exit);
    }

    if command_name == "/help" {
        if command_args.is_empty() {
            println!("{}", render_help_overview());
        } else {
            let topic = normalize_help_topic(command_args);
            match render_command_help(&topic) {
                Some(help) => println!("{help}"),
                None => println!("{}", unknown_help_topic_message(&topic)),
            }
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session" {
        if !command_args.is_empty() {
            println!("usage: /session");
            return Ok(CommandAction::Continue);
        }
        match session_runtime.as_ref() {
            Some(runtime) => {
                println!(
                    "session: path={} entries={} active_head={}",
                    runtime.store.path().display(),
                    runtime.store.entries().len(),
                    runtime
                        .active_head
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
            }
            None => println!("session: disabled"),
        }
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-export" {
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.is_empty() {
            println!("usage: /session-export <path>");
            return Ok(CommandAction::Continue);
        }

        let destination = PathBuf::from(command_args);
        let exported = runtime
            .store
            .export_lineage(runtime.active_head, &destination)?;
        println!(
            "session export complete: path={} entries={} head={}",
            destination.display(),
            exported,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-import" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.is_empty() {
            println!("usage: /session-import <path>");
            return Ok(CommandAction::Continue);
        }

        let source = PathBuf::from(command_args);
        let report = runtime
            .store
            .import_snapshot(&source, session_import_mode)?;
        runtime.active_head = report.active_head;
        reload_agent_from_active_head(agent, runtime)?;
        println!(
            "session import complete: path={} mode={} imported_entries={} remapped_entries={} remapped_ids={} replaced_entries={} total_entries={} head={}",
            source.display(),
            session_import_mode_label(session_import_mode),
            report.imported_entries,
            report.remapped_entries,
            format_remap_ids(&report.remapped_ids),
            report.replaced_entries,
            report.resulting_entries,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/policy" {
        if !command_args.is_empty() {
            println!("usage: /policy");
            return Ok(CommandAction::Continue);
        }
        println!("{tool_policy_json}");
        return Ok(CommandAction::Continue);
    }

    if command_name == "/resume" {
        if !command_args.is_empty() {
            println!("usage: /resume");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        runtime.active_head = runtime.store.head_id();
        reload_agent_from_active_head(agent, runtime)?;
        println!(
            "resumed at head {}",
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/branches" {
        if !command_args.is_empty() {
            println!("usage: /branches");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_ref() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let tips = runtime.store.branch_tips();
        if tips.is_empty() {
            println!("no branches");
        } else {
            for tip in tips {
                println!(
                    "id={} parent={} text={}",
                    tip.id,
                    tip.parent_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    summarize_message(&tip.message)
                );
            }
        }

        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-repair" {
        if !command_args.is_empty() {
            println!("usage: /session-repair");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let report = runtime.store.repair()?;
        runtime.active_head = runtime
            .active_head
            .filter(|head| runtime.store.contains(*head))
            .or_else(|| runtime.store.head_id());
        reload_agent_from_active_head(agent, runtime)?;

        println!(
            "repair complete: removed_duplicates={} duplicate_ids={} removed_invalid_parent={} invalid_parent_ids={} removed_cycles={} cycle_ids={}",
            report.removed_duplicates,
            format_id_list(&report.duplicate_ids),
            report.removed_invalid_parent,
            format_id_list(&report.invalid_parent_ids),
            report.removed_cycles,
            format_id_list(&report.cycle_ids)
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/session-compact" {
        if !command_args.is_empty() {
            println!("usage: /session-compact");
            return Ok(CommandAction::Continue);
        }
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let report = runtime.store.compact_to_lineage(runtime.active_head)?;
        runtime.active_head = report
            .head_id
            .filter(|head| runtime.store.contains(*head))
            .or_else(|| runtime.store.head_id());
        reload_agent_from_active_head(agent, runtime)?;

        println!(
            "compact complete: removed_entries={} retained_entries={} head={}",
            report.removed_entries,
            report.retained_entries,
            runtime
                .active_head
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        return Ok(CommandAction::Continue);
    }

    if command_name == "/branch" {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };
        if command_args.is_empty() {
            println!("usage: /branch <id>");
            return Ok(CommandAction::Continue);
        }

        let target = command_args
            .parse::<u64>()
            .map_err(|_| anyhow!("invalid branch id '{}'; expected an integer", command_args))?;

        if !runtime.store.contains(target) {
            bail!("unknown session id {}", target);
        }

        runtime.active_head = Some(target);
        reload_agent_from_active_head(agent, runtime)?;
        println!("switched to branch id {target}");
        return Ok(CommandAction::Continue);
    }

    println!("{}", unknown_command_message(parsed.name));
    Ok(CommandAction::Continue)
}

fn parse_command(input: &str) -> Option<ParsedCommand<'_>> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default();
    let args = parts.next().map(str::trim).unwrap_or_default();
    Some(ParsedCommand { name, args })
}

fn canonical_command_name(name: &str) -> &str {
    if name == "/exit" {
        "/quit"
    } else {
        name
    }
}

fn session_import_mode_label(mode: SessionImportMode) -> &'static str {
    match mode {
        SessionImportMode::Merge => "merge",
        SessionImportMode::Replace => "replace",
    }
}

fn format_id_list(ids: &[u64]) -> String {
    if ids.is_empty() {
        return "none".to_string();
    }
    ids.iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn format_remap_ids(remapped: &[(u64, u64)]) -> String {
    if remapped.is_empty() {
        return "none".to_string();
    }
    remapped
        .iter()
        .map(|(from, to)| format!("{from}->{to}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_help_topic(topic: &str) -> String {
    let trimmed = topic.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn render_help_overview() -> String {
    let mut lines = vec!["commands:".to_string()];
    for spec in COMMAND_SPECS {
        lines.push(format!("  {:<22} {}", spec.usage, spec.description));
    }
    lines.push("tip: run /help <command> for details".to_string());
    lines.join("\n")
}

fn render_command_help(topic: &str) -> Option<String> {
    let normalized = normalize_help_topic(topic);
    let command_name = canonical_command_name(&normalized);
    let spec = COMMAND_SPECS
        .iter()
        .find(|entry| entry.name == command_name)?;
    Some(format!(
        "command: {}\nusage: {}\n{}\n{}\nexample: {}",
        spec.name, spec.usage, spec.description, spec.details, spec.example
    ))
}

fn unknown_help_topic_message(topic: &str) -> String {
    match suggest_command(topic) {
        Some(suggestion) => format!(
            "unknown help topic: {topic}\ndid you mean {suggestion}?\nrun /help for command list"
        ),
        None => format!("unknown help topic: {topic}\nrun /help for command list"),
    }
}

fn unknown_command_message(command: &str) -> String {
    match suggest_command(command) {
        Some(suggestion) => {
            format!("unknown command: {command}\ndid you mean {suggestion}?\nrun /help for command list")
        }
        None => format!("unknown command: {command}\nrun /help for command list"),
    }
}

fn suggest_command(command: &str) -> Option<&'static str> {
    let command = canonical_command_name(command);
    if command.is_empty() {
        return None;
    }

    if let Some(prefix_match) = COMMAND_NAMES
        .iter()
        .find(|candidate| candidate.starts_with(command))
    {
        return Some(prefix_match);
    }

    let mut best: Option<(&str, usize)> = None;
    for candidate in COMMAND_NAMES {
        let distance = levenshtein_distance(command, candidate);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {}
            _ => best = Some((candidate, distance)),
        }
    }

    let (candidate, distance) = best?;
    let threshold = match command.len() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    if distance <= threshold {
        Some(candidate)
    } else {
        None
    }
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, left) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, right) in b_chars.iter().enumerate() {
            let substitution_cost = if left == *right { 0 } else { 1 };
            let deletion = previous[j + 1] + 1;
            let insertion = current[j] + 1;
            let substitution = previous[j] + substitution_cost;
            current[j + 1] = deletion.min(insertion).min(substitution);
        }
        previous.clone_from_slice(&current);
    }

    previous[b_chars.len()]
}

fn reload_agent_from_active_head(agent: &mut Agent, runtime: &SessionRuntime) -> Result<()> {
    let lineage = runtime.store.lineage_messages(runtime.active_head)?;
    agent.replace_messages(lineage);
    Ok(())
}

fn summarize_message(message: &Message) -> String {
    let text = message.text_content().replace('\n', " ");
    if text.trim().is_empty() {
        return format!(
            "{:?} (tool_calls={})",
            message.role,
            message.tool_calls().len()
        );
    }

    let max = 60;
    if text.chars().count() <= max {
        text
    } else {
        let summary = text.chars().take(max).collect::<String>();
        format!("{summary}...")
    }
}

fn persist_messages(
    session_runtime: &mut Option<SessionRuntime>,
    new_messages: &[Message],
) -> Result<()> {
    let Some(runtime) = session_runtime.as_mut() else {
        return Ok(());
    };

    runtime.active_head = runtime
        .store
        .append_messages(runtime.active_head, new_messages)?;
    Ok(())
}

fn print_assistant_messages(messages: &[Message], render_options: RenderOptions) {
    for message in messages {
        if message.role != MessageRole::Assistant {
            continue;
        }

        let text = message.text_content();
        if !text.trim().is_empty() {
            println!();
            if render_options.stream_output {
                let mut stdout = std::io::stdout();
                for chunk in stream_text_chunks(&text) {
                    print!("{chunk}");
                    let _ = stdout.flush();
                    if render_options.stream_delay_ms > 0 {
                        std::thread::sleep(Duration::from_millis(render_options.stream_delay_ms));
                    }
                }
                println!("\n");
            } else {
                println!("{text}\n");
            }
            continue;
        }

        let tool_calls = message.tool_calls();
        if !tool_calls.is_empty() {
            println!(
                "\n[assistant requested {} tool call(s)]\n",
                tool_calls.len()
            );
        }
    }
}

fn stream_text_chunks(text: &str) -> Vec<&str> {
    text.split_inclusive(char::is_whitespace).collect()
}

fn event_to_json(event: &AgentEvent) -> serde_json::Value {
    match event {
        AgentEvent::AgentStart => serde_json::json!({ "type": "agent_start" }),
        AgentEvent::AgentEnd { new_messages } => {
            serde_json::json!({ "type": "agent_end", "new_messages": new_messages })
        }
        AgentEvent::TurnStart { turn } => serde_json::json!({ "type": "turn_start", "turn": turn }),
        AgentEvent::TurnEnd { turn, tool_results } => {
            serde_json::json!({ "type": "turn_end", "turn": turn, "tool_results": tool_results })
        }
        AgentEvent::MessageAdded { message } => serde_json::json!({
            "type": "message_added",
            "role": format!("{:?}", message.role).to_lowercase(),
            "text": message.text_content(),
            "tool_calls": message.tool_calls().len(),
        }),
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => serde_json::json!({
            "type": "tool_execution_start",
            "tool_call_id": tool_call_id,
            "tool_name": tool_name,
            "arguments": arguments,
        }),
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => serde_json::json!({
            "type": "tool_execution_end",
            "tool_call_id": tool_call_id,
            "tool_name": tool_name,
            "is_error": result.is_error,
            "content": result.content,
        }),
    }
}

fn build_client(cli: &Cli, provider: Provider) -> Result<Arc<dyn LlmClient>> {
    match provider {
        Provider::OpenAi => {
            let api_key = resolve_api_key(vec![
                cli.openai_api_key.clone(),
                cli.api_key.clone(),
                std::env::var("OPENAI_API_KEY").ok(),
                std::env::var("PI_API_KEY").ok(),
            ])
            .ok_or_else(|| {
                anyhow!(
                    "missing OpenAI API key. Set OPENAI_API_KEY, PI_API_KEY, --openai-api-key, or --api-key"
                )
            })?;

            let client = OpenAiClient::new(OpenAiConfig {
                api_base: cli.api_base.clone(),
                api_key,
                organization: None,
                request_timeout_ms: cli.request_timeout_ms.max(1),
            })?;
            Ok(Arc::new(client))
        }
        Provider::Anthropic => {
            let api_key = resolve_api_key(vec![
                cli.anthropic_api_key.clone(),
                cli.api_key.clone(),
                std::env::var("ANTHROPIC_API_KEY").ok(),
                std::env::var("PI_API_KEY").ok(),
            ])
            .ok_or_else(|| {
                anyhow!(
                    "missing Anthropic API key. Set ANTHROPIC_API_KEY, PI_API_KEY, --anthropic-api-key, or --api-key"
                )
            })?;

            let client = AnthropicClient::new(AnthropicConfig {
                api_base: cli.anthropic_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
            })?;
            Ok(Arc::new(client))
        }
        Provider::Google => {
            let api_key = resolve_api_key(vec![
                cli.google_api_key.clone(),
                cli.api_key.clone(),
                std::env::var("GEMINI_API_KEY").ok(),
                std::env::var("GOOGLE_API_KEY").ok(),
                std::env::var("PI_API_KEY").ok(),
            ])
            .ok_or_else(|| {
                anyhow!(
                    "missing Google API key. Set GEMINI_API_KEY, GOOGLE_API_KEY, PI_API_KEY, --google-api-key, or --api-key"
                )
            })?;

            let client = GoogleClient::new(GoogleConfig {
                api_base: cli.google_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
            })?;
            Ok(Arc::new(client))
        }
    }
}

fn resolve_api_key(candidates: Vec<Option<String>>) -> Option<String> {
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}

fn build_tool_policy(cli: &Cli) -> Result<ToolPolicy> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut roots = vec![cwd];
    roots.extend(cli.allow_path.clone());

    let mut policy = ToolPolicy::new(roots);
    policy.bash_timeout_ms = cli.bash_timeout_ms.max(1);
    policy.max_command_output_bytes = cli.max_tool_output_bytes.max(128);
    policy.max_file_read_bytes = cli.max_file_read_bytes.max(1_024);
    policy.max_file_write_bytes = cli.max_file_write_bytes.max(1_024);
    policy.max_command_length = cli.max_command_length.max(8);
    policy.allow_command_newlines = cli.allow_command_newlines;
    policy.set_bash_profile(cli.bash_profile.into());
    policy.os_sandbox_mode = cli.os_sandbox_mode.into();
    policy.os_sandbox_command = parse_sandbox_command_tokens(&cli.os_sandbox_command)?;
    policy.enforce_regular_files = cli.enforce_regular_files;
    if !cli.allow_command.is_empty() {
        for command in &cli.allow_command {
            let command = command.trim();
            if command.is_empty() {
                continue;
            }
            if !policy
                .allowed_commands
                .iter()
                .any(|existing| existing == command)
            {
                policy.allowed_commands.push(command.to_string());
            }
        }
    }
    Ok(policy)
}

fn parse_sandbox_command_tokens(raw_tokens: &[String]) -> Result<Vec<String>> {
    let mut parsed = Vec::new();
    for raw in raw_tokens {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens = shell_words::split(trimmed).map_err(|error| {
            anyhow!("invalid --os-sandbox-command token '{}': {error}", trimmed)
        })?;
        if tokens.is_empty() {
            continue;
        }
        parsed.extend(tokens);
    }
    Ok(parsed)
}

fn tool_policy_to_json(policy: &ToolPolicy) -> serde_json::Value {
    serde_json::json!({
        "allowed_roots": policy
            .allowed_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "max_file_read_bytes": policy.max_file_read_bytes,
        "max_file_write_bytes": policy.max_file_write_bytes,
        "max_command_output_bytes": policy.max_command_output_bytes,
        "bash_timeout_ms": policy.bash_timeout_ms,
        "max_command_length": policy.max_command_length,
        "allow_command_newlines": policy.allow_command_newlines,
        "bash_profile": format!("{:?}", policy.bash_profile).to_lowercase(),
        "allowed_commands": policy.allowed_commands.clone(),
        "os_sandbox_mode": format!("{:?}", policy.os_sandbox_mode).to_lowercase(),
        "os_sandbox_command": policy.os_sandbox_command.clone(),
        "enforce_regular_files": policy.enforce_regular_files,
    })
}

fn init_tracing() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, VecDeque},
        future::{pending, ready},
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };

    use async_trait::async_trait;
    use pi_agent_core::{Agent, AgentConfig, AgentEvent, ToolExecutionResult};
    use pi_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, MessageRole, PiAiError,
    };
    use tempfile::tempdir;
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::time::sleep;

    use super::{
        apply_trust_root_mutations, build_tool_policy, ensure_non_empty_text, format_id_list,
        format_remap_ids, handle_command, handle_command_with_session_import_mode,
        initialize_session, parse_command, parse_sandbox_command_tokens, parse_trust_rotation_spec,
        parse_trusted_root_spec, render_command_help, render_help_overview, resolve_prompt_input,
        resolve_skill_trust_roots, resolve_system_prompt, run_prompt_with_cancellation,
        stream_text_chunks, tool_audit_event_json, tool_policy_to_json, unknown_command_message,
        validate_session_file, Cli, CliBashProfile, CliOsSandboxMode, CliSessionImportMode,
        CommandAction, PromptRunStatus, RenderOptions, SessionRuntime, ToolAuditLogger,
        TrustedRootRecord,
    };
    use crate::resolve_api_key;
    use crate::session::{SessionImportMode, SessionStore};
    use crate::tools::{BashCommandProfile, OsSandboxMode};

    struct NoopClient;

    #[async_trait]
    impl LlmClient for NoopClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            Err(PiAiError::InvalidResponse(
                "noop client should not be called".to_string(),
            ))
        }
    }

    struct SuccessClient;

    #[async_trait]
    impl LlmClient for SuccessClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            Ok(ChatResponse {
                message: pi_ai::Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct SlowClient;

    #[async_trait]
    impl LlmClient for SlowClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            sleep(Duration::from_secs(5)).await;
            Ok(ChatResponse {
                message: pi_ai::Message::assistant_text("slow"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    struct QueueClient {
        responses: AsyncMutex<VecDeque<ChatResponse>>,
    }

    #[async_trait]
    impl LlmClient for QueueClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            let mut responses = self.responses.lock().await;
            responses.pop_front().ok_or_else(|| {
                PiAiError::InvalidResponse("mock response queue is empty".to_string())
            })
        }
    }

    fn test_render_options() -> RenderOptions {
        RenderOptions {
            stream_output: false,
            stream_delay_ms: 0,
        }
    }

    fn test_tool_policy_json() -> serde_json::Value {
        serde_json::json!({
            "allowed_roots": [],
            "bash_profile": "balanced",
        })
    }

    fn test_cli() -> Cli {
        Cli {
            model: "openai/gpt-4o-mini".to_string(),
            api_base: "https://api.openai.com/v1".to_string(),
            anthropic_api_base: "https://api.anthropic.com/v1".to_string(),
            google_api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_key: None,
            openai_api_key: None,
            anthropic_api_key: None,
            google_api_key: None,
            system_prompt: "sys".to_string(),
            system_prompt_file: None,
            skills_dir: PathBuf::from(".pi/skills"),
            skills: vec![],
            install_skill: vec![],
            install_skill_url: vec![],
            install_skill_sha256: vec![],
            skill_registry_url: None,
            skill_registry_sha256: None,
            install_skill_from_registry: vec![],
            skill_trust_root: vec![],
            skill_trust_root_file: None,
            skill_trust_add: vec![],
            skill_trust_revoke: vec![],
            skill_trust_rotate: vec![],
            require_signed_skills: false,
            max_turns: 8,
            request_timeout_ms: 120_000,
            turn_timeout_ms: 0,
            json_events: false,
            stream_output: true,
            stream_delay_ms: 0,
            prompt: None,
            prompt_file: None,
            session: PathBuf::from(".pi/sessions/default.jsonl"),
            no_session: false,
            session_validate: false,
            session_import_mode: CliSessionImportMode::Merge,
            branch_from: None,
            session_lock_wait_ms: 5_000,
            session_lock_stale_ms: 30_000,
            allow_path: vec![],
            bash_timeout_ms: 500,
            max_tool_output_bytes: 1024,
            max_file_read_bytes: 2048,
            max_file_write_bytes: 2048,
            max_command_length: 4096,
            allow_command_newlines: true,
            bash_profile: CliBashProfile::Balanced,
            allow_command: vec![],
            print_tool_policy: false,
            tool_audit_log: None,
            os_sandbox_mode: CliOsSandboxMode::Off,
            os_sandbox_command: vec![],
            enforce_regular_files: true,
        }
    }

    #[test]
    fn resolve_api_key_uses_first_non_empty_candidate() {
        let key = resolve_api_key(vec![
            Some("".to_string()),
            Some("  ".to_string()),
            Some("abc".to_string()),
            Some("def".to_string()),
        ]);

        assert_eq!(key, Some("abc".to_string()));
    }

    #[test]
    fn resolve_api_key_returns_none_when_all_candidates_are_empty() {
        let key = resolve_api_key(vec![None, Some("".to_string())]);
        assert!(key.is_none());
    }

    #[test]
    fn unit_resolve_prompt_input_uses_inline_prompt() {
        let mut cli = test_cli();
        cli.prompt = Some("inline prompt".to_string());

        let prompt = resolve_prompt_input(&cli).expect("resolve prompt");
        assert_eq!(prompt.as_deref(), Some("inline prompt"));
    }

    #[test]
    fn unit_ensure_non_empty_text_returns_original_content() {
        let text = ensure_non_empty_text("hello".to_string(), "prompt".to_string())
            .expect("non-empty text should pass");
        assert_eq!(text, "hello");
    }

    #[test]
    fn regression_ensure_non_empty_text_rejects_blank_content() {
        let error = ensure_non_empty_text(" \n\t".to_string(), "prompt".to_string())
            .expect_err("blank text should fail");
        assert!(error.to_string().contains("prompt is empty"));
    }

    #[test]
    fn unit_parse_command_splits_name_and_args_with_extra_whitespace() {
        let parsed = parse_command("   /branch    42   ").expect("parse command");
        assert_eq!(parsed.name, "/branch");
        assert_eq!(parsed.args, "42");
    }

    #[test]
    fn regression_parse_command_rejects_non_slash_input() {
        assert!(parse_command("help").is_none());
    }

    #[test]
    fn functional_render_help_overview_lists_known_commands() {
        let help = render_help_overview();
        assert!(help.contains("/help [command]"));
        assert!(help.contains("/session"));
        assert!(help.contains("/session-export <path>"));
        assert!(help.contains("/session-import <path>"));
        assert!(help.contains("/branch <id>"));
        assert!(help.contains("/quit"));
    }

    #[test]
    fn functional_render_command_help_supports_branch_topic_without_slash() {
        let help = render_command_help("branch").expect("render help");
        assert!(help.contains("command: /branch"));
        assert!(help.contains("usage: /branch <id>"));
        assert!(help.contains("example: /branch 12"));
    }

    #[test]
    fn regression_unknown_command_message_suggests_closest_match() {
        let message = unknown_command_message("/polciy");
        assert!(message.contains("did you mean /policy?"));
    }

    #[test]
    fn regression_unknown_command_message_without_close_match_has_no_suggestion() {
        let message = unknown_command_message("/zzzzzzzz");
        assert!(!message.contains("did you mean"));
    }

    #[test]
    fn unit_format_id_list_renders_none_and_csv() {
        assert_eq!(format_id_list(&[]), "none");
        assert_eq!(format_id_list(&[1, 2, 42]), "1,2,42");
    }

    #[test]
    fn unit_format_remap_ids_renders_none_and_pairs() {
        assert_eq!(format_remap_ids(&[]), "none");
        assert_eq!(format_remap_ids(&[(1, 3), (2, 4)]), "1->3,2->4");
    }

    #[test]
    fn functional_help_command_returns_continue_action() {
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut runtime = None;
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command("/help branch", &mut agent, &mut runtime, &tool_policy_json)
            .expect("help should succeed");
        assert_eq!(action, CommandAction::Continue);
    }

    #[test]
    fn functional_resolve_prompt_input_reads_prompt_file() {
        let temp = tempdir().expect("tempdir");
        let prompt_path = temp.path().join("prompt.txt");
        std::fs::write(&prompt_path, "file prompt\nline two").expect("write prompt");

        let mut cli = test_cli();
        cli.prompt_file = Some(prompt_path);

        let prompt = resolve_prompt_input(&cli).expect("resolve prompt from file");
        assert_eq!(prompt.as_deref(), Some("file prompt\nline two"));
    }

    #[test]
    fn regression_resolve_prompt_input_rejects_empty_prompt_file() {
        let temp = tempdir().expect("tempdir");
        let prompt_path = temp.path().join("prompt.txt");
        std::fs::write(&prompt_path, "   \n\t").expect("write prompt");

        let mut cli = test_cli();
        cli.prompt_file = Some(prompt_path.clone());

        let error = resolve_prompt_input(&cli).expect_err("empty prompt should fail");
        assert!(error
            .to_string()
            .contains(&format!("prompt file {} is empty", prompt_path.display())));
    }

    #[test]
    fn unit_resolve_system_prompt_uses_inline_value_when_file_is_unset() {
        let mut cli = test_cli();
        cli.system_prompt = "inline system".to_string();

        let system_prompt = resolve_system_prompt(&cli).expect("resolve system prompt");
        assert_eq!(system_prompt, "inline system");
    }

    #[test]
    fn functional_resolve_system_prompt_reads_system_prompt_file() {
        let temp = tempdir().expect("tempdir");
        let prompt_path = temp.path().join("system.txt");
        std::fs::write(&prompt_path, "system from file").expect("write prompt");

        let mut cli = test_cli();
        cli.system_prompt_file = Some(prompt_path);

        let system_prompt = resolve_system_prompt(&cli).expect("resolve system prompt");
        assert_eq!(system_prompt, "system from file");
    }

    #[test]
    fn regression_resolve_system_prompt_rejects_empty_system_prompt_file() {
        let temp = tempdir().expect("tempdir");
        let prompt_path = temp.path().join("system.txt");
        std::fs::write(&prompt_path, "\n\t  ").expect("write prompt");

        let mut cli = test_cli();
        cli.system_prompt_file = Some(prompt_path.clone());

        let error = resolve_system_prompt(&cli).expect_err("empty system prompt should fail");
        assert!(error.to_string().contains(&format!(
            "system prompt file {} is empty",
            prompt_path.display()
        )));
    }

    #[test]
    fn pathbuf_from_cli_default_is_relative() {
        let path = PathBuf::from(".pi/sessions/default.jsonl");
        assert!(!path.is_absolute());
    }

    #[test]
    fn unit_parse_trusted_root_spec_accepts_key_id_and_base64() {
        let parsed = parse_trusted_root_spec("root=ZmFrZS1rZXk=").expect("parse root");
        assert_eq!(parsed.id, "root");
        assert_eq!(parsed.public_key, "ZmFrZS1rZXk=");
    }

    #[test]
    fn regression_parse_trusted_root_spec_rejects_invalid_shapes() {
        let error = parse_trusted_root_spec("missing-separator").expect_err("should fail");
        assert!(error.to_string().contains("expected key_id=base64_key"));
    }

    #[test]
    fn unit_parse_trust_rotation_spec_accepts_old_and_new_key() {
        let (old_id, new_key) =
            parse_trust_rotation_spec("old:new=YQ==").expect("rotation spec parse");
        assert_eq!(old_id, "old");
        assert_eq!(new_key.id, "new");
        assert_eq!(new_key.public_key, "YQ==");
    }

    #[test]
    fn regression_parse_trust_rotation_spec_rejects_invalid_shapes() {
        let error = parse_trust_rotation_spec("invalid-shape").expect_err("should fail");
        assert!(error
            .to_string()
            .contains("expected old_id:new_id=base64_key"));
    }

    #[test]
    fn functional_apply_trust_root_mutations_add_revoke_and_rotate() {
        let mut records = vec![TrustedRootRecord {
            id: "old".to_string(),
            public_key: "YQ==".to_string(),
            revoked: false,
            expires_unix: None,
            rotated_from: None,
        }];
        let mut cli = test_cli();
        cli.skill_trust_add = vec!["extra=Yg==".to_string()];
        cli.skill_trust_revoke = vec!["extra".to_string()];
        cli.skill_trust_rotate = vec!["old:new=Yw==".to_string()];

        let report = apply_trust_root_mutations(&mut records, &cli).expect("mutate");
        assert_eq!(report.added, 2);
        assert_eq!(report.updated, 0);
        assert_eq!(report.revoked, 1);
        assert_eq!(report.rotated, 1);

        let old = records
            .iter()
            .find(|record| record.id == "old")
            .expect("old");
        let new = records
            .iter()
            .find(|record| record.id == "new")
            .expect("new");
        let extra = records
            .iter()
            .find(|record| record.id == "extra")
            .expect("extra");
        assert!(old.revoked);
        assert_eq!(new.rotated_from.as_deref(), Some("old"));
        assert!(extra.revoked);
    }

    #[test]
    fn functional_resolve_skill_trust_roots_loads_inline_and_file_entries() {
        let temp = tempdir().expect("tempdir");
        let roots_file = temp.path().join("roots.json");
        std::fs::write(
            &roots_file,
            r#"{"roots":[{"id":"file-root","public_key":"YQ=="}]}"#,
        )
        .expect("write roots");

        let mut cli = test_cli();
        cli.skill_trust_root = vec!["inline-root=Yg==".to_string()];
        cli.skill_trust_root_file = Some(roots_file);

        let roots = resolve_skill_trust_roots(&cli).expect("resolve roots");
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].id, "inline-root");
        assert_eq!(roots[1].id, "file-root");
    }

    #[test]
    fn integration_resolve_skill_trust_roots_applies_mutations_and_persists_file() {
        let temp = tempdir().expect("tempdir");
        let roots_file = temp.path().join("roots.json");
        std::fs::write(
            &roots_file,
            r#"{"roots":[{"id":"old","public_key":"YQ=="}]}"#,
        )
        .expect("write roots");

        let mut cli = test_cli();
        cli.skill_trust_root_file = Some(roots_file.clone());
        cli.skill_trust_rotate = vec!["old:new=Yg==".to_string()];

        let roots = resolve_skill_trust_roots(&cli).expect("resolve roots");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, "new");

        let raw = std::fs::read_to_string(&roots_file).expect("read persisted");
        assert!(raw.contains("\"id\": \"old\""));
        assert!(raw.contains("\"revoked\": true"));
        assert!(raw.contains("\"id\": \"new\""));
    }

    #[test]
    fn regression_resolve_skill_trust_roots_requires_file_for_mutations() {
        let mut cli = test_cli();
        cli.skill_trust_add = vec!["root=YQ==".to_string()];
        let error = resolve_skill_trust_roots(&cli).expect_err("should fail");
        assert!(error
            .to_string()
            .contains("--skill-trust-root-file is required"));
    }

    #[test]
    fn unit_stream_text_chunks_preserve_whitespace_boundaries() {
        let chunks = stream_text_chunks("hello world\nnext");
        assert_eq!(chunks, vec!["hello ", "world\n", "next"]);
    }

    #[test]
    fn regression_stream_text_chunks_handles_empty_and_single_word() {
        assert!(stream_text_chunks("").is_empty());
        assert_eq!(stream_text_chunks("token"), vec!["token"]);
    }

    #[test]
    fn unit_tool_audit_event_json_for_start_has_expected_shape() {
        let mut starts = HashMap::new();
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "bash".to_string(),
            arguments: serde_json::json!({ "command": "pwd" }),
        };
        let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

        assert_eq!(payload["event"], "tool_execution_start");
        assert_eq!(payload["tool_call_id"], "call-1");
        assert_eq!(payload["tool_name"], "bash");
        assert!(payload["arguments_bytes"].as_u64().unwrap_or(0) > 0);
        assert!(starts.contains_key("call-1"));
    }

    #[test]
    fn unit_tool_audit_event_json_for_end_tracks_duration_and_error_state() {
        let mut starts = HashMap::new();
        starts.insert("call-2".to_string(), Instant::now());
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-2".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::error(serde_json::json!({ "error": "denied" })),
        };
        let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

        assert_eq!(payload["event"], "tool_execution_end");
        assert_eq!(payload["tool_call_id"], "call-2");
        assert_eq!(payload["is_error"], true);
        assert!(payload["result_bytes"].as_u64().unwrap_or(0) > 0);
        assert!(payload["duration_ms"].is_number() || payload["duration_ms"].is_null());
        assert!(!starts.contains_key("call-2"));
    }

    #[test]
    fn integration_tool_audit_logger_persists_jsonl_records() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("tool-audit.jsonl");
        let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");

        let start = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-3".to_string(),
            tool_name: "write".to_string(),
            arguments: serde_json::json!({ "path": "out.txt", "content": "x" }),
        };
        logger.log_event(&start).expect("write start event");

        let end = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-3".to_string(),
            tool_name: "write".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({ "bytes_written": 1 })),
        };
        logger.log_event(&end).expect("write end event");

        let raw = std::fs::read_to_string(log_path).expect("read audit log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse first");
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("parse second");
        assert_eq!(first["event"], "tool_execution_start");
        assert_eq!(second["event"], "tool_execution_end");
        assert_eq!(second["is_error"], false);
    }

    #[tokio::test]
    async fn integration_run_prompt_with_cancellation_completes_when_not_cancelled() {
        let mut agent = Agent::new(Arc::new(SuccessClient), AgentConfig::default());
        let mut runtime = None;

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut runtime,
            "hello",
            0,
            pending::<()>(),
            test_render_options(),
        )
        .await
        .expect("prompt should complete");

        assert_eq!(status, PromptRunStatus::Completed);
        assert_eq!(agent.messages().len(), 3);
        assert_eq!(agent.messages()[1].role, MessageRole::User);
        assert_eq!(agent.messages()[2].role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn regression_run_prompt_with_cancellation_restores_agent_state() {
        let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());
        let initial_messages = agent.messages().to_vec();
        let mut runtime = None;

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut runtime,
            "cancel me",
            0,
            ready(()),
            test_render_options(),
        )
        .await
        .expect("cancellation branch should succeed");

        assert_eq!(status, PromptRunStatus::Cancelled);
        assert_eq!(agent.messages().len(), initial_messages.len());
        assert_eq!(agent.messages()[0].role, initial_messages[0].role);
        assert_eq!(
            agent.messages()[0].text_content(),
            initial_messages[0].text_content()
        );
    }

    #[tokio::test]
    async fn functional_run_prompt_with_timeout_restores_agent_state() {
        let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());
        let initial_messages = agent.messages().to_vec();
        let mut runtime = None;

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut runtime,
            "timeout me",
            20,
            pending::<()>(),
            test_render_options(),
        )
        .await
        .expect("timeout branch should succeed");

        assert_eq!(status, PromptRunStatus::TimedOut);
        assert_eq!(agent.messages().len(), initial_messages.len());
        assert_eq!(
            agent.messages()[0].text_content(),
            initial_messages[0].text_content()
        );
    }

    #[tokio::test]
    async fn integration_regression_cancellation_does_not_persist_partial_session_entries() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("cancel-session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let active_head = store
            .ensure_initialized("You are a helpful coding assistant.")
            .expect("initialize session");

        let mut runtime = Some(SessionRuntime { store, active_head });
        let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut runtime,
            "cancel me",
            0,
            ready(()),
            test_render_options(),
        )
        .await
        .expect("cancelled prompt should succeed");

        assert_eq!(status, PromptRunStatus::Cancelled);
        assert_eq!(runtime.as_ref().expect("runtime").store.entries().len(), 1);

        let reloaded = SessionStore::load(&path).expect("reload");
        assert_eq!(reloaded.entries().len(), 1);
    }

    #[tokio::test]
    async fn integration_regression_timeout_does_not_persist_partial_session_entries() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("timeout-session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let active_head = store
            .ensure_initialized("You are a helpful coding assistant.")
            .expect("initialize session");

        let mut runtime = Some(SessionRuntime { store, active_head });
        let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());

        let status = run_prompt_with_cancellation(
            &mut agent,
            &mut runtime,
            "timeout me",
            20,
            pending::<()>(),
            test_render_options(),
        )
        .await
        .expect("timed-out prompt should succeed");

        assert_eq!(status, PromptRunStatus::TimedOut);
        assert_eq!(runtime.as_ref().expect("runtime").store.entries().len(), 1);

        let reloaded = SessionStore::load(&path).expect("reload");
        assert_eq!(reloaded.entries().len(), 1);
    }

    #[tokio::test]
    async fn integration_agent_bash_policy_blocks_overlong_commands() {
        let temp = tempdir().expect("tempdir");
        let responses = VecDeque::from(vec![
            ChatResponse {
                message: pi_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({
                        "command": "printf",
                        "cwd": temp.path().display().to_string(),
                    }),
                }]),
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: pi_ai::Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ]);

        let client = Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        });
        let mut agent = Agent::new(client, AgentConfig::default());

        let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.max_command_length = 4;
        crate::tools::register_builtin_tools(&mut agent, policy);

        let new_messages = agent
            .prompt("run command")
            .await
            .expect("prompt should succeed");
        let tool_message = new_messages
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("tool result should be present");

        assert!(tool_message.is_error);
        assert!(tool_message.text_content().contains("command is too long"));
    }

    #[tokio::test]
    async fn integration_agent_write_policy_blocks_oversized_content() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("target.txt");
        let responses = VecDeque::from(vec![
            ChatResponse {
                message: pi_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                    id: "call-1".to_string(),
                    name: "write".to_string(),
                    arguments: serde_json::json!({
                        "path": target,
                        "content": "hello",
                    }),
                }]),
                finish_reason: Some("tool_calls".to_string()),
                usage: ChatUsage::default(),
            },
            ChatResponse {
                message: pi_ai::Message::assistant_text("done"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            },
        ]);

        let client = Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        });
        let mut agent = Agent::new(client, AgentConfig::default());

        let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.max_file_write_bytes = 4;
        crate::tools::register_builtin_tools(&mut agent, policy);

        let new_messages = agent
            .prompt("write file")
            .await
            .expect("prompt should succeed");
        let tool_message = new_messages
            .iter()
            .find(|message| message.role == MessageRole::Tool)
            .expect("tool result should be present");

        assert!(tool_message.is_error);
        assert!(tool_message.text_content().contains("content is too large"));
    }

    #[test]
    fn branch_and_resume_commands_reload_agent_messages() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("q1"),
                    pi_ai::Message::assistant_text("a1"),
                    pi_ai::Message::user("q2"),
                    pi_ai::Message::assistant_text("a2"),
                ],
            )
            .expect("append")
            .expect("head id");

        let branch_target = head - 2;

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store
            .lineage_messages(Some(head))
            .expect("lineage should resolve");
        agent.replace_messages(lineage);

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command(
            &format!("  /branch    {branch_target}   "),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect("branch command should succeed");
        assert_eq!(action, CommandAction::Continue);
        assert_eq!(
            runtime.as_ref().and_then(|runtime| runtime.active_head),
            Some(branch_target)
        );
        assert_eq!(agent.messages().len(), 3);

        let action = handle_command("/resume", &mut agent, &mut runtime, &tool_policy_json)
            .expect("resume command should succeed");
        assert_eq!(action, CommandAction::Continue);
        assert_eq!(
            runtime.as_ref().and_then(|runtime| runtime.active_head),
            Some(head)
        );
        assert_eq!(agent.messages().len(), 5);
    }

    #[test]
    fn exit_commands_return_exit_action() {
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut runtime = None;
        let tool_policy_json = test_tool_policy_json();

        assert_eq!(
            handle_command("/quit", &mut agent, &mut runtime, &tool_policy_json)
                .expect("quit should succeed"),
            CommandAction::Exit
        );
        assert_eq!(
            handle_command("/exit", &mut agent, &mut runtime, &tool_policy_json)
                .expect("exit should succeed"),
            CommandAction::Exit
        );
    }

    #[test]
    fn policy_command_returns_continue_action() {
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut runtime = None;
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command("/policy", &mut agent, &mut runtime, &tool_policy_json)
            .expect("policy should succeed");
        assert_eq!(action, CommandAction::Continue);
    }

    #[test]
    fn functional_session_export_command_writes_active_lineage_snapshot() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let export_path = temp.path().join("snapshot.jsonl");

        let mut store = SessionStore::load(&session_path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        let head = store
            .append_messages(
                head,
                &[
                    pi_ai::Message::user("q1"),
                    pi_ai::Message::assistant_text("a1"),
                ],
            )
            .expect("append")
            .expect("head");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command(
            &format!("/session-export {}", export_path.display()),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect("session export should succeed");
        assert_eq!(action, CommandAction::Continue);

        let exported = SessionStore::load(&export_path).expect("load exported");
        assert_eq!(exported.entries().len(), 3);
        assert_eq!(exported.entries()[0].message.text_content(), "sys");
        assert_eq!(exported.entries()[1].message.text_content(), "q1");
        assert_eq!(exported.entries()[2].message.text_content(), "a1");
    }

    #[test]
    fn functional_session_import_command_merges_snapshot_and_updates_active_head() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let import_path = temp.path().join("import.jsonl");

        let mut target_store = SessionStore::load(&session_path).expect("load target");
        let target_head = target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target root")
            .expect("target head");
        target_store
            .append_messages(Some(target_head), &[pi_ai::Message::user("target-user")])
            .expect("append target user");

        let mut import_store = SessionStore::load(&import_path).expect("load import");
        let import_head = import_store
            .append_messages(None, &[pi_ai::Message::system("import-root")])
            .expect("append import root");
        import_store
            .append_messages(import_head, &[pi_ai::Message::user("import-user")])
            .expect("append import user");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let target_lineage = target_store
            .lineage_messages(target_store.head_id())
            .expect("target lineage");
        agent.replace_messages(target_lineage);

        let mut runtime = Some(SessionRuntime {
            store: target_store,
            active_head: Some(2),
        });
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command(
            &format!("/session-import {}", import_path.display()),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect("session import should succeed");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.store.entries().len(), 4);
        assert_eq!(runtime.active_head, Some(4));
        assert_eq!(runtime.store.entries()[2].id, 3);
        assert_eq!(runtime.store.entries()[2].parent_id, None);
        assert_eq!(runtime.store.entries()[3].id, 4);
        assert_eq!(runtime.store.entries()[3].parent_id, Some(3));
        assert_eq!(agent.messages().len(), 2);
        assert_eq!(agent.messages()[0].text_content(), "import-root");
        assert_eq!(agent.messages()[1].text_content(), "import-user");
    }

    #[test]
    fn integration_session_import_command_replace_mode_overwrites_runtime_state() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session-replace.jsonl");
        let import_path = temp.path().join("import-replace.jsonl");

        let mut target_store = SessionStore::load(&session_path).expect("load target");
        let head = target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target root");
        target_store
            .append_messages(head, &[pi_ai::Message::user("target-user")])
            .expect("append target user");

        let import_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":10,"parent_id":null,"message":pi_ai::Message::system("import-root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":11,"parent_id":10,"message":pi_ai::Message::assistant_text("import-assistant")}).to_string(),
        ]
        .join("\n");
        std::fs::write(&import_path, format!("{import_raw}\n")).expect("write import snapshot");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let target_lineage = target_store
            .lineage_messages(target_store.head_id())
            .expect("target lineage");
        agent.replace_messages(target_lineage);

        let mut runtime = Some(SessionRuntime {
            store: target_store,
            active_head: Some(2),
        });
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command_with_session_import_mode(
            &format!("/session-import {}", import_path.display()),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Replace,
        )
        .expect("session replace import should succeed");
        assert_eq!(action, CommandAction::Continue);

        let mut runtime = runtime.expect("runtime");
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(runtime.store.entries()[0].id, 10);
        assert_eq!(runtime.store.entries()[1].id, 11);
        assert_eq!(runtime.active_head, Some(11));
        assert_eq!(agent.messages().len(), 2);
        assert_eq!(agent.messages()[0].text_content(), "import-root");
        assert_eq!(agent.messages()[1].text_content(), "import-assistant");

        let next = runtime
            .store
            .append_messages(
                runtime.active_head,
                &[pi_ai::Message::user("after-replace")],
            )
            .expect("append after replace");
        assert_eq!(next, Some(12));
    }

    #[test]
    fn regression_session_import_command_rejects_invalid_snapshot() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session-invalid.jsonl");
        let import_path = temp.path().join("import-invalid.jsonl");

        let mut target_store = SessionStore::load(&session_path).expect("load target");
        target_store
            .append_messages(None, &[pi_ai::Message::system("target-root")])
            .expect("append target");
        let import_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":pi_ai::Message::system("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
        std::fs::write(&import_path, format!("{import_raw}\n")).expect("write invalid import");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let target_lineage = target_store
            .lineage_messages(target_store.head_id())
            .expect("target lineage");
        agent.replace_messages(target_lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store: target_store,
            active_head: Some(1),
        });
        let tool_policy_json = test_tool_policy_json();

        let error = handle_command(
            &format!("/session-import {}", import_path.display()),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect_err("invalid import should fail");
        assert!(error
            .to_string()
            .contains("import session validation failed"));

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.store.entries().len(), 1);
        assert_eq!(runtime.active_head, Some(1));
        assert_eq!(agent.messages().len(), target_lineage.len());
        assert_eq!(agent.messages()[0].text_content(), "target-root");
    }

    #[test]
    fn functional_validate_session_file_succeeds_for_valid_session() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");

        let mut store = SessionStore::load(&session_path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        store
            .append_messages(head, &[pi_ai::Message::user("hello")])
            .expect("append");

        let mut cli = test_cli();
        cli.session = session_path;
        cli.session_validate = true;

        validate_session_file(&cli).expect("session validation should pass");
    }

    #[test]
    fn regression_validate_session_file_fails_for_invalid_session_graph() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");

        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":pi_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":pi_ai::Message::user("cycle")}).to_string(),
        ]
        .join("\n");
        std::fs::write(&session_path, format!("{raw}\n")).expect("write invalid session");

        let mut cli = test_cli();
        cli.session = session_path;
        cli.session_validate = true;

        let error =
            validate_session_file(&cli).expect_err("session validation should fail for cycle");
        assert!(error.to_string().contains("session validation failed"));
        assert!(error.to_string().contains("cycles=2"));
    }

    #[test]
    fn regression_validate_session_file_rejects_no_session_flag() {
        let mut cli = test_cli();
        cli.no_session = true;
        cli.session_validate = true;

        let error = validate_session_file(&cli)
            .expect_err("validation with no-session flag should fail fast");
        assert!(error
            .to_string()
            .contains("--session-validate cannot be used together with --no-session"));
    }

    #[test]
    fn session_repair_command_runs_successfully() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&path).expect("load");
        let head = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append");
        store
            .append_messages(head, &[pi_ai::Message::user("hello")])
            .expect("append");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store
            .lineage_messages(store.head_id())
            .expect("lineage should resolve");
        agent.replace_messages(lineage);

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(2),
        });
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command(
            "/session-repair",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect("repair command should succeed");
        assert_eq!(action, CommandAction::Continue);
        assert_eq!(agent.messages().len(), 2);
    }

    #[test]
    fn session_compact_command_prunes_inactive_branch() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("session-compact.jsonl");

        let mut store = SessionStore::load(&path).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append")
            .expect("root");
        let head = store
            .append_messages(
                Some(root),
                &[
                    pi_ai::Message::user("main-q"),
                    pi_ai::Message::assistant_text("main-a"),
                ],
            )
            .expect("append")
            .expect("main head");
        store
            .append_messages(Some(root), &[pi_ai::Message::user("branch-q")])
            .expect("append branch");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store
            .lineage_messages(Some(head))
            .expect("lineage should resolve");
        agent.replace_messages(lineage);

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command(
            "/session-compact",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect("compact command should succeed");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.store.entries().len(), 3);
        assert_eq!(runtime.store.branch_tips().len(), 1);
        assert_eq!(runtime.store.branch_tips()[0].id, head);
        assert_eq!(agent.messages().len(), 3);
    }

    #[test]
    fn integration_initialize_session_applies_lock_timeout_policy() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("locked-session.jsonl");
        let lock_path = session_path.with_extension("lock");
        std::fs::write(&lock_path, "locked").expect("write lock");

        let mut cli = test_cli();
        cli.session = session_path;
        cli.session_lock_wait_ms = 120;
        cli.session_lock_stale_ms = 0;
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let start = Instant::now();

        let error = initialize_session(&mut agent, &cli, "sys")
            .expect_err("initialization should fail when lock persists");
        assert!(error.to_string().contains("timed out acquiring lock"));
        assert!(start.elapsed() < Duration::from_secs(2));

        std::fs::remove_file(lock_path).expect("cleanup lock");
    }

    #[test]
    fn functional_initialize_session_reclaims_stale_lock_when_enabled() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("stale-lock-session.jsonl");
        let lock_path = session_path.with_extension("lock");
        std::fs::write(&lock_path, "stale").expect("write lock");
        std::thread::sleep(Duration::from_millis(30));

        let mut cli = test_cli();
        cli.session = session_path;
        cli.session_lock_wait_ms = 1_000;
        cli.session_lock_stale_ms = 10;
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

        let runtime = initialize_session(&mut agent, &cli, "sys")
            .expect("initialization should reclaim stale lock");
        assert_eq!(runtime.store.entries().len(), 1);
        assert!(!lock_path.exists());
    }

    #[test]
    fn unit_parse_sandbox_command_tokens_supports_shell_words_and_placeholders() {
        let tokens = parse_sandbox_command_tokens(&[
            "bwrap".to_string(),
            "--bind".to_string(),
            "\"{cwd}\"".to_string(),
            "{cwd}".to_string(),
            "{shell}".to_string(),
            "{command}".to_string(),
        ])
        .expect("parse should succeed");

        assert_eq!(
            tokens,
            vec![
                "bwrap".to_string(),
                "--bind".to_string(),
                "{cwd}".to_string(),
                "{cwd}".to_string(),
                "{shell}".to_string(),
                "{command}".to_string(),
            ]
        );
    }

    #[test]
    fn regression_parse_sandbox_command_tokens_rejects_invalid_quotes() {
        let error = parse_sandbox_command_tokens(&["\"unterminated".to_string()])
            .expect_err("parse should fail");
        assert!(error
            .to_string()
            .contains("invalid --os-sandbox-command token"));
    }

    #[test]
    fn build_tool_policy_includes_cwd_and_custom_root() {
        let mut cli = test_cli();
        cli.allow_path = vec![PathBuf::from("/tmp")];

        let policy = build_tool_policy(&cli).expect("policy should build");
        assert!(policy.allowed_roots.len() >= 2);
        assert_eq!(policy.bash_timeout_ms, 500);
        assert_eq!(policy.max_command_output_bytes, 1024);
        assert_eq!(policy.max_file_read_bytes, 2048);
        assert_eq!(policy.max_file_write_bytes, 2048);
        assert_eq!(policy.max_command_length, 4096);
        assert!(policy.allow_command_newlines);
        assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Off);
        assert!(policy.os_sandbox_command.is_empty());
        assert!(policy.enforce_regular_files);
    }

    #[test]
    fn unit_tool_policy_to_json_includes_key_limits_and_modes() {
        let mut cli = test_cli();
        cli.bash_profile = CliBashProfile::Strict;
        cli.os_sandbox_mode = CliOsSandboxMode::Auto;
        cli.max_file_write_bytes = 4096;

        let policy = build_tool_policy(&cli).expect("policy should build");
        let payload = tool_policy_to_json(&policy);
        assert_eq!(payload["bash_profile"], "strict");
        assert_eq!(payload["os_sandbox_mode"], "auto");
        assert_eq!(payload["max_file_write_bytes"], 4096);
        assert_eq!(payload["enforce_regular_files"], true);
    }

    #[test]
    fn functional_build_tool_policy_applies_strict_profile_and_custom_allowlist() {
        let mut cli = test_cli();
        cli.bash_profile = CliBashProfile::Strict;
        cli.allow_command = vec!["python".to_string(), "cargo-nextest*".to_string()];

        let policy = build_tool_policy(&cli).expect("policy should build");
        assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
        assert!(policy.allowed_commands.contains(&"python".to_string()));
        assert!(policy
            .allowed_commands
            .contains(&"cargo-nextest*".to_string()));
        assert!(!policy.allowed_commands.contains(&"rm".to_string()));
    }

    #[test]
    fn regression_build_tool_policy_permissive_profile_disables_allowlist() {
        let mut cli = test_cli();
        cli.bash_profile = CliBashProfile::Permissive;
        let policy = build_tool_policy(&cli).expect("policy should build");
        assert!(policy.allowed_commands.is_empty());
    }

    #[test]
    fn functional_build_tool_policy_applies_sandbox_and_regular_file_settings() {
        let mut cli = test_cli();
        cli.os_sandbox_mode = CliOsSandboxMode::Auto;
        cli.os_sandbox_command = vec![
            "sandbox-run".to_string(),
            "--cwd".to_string(),
            "{cwd}".to_string(),
        ];
        cli.max_file_write_bytes = 4096;
        cli.enforce_regular_files = false;

        let policy = build_tool_policy(&cli).expect("policy should build");
        assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Auto);
        assert_eq!(
            policy.os_sandbox_command,
            vec![
                "sandbox-run".to_string(),
                "--cwd".to_string(),
                "{cwd}".to_string()
            ]
        );
        assert_eq!(policy.max_file_write_bytes, 4096);
        assert!(!policy.enforce_regular_files);
    }
}
