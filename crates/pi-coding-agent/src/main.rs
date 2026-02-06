mod session;
mod skills;
mod tools;

use std::{future::Future, io::Write, path::PathBuf, sync::Arc, time::Duration};

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

use crate::session::SessionStore;
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
        env = "PI_SESSION",
        default_value = ".pi/sessions/default.jsonl",
        help = "Session JSONL file"
    )]
    session: PathBuf,

    #[arg(long, help = "Disable session persistence")]
    no_session: bool,

    #[arg(long, help = "Start from a specific session entry id")]
    branch_from: Option<u64>,

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

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

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
    let catalog = load_catalog(&cli.skills_dir)
        .with_context(|| format!("failed to load skills from {}", cli.skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    let system_prompt = augment_system_prompt(&cli.system_prompt, &selected_skills);

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
    tools::register_builtin_tools(&mut agent, tool_policy);
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

    if let Some(prompt) = cli.prompt {
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

    run_interactive(agent, session_runtime, cli.turn_timeout_ms, render_options).await
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

fn initialize_session(agent: &mut Agent, cli: &Cli, system_prompt: &str) -> Result<SessionRuntime> {
    let mut store = SessionStore::load(&cli.session)?;

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
            if handle_command(trimmed, &mut agent, &mut session_runtime)? == CommandAction::Exit {
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

fn handle_command(
    command: &str,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
) -> Result<CommandAction> {
    if matches!(command, "/exit" | "/quit") {
        return Ok(CommandAction::Exit);
    }

    if command == "/session" {
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

    if command == "/resume" {
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

    if command == "/branches" {
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

    if command == "/session-repair" {
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
            "repair complete: removed_duplicates={} removed_invalid_parent={} removed_cycles={}",
            report.removed_duplicates, report.removed_invalid_parent, report.removed_cycles
        );
        return Ok(CommandAction::Continue);
    }

    if command == "/session-compact" {
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

    if let Some(rest) = command.strip_prefix("/branch ") {
        let Some(runtime) = session_runtime.as_mut() else {
            println!("session is disabled");
            return Ok(CommandAction::Continue);
        };

        let target = rest
            .trim()
            .parse::<u64>()
            .map_err(|_| anyhow!("invalid branch id '{}'; expected an integer", rest.trim()))?;

        if !runtime.store.contains(target) {
            bail!("unknown session id {}", target);
        }

        runtime.active_head = Some(target);
        reload_agent_from_active_head(agent, runtime)?;
        println!("switched to branch id {target}");
        return Ok(CommandAction::Continue);
    }

    println!(
        "unknown command: {}\ncommands: /session, /session-repair, /session-compact, /branches, /branch <id>, /resume, /quit",
        command
    );
    Ok(CommandAction::Continue)
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
        collections::VecDeque,
        future::{pending, ready},
        path::PathBuf,
        sync::Arc,
        time::Duration,
    };

    use async_trait::async_trait;
    use pi_agent_core::{Agent, AgentConfig};
    use pi_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, MessageRole, PiAiError,
    };
    use tempfile::tempdir;
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::time::sleep;

    use super::{
        apply_trust_root_mutations, build_tool_policy, handle_command,
        parse_sandbox_command_tokens, parse_trust_rotation_spec, parse_trusted_root_spec,
        resolve_skill_trust_roots, run_prompt_with_cancellation, stream_text_chunks, Cli,
        CliBashProfile, CliOsSandboxMode, CommandAction, PromptRunStatus, RenderOptions,
        SessionRuntime, TrustedRootRecord,
    };
    use crate::resolve_api_key;
    use crate::session::SessionStore;
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
            session: PathBuf::from(".pi/sessions/default.jsonl"),
            no_session: false,
            branch_from: None,
            allow_path: vec![],
            bash_timeout_ms: 500,
            max_tool_output_bytes: 1024,
            max_file_read_bytes: 2048,
            max_command_length: 4096,
            allow_command_newlines: true,
            bash_profile: CliBashProfile::Balanced,
            allow_command: vec![],
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

        let action = handle_command(
            &format!("/branch {branch_target}"),
            &mut agent,
            &mut runtime,
        )
        .expect("branch command should succeed");
        assert_eq!(action, CommandAction::Continue);
        assert_eq!(
            runtime.as_ref().and_then(|runtime| runtime.active_head),
            Some(branch_target)
        );
        assert_eq!(agent.messages().len(), 3);

        let action = handle_command("/resume", &mut agent, &mut runtime)
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

        assert_eq!(
            handle_command("/quit", &mut agent, &mut runtime).expect("quit should succeed"),
            CommandAction::Exit
        );
        assert_eq!(
            handle_command("/exit", &mut agent, &mut runtime).expect("exit should succeed"),
            CommandAction::Exit
        );
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

        let action = handle_command("/session-repair", &mut agent, &mut runtime)
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

        let action = handle_command("/session-compact", &mut agent, &mut runtime)
            .expect("compact command should succeed");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.store.entries().len(), 3);
        assert_eq!(runtime.store.branch_tips().len(), 1);
        assert_eq!(runtime.store.branch_tips()[0].id, head);
        assert_eq!(agent.messages().len(), 3);
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
        assert_eq!(policy.max_command_length, 4096);
        assert!(policy.allow_command_newlines);
        assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Off);
        assert!(policy.os_sandbox_command.is_empty());
        assert!(policy.enforce_regular_files);
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
        assert!(!policy.enforce_regular_files);
    }
}
