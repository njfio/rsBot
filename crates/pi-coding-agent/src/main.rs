mod channel_store;
mod commands;
mod credentials;
mod events;
mod github_issues;
mod session;
mod skills;
mod slack;
mod tools;
#[cfg(test)]
mod transport_conformance;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    future::Future,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use clap::{ArgAction, Parser, ValueEnum};
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, ChatResponse, GoogleClient, GoogleConfig,
    LlmClient, Message, MessageRole, ModelRef, OpenAiClient, OpenAiConfig, PiAiError, Provider,
    StreamDeltaHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::channel_store::ChannelStore;
#[cfg(test)]
pub(crate) use crate::commands::handle_command;
pub(crate) use crate::commands::{
    canonical_command_name, execute_command_file, handle_command_with_session_import_mode,
    parse_command, CommandAction, COMMAND_NAMES, COMMAND_SPECS,
};
#[cfg(test)]
pub(crate) use crate::commands::{
    parse_command_file, CommandFileEntry, CommandFileReport,
};
#[cfg(test)]
pub(crate) use crate::credentials::{
    decrypt_credential_store_secret, encrypt_credential_store_secret,
    parse_integration_auth_command, IntegrationAuthCommand, IntegrationCredentialStoreRecord,
};
pub(crate) use crate::credentials::{
    execute_integration_auth_command, load_credential_store, reauth_required_error,
    refresh_provider_access_token, resolve_credential_store_encryption_mode,
    resolve_non_empty_cli_value, resolve_secret_from_cli_or_store_id, save_credential_store,
    CredentialStoreData, ProviderCredentialStoreRecord,
};
use crate::events::{
    ingest_webhook_immediate_event, run_event_scheduler, EventSchedulerConfig,
    EventWebhookIngestConfig, WebhookSignatureAlgorithm,
};
use crate::session::{SessionImportMode, SessionStore};
use crate::skills::{
    augment_system_prompt, build_local_skill_lock_hints, build_registry_skill_lock_hints,
    build_remote_skill_lock_hints, default_skills_cache_dir, default_skills_lock_path,
    fetch_registry_manifest_with_cache, install_remote_skills_with_cache, install_skills,
    load_catalog, load_skills_lockfile, resolve_registry_skill_sources,
    resolve_remote_skill_sources, resolve_selected_skills, sync_skills_with_lockfile,
    write_skills_lockfile, SkillsDownloadOptions, TrustedKey,
};
use crate::tools::{
    tool_policy_preset_name, BashCommandProfile, OsSandboxMode, ToolPolicy, ToolPolicyPreset,
};
use github_issues::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
use slack::{run_slack_bridge, SlackBridgeRuntimeConfig};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliCommandFileErrorMode {
    FailFast,
    ContinueOnError,
}

fn command_file_error_mode_label(mode: CliCommandFileErrorMode) -> &'static str {
    match mode {
        CliCommandFileErrorMode::FailFast => "fail-fast",
        CliCommandFileErrorMode::ContinueOnError => "continue-on-error",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliWebhookSignatureAlgorithm {
    GithubSha256,
    SlackV0,
}

impl From<CliWebhookSignatureAlgorithm> for WebhookSignatureAlgorithm {
    fn from(value: CliWebhookSignatureAlgorithm) -> Self {
        match value {
            CliWebhookSignatureAlgorithm::GithubSha256 => WebhookSignatureAlgorithm::GithubSha256,
            CliWebhookSignatureAlgorithm::SlackV0 => WebhookSignatureAlgorithm::SlackV0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliToolPolicyPreset {
    Permissive,
    Balanced,
    Strict,
    Hardened,
}

impl From<CliToolPolicyPreset> for ToolPolicyPreset {
    fn from(value: CliToolPolicyPreset) -> Self {
        match value {
            CliToolPolicyPreset::Permissive => ToolPolicyPreset::Permissive,
            CliToolPolicyPreset::Balanced => ToolPolicyPreset::Balanced,
            CliToolPolicyPreset::Strict => ToolPolicyPreset::Strict,
            CliToolPolicyPreset::Hardened => ToolPolicyPreset::Hardened,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliProviderAuthMode {
    ApiKey,
    OauthToken,
    Adc,
    SessionToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliCredentialStoreEncryptionMode {
    Auto,
    None,
    Keyed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CredentialStoreEncryptionMode {
    None,
    Keyed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ProviderAuthMethod {
    ApiKey,
    OauthToken,
    Adc,
    SessionToken,
}

impl ProviderAuthMethod {
    fn as_str(self) -> &'static str {
        match self {
            ProviderAuthMethod::ApiKey => "api_key",
            ProviderAuthMethod::OauthToken => "oauth_token",
            ProviderAuthMethod::Adc => "adc",
            ProviderAuthMethod::SessionToken => "session_token",
        }
    }
}

impl From<CliProviderAuthMode> for ProviderAuthMethod {
    fn from(value: CliProviderAuthMode) -> Self {
        match value {
            CliProviderAuthMode::ApiKey => ProviderAuthMethod::ApiKey,
            CliProviderAuthMode::OauthToken => ProviderAuthMethod::OauthToken,
            CliProviderAuthMode::Adc => ProviderAuthMethod::Adc,
            CliProviderAuthMode::SessionToken => ProviderAuthMethod::SessionToken,
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
        long = "fallback-model",
        env = "PI_FALLBACK_MODEL",
        value_delimiter = ',',
        help = "Optional fallback model chain in provider/model format. Triggered only on retriable provider failures."
    )]
    fallback_model: Vec<String>,

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
        long = "openai-auth-mode",
        env = "PI_OPENAI_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for OpenAI provider"
    )]
    openai_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "anthropic-auth-mode",
        env = "PI_ANTHROPIC_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Anthropic provider"
    )]
    anthropic_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "google-auth-mode",
        env = "PI_GOOGLE_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Google provider"
    )]
    google_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "credential-store",
        env = "PI_CREDENTIAL_STORE",
        default_value = ".pi/credentials.json",
        help = "Credential store file path for non-API-key provider auth modes"
    )]
    credential_store: PathBuf,

    #[arg(
        long = "credential-store-key",
        env = "PI_CREDENTIAL_STORE_KEY",
        hide_env_values = true,
        help = "Optional encryption key for credential store entries when keyed encryption is enabled"
    )]
    credential_store_key: Option<String>,

    #[arg(
        long = "credential-store-encryption",
        env = "PI_CREDENTIAL_STORE_ENCRYPTION",
        value_enum,
        default_value_t = CliCredentialStoreEncryptionMode::Auto,
        help = "Credential store encryption mode: auto, none, or keyed"
    )]
    credential_store_encryption: CliCredentialStoreEncryptionMode,

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
        long = "skills-cache-dir",
        env = "PI_SKILLS_CACHE_DIR",
        help = "Cache directory for downloaded registry manifests and remote skill artifacts (defaults to <skills-dir>/.cache)"
    )]
    skills_cache_dir: Option<PathBuf>,

    #[arg(
        long = "skills-offline",
        env = "PI_SKILLS_OFFLINE",
        default_value_t = false,
        help = "Disable network fetches for remote/registry skills and require cache hits"
    )]
    skills_offline: bool,

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

    #[arg(
        long = "skills-lock-file",
        env = "PI_SKILLS_LOCK_FILE",
        help = "Path to skills lockfile (defaults to <skills-dir>/skills.lock.json)"
    )]
    skills_lock_file: Option<PathBuf>,

    #[arg(
        long = "skills-lock-write",
        env = "PI_SKILLS_LOCK_WRITE",
        default_value_t = false,
        help = "Write/update skills lockfile from the current installed skills"
    )]
    skills_lock_write: bool,

    #[arg(
        long = "skills-sync",
        env = "PI_SKILLS_SYNC",
        default_value_t = false,
        help = "Verify installed skills match the lockfile and fail on drift"
    )]
    skills_sync: bool,

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
        env = "PI_PROVIDER_MAX_RETRIES",
        default_value_t = 2,
        help = "Maximum retry attempts for retryable provider HTTP failures"
    )]
    provider_max_retries: usize,

    #[arg(
        long,
        env = "PI_PROVIDER_RETRY_BUDGET_MS",
        default_value_t = 0,
        help = "Optional cumulative retry backoff budget in milliseconds (0 disables budget)"
    )]
    provider_retry_budget_ms: u64,

    #[arg(
        long,
        env = "PI_PROVIDER_RETRY_JITTER",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Enable bounded jitter for provider retry backoff delays"
    )]
    provider_retry_jitter: bool,

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
        env = "PI_COMMAND_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_file",
        help = "Execute slash commands from a UTF-8 file and exit"
    )]
    command_file: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_COMMAND_FILE_ERROR_MODE",
        value_enum,
        default_value = "fail-fast",
        requires = "command_file",
        help = "Behavior when command-file execution hits malformed or failing commands"
    )]
    command_file_error_mode: CliCommandFileErrorMode,

    #[arg(
        long = "channel-store-root",
        env = "PI_CHANNEL_STORE_ROOT",
        default_value = ".pi/channel-store",
        help = "Base directory for transport-agnostic ChannelStore data"
    )]
    channel_store_root: PathBuf,

    #[arg(
        long = "channel-store-inspect",
        env = "PI_CHANNEL_STORE_INSPECT",
        conflicts_with = "channel_store_repair",
        value_name = "transport/channel_id",
        help = "Inspect ChannelStore state for one channel and exit"
    )]
    channel_store_inspect: Option<String>,

    #[arg(
        long = "channel-store-repair",
        env = "PI_CHANNEL_STORE_REPAIR",
        conflicts_with = "channel_store_inspect",
        value_name = "transport/channel_id",
        help = "Repair malformed ChannelStore JSONL files for one channel and exit"
    )]
    channel_store_repair: Option<String>,

    #[arg(
        long = "events-runner",
        env = "PI_EVENTS_RUNNER",
        default_value_t = false,
        help = "Run filesystem-backed scheduled events worker"
    )]
    events_runner: bool,

    #[arg(
        long = "events-dir",
        env = "PI_EVENTS_DIR",
        default_value = ".pi/events",
        help = "Directory containing event definition JSON files"
    )]
    events_dir: PathBuf,

    #[arg(
        long = "events-state-path",
        env = "PI_EVENTS_STATE_PATH",
        default_value = ".pi/events/state.json",
        help = "Persistent scheduler state path for periodic/debounce tracking"
    )]
    events_state_path: PathBuf,

    #[arg(
        long = "events-poll-interval-ms",
        env = "PI_EVENTS_POLL_INTERVAL_MS",
        default_value_t = 1_000,
        requires = "events_runner",
        help = "Scheduler poll interval in milliseconds"
    )]
    events_poll_interval_ms: u64,

    #[arg(
        long = "events-queue-limit",
        env = "PI_EVENTS_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "events_runner",
        help = "Maximum due events executed per poll cycle"
    )]
    events_queue_limit: usize,

    #[arg(
        long = "events-stale-immediate-max-age-seconds",
        env = "PI_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS",
        default_value_t = 86_400,
        requires = "events_runner",
        help = "Maximum age for immediate events before they are skipped and removed (0 disables)"
    )]
    events_stale_immediate_max_age_seconds: u64,

    #[arg(
        long = "event-webhook-ingest-file",
        env = "PI_EVENT_WEBHOOK_INGEST_FILE",
        value_name = "PATH",
        conflicts_with = "events_runner",
        help = "One-shot webhook ingestion: read payload file, enqueue debounced immediate event, and exit"
    )]
    event_webhook_ingest_file: Option<PathBuf>,

    #[arg(
        long = "event-webhook-channel",
        env = "PI_EVENT_WEBHOOK_CHANNEL",
        requires = "event_webhook_ingest_file",
        value_name = "transport/channel_id",
        help = "Channel reference used for webhook-ingested immediate events"
    )]
    event_webhook_channel: Option<String>,

    #[arg(
        long = "event-webhook-prompt-prefix",
        env = "PI_EVENT_WEBHOOK_PROMPT_PREFIX",
        default_value = "Handle webhook-triggered event.",
        requires = "event_webhook_ingest_file",
        help = "Prompt prefix prepended before webhook payload content"
    )]
    event_webhook_prompt_prefix: String,

    #[arg(
        long = "event-webhook-debounce-key",
        env = "PI_EVENT_WEBHOOK_DEBOUNCE_KEY",
        requires = "event_webhook_ingest_file",
        help = "Optional debounce key shared across webhook ingestions"
    )]
    event_webhook_debounce_key: Option<String>,

    #[arg(
        long = "event-webhook-debounce-window-seconds",
        env = "PI_EVENT_WEBHOOK_DEBOUNCE_WINDOW_SECONDS",
        default_value_t = 60,
        requires = "event_webhook_ingest_file",
        help = "Debounce window in seconds for repeated webhook ingestions with same key"
    )]
    event_webhook_debounce_window_seconds: u64,

    #[arg(
        long = "event-webhook-signature",
        env = "PI_EVENT_WEBHOOK_SIGNATURE",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Raw webhook signature header value (for signed ingest verification)"
    )]
    event_webhook_signature: Option<String>,

    #[arg(
        long = "event-webhook-timestamp",
        env = "PI_EVENT_WEBHOOK_TIMESTAMP",
        requires = "event_webhook_ingest_file",
        help = "Webhook timestamp header value used by signature algorithms that require timestamp checks"
    )]
    event_webhook_timestamp: Option<String>,

    #[arg(
        long = "event-webhook-secret",
        env = "PI_EVENT_WEBHOOK_SECRET",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Shared secret used to verify signed webhook payloads"
    )]
    event_webhook_secret: Option<String>,

    #[arg(
        long = "event-webhook-secret-id",
        env = "PI_EVENT_WEBHOOK_SECRET_ID",
        requires = "event_webhook_ingest_file",
        help = "Credential-store integration id used to resolve webhook signing secret"
    )]
    event_webhook_secret_id: Option<String>,

    #[arg(
        long = "event-webhook-signature-algorithm",
        env = "PI_EVENT_WEBHOOK_SIGNATURE_ALGORITHM",
        value_enum,
        requires = "event_webhook_ingest_file",
        help = "Webhook signature algorithm (github-sha256, slack-v0)"
    )]
    event_webhook_signature_algorithm: Option<CliWebhookSignatureAlgorithm>,

    #[arg(
        long = "event-webhook-signature-max-skew-seconds",
        env = "PI_EVENT_WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS",
        default_value_t = 300,
        requires = "event_webhook_ingest_file",
        help = "Maximum allowed webhook timestamp skew in seconds (0 disables skew checks)"
    )]
    event_webhook_signature_max_skew_seconds: u64,

    #[arg(
        long = "github-issues-bridge",
        env = "PI_GITHUB_ISSUES_BRIDGE",
        default_value_t = false,
        help = "Run as a GitHub Issues conversational transport loop instead of interactive prompt mode"
    )]
    github_issues_bridge: bool,

    #[arg(
        long = "github-repo",
        env = "PI_GITHUB_REPO",
        requires = "github_issues_bridge",
        help = "GitHub repository in owner/repo format used by --github-issues-bridge"
    )]
    github_repo: Option<String>,

    #[arg(
        long = "github-token",
        env = "GITHUB_TOKEN",
        hide_env_values = true,
        requires = "github_issues_bridge",
        help = "GitHub token used for API access in --github-issues-bridge mode"
    )]
    github_token: Option<String>,

    #[arg(
        long = "github-token-id",
        env = "PI_GITHUB_TOKEN_ID",
        requires = "github_issues_bridge",
        help = "Credential-store integration id used to resolve GitHub bridge token"
    )]
    github_token_id: Option<String>,

    #[arg(
        long = "github-bot-login",
        env = "PI_GITHUB_BOT_LOGIN",
        requires = "github_issues_bridge",
        help = "Optional bot login used to ignore self-comments and identify already-replied events"
    )]
    github_bot_login: Option<String>,

    #[arg(
        long = "github-api-base",
        env = "PI_GITHUB_API_BASE",
        default_value = "https://api.github.com",
        requires = "github_issues_bridge",
        help = "GitHub API base URL"
    )]
    github_api_base: String,

    #[arg(
        long = "github-state-dir",
        env = "PI_GITHUB_STATE_DIR",
        default_value = ".pi/github-issues",
        requires = "github_issues_bridge",
        help = "Directory for github bridge state/session/event logs"
    )]
    github_state_dir: PathBuf,

    #[arg(
        long = "github-poll-interval-seconds",
        env = "PI_GITHUB_POLL_INTERVAL_SECONDS",
        default_value_t = 30,
        requires = "github_issues_bridge",
        help = "Polling interval in seconds for github bridge mode"
    )]
    github_poll_interval_seconds: u64,

    #[arg(
        long = "github-include-issue-body",
        env = "PI_GITHUB_INCLUDE_ISSUE_BODY",
        default_value_t = false,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Treat the issue description itself as an initial conversation event"
    )]
    github_include_issue_body: bool,

    #[arg(
        long = "github-include-edited-comments",
        env = "PI_GITHUB_INCLUDE_EDITED_COMMENTS",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Process edited issue comments as new events (deduped by comment id + updated timestamp)"
    )]
    github_include_edited_comments: bool,

    #[arg(
        long = "github-processed-event-cap",
        env = "PI_GITHUB_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "github_issues_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    github_processed_event_cap: usize,

    #[arg(
        long = "github-retry-max-attempts",
        env = "PI_GITHUB_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "github_issues_bridge",
        help = "Maximum attempts for retryable github api failures (429/5xx/transport)"
    )]
    github_retry_max_attempts: usize,

    #[arg(
        long = "github-retry-base-delay-ms",
        env = "PI_GITHUB_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "github_issues_bridge",
        help = "Base backoff delay in milliseconds for github api retries"
    )]
    github_retry_base_delay_ms: u64,

    #[arg(
        long = "slack-bridge",
        env = "PI_SLACK_BRIDGE",
        default_value_t = false,
        help = "Run as a Slack Socket Mode conversational transport loop instead of interactive prompt mode"
    )]
    slack_bridge: bool,

    #[arg(
        long = "slack-app-token",
        env = "PI_SLACK_APP_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack Socket Mode app token (xapp-...)"
    )]
    slack_app_token: Option<String>,

    #[arg(
        long = "slack-app-token-id",
        env = "PI_SLACK_APP_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack app token"
    )]
    slack_app_token_id: Option<String>,

    #[arg(
        long = "slack-bot-token",
        env = "PI_SLACK_BOT_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack bot token for Web API (xoxb-...)"
    )]
    slack_bot_token: Option<String>,

    #[arg(
        long = "slack-bot-token-id",
        env = "PI_SLACK_BOT_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack bot token"
    )]
    slack_bot_token_id: Option<String>,

    #[arg(
        long = "slack-bot-user-id",
        env = "PI_SLACK_BOT_USER_ID",
        requires = "slack_bridge",
        help = "Optional bot user id used to strip self-mentions and ignore self-authored events"
    )]
    slack_bot_user_id: Option<String>,

    #[arg(
        long = "slack-api-base",
        env = "PI_SLACK_API_BASE",
        default_value = "https://slack.com/api",
        requires = "slack_bridge",
        help = "Slack Web API base URL"
    )]
    slack_api_base: String,

    #[arg(
        long = "slack-state-dir",
        env = "PI_SLACK_STATE_DIR",
        default_value = ".pi/slack",
        requires = "slack_bridge",
        help = "Directory for slack bridge state/session/event logs"
    )]
    slack_state_dir: PathBuf,

    #[arg(
        long = "slack-thread-detail-output",
        env = "PI_SLACK_THREAD_DETAIL_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "slack_bridge",
        help = "When responses exceed threshold, keep summary in placeholder and post full response as a threaded detail message"
    )]
    slack_thread_detail_output: bool,

    #[arg(
        long = "slack-thread-detail-threshold-chars",
        env = "PI_SLACK_THREAD_DETAIL_THRESHOLD_CHARS",
        default_value_t = 1500,
        requires = "slack_bridge",
        help = "Character threshold used with --slack-thread-detail-output"
    )]
    slack_thread_detail_threshold_chars: usize,

    #[arg(
        long = "slack-processed-event-cap",
        env = "PI_SLACK_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "slack_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    slack_processed_event_cap: usize,

    #[arg(
        long = "slack-max-event-age-seconds",
        env = "PI_SLACK_MAX_EVENT_AGE_SECONDS",
        default_value_t = 7_200,
        requires = "slack_bridge",
        help = "Ignore inbound Slack events older than this many seconds (0 disables age checks)"
    )]
    slack_max_event_age_seconds: u64,

    #[arg(
        long = "slack-reconnect-delay-ms",
        env = "PI_SLACK_RECONNECT_DELAY_MS",
        default_value_t = 1_000,
        requires = "slack_bridge",
        help = "Delay before reconnecting after socket/session errors"
    )]
    slack_reconnect_delay_ms: u64,

    #[arg(
        long = "slack-retry-max-attempts",
        env = "PI_SLACK_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "slack_bridge",
        help = "Maximum attempts for retryable slack api failures (429/5xx/transport)"
    )]
    slack_retry_max_attempts: usize,

    #[arg(
        long = "slack-retry-base-delay-ms",
        env = "PI_SLACK_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "slack_bridge",
        help = "Base backoff delay in milliseconds for slack api retries"
    )]
    slack_retry_base_delay_ms: u64,

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
        long,
        env = "PI_TOOL_POLICY_PRESET",
        value_enum,
        default_value = "balanced",
        help = "Tool policy preset: permissive, balanced, strict, or hardened"
    )]
    tool_policy_preset: CliToolPolicyPreset,

    #[arg(
        long,
        env = "PI_BASH_DRY_RUN",
        default_value_t = false,
        help = "Validate bash commands against policy without executing them"
    )]
    bash_dry_run: bool,

    #[arg(
        long,
        env = "PI_TOOL_POLICY_TRACE",
        default_value_t = false,
        help = "Include policy evaluation trace details in bash tool results"
    )]
    tool_policy_trace: bool,

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
        env = "PI_TELEMETRY_LOG",
        help = "Optional JSONL file path for prompt-level telemetry summaries"
    )]
    telemetry_log: Option<PathBuf>,

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

#[derive(Debug, Clone)]
struct SkillsSyncCommandConfig {
    skills_dir: PathBuf,
    default_lock_path: PathBuf,
    default_trust_root_path: Option<PathBuf>,
    doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorProviderKeyStatus {
    provider_kind: Provider,
    provider: String,
    key_env_var: String,
    present: bool,
    auth_mode: ProviderAuthMethod,
    mode_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorCommandConfig {
    model: String,
    provider_keys: Vec<DoctorProviderKeyStatus>,
    session_enabled: bool,
    session_path: PathBuf,
    skills_dir: PathBuf,
    skills_lock_path: PathBuf,
    trust_root_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProfileSessionDefaults {
    enabled: bool,
    path: Option<String>,
    import_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProfilePolicyDefaults {
    tool_policy_preset: String,
    bash_profile: String,
    bash_dry_run: bool,
    os_sandbox_mode: String,
    enforce_regular_files: bool,
    bash_timeout_ms: u64,
    max_command_length: usize,
    max_tool_output_bytes: usize,
    max_file_read_bytes: usize,
    max_file_write_bytes: usize,
    allow_command_newlines: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProfileAuthDefaults {
    #[serde(default = "default_provider_auth_method")]
    openai: ProviderAuthMethod,
    #[serde(default = "default_provider_auth_method")]
    anthropic: ProviderAuthMethod,
    #[serde(default = "default_provider_auth_method")]
    google: ProviderAuthMethod,
}

impl Default for ProfileAuthDefaults {
    fn default() -> Self {
        Self {
            openai: default_provider_auth_method(),
            anthropic: default_provider_auth_method(),
            google: default_provider_auth_method(),
        }
    }
}

fn default_provider_auth_method() -> ProviderAuthMethod {
    ProviderAuthMethod::ApiKey
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProfileDefaults {
    model: String,
    fallback_models: Vec<String>,
    session: ProfileSessionDefaults,
    policy: ProfilePolicyDefaults,
    #[serde(default)]
    auth: ProfileAuthDefaults,
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

#[derive(Clone, Copy)]
struct CommandExecutionContext<'a> {
    tool_policy_json: &'a serde_json::Value,
    session_import_mode: SessionImportMode,
    profile_defaults: &'a ProfileDefaults,
    skills_command_config: &'a SkillsSyncCommandConfig,
    auth_command_config: &'a AuthCommandConfig,
}

#[derive(Debug, Clone)]
struct AuthCommandConfig {
    credential_store: PathBuf,
    credential_store_key: Option<String>,
    credential_store_encryption: CredentialStoreEncryptionMode,
    api_key: Option<String>,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
    google_api_key: Option<String>,
    openai_auth_mode: ProviderAuthMethod,
    anthropic_auth_mode: ProviderAuthMethod,
    google_auth_mode: ProviderAuthMethod,
}

fn build_auth_command_config(cli: &Cli) -> AuthCommandConfig {
    AuthCommandConfig {
        credential_store: cli.credential_store.clone(),
        credential_store_key: cli.credential_store_key.clone(),
        credential_store_encryption: resolve_credential_store_encryption_mode(cli),
        api_key: cli.api_key.clone(),
        openai_api_key: cli.openai_api_key.clone(),
        anthropic_api_key: cli.anthropic_api_key.clone(),
        google_api_key: cli.google_api_key.clone(),
        openai_auth_mode: cli.openai_auth_mode.into(),
        anthropic_auth_mode: cli.anthropic_auth_mode.into(),
        google_auth_mode: cli.google_auth_mode.into(),
    }
}

#[derive(Clone, Copy)]
struct InteractiveRuntimeConfig<'a> {
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    command_context: CommandExecutionContext<'a>,
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

#[derive(Debug, Default)]
struct PromptTelemetryState {
    next_prompt_id: u64,
    active: Option<PromptTelemetryRunState>,
}

#[derive(Debug)]
struct PromptTelemetryRunState {
    prompt_id: u64,
    started_unix_ms: u64,
    started: Instant,
    turn_count: u64,
    request_duration_ms_total: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    tool_calls: u64,
    tool_errors: u64,
    finish_reason: Option<String>,
}

#[derive(Clone)]
struct PromptTelemetryLogger {
    path: PathBuf,
    provider: String,
    model: String,
    file: Arc<Mutex<std::fs::File>>,
    state: Arc<Mutex<PromptTelemetryState>>,
}

impl PromptTelemetryLogger {
    fn open(path: PathBuf, provider: &str, model: &str) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create telemetry log directory {}",
                        parent.display()
                    )
                })?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open telemetry log {}", path.display()))?;
        Ok(Self {
            path,
            provider: provider.to_string(),
            model: model.to_string(),
            file: Arc::new(Mutex::new(file)),
            state: Arc::new(Mutex::new(PromptTelemetryState::default())),
        })
    }

    fn build_record(
        &self,
        active: PromptTelemetryRunState,
        status: &'static str,
        success: bool,
    ) -> Value {
        serde_json::json!({
            "record_type": "prompt_telemetry_v1",
            "schema_version": 1,
            "timestamp_unix_ms": current_unix_timestamp_ms(),
            "prompt_id": active.prompt_id,
            "provider": self.provider,
            "model": self.model,
            "status": status,
            "success": success,
            "started_unix_ms": active.started_unix_ms,
            "duration_ms": active.started.elapsed().as_millis() as u64,
            "turn_count": active.turn_count,
            "request_duration_ms_total": active.request_duration_ms_total,
            "finish_reason": active.finish_reason,
            "token_usage": {
                "input_tokens": active.input_tokens,
                "output_tokens": active.output_tokens,
                "total_tokens": active.total_tokens,
            },
            "tool_calls": active.tool_calls,
            "tool_errors": active.tool_errors,
            "redaction_policy": {
                "prompt_content": "omitted",
                "tool_arguments": "omitted",
                "tool_results": "bytes_only",
            }
        })
    }

    fn log_event(&self, event: &AgentEvent) -> Result<()> {
        let mut records = Vec::new();
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow!("telemetry state lock is poisoned"))?;
            match event {
                AgentEvent::AgentStart => {
                    if let Some(active) = state.active.take() {
                        records.push(self.build_record(active, "interrupted", false));
                    }
                    state.next_prompt_id = state.next_prompt_id.saturating_add(1);
                    let prompt_id = state.next_prompt_id;
                    state.active = Some(PromptTelemetryRunState {
                        prompt_id,
                        started_unix_ms: current_unix_timestamp_ms(),
                        started: Instant::now(),
                        turn_count: 0,
                        request_duration_ms_total: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        total_tokens: 0,
                        tool_calls: 0,
                        tool_errors: 0,
                        finish_reason: None,
                    });
                }
                AgentEvent::TurnEnd {
                    request_duration_ms,
                    usage,
                    finish_reason,
                    ..
                } => {
                    if let Some(active) = state.active.as_mut() {
                        active.turn_count = active.turn_count.saturating_add(1);
                        active.request_duration_ms_total = active
                            .request_duration_ms_total
                            .saturating_add(*request_duration_ms);
                        active.input_tokens =
                            active.input_tokens.saturating_add(usage.input_tokens);
                        active.output_tokens =
                            active.output_tokens.saturating_add(usage.output_tokens);
                        active.total_tokens =
                            active.total_tokens.saturating_add(usage.total_tokens);
                        active.finish_reason = finish_reason.clone();
                    }
                }
                AgentEvent::ToolExecutionEnd { result, .. } => {
                    if let Some(active) = state.active.as_mut() {
                        active.tool_calls = active.tool_calls.saturating_add(1);
                        if result.is_error {
                            active.tool_errors = active.tool_errors.saturating_add(1);
                        }
                    }
                }
                AgentEvent::AgentEnd { .. } => {
                    if let Some(active) = state.active.take() {
                        let success = active.tool_errors == 0;
                        let status = if success {
                            "completed"
                        } else {
                            "completed_with_tool_errors"
                        };
                        records.push(self.build_record(active, status, success));
                    }
                }
                _ => {}
            }
        }

        if records.is_empty() {
            return Ok(());
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("telemetry file lock is poisoned"))?;
        for record in records {
            let line =
                serde_json::to_string(&record).context("failed to encode telemetry event")?;
            writeln!(file, "{line}").with_context(|| {
                format!("failed to write telemetry log {}", self.path.display())
            })?;
        }
        file.flush()
            .with_context(|| format!("failed to flush telemetry log {}", self.path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ToolAuditAggregate {
    count: u64,
    error_count: u64,
    durations_ms: Vec<u64>,
}

#[derive(Debug, Default)]
struct ProviderAuditAggregate {
    count: u64,
    error_count: u64,
    durations_ms: Vec<u64>,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Default)]
struct AuditSummary {
    record_count: u64,
    tool_event_count: u64,
    prompt_record_count: u64,
    tools: BTreeMap<String, ToolAuditAggregate>,
    providers: BTreeMap<String, ProviderAuditAggregate>,
}

fn summarize_audit_file(path: &Path) -> Result<AuditSummary> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open audit file {}", path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut summary = AuditSummary::default();
    for (line_no, raw_line) in std::io::BufRead::lines(reader).enumerate() {
        let line = raw_line.with_context(|| {
            format!(
                "failed to read line {} from {}",
                line_no + 1,
                path.display()
            )
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        summary.record_count = summary.record_count.saturating_add(1);
        let value: Value = serde_json::from_str(trimmed).with_context(|| {
            format!(
                "failed to parse JSON at line {} in {}",
                line_no + 1,
                path.display()
            )
        })?;

        if value.get("event").and_then(Value::as_str) == Some("tool_execution_end") {
            summary.tool_event_count = summary.tool_event_count.saturating_add(1);
            let tool_name = value
                .get("tool_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown_tool")
                .to_string();
            let duration_ms = value.get("duration_ms").and_then(Value::as_u64);
            let is_error = value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let aggregate = summary.tools.entry(tool_name).or_default();
            aggregate.count = aggregate.count.saturating_add(1);
            if is_error {
                aggregate.error_count = aggregate.error_count.saturating_add(1);
            }
            if let Some(duration_ms) = duration_ms {
                aggregate.durations_ms.push(duration_ms);
            }
            continue;
        }

        if value.get("record_type").and_then(Value::as_str) == Some("prompt_telemetry_v1") {
            summary.prompt_record_count = summary.prompt_record_count.saturating_add(1);
            let provider = value
                .get("provider")
                .and_then(Value::as_str)
                .unwrap_or("unknown_provider")
                .to_string();
            let duration_ms = value
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let status = value.get("status").and_then(Value::as_str);
            let success = value
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| status == Some("completed"));

            let usage = value
                .get("token_usage")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let input_tokens = usage
                .get("input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let output_tokens = usage
                .get("output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let total_tokens = usage
                .get("total_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);

            let aggregate = summary.providers.entry(provider).or_default();
            aggregate.count = aggregate.count.saturating_add(1);
            if !success {
                aggregate.error_count = aggregate.error_count.saturating_add(1);
            }
            if duration_ms > 0 {
                aggregate.durations_ms.push(duration_ms);
            }
            aggregate.input_tokens = aggregate.input_tokens.saturating_add(input_tokens);
            aggregate.output_tokens = aggregate.output_tokens.saturating_add(output_tokens);
            aggregate.total_tokens = aggregate.total_tokens.saturating_add(total_tokens);
        }
    }

    Ok(summary)
}

fn percentile_duration_ms(values: &[u64], percentile_numerator: u64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let len = sorted.len() as u64;
    let rank = len.saturating_mul(percentile_numerator).saturating_add(99) / 100;
    let index = rank.saturating_sub(1).min(len.saturating_sub(1)) as usize;
    sorted[index]
}

fn render_audit_summary(path: &Path, summary: &AuditSummary) -> String {
    let mut lines = vec![format!(
        "audit summary: path={} records={} tool_events={} prompt_records={}",
        path.display(),
        summary.record_count,
        summary.tool_event_count,
        summary.prompt_record_count
    )];

    lines.push("tool_breakdown:".to_string());
    if summary.tools.is_empty() {
        lines.push("  none".to_string());
    } else {
        for (tool_name, aggregate) in &summary.tools {
            let error_rate = if aggregate.count == 0 {
                0.0
            } else {
                (aggregate.error_count as f64 / aggregate.count as f64) * 100.0
            };
            lines.push(format!(
                "  {} count={} error_rate={:.2}% p50_ms={} p95_ms={}",
                tool_name,
                aggregate.count,
                error_rate,
                percentile_duration_ms(&aggregate.durations_ms, 50),
                percentile_duration_ms(&aggregate.durations_ms, 95),
            ));
        }
    }

    lines.push("provider_breakdown:".to_string());
    if summary.providers.is_empty() {
        lines.push("  none".to_string());
    } else {
        for (provider, aggregate) in &summary.providers {
            let error_rate = if aggregate.count == 0 {
                0.0
            } else {
                (aggregate.error_count as f64 / aggregate.count as f64) * 100.0
            };
            lines.push(format!(
                "  {} count={} error_rate={:.2}% p50_ms={} p95_ms={} input_tokens={} output_tokens={} total_tokens={}",
                provider,
                aggregate.count,
                error_rate,
                percentile_duration_ms(&aggregate.durations_ms, 50),
                percentile_duration_ms(&aggregate.durations_ms, 95),
                aggregate.input_tokens,
                aggregate.output_tokens,
                aggregate.total_tokens,
            ));
        }
    }

    lines.join("\n")
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
    if cli.channel_store_inspect.is_some() || cli.channel_store_repair.is_some() {
        execute_channel_store_admin_command(&cli)?;
        return Ok(());
    }
    if cli.event_webhook_ingest_file.is_some() {
        validate_event_webhook_ingest_cli(&cli)?;
        let payload_file = cli
            .event_webhook_ingest_file
            .clone()
            .ok_or_else(|| anyhow!("--event-webhook-ingest-file is required"))?;
        let channel_ref = cli
            .event_webhook_channel
            .clone()
            .ok_or_else(|| anyhow!("--event-webhook-channel is required"))?;
        let event_webhook_secret = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.event_webhook_secret.as_deref(),
            cli.event_webhook_secret_id.as_deref(),
            "--event-webhook-secret-id",
        )?;
        ingest_webhook_immediate_event(&EventWebhookIngestConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            channel_ref,
            payload_file,
            prompt_prefix: cli.event_webhook_prompt_prefix.clone(),
            debounce_key: cli.event_webhook_debounce_key.clone(),
            debounce_window_seconds: cli.event_webhook_debounce_window_seconds,
            signature: cli.event_webhook_signature.clone(),
            timestamp: cli.event_webhook_timestamp.clone(),
            secret: event_webhook_secret,
            signature_algorithm: cli.event_webhook_signature_algorithm.map(Into::into),
            signature_max_skew_seconds: cli.event_webhook_signature_max_skew_seconds,
        })?;
        return Ok(());
    }

    if cli.no_session && cli.branch_from.is_some() {
        bail!("--branch-from cannot be used together with --no-session");
    }

    let model_ref = ModelRef::parse(&cli.model)
        .map_err(|error| anyhow!("failed to parse --model '{}': {error}", cli.model))?;
    let fallback_model_refs = resolve_fallback_models(&cli, &model_ref)?;

    let client = build_client_with_fallbacks(&cli, &model_ref, &fallback_model_refs)?;
    let mut skill_lock_hints = Vec::new();
    if !cli.install_skill.is_empty() {
        let report = install_skills(&cli.install_skill, &cli.skills_dir)?;
        skill_lock_hints.extend(build_local_skill_lock_hints(&cli.install_skill)?);
        println!(
            "skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let skills_download_options = SkillsDownloadOptions {
        cache_dir: Some(
            cli.skills_cache_dir
                .clone()
                .unwrap_or_else(|| default_skills_cache_dir(&cli.skills_dir)),
        ),
        offline: cli.skills_offline,
    };
    let remote_skill_sources =
        resolve_remote_skill_sources(&cli.install_skill_url, &cli.install_skill_sha256)?;
    if !remote_skill_sources.is_empty() {
        let report = install_remote_skills_with_cache(
            &remote_skill_sources,
            &cli.skills_dir,
            &skills_download_options,
        )
        .await?;
        skill_lock_hints.extend(build_remote_skill_lock_hints(&remote_skill_sources)?);
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
        let manifest = fetch_registry_manifest_with_cache(
            registry_url,
            cli.skill_registry_sha256.as_deref(),
            &skills_download_options,
        )
        .await?;
        let sources = resolve_registry_skill_sources(
            &manifest,
            &cli.install_skill_from_registry,
            &trusted_skill_roots,
            cli.require_signed_skills,
        )?;
        let report =
            install_remote_skills_with_cache(&sources, &cli.skills_dir, &skills_download_options)
                .await?;
        skill_lock_hints.extend(build_registry_skill_lock_hints(
            registry_url,
            &cli.install_skill_from_registry,
            &sources,
        )?);
        println!(
            "registry skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let skills_lock_path = cli
        .skills_lock_file
        .clone()
        .unwrap_or_else(|| default_skills_lock_path(&cli.skills_dir));
    if cli.skills_lock_write {
        let lockfile =
            write_skills_lockfile(&cli.skills_dir, &skills_lock_path, &skill_lock_hints)?;
        println!(
            "{}",
            render_skills_lock_write_success(&skills_lock_path, lockfile.entries.len())
        );
    }
    if cli.skills_sync {
        let report = sync_skills_with_lockfile(&cli.skills_dir, &skills_lock_path)?;
        if report.in_sync() {
            println!("{}", render_skills_sync_in_sync(&skills_lock_path, &report));
        } else {
            bail!(
                "skills sync drift detected: path={} {}",
                skills_lock_path.display(),
                render_skills_sync_drift_details(&report)
            );
        }
    }
    let base_system_prompt = resolve_system_prompt(&cli)?;
    let catalog = load_catalog(&cli.skills_dir)
        .with_context(|| format!("failed to load skills from {}", cli.skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    let system_prompt = augment_system_prompt(&base_system_prompt, &selected_skills);

    let tool_policy = build_tool_policy(&cli)?;
    let tool_policy_json = tool_policy_to_json(&tool_policy);
    if cli.print_tool_policy {
        println!("{tool_policy_json}");
    }
    let render_options = RenderOptions::from_cli(&cli);
    validate_github_issues_bridge_cli(&cli)?;
    validate_slack_bridge_cli(&cli)?;
    validate_events_runner_cli(&cli)?;
    if cli.github_issues_bridge {
        let repo_slug = cli.github_repo.clone().ok_or_else(|| {
            anyhow!("--github-repo is required when --github-issues-bridge is set")
        })?;
        let token = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.github_token.as_deref(),
            cli.github_token_id.as_deref(),
            "--github-token-id",
        )?
        .ok_or_else(|| {
            anyhow!(
                "--github-token (or --github-token-id) is required when --github-issues-bridge is set"
            )
        })?;
        return run_github_issues_bridge(GithubIssuesBridgeRuntimeConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            state_dir: cli.github_state_dir.clone(),
            repo_slug,
            api_base: cli.github_api_base.clone(),
            token,
            bot_login: cli.github_bot_login.clone(),
            poll_interval: Duration::from_secs(cli.github_poll_interval_seconds.max(1)),
            include_issue_body: cli.github_include_issue_body,
            include_edited_comments: cli.github_include_edited_comments,
            processed_event_cap: cli.github_processed_event_cap.max(1),
            retry_max_attempts: cli.github_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.github_retry_base_delay_ms.max(1),
        })
        .await;
    }
    if cli.slack_bridge {
        let app_token = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.slack_app_token.as_deref(),
            cli.slack_app_token_id.as_deref(),
            "--slack-app-token-id",
        )?
        .ok_or_else(|| {
            anyhow!("--slack-app-token (or --slack-app-token-id) is required when --slack-bridge is set")
        })?;
        let bot_token = resolve_secret_from_cli_or_store_id(
            &cli,
            cli.slack_bot_token.as_deref(),
            cli.slack_bot_token_id.as_deref(),
            "--slack-bot-token-id",
        )?
        .ok_or_else(|| {
            anyhow!("--slack-bot-token (or --slack-bot-token-id) is required when --slack-bridge is set")
        })?;
        return run_slack_bridge(SlackBridgeRuntimeConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            request_timeout_ms: cli.request_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            state_dir: cli.slack_state_dir.clone(),
            api_base: cli.slack_api_base.clone(),
            app_token,
            bot_token,
            bot_user_id: cli.slack_bot_user_id.clone(),
            detail_thread_output: cli.slack_thread_detail_output,
            detail_thread_threshold_chars: cli.slack_thread_detail_threshold_chars.max(1),
            processed_event_cap: cli.slack_processed_event_cap.max(1),
            max_event_age_seconds: cli.slack_max_event_age_seconds,
            reconnect_delay: Duration::from_millis(cli.slack_reconnect_delay_ms.max(1)),
            retry_max_attempts: cli.slack_retry_max_attempts.max(1),
            retry_base_delay_ms: cli.slack_retry_base_delay_ms.max(1),
        })
        .await;
    }
    if cli.events_runner {
        return run_event_scheduler(EventSchedulerConfig {
            client: client.clone(),
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            tool_policy: tool_policy.clone(),
            turn_timeout_ms: cli.turn_timeout_ms,
            render_options,
            session_lock_wait_ms: cli.session_lock_wait_ms,
            session_lock_stale_ms: cli.session_lock_stale_ms,
            channel_store_root: cli.channel_store_root.clone(),
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            poll_interval: Duration::from_millis(cli.events_poll_interval_ms.max(1)),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        })
        .await;
    }

    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
            max_turns: cli.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );
    tools::register_builtin_tools(&mut agent, tool_policy);
    if let Some(path) = cli.tool_audit_log.clone() {
        let logger = ToolAuditLogger::open(path)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("tool audit logger error: {error}");
            }
        });
    }
    if let Some(path) = cli.telemetry_log.clone() {
        let logger =
            PromptTelemetryLogger::open(path, model_ref.provider.as_str(), &model_ref.model)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("telemetry logger error: {error}");
            }
        });
    }
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

    let skills_sync_command_config = SkillsSyncCommandConfig {
        skills_dir: cli.skills_dir.clone(),
        default_lock_path: skills_lock_path.clone(),
        default_trust_root_path: cli.skill_trust_root_file.clone(),
        doctor_config: build_doctor_command_config(
            &cli,
            &model_ref,
            &fallback_model_refs,
            &skills_lock_path,
        ),
    };
    let profile_defaults = build_profile_defaults(&cli);
    let auth_command_config = build_auth_command_config(&cli);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: cli.session_import_mode.into(),
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_sync_command_config,
        auth_command_config: &auth_command_config,
    };
    let interactive_config = InteractiveRuntimeConfig {
        turn_timeout_ms: cli.turn_timeout_ms,
        render_options,
        command_context,
    };
    if let Some(command_file_path) = cli.command_file.as_deref() {
        execute_command_file(
            command_file_path,
            cli.command_file_error_mode,
            &mut agent,
            &mut session_runtime,
            command_context,
        )?;
        return Ok(());
    }
    run_interactive(agent, session_runtime, interactive_config).await
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

fn validate_github_issues_bridge_cli(cli: &Cli) -> Result<()> {
    if !cli.github_issues_bridge {
        return Ok(());
    }

    if cli.prompt.is_some() || cli.prompt_file.is_some() || cli.command_file.is_some() {
        bail!(
            "--github-issues-bridge cannot be combined with --prompt, --prompt-file, or --command-file"
        );
    }
    if cli.no_session {
        bail!("--github-issues-bridge cannot be used together with --no-session");
    }
    if cli.github_poll_interval_seconds == 0 {
        bail!("--github-poll-interval-seconds must be greater than 0");
    }
    if cli.github_processed_event_cap == 0 {
        bail!("--github-processed-event-cap must be greater than 0");
    }
    if cli.github_retry_max_attempts == 0 {
        bail!("--github-retry-max-attempts must be greater than 0");
    }
    if cli.github_retry_base_delay_ms == 0 {
        bail!("--github-retry-base-delay-ms must be greater than 0");
    }
    if cli
        .github_repo
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        bail!("--github-repo is required when --github-issues-bridge is set");
    }
    let has_github_token = resolve_non_empty_cli_value(cli.github_token.as_deref()).is_some();
    let has_github_token_id = resolve_non_empty_cli_value(cli.github_token_id.as_deref()).is_some();
    if !has_github_token && !has_github_token_id {
        bail!(
            "--github-token (or --github-token-id) is required when --github-issues-bridge is set"
        );
    }
    Ok(())
}

fn validate_slack_bridge_cli(cli: &Cli) -> Result<()> {
    if !cli.slack_bridge {
        return Ok(());
    }

    if cli.prompt.is_some() || cli.prompt_file.is_some() || cli.command_file.is_some() {
        bail!("--slack-bridge cannot be combined with --prompt, --prompt-file, or --command-file");
    }
    if cli.no_session {
        bail!("--slack-bridge cannot be used together with --no-session");
    }
    if cli.github_issues_bridge {
        bail!("--slack-bridge cannot be combined with --github-issues-bridge");
    }
    let has_slack_app_token = resolve_non_empty_cli_value(cli.slack_app_token.as_deref()).is_some();
    let has_slack_app_token_id =
        resolve_non_empty_cli_value(cli.slack_app_token_id.as_deref()).is_some();
    if !has_slack_app_token && !has_slack_app_token_id {
        bail!("--slack-app-token (or --slack-app-token-id) is required when --slack-bridge is set");
    }
    let has_slack_bot_token = resolve_non_empty_cli_value(cli.slack_bot_token.as_deref()).is_some();
    let has_slack_bot_token_id =
        resolve_non_empty_cli_value(cli.slack_bot_token_id.as_deref()).is_some();
    if !has_slack_bot_token && !has_slack_bot_token_id {
        bail!("--slack-bot-token (or --slack-bot-token-id) is required when --slack-bridge is set");
    }
    if cli.slack_thread_detail_threshold_chars == 0 {
        bail!("--slack-thread-detail-threshold-chars must be greater than 0");
    }
    if cli.slack_processed_event_cap == 0 {
        bail!("--slack-processed-event-cap must be greater than 0");
    }
    if cli.slack_reconnect_delay_ms == 0 {
        bail!("--slack-reconnect-delay-ms must be greater than 0");
    }
    if cli.slack_retry_max_attempts == 0 {
        bail!("--slack-retry-max-attempts must be greater than 0");
    }
    if cli.slack_retry_base_delay_ms == 0 {
        bail!("--slack-retry-base-delay-ms must be greater than 0");
    }

    Ok(())
}

fn validate_events_runner_cli(cli: &Cli) -> Result<()> {
    if !cli.events_runner {
        return Ok(());
    }

    if cli.prompt.is_some() || cli.prompt_file.is_some() || cli.command_file.is_some() {
        bail!("--events-runner cannot be combined with --prompt, --prompt-file, or --command-file");
    }
    if cli.no_session {
        bail!("--events-runner cannot be used together with --no-session");
    }
    if cli.github_issues_bridge || cli.slack_bridge {
        bail!("--events-runner cannot be combined with --github-issues-bridge or --slack-bridge");
    }
    if cli.events_poll_interval_ms == 0 {
        bail!("--events-poll-interval-ms must be greater than 0");
    }
    if cli.events_queue_limit == 0 {
        bail!("--events-queue-limit must be greater than 0");
    }
    Ok(())
}

fn validate_event_webhook_ingest_cli(cli: &Cli) -> Result<()> {
    if cli.event_webhook_ingest_file.is_none() {
        return Ok(());
    }
    if cli.events_runner {
        bail!("--event-webhook-ingest-file cannot be combined with --events-runner");
    }
    if cli
        .event_webhook_channel
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        bail!("--event-webhook-channel is required when --event-webhook-ingest-file is set");
    }
    if cli.event_webhook_debounce_window_seconds == 0 {
        bail!("--event-webhook-debounce-window-seconds must be greater than 0");
    }

    let signing_configured = cli.event_webhook_signature.is_some()
        || cli.event_webhook_timestamp.is_some()
        || cli.event_webhook_secret.is_some()
        || cli.event_webhook_secret_id.is_some()
        || cli.event_webhook_signature_algorithm.is_some();
    if signing_configured {
        if cli
            .event_webhook_signature
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
        {
            bail!(
                "--event-webhook-signature is required when webhook signature verification is configured"
            );
        }
        let has_webhook_secret =
            resolve_non_empty_cli_value(cli.event_webhook_secret.as_deref()).is_some();
        let has_webhook_secret_id =
            resolve_non_empty_cli_value(cli.event_webhook_secret_id.as_deref()).is_some();
        if !has_webhook_secret && !has_webhook_secret_id {
            bail!("--event-webhook-secret (or --event-webhook-secret-id) is required when webhook signature verification is configured");
        }
        match cli.event_webhook_signature_algorithm {
            Some(CliWebhookSignatureAlgorithm::GithubSha256) => {}
            Some(CliWebhookSignatureAlgorithm::SlackV0) => {
                if cli
                    .event_webhook_timestamp
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
                {
                    bail!(
                        "--event-webhook-timestamp is required when --event-webhook-signature-algorithm=slack-v0"
                    );
                }
            }
            None => {
                bail!(
                    "--event-webhook-signature-algorithm is required when webhook signature verification is configured"
                );
            }
        }
    }
    Ok(())
}

fn execute_channel_store_admin_command(cli: &Cli) -> Result<()> {
    if let Some(raw_ref) = cli.channel_store_inspect.as_deref() {
        let channel_ref = ChannelStore::parse_channel_ref(raw_ref)?;
        let store = ChannelStore::open(
            &cli.channel_store_root,
            &channel_ref.transport,
            &channel_ref.channel_id,
        )?;
        let report = store.inspect()?;
        println!(
            "channel store inspect: transport={} channel_id={} dir={} log_records={} context_records={} invalid_log_lines={} invalid_context_lines={} memory_exists={} memory_bytes={}",
            report.transport,
            report.channel_id,
            report.channel_dir.display(),
            report.log_records,
            report.context_records,
            report.invalid_log_lines,
            report.invalid_context_lines,
            report.memory_exists,
            report.memory_bytes,
        );
        return Ok(());
    }

    if let Some(raw_ref) = cli.channel_store_repair.as_deref() {
        let channel_ref = ChannelStore::parse_channel_ref(raw_ref)?;
        let store = ChannelStore::open(
            &cli.channel_store_root,
            &channel_ref.transport,
            &channel_ref.channel_id,
        )?;
        let report = store.repair()?;
        println!(
            "channel store repair: transport={} channel_id={} log_removed_lines={} context_removed_lines={} log_backup_path={} context_backup_path={}",
            channel_ref.transport,
            channel_ref.channel_id,
            report.log_removed_lines,
            report.context_removed_lines,
            report
                .log_backup_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
            report
                .context_backup_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        return Ok(());
    }

    Ok(())
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

fn load_trust_root_records(path: &Path) -> Result<Vec<TrustedRootRecord>> {
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

fn save_trust_root_records(path: &Path, records: &[TrustedRootRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut payload = serde_json::to_string_pretty(&TrustedRootFileFormat::Wrapped {
        roots: records.to_vec(),
    })
    .context("failed to serialize trusted root records")?;
    payload.push('\n');
    write_text_atomic(path, &payload)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn apply_trust_root_mutations(
    records: &mut Vec<TrustedRootRecord>,
    cli: &Cli,
) -> Result<TrustMutationReport> {
    apply_trust_root_mutation_specs(
        records,
        &cli.skill_trust_add,
        &cli.skill_trust_revoke,
        &cli.skill_trust_rotate,
    )
}

fn apply_trust_root_mutation_specs(
    records: &mut Vec<TrustedRootRecord>,
    add_specs: &[String],
    revoke_ids: &[String],
    rotate_specs: &[String],
) -> Result<TrustMutationReport> {
    let mut report = TrustMutationReport::default();

    for spec in add_specs {
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

    for id in revoke_ids {
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

    for spec in rotate_specs {
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
    config: InteractiveRuntimeConfig<'_>,
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
                config.command_context.tool_policy_json,
                config.command_context.session_import_mode,
                config.command_context.profile_defaults,
                config.command_context.skills_command_config,
                config.command_context.auth_command_config,
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
            config.turn_timeout_ms,
            tokio::signal::ctrl_c(),
            config.render_options,
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
    let streamed_output = Arc::new(AtomicBool::new(false));
    let stream_delta_handler = if render_options.stream_output {
        let streamed_output = streamed_output.clone();
        let stream_delay_ms = render_options.stream_delay_ms;
        Some(Arc::new(move |delta: String| {
            if delta.is_empty() {
                return;
            }
            streamed_output.store(true, Ordering::Relaxed);
            print!("{delta}");
            let _ = std::io::stdout().flush();
            if stream_delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(stream_delay_ms));
            }
        }) as StreamDeltaHandler)
    } else {
        None
    };
    tokio::pin!(cancellation_signal);

    enum PromptOutcome<T> {
        Result(T),
        Cancelled,
        TimedOut,
    }

    let prompt_result = if turn_timeout_ms == 0 {
        tokio::select! {
            result = agent.prompt_with_stream(prompt, stream_delta_handler.clone()) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
        }
    } else {
        let timeout = tokio::time::sleep(Duration::from_millis(turn_timeout_ms));
        tokio::pin!(timeout);
        tokio::select! {
            result = agent.prompt_with_stream(prompt, stream_delta_handler.clone()) => PromptOutcome::Result(result),
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
    print_assistant_messages(
        &new_messages,
        render_options,
        streamed_output.load(Ordering::Relaxed),
    );
    Ok(PromptRunStatus::Completed)
}

const SESSION_SEARCH_DEFAULT_RESULTS: usize = 50;
const SESSION_SEARCH_MAX_RESULTS: usize = 200;
const SESSION_SEARCH_PREVIEW_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSearchMatch {
    id: u64,
    parent_id: Option<u64>,
    role: String,
    preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSearchArgs {
    query: String,
    role: Option<String>,
    limit: usize,
}

fn parse_session_search_role(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "system" | "user" | "assistant" | "tool" => Ok(normalized),
        _ => bail!(
            "invalid role '{}'; expected one of: system, user, assistant, tool",
            raw
        ),
    }
}

fn parse_session_search_limit(raw: &str) -> Result<usize> {
    let value = raw
        .trim()
        .parse::<usize>()
        .with_context(|| format!("invalid limit '{}'; expected an integer", raw))?;
    if value == 0 {
        bail!("limit must be greater than 0");
    }
    if value > SESSION_SEARCH_MAX_RESULTS {
        bail!(
            "limit {} exceeds maximum {}",
            value,
            SESSION_SEARCH_MAX_RESULTS
        );
    }
    Ok(value)
}

fn parse_session_search_args(command_args: &str) -> Result<SessionSearchArgs> {
    let mut query_parts = Vec::new();
    let mut role = None;
    let mut limit = SESSION_SEARCH_DEFAULT_RESULTS;
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    let mut index = 0usize;
    while index < tokens.len() {
        let token = tokens[index];
        if token == "--role" {
            let value = tokens
                .get(index + 1)
                .ok_or_else(|| anyhow!("missing value for --role"))?;
            role = Some(parse_session_search_role(value)?);
            index += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--role=") {
            role = Some(parse_session_search_role(value)?);
            index += 1;
            continue;
        }
        if token == "--limit" {
            let value = tokens
                .get(index + 1)
                .ok_or_else(|| anyhow!("missing value for --limit"))?;
            limit = parse_session_search_limit(value)?;
            index += 2;
            continue;
        }
        if let Some(value) = token.strip_prefix("--limit=") {
            limit = parse_session_search_limit(value)?;
            index += 1;
            continue;
        }
        if token.starts_with("--") {
            bail!("unknown flag '{}'", token);
        }

        query_parts.push(token.to_string());
        index += 1;
    }

    let query = query_parts.join(" ");
    if query.trim().is_empty() {
        bail!("query is required");
    }

    Ok(SessionSearchArgs { query, role, limit })
}

fn normalize_preview_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn session_message_preview(message: &Message) -> String {
    let normalized = normalize_preview_text(&message.text_content());
    let preview = if normalized.is_empty() {
        "(no text)".to_string()
    } else {
        normalized
    };

    if preview.chars().count() <= SESSION_SEARCH_PREVIEW_CHARS {
        return preview;
    }
    let truncated = preview
        .chars()
        .take(SESSION_SEARCH_PREVIEW_CHARS)
        .collect::<String>();
    format!("{truncated}...")
}

fn session_message_role(message: &Message) -> String {
    format!("{:?}", message.role).to_lowercase()
}

fn search_session_entries(
    entries: &[crate::session::SessionEntry],
    query: &str,
    role_filter: Option<&str>,
    max_results: usize,
) -> (Vec<SessionSearchMatch>, usize) {
    let normalized_query = query.to_lowercase();
    let mut ordered_entries = entries.iter().collect::<Vec<_>>();
    ordered_entries.sort_by_key(|entry| entry.id);

    let mut matches = Vec::new();
    let mut total_matches = 0usize;
    for entry in ordered_entries {
        let role = session_message_role(&entry.message);
        if let Some(role_filter) = role_filter {
            if role != role_filter {
                continue;
            }
        }
        let text = entry.message.text_content();
        let role_hit = role.contains(&normalized_query);
        let text_hit = text.to_lowercase().contains(&normalized_query);
        if !role_hit && !text_hit {
            continue;
        }

        total_matches += 1;
        if matches.len() >= max_results {
            continue;
        }
        matches.push(SessionSearchMatch {
            id: entry.id,
            parent_id: entry.parent_id,
            role,
            preview: session_message_preview(&entry.message),
        });
    }

    (matches, total_matches)
}

fn render_session_search(
    query: &str,
    role_filter: Option<&str>,
    entries_count: usize,
    matches: &[SessionSearchMatch],
    total_matches: usize,
    max_results: usize,
) -> String {
    let role = role_filter.unwrap_or("any");
    let mut lines = vec![format!(
        "session search: query=\"{}\" role={} entries={} matches={} shown={} limit={}",
        query,
        role,
        entries_count,
        total_matches,
        matches.len(),
        max_results
    )];
    if matches.is_empty() {
        lines.push("results: none".to_string());
        return lines.join("\n");
    }

    for item in matches {
        lines.push(format!(
            "result: id={} parent={} role={} preview={}",
            item.id,
            item.parent_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            item.role,
            item.preview
        ));
    }
    lines.join("\n")
}

fn execute_session_search_command(runtime: &SessionRuntime, command_args: &str) -> String {
    let args = match parse_session_search_args(command_args) {
        Ok(args) => args,
        Err(error) => return format!("session search error: error={error}"),
    };

    let (matches, total_matches) = search_session_entries(
        runtime.store.entries(),
        &args.query,
        args.role.as_deref(),
        args.limit,
    );
    render_session_search(
        &args.query,
        args.role.as_deref(),
        runtime.store.entries().len(),
        &matches,
        total_matches,
        args.limit,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionDiffEntry {
    id: u64,
    parent_id: Option<u64>,
    role: String,
    preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionDiffReport {
    source: &'static str,
    left_id: u64,
    right_id: u64,
    shared_depth: usize,
    left_depth: usize,
    right_depth: usize,
    shared_entries: Vec<SessionDiffEntry>,
    left_only_entries: Vec<SessionDiffEntry>,
    right_only_entries: Vec<SessionDiffEntry>,
}

fn parse_session_diff_args(command_args: &str) -> Result<Option<(u64, u64)>> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(None);
    }
    if tokens.len() != 2 {
        bail!("usage: /session-diff [<left-id> <right-id>]");
    }
    let left = tokens[0].parse::<u64>().with_context(|| {
        format!(
            "invalid left session id '{}'; expected an integer",
            tokens[0]
        )
    })?;
    let right = tokens[1].parse::<u64>().with_context(|| {
        format!(
            "invalid right session id '{}'; expected an integer",
            tokens[1]
        )
    })?;
    Ok(Some((left, right)))
}

fn session_diff_entry(entry: &crate::session::SessionEntry) -> SessionDiffEntry {
    SessionDiffEntry {
        id: entry.id,
        parent_id: entry.parent_id,
        role: session_message_role(&entry.message),
        preview: session_message_preview(&entry.message),
    }
}

fn shared_lineage_prefix_depth(
    left: &[crate::session::SessionEntry],
    right: &[crate::session::SessionEntry],
) -> usize {
    let mut depth = 0usize;
    for (left_entry, right_entry) in left.iter().zip(right.iter()) {
        if left_entry.id != right_entry.id {
            break;
        }
        depth += 1;
    }
    depth
}

fn resolve_session_diff_heads(
    runtime: &SessionRuntime,
    heads: Option<(u64, u64)>,
) -> Result<(u64, u64, &'static str)> {
    match heads {
        Some((left_id, right_id)) => {
            if !runtime.store.contains(left_id) {
                bail!("unknown left session id {left_id}");
            }
            if !runtime.store.contains(right_id) {
                bail!("unknown right session id {right_id}");
            }
            Ok((left_id, right_id, "explicit"))
        }
        None => {
            let left_id = runtime
                .active_head
                .ok_or_else(|| anyhow!("active head is not set"))?;
            if !runtime.store.contains(left_id) {
                bail!("active head {} does not exist in session", left_id);
            }
            let right_id = runtime
                .store
                .head_id()
                .ok_or_else(|| anyhow!("latest head is not set"))?;
            Ok((left_id, right_id, "default"))
        }
    }
}

fn compute_session_diff(
    runtime: &SessionRuntime,
    heads: Option<(u64, u64)>,
) -> Result<SessionDiffReport> {
    let (left_id, right_id, source) = resolve_session_diff_heads(runtime, heads)?;
    let left_lineage = runtime.store.lineage_entries(Some(left_id))?;
    let right_lineage = runtime.store.lineage_entries(Some(right_id))?;
    let shared_depth = shared_lineage_prefix_depth(&left_lineage, &right_lineage);

    Ok(SessionDiffReport {
        source,
        left_id,
        right_id,
        shared_depth,
        left_depth: left_lineage.len(),
        right_depth: right_lineage.len(),
        shared_entries: left_lineage
            .iter()
            .take(shared_depth)
            .map(session_diff_entry)
            .collect(),
        left_only_entries: left_lineage
            .iter()
            .skip(shared_depth)
            .map(session_diff_entry)
            .collect(),
        right_only_entries: right_lineage
            .iter()
            .skip(shared_depth)
            .map(session_diff_entry)
            .collect(),
    })
}

fn render_session_diff_entry(prefix: &str, entry: &SessionDiffEntry) -> String {
    format!(
        "{prefix}: id={} parent={} role={} preview={}",
        entry.id,
        entry
            .parent_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        entry.role,
        entry.preview
    )
}

fn render_session_diff(report: &SessionDiffReport) -> String {
    let mut lines = vec![
        format!(
            "session diff: source={} left={} right={}",
            report.source, report.left_id, report.right_id
        ),
        format!(
            "summary: shared_depth={} left_depth={} right_depth={} left_only={} right_only={}",
            report.shared_depth,
            report.left_depth,
            report.right_depth,
            report.left_only_entries.len(),
            report.right_only_entries.len()
        ),
    ];

    if report.shared_entries.is_empty() {
        lines.push("shared: none".to_string());
    } else {
        for entry in &report.shared_entries {
            lines.push(render_session_diff_entry("shared", entry));
        }
    }

    if report.left_only_entries.is_empty() {
        lines.push("left-only: none".to_string());
    } else {
        for entry in &report.left_only_entries {
            lines.push(render_session_diff_entry("left-only", entry));
        }
    }

    if report.right_only_entries.is_empty() {
        lines.push("right-only: none".to_string());
    } else {
        for entry in &report.right_only_entries {
            lines.push(render_session_diff_entry("right-only", entry));
        }
    }

    lines.join("\n")
}

fn execute_session_diff_command(runtime: &SessionRuntime, heads: Option<(u64, u64)>) -> String {
    match compute_session_diff(runtime, heads) {
        Ok(report) => render_session_diff(&report),
        Err(error) => format!("session diff error: {error}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionStats {
    entries: usize,
    branch_tips: usize,
    roots: usize,
    max_depth: usize,
    active_depth: Option<usize>,
    latest_depth: Option<usize>,
    active_head: Option<u64>,
    latest_head: Option<u64>,
    active_is_latest: bool,
    role_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionStatsOutputFormat {
    Text,
    Json,
}

fn parse_session_stats_args(command_args: &str) -> Result<SessionStatsOutputFormat> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(SessionStatsOutputFormat::Text);
    }
    if tokens.len() == 1 && tokens[0] == "--json" {
        return Ok(SessionStatsOutputFormat::Json);
    }
    bail!("usage: /session-stats [--json]");
}

fn compute_session_entry_depths(
    entries: &[crate::session::SessionEntry],
) -> Result<HashMap<u64, usize>> {
    let mut parent_by_id = HashMap::new();
    for entry in entries {
        if parent_by_id.insert(entry.id, entry.parent_id).is_some() {
            bail!("duplicate session entry id {}", entry.id);
        }
    }

    fn depth_for(
        id: u64,
        parent_by_id: &HashMap<u64, Option<u64>>,
        memo: &mut HashMap<u64, usize>,
        visiting: &mut HashSet<u64>,
    ) -> Result<usize> {
        if let Some(depth) = memo.get(&id) {
            return Ok(*depth);
        }
        if !visiting.insert(id) {
            bail!("detected cycle while computing depth at session id {id}");
        }

        let Some(parent_id) = parent_by_id.get(&id) else {
            bail!("unknown session entry id {}", id);
        };
        let depth = match parent_id {
            None => 1,
            Some(parent_id) => {
                if !parent_by_id.contains_key(parent_id) {
                    bail!("missing parent id {} for session entry {}", parent_id, id);
                }
                depth_for(*parent_id, parent_by_id, memo, visiting)? + 1
            }
        };
        visiting.remove(&id);
        memo.insert(id, depth);
        Ok(depth)
    }

    let mut memo = HashMap::new();
    for id in parent_by_id.keys().copied() {
        let mut visiting = HashSet::new();
        let _ = depth_for(id, &parent_by_id, &mut memo, &mut visiting)?;
    }
    Ok(memo)
}

fn compute_session_stats(runtime: &SessionRuntime) -> Result<SessionStats> {
    let entries = runtime.store.entries();
    let depths = compute_session_entry_depths(entries)?;
    let mut role_counts = BTreeMap::new();
    for entry in entries {
        let role = session_message_role(&entry.message);
        *role_counts.entry(role).or_insert(0) += 1;
    }

    let latest_head = runtime.store.head_id();
    let latest_depth = latest_head.and_then(|id| depths.get(&id).copied());
    let active_depth = match runtime.active_head {
        Some(id) => Some(
            *depths
                .get(&id)
                .ok_or_else(|| anyhow!("active head {} does not exist in session", id))?,
        ),
        None => None,
    };

    Ok(SessionStats {
        entries: entries.len(),
        branch_tips: runtime.store.branch_tips().len(),
        roots: entries
            .iter()
            .filter(|entry| entry.parent_id.is_none())
            .count(),
        max_depth: depths.values().copied().max().unwrap_or(0),
        active_depth,
        latest_depth,
        active_head: runtime.active_head,
        latest_head,
        active_is_latest: runtime.active_head == latest_head,
        role_counts,
    })
}

fn render_session_stats(stats: &SessionStats) -> String {
    let mut lines = vec![format!(
        "session stats: entries={} branch_tips={} roots={} max_depth={}",
        stats.entries, stats.branch_tips, stats.roots, stats.max_depth
    )];
    lines.push(format!(
        "heads: active={} latest={} active_is_latest={}",
        stats
            .active_head
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        stats
            .latest_head
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        stats.active_is_latest
    ));
    lines.push(format!(
        "depth: active={} latest={}",
        stats
            .active_depth
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        stats
            .latest_depth
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    ));

    if stats.role_counts.is_empty() {
        lines.push("roles: none".to_string());
    } else {
        for (role, count) in &stats.role_counts {
            lines.push(format!("role: {}={}", role, count));
        }
    }

    lines.join("\n")
}

fn render_session_stats_json(stats: &SessionStats) -> String {
    serde_json::json!({
        "entries": stats.entries,
        "branch_tips": stats.branch_tips,
        "roots": stats.roots,
        "max_depth": stats.max_depth,
        "active_depth": stats.active_depth,
        "latest_depth": stats.latest_depth,
        "active_head": stats.active_head,
        "latest_head": stats.latest_head,
        "active_is_latest": stats.active_is_latest,
        "role_counts": stats.role_counts,
    })
    .to_string()
}

fn execute_session_stats_command(
    runtime: &SessionRuntime,
    format: SessionStatsOutputFormat,
) -> String {
    match compute_session_stats(runtime) {
        Ok(stats) => match format {
            SessionStatsOutputFormat::Text => render_session_stats(&stats),
            SessionStatsOutputFormat::Json => render_session_stats_json(&stats),
        },
        Err(error) => match format {
            SessionStatsOutputFormat::Text => format!("session stats error: {error}"),
            SessionStatsOutputFormat::Json => serde_json::json!({
                "error": format!("session stats error: {error}")
            })
            .to_string(),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionGraphFormat {
    Mermaid,
    Dot,
}

impl SessionGraphFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Mermaid => "mermaid",
            Self::Dot => "dot",
        }
    }
}

fn resolve_session_graph_format(path: &Path) -> SessionGraphFormat {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("dot") {
        SessionGraphFormat::Dot
    } else {
        SessionGraphFormat::Mermaid
    }
}

fn escape_graph_label(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn session_graph_node_label(entry: &crate::session::SessionEntry) -> String {
    format!(
        "{}: {} | {}",
        entry.id,
        session_message_role(&entry.message),
        session_message_preview(&entry.message)
    )
}

fn render_session_graph_mermaid(entries: &[crate::session::SessionEntry]) -> String {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| entry.id);

    let mut lines = vec!["graph TD".to_string()];
    if ordered.is_empty() {
        lines.push("  empty[\"(empty session)\"]".to_string());
        return lines.join("\n");
    }

    for entry in &ordered {
        lines.push(format!(
            "  n{}[\"{}\"]",
            entry.id,
            escape_graph_label(&session_graph_node_label(entry))
        ));
    }
    for entry in &ordered {
        if let Some(parent_id) = entry.parent_id {
            lines.push(format!("  n{} --> n{}", parent_id, entry.id));
        }
    }
    lines.join("\n")
}

fn render_session_graph_dot(entries: &[crate::session::SessionEntry]) -> String {
    let mut ordered = entries.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|entry| entry.id);

    let mut lines = vec!["digraph session {".to_string(), "  rankdir=LR;".to_string()];
    if ordered.is_empty() {
        lines.push("  empty [label=\"(empty session)\"];".to_string());
    } else {
        for entry in &ordered {
            lines.push(format!(
                "  n{} [label=\"{}\"];",
                entry.id,
                escape_graph_label(&session_graph_node_label(entry))
            ));
        }
        for entry in &ordered {
            if let Some(parent_id) = entry.parent_id {
                lines.push(format!("  n{} -> n{};", parent_id, entry.id));
            }
        }
    }
    lines.push("}".to_string());
    lines.join("\n")
}

fn render_session_graph(
    format: SessionGraphFormat,
    entries: &[crate::session::SessionEntry],
) -> String {
    match format {
        SessionGraphFormat::Mermaid => render_session_graph_mermaid(entries),
        SessionGraphFormat::Dot => render_session_graph_dot(entries),
    }
}

fn write_text_atomic(path: &Path, content: &str) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("destination path cannot be empty");
    }
    if path.exists() && path.is_dir() {
        bail!("destination path '{}' is a directory", path.display());
    }

    let parent_dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent_dir)
        .with_context(|| format!("failed to create {}", parent_dir.display()))?;

    let temp_name = format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("session-graph"),
        std::process::id(),
        current_unix_timestamp()
    );
    let temp_path = parent_dir.join(temp_name);
    std::fs::write(&temp_path, content)
        .with_context(|| format!("failed to write temporary file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "failed to rename temporary graph file {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn execute_session_graph_export_command(runtime: &SessionRuntime, command_args: &str) -> String {
    let destination = PathBuf::from(command_args.trim());
    let format = resolve_session_graph_format(&destination);
    let graph = render_session_graph(format, runtime.store.entries());
    let nodes = runtime.store.entries().len();
    let edges = runtime
        .store
        .entries()
        .iter()
        .filter(|entry| entry.parent_id.is_some())
        .count();

    match write_text_atomic(&destination, &graph) {
        Ok(()) => format!(
            "session graph export: path={} format={} nodes={} edges={}",
            destination.display(),
            format.as_str(),
            nodes,
            edges
        ),
        Err(error) => format!(
            "session graph export error: path={} error={error}",
            destination.display()
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn as_str(self) -> &'static str {
        match self {
            DoctorStatus::Pass => "pass",
            DoctorStatus::Warn => "warn",
            DoctorStatus::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorCheckResult {
    key: String,
    status: DoctorStatus,
    code: String,
    path: Option<String>,
    action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorCommandOutputFormat {
    Text,
    Json,
}

fn parse_doctor_command_args(command_args: &str) -> Result<DoctorCommandOutputFormat> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok(DoctorCommandOutputFormat::Text);
    }
    if tokens.len() == 1 && tokens[0] == "--json" {
        return Ok(DoctorCommandOutputFormat::Json);
    }
    bail!("usage: /doctor [--json]");
}

fn run_doctor_checks(config: &DoctorCommandConfig) -> Vec<DoctorCheckResult> {
    let mut checks = Vec::new();
    checks.push(DoctorCheckResult {
        key: "model".to_string(),
        status: DoctorStatus::Pass,
        code: config.model.clone(),
        path: None,
        action: None,
    });

    for provider_check in &config.provider_keys {
        let mode_status = if provider_check.mode_supported {
            DoctorStatus::Pass
        } else {
            DoctorStatus::Fail
        };
        checks.push(DoctorCheckResult {
            key: format!("provider_auth_mode.{}", provider_check.provider),
            status: mode_status,
            code: provider_check.auth_mode.as_str().to_string(),
            path: None,
            action: if provider_check.mode_supported {
                None
            } else {
                Some(format!(
                    "set {} api-key",
                    provider_auth_mode_flag(provider_check.provider_kind)
                ))
            },
        });

        let (status, code, action) = if provider_check.auth_mode == ProviderAuthMethod::ApiKey {
            if provider_check.present {
                (DoctorStatus::Pass, "present".to_string(), None)
            } else {
                (
                    DoctorStatus::Fail,
                    "missing".to_string(),
                    Some(format!("set {}", provider_check.key_env_var)),
                )
            }
        } else {
            (
                DoctorStatus::Warn,
                "not_required_for_mode".to_string(),
                None,
            )
        };
        checks.push(DoctorCheckResult {
            key: format!("provider_key.{}", provider_check.provider),
            status,
            code,
            path: None,
            action,
        });
    }

    if !config.session_enabled {
        checks.push(DoctorCheckResult {
            key: "session_path".to_string(),
            status: DoctorStatus::Warn,
            code: "session_disabled".to_string(),
            path: Some(config.session_path.display().to_string()),
            action: Some("omit --no-session to enable persistence".to_string()),
        });
    } else if config.session_path.exists() {
        match std::fs::metadata(&config.session_path) {
            Ok(metadata) if metadata.is_file() => checks.push(DoctorCheckResult {
                key: "session_path".to_string(),
                status: DoctorStatus::Pass,
                code: "readable".to_string(),
                path: Some(config.session_path.display().to_string()),
                action: None,
            }),
            Ok(_) => checks.push(DoctorCheckResult {
                key: "session_path".to_string(),
                status: DoctorStatus::Fail,
                code: "not_file".to_string(),
                path: Some(config.session_path.display().to_string()),
                action: Some("choose a regular file path for --session".to_string()),
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "session_path".to_string(),
                status: DoctorStatus::Fail,
                code: format!("metadata_error:{error}"),
                path: Some(config.session_path.display().to_string()),
                action: Some("fix session path permissions".to_string()),
            }),
        }
    } else {
        let parent_exists = config
            .session_path
            .parent()
            .map(|parent| parent.exists())
            .unwrap_or(false);
        checks.push(DoctorCheckResult {
            key: "session_path".to_string(),
            status: if parent_exists {
                DoctorStatus::Warn
            } else {
                DoctorStatus::Fail
            },
            code: if parent_exists {
                "missing_will_create".to_string()
            } else {
                "missing_parent".to_string()
            },
            path: Some(config.session_path.display().to_string()),
            action: if parent_exists {
                Some("run a prompt or command to create the session file".to_string())
            } else {
                Some("create the parent directory for --session".to_string())
            },
        });
    }

    if config.skills_dir.exists() {
        match std::fs::metadata(&config.skills_dir) {
            Ok(metadata) if metadata.is_dir() => checks.push(DoctorCheckResult {
                key: "skills_dir".to_string(),
                status: DoctorStatus::Pass,
                code: "readable_dir".to_string(),
                path: Some(config.skills_dir.display().to_string()),
                action: None,
            }),
            Ok(_) => checks.push(DoctorCheckResult {
                key: "skills_dir".to_string(),
                status: DoctorStatus::Fail,
                code: "not_dir".to_string(),
                path: Some(config.skills_dir.display().to_string()),
                action: Some("set --skills-dir to an existing directory".to_string()),
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "skills_dir".to_string(),
                status: DoctorStatus::Fail,
                code: format!("metadata_error:{error}"),
                path: Some(config.skills_dir.display().to_string()),
                action: Some("fix skills directory permissions".to_string()),
            }),
        }
    } else {
        checks.push(DoctorCheckResult {
            key: "skills_dir".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(config.skills_dir.display().to_string()),
            action: Some("create --skills-dir or install at least one skill".to_string()),
        });
    }

    if config.skills_lock_path.exists() {
        match std::fs::read_to_string(&config.skills_lock_path) {
            Ok(_) => checks.push(DoctorCheckResult {
                key: "skills_lock".to_string(),
                status: DoctorStatus::Pass,
                code: "readable".to_string(),
                path: Some(config.skills_lock_path.display().to_string()),
                action: None,
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "skills_lock".to_string(),
                status: DoctorStatus::Fail,
                code: format!("read_error:{error}"),
                path: Some(config.skills_lock_path.display().to_string()),
                action: Some("fix lockfile permissions or regenerate lockfile".to_string()),
            }),
        }
    } else {
        checks.push(DoctorCheckResult {
            key: "skills_lock".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(config.skills_lock_path.display().to_string()),
            action: Some("run /skills-lock-write to generate lockfile".to_string()),
        });
    }

    match config.trust_root_path.as_ref() {
        Some(path) if path.exists() => match std::fs::read_to_string(path) {
            Ok(_) => checks.push(DoctorCheckResult {
                key: "trust_root".to_string(),
                status: DoctorStatus::Pass,
                code: "readable".to_string(),
                path: Some(path.display().to_string()),
                action: None,
            }),
            Err(error) => checks.push(DoctorCheckResult {
                key: "trust_root".to_string(),
                status: DoctorStatus::Fail,
                code: format!("read_error:{error}"),
                path: Some(path.display().to_string()),
                action: Some("fix trust-root file permissions".to_string()),
            }),
        },
        Some(path) => checks.push(DoctorCheckResult {
            key: "trust_root".to_string(),
            status: DoctorStatus::Warn,
            code: "missing".to_string(),
            path: Some(path.display().to_string()),
            action: Some("create trust-root file or adjust --skill-trust-root-file".to_string()),
        }),
        None => checks.push(DoctorCheckResult {
            key: "trust_root".to_string(),
            status: DoctorStatus::Warn,
            code: "not_configured".to_string(),
            path: None,
            action: Some("configure --skill-trust-root-file when using signed skills".to_string()),
        }),
    }

    checks
}

fn render_doctor_report(checks: &[DoctorCheckResult]) -> String {
    let pass = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .count();

    let mut lines = vec![format!(
        "doctor summary: checks={} pass={} warn={} fail={}",
        checks.len(),
        pass,
        warn,
        fail
    )];

    for check in checks {
        lines.push(format!(
            "doctor check: key={} status={} code={} path={} action={}",
            check.key,
            check.status.as_str(),
            check.code,
            check.path.as_deref().unwrap_or("none"),
            check.action.as_deref().unwrap_or("none")
        ));
    }

    lines.join("\n")
}

fn render_doctor_report_json(checks: &[DoctorCheckResult]) -> String {
    let pass = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Pass)
        .count();
    let warn = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Warn)
        .count();
    let fail = checks
        .iter()
        .filter(|item| item.status == DoctorStatus::Fail)
        .count();

    serde_json::json!({
        "summary": {
            "checks": checks.len(),
            "pass": pass,
            "warn": warn,
            "fail": fail,
        },
        "checks": checks
            .iter()
            .map(|check| {
                serde_json::json!({
                    "key": check.key,
                    "status": check.status.as_str(),
                    "code": check.code,
                    "path": check.path,
                    "action": check.action,
                })
            })
            .collect::<Vec<_>>()
    })
    .to_string()
}

fn execute_doctor_command(
    config: &DoctorCommandConfig,
    format: DoctorCommandOutputFormat,
) -> String {
    let checks = run_doctor_checks(config);
    match format {
        DoctorCommandOutputFormat::Text => render_doctor_report(&checks),
        DoctorCommandOutputFormat::Json => render_doctor_report_json(&checks),
    }
}

const AUTH_USAGE: &str = "usage: /auth <login|status|logout> ...";
const AUTH_LOGIN_USAGE: &str = "usage: /auth login <provider> [--mode <mode>] [--json]";
const AUTH_STATUS_USAGE: &str = "usage: /auth status [provider] [--json]";
const AUTH_LOGOUT_USAGE: &str = "usage: /auth logout <provider> [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthCommand {
    Login {
        provider: Provider,
        mode: Option<ProviderAuthMethod>,
        json_output: bool,
    },
    Status {
        provider: Option<Provider>,
        json_output: bool,
    },
    Logout {
        provider: Provider,
        json_output: bool,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct AuthStatusRow {
    provider: String,
    mode: String,
    mode_supported: bool,
    available: bool,
    state: String,
    source: String,
    reason: String,
    expires_unix: Option<u64>,
    revoked: bool,
}

fn parse_auth_provider(token: &str) -> Result<Provider> {
    match token.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(Provider::OpenAi),
        "anthropic" => Ok(Provider::Anthropic),
        "google" => Ok(Provider::Google),
        other => bail!(
            "unknown provider '{}'; supported providers: openai, anthropic, google",
            other
        ),
    }
}

fn parse_provider_auth_method_token(token: &str) -> Result<ProviderAuthMethod> {
    match token.trim().to_ascii_lowercase().as_str() {
        "api-key" | "api_key" => Ok(ProviderAuthMethod::ApiKey),
        "oauth-token" | "oauth_token" => Ok(ProviderAuthMethod::OauthToken),
        "adc" => Ok(ProviderAuthMethod::Adc),
        "session-token" | "session_token" => Ok(ProviderAuthMethod::SessionToken),
        other => bail!(
            "unknown auth mode '{}'; supported modes: api-key, oauth-token, adc, session-token",
            other
        ),
    }
}

fn parse_auth_command(command_args: &str) -> Result<AuthCommand> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{AUTH_USAGE}");
    }

    match tokens[0] {
        "login" => {
            if tokens.len() < 2 {
                bail!("{AUTH_LOGIN_USAGE}");
            }
            let provider = parse_auth_provider(tokens[1])?;
            let mut mode = None;
            let mut json_output = false;

            let mut index = 2usize;
            while index < tokens.len() {
                match tokens[index] {
                    "--json" => {
                        json_output = true;
                        index += 1;
                    }
                    "--mode" => {
                        if mode.is_some() {
                            bail!("duplicate --mode flag; {AUTH_LOGIN_USAGE}");
                        }
                        let Some(raw_mode) = tokens.get(index + 1) else {
                            bail!("missing auth mode after --mode; {AUTH_LOGIN_USAGE}");
                        };
                        mode = Some(parse_provider_auth_method_token(raw_mode)?);
                        index += 2;
                    }
                    other => bail!("unexpected argument '{}'; {AUTH_LOGIN_USAGE}", other),
                }
            }

            Ok(AuthCommand::Login {
                provider,
                mode,
                json_output,
            })
        }
        "status" => {
            let mut provider: Option<Provider> = None;
            let mut json_output = false;
            for token in tokens.into_iter().skip(1) {
                if token == "--json" {
                    json_output = true;
                    continue;
                }
                if provider.is_some() {
                    bail!("unexpected argument '{}'; {AUTH_STATUS_USAGE}", token);
                }
                provider = Some(parse_auth_provider(token)?);
            }
            Ok(AuthCommand::Status {
                provider,
                json_output,
            })
        }
        "logout" => {
            if tokens.len() < 2 {
                bail!("{AUTH_LOGOUT_USAGE}");
            }
            let provider = parse_auth_provider(tokens[1])?;
            let mut json_output = false;
            for token in tokens.into_iter().skip(2) {
                if token == "--json" {
                    json_output = true;
                } else {
                    bail!("unexpected argument '{}'; {AUTH_LOGOUT_USAGE}", token);
                }
            }
            Ok(AuthCommand::Logout {
                provider,
                json_output,
            })
        }
        other => bail!("unknown subcommand '{}'; {AUTH_USAGE}", other),
    }
}

fn provider_api_key_candidates_from_auth_config(
    config: &AuthCommandConfig,
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    provider_api_key_candidates_with_inputs(
        provider,
        config.api_key.clone(),
        config.openai_api_key.clone(),
        config.anthropic_api_key.clone(),
        config.google_api_key.clone(),
    )
}

fn provider_login_access_token_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "PI_AUTH_ACCESS_TOKEN",
                std::env::var("PI_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "OPENAI_ACCESS_TOKEN",
                std::env::var("OPENAI_ACCESS_TOKEN").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "PI_AUTH_ACCESS_TOKEN",
                std::env::var("PI_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "ANTHROPIC_ACCESS_TOKEN",
                std::env::var("ANTHROPIC_ACCESS_TOKEN").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "PI_AUTH_ACCESS_TOKEN",
                std::env::var("PI_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "GOOGLE_ACCESS_TOKEN",
                std::env::var("GOOGLE_ACCESS_TOKEN").ok(),
            ),
        ],
    }
}

fn provider_login_refresh_token_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "PI_AUTH_REFRESH_TOKEN",
                std::env::var("PI_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "OPENAI_REFRESH_TOKEN",
                std::env::var("OPENAI_REFRESH_TOKEN").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "PI_AUTH_REFRESH_TOKEN",
                std::env::var("PI_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "ANTHROPIC_REFRESH_TOKEN",
                std::env::var("ANTHROPIC_REFRESH_TOKEN").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "PI_AUTH_REFRESH_TOKEN",
                std::env::var("PI_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "GOOGLE_REFRESH_TOKEN",
                std::env::var("GOOGLE_REFRESH_TOKEN").ok(),
            ),
        ],
    }
}

fn provider_login_expires_candidates(provider: Provider) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "PI_AUTH_EXPIRES_UNIX",
                std::env::var("PI_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "OPENAI_AUTH_EXPIRES_UNIX",
                std::env::var("OPENAI_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "PI_AUTH_EXPIRES_UNIX",
                std::env::var("PI_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "ANTHROPIC_AUTH_EXPIRES_UNIX",
                std::env::var("ANTHROPIC_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "PI_AUTH_EXPIRES_UNIX",
                std::env::var("PI_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "GOOGLE_AUTH_EXPIRES_UNIX",
                std::env::var("GOOGLE_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
    }
}

fn resolve_auth_login_expires_unix(provider: Provider) -> Result<Option<u64>> {
    for (source, value) in provider_login_expires_candidates(provider) {
        let Some(value) = value else {
            continue;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = trimmed
            .parse::<u64>()
            .with_context(|| format!("invalid unix timestamp in {}", source))?;
        return Ok(Some(parsed));
    }
    Ok(None)
}

fn execute_auth_login_command(
    config: &AuthCommandConfig,
    provider: Provider,
    mode_override: Option<ProviderAuthMethod>,
    json_output: bool,
) -> String {
    let mode = mode_override
        .unwrap_or_else(|| configured_provider_auth_method_from_config(config, provider));
    let capability = provider_auth_capability(provider, mode);
    if !capability.supported {
        let reason = format!(
            "auth mode '{}' is not supported for provider '{}': {}",
            mode.as_str(),
            provider.as_str(),
            capability.reason
        );
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": provider.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} error={reason}",
            provider.as_str(),
            mode.as_str()
        );
    }

    match mode {
        ProviderAuthMethod::ApiKey => {
            match resolve_non_empty_secret_with_source(
                provider_api_key_candidates_from_auth_config(config, provider),
            ) {
                Some((_secret, source)) => {
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "ready",
                            "source": source,
                            "persisted": false,
                        })
                        .to_string();
                    }
                    format!(
                        "auth login: provider={} mode={} status=ready source={} persisted=false",
                        provider.as_str(),
                        mode.as_str(),
                        source
                    )
                }
                None => {
                    let reason = missing_provider_api_key_message(provider).to_string();
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "error",
                            "reason": reason,
                        })
                        .to_string();
                    }
                    format!(
                        "auth login error: provider={} mode={} error={reason}",
                        provider.as_str(),
                        mode.as_str()
                    )
                }
            }
        }
        ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken => {
            let Some((access_token, access_source)) = resolve_non_empty_secret_with_source(
                provider_login_access_token_candidates(provider),
            ) else {
                let reason = "missing access token for login. Set PI_AUTH_ACCESS_TOKEN or provider-specific *_ACCESS_TOKEN env var".to_string();
                if json_output {
                    return serde_json::json!({
                        "command": "auth.login",
                        "provider": provider.as_str(),
                        "mode": mode.as_str(),
                        "status": "error",
                        "reason": reason,
                    })
                    .to_string();
                }
                return format!(
                    "auth login error: provider={} mode={} error={reason}",
                    provider.as_str(),
                    mode.as_str()
                );
            };

            let refresh_token = resolve_non_empty_secret_with_source(
                provider_login_refresh_token_candidates(provider),
            )
            .map(|(secret, _source)| secret);
            let expires_unix = match resolve_auth_login_expires_unix(provider)
                .map(|value| value.unwrap_or_else(|| current_unix_timestamp().saturating_add(3600)))
            {
                Ok(value) => value,
                Err(error) => {
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "error",
                            "reason": error.to_string(),
                        })
                        .to_string();
                    }
                    return format!(
                        "auth login error: provider={} mode={} error={error}",
                        provider.as_str(),
                        mode.as_str()
                    );
                }
            };

            let mut store = match load_credential_store(
                &config.credential_store,
                config.credential_store_encryption,
                config.credential_store_key.as_deref(),
            ) {
                Ok(store) => store,
                Err(error) => {
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "error",
                            "reason": error.to_string(),
                        })
                        .to_string();
                    }
                    return format!(
                        "auth login error: provider={} mode={} error={error}",
                        provider.as_str(),
                        mode.as_str()
                    );
                }
            };
            store.providers.insert(
                provider.as_str().to_string(),
                ProviderCredentialStoreRecord {
                    auth_method: mode,
                    access_token: Some(access_token),
                    refresh_token,
                    expires_unix: Some(expires_unix),
                    revoked: false,
                },
            );
            if let Err(error) = save_credential_store(
                &config.credential_store,
                &store,
                config.credential_store_key.as_deref(),
            ) {
                if json_output {
                    return serde_json::json!({
                        "command": "auth.login",
                        "provider": provider.as_str(),
                        "mode": mode.as_str(),
                        "status": "error",
                        "reason": error.to_string(),
                    })
                    .to_string();
                }
                return format!(
                    "auth login error: provider={} mode={} error={error}",
                    provider.as_str(),
                    mode.as_str()
                );
            }

            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": provider.as_str(),
                    "mode": mode.as_str(),
                    "status": "saved",
                    "source": access_source,
                    "credential_store": config.credential_store.display().to_string(),
                    "expires_unix": expires_unix,
                })
                .to_string();
            }
            format!(
                "auth login: provider={} mode={} status=saved source={} credential_store={} expires_unix={}",
                provider.as_str(),
                mode.as_str(),
                access_source,
                config.credential_store.display(),
                expires_unix
            )
        }
        ProviderAuthMethod::Adc => {
            let reason = "adc login flow is not implemented".to_string();
            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": provider.as_str(),
                    "mode": mode.as_str(),
                    "status": "error",
                    "reason": reason,
                })
                .to_string();
            }
            format!(
                "auth login error: provider={} mode={} error={reason}",
                provider.as_str(),
                mode.as_str()
            )
        }
    }
}

fn auth_status_row_for_provider(
    config: &AuthCommandConfig,
    provider: Provider,
    store: Option<&CredentialStoreData>,
    store_error: Option<&str>,
) -> AuthStatusRow {
    let mode = configured_provider_auth_method_from_config(config, provider);
    let capability = provider_auth_capability(provider, mode);
    if !capability.supported {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: false,
            available: false,
            state: "unsupported_mode".to_string(),
            source: "none".to_string(),
            reason: capability.reason.to_string(),
            expires_unix: None,
            revoked: false,
        };
    }

    if mode == ProviderAuthMethod::ApiKey {
        if let Some((_secret, source)) = resolve_non_empty_secret_with_source(
            provider_api_key_candidates_from_auth_config(config, provider),
        ) {
            return AuthStatusRow {
                provider: provider.as_str().to_string(),
                mode: mode.as_str().to_string(),
                mode_supported: true,
                available: true,
                state: "ready".to_string(),
                source,
                reason: "api_key_available".to_string(),
                expires_unix: None,
                revoked: false,
            };
        }
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_api_key".to_string(),
            source: "none".to_string(),
            reason: missing_provider_api_key_message(provider).to_string(),
            expires_unix: None,
            revoked: false,
        };
    }

    if let Some(error) = store_error {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "store_error".to_string(),
            source: "none".to_string(),
            reason: error.to_string(),
            expires_unix: None,
            revoked: false,
        };
    }

    let Some(store) = store else {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_credential_store".to_string(),
            source: "none".to_string(),
            reason: "credential store is unavailable".to_string(),
            expires_unix: None,
            revoked: false,
        };
    };

    let Some(entry) = store.providers.get(provider.as_str()) else {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_credential".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry is missing".to_string(),
            expires_unix: None,
            revoked: false,
        };
    };
    if entry.auth_method != mode {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "mode_mismatch".to_string(),
            source: "credential_store".to_string(),
            reason: format!(
                "credential store entry mode '{}' does not match configured mode '{}'",
                entry.auth_method.as_str(),
                mode.as_str()
            ),
            expires_unix: entry.expires_unix,
            revoked: entry.revoked,
        };
    }
    if entry.revoked {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "revoked".to_string(),
            source: "credential_store".to_string(),
            reason: "credential has been revoked".to_string(),
            expires_unix: entry.expires_unix,
            revoked: true,
        };
    }
    if entry
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_access_token".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry has no access token".to_string(),
            expires_unix: entry.expires_unix,
            revoked: false,
        };
    }

    let now_unix = current_unix_timestamp();
    if entry
        .expires_unix
        .map(|value| value <= now_unix)
        .unwrap_or(false)
    {
        let refresh_pending = entry
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some();
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: if refresh_pending {
                "expired_refresh_pending".to_string()
            } else {
                "expired".to_string()
            },
            source: "credential_store".to_string(),
            reason: if refresh_pending {
                "access token expired; refresh will run on next provider use".to_string()
            } else {
                "access token expired and no refresh token is available".to_string()
            },
            expires_unix: entry.expires_unix,
            revoked: false,
        };
    }

    AuthStatusRow {
        provider: provider.as_str().to_string(),
        mode: mode.as_str().to_string(),
        mode_supported: true,
        available: true,
        state: "ready".to_string(),
        source: "credential_store".to_string(),
        reason: "credential available".to_string(),
        expires_unix: entry.expires_unix,
        revoked: false,
    }
}

fn execute_auth_status_command(
    config: &AuthCommandConfig,
    provider: Option<Provider>,
    json_output: bool,
) -> String {
    let providers = if let Some(provider) = provider {
        vec![provider]
    } else {
        vec![Provider::OpenAi, Provider::Anthropic, Provider::Google]
    };

    let requires_store = providers.iter().any(|provider| {
        configured_provider_auth_method_from_config(config, *provider) != ProviderAuthMethod::ApiKey
    });
    let (store, store_error) = if requires_store {
        match load_credential_store(
            &config.credential_store,
            config.credential_store_encryption,
            config.credential_store_key.as_deref(),
        ) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };

    let rows = providers
        .iter()
        .map(|provider| {
            auth_status_row_for_provider(config, *provider, store.as_ref(), store_error.as_deref())
        })
        .collect::<Vec<_>>();
    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);

    if json_output {
        return serde_json::json!({
            "command": "auth.status",
            "providers": rows.len(),
            "available": available,
            "unavailable": unavailable,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "auth status: providers={} available={} unavailable={}",
        rows.len(),
        available,
        unavailable
    )];
    for row in rows {
        lines.push(format!(
            "auth provider: name={} mode={} mode_supported={} available={} state={} source={} reason={} expires_unix={} revoked={}",
            row.provider,
            row.mode,
            row.mode_supported,
            row.available,
            row.state,
            row.source,
            row.reason,
            row.expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.revoked
        ));
    }
    lines.join("\n")
}

fn execute_auth_logout_command(
    config: &AuthCommandConfig,
    provider: Provider,
    json_output: bool,
) -> String {
    let mut store = match load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    ) {
        Ok(store) => store,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": "auth.logout",
                    "provider": provider.as_str(),
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return format!(
                "auth logout error: provider={} error={error}",
                provider.as_str()
            );
        }
    };

    let status = if let Some(entry) = store.providers.get_mut(provider.as_str()) {
        entry.revoked = true;
        entry.access_token = None;
        entry.refresh_token = None;
        entry.expires_unix = None;
        "revoked"
    } else {
        "not_found"
    };

    if status == "revoked" {
        if let Err(error) = save_credential_store(
            &config.credential_store,
            &store,
            config.credential_store_key.as_deref(),
        ) {
            if json_output {
                return serde_json::json!({
                    "command": "auth.logout",
                    "provider": provider.as_str(),
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return format!(
                "auth logout error: provider={} error={error}",
                provider.as_str()
            );
        }
    }

    if json_output {
        return serde_json::json!({
            "command": "auth.logout",
            "provider": provider.as_str(),
            "status": status,
            "credential_store": config.credential_store.display().to_string(),
        })
        .to_string();
    }

    format!(
        "auth logout: provider={} status={} credential_store={}",
        provider.as_str(),
        status,
        config.credential_store.display()
    )
}

fn execute_auth_command(config: &AuthCommandConfig, command_args: &str) -> String {
    let command = match parse_auth_command(command_args) {
        Ok(command) => command,
        Err(error) => return format!("auth error: {error}"),
    };

    match command {
        AuthCommand::Login {
            provider,
            mode,
            json_output,
        } => execute_auth_login_command(config, provider, mode, json_output),
        AuthCommand::Status {
            provider,
            json_output,
        } => execute_auth_status_command(config, provider, json_output),
        AuthCommand::Logout {
            provider,
            json_output,
        } => execute_auth_logout_command(config, provider, json_output),
    }
}

const MACRO_SCHEMA_VERSION: u32 = 1;
const MACRO_USAGE: &str = "usage: /macro <save|run|list|show|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
enum MacroCommand {
    List,
    Save {
        name: String,
        commands_file: PathBuf,
    },
    Run {
        name: String,
        dry_run: bool,
    },
    Show {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MacroFile {
    schema_version: u32,
    macros: BTreeMap<String, Vec<String>>,
}

fn default_macro_config_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".pi")
        .join("macros.json"))
}

fn validate_macro_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("macro name must not be empty");
    };
    if !first.is_ascii_alphabetic() {
        bail!("macro name '{}' must start with an ASCII letter", name);
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) {
        bail!(
            "macro name '{}' must contain only ASCII letters, digits, '-' or '_'",
            name
        );
    }
    Ok(())
}

fn parse_macro_command(command_args: &str) -> Result<MacroCommand> {
    const USAGE_LIST: &str = "usage: /macro list";
    const USAGE_SAVE: &str = "usage: /macro save <name> <commands_file>";
    const USAGE_RUN: &str = "usage: /macro run <name> [--dry-run]";
    const USAGE_SHOW: &str = "usage: /macro show <name>";
    const USAGE_DELETE: &str = "usage: /macro delete <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{MACRO_USAGE}");
    }

    match tokens[0] {
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(MacroCommand::List)
        }
        "save" => {
            if tokens.len() != 3 {
                bail!("{USAGE_SAVE}");
            }
            validate_macro_name(tokens[1])?;
            Ok(MacroCommand::Save {
                name: tokens[1].to_string(),
                commands_file: PathBuf::from(tokens[2]),
            })
        }
        "run" => {
            if !(2..=3).contains(&tokens.len()) {
                bail!("{USAGE_RUN}");
            }
            validate_macro_name(tokens[1])?;
            let dry_run = if tokens.len() == 3 {
                if tokens[2] != "--dry-run" {
                    bail!("{USAGE_RUN}");
                }
                true
            } else {
                false
            };
            Ok(MacroCommand::Run {
                name: tokens[1].to_string(),
                dry_run,
            })
        }
        "show" => {
            if tokens.len() != 2 {
                bail!("{USAGE_SHOW}");
            }
            validate_macro_name(tokens[1])?;
            Ok(MacroCommand::Show {
                name: tokens[1].to_string(),
            })
        }
        "delete" => {
            if tokens.len() != 2 {
                bail!("{USAGE_DELETE}");
            }
            validate_macro_name(tokens[1])?;
            Ok(MacroCommand::Delete {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {MACRO_USAGE}", other),
    }
}

fn load_macro_file(path: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read macro file {}", path.display()))?;
    let parsed = serde_json::from_str::<MacroFile>(&raw)
        .with_context(|| format!("failed to parse macro file {}", path.display()))?;
    if parsed.schema_version != MACRO_SCHEMA_VERSION {
        bail!(
            "unsupported macro schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            MACRO_SCHEMA_VERSION
        );
    }
    Ok(parsed.macros)
}

fn save_macro_file(path: &Path, macros: &BTreeMap<String, Vec<String>>) -> Result<()> {
    let payload = MacroFile {
        schema_version: MACRO_SCHEMA_VERSION,
        macros: macros.clone(),
    };
    let mut encoded = serde_json::to_string_pretty(&payload).context("failed to encode macros")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "macro config path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create macro config directory {}",
            parent.display()
        )
    })?;
    write_text_atomic(path, &encoded)
}

fn load_macro_commands(commands_file: &Path) -> Result<Vec<String>> {
    let raw = std::fs::read_to_string(commands_file)
        .with_context(|| format!("failed to read commands file {}", commands_file.display()))?;
    let commands = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if commands.is_empty() {
        bail!(
            "commands file {} does not contain runnable commands",
            commands_file.display()
        );
    }
    Ok(commands)
}

fn validate_macro_command_entry(command: &str) -> Result<()> {
    let parsed = parse_command(command)
        .ok_or_else(|| anyhow!("invalid macro command '{command}': command must start with '/'"))?;
    let name = canonical_command_name(parsed.name);
    if !COMMAND_NAMES.contains(&name) {
        bail!("invalid macro command '{command}': unknown command '{name}'");
    }
    if matches!(name, "/quit" | "/exit") {
        bail!("invalid macro command '{command}': exit commands are not allowed");
    }
    if name == "/macro" {
        bail!("invalid macro command '{command}': nested /macro commands are not allowed");
    }
    Ok(())
}

fn validate_macro_commands(commands: &[String]) -> Result<()> {
    for (index, command) in commands.iter().enumerate() {
        validate_macro_command_entry(command)
            .with_context(|| format!("macro command #{index} failed validation"))?;
    }
    Ok(())
}

fn render_macro_list(path: &Path, macros: &BTreeMap<String, Vec<String>>) -> String {
    let mut lines = vec![format!(
        "macro list: path={} count={}",
        path.display(),
        macros.len()
    )];
    if macros.is_empty() {
        lines.push("macros: none".to_string());
        return lines.join("\n");
    }
    for (name, commands) in macros {
        lines.push(format!("macro: name={} commands={}", name, commands.len()));
    }
    lines.join("\n")
}

fn render_macro_show(path: &Path, name: &str, commands: &[String]) -> String {
    let mut lines = vec![format!(
        "macro show: path={} name={} commands={}",
        path.display(),
        name,
        commands.len()
    )];
    for (index, command) in commands.iter().enumerate() {
        lines.push(format!("command: index={} value={command}", index));
    }
    lines.join("\n")
}

fn execute_macro_command(
    command_args: &str,
    macro_path: &Path,
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    command_context: CommandExecutionContext<'_>,
) -> String {
    let command = match parse_macro_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!("macro error: path={} error={error}", macro_path.display());
        }
    };

    let mut macros = match load_macro_file(macro_path) {
        Ok(macros) => macros,
        Err(error) => {
            return format!("macro error: path={} error={error}", macro_path.display());
        }
    };

    match command {
        MacroCommand::List => render_macro_list(macro_path, &macros),
        MacroCommand::Save {
            name,
            commands_file,
        } => {
            let commands = match load_macro_commands(&commands_file) {
                Ok(commands) => commands,
                Err(error) => {
                    return format!(
                        "macro error: path={} name={} error={error}",
                        macro_path.display(),
                        name
                    );
                }
            };
            if let Err(error) = validate_macro_commands(&commands) {
                return format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                );
            }
            macros.insert(name.clone(), commands.clone());
            match save_macro_file(macro_path, &macros) {
                Ok(()) => format!(
                    "macro save: path={} name={} source={} commands={}",
                    macro_path.display(),
                    name,
                    commands_file.display(),
                    commands.len()
                ),
                Err(error) => format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                ),
            }
        }
        MacroCommand::Run { name, dry_run } => {
            let Some(commands) = macros.get(&name) else {
                return format!(
                    "macro error: path={} name={} error=unknown macro '{}'",
                    macro_path.display(),
                    name,
                    name
                );
            };
            if let Err(error) = validate_macro_commands(commands) {
                return format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                );
            }
            if dry_run {
                let mut lines = vec![format!(
                    "macro run: path={} name={} mode=dry-run commands={}",
                    macro_path.display(),
                    name,
                    commands.len()
                )];
                for command in commands {
                    lines.push(format!("plan: command={command}"));
                }
                return lines.join("\n");
            }

            for command in commands {
                match handle_command_with_session_import_mode(
                    command,
                    agent,
                    session_runtime,
                    command_context.tool_policy_json,
                    command_context.session_import_mode,
                    command_context.profile_defaults,
                    command_context.skills_command_config,
                    command_context.auth_command_config,
                ) {
                    Ok(CommandAction::Continue) => {}
                    Ok(CommandAction::Exit) => {
                        return format!(
                            "macro error: path={} name={} error=exit command is not allowed in macros",
                            macro_path.display(),
                            name
                        );
                    }
                    Err(error) => {
                        return format!(
                            "macro error: path={} name={} command={} error={error}",
                            macro_path.display(),
                            name,
                            command
                        );
                    }
                }
            }

            format!(
                "macro run: path={} name={} mode=apply commands={} executed={}",
                macro_path.display(),
                name,
                commands.len(),
                commands.len()
            )
        }
        MacroCommand::Show { name } => {
            let Some(commands) = macros.get(&name) else {
                return format!(
                    "macro error: path={} name={} error=unknown macro '{}'",
                    macro_path.display(),
                    name,
                    name
                );
            };
            render_macro_show(macro_path, &name, commands)
        }
        MacroCommand::Delete { name } => {
            if !macros.contains_key(&name) {
                return format!(
                    "macro error: path={} name={} error=unknown macro '{}'",
                    macro_path.display(),
                    name,
                    name
                );
            }

            macros.remove(&name);
            match save_macro_file(macro_path, &macros) {
                Ok(()) => format!(
                    "macro delete: path={} name={} status=deleted remaining={}",
                    macro_path.display(),
                    name,
                    macros.len()
                ),
                Err(error) => format!(
                    "macro error: path={} name={} error={error}",
                    macro_path.display(),
                    name
                ),
            }
        }
    }
}

const PROFILE_SCHEMA_VERSION: u32 = 1;
const PROFILE_USAGE: &str = "usage: /profile <save|load|list|show|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProfileCommand {
    Save { name: String },
    Load { name: String },
    List,
    Show { name: String },
    Delete { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProfileStoreFile {
    schema_version: u32,
    profiles: BTreeMap<String, ProfileDefaults>,
}

fn default_profile_store_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()
        .context("failed to resolve current working directory")?
        .join(".pi")
        .join("profiles.json"))
}

fn validate_profile_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("profile name must not be empty");
    };
    if !first.is_ascii_alphabetic() {
        bail!("profile name '{}' must start with an ASCII letter", name);
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) {
        bail!(
            "profile name '{}' must contain only ASCII letters, digits, '-' or '_'",
            name
        );
    }
    Ok(())
}

fn parse_profile_command(command_args: &str) -> Result<ProfileCommand> {
    const USAGE_SAVE: &str = "usage: /profile save <name>";
    const USAGE_LOAD: &str = "usage: /profile load <name>";
    const USAGE_LIST: &str = "usage: /profile list";
    const USAGE_SHOW: &str = "usage: /profile show <name>";
    const USAGE_DELETE: &str = "usage: /profile delete <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{PROFILE_USAGE}");
    }

    match tokens[0] {
        "save" => {
            if tokens.len() != 2 {
                bail!("{USAGE_SAVE}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Save {
                name: tokens[1].to_string(),
            })
        }
        "load" => {
            if tokens.len() != 2 {
                bail!("{USAGE_LOAD}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Load {
                name: tokens[1].to_string(),
            })
        }
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(ProfileCommand::List)
        }
        "show" => {
            if tokens.len() != 2 {
                bail!("{USAGE_SHOW}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Show {
                name: tokens[1].to_string(),
            })
        }
        "delete" => {
            if tokens.len() != 2 {
                bail!("{USAGE_DELETE}");
            }
            validate_profile_name(tokens[1])?;
            Ok(ProfileCommand::Delete {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {PROFILE_USAGE}", other),
    }
}

fn load_profile_store(path: &Path) -> Result<BTreeMap<String, ProfileDefaults>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read profile store {}", path.display()))?;
    let parsed = serde_json::from_str::<ProfileStoreFile>(&raw)
        .with_context(|| format!("failed to parse profile store {}", path.display()))?;
    if parsed.schema_version != PROFILE_SCHEMA_VERSION {
        bail!(
            "unsupported profile schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            PROFILE_SCHEMA_VERSION
        );
    }
    Ok(parsed.profiles)
}

fn save_profile_store(path: &Path, profiles: &BTreeMap<String, ProfileDefaults>) -> Result<()> {
    let payload = ProfileStoreFile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profiles: profiles.clone(),
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode profile store")?;
    encoded.push('\n');
    let parent = path.parent().ok_or_else(|| {
        anyhow!(
            "profile store path {} does not have a parent directory",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create profile directory {}", parent.display()))?;
    write_text_atomic(path, &encoded)
}

fn render_profile_diffs(current: &ProfileDefaults, loaded: &ProfileDefaults) -> Vec<String> {
    fn to_list(values: &[String]) -> String {
        if values.is_empty() {
            "none".to_string()
        } else {
            values.join(",")
        }
    }

    let mut diffs = Vec::new();
    if current.model != loaded.model {
        diffs.push(format!(
            "diff: field=model current={} loaded={}",
            current.model, loaded.model
        ));
    }
    if current.fallback_models != loaded.fallback_models {
        diffs.push(format!(
            "diff: field=fallback_models current={} loaded={}",
            to_list(&current.fallback_models),
            to_list(&loaded.fallback_models)
        ));
    }
    if current.session.enabled != loaded.session.enabled {
        diffs.push(format!(
            "diff: field=session.enabled current={} loaded={}",
            current.session.enabled, loaded.session.enabled
        ));
    }
    if current.session.path != loaded.session.path {
        diffs.push(format!(
            "diff: field=session.path current={} loaded={}",
            current.session.path.as_deref().unwrap_or("none"),
            loaded.session.path.as_deref().unwrap_or("none")
        ));
    }
    if current.session.import_mode != loaded.session.import_mode {
        diffs.push(format!(
            "diff: field=session.import_mode current={} loaded={}",
            current.session.import_mode, loaded.session.import_mode
        ));
    }
    if current.policy.tool_policy_preset != loaded.policy.tool_policy_preset {
        diffs.push(format!(
            "diff: field=policy.tool_policy_preset current={} loaded={}",
            current.policy.tool_policy_preset, loaded.policy.tool_policy_preset
        ));
    }
    if current.policy.bash_profile != loaded.policy.bash_profile {
        diffs.push(format!(
            "diff: field=policy.bash_profile current={} loaded={}",
            current.policy.bash_profile, loaded.policy.bash_profile
        ));
    }
    if current.policy.bash_dry_run != loaded.policy.bash_dry_run {
        diffs.push(format!(
            "diff: field=policy.bash_dry_run current={} loaded={}",
            current.policy.bash_dry_run, loaded.policy.bash_dry_run
        ));
    }
    if current.policy.os_sandbox_mode != loaded.policy.os_sandbox_mode {
        diffs.push(format!(
            "diff: field=policy.os_sandbox_mode current={} loaded={}",
            current.policy.os_sandbox_mode, loaded.policy.os_sandbox_mode
        ));
    }
    if current.policy.enforce_regular_files != loaded.policy.enforce_regular_files {
        diffs.push(format!(
            "diff: field=policy.enforce_regular_files current={} loaded={}",
            current.policy.enforce_regular_files, loaded.policy.enforce_regular_files
        ));
    }
    if current.policy.bash_timeout_ms != loaded.policy.bash_timeout_ms {
        diffs.push(format!(
            "diff: field=policy.bash_timeout_ms current={} loaded={}",
            current.policy.bash_timeout_ms, loaded.policy.bash_timeout_ms
        ));
    }
    if current.policy.max_command_length != loaded.policy.max_command_length {
        diffs.push(format!(
            "diff: field=policy.max_command_length current={} loaded={}",
            current.policy.max_command_length, loaded.policy.max_command_length
        ));
    }
    if current.policy.max_tool_output_bytes != loaded.policy.max_tool_output_bytes {
        diffs.push(format!(
            "diff: field=policy.max_tool_output_bytes current={} loaded={}",
            current.policy.max_tool_output_bytes, loaded.policy.max_tool_output_bytes
        ));
    }
    if current.policy.max_file_read_bytes != loaded.policy.max_file_read_bytes {
        diffs.push(format!(
            "diff: field=policy.max_file_read_bytes current={} loaded={}",
            current.policy.max_file_read_bytes, loaded.policy.max_file_read_bytes
        ));
    }
    if current.policy.max_file_write_bytes != loaded.policy.max_file_write_bytes {
        diffs.push(format!(
            "diff: field=policy.max_file_write_bytes current={} loaded={}",
            current.policy.max_file_write_bytes, loaded.policy.max_file_write_bytes
        ));
    }
    if current.policy.allow_command_newlines != loaded.policy.allow_command_newlines {
        diffs.push(format!(
            "diff: field=policy.allow_command_newlines current={} loaded={}",
            current.policy.allow_command_newlines, loaded.policy.allow_command_newlines
        ));
    }
    if current.auth.openai != loaded.auth.openai {
        diffs.push(format!(
            "diff: field=auth.openai current={} loaded={}",
            current.auth.openai.as_str(),
            loaded.auth.openai.as_str()
        ));
    }
    if current.auth.anthropic != loaded.auth.anthropic {
        diffs.push(format!(
            "diff: field=auth.anthropic current={} loaded={}",
            current.auth.anthropic.as_str(),
            loaded.auth.anthropic.as_str()
        ));
    }
    if current.auth.google != loaded.auth.google {
        diffs.push(format!(
            "diff: field=auth.google current={} loaded={}",
            current.auth.google.as_str(),
            loaded.auth.google.as_str()
        ));
    }

    diffs
}

fn render_profile_list(
    profile_path: &Path,
    profiles: &BTreeMap<String, ProfileDefaults>,
) -> String {
    if profiles.is_empty() {
        return format!(
            "profile list: path={} profiles=0 names=none",
            profile_path.display()
        );
    }

    let mut lines = vec![format!(
        "profile list: path={} profiles={}",
        profile_path.display(),
        profiles.len()
    )];
    for name in profiles.keys() {
        lines.push(format!("profile: name={name}"));
    }
    lines.join("\n")
}

fn render_profile_show(profile_path: &Path, name: &str, profile: &ProfileDefaults) -> String {
    let fallback_models = if profile.fallback_models.is_empty() {
        "none".to_string()
    } else {
        profile.fallback_models.join(",")
    };
    let mut lines = vec![format!(
        "profile show: path={} name={} status=found",
        profile_path.display(),
        name
    )];
    lines.push(format!("value: model={}", profile.model));
    lines.push(format!("value: fallback_models={fallback_models}"));
    lines.push(format!(
        "value: session.enabled={}",
        profile.session.enabled
    ));
    lines.push(format!(
        "value: session.path={}",
        profile.session.path.as_deref().unwrap_or("none")
    ));
    lines.push(format!(
        "value: session.import_mode={}",
        profile.session.import_mode
    ));
    lines.push(format!(
        "value: policy.tool_policy_preset={}",
        profile.policy.tool_policy_preset
    ));
    lines.push(format!(
        "value: policy.bash_profile={}",
        profile.policy.bash_profile
    ));
    lines.push(format!(
        "value: policy.bash_dry_run={}",
        profile.policy.bash_dry_run
    ));
    lines.push(format!(
        "value: policy.os_sandbox_mode={}",
        profile.policy.os_sandbox_mode
    ));
    lines.push(format!(
        "value: policy.enforce_regular_files={}",
        profile.policy.enforce_regular_files
    ));
    lines.push(format!(
        "value: policy.bash_timeout_ms={}",
        profile.policy.bash_timeout_ms
    ));
    lines.push(format!(
        "value: policy.max_command_length={}",
        profile.policy.max_command_length
    ));
    lines.push(format!(
        "value: policy.max_tool_output_bytes={}",
        profile.policy.max_tool_output_bytes
    ));
    lines.push(format!(
        "value: policy.max_file_read_bytes={}",
        profile.policy.max_file_read_bytes
    ));
    lines.push(format!(
        "value: policy.max_file_write_bytes={}",
        profile.policy.max_file_write_bytes
    ));
    lines.push(format!(
        "value: policy.allow_command_newlines={}",
        profile.policy.allow_command_newlines
    ));
    lines.push(format!(
        "value: auth.openai={}",
        profile.auth.openai.as_str()
    ));
    lines.push(format!(
        "value: auth.anthropic={}",
        profile.auth.anthropic.as_str()
    ));
    lines.push(format!(
        "value: auth.google={}",
        profile.auth.google.as_str()
    ));
    lines.join("\n")
}

fn execute_profile_command(
    command_args: &str,
    profile_path: &Path,
    current_defaults: &ProfileDefaults,
) -> String {
    let command = match parse_profile_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!(
                "profile error: path={} error={error}",
                profile_path.display()
            );
        }
    };
    let mut profiles = match load_profile_store(profile_path) {
        Ok(profiles) => profiles,
        Err(error) => {
            return format!(
                "profile error: path={} error={error}",
                profile_path.display()
            );
        }
    };

    match command {
        ProfileCommand::Save { name } => {
            profiles.insert(name.clone(), current_defaults.clone());
            match save_profile_store(profile_path, &profiles) {
                Ok(()) => format!(
                    "profile save: path={} name={} status=saved",
                    profile_path.display(),
                    name
                ),
                Err(error) => format!(
                    "profile error: path={} name={} error={error}",
                    profile_path.display(),
                    name
                ),
            }
        }
        ProfileCommand::List => render_profile_list(profile_path, &profiles),
        ProfileCommand::Show { name } => {
            let Some(loaded) = profiles.get(&name) else {
                return format!(
                    "profile error: path={} name={} error=unknown profile '{}'",
                    profile_path.display(),
                    name,
                    name
                );
            };
            render_profile_show(profile_path, &name, loaded)
        }
        ProfileCommand::Load { name } => {
            let Some(loaded) = profiles.get(&name) else {
                return format!(
                    "profile error: path={} name={} error=unknown profile '{}'",
                    profile_path.display(),
                    name,
                    name
                );
            };
            let diffs = render_profile_diffs(current_defaults, loaded);
            if diffs.is_empty() {
                return format!(
                    "profile load: path={} name={} status=in_sync diffs=0",
                    profile_path.display(),
                    name
                );
            }
            let mut lines = vec![format!(
                "profile load: path={} name={} status=diff diffs={}",
                profile_path.display(),
                name,
                diffs.len()
            )];
            lines.extend(diffs);
            lines.join("\n")
        }
        ProfileCommand::Delete { name } => {
            if profiles.remove(&name).is_none() {
                return format!(
                    "profile error: path={} name={} error=unknown profile '{}'",
                    profile_path.display(),
                    name,
                    name
                );
            }
            match save_profile_store(profile_path, &profiles) {
                Ok(()) => format!(
                    "profile delete: path={} name={} status=deleted remaining={}",
                    profile_path.display(),
                    name,
                    profiles.len()
                ),
                Err(error) => format!(
                    "profile error: path={} name={} error={error}",
                    profile_path.display(),
                    name
                ),
            }
        }
    }
}

const BRANCH_ALIAS_SCHEMA_VERSION: u32 = 1;
const BRANCH_ALIAS_USAGE: &str = "usage: /branch-alias <set|list|use> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
enum BranchAliasCommand {
    List,
    Set { name: String, id: u64 },
    Use { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BranchAliasFile {
    schema_version: u32,
    aliases: BTreeMap<String, u64>,
}

fn branch_alias_path_for_session(session_path: &Path) -> PathBuf {
    session_path.with_extension("aliases.json")
}

fn validate_branch_alias_name(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        bail!("alias name must not be empty");
    };
    if !first.is_ascii_alphabetic() {
        bail!("alias name '{}' must start with an ASCII letter", name);
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')) {
        bail!(
            "alias name '{}' must contain only ASCII letters, digits, '-' or '_'",
            name
        );
    }
    Ok(())
}

fn parse_branch_alias_command(command_args: &str) -> Result<BranchAliasCommand> {
    const USAGE_LIST: &str = "usage: /branch-alias list";
    const USAGE_SET: &str = "usage: /branch-alias set <name> <id>";
    const USAGE_USE: &str = "usage: /branch-alias use <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{BRANCH_ALIAS_USAGE}");
    }

    match tokens[0] {
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(BranchAliasCommand::List)
        }
        "set" => {
            if tokens.len() != 3 {
                bail!("{USAGE_SET}");
            }
            validate_branch_alias_name(tokens[1])?;
            let id = tokens[2]
                .parse::<u64>()
                .map_err(|_| anyhow!("invalid branch id '{}'; expected an integer", tokens[2]))?;
            Ok(BranchAliasCommand::Set {
                name: tokens[1].to_string(),
                id,
            })
        }
        "use" => {
            if tokens.len() != 2 {
                bail!("{USAGE_USE}");
            }
            validate_branch_alias_name(tokens[1])?;
            Ok(BranchAliasCommand::Use {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {BRANCH_ALIAS_USAGE}", other),
    }
}

fn load_branch_aliases(path: &Path) -> Result<BTreeMap<String, u64>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read alias file {}", path.display()))?;
    let parsed = serde_json::from_str::<BranchAliasFile>(&raw)
        .with_context(|| format!("failed to parse alias file {}", path.display()))?;
    if parsed.schema_version != BRANCH_ALIAS_SCHEMA_VERSION {
        bail!(
            "unsupported alias schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            BRANCH_ALIAS_SCHEMA_VERSION
        );
    }
    Ok(parsed.aliases)
}

fn save_branch_aliases(path: &Path, aliases: &BTreeMap<String, u64>) -> Result<()> {
    let payload = BranchAliasFile {
        schema_version: BRANCH_ALIAS_SCHEMA_VERSION,
        aliases: aliases.clone(),
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode branch aliases")?;
    encoded.push('\n');
    write_text_atomic(path, &encoded)
}

fn render_branch_alias_list(
    path: &Path,
    aliases: &BTreeMap<String, u64>,
    runtime: &SessionRuntime,
) -> String {
    let mut lines = vec![format!(
        "branch alias list: path={} count={}",
        path.display(),
        aliases.len()
    )];
    if aliases.is_empty() {
        lines.push("aliases: none".to_string());
        return lines.join("\n");
    }
    for (name, id) in aliases {
        let status = if runtime.store.contains(*id) {
            "ok"
        } else {
            "stale"
        };
        lines.push(format!("alias: name={} id={} status={}", name, id, status));
    }
    lines.join("\n")
}

fn execute_branch_alias_command(
    command_args: &str,
    agent: &mut Agent,
    runtime: &mut SessionRuntime,
) -> String {
    let alias_path = branch_alias_path_for_session(runtime.store.path());
    let command = match parse_branch_alias_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!(
                "branch alias error: path={} error={error}",
                alias_path.display()
            )
        }
    };

    let mut aliases = match load_branch_aliases(&alias_path) {
        Ok(aliases) => aliases,
        Err(error) => {
            return format!(
                "branch alias error: path={} error={error}",
                alias_path.display()
            )
        }
    };

    match command {
        BranchAliasCommand::List => render_branch_alias_list(&alias_path, &aliases, runtime),
        BranchAliasCommand::Set { name, id } => {
            if !runtime.store.contains(id) {
                return format!(
                    "branch alias error: path={} name={} error=unknown session id {}",
                    alias_path.display(),
                    name,
                    id
                );
            }
            aliases.insert(name.clone(), id);
            match save_branch_aliases(&alias_path, &aliases) {
                Ok(()) => format!(
                    "branch alias set: path={} name={} id={}",
                    alias_path.display(),
                    name,
                    id
                ),
                Err(error) => format!(
                    "branch alias error: path={} name={} error={error}",
                    alias_path.display(),
                    name
                ),
            }
        }
        BranchAliasCommand::Use { name } => {
            let Some(id) = aliases.get(&name).copied() else {
                return format!(
                    "branch alias error: path={} name={} error=unknown alias '{}'",
                    alias_path.display(),
                    name,
                    name
                );
            };
            if !runtime.store.contains(id) {
                return format!(
                    "branch alias error: path={} name={} error=alias points to unknown session id {}",
                    alias_path.display(),
                    name,
                    id
                );
            }
            runtime.active_head = Some(id);
            match reload_agent_from_active_head(agent, runtime) {
                Ok(()) => format!(
                    "branch alias use: path={} name={} id={}",
                    alias_path.display(),
                    name,
                    id
                ),
                Err(error) => format!(
                    "branch alias error: path={} name={} error={error}",
                    alias_path.display(),
                    name
                ),
            }
        }
    }
}

const SESSION_BOOKMARK_SCHEMA_VERSION: u32 = 1;
const SESSION_BOOKMARK_USAGE: &str = "usage: /session-bookmark <set|list|use|delete> ...";

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionBookmarkCommand {
    List,
    Set { name: String, id: u64 },
    Use { name: String },
    Delete { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SessionBookmarkFile {
    schema_version: u32,
    bookmarks: BTreeMap<String, u64>,
}

fn session_bookmark_path_for_session(session_path: &Path) -> PathBuf {
    session_path.with_extension("bookmarks.json")
}

fn parse_session_bookmark_command(command_args: &str) -> Result<SessionBookmarkCommand> {
    const USAGE_LIST: &str = "usage: /session-bookmark list";
    const USAGE_SET: &str = "usage: /session-bookmark set <name> <id>";
    const USAGE_USE: &str = "usage: /session-bookmark use <name>";
    const USAGE_DELETE: &str = "usage: /session-bookmark delete <name>";

    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{SESSION_BOOKMARK_USAGE}");
    }

    match tokens[0] {
        "list" => {
            if tokens.len() != 1 {
                bail!("{USAGE_LIST}");
            }
            Ok(SessionBookmarkCommand::List)
        }
        "set" => {
            if tokens.len() != 3 {
                bail!("{USAGE_SET}");
            }
            validate_branch_alias_name(tokens[1])?;
            let id = tokens[2]
                .parse::<u64>()
                .map_err(|_| anyhow!("invalid bookmark id '{}'; expected an integer", tokens[2]))?;
            Ok(SessionBookmarkCommand::Set {
                name: tokens[1].to_string(),
                id,
            })
        }
        "use" => {
            if tokens.len() != 2 {
                bail!("{USAGE_USE}");
            }
            validate_branch_alias_name(tokens[1])?;
            Ok(SessionBookmarkCommand::Use {
                name: tokens[1].to_string(),
            })
        }
        "delete" => {
            if tokens.len() != 2 {
                bail!("{USAGE_DELETE}");
            }
            validate_branch_alias_name(tokens[1])?;
            Ok(SessionBookmarkCommand::Delete {
                name: tokens[1].to_string(),
            })
        }
        other => bail!("unknown subcommand '{}'; {SESSION_BOOKMARK_USAGE}", other),
    }
}

fn load_session_bookmarks(path: &Path) -> Result<BTreeMap<String, u64>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session bookmark file {}", path.display()))?;
    let parsed = serde_json::from_str::<SessionBookmarkFile>(&raw)
        .with_context(|| format!("failed to parse session bookmark file {}", path.display()))?;
    if parsed.schema_version != SESSION_BOOKMARK_SCHEMA_VERSION {
        bail!(
            "unsupported session bookmark schema_version {} in {} (expected {})",
            parsed.schema_version,
            path.display(),
            SESSION_BOOKMARK_SCHEMA_VERSION
        );
    }
    Ok(parsed.bookmarks)
}

fn save_session_bookmarks(path: &Path, bookmarks: &BTreeMap<String, u64>) -> Result<()> {
    let payload = SessionBookmarkFile {
        schema_version: SESSION_BOOKMARK_SCHEMA_VERSION,
        bookmarks: bookmarks.clone(),
    };
    let mut encoded =
        serde_json::to_string_pretty(&payload).context("failed to encode session bookmarks")?;
    encoded.push('\n');
    write_text_atomic(path, &encoded)
}

fn render_session_bookmark_list(
    path: &Path,
    bookmarks: &BTreeMap<String, u64>,
    runtime: &SessionRuntime,
) -> String {
    let mut lines = vec![format!(
        "session bookmark list: path={} count={}",
        path.display(),
        bookmarks.len()
    )];
    if bookmarks.is_empty() {
        lines.push("bookmarks: none".to_string());
        return lines.join("\n");
    }
    for (name, id) in bookmarks {
        let status = if runtime.store.contains(*id) {
            "ok"
        } else {
            "stale"
        };
        lines.push(format!(
            "bookmark: name={} id={} status={}",
            name, id, status
        ));
    }
    lines.join("\n")
}

fn execute_session_bookmark_command(
    command_args: &str,
    agent: &mut Agent,
    runtime: &mut SessionRuntime,
) -> String {
    let bookmark_path = session_bookmark_path_for_session(runtime.store.path());
    let command = match parse_session_bookmark_command(command_args) {
        Ok(command) => command,
        Err(error) => {
            return format!(
                "session bookmark error: path={} error={error}",
                bookmark_path.display()
            );
        }
    };

    let mut bookmarks = match load_session_bookmarks(&bookmark_path) {
        Ok(bookmarks) => bookmarks,
        Err(error) => {
            return format!(
                "session bookmark error: path={} error={error}",
                bookmark_path.display()
            );
        }
    };

    match command {
        SessionBookmarkCommand::List => {
            render_session_bookmark_list(&bookmark_path, &bookmarks, runtime)
        }
        SessionBookmarkCommand::Set { name, id } => {
            if !runtime.store.contains(id) {
                return format!(
                    "session bookmark error: path={} name={} error=unknown session id {}",
                    bookmark_path.display(),
                    name,
                    id
                );
            }
            bookmarks.insert(name.clone(), id);
            match save_session_bookmarks(&bookmark_path, &bookmarks) {
                Ok(()) => format!(
                    "session bookmark set: path={} name={} id={}",
                    bookmark_path.display(),
                    name,
                    id
                ),
                Err(error) => format!(
                    "session bookmark error: path={} name={} error={error}",
                    bookmark_path.display(),
                    name
                ),
            }
        }
        SessionBookmarkCommand::Use { name } => {
            let Some(id) = bookmarks.get(&name).copied() else {
                return format!(
                    "session bookmark error: path={} name={} error=unknown bookmark '{}'",
                    bookmark_path.display(),
                    name,
                    name
                );
            };
            if !runtime.store.contains(id) {
                return format!(
                    "session bookmark error: path={} name={} error=bookmark points to unknown session id {}",
                    bookmark_path.display(),
                    name,
                    id
                );
            }
            runtime.active_head = Some(id);
            match reload_agent_from_active_head(agent, runtime) {
                Ok(()) => format!(
                    "session bookmark use: path={} name={} id={}",
                    bookmark_path.display(),
                    name,
                    id
                ),
                Err(error) => format!(
                    "session bookmark error: path={} name={} error={error}",
                    bookmark_path.display(),
                    name
                ),
            }
        }
        SessionBookmarkCommand::Delete { name } => {
            if bookmarks.remove(&name).is_none() {
                return format!(
                    "session bookmark error: path={} name={} error=unknown bookmark '{}'",
                    bookmark_path.display(),
                    name,
                    name
                );
            }
            match save_session_bookmarks(&bookmark_path, &bookmarks) {
                Ok(()) => format!(
                    "session bookmark delete: path={} name={} status=deleted remaining={}",
                    bookmark_path.display(),
                    name,
                    bookmarks.len()
                ),
                Err(error) => format!(
                    "session bookmark error: path={} name={} error={error}",
                    bookmark_path.display(),
                    name
                ),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillsSearchMatch {
    name: String,
    file: String,
    name_hit: bool,
    content_hit: bool,
}

fn parse_skills_search_args(command_args: &str) -> Result<(String, usize)> {
    const DEFAULT_MAX_RESULTS: usize = 20;
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("query is required");
    }

    let mut max_results = DEFAULT_MAX_RESULTS;
    let query_tokens = if let Some(last) = tokens.last() {
        match last.parse::<usize>() {
            Ok(parsed_limit) => {
                if parsed_limit == 0 {
                    bail!("max_results must be greater than zero");
                }
                max_results = parsed_limit;
                &tokens[..tokens.len() - 1]
            }
            Err(_) => &tokens[..],
        }
    } else {
        &tokens[..]
    };

    if query_tokens.is_empty() {
        bail!("query is required");
    }
    let query = query_tokens.join(" ");
    if query.trim().is_empty() {
        bail!("query is required");
    }

    Ok((query, max_results))
}

fn render_skills_search(
    skills_dir: &Path,
    query: &str,
    max_results: usize,
    matches: &[SkillsSearchMatch],
    total_matches: usize,
) -> String {
    let mut lines = vec![format!(
        "skills search: path={} query={:?} max_results={} matched={} shown={}",
        skills_dir.display(),
        query,
        max_results,
        total_matches,
        matches.len()
    )];
    if matches.is_empty() {
        lines.push("skills: none".to_string());
        return lines.join("\n");
    }

    for entry in matches {
        let match_kind = match (entry.name_hit, entry.content_hit) {
            (true, true) => "name+content",
            (true, false) => "name",
            (false, true) => "content",
            (false, false) => "unknown",
        };
        lines.push(format!(
            "skill: name={} file={} match={}",
            entry.name, entry.file, match_kind
        ));
    }
    lines.join("\n")
}

fn execute_skills_search_command(skills_dir: &Path, command_args: &str) -> String {
    let (query, max_results) = match parse_skills_search_args(command_args) {
        Ok(parsed) => parsed,
        Err(error) => {
            return format!(
                "skills search error: path={} args={:?} error={error}",
                skills_dir.display(),
                command_args
            )
        }
    };

    let catalog = match load_catalog(skills_dir) {
        Ok(catalog) => catalog,
        Err(error) => {
            return format!(
                "skills search error: path={} query={:?} error={error}",
                skills_dir.display(),
                query
            )
        }
    };

    let query_lower = query.to_ascii_lowercase();
    let mut matches = Vec::new();
    for skill in catalog {
        let name_hit = skill.name.to_ascii_lowercase().contains(&query_lower);
        let content_hit = skill.content.to_ascii_lowercase().contains(&query_lower);
        if !(name_hit || content_hit) {
            continue;
        }
        let file = skill
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        matches.push(SkillsSearchMatch {
            name: skill.name,
            file,
            name_hit,
            content_hit,
        });
    }

    matches.sort_by(|left, right| {
        right
            .name_hit
            .cmp(&left.name_hit)
            .then_with(|| left.name.cmp(&right.name))
    });
    let total_matches = matches.len();
    matches.truncate(max_results);

    render_skills_search(skills_dir, &query, max_results, &matches, total_matches)
}

fn render_skills_show(skills_dir: &Path, skill: &skills::Skill) -> String {
    let file = skill
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    format!(
        "skills show: path={} name={} file={} content_bytes={}\n---\n{}",
        skills_dir.display(),
        skill.name,
        file,
        skill.content.len(),
        skill.content
    )
}

fn execute_skills_show_command(skills_dir: &Path, skill_name: &str) -> String {
    match load_catalog(skills_dir) {
        Ok(catalog) => match catalog.into_iter().find(|skill| skill.name == skill_name) {
            Some(skill) => render_skills_show(skills_dir, &skill),
            None => format!(
                "skills show error: path={} name={} error=unknown skill '{}'",
                skills_dir.display(),
                skill_name,
                skill_name
            ),
        },
        Err(error) => format!(
            "skills show error: path={} name={} error={error}",
            skills_dir.display(),
            skill_name
        ),
    }
}

fn render_skills_list(skills_dir: &Path, catalog: &[skills::Skill]) -> String {
    let mut lines = vec![format!(
        "skills list: path={} count={}",
        skills_dir.display(),
        catalog.len()
    )];
    if catalog.is_empty() {
        lines.push("skills: none".to_string());
    } else {
        for skill in catalog {
            let file = skill
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            lines.push(format!("skill: name={} file={}", skill.name, file));
        }
    }
    lines.join("\n")
}

fn execute_skills_list_command(skills_dir: &Path) -> String {
    match load_catalog(skills_dir) {
        Ok(catalog) => render_skills_list(skills_dir, &catalog),
        Err(error) => format!(
            "skills list error: path={} error={error}",
            skills_dir.display()
        ),
    }
}

fn resolve_skills_lock_path(command_args: &str, default_lock_path: &Path) -> PathBuf {
    if command_args.is_empty() {
        default_lock_path.to_path_buf()
    } else {
        PathBuf::from(command_args)
    }
}

fn parse_skills_lock_diff_args(
    command_args: &str,
    default_lock_path: &Path,
) -> Result<(PathBuf, bool)> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok((default_lock_path.to_path_buf(), false));
    }

    let mut lock_path: Option<PathBuf> = None;
    let mut json_output = false;
    for token in tokens {
        if token == "--json" {
            json_output = true;
            continue;
        }

        if lock_path.is_some() {
            bail!(
                "unexpected argument '{}'; usage: /skills-lock-diff [lockfile_path] [--json]",
                token
            );
        }
        lock_path = Some(PathBuf::from(token));
    }

    Ok((
        lock_path.unwrap_or_else(|| default_lock_path.to_path_buf()),
        json_output,
    ))
}

const SKILLS_PRUNE_USAGE: &str = "usage: /skills-prune [lockfile_path] [--dry-run|--apply]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillsPruneMode {
    DryRun,
    Apply,
}

impl SkillsPruneMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillsPruneCandidate {
    file: String,
    path: PathBuf,
}

fn parse_skills_prune_args(
    command_args: &str,
    default_lock_path: &Path,
) -> Result<(PathBuf, SkillsPruneMode)> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Ok((default_lock_path.to_path_buf(), SkillsPruneMode::DryRun));
    }

    let mut lock_path: Option<PathBuf> = None;
    let mut mode = SkillsPruneMode::DryRun;
    let mut mode_flag_seen = false;
    for token in tokens {
        match token {
            "--dry-run" => {
                if mode_flag_seen && mode != SkillsPruneMode::DryRun {
                    bail!("conflicting flags '--dry-run' and '--apply'; {SKILLS_PRUNE_USAGE}");
                }
                mode = SkillsPruneMode::DryRun;
                mode_flag_seen = true;
            }
            "--apply" => {
                if mode_flag_seen && mode != SkillsPruneMode::Apply {
                    bail!("conflicting flags '--dry-run' and '--apply'; {SKILLS_PRUNE_USAGE}");
                }
                mode = SkillsPruneMode::Apply;
                mode_flag_seen = true;
            }
            _ => {
                if lock_path.is_some() {
                    bail!("unexpected argument '{}'; {SKILLS_PRUNE_USAGE}", token);
                }
                lock_path = Some(PathBuf::from(token));
            }
        }
    }

    Ok((
        lock_path.unwrap_or_else(|| default_lock_path.to_path_buf()),
        mode,
    ))
}

fn validate_skills_prune_file_name(file: &str) -> Result<()> {
    if file.contains('\\') {
        bail!(
            "unsafe lockfile entry '{}': path separators are not allowed",
            file
        );
    }

    let path = Path::new(file);
    if path.is_absolute() {
        bail!(
            "unsafe lockfile entry '{}': absolute paths are not allowed",
            file
        );
    }

    let mut components = path.components();
    let first = components.next();
    if components.next().is_some() {
        bail!(
            "unsafe lockfile entry '{}': nested paths are not allowed",
            file
        );
    }

    match first {
        Some(std::path::Component::Normal(component)) => {
            let Some(component) = component.to_str() else {
                bail!("unsafe lockfile entry '{}': path must be valid UTF-8", file);
            };
            if component.is_empty() {
                bail!("unsafe lockfile entry '{}': empty file name", file);
            }
        }
        _ => bail!("unsafe lockfile entry '{}': invalid path component", file),
    }

    if !file.ends_with(".md") {
        bail!(
            "unsafe lockfile entry '{}': only markdown files can be pruned",
            file
        );
    }

    Ok(())
}

fn resolve_prunable_skill_file_name(skills_dir: &Path, skill_path: &Path) -> Result<String> {
    let relative_path = skill_path.strip_prefix(skills_dir).with_context(|| {
        format!(
            "unsafe skill path '{}': outside skills dir '{}'",
            skill_path.display(),
            skills_dir.display()
        )
    })?;
    let mut components = relative_path.components();
    let first = components.next();
    if components.next().is_some() {
        bail!(
            "unsafe skill path '{}': nested paths are not allowed",
            skill_path.display()
        );
    }
    let Some(std::path::Component::Normal(file_os_str)) = first else {
        bail!(
            "unsafe skill path '{}': invalid file path component",
            skill_path.display()
        );
    };
    let Some(file) = file_os_str.to_str() else {
        bail!(
            "unsafe skill path '{}': file name must be valid UTF-8",
            skill_path.display()
        );
    };
    validate_skills_prune_file_name(file)?;
    Ok(file.to_string())
}

fn derive_skills_prune_candidates(
    skills_dir: &Path,
    catalog: &[skills::Skill],
    tracked_files: &HashSet<String>,
) -> Result<Vec<SkillsPruneCandidate>> {
    let mut candidates = Vec::new();
    for skill in catalog {
        let file = resolve_prunable_skill_file_name(skills_dir, &skill.path)?;
        if tracked_files.contains(&file) {
            continue;
        }
        candidates.push(SkillsPruneCandidate {
            file,
            path: skill.path.clone(),
        });
    }
    candidates.sort_by(|left, right| left.file.cmp(&right.file));
    Ok(candidates)
}

fn execute_skills_prune_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let (lock_path, mode) = match parse_skills_prune_args(command_args, default_lock_path) {
        Ok(parsed) => parsed,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                default_lock_path.display(),
                SkillsPruneMode::DryRun.as_str()
            )
        }
    };

    let lockfile = match load_skills_lockfile(&lock_path) {
        Ok(lockfile) => lockfile,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            )
        }
    };

    let mut tracked_files = HashSet::new();
    for entry in &lockfile.entries {
        if let Err(error) = validate_skills_prune_file_name(&entry.file) {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            );
        }
        tracked_files.insert(entry.file.clone());
    }

    let catalog = match load_catalog(skills_dir) {
        Ok(catalog) => catalog,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            )
        }
    };

    let candidates = match derive_skills_prune_candidates(skills_dir, &catalog, &tracked_files) {
        Ok(candidates) => candidates,
        Err(error) => {
            return format!(
                "skills prune error: path={} mode={} error={error}",
                lock_path.display(),
                mode.as_str()
            )
        }
    };

    let mut lines = vec![format!(
        "skills prune: mode={} lockfile={} skills_dir={} tracked_entries={} installed_skills={} prune_candidates={}",
        mode.as_str(),
        lock_path.display(),
        skills_dir.display(),
        tracked_files.len(),
        catalog.len(),
        candidates.len()
    )];

    if candidates.is_empty() {
        lines.push("prune: none".to_string());
        return lines.join("\n");
    }

    for candidate in &candidates {
        let action = match mode {
            SkillsPruneMode::DryRun => "would_delete",
            SkillsPruneMode::Apply => "delete",
        };
        lines.push(format!("prune: file={} action={action}", candidate.file));
    }

    if mode == SkillsPruneMode::DryRun {
        return lines.join("\n");
    }

    let mut deleted = 0usize;
    let mut failed = 0usize;
    for candidate in &candidates {
        match std::fs::remove_file(&candidate.path) {
            Ok(()) => {
                deleted += 1;
                lines.push(format!("prune: file={} status=deleted", candidate.file));
            }
            Err(error) => {
                failed += 1;
                lines.push(format!(
                    "prune: file={} status=error error={error}",
                    candidate.file
                ));
            }
        }
    }
    lines.push(format!(
        "skills prune result: mode=apply deleted={} failed={}",
        deleted, failed
    ));
    lines.join("\n")
}

const SKILLS_TRUST_LIST_USAGE: &str = "usage: /skills-trust-list [trust_root_file]";
const SKILLS_TRUST_ADD_USAGE: &str = "usage: /skills-trust-add <id=base64_key> [trust_root_file]";
const SKILLS_TRUST_REVOKE_USAGE: &str = "usage: /skills-trust-revoke <id> [trust_root_file]";
const SKILLS_TRUST_ROTATE_USAGE: &str =
    "usage: /skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]";

fn parse_skills_trust_mutation_args(
    command_args: &str,
    default_trust_root_path: Option<&Path>,
    usage: &str,
) -> Result<(String, PathBuf)> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{usage}");
    }
    if tokens.len() > 2 {
        bail!("unexpected argument '{}'; {usage}", tokens[2]);
    }

    let trust_root_path = if tokens.len() == 2 {
        PathBuf::from(tokens[1])
    } else {
        default_trust_root_path
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("trust root file is required; {usage}"))?
    };
    Ok((tokens[0].to_string(), trust_root_path))
}

fn parse_skills_trust_list_args(
    command_args: &str,
    default_trust_root_path: Option<&Path>,
) -> Result<PathBuf> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return default_trust_root_path
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("trust root file is required; {SKILLS_TRUST_LIST_USAGE}"));
    }

    if tokens.len() > 1 {
        bail!(
            "unexpected argument '{}'; {SKILLS_TRUST_LIST_USAGE}",
            tokens[1]
        );
    }

    Ok(PathBuf::from(tokens[0]))
}

fn execute_skills_trust_add_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let (spec, trust_root_path) = match parse_skills_trust_mutation_args(
        command_args,
        default_trust_root_path,
        SKILLS_TRUST_ADD_USAGE,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust add error: path={} error={error}",
                configured_path
            );
        }
    };

    let key = match parse_trusted_root_spec(&spec) {
        Ok(key) => key,
        Err(error) => {
            return format!(
                "skills trust add error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    let mut records = match load_trust_root_records(&trust_root_path) {
        Ok(records) => records,
        Err(error) => {
            return format!(
                "skills trust add error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };
    let add_specs = vec![spec];
    let report = match apply_trust_root_mutation_specs(&mut records, &add_specs, &[], &[]) {
        Ok(report) => report,
        Err(error) => {
            return format!(
                "skills trust add error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    match save_trust_root_records(&trust_root_path, &records) {
        Ok(()) => format!(
            "skills trust add: path={} id={} added={} updated={} revoked={} rotated={}",
            trust_root_path.display(),
            key.id,
            report.added,
            report.updated,
            report.revoked,
            report.rotated
        ),
        Err(error) => format!(
            "skills trust add error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

fn execute_skills_trust_revoke_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let (spec, trust_root_path) = match parse_skills_trust_mutation_args(
        command_args,
        default_trust_root_path,
        SKILLS_TRUST_REVOKE_USAGE,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust revoke error: path={} error={error}",
                configured_path
            );
        }
    };

    let mut records = match load_trust_root_records(&trust_root_path) {
        Ok(records) => records,
        Err(error) => {
            return format!(
                "skills trust revoke error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };
    let revoke_ids = vec![spec.clone()];
    let report = match apply_trust_root_mutation_specs(&mut records, &[], &revoke_ids, &[]) {
        Ok(report) => report,
        Err(error) => {
            return format!(
                "skills trust revoke error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    match save_trust_root_records(&trust_root_path, &records) {
        Ok(()) => format!(
            "skills trust revoke: path={} id={} added={} updated={} revoked={} rotated={}",
            trust_root_path.display(),
            spec,
            report.added,
            report.updated,
            report.revoked,
            report.rotated
        ),
        Err(error) => format!(
            "skills trust revoke error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

fn execute_skills_trust_rotate_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let (spec, trust_root_path) = match parse_skills_trust_mutation_args(
        command_args,
        default_trust_root_path,
        SKILLS_TRUST_ROTATE_USAGE,
    ) {
        Ok(parsed) => parsed,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust rotate error: path={} error={error}",
                configured_path
            );
        }
    };

    let (old_id, new_key) = match parse_trust_rotation_spec(&spec) {
        Ok(parsed) => parsed,
        Err(error) => {
            return format!(
                "skills trust rotate error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    let mut records = match load_trust_root_records(&trust_root_path) {
        Ok(records) => records,
        Err(error) => {
            return format!(
                "skills trust rotate error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };
    let rotate_specs = vec![spec];
    let report = match apply_trust_root_mutation_specs(&mut records, &[], &[], &rotate_specs) {
        Ok(report) => report,
        Err(error) => {
            return format!(
                "skills trust rotate error: path={} error={error}",
                trust_root_path.display()
            );
        }
    };

    match save_trust_root_records(&trust_root_path, &records) {
        Ok(()) => format!(
            "skills trust rotate: path={} old_id={} new_id={} added={} updated={} revoked={} rotated={}",
            trust_root_path.display(),
            old_id,
            new_key.id,
            report.added,
            report.updated,
            report.revoked,
            report.rotated
        ),
        Err(error) => format!(
            "skills trust rotate error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

fn trust_record_status(record: &TrustedRootRecord, now_unix: u64) -> &'static str {
    if record.revoked {
        "revoked"
    } else if is_expired_unix(record.expires_unix, now_unix) {
        "expired"
    } else {
        "active"
    }
}

fn render_skills_trust_list(path: &Path, records: &[TrustedRootRecord]) -> String {
    let now_unix = current_unix_timestamp();
    let mut lines = vec![format!(
        "skills trust list: path={} count={}",
        path.display(),
        records.len()
    )];

    if records.is_empty() {
        lines.push("roots: none".to_string());
        return lines.join("\n");
    }

    for record in records {
        lines.push(format!(
            "root: id={} revoked={} expires_unix={} rotated_from={} status={}",
            record.id,
            record.revoked,
            record
                .expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            record.rotated_from.as_deref().unwrap_or("none"),
            trust_record_status(record, now_unix)
        ));
    }

    lines.join("\n")
}

fn execute_skills_trust_list_command(
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let trust_root_path = match parse_skills_trust_list_args(command_args, default_trust_root_path)
    {
        Ok(path) => path,
        Err(error) => {
            let configured_path = default_trust_root_path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string());
            return format!(
                "skills trust list error: path={} error={error}",
                configured_path
            );
        }
    };

    match load_trust_root_records(&trust_root_path) {
        Ok(mut records) => {
            records.sort_by(|left, right| left.id.cmp(&right.id));
            render_skills_trust_list(&trust_root_path, &records)
        }
        Err(error) => format!(
            "skills trust list error: path={} error={error}",
            trust_root_path.display()
        ),
    }
}

fn render_skills_lock_diff_in_sync(path: &Path, report: &skills::SkillsSyncReport) -> String {
    format!(
        "skills lock diff: in-sync path={} expected_entries={} actual_entries={}",
        path.display(),
        report.expected_entries,
        report.actual_entries
    )
}

fn render_skills_lock_diff_drift(path: &Path, report: &skills::SkillsSyncReport) -> String {
    format!(
        "skills lock diff: drift path={} {}",
        path.display(),
        render_skills_sync_drift_details(report)
    )
}

fn execute_skills_lock_diff_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let (lock_path, json_output) =
        match parse_skills_lock_diff_args(command_args, default_lock_path) {
            Ok(parsed) => parsed,
            Err(error) => {
                return format!(
                    "skills lock diff error: path={} error={error}",
                    default_lock_path.display()
                )
            }
        };

    match sync_skills_with_lockfile(skills_dir, &lock_path) {
        Ok(report) => {
            if json_output {
                return serde_json::json!({
                    "path": lock_path.display().to_string(),
                    "status": if report.in_sync() { "in_sync" } else { "drift" },
                    "in_sync": report.in_sync(),
                    "expected_entries": report.expected_entries,
                    "actual_entries": report.actual_entries,
                    "missing": report.missing,
                    "extra": report.extra,
                    "changed": report.changed,
                    "metadata_mismatch": report.metadata_mismatch,
                })
                .to_string();
            }
            if report.in_sync() {
                render_skills_lock_diff_in_sync(&lock_path, &report)
            } else {
                render_skills_lock_diff_drift(&lock_path, &report)
            }
        }
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "path": lock_path.display().to_string(),
                    "status": "error",
                    "error": error.to_string(),
                })
                .to_string();
            }
            format!(
                "skills lock diff error: path={} error={error}",
                lock_path.display()
            )
        }
    }
}

fn render_skills_lock_write_success(path: &Path, entries: usize) -> String {
    format!(
        "skills lock write: path={} entries={entries}",
        path.display()
    )
}

fn execute_skills_lock_write_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let lock_path = resolve_skills_lock_path(command_args, default_lock_path);
    match write_skills_lockfile(skills_dir, &lock_path, &[]) {
        Ok(lockfile) => render_skills_lock_write_success(&lock_path, lockfile.entries.len()),
        Err(error) => format!(
            "skills lock write error: path={} error={error}",
            lock_path.display()
        ),
    }
}

fn render_skills_sync_field(items: &[String], separator: &str) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(separator)
    }
}

fn render_skills_sync_drift_details(report: &skills::SkillsSyncReport) -> String {
    format!(
        "expected_entries={} actual_entries={} missing={} extra={} changed={} metadata={}",
        report.expected_entries,
        report.actual_entries,
        render_skills_sync_field(&report.missing, ","),
        render_skills_sync_field(&report.extra, ","),
        render_skills_sync_field(&report.changed, ","),
        render_skills_sync_field(&report.metadata_mismatch, ";")
    )
}

fn render_skills_sync_in_sync(path: &Path, report: &skills::SkillsSyncReport) -> String {
    format!(
        "skills sync: in-sync path={} expected_entries={} actual_entries={}",
        path.display(),
        report.expected_entries,
        report.actual_entries
    )
}

fn render_skills_sync_drift(path: &Path, report: &skills::SkillsSyncReport) -> String {
    format!(
        "skills sync: drift path={} {}",
        path.display(),
        render_skills_sync_drift_details(report)
    )
}

fn execute_skills_sync_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    command_args: &str,
) -> String {
    let lock_path = resolve_skills_lock_path(command_args, default_lock_path);
    match sync_skills_with_lockfile(skills_dir, &lock_path) {
        Ok(report) => {
            if report.in_sync() {
                render_skills_sync_in_sync(&lock_path, &report)
            } else {
                render_skills_sync_drift(&lock_path, &report)
            }
        }
        Err(error) => format!(
            "skills sync error: path={} error={error}",
            lock_path.display()
        ),
    }
}

const SKILLS_VERIFY_USAGE: &str =
    "usage: /skills-verify [lockfile_path] [trust_root_file] [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillsVerifyArgs {
    lock_path: PathBuf,
    trust_root_path: Option<PathBuf>,
    json_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SkillsVerifyStatus {
    Pass,
    Warn,
    Fail,
}

impl SkillsVerifyStatus {
    fn severity(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Warn => 1,
            Self::Fail => 2,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillsVerifyEntry {
    file: String,
    name: String,
    status: SkillsVerifyStatus,
    checks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct SkillsVerifyTrustSummary {
    total: usize,
    active: usize,
    revoked: usize,
    expired: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
struct SkillsVerifySummary {
    entries: usize,
    pass: usize,
    warn: usize,
    fail: usize,
    status: SkillsVerifyStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillsVerifyReport {
    lock_path: String,
    trust_root_path: Option<String>,
    expected_entries: usize,
    actual_entries: usize,
    missing: Vec<String>,
    extra: Vec<String>,
    changed: Vec<String>,
    metadata_mismatch: Vec<String>,
    trust: Option<SkillsVerifyTrustSummary>,
    summary: SkillsVerifySummary,
    entries: Vec<SkillsVerifyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrustRootState {
    Active,
    Revoked,
    Expired,
}

fn parse_skills_verify_args(
    command_args: &str,
    default_lock_path: &Path,
    default_trust_root_path: Option<&Path>,
) -> Result<SkillsVerifyArgs> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut positional = Vec::new();
    let mut json_output = false;
    for token in tokens {
        if token == "--json" {
            json_output = true;
            continue;
        }
        positional.push(token);
    }

    if positional.len() > 2 {
        bail!(
            "unexpected argument '{}'; {SKILLS_VERIFY_USAGE}",
            positional[2]
        );
    }

    let lock_path = positional
        .first()
        .map(|token| PathBuf::from(*token))
        .unwrap_or_else(|| default_lock_path.to_path_buf());
    let trust_root_path = positional
        .get(1)
        .map(|token| PathBuf::from(*token))
        .or_else(|| default_trust_root_path.map(Path::to_path_buf));

    Ok(SkillsVerifyArgs {
        lock_path,
        trust_root_path,
        json_output,
    })
}

fn update_verify_status(
    status: &mut SkillsVerifyStatus,
    checks: &mut Vec<String>,
    next_status: SkillsVerifyStatus,
    check: String,
) {
    if next_status.severity() > status.severity() {
        *status = next_status;
    }
    checks.push(check);
}

fn build_skills_verify_report(
    skills_dir: &Path,
    lock_path: &Path,
    trust_root_path: Option<&Path>,
) -> Result<SkillsVerifyReport> {
    let lockfile = load_skills_lockfile(lock_path)?;
    let sync_report = sync_skills_with_lockfile(skills_dir, lock_path)?;

    let trust_data = if let Some(path) = trust_root_path {
        let records = load_trust_root_records(path)?;
        let now_unix = current_unix_timestamp();
        let mut trust_index = HashMap::new();
        let mut summary = SkillsVerifyTrustSummary {
            total: records.len(),
            active: 0,
            revoked: 0,
            expired: 0,
        };
        for record in records {
            let state = if record.revoked {
                summary.revoked += 1;
                TrustRootState::Revoked
            } else if is_expired_unix(record.expires_unix, now_unix) {
                summary.expired += 1;
                TrustRootState::Expired
            } else {
                summary.active += 1;
                TrustRootState::Active
            };
            trust_index.insert(record.id, state);
        }
        Some((summary, trust_index))
    } else {
        None
    };

    let mut metadata_by_file: HashMap<String, Vec<String>> = HashMap::new();
    for item in &sync_report.metadata_mismatch {
        if let Some((file, reason)) = item.split_once(": ") {
            metadata_by_file
                .entry(file.to_string())
                .or_default()
                .push(reason.to_string());
        }
    }

    let missing_files = sync_report.missing.iter().cloned().collect::<HashSet<_>>();
    let changed_files = sync_report.changed.iter().cloned().collect::<HashSet<_>>();
    let extra_files = sync_report.extra.iter().cloned().collect::<HashSet<_>>();

    let mut lock_entries = lockfile.entries.clone();
    lock_entries.sort_by(|left, right| left.file.cmp(&right.file));

    let mut entries = Vec::new();
    for lock_entry in lock_entries {
        let mut status = SkillsVerifyStatus::Pass;
        let mut checks = Vec::new();

        if missing_files.contains(&lock_entry.file) {
            update_verify_status(
                &mut status,
                &mut checks,
                SkillsVerifyStatus::Fail,
                "sync=missing".to_string(),
            );
        } else if changed_files.contains(&lock_entry.file) {
            update_verify_status(
                &mut status,
                &mut checks,
                SkillsVerifyStatus::Fail,
                "sync=changed".to_string(),
            );
        } else {
            checks.push("sync=ok".to_string());
        }

        if let Some(reasons) = metadata_by_file.get(&lock_entry.file) {
            for reason in reasons {
                update_verify_status(
                    &mut status,
                    &mut checks,
                    SkillsVerifyStatus::Fail,
                    format!("metadata={reason}"),
                );
            }
        }

        match &lock_entry.source {
            crate::skills::SkillLockSource::Remote {
                signing_key_id,
                signature,
                ..
            }
            | crate::skills::SkillLockSource::Registry {
                signing_key_id,
                signature,
                ..
            } => match (signing_key_id.as_deref(), signature.as_deref()) {
                (None, None) => update_verify_status(
                    &mut status,
                    &mut checks,
                    SkillsVerifyStatus::Warn,
                    "signature=unsigned".to_string(),
                ),
                (Some(_), None) | (None, Some(_)) => update_verify_status(
                    &mut status,
                    &mut checks,
                    SkillsVerifyStatus::Fail,
                    "signature=incomplete_metadata".to_string(),
                ),
                (Some(key_id), Some(_)) => {
                    if let Some((_, trust_index)) = &trust_data {
                        match trust_index.get(key_id) {
                            Some(TrustRootState::Active) => {
                                checks.push(format!("signature=trusted key={key_id}"));
                            }
                            Some(TrustRootState::Revoked) => update_verify_status(
                                &mut status,
                                &mut checks,
                                SkillsVerifyStatus::Fail,
                                format!("signature=revoked key={key_id}"),
                            ),
                            Some(TrustRootState::Expired) => update_verify_status(
                                &mut status,
                                &mut checks,
                                SkillsVerifyStatus::Fail,
                                format!("signature=expired key={key_id}"),
                            ),
                            None => update_verify_status(
                                &mut status,
                                &mut checks,
                                SkillsVerifyStatus::Fail,
                                format!("signature=untrusted key={key_id}"),
                            ),
                        }
                    } else {
                        update_verify_status(
                            &mut status,
                            &mut checks,
                            SkillsVerifyStatus::Warn,
                            format!("signature=unverified key={key_id} trust_root=none"),
                        );
                    }
                }
            },
            crate::skills::SkillLockSource::Unknown => checks.push("source=unknown".to_string()),
            crate::skills::SkillLockSource::Local { .. } => checks.push("source=local".to_string()),
        }

        entries.push(SkillsVerifyEntry {
            file: lock_entry.file,
            name: lock_entry.name,
            status,
            checks,
        });
    }

    for file in extra_files {
        entries.push(SkillsVerifyEntry {
            name: file.trim_end_matches(".md").to_string(),
            file,
            status: SkillsVerifyStatus::Fail,
            checks: vec!["sync=extra_not_in_lockfile".to_string()],
        });
    }
    entries.sort_by(|left, right| left.file.cmp(&right.file));

    let mut pass = 0usize;
    let mut warn = 0usize;
    let mut fail = 0usize;
    for entry in &entries {
        match entry.status {
            SkillsVerifyStatus::Pass => pass += 1,
            SkillsVerifyStatus::Warn => warn += 1,
            SkillsVerifyStatus::Fail => fail += 1,
        }
    }
    let overall_status = if fail > 0 {
        SkillsVerifyStatus::Fail
    } else if warn > 0 {
        SkillsVerifyStatus::Warn
    } else {
        SkillsVerifyStatus::Pass
    };

    Ok(SkillsVerifyReport {
        lock_path: lock_path.display().to_string(),
        trust_root_path: trust_root_path.map(|path| path.display().to_string()),
        expected_entries: sync_report.expected_entries,
        actual_entries: sync_report.actual_entries,
        missing: sync_report.missing,
        extra: sync_report.extra,
        changed: sync_report.changed,
        metadata_mismatch: sync_report.metadata_mismatch,
        trust: trust_data.map(|(summary, _)| summary),
        summary: SkillsVerifySummary {
            entries: entries.len(),
            pass,
            warn,
            fail,
            status: overall_status,
        },
        entries,
    })
}

fn render_skills_verify_report(report: &SkillsVerifyReport) -> String {
    let mut lines = vec![format!(
        "skills verify: status={} lock_path={} trust_root_path={} entries={} pass={} warn={} fail={}",
        report.summary.status.as_str(),
        report.lock_path,
        report.trust_root_path.as_deref().unwrap_or("none"),
        report.summary.entries,
        report.summary.pass,
        report.summary.warn,
        report.summary.fail
    )];
    lines.push(format!(
        "sync: expected_entries={} actual_entries={} missing={} extra={} changed={} metadata={}",
        report.expected_entries,
        report.actual_entries,
        render_skills_sync_field(&report.missing, ","),
        render_skills_sync_field(&report.extra, ","),
        render_skills_sync_field(&report.changed, ","),
        render_skills_sync_field(&report.metadata_mismatch, ";")
    ));
    if let Some(trust) = report.trust {
        lines.push(format!(
            "trust: total={} active={} revoked={} expired={}",
            trust.total, trust.active, trust.revoked, trust.expired
        ));
    } else {
        lines.push("trust: none".to_string());
    }

    if report.entries.is_empty() {
        lines.push("entry: none".to_string());
        return lines.join("\n");
    }

    for entry in &report.entries {
        lines.push(format!(
            "entry: file={} name={} status={} checks={}",
            entry.file,
            entry.name,
            entry.status.as_str(),
            entry.checks.join(";")
        ));
    }
    lines.join("\n")
}

fn execute_skills_verify_command(
    skills_dir: &Path,
    default_lock_path: &Path,
    default_trust_root_path: Option<&Path>,
    command_args: &str,
) -> String {
    let args =
        match parse_skills_verify_args(command_args, default_lock_path, default_trust_root_path) {
            Ok(args) => args,
            Err(error) => {
                return format!(
                    "skills verify error: path={} error={error}",
                    default_lock_path.display()
                );
            }
        };

    match build_skills_verify_report(skills_dir, &args.lock_path, args.trust_root_path.as_deref()) {
        Ok(report) => {
            if args.json_output {
                serde_json::to_string(&report).unwrap_or_else(|error| {
                    serde_json::json!({
                        "status": "error",
                        "path": args.lock_path.display().to_string(),
                        "error": format!("failed to serialize skills verify report: {error}"),
                    })
                    .to_string()
                })
            } else {
                render_skills_verify_report(&report)
            }
        }
        Err(error) => {
            if args.json_output {
                serde_json::json!({
                    "status": "error",
                    "path": args.lock_path.display().to_string(),
                    "error": error.to_string(),
                })
                .to_string()
            } else {
                format!(
                    "skills verify error: path={} error={error}",
                    args.lock_path.display()
                )
            }
        }
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

fn print_assistant_messages(
    messages: &[Message],
    render_options: RenderOptions,
    suppress_first_streamed_text: bool,
) {
    let mut suppressed_once = false;
    for message in messages {
        if message.role != MessageRole::Assistant {
            continue;
        }

        let text = message.text_content();
        if !text.trim().is_empty() {
            if render_options.stream_output && suppress_first_streamed_text && !suppressed_once {
                suppressed_once = true;
                println!("\n");
                continue;
            }
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
        AgentEvent::TurnEnd {
            turn,
            tool_results,
            request_duration_ms,
            usage,
            finish_reason,
        } => serde_json::json!({
            "type": "turn_end",
            "turn": turn,
            "tool_results": tool_results,
            "request_duration_ms": request_duration_ms,
            "usage": usage,
            "finish_reason": finish_reason,
        }),
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

type FallbackEventSink = Arc<dyn Fn(serde_json::Value) + Send + Sync>;

#[derive(Clone)]
struct ClientRoute {
    provider: Provider,
    model: String,
    client: Arc<dyn LlmClient>,
}

impl ClientRoute {
    fn model_ref(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }
}

struct FallbackRoutingClient {
    routes: Vec<ClientRoute>,
    event_sink: Option<FallbackEventSink>,
}

impl FallbackRoutingClient {
    fn new(routes: Vec<ClientRoute>, event_sink: Option<FallbackEventSink>) -> Self {
        Self { routes, event_sink }
    }

    fn emit_fallback_event(
        &self,
        from: &ClientRoute,
        to: &ClientRoute,
        error: &PiAiError,
        fallback_index: usize,
    ) {
        let Some(sink) = &self.event_sink else {
            return;
        };
        let (error_kind, status) = fallback_error_metadata(error);
        sink(serde_json::json!({
            "type": "provider_fallback",
            "from_model": from.model_ref(),
            "to_model": to.model_ref(),
            "error_kind": error_kind,
            "status": status,
            "fallback_index": fallback_index,
        }));
    }

    async fn complete_inner(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        if self.routes.is_empty() {
            return Err(PiAiError::InvalidResponse(
                "no provider routes configured".to_string(),
            ));
        }

        for (index, route) in self.routes.iter().enumerate() {
            let mut routed_request = request.clone();
            routed_request.model = route.model.clone();

            let response = if let Some(stream_handler) = on_delta.clone() {
                route
                    .client
                    .complete_with_stream(routed_request, Some(stream_handler))
                    .await
            } else {
                route.client.complete(routed_request).await
            };

            match response {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let Some(next_route) = self.routes.get(index + 1) else {
                        return Err(error);
                    };
                    if is_retryable_provider_error(&error) {
                        self.emit_fallback_event(route, next_route, &error, index + 1);
                        continue;
                    }
                    return Err(error);
                }
            }
        }

        Err(PiAiError::InvalidResponse(
            "provider fallback chain exhausted unexpectedly".to_string(),
        ))
    }
}

#[async_trait]
impl LlmClient for FallbackRoutingClient {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, PiAiError> {
        self.complete_inner(request, None).await
    }

    async fn complete_with_stream(
        &self,
        request: ChatRequest,
        on_delta: Option<StreamDeltaHandler>,
    ) -> Result<ChatResponse, PiAiError> {
        self.complete_inner(request, on_delta).await
    }
}

fn is_retryable_status(status: u16) -> bool {
    status == 408 || status == 409 || status == 425 || status == 429 || status >= 500
}

fn is_retryable_provider_error(error: &PiAiError) -> bool {
    match error {
        PiAiError::HttpStatus { status, .. } => is_retryable_status(*status),
        PiAiError::Http(inner) => {
            inner.is_timeout() || inner.is_connect() || inner.is_request() || inner.is_body()
        }
        _ => false,
    }
}

fn fallback_error_metadata(error: &PiAiError) -> (&'static str, Option<u16>) {
    match error {
        PiAiError::HttpStatus { status, .. } => ("http_status", Some(*status)),
        PiAiError::Http(inner) if inner.is_timeout() => ("http_timeout", None),
        PiAiError::Http(inner) if inner.is_connect() => ("http_connect", None),
        PiAiError::Http(inner) if inner.is_request() => ("http_request", None),
        PiAiError::Http(inner) if inner.is_body() => ("http_body", None),
        PiAiError::Http(_) => ("http_other", None),
        PiAiError::MissingApiKey => ("missing_api_key", None),
        PiAiError::Serde(_) => ("serde", None),
        PiAiError::InvalidResponse(_) => ("invalid_response", None),
    }
}

fn resolve_store_backed_provider_credential(
    cli: &Cli,
    provider: Provider,
    method: ProviderAuthMethod,
) -> Result<ResolvedProviderCredential> {
    let key = cli.credential_store_key.as_deref();
    let default_mode = resolve_credential_store_encryption_mode(cli);
    let mut store =
        load_credential_store(&cli.credential_store, default_mode, key).with_context(|| {
            format!(
                "failed to load provider credential store {}",
                cli.credential_store.display()
            )
        })?;
    let provider_key = provider.as_str().to_string();
    let Some(mut entry) = store.providers.get(&provider_key).cloned() else {
        return Err(reauth_required_error(
            provider,
            "credential store entry is missing",
        ));
    };

    if entry.auth_method != method {
        return Err(reauth_required_error(
            provider,
            "credential store auth mode does not match requested mode",
        ));
    }
    if entry.revoked {
        return Err(reauth_required_error(provider, "credential is revoked"));
    }

    let now_unix = current_unix_timestamp();
    let is_expired = entry
        .expires_unix
        .map(|value| value <= now_unix)
        .unwrap_or(false);
    let mut store_dirty = false;
    if is_expired {
        let Some(refresh_token) = entry
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
        else {
            return Err(reauth_required_error(
                provider,
                "credential expired and no refresh token is available",
            ));
        };

        match refresh_provider_access_token(provider, &refresh_token, now_unix) {
            Ok(refreshed) => {
                entry.access_token = Some(refreshed.access_token.clone());
                entry.refresh_token = refreshed.refresh_token.clone().or(Some(refresh_token));
                entry.expires_unix = refreshed.expires_unix;
                entry.revoked = false;
                store_dirty = true;
            }
            Err(error) => {
                if error.to_string().contains("revoked") {
                    entry.revoked = true;
                    store.providers.insert(provider_key.clone(), entry.clone());
                    let _ = save_credential_store(&cli.credential_store, &store, key);
                    return Err(reauth_required_error(
                        provider,
                        "refresh token has been revoked",
                    ));
                }
                return Err(reauth_required_error(provider, "credential refresh failed"));
            }
        }
    }

    let access_token = entry
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            reauth_required_error(
                provider,
                "credential store entry does not contain an access token",
            )
        })?;

    if store_dirty {
        store.providers.insert(provider_key, entry.clone());
        save_credential_store(&cli.credential_store, &store, key).with_context(|| {
            format!(
                "failed to persist refreshed provider credential store {}",
                cli.credential_store.display()
            )
        })?;
    }

    Ok(ResolvedProviderCredential {
        method,
        secret: Some(access_token),
        source: Some("credential_store".to_string()),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderAuthCapability {
    method: ProviderAuthMethod,
    supported: bool,
    reason: &'static str,
}

const OPENAI_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
    ProviderAuthCapability {
        method: ProviderAuthMethod::ApiKey,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::OauthToken,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::Adc,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::SessionToken,
        supported: true,
        reason: "supported",
    },
];

const ANTHROPIC_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
    ProviderAuthCapability {
        method: ProviderAuthMethod::ApiKey,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::OauthToken,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::Adc,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::SessionToken,
        supported: false,
        reason: "unsupported",
    },
];

const GOOGLE_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
    ProviderAuthCapability {
        method: ProviderAuthMethod::ApiKey,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::OauthToken,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::Adc,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::SessionToken,
        supported: false,
        reason: "unsupported",
    },
];

fn provider_auth_capabilities(provider: Provider) -> &'static [ProviderAuthCapability] {
    match provider {
        Provider::OpenAi => OPENAI_AUTH_CAPABILITIES,
        Provider::Anthropic => ANTHROPIC_AUTH_CAPABILITIES,
        Provider::Google => GOOGLE_AUTH_CAPABILITIES,
    }
}

fn provider_auth_capability(
    provider: Provider,
    method: ProviderAuthMethod,
) -> ProviderAuthCapability {
    provider_auth_capabilities(provider)
        .iter()
        .find(|capability| capability.method == method)
        .copied()
        .unwrap_or(ProviderAuthCapability {
            method,
            supported: false,
            reason: "unknown",
        })
}

fn configured_provider_auth_method(cli: &Cli, provider: Provider) -> ProviderAuthMethod {
    match provider {
        Provider::OpenAi => cli.openai_auth_mode.into(),
        Provider::Anthropic => cli.anthropic_auth_mode.into(),
        Provider::Google => cli.google_auth_mode.into(),
    }
}

fn configured_provider_auth_method_from_config(
    config: &AuthCommandConfig,
    provider: Provider,
) -> ProviderAuthMethod {
    match provider {
        Provider::OpenAi => config.openai_auth_mode,
        Provider::Anthropic => config.anthropic_auth_mode,
        Provider::Google => config.google_auth_mode,
    }
}

fn provider_auth_mode_flag(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "--openai-auth-mode",
        Provider::Anthropic => "--anthropic-auth-mode",
        Provider::Google => "--google-auth-mode",
    }
}

fn missing_provider_api_key_message(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => {
            "missing OpenAI API key. Set OPENAI_API_KEY, PI_API_KEY, --openai-api-key, or --api-key"
        }
        Provider::Anthropic => {
            "missing Anthropic API key. Set ANTHROPIC_API_KEY, PI_API_KEY, --anthropic-api-key, or --api-key"
        }
        Provider::Google => {
            "missing Google API key. Set GEMINI_API_KEY, GOOGLE_API_KEY, PI_API_KEY, --google-api-key, or --api-key"
        }
    }
}

fn provider_api_key_candidates_with_inputs(
    provider: Provider,
    api_key: Option<String>,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
    google_api_key: Option<String>,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            ("--openai-api-key", openai_api_key),
            ("--api-key", api_key),
            ("OPENAI_API_KEY", std::env::var("OPENAI_API_KEY").ok()),
            ("PI_API_KEY", std::env::var("PI_API_KEY").ok()),
        ],
        Provider::Anthropic => vec![
            ("--anthropic-api-key", anthropic_api_key),
            ("--api-key", api_key),
            ("ANTHROPIC_API_KEY", std::env::var("ANTHROPIC_API_KEY").ok()),
            ("PI_API_KEY", std::env::var("PI_API_KEY").ok()),
        ],
        Provider::Google => vec![
            ("--google-api-key", google_api_key),
            ("--api-key", api_key),
            ("GEMINI_API_KEY", std::env::var("GEMINI_API_KEY").ok()),
            ("GOOGLE_API_KEY", std::env::var("GOOGLE_API_KEY").ok()),
            ("PI_API_KEY", std::env::var("PI_API_KEY").ok()),
        ],
    }
}

fn provider_api_key_candidates(
    cli: &Cli,
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    provider_api_key_candidates_with_inputs(
        provider,
        cli.api_key.clone(),
        cli.openai_api_key.clone(),
        cli.anthropic_api_key.clone(),
        cli.google_api_key.clone(),
    )
}

fn resolve_non_empty_secret_with_source(
    candidates: Vec<(&'static str, Option<String>)>,
) -> Option<(String, String)> {
    candidates.into_iter().find_map(|(source, value)| {
        let value = value?;
        if value.trim().is_empty() {
            return None;
        }
        Some((value, source.to_string()))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedProviderCredential {
    method: ProviderAuthMethod,
    secret: Option<String>,
    source: Option<String>,
}

trait ProviderCredentialResolver {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ResolvedProviderCredential>;
}

struct CliProviderCredentialResolver<'a> {
    cli: &'a Cli,
}

impl ProviderCredentialResolver for CliProviderCredentialResolver<'_> {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ResolvedProviderCredential> {
        match method {
            ProviderAuthMethod::ApiKey => {
                let (secret, source) = resolve_non_empty_secret_with_source(
                    provider_api_key_candidates(self.cli, provider),
                )
                .ok_or_else(|| anyhow!(missing_provider_api_key_message(provider)))?;
                Ok(ResolvedProviderCredential {
                    method,
                    secret: Some(secret),
                    source: Some(source),
                })
            }
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken => {
                resolve_store_backed_provider_credential(self.cli, provider, method)
            }
            ProviderAuthMethod::Adc => Ok(ResolvedProviderCredential {
                method,
                secret: None,
                source: None,
            }),
        }
    }
}

fn resolve_fallback_models(cli: &Cli, primary: &ModelRef) -> Result<Vec<ModelRef>> {
    let mut resolved = Vec::new();
    for raw in &cli.fallback_model {
        let parsed = ModelRef::parse(raw)
            .map_err(|error| anyhow!("failed to parse --fallback-model '{}': {error}", raw))?;

        if parsed.provider == primary.provider && parsed.model == primary.model {
            continue;
        }

        if resolved.iter().any(|existing: &ModelRef| {
            existing.provider == parsed.provider && existing.model == parsed.model
        }) {
            continue;
        }

        resolved.push(parsed);
    }
    Ok(resolved)
}

fn build_client_with_fallbacks(
    cli: &Cli,
    primary: &ModelRef,
    fallback_models: &[ModelRef],
) -> Result<Arc<dyn LlmClient>> {
    let primary_client = build_provider_client(cli, primary.provider)
        .with_context(|| format!("failed to create {} client", primary.provider))?;
    if fallback_models.is_empty() {
        return Ok(primary_client);
    }

    let mut provider_clients: Vec<(Provider, Arc<dyn LlmClient>)> =
        vec![(primary.provider, primary_client.clone())];
    let mut routes = vec![ClientRoute {
        provider: primary.provider,
        model: primary.model.clone(),
        client: primary_client,
    }];

    for model_ref in fallback_models {
        let client = if let Some((_, existing)) = provider_clients
            .iter()
            .find(|(provider, _)| *provider == model_ref.provider)
        {
            existing.clone()
        } else {
            let created = build_provider_client(cli, model_ref.provider).with_context(|| {
                format!(
                    "failed to create {} client for fallback model '{}'",
                    model_ref.provider, model_ref.model
                )
            })?;
            provider_clients.push((model_ref.provider, created.clone()));
            created
        };

        routes.push(ClientRoute {
            provider: model_ref.provider,
            model: model_ref.model.clone(),
            client,
        });
    }

    let event_sink = if cli.json_events {
        Some(Arc::new(|event| println!("{event}")) as FallbackEventSink)
    } else {
        None
    };
    Ok(Arc::new(FallbackRoutingClient::new(routes, event_sink)))
}

fn build_provider_client(cli: &Cli, provider: Provider) -> Result<Arc<dyn LlmClient>> {
    let auth_mode = configured_provider_auth_method(cli, provider);
    let capability = provider_auth_capability(provider, auth_mode);
    if !capability.supported {
        bail!(
            "unsupported auth mode '{}' for provider '{}': {} (set {} api-key)",
            auth_mode.as_str(),
            provider.as_str(),
            capability.reason,
            provider_auth_mode_flag(provider),
        );
    }

    let resolver = CliProviderCredentialResolver { cli };
    let resolved = resolver.resolve(provider, auth_mode)?;
    let auth_source = resolved.source.as_deref().unwrap_or("none");

    match provider {
        Provider::OpenAi => {
            let api_key = resolved.secret.ok_or_else(|| {
                anyhow!(
                    "resolved auth mode '{}' for '{}' did not provide a credential",
                    resolved.method.as_str(),
                    provider.as_str()
                )
            })?;

            let client = OpenAiClient::new(OpenAiConfig {
                api_base: cli.api_base.clone(),
                api_key,
                organization: None,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            tracing::debug!(
                provider = provider.as_str(),
                auth_mode = resolved.method.as_str(),
                auth_source = auth_source,
                "provider auth resolved"
            );
            Ok(Arc::new(client))
        }
        Provider::Anthropic => {
            let api_key = resolved.secret.ok_or_else(|| {
                anyhow!(
                    "resolved auth mode '{}' for '{}' did not provide a credential",
                    resolved.method.as_str(),
                    provider.as_str()
                )
            })?;

            let client = AnthropicClient::new(AnthropicConfig {
                api_base: cli.anthropic_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            tracing::debug!(
                provider = provider.as_str(),
                auth_mode = resolved.method.as_str(),
                auth_source = auth_source,
                "provider auth resolved"
            );
            Ok(Arc::new(client))
        }
        Provider::Google => {
            let api_key = resolved.secret.ok_or_else(|| {
                anyhow!(
                    "resolved auth mode '{}' for '{}' did not provide a credential",
                    resolved.method.as_str(),
                    provider.as_str()
                )
            })?;

            let client = GoogleClient::new(GoogleConfig {
                api_base: cli.google_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            tracing::debug!(
                provider = provider.as_str(),
                auth_mode = resolved.method.as_str(),
                auth_source = auth_source,
                "provider auth resolved"
            );
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

const TOOL_POLICY_SCHEMA_VERSION: u32 = 2;

fn build_tool_policy(cli: &Cli) -> Result<ToolPolicy> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut roots = vec![cwd];
    roots.extend(cli.allow_path.clone());

    let mut policy = ToolPolicy::new(roots);
    policy.apply_preset(cli.tool_policy_preset.into());

    if cli.bash_timeout_ms != 120_000 {
        policy.bash_timeout_ms = cli.bash_timeout_ms.max(1);
    }
    if cli.max_tool_output_bytes != 16_000 {
        policy.max_command_output_bytes = cli.max_tool_output_bytes.max(128);
    }
    if cli.max_file_read_bytes != 1_000_000 {
        policy.max_file_read_bytes = cli.max_file_read_bytes.max(1_024);
    }
    if cli.max_file_write_bytes != 1_000_000 {
        policy.max_file_write_bytes = cli.max_file_write_bytes.max(1_024);
    }
    if cli.max_command_length != 4_096 {
        policy.max_command_length = cli.max_command_length.max(8);
    }
    if cli.allow_command_newlines {
        policy.allow_command_newlines = true;
    }
    if cli.bash_profile != CliBashProfile::Balanced {
        policy.set_bash_profile(cli.bash_profile.into());
    }
    if cli.os_sandbox_mode != CliOsSandboxMode::Off {
        policy.os_sandbox_mode = cli.os_sandbox_mode.into();
    }
    if !cli.os_sandbox_command.is_empty() {
        policy.os_sandbox_command = parse_sandbox_command_tokens(&cli.os_sandbox_command)?;
    }
    if !cli.enforce_regular_files {
        policy.enforce_regular_files = false;
    }
    if cli.bash_dry_run {
        policy.bash_dry_run = true;
    }
    if cli.tool_policy_trace {
        policy.tool_policy_trace = true;
    }
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
        "schema_version": TOOL_POLICY_SCHEMA_VERSION,
        "preset": tool_policy_preset_name(policy.policy_preset),
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
        "bash_dry_run": policy.bash_dry_run,
        "tool_policy_trace": policy.tool_policy_trace,
    })
}

fn build_profile_defaults(cli: &Cli) -> ProfileDefaults {
    ProfileDefaults {
        model: cli.model.clone(),
        fallback_models: cli.fallback_model.clone(),
        session: ProfileSessionDefaults {
            enabled: !cli.no_session,
            path: if cli.no_session {
                None
            } else {
                Some(cli.session.display().to_string())
            },
            import_mode: format!("{:?}", cli.session_import_mode).to_lowercase(),
        },
        policy: ProfilePolicyDefaults {
            tool_policy_preset: format!("{:?}", cli.tool_policy_preset).to_lowercase(),
            bash_profile: format!("{:?}", cli.bash_profile).to_lowercase(),
            bash_dry_run: cli.bash_dry_run,
            os_sandbox_mode: format!("{:?}", cli.os_sandbox_mode).to_lowercase(),
            enforce_regular_files: cli.enforce_regular_files,
            bash_timeout_ms: cli.bash_timeout_ms,
            max_command_length: cli.max_command_length,
            max_tool_output_bytes: cli.max_tool_output_bytes,
            max_file_read_bytes: cli.max_file_read_bytes,
            max_file_write_bytes: cli.max_file_write_bytes,
            allow_command_newlines: cli.allow_command_newlines,
        },
        auth: ProfileAuthDefaults {
            openai: cli.openai_auth_mode.into(),
            anthropic: cli.anthropic_auth_mode.into(),
            google: cli.google_auth_mode.into(),
        },
    }
}

fn provider_key_env_var(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "OPENAI_API_KEY",
        Provider::Anthropic => "ANTHROPIC_API_KEY",
        Provider::Google => "GEMINI_API_KEY",
    }
}

fn provider_key_present(cli: &Cli, provider: Provider) -> bool {
    match provider {
        Provider::OpenAi => {
            resolve_api_key(vec![cli.openai_api_key.clone(), cli.api_key.clone()]).is_some()
        }
        Provider::Anthropic => {
            resolve_api_key(vec![cli.anthropic_api_key.clone(), cli.api_key.clone()]).is_some()
        }
        Provider::Google => {
            resolve_api_key(vec![cli.google_api_key.clone(), cli.api_key.clone()]).is_some()
        }
    }
}

fn build_doctor_command_config(
    cli: &Cli,
    primary_model: &ModelRef,
    fallback_models: &[ModelRef],
    skills_lock_path: &Path,
) -> DoctorCommandConfig {
    let mut providers = Vec::new();
    providers.push(primary_model.provider);
    for model in fallback_models {
        if !providers.contains(&model.provider) {
            providers.push(model.provider);
        }
    }
    providers.sort_by_key(|provider| provider.as_str().to_string());
    let provider_keys = providers
        .into_iter()
        .map(|provider| {
            let auth_mode = configured_provider_auth_method(cli, provider);
            let capability = provider_auth_capability(provider, auth_mode);
            DoctorProviderKeyStatus {
                provider_kind: provider,
                provider: provider.as_str().to_string(),
                key_env_var: provider_key_env_var(provider).to_string(),
                present: provider_key_present(cli, provider),
                auth_mode,
                mode_supported: capability.supported,
            }
        })
        .collect::<Vec<_>>();

    DoctorCommandConfig {
        model: format!(
            "{}/{}",
            primary_model.provider.as_str(),
            primary_model.model
        ),
        provider_keys,
        session_enabled: !cli.no_session,
        session_path: cli.session.clone(),
        skills_dir: cli.skills_dir.clone(),
        skills_lock_path: skills_lock_path.to_path_buf(),
        trust_root_path: cli.skill_trust_root_file.clone(),
    }
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
        collections::{BTreeMap, HashMap, HashSet, VecDeque},
        future::{pending, ready},
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant},
    };

    use async_trait::async_trait;
    use clap::Parser;
    use pi_agent_core::{Agent, AgentConfig, AgentEvent, ToolExecutionResult};
    use pi_ai::{
        ChatRequest, ChatResponse, ChatUsage, ContentBlock, LlmClient, Message, MessageRole,
        ModelRef, PiAiError, Provider,
    };
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;
    use tokio::sync::Mutex as AsyncMutex;
    use tokio::time::sleep;

    use super::{
        apply_trust_root_mutations, branch_alias_path_for_session, build_auth_command_config,
        build_doctor_command_config, build_profile_defaults, build_provider_client,
        build_tool_policy, command_file_error_mode_label, compute_session_entry_depths,
        compute_session_stats, current_unix_timestamp, decrypt_credential_store_secret,
        default_macro_config_path, default_profile_store_path, default_skills_lock_path,
        derive_skills_prune_candidates, encrypt_credential_store_secret, ensure_non_empty_text,
        escape_graph_label, execute_auth_command, execute_branch_alias_command,
        execute_channel_store_admin_command, execute_command_file, execute_doctor_command,
        execute_integration_auth_command, execute_macro_command, execute_profile_command,
        execute_session_bookmark_command, execute_session_diff_command,
        execute_session_graph_export_command, execute_session_search_command,
        execute_session_stats_command, execute_skills_list_command,
        execute_skills_lock_diff_command, execute_skills_lock_write_command,
        execute_skills_prune_command, execute_skills_search_command, execute_skills_show_command,
        execute_skills_sync_command, execute_skills_trust_add_command,
        execute_skills_trust_list_command, execute_skills_trust_revoke_command,
        execute_skills_trust_rotate_command, execute_skills_verify_command, format_id_list,
        format_remap_ids, handle_command, handle_command_with_session_import_mode,
        initialize_session, is_retryable_provider_error, load_branch_aliases,
        load_credential_store, load_macro_file, load_profile_store, load_session_bookmarks,
        load_trust_root_records, parse_auth_command, parse_branch_alias_command, parse_command,
        parse_command_file, parse_doctor_command_args, parse_integration_auth_command,
        parse_macro_command, parse_profile_command, parse_sandbox_command_tokens,
        parse_session_bookmark_command, parse_session_diff_args, parse_session_search_args,
        parse_session_stats_args, parse_skills_lock_diff_args, parse_skills_prune_args,
        parse_skills_search_args, parse_skills_trust_list_args, parse_skills_trust_mutation_args,
        parse_skills_verify_args, parse_trust_rotation_spec, parse_trusted_root_spec,
        percentile_duration_ms, provider_auth_capability, refresh_provider_access_token,
        render_audit_summary, render_command_help, render_doctor_report, render_doctor_report_json,
        render_help_overview, render_macro_list, render_macro_show, render_profile_diffs,
        render_profile_list, render_profile_show, render_session_diff, render_session_graph_dot,
        render_session_graph_mermaid, render_session_stats, render_session_stats_json,
        render_skills_list, render_skills_lock_diff_drift, render_skills_lock_diff_in_sync,
        render_skills_lock_write_success, render_skills_search, render_skills_show,
        render_skills_sync_drift_details, render_skills_trust_list, render_skills_verify_report,
        resolve_credential_store_encryption_mode, resolve_fallback_models, resolve_prompt_input,
        resolve_prunable_skill_file_name, resolve_secret_from_cli_or_store_id,
        resolve_session_graph_format, resolve_skill_trust_roots, resolve_skills_lock_path,
        resolve_store_backed_provider_credential, resolve_system_prompt, run_doctor_checks,
        run_prompt_with_cancellation, save_branch_aliases, save_credential_store, save_macro_file,
        save_profile_store, save_session_bookmarks, search_session_entries,
        session_bookmark_path_for_session, session_message_preview, shared_lineage_prefix_depth,
        stream_text_chunks, summarize_audit_file, tool_audit_event_json, tool_policy_to_json,
        trust_record_status, unknown_command_message, validate_branch_alias_name,
        validate_event_webhook_ingest_cli, validate_events_runner_cli,
        validate_github_issues_bridge_cli, validate_macro_command_entry, validate_macro_name,
        validate_profile_name, validate_session_file, validate_skills_prune_file_name,
        validate_slack_bridge_cli, AuthCommand, AuthCommandConfig, BranchAliasCommand,
        BranchAliasFile, Cli, CliBashProfile, CliCommandFileErrorMode,
        CliCredentialStoreEncryptionMode, CliOsSandboxMode, CliProviderAuthMode,
        CliSessionImportMode, CliToolPolicyPreset, CliWebhookSignatureAlgorithm, ClientRoute,
        CommandAction, CommandExecutionContext, CommandFileEntry, CommandFileReport,
        CredentialStoreData, CredentialStoreEncryptionMode, DoctorCheckResult, DoctorCommandConfig,
        DoctorCommandOutputFormat, DoctorProviderKeyStatus, DoctorStatus, FallbackRoutingClient,
        IntegrationAuthCommand, IntegrationCredentialStoreRecord, MacroCommand, MacroFile,
        ProfileCommand, ProfileDefaults, ProfileStoreFile, PromptRunStatus, PromptTelemetryLogger,
        ProviderAuthMethod, ProviderCredentialStoreRecord, RenderOptions, SessionBookmarkCommand,
        SessionBookmarkFile, SessionDiffEntry, SessionDiffReport, SessionGraphFormat,
        SessionRuntime, SessionSearchArgs, SessionStats, SessionStatsOutputFormat, SkillsPruneMode,
        SkillsSyncCommandConfig, SkillsVerifyEntry, SkillsVerifyReport, SkillsVerifyStatus,
        SkillsVerifySummary, SkillsVerifyTrustSummary, ToolAuditLogger, TrustedRootRecord,
        BRANCH_ALIAS_SCHEMA_VERSION, BRANCH_ALIAS_USAGE, MACRO_SCHEMA_VERSION, MACRO_USAGE,
        PROFILE_SCHEMA_VERSION, PROFILE_USAGE, SESSION_BOOKMARK_SCHEMA_VERSION,
        SESSION_BOOKMARK_USAGE, SESSION_SEARCH_DEFAULT_RESULTS, SESSION_SEARCH_PREVIEW_CHARS,
        SKILLS_PRUNE_USAGE, SKILLS_TRUST_ADD_USAGE, SKILLS_TRUST_LIST_USAGE, SKILLS_VERIFY_USAGE,
    };
    use crate::resolve_api_key;
    use crate::session::{SessionImportMode, SessionStore};
    use crate::tools::{BashCommandProfile, OsSandboxMode, ToolPolicyPreset};

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

    struct SequenceClient {
        outcomes: AsyncMutex<VecDeque<Result<ChatResponse, PiAiError>>>,
    }

    #[async_trait]
    impl LlmClient for SequenceClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            let mut outcomes = self.outcomes.lock().await;
            outcomes.pop_front().unwrap_or_else(|| {
                Err(PiAiError::InvalidResponse(
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

    fn test_tool_policy_json() -> serde_json::Value {
        serde_json::json!({
            "allowed_roots": [],
            "bash_profile": "balanced",
        })
    }

    fn test_chat_request() -> ChatRequest {
        ChatRequest {
            model: "placeholder-model".to_string(),
            messages: vec![Message::user("hello")],
            tools: vec![],
            max_tokens: None,
            temperature: None,
        }
    }

    fn test_cli() -> Cli {
        Cli {
            model: "openai/gpt-4o-mini".to_string(),
            fallback_model: vec![],
            api_base: "https://api.openai.com/v1".to_string(),
            anthropic_api_base: "https://api.anthropic.com/v1".to_string(),
            google_api_base: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            api_key: None,
            openai_api_key: None,
            anthropic_api_key: None,
            google_api_key: None,
            openai_auth_mode: CliProviderAuthMode::ApiKey,
            anthropic_auth_mode: CliProviderAuthMode::ApiKey,
            google_auth_mode: CliProviderAuthMode::ApiKey,
            credential_store: PathBuf::from(".pi/credentials.json"),
            credential_store_key: None,
            credential_store_encryption: CliCredentialStoreEncryptionMode::Auto,
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
            skills_cache_dir: None,
            skills_offline: false,
            skill_trust_root: vec![],
            skill_trust_root_file: None,
            skill_trust_add: vec![],
            skill_trust_revoke: vec![],
            skill_trust_rotate: vec![],
            require_signed_skills: false,
            skills_lock_file: None,
            skills_lock_write: false,
            skills_sync: false,
            max_turns: 8,
            request_timeout_ms: 120_000,
            provider_max_retries: 2,
            provider_retry_budget_ms: 0,
            provider_retry_jitter: true,
            turn_timeout_ms: 0,
            json_events: false,
            stream_output: true,
            stream_delay_ms: 0,
            prompt: None,
            prompt_file: None,
            command_file: None,
            command_file_error_mode: CliCommandFileErrorMode::FailFast,
            channel_store_root: PathBuf::from(".pi/channel-store"),
            channel_store_inspect: None,
            channel_store_repair: None,
            events_runner: false,
            events_dir: PathBuf::from(".pi/events"),
            events_state_path: PathBuf::from(".pi/events/state.json"),
            events_poll_interval_ms: 1_000,
            events_queue_limit: 64,
            events_stale_immediate_max_age_seconds: 86_400,
            event_webhook_ingest_file: None,
            event_webhook_channel: None,
            event_webhook_prompt_prefix: "Handle webhook-triggered event.".to_string(),
            event_webhook_debounce_key: None,
            event_webhook_debounce_window_seconds: 60,
            event_webhook_signature: None,
            event_webhook_timestamp: None,
            event_webhook_secret: None,
            event_webhook_secret_id: None,
            event_webhook_signature_algorithm: None,
            event_webhook_signature_max_skew_seconds: 300,
            github_issues_bridge: false,
            github_repo: None,
            github_token: None,
            github_token_id: None,
            github_bot_login: None,
            github_api_base: "https://api.github.com".to_string(),
            github_state_dir: PathBuf::from(".pi/github-issues"),
            github_poll_interval_seconds: 30,
            github_include_issue_body: false,
            github_include_edited_comments: true,
            github_processed_event_cap: 10_000,
            github_retry_max_attempts: 4,
            github_retry_base_delay_ms: 500,
            slack_bridge: false,
            slack_app_token: None,
            slack_app_token_id: None,
            slack_bot_token: None,
            slack_bot_token_id: None,
            slack_bot_user_id: None,
            slack_api_base: "https://slack.com/api".to_string(),
            slack_state_dir: PathBuf::from(".pi/slack"),
            slack_thread_detail_output: true,
            slack_thread_detail_threshold_chars: 1500,
            slack_processed_event_cap: 10_000,
            slack_max_event_age_seconds: 7_200,
            slack_reconnect_delay_ms: 1_000,
            slack_retry_max_attempts: 4,
            slack_retry_base_delay_ms: 500,
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
            tool_policy_preset: CliToolPolicyPreset::Balanced,
            bash_dry_run: false,
            tool_policy_trace: false,
            allow_command: vec![],
            print_tool_policy: false,
            tool_audit_log: None,
            telemetry_log: None,
            os_sandbox_mode: CliOsSandboxMode::Off,
            os_sandbox_command: vec![],
            enforce_regular_files: true,
        }
    }

    fn write_test_provider_credential(
        path: &Path,
        encryption: CredentialStoreEncryptionMode,
        key: Option<&str>,
        provider: Provider,
        record: ProviderCredentialStoreRecord,
    ) {
        let mut store = CredentialStoreData {
            encryption,
            providers: BTreeMap::new(),
            integrations: BTreeMap::new(),
        };
        store
            .providers
            .insert(provider.as_str().to_string(), record);
        save_credential_store(path, &store, key).expect("save credential store");
    }

    fn write_test_integration_credential(
        path: &Path,
        encryption: CredentialStoreEncryptionMode,
        key: Option<&str>,
        integration_id: &str,
        record: IntegrationCredentialStoreRecord,
    ) {
        let mut store = CredentialStoreData {
            encryption,
            providers: BTreeMap::new(),
            integrations: BTreeMap::new(),
        };
        store
            .integrations
            .insert(integration_id.to_string(), record);
        save_credential_store(path, &store, key).expect("save credential store");
    }

    fn skills_command_config(
        skills_dir: &Path,
        lock_path: &Path,
        trust_root_path: Option<&Path>,
    ) -> SkillsSyncCommandConfig {
        SkillsSyncCommandConfig {
            skills_dir: skills_dir.to_path_buf(),
            default_lock_path: lock_path.to_path_buf(),
            default_trust_root_path: trust_root_path.map(Path::to_path_buf),
            doctor_config: DoctorCommandConfig {
                model: "openai/gpt-4o-mini".to_string(),
                provider_keys: vec![DoctorProviderKeyStatus {
                    provider_kind: Provider::OpenAi,
                    provider: "openai".to_string(),
                    key_env_var: "OPENAI_API_KEY".to_string(),
                    present: true,
                    auth_mode: ProviderAuthMethod::ApiKey,
                    mode_supported: true,
                }],
                session_enabled: true,
                session_path: PathBuf::from(".pi/sessions/default.jsonl"),
                skills_dir: skills_dir.to_path_buf(),
                skills_lock_path: lock_path.to_path_buf(),
                trust_root_path: trust_root_path.map(Path::to_path_buf),
            },
        }
    }

    fn test_profile_defaults() -> ProfileDefaults {
        build_profile_defaults(&test_cli())
    }

    fn test_auth_command_config() -> AuthCommandConfig {
        build_auth_command_config(&test_cli())
    }

    fn test_command_context<'a>(
        tool_policy_json: &'a serde_json::Value,
        profile_defaults: &'a ProfileDefaults,
        skills_command_config: &'a SkillsSyncCommandConfig,
        auth_command_config: &'a AuthCommandConfig,
    ) -> CommandExecutionContext<'a> {
        CommandExecutionContext {
            tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults,
            skills_command_config,
            auth_command_config,
        }
    }

    fn set_provider_auth_mode(
        config: &mut AuthCommandConfig,
        provider: Provider,
        mode: ProviderAuthMethod,
    ) {
        match provider {
            Provider::OpenAi => config.openai_auth_mode = mode,
            Provider::Anthropic => config.anthropic_auth_mode = mode,
            Provider::Google => config.google_auth_mode = mode,
        }
    }

    fn set_provider_api_key(config: &mut AuthCommandConfig, provider: Provider, value: &str) {
        match provider {
            Provider::OpenAi => config.openai_api_key = Some(value.to_string()),
            Provider::Anthropic => config.anthropic_api_key = Some(value.to_string()),
            Provider::Google => config.google_api_key = Some(value.to_string()),
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
    fn unit_cli_provider_retry_flags_accept_explicit_baseline_values() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--provider-max-retries",
            "2",
            "--provider-retry-budget-ms",
            "0",
            "--provider-retry-jitter",
            "true",
        ]);
        assert_eq!(cli.provider_max_retries, 2);
        assert_eq!(cli.provider_retry_budget_ms, 0);
        assert!(cli.provider_retry_jitter);
    }

    #[test]
    fn functional_cli_provider_retry_flags_accept_overrides() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--provider-max-retries",
            "5",
            "--provider-retry-budget-ms",
            "1500",
            "--provider-retry-jitter",
            "false",
        ]);
        assert_eq!(cli.provider_max_retries, 5);
        assert_eq!(cli.provider_retry_budget_ms, 1500);
        assert!(!cli.provider_retry_jitter);
    }

    #[test]
    fn unit_cli_provider_auth_mode_flags_default_to_api_key() {
        let cli = Cli::parse_from(["pi-rs"]);
        assert_eq!(cli.openai_auth_mode, CliProviderAuthMode::ApiKey);
        assert_eq!(cli.anthropic_auth_mode, CliProviderAuthMode::ApiKey);
        assert_eq!(cli.google_auth_mode, CliProviderAuthMode::ApiKey);
    }

    #[test]
    fn functional_cli_provider_auth_mode_flags_accept_overrides() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--openai-auth-mode",
            "oauth-token",
            "--anthropic-auth-mode",
            "session-token",
            "--google-auth-mode",
            "adc",
        ]);
        assert_eq!(cli.openai_auth_mode, CliProviderAuthMode::OauthToken);
        assert_eq!(cli.anthropic_auth_mode, CliProviderAuthMode::SessionToken);
        assert_eq!(cli.google_auth_mode, CliProviderAuthMode::Adc);
    }

    #[test]
    fn unit_cli_credential_store_flags_default_to_auto_mode_and_default_path() {
        let cli = Cli::parse_from(["pi-rs"]);
        assert_eq!(cli.credential_store, PathBuf::from(".pi/credentials.json"));
        assert!(cli.credential_store_key.is_none());
        assert_eq!(
            cli.credential_store_encryption,
            CliCredentialStoreEncryptionMode::Auto
        );
    }

    #[test]
    fn functional_cli_credential_store_flags_accept_explicit_overrides() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--credential-store",
            "custom/credentials.json",
            "--credential-store-key",
            "secret-store-key",
            "--credential-store-encryption",
            "keyed",
        ]);
        assert_eq!(
            cli.credential_store,
            PathBuf::from("custom/credentials.json")
        );
        assert_eq!(
            cli.credential_store_key.as_deref(),
            Some("secret-store-key")
        );
        assert_eq!(
            cli.credential_store_encryption,
            CliCredentialStoreEncryptionMode::Keyed
        );
    }

    #[test]
    fn unit_cli_integration_secret_id_flags_default_to_none() {
        let cli = Cli::parse_from(["pi-rs"]);
        assert!(cli.event_webhook_secret_id.is_none());
        assert!(cli.github_token_id.is_none());
        assert!(cli.slack_app_token_id.is_none());
        assert!(cli.slack_bot_token_id.is_none());
    }

    #[test]
    fn functional_cli_integration_secret_id_flags_accept_explicit_values() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--event-webhook-ingest-file",
            "payload.json",
            "--github-issues-bridge",
            "--slack-bridge",
            "--event-webhook-secret-id",
            "event-webhook-secret",
            "--github-token-id",
            "github-token",
            "--slack-app-token-id",
            "slack-app-token",
            "--slack-bot-token-id",
            "slack-bot-token",
        ]);
        assert_eq!(
            cli.event_webhook_secret_id.as_deref(),
            Some("event-webhook-secret")
        );
        assert_eq!(cli.github_token_id.as_deref(), Some("github-token"));
        assert_eq!(cli.slack_app_token_id.as_deref(), Some("slack-app-token"));
        assert_eq!(cli.slack_bot_token_id.as_deref(), Some("slack-bot-token"));
    }

    #[test]
    fn unit_parse_auth_command_supports_login_status_logout_and_json() {
        let login =
            parse_auth_command("login openai --mode oauth-token --json").expect("parse auth login");
        assert_eq!(
            login,
            AuthCommand::Login {
                provider: Provider::OpenAi,
                mode: Some(ProviderAuthMethod::OauthToken),
                json_output: true,
            }
        );

        let status = parse_auth_command("status anthropic --json").expect("parse auth status");
        assert_eq!(
            status,
            AuthCommand::Status {
                provider: Some(Provider::Anthropic),
                json_output: true,
            }
        );

        let logout = parse_auth_command("logout google").expect("parse auth logout");
        assert_eq!(
            logout,
            AuthCommand::Logout {
                provider: Provider::Google,
                json_output: false,
            }
        );
    }

    #[test]
    fn regression_parse_auth_command_rejects_unknown_provider_mode_and_usage_errors() {
        let unknown_provider =
            parse_auth_command("login mystery --mode oauth-token").expect_err("provider fail");
        assert!(unknown_provider.to_string().contains("unknown provider"));

        let unknown_mode =
            parse_auth_command("login openai --mode unknown").expect_err("mode fail");
        assert!(unknown_mode.to_string().contains("unknown auth mode"));

        let missing_login_provider = parse_auth_command("login").expect_err("usage fail for login");
        assert!(missing_login_provider
            .to_string()
            .contains("usage: /auth login"));

        let unknown_subcommand = parse_auth_command("noop").expect_err("subcommand fail");
        assert!(unknown_subcommand.to_string().contains("usage: /auth"));
    }

    #[test]
    fn unit_parse_integration_auth_command_supports_set_status_rotate_revoke_and_json() {
        let set = parse_integration_auth_command("set github-token ghp_token --json")
            .expect("parse integration set");
        assert_eq!(
            set,
            IntegrationAuthCommand::Set {
                integration_id: "github-token".to_string(),
                secret: "ghp_token".to_string(),
                json_output: true,
            }
        );

        let status = parse_integration_auth_command("status slack-app-token --json")
            .expect("parse integration status");
        assert_eq!(
            status,
            IntegrationAuthCommand::Status {
                integration_id: Some("slack-app-token".to_string()),
                json_output: true,
            }
        );

        let rotate = parse_integration_auth_command("rotate slack-bot-token next_secret")
            .expect("parse integration rotate");
        assert_eq!(
            rotate,
            IntegrationAuthCommand::Rotate {
                integration_id: "slack-bot-token".to_string(),
                secret: "next_secret".to_string(),
                json_output: false,
            }
        );

        let revoke = parse_integration_auth_command("revoke event-webhook-secret")
            .expect("parse integration revoke");
        assert_eq!(
            revoke,
            IntegrationAuthCommand::Revoke {
                integration_id: "event-webhook-secret".to_string(),
                json_output: false,
            }
        );
    }

    #[test]
    fn regression_parse_integration_auth_command_rejects_usage_and_invalid_ids() {
        let error = parse_integration_auth_command("set github-token").expect_err("missing secret");
        assert!(error
            .to_string()
            .contains("usage: /integration-auth set <integration-id> <secret> [--json]"));

        let error = parse_integration_auth_command("status bad$id").expect_err("invalid id");
        assert!(error.to_string().contains("contains unsupported character"));

        let error = parse_integration_auth_command("unknown").expect_err("unknown subcommand");
        assert!(error
            .to_string()
            .contains("usage: /integration-auth <set|status|rotate|revoke> ..."));
    }

    #[test]
    fn unit_auth_conformance_provider_capability_matrix_matches_expected_support() {
        let cases = vec![
            (
                Provider::OpenAi,
                ProviderAuthMethod::ApiKey,
                true,
                "supported",
            ),
            (
                Provider::OpenAi,
                ProviderAuthMethod::OauthToken,
                true,
                "supported",
            ),
            (
                Provider::OpenAi,
                ProviderAuthMethod::SessionToken,
                true,
                "supported",
            ),
            (
                Provider::OpenAi,
                ProviderAuthMethod::Adc,
                false,
                "not_implemented",
            ),
            (
                Provider::Anthropic,
                ProviderAuthMethod::ApiKey,
                true,
                "supported",
            ),
            (
                Provider::Anthropic,
                ProviderAuthMethod::OauthToken,
                false,
                "not_implemented",
            ),
            (
                Provider::Anthropic,
                ProviderAuthMethod::SessionToken,
                false,
                "unsupported",
            ),
            (
                Provider::Anthropic,
                ProviderAuthMethod::Adc,
                false,
                "not_implemented",
            ),
            (
                Provider::Google,
                ProviderAuthMethod::ApiKey,
                true,
                "supported",
            ),
            (
                Provider::Google,
                ProviderAuthMethod::OauthToken,
                false,
                "not_implemented",
            ),
            (
                Provider::Google,
                ProviderAuthMethod::SessionToken,
                false,
                "unsupported",
            ),
            (
                Provider::Google,
                ProviderAuthMethod::Adc,
                false,
                "not_implemented",
            ),
        ];

        for (provider, mode, expected_supported, expected_reason) in cases {
            let capability = provider_auth_capability(provider, mode);
            assert_eq!(capability.supported, expected_supported);
            assert_eq!(capability.reason, expected_reason);
        }
    }

    #[test]
    fn functional_auth_conformance_status_matrix_reports_expected_rows() {
        #[derive(Debug)]
        struct AuthConformanceCase {
            provider: Provider,
            mode: ProviderAuthMethod,
            api_key: Option<&'static str>,
            store_record: Option<ProviderCredentialStoreRecord>,
            expected_state: &'static str,
            expected_available: bool,
            expected_source: &'static str,
        }

        let temp = tempdir().expect("tempdir");
        let future_expiry = current_unix_timestamp().saturating_add(600);
        let cases = vec![
            AuthConformanceCase {
                provider: Provider::OpenAi,
                mode: ProviderAuthMethod::ApiKey,
                api_key: Some("openai-conformance-key"),
                store_record: None,
                expected_state: "ready",
                expected_available: true,
                expected_source: "--openai-api-key",
            },
            AuthConformanceCase {
                provider: Provider::Anthropic,
                mode: ProviderAuthMethod::ApiKey,
                api_key: Some("anthropic-conformance-key"),
                store_record: None,
                expected_state: "ready",
                expected_available: true,
                expected_source: "--anthropic-api-key",
            },
            AuthConformanceCase {
                provider: Provider::Google,
                mode: ProviderAuthMethod::ApiKey,
                api_key: Some("google-conformance-key"),
                store_record: None,
                expected_state: "ready",
                expected_available: true,
                expected_source: "--google-api-key",
            },
            AuthConformanceCase {
                provider: Provider::OpenAi,
                mode: ProviderAuthMethod::OauthToken,
                api_key: None,
                store_record: Some(ProviderCredentialStoreRecord {
                    auth_method: ProviderAuthMethod::OauthToken,
                    access_token: Some("oauth-access".to_string()),
                    refresh_token: Some("oauth-refresh".to_string()),
                    expires_unix: Some(future_expiry),
                    revoked: false,
                }),
                expected_state: "ready",
                expected_available: true,
                expected_source: "credential_store",
            },
            AuthConformanceCase {
                provider: Provider::OpenAi,
                mode: ProviderAuthMethod::SessionToken,
                api_key: None,
                store_record: Some(ProviderCredentialStoreRecord {
                    auth_method: ProviderAuthMethod::SessionToken,
                    access_token: Some("session-access".to_string()),
                    refresh_token: Some("session-refresh".to_string()),
                    expires_unix: Some(future_expiry),
                    revoked: false,
                }),
                expected_state: "ready",
                expected_available: true,
                expected_source: "credential_store",
            },
            AuthConformanceCase {
                provider: Provider::Anthropic,
                mode: ProviderAuthMethod::OauthToken,
                api_key: None,
                store_record: None,
                expected_state: "unsupported_mode",
                expected_available: false,
                expected_source: "none",
            },
            AuthConformanceCase {
                provider: Provider::Google,
                mode: ProviderAuthMethod::SessionToken,
                api_key: None,
                store_record: None,
                expected_state: "unsupported_mode",
                expected_available: false,
                expected_source: "none",
            },
        ];

        let mut matrix_rows = Vec::new();
        for (index, case) in cases.into_iter().enumerate() {
            let mut config = test_auth_command_config();
            config.credential_store = temp.path().join(format!("auth-conformance-{index}.json"));
            config.credential_store_encryption = CredentialStoreEncryptionMode::None;
            config.api_key = None;
            config.openai_api_key = None;
            config.anthropic_api_key = None;
            config.google_api_key = None;
            set_provider_auth_mode(&mut config, case.provider, case.mode);
            if let Some(api_key) = case.api_key {
                set_provider_api_key(&mut config, case.provider, api_key);
            }
            if let Some(record) = case.store_record {
                write_test_provider_credential(
                    &config.credential_store,
                    CredentialStoreEncryptionMode::None,
                    None,
                    case.provider,
                    record,
                );
            }

            let output = execute_auth_command(
                &config,
                &format!("status {} --json", case.provider.as_str()),
            );
            let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status");
            let row = &payload["entries"][0];
            matrix_rows.push(format!(
                "{}:{}:{}:{}",
                case.provider.as_str(),
                case.mode.as_str(),
                row["state"].as_str().unwrap_or("unknown"),
                row["available"].as_bool().unwrap_or(false)
            ));
            assert_eq!(row["provider"], case.provider.as_str());
            assert_eq!(row["mode"], case.mode.as_str());
            assert_eq!(row["state"], case.expected_state);
            assert_eq!(row["available"], case.expected_available);
            assert_eq!(row["source"], case.expected_source);
        }

        assert_eq!(
            matrix_rows,
            vec![
                "openai:api_key:ready:true",
                "anthropic:api_key:ready:true",
                "google:api_key:ready:true",
                "openai:oauth_token:ready:true",
                "openai:session_token:ready:true",
                "anthropic:oauth_token:unsupported_mode:false",
                "google:session_token:unsupported_mode:false",
            ]
        );
    }

    #[test]
    fn integration_auth_conformance_store_backed_status_matrix_handles_stale_token_scenarios() {
        #[derive(Debug)]
        struct StaleCase {
            mode: ProviderAuthMethod,
            record: ProviderCredentialStoreRecord,
            expected_state: &'static str,
            access_secret: &'static str,
            refresh_secret: Option<&'static str>,
        }

        let temp = tempdir().expect("tempdir");
        let now = current_unix_timestamp();
        let cases = vec![
            StaleCase {
                mode: ProviderAuthMethod::OauthToken,
                record: ProviderCredentialStoreRecord {
                    auth_method: ProviderAuthMethod::OauthToken,
                    access_token: Some("oauth-access-secret".to_string()),
                    refresh_token: Some("oauth-refresh-secret".to_string()),
                    expires_unix: Some(now.saturating_sub(1)),
                    revoked: false,
                },
                expected_state: "expired_refresh_pending",
                access_secret: "oauth-access-secret",
                refresh_secret: Some("oauth-refresh-secret"),
            },
            StaleCase {
                mode: ProviderAuthMethod::SessionToken,
                record: ProviderCredentialStoreRecord {
                    auth_method: ProviderAuthMethod::SessionToken,
                    access_token: Some("session-access-secret".to_string()),
                    refresh_token: None,
                    expires_unix: Some(now.saturating_sub(1)),
                    revoked: false,
                },
                expected_state: "expired",
                access_secret: "session-access-secret",
                refresh_secret: None,
            },
            StaleCase {
                mode: ProviderAuthMethod::SessionToken,
                record: ProviderCredentialStoreRecord {
                    auth_method: ProviderAuthMethod::SessionToken,
                    access_token: Some("revoked-access-secret".to_string()),
                    refresh_token: Some("revoked-refresh-secret".to_string()),
                    expires_unix: Some(now.saturating_add(60)),
                    revoked: true,
                },
                expected_state: "revoked",
                access_secret: "revoked-access-secret",
                refresh_secret: Some("revoked-refresh-secret"),
            },
            StaleCase {
                mode: ProviderAuthMethod::OauthToken,
                record: ProviderCredentialStoreRecord {
                    auth_method: ProviderAuthMethod::OauthToken,
                    access_token: None,
                    refresh_token: Some("missing-access-refresh-secret".to_string()),
                    expires_unix: Some(now.saturating_add(60)),
                    revoked: false,
                },
                expected_state: "missing_access_token",
                access_secret: "not-present-access-secret",
                refresh_secret: Some("missing-access-refresh-secret"),
            },
        ];

        for (index, case) in cases.into_iter().enumerate() {
            let mut config = test_auth_command_config();
            config.credential_store = temp.path().join(format!("auth-stale-{index}.json"));
            config.credential_store_encryption = CredentialStoreEncryptionMode::None;
            config.api_key = None;
            config.openai_api_key = None;
            set_provider_auth_mode(&mut config, Provider::OpenAi, case.mode);
            write_test_provider_credential(
                &config.credential_store,
                CredentialStoreEncryptionMode::None,
                None,
                Provider::OpenAi,
                case.record,
            );

            let json_output = execute_auth_command(&config, "status openai --json");
            let payload: serde_json::Value =
                serde_json::from_str(&json_output).expect("parse status json");
            let row = &payload["entries"][0];
            assert_eq!(row["provider"], "openai");
            assert_eq!(row["mode"], case.mode.as_str());
            assert_eq!(row["state"], case.expected_state);
            assert_eq!(row["available"], false);
            assert!(!json_output.contains(case.access_secret));
            if let Some(refresh_secret) = case.refresh_secret {
                assert!(!json_output.contains(refresh_secret));
            }

            let text_output = execute_auth_command(&config, "status openai");
            assert!(!text_output.contains(case.access_secret));
            if let Some(refresh_secret) = case.refresh_secret {
                assert!(!text_output.contains(refresh_secret));
            }
        }
    }

    #[test]
    fn regression_auth_security_matrix_blocks_unsupported_mode_bypass_attempts() {
        let unsupported_cases = vec![
            (Provider::Anthropic, ProviderAuthMethod::OauthToken),
            (Provider::Anthropic, ProviderAuthMethod::SessionToken),
            (Provider::Anthropic, ProviderAuthMethod::Adc),
            (Provider::Google, ProviderAuthMethod::OauthToken),
            (Provider::Google, ProviderAuthMethod::SessionToken),
            (Provider::Google, ProviderAuthMethod::Adc),
        ];

        for (provider, mode) in unsupported_cases {
            let capability = provider_auth_capability(provider, mode);
            assert!(!capability.supported);

            let output = execute_auth_command(
                &test_auth_command_config(),
                &format!(
                    "login {} --mode {} --json",
                    provider.as_str(),
                    mode.as_str()
                ),
            );
            let payload: serde_json::Value =
                serde_json::from_str(&output).expect("parse login output");
            assert_eq!(payload["command"], "auth.login");
            assert_eq!(payload["provider"], provider.as_str());
            assert_eq!(payload["mode"], mode.as_str());
            assert_eq!(payload["status"], "error");
            assert!(payload["reason"]
                .as_str()
                .unwrap_or_default()
                .contains("not supported"));
        }
    }

    #[test]
    fn functional_execute_auth_command_login_status_logout_lifecycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_auth_command_config();
        config.credential_store = temp.path().join("credentials.json");
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;
        config.openai_auth_mode = ProviderAuthMethod::OauthToken;

        let expires_unix = current_unix_timestamp().saturating_add(3600);
        std::env::set_var("OPENAI_ACCESS_TOKEN", "openai-access-token");
        std::env::set_var("OPENAI_REFRESH_TOKEN", "openai-refresh-token");
        std::env::set_var("OPENAI_AUTH_EXPIRES_UNIX", expires_unix.to_string());

        let login_output = execute_auth_command(&config, "login openai --json");
        let login_json: serde_json::Value =
            serde_json::from_str(&login_output).expect("parse login output");
        assert_eq!(login_json["status"], "saved");
        assert_eq!(login_json["provider"], "openai");
        assert_eq!(login_json["mode"], "oauth_token");
        assert_eq!(login_json["expires_unix"], expires_unix);

        let status_output = execute_auth_command(&config, "status openai --json");
        let status_json: serde_json::Value =
            serde_json::from_str(&status_output).expect("parse status output");
        assert_eq!(status_json["available"], 1);
        assert_eq!(status_json["entries"][0]["provider"], "openai");
        assert_eq!(status_json["entries"][0]["state"], "ready");
        assert_eq!(status_json["entries"][0]["source"], "credential_store");

        let logout_output = execute_auth_command(&config, "logout openai --json");
        let logout_json: serde_json::Value =
            serde_json::from_str(&logout_output).expect("parse logout output");
        assert_eq!(logout_json["status"], "revoked");

        let post_logout_status = execute_auth_command(&config, "status openai --json");
        let post_logout_json: serde_json::Value =
            serde_json::from_str(&post_logout_status).expect("parse post logout status");
        assert_eq!(post_logout_json["entries"][0]["state"], "revoked");
        assert_eq!(post_logout_json["entries"][0]["available"], false);

        std::env::remove_var("OPENAI_ACCESS_TOKEN");
        std::env::remove_var("OPENAI_REFRESH_TOKEN");
        std::env::remove_var("OPENAI_AUTH_EXPIRES_UNIX");
    }

    #[test]
    fn integration_execute_auth_command_status_reports_store_backed_state() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        write_test_provider_credential(
            &store_path,
            CredentialStoreEncryptionMode::None,
            None,
            Provider::OpenAi,
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::SessionToken,
                access_token: Some("session-access".to_string()),
                refresh_token: Some("session-refresh".to_string()),
                expires_unix: Some(current_unix_timestamp().saturating_add(1200)),
                revoked: false,
            },
        );

        let mut config = test_auth_command_config();
        config.credential_store = store_path;
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;
        config.openai_auth_mode = ProviderAuthMethod::SessionToken;

        let output = execute_auth_command(&config, "status openai --json");
        let payload: serde_json::Value = serde_json::from_str(&output).expect("parse status");
        assert_eq!(payload["entries"][0]["provider"], "openai");
        assert_eq!(payload["entries"][0]["mode"], "session_token");
        assert_eq!(payload["entries"][0]["state"], "ready");
        assert_eq!(payload["entries"][0]["available"], true);
    }

    #[test]
    fn functional_execute_integration_auth_command_set_status_rotate_revoke_lifecycle() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_auth_command_config();
        config.credential_store = temp.path().join("integration-credentials.json");
        config.credential_store_encryption = CredentialStoreEncryptionMode::None;

        let set_output =
            execute_integration_auth_command(&config, "set github-token ghp_secret --json");
        let set_json: serde_json::Value = serde_json::from_str(&set_output).expect("parse set");
        assert_eq!(set_json["command"], "integration_auth.set");
        assert_eq!(set_json["integration_id"], "github-token");
        assert_eq!(set_json["status"], "saved");
        assert!(!set_output.contains("ghp_secret"));

        let status_output = execute_integration_auth_command(&config, "status github-token --json");
        let status_json: serde_json::Value =
            serde_json::from_str(&status_output).expect("parse status");
        assert_eq!(status_json["available"], 1);
        assert_eq!(status_json["entries"][0]["integration_id"], "github-token");
        assert_eq!(status_json["entries"][0]["state"], "ready");
        assert_eq!(status_json["entries"][0]["revoked"], false);
        assert!(!status_output.contains("ghp_secret"));

        let rotate_output =
            execute_integration_auth_command(&config, "rotate github-token ghp_rotated --json");
        let rotate_json: serde_json::Value =
            serde_json::from_str(&rotate_output).expect("parse rotate");
        assert_eq!(rotate_json["command"], "integration_auth.rotate");
        assert_eq!(rotate_json["status"], "rotated");
        assert!(!rotate_output.contains("ghp_rotated"));

        let revoke_output = execute_integration_auth_command(&config, "revoke github-token --json");
        let revoke_json: serde_json::Value =
            serde_json::from_str(&revoke_output).expect("parse revoke");
        assert_eq!(revoke_json["command"], "integration_auth.revoke");
        assert_eq!(revoke_json["status"], "revoked");

        let post_revoke_status =
            execute_integration_auth_command(&config, "status github-token --json");
        let post_revoke_json: serde_json::Value =
            serde_json::from_str(&post_revoke_status).expect("parse status");
        assert_eq!(post_revoke_json["entries"][0]["state"], "revoked");
        assert_eq!(post_revoke_json["entries"][0]["available"], false);

        let store = load_credential_store(
            &config.credential_store,
            CredentialStoreEncryptionMode::None,
            None,
        )
        .expect("load credential store");
        let entry = store
            .integrations
            .get("github-token")
            .expect("github integration entry");
        assert!(entry.secret.is_none());
        assert!(entry.revoked);
    }

    #[test]
    fn regression_execute_auth_command_login_rejects_unsupported_provider_mode() {
        let config = test_auth_command_config();
        let output = execute_auth_command(&config, "login google --mode oauth-token --json");
        let payload: serde_json::Value = serde_json::from_str(&output).expect("parse output");
        assert_eq!(payload["status"], "error");
        assert!(payload["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("not supported"));
    }

    #[test]
    fn unit_cli_skills_lock_flags_default_to_disabled() {
        let cli = Cli::parse_from(["pi-rs"]);
        assert!(!cli.skills_lock_write);
        assert!(!cli.skills_sync);
        assert!(cli.skills_lock_file.is_none());
    }

    #[test]
    fn functional_cli_skills_lock_flags_accept_explicit_values() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--skills-lock-write",
            "--skills-sync",
            "--skills-lock-file",
            "custom/skills.lock.json",
        ]);
        assert!(cli.skills_lock_write);
        assert!(cli.skills_sync);
        assert_eq!(
            cli.skills_lock_file,
            Some(PathBuf::from("custom/skills.lock.json"))
        );
    }

    #[test]
    fn unit_cli_skills_cache_flags_default_to_online_mode() {
        let cli = Cli::parse_from(["pi-rs"]);
        assert!(!cli.skills_offline);
        assert!(cli.skills_cache_dir.is_none());
    }

    #[test]
    fn functional_cli_skills_cache_flags_accept_explicit_values() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--skills-offline",
            "--skills-cache-dir",
            "custom/skills-cache",
        ]);
        assert!(cli.skills_offline);
        assert_eq!(
            cli.skills_cache_dir,
            Some(PathBuf::from("custom/skills-cache"))
        );
    }

    #[test]
    fn unit_cli_command_file_flags_default_to_disabled() {
        let cli = Cli::parse_from(["pi-rs"]);
        assert!(cli.command_file.is_none());
        assert_eq!(
            cli.command_file_error_mode,
            CliCommandFileErrorMode::FailFast
        );
    }

    #[test]
    fn functional_cli_command_file_flags_accept_overrides() {
        let cli = Cli::parse_from([
            "pi-rs",
            "--command-file",
            "automation.commands",
            "--command-file-error-mode",
            "continue-on-error",
        ]);
        assert_eq!(cli.command_file, Some(PathBuf::from("automation.commands")));
        assert_eq!(
            cli.command_file_error_mode,
            CliCommandFileErrorMode::ContinueOnError
        );
    }

    #[test]
    fn unit_is_retryable_provider_error_classifies_status_errors() {
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 429,
            body: "rate limited".to_string(),
        }));
        assert!(is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 503,
            body: "unavailable".to_string(),
        }));
        assert!(!is_retryable_provider_error(&PiAiError::HttpStatus {
            status: 401,
            body: "unauthorized".to_string(),
        }));
        assert!(!is_retryable_provider_error(&PiAiError::InvalidResponse(
            "bad payload".to_string(),
        )));
    }

    #[test]
    fn functional_resolve_fallback_models_parses_deduplicates_and_skips_primary() {
        let primary = ModelRef::parse("openai/gpt-4o-mini").expect("primary model parse");
        let mut cli = test_cli();
        cli.fallback_model = vec![
            "openai/gpt-4o-mini".to_string(),
            "google/gemini-2.5-pro".to_string(),
            "google/gemini-2.5-pro".to_string(),
            "anthropic/claude-sonnet-4-20250514".to_string(),
        ];

        let resolved = resolve_fallback_models(&cli, &primary).expect("resolve fallbacks");
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].provider, Provider::Google);
        assert_eq!(resolved[0].model, "gemini-2.5-pro");
        assert_eq!(resolved[1].provider, Provider::Anthropic);
    }

    #[tokio::test]
    async fn functional_fallback_routing_client_uses_next_route_for_retryable_error() {
        let primary = Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([Err(PiAiError::HttpStatus {
                status: 503,
                body: "unavailable".to_string(),
            })])),
        });
        let fallback = Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
                message: Message::assistant_text("fallback success"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })])),
        });

        let client = FallbackRoutingClient::new(
            vec![
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-primary".to_string(),
                    client: primary as Arc<dyn LlmClient>,
                },
                ClientRoute {
                    provider: Provider::Anthropic,
                    model: "claude-fallback".to_string(),
                    client: fallback as Arc<dyn LlmClient>,
                },
            ],
            None,
        );

        let response = client
            .complete(test_chat_request())
            .await
            .expect("fallback should recover request");
        assert_eq!(response.message.text_content(), "fallback success");
    }

    #[tokio::test]
    async fn regression_fallback_routing_client_skips_fallback_on_non_retryable_error() {
        let primary = Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([Err(PiAiError::HttpStatus {
                status: 400,
                body: "bad request".to_string(),
            })])),
        });
        let fallback = Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
                message: Message::assistant_text("should not run"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })])),
        });

        let client = FallbackRoutingClient::new(
            vec![
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-primary".to_string(),
                    client: primary as Arc<dyn LlmClient>,
                },
                ClientRoute {
                    provider: Provider::Google,
                    model: "gemini-fallback".to_string(),
                    client: fallback.clone() as Arc<dyn LlmClient>,
                },
            ],
            None,
        );

        let error = client
            .complete(test_chat_request())
            .await
            .expect_err("non-retryable error should return immediately");
        match error {
            PiAiError::HttpStatus { status, body } => {
                assert_eq!(status, 400);
                assert!(body.contains("bad request"));
            }
            other => panic!("expected HttpStatus error, got {other:?}"),
        }

        let fallback_remaining = fallback.outcomes.lock().await.len();
        assert_eq!(
            fallback_remaining, 1,
            "fallback route should not be invoked"
        );
    }

    #[tokio::test]
    async fn integration_fallback_routing_client_emits_json_event_on_failover() {
        let primary = Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([Err(PiAiError::HttpStatus {
                status: 429,
                body: "rate limited".to_string(),
            })])),
        });
        let fallback = Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([Ok(ChatResponse {
                message: Message::assistant_text("fallback ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })])),
        });
        let events = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let sink_events = events.clone();
        let sink = Arc::new(move |event: serde_json::Value| {
            sink_events.lock().expect("event lock").push(event);
        });

        let client = FallbackRoutingClient::new(
            vec![
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-primary".to_string(),
                    client: primary as Arc<dyn LlmClient>,
                },
                ClientRoute {
                    provider: Provider::OpenAi,
                    model: "gpt-fallback".to_string(),
                    client: fallback as Arc<dyn LlmClient>,
                },
            ],
            Some(sink),
        );

        let _ = client
            .complete(test_chat_request())
            .await
            .expect("fallback should succeed");

        let events = events.lock().expect("event lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["type"], "provider_fallback");
        assert_eq!(events[0]["from_model"], "openai/gpt-primary");
        assert_eq!(events[0]["to_model"], "openai/gpt-fallback");
        assert_eq!(events[0]["error_kind"], "http_status");
        assert_eq!(events[0]["status"], 429);
        assert_eq!(events[0]["fallback_index"], 1);
    }

    #[test]
    fn resolve_api_key_returns_none_when_all_candidates_are_empty() {
        let key = resolve_api_key(vec![None, Some("".to_string())]);
        assert!(key.is_none());
    }

    #[test]
    fn unit_provider_auth_capability_reports_api_key_support() {
        let openai = provider_auth_capability(Provider::OpenAi, ProviderAuthMethod::ApiKey);
        assert!(openai.supported);
        assert_eq!(openai.reason, "supported");

        let google = provider_auth_capability(Provider::Google, ProviderAuthMethod::OauthToken);
        assert!(!google.supported);
        assert_eq!(google.reason, "not_implemented");
    }

    #[test]
    fn regression_build_provider_client_rejects_unsupported_auth_mode() {
        let mut cli = test_cli();
        cli.google_auth_mode = CliProviderAuthMode::OauthToken;

        match build_provider_client(&cli, Provider::Google) {
            Ok(_) => panic!("unsupported auth mode should fail"),
            Err(error) => {
                assert!(error.to_string().contains("unsupported auth mode"));
                assert!(error.to_string().contains("--google-auth-mode api-key"));
            }
        }
    }

    #[test]
    fn integration_build_provider_client_preserves_api_key_mode_behavior() {
        let mut cli = test_cli();
        cli.openai_api_key = Some("test-openai-key".to_string());

        let client = build_provider_client(&cli, Provider::OpenAi).expect("build client");
        let ptr = Arc::as_ptr(&client);
        assert!(!ptr.is_null());
    }

    #[test]
    fn unit_encrypt_and_decrypt_credential_store_secret_roundtrip_keyed() {
        let secret = "secret-token-123";
        let encoded = encrypt_credential_store_secret(
            secret,
            CredentialStoreEncryptionMode::Keyed,
            Some("very-strong-key"),
        )
        .expect("encode credential");
        assert!(encoded.starts_with("enc:v1:"));
        assert!(!encoded.contains(secret));

        let decoded = decrypt_credential_store_secret(
            &encoded,
            CredentialStoreEncryptionMode::Keyed,
            Some("very-strong-key"),
        )
        .expect("decode credential");
        assert_eq!(decoded, secret);
    }

    #[test]
    fn regression_decrypt_credential_store_secret_rejects_wrong_key() {
        let encoded = encrypt_credential_store_secret(
            "secret-token-xyz",
            CredentialStoreEncryptionMode::Keyed,
            Some("correct-key-123"),
        )
        .expect("encode credential");

        let error = decrypt_credential_store_secret(
            &encoded,
            CredentialStoreEncryptionMode::Keyed,
            Some("wrong-key-123"),
        )
        .expect_err("wrong key should fail");
        assert!(error.to_string().contains("integrity check failed"));
    }

    #[test]
    fn functional_credential_store_roundtrip_preserves_provider_records() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        write_test_provider_credential(
            &store_path,
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-key"),
            Provider::OpenAi,
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("openai-access".to_string()),
                refresh_token: Some("openai-refresh".to_string()),
                expires_unix: Some(12345),
                revoked: false,
            },
        );

        let loaded = load_credential_store(
            &store_path,
            CredentialStoreEncryptionMode::None,
            Some("credential-key"),
        )
        .expect("load credential store");
        let entry = loaded
            .providers
            .get("openai")
            .expect("openai entry should exist");
        assert_eq!(entry.auth_method, ProviderAuthMethod::OauthToken);
        assert_eq!(entry.access_token.as_deref(), Some("openai-access"));
        assert_eq!(entry.refresh_token.as_deref(), Some("openai-refresh"));
        assert_eq!(entry.expires_unix, Some(12345));
        assert!(!entry.revoked);
    }

    #[test]
    fn integration_credential_store_roundtrip_preserves_integration_records() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("integration-credentials.json");
        write_test_integration_credential(
            &store_path,
            CredentialStoreEncryptionMode::Keyed,
            Some("credential-key"),
            "github-token",
            IntegrationCredentialStoreRecord {
                secret: Some("ghp_top_secret".to_string()),
                revoked: false,
                updated_unix: Some(98765),
            },
        );

        let loaded = load_credential_store(
            &store_path,
            CredentialStoreEncryptionMode::None,
            Some("credential-key"),
        )
        .expect("load credential store");
        let entry = loaded
            .integrations
            .get("github-token")
            .expect("integration entry");
        assert_eq!(entry.secret.as_deref(), Some("ghp_top_secret"));
        assert!(!entry.revoked);
        assert_eq!(entry.updated_unix, Some(98765));
    }

    #[test]
    fn regression_load_credential_store_allows_legacy_provider_only_payload() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("legacy-credentials.json");
        std::fs::write(
            &store_path,
            r#"{
  "schema_version": 1,
  "encryption": "none",
  "providers": {
    "openai": {
      "auth_method": "oauth_token",
      "access_token": "legacy-access",
      "refresh_token": "legacy-refresh",
      "expires_unix": 42,
      "revoked": false
    }
  }
}
"#,
        )
        .expect("write legacy credential store");

        let loaded = load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
            .expect("load legacy credential store");
        assert!(loaded.integrations.is_empty());
        assert_eq!(
            loaded
                .providers
                .get("openai")
                .and_then(|entry| entry.access_token.as_deref()),
            Some("legacy-access")
        );
    }

    #[test]
    fn functional_resolve_store_backed_provider_credential_refreshes_expired_token() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        let now = current_unix_timestamp();

        write_test_provider_credential(
            &store_path,
            CredentialStoreEncryptionMode::None,
            None,
            Provider::OpenAi,
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("stale-access".to_string()),
                refresh_token: Some("refresh-token".to_string()),
                expires_unix: Some(now.saturating_sub(30)),
                revoked: false,
            },
        );

        let mut cli = test_cli();
        cli.credential_store = store_path.clone();
        cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
        cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

        let resolved = resolve_store_backed_provider_credential(
            &cli,
            Provider::OpenAi,
            ProviderAuthMethod::OauthToken,
        )
        .expect("resolve refreshed credential");
        assert_eq!(resolved.method, ProviderAuthMethod::OauthToken);
        assert_eq!(resolved.source.as_deref(), Some("credential_store"));
        let access = resolved.secret.expect("access token");
        assert!(access.starts_with("openai_access_"));
        assert_ne!(access, "stale-access");

        let persisted =
            load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
                .expect("reload store");
        let entry = persisted.providers.get("openai").expect("openai entry");
        assert_eq!(entry.access_token.as_deref(), Some(access.as_str()));
        assert!(entry.expires_unix.unwrap_or(0) > now);
    }

    #[test]
    fn functional_refresh_provider_access_token_generates_deterministic_shape() {
        let refreshed = refresh_provider_access_token(Provider::OpenAi, "refresh-token", 1700)
            .expect("refresh token");
        assert!(refreshed.access_token.starts_with("openai_access_"));
        assert!(refreshed
            .refresh_token
            .as_deref()
            .unwrap_or_default()
            .starts_with("openai_refresh_"));
        assert_eq!(refreshed.expires_unix, Some(1700 + 3600));
    }

    #[test]
    fn regression_resolve_store_backed_provider_credential_marks_revoked_refresh_token() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        let now = current_unix_timestamp();

        write_test_provider_credential(
            &store_path,
            CredentialStoreEncryptionMode::None,
            None,
            Provider::OpenAi,
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("stale-access".to_string()),
                refresh_token: Some("revoked-refresh-token".to_string()),
                expires_unix: Some(now.saturating_sub(5)),
                revoked: false,
            },
        );

        let mut cli = test_cli();
        cli.credential_store = store_path.clone();
        cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
        cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

        let error = resolve_store_backed_provider_credential(
            &cli,
            Provider::OpenAi,
            ProviderAuthMethod::OauthToken,
        )
        .expect_err("revoked refresh should require re-auth");
        assert!(error.to_string().contains("requires re-authentication"));
        assert!(error.to_string().contains("revoked"));

        let persisted =
            load_credential_store(&store_path, CredentialStoreEncryptionMode::None, None)
                .expect("reload store");
        let entry = persisted.providers.get("openai").expect("openai entry");
        assert!(entry.revoked);
    }

    #[test]
    fn regression_resolve_store_backed_provider_credential_hides_corrupted_payload_values() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        let leaked_value = "leaky-secret-token";
        let payload = format!(
            "{{\"schema_version\":1,\"encryption\":\"keyed\",\"providers\":{{\"openai\":{{\"auth_method\":\"oauth_token\",\"access_token\":\"enc:v1:not-base64-{leaked_value}\",\"refresh_token\":null,\"expires_unix\":null,\"revoked\":false}}}}}}"
        );
        std::fs::write(&store_path, payload).expect("write corrupted store");

        let mut cli = test_cli();
        cli.credential_store = store_path;
        cli.credential_store_key = Some("valid-key-123".to_string());
        cli.credential_store_encryption = CliCredentialStoreEncryptionMode::Keyed;
        cli.openai_auth_mode = CliProviderAuthMode::OauthToken;

        let error = resolve_store_backed_provider_credential(
            &cli,
            Provider::OpenAi,
            ProviderAuthMethod::OauthToken,
        )
        .expect_err("corrupted store should fail");
        let message = error.to_string();
        assert!(
            message.contains("failed to load provider credential store")
                || message.contains("invalid or corrupted")
        );
        assert!(!error.to_string().contains(leaked_value));
    }

    #[test]
    fn integration_build_provider_client_supports_openai_oauth_from_credential_store() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        write_test_provider_credential(
            &store_path,
            CredentialStoreEncryptionMode::None,
            None,
            Provider::OpenAi,
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("openai-oauth-access".to_string()),
                refresh_token: Some("refresh-token".to_string()),
                expires_unix: Some(current_unix_timestamp().saturating_add(900)),
                revoked: false,
            },
        );

        let mut cli = test_cli();
        cli.openai_auth_mode = CliProviderAuthMode::OauthToken;
        cli.credential_store = store_path;
        cli.credential_store_encryption = CliCredentialStoreEncryptionMode::None;

        let client = build_provider_client(&cli, Provider::OpenAi).expect("build oauth client");
        let ptr = Arc::as_ptr(&client);
        assert!(!ptr.is_null());
    }

    #[test]
    fn unit_resolve_credential_store_encryption_mode_auto_uses_key_presence() {
        let mut cli = test_cli();
        cli.credential_store_encryption = CliCredentialStoreEncryptionMode::Auto;
        cli.credential_store_key = None;
        assert_eq!(
            resolve_credential_store_encryption_mode(&cli),
            CredentialStoreEncryptionMode::None
        );

        cli.credential_store_key = Some("configured-key".to_string());
        assert_eq!(
            resolve_credential_store_encryption_mode(&cli),
            CredentialStoreEncryptionMode::Keyed
        );
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
    fn unit_parse_session_search_args_supports_query_role_and_limit() {
        assert_eq!(
            parse_session_search_args("  retry budget  ").expect("parse query"),
            SessionSearchArgs {
                query: "retry budget".to_string(),
                role: None,
                limit: SESSION_SEARCH_DEFAULT_RESULTS,
            }
        );
        assert_eq!(
            parse_session_search_args("target --role user --limit 5").expect("parse flags"),
            SessionSearchArgs {
                query: "target".to_string(),
                role: Some("user".to_string()),
                limit: 5,
            }
        );
        assert_eq!(
            parse_session_search_args("--role=assistant --limit=9 delta").expect("parse inline"),
            SessionSearchArgs {
                query: "delta".to_string(),
                role: Some("assistant".to_string()),
                limit: 9,
            }
        );
    }

    #[test]
    fn regression_parse_session_search_args_rejects_invalid_role_limit_and_flags() {
        let empty = parse_session_search_args(" \n\t ").expect_err("empty query should fail");
        assert!(empty.to_string().contains("query is required"));

        let invalid_role =
            parse_session_search_args("retry --role owner").expect_err("invalid role should fail");
        assert!(invalid_role.to_string().contains("invalid role"));

        let invalid_limit =
            parse_session_search_args("retry --limit 0").expect_err("limit zero should fail");
        assert!(invalid_limit
            .to_string()
            .contains("limit must be greater than 0"));

        let too_large = parse_session_search_args("retry --limit 9999")
            .expect_err("too large limit should fail");
        assert!(too_large.to_string().contains("exceeds maximum"));

        let missing_role =
            parse_session_search_args("retry --role").expect_err("missing role value should fail");
        assert!(missing_role
            .to_string()
            .contains("missing value for --role"));

        let unknown_flag =
            parse_session_search_args("retry --unknown").expect_err("unknown flag should fail");
        assert!(unknown_flag.to_string().contains("unknown flag"));
    }

    #[test]
    fn unit_session_message_preview_normalizes_whitespace_and_truncates() {
        let message = Message::user(format!(
            "line one\nline two {}",
            "x".repeat(SESSION_SEARCH_PREVIEW_CHARS)
        ));
        let preview = session_message_preview(&message);
        assert!(preview.starts_with("line one line two"));
        assert!(preview.ends_with("..."));
    }

    #[test]
    fn unit_search_session_entries_matches_role_and_text_case_insensitively() {
        let entries = vec![
            crate::session::SessionEntry {
                id: 2,
                parent_id: Some(1),
                message: Message::assistant_text("Budget stabilized"),
            },
            crate::session::SessionEntry {
                id: 1,
                parent_id: None,
                message: Message::user("Root question"),
            },
        ];

        let (role_matches, role_total) = search_session_entries(&entries, "USER", None, 10);
        assert_eq!(role_total, 1);
        assert_eq!(role_matches[0].id, 1);
        assert_eq!(role_matches[0].role, "user");

        let (text_matches, text_total) = search_session_entries(&entries, "budget", None, 10);
        assert_eq!(text_total, 1);
        assert_eq!(text_matches[0].id, 2);
        assert_eq!(text_matches[0].role, "assistant");

        let (assistant_only, assistant_total) =
            search_session_entries(&entries, "budget", Some("assistant"), 10);
        assert_eq!(assistant_total, 1);
        assert_eq!(assistant_only[0].id, 2);
    }

    #[test]
    fn functional_execute_session_search_command_renders_result_rows() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let head = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append root");
        let head = store
            .append_messages(head, &[Message::user("Retry budget fix in progress")])
            .expect("append user");
        let runtime = SessionRuntime {
            store,
            active_head: head,
        };

        let output = execute_session_search_command(&runtime, "retry");
        assert!(output.contains("session search: query=\"retry\" role=any"));
        assert!(output.contains("matches=1"));
        assert!(output.contains("shown=1"));
        assert!(output.contains("limit=50"));
        assert!(output.contains("result: id=2 parent=1 role=user"));
        assert!(output.contains("preview=Retry budget fix in progress"));
    }

    #[test]
    fn regression_search_session_entries_caps_huge_result_sets() {
        let entries = (1..=200)
            .map(|id| crate::session::SessionEntry {
                id,
                parent_id: if id == 1 { None } else { Some(id - 1) },
                message: Message::user(format!("needle-{id}")),
            })
            .collect::<Vec<_>>();
        let (matches, total_matches) =
            search_session_entries(&entries, "needle", None, SESSION_SEARCH_DEFAULT_RESULTS);
        assert_eq!(total_matches, 200);
        assert_eq!(matches.len(), SESSION_SEARCH_DEFAULT_RESULTS);
        assert_eq!(matches[0].id, 1);
        assert_eq!(
            matches.last().map(|item| item.id),
            Some(SESSION_SEARCH_DEFAULT_RESULTS as u64)
        );
    }

    #[test]
    fn integration_execute_session_search_command_scans_entries_across_branches() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append root");
        let main_head = store
            .append_messages(root, &[Message::user("main branch target")])
            .expect("append main");
        let _branch_head = store
            .append_messages(root, &[Message::user("branch target")])
            .expect("append branch");
        let runtime = SessionRuntime {
            store,
            active_head: main_head,
        };

        let output = execute_session_search_command(&runtime, "target");
        let main_index = output.find("result: id=2").expect("main result");
        let branch_index = output.find("result: id=3").expect("branch result");
        assert!(main_index < branch_index);
    }

    #[test]
    fn integration_execute_session_search_command_applies_role_filter_and_limit() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root target")])
            .expect("append root");
        let user_id = store
            .append_messages(root, &[Message::user("target user message")])
            .expect("append user");
        let _assistant_id = store
            .append_messages(
                user_id,
                &[Message::assistant_text("target assistant message")],
            )
            .expect("append assistant");
        let _tool_id = store
            .append_messages(
                user_id,
                &[Message::tool_result(
                    "tool-call-1",
                    "tool_call",
                    "{}",
                    false,
                )],
            )
            .expect("append tool");
        let runtime = SessionRuntime {
            store,
            active_head: user_id,
        };

        let output = execute_session_search_command(&runtime, "target --role user --limit 1");
        assert!(output.contains("role=user"));
        assert!(output.contains("matches=1"));
        assert!(output.contains("shown=1"));
        assert!(output.contains("limit=1"));
        assert!(output.contains("result: id=2 parent=1 role=user"));
        assert!(!output.contains("role=assistant"));
        assert!(!output.contains("role=tool"));
    }

    #[test]
    fn unit_parse_session_diff_args_supports_default_and_explicit_heads() {
        assert_eq!(parse_session_diff_args("").expect("default heads"), None);
        assert_eq!(
            parse_session_diff_args(" 12  24 ").expect("explicit heads"),
            Some((12, 24))
        );
    }

    #[test]
    fn regression_parse_session_diff_args_rejects_invalid_shapes() {
        let usage = parse_session_diff_args("12").expect_err("single head should fail");
        assert!(usage
            .to_string()
            .contains("usage: /session-diff [<left-id> <right-id>]"));

        let left_error = parse_session_diff_args("left 2").expect_err("invalid left head");
        assert!(left_error
            .to_string()
            .contains("invalid left session id 'left'"));
    }

    #[test]
    fn unit_shared_lineage_prefix_depth_returns_common_ancestor_depth() {
        let left = vec![
            crate::session::SessionEntry {
                id: 1,
                parent_id: None,
                message: Message::system("root"),
            },
            crate::session::SessionEntry {
                id: 2,
                parent_id: Some(1),
                message: Message::user("shared"),
            },
            crate::session::SessionEntry {
                id: 4,
                parent_id: Some(2),
                message: Message::assistant_text("left"),
            },
        ];
        let right = vec![
            crate::session::SessionEntry {
                id: 1,
                parent_id: None,
                message: Message::system("root"),
            },
            crate::session::SessionEntry {
                id: 2,
                parent_id: Some(1),
                message: Message::user("shared"),
            },
            crate::session::SessionEntry {
                id: 5,
                parent_id: Some(2),
                message: Message::assistant_text("right"),
            },
        ];

        assert_eq!(shared_lineage_prefix_depth(&left, &right), 2);
    }

    #[test]
    fn functional_render_session_diff_includes_summary_and_lineage_rows() {
        let report = SessionDiffReport {
            source: "explicit",
            left_id: 4,
            right_id: 5,
            shared_depth: 2,
            left_depth: 3,
            right_depth: 3,
            shared_entries: vec![SessionDiffEntry {
                id: 1,
                parent_id: None,
                role: "system".to_string(),
                preview: "root".to_string(),
            }],
            left_only_entries: vec![SessionDiffEntry {
                id: 4,
                parent_id: Some(2),
                role: "assistant".to_string(),
                preview: "left path".to_string(),
            }],
            right_only_entries: vec![SessionDiffEntry {
                id: 5,
                parent_id: Some(2),
                role: "assistant".to_string(),
                preview: "right path".to_string(),
            }],
        };

        let output = render_session_diff(&report);
        assert!(output.contains("session diff: source=explicit left=4 right=5"));
        assert!(output.contains(
            "summary: shared_depth=2 left_depth=3 right_depth=3 left_only=1 right_only=1"
        ));
        assert!(output.contains("shared: id=1 parent=none role=system preview=root"));
        assert!(output.contains("left-only: id=4 parent=2 role=assistant preview=left path"));
        assert!(output.contains("right-only: id=5 parent=2 role=assistant preview=right path"));
    }

    #[test]
    fn integration_execute_session_diff_command_defaults_to_active_and_latest_heads() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let main_head = store
            .append_messages(Some(root), &[Message::user("main user")])
            .expect("append main")
            .expect("main head");
        let latest_head = store
            .append_messages(Some(root), &[Message::user("branch user")])
            .expect("append branch")
            .expect("branch head");
        let runtime = SessionRuntime {
            store,
            active_head: Some(main_head),
        };

        let output = execute_session_diff_command(&runtime, None);
        assert!(output.contains(&format!(
            "session diff: source=default left={} right={}",
            main_head, latest_head
        )));
        assert!(output.contains(
            "summary: shared_depth=1 left_depth=2 right_depth=2 left_only=1 right_only=1"
        ));
        assert!(output.contains("shared: id=1 parent=none role=system preview=sys"));
        assert!(output.contains("left-only: id=2 parent=1 role=user preview=main user"));
        assert!(output.contains("right-only: id=3 parent=1 role=user preview=branch user"));
    }

    #[test]
    fn integration_execute_session_diff_command_supports_explicit_identical_heads() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[Message::user("user")])
            .expect("append user")
            .expect("head id");
        let runtime = SessionRuntime {
            store,
            active_head: Some(head),
        };

        let output = execute_session_diff_command(&runtime, Some((head, head)));
        assert!(output.contains("summary: shared_depth=2 left_depth=2 right_depth=2"));
        assert!(output.contains("left-only: none"));
        assert!(output.contains("right-only: none"));
    }

    #[test]
    fn regression_execute_session_diff_command_reports_unknown_ids() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append")
            .expect("root");
        let runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };

        let output = execute_session_diff_command(&runtime, Some((999, root)));
        assert!(output.contains("session diff error: unknown left session id 999"));
    }

    #[test]
    fn regression_execute_session_diff_command_reports_empty_session_default_heads() {
        let temp = tempdir().expect("tempdir");
        let store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let runtime = SessionRuntime {
            store,
            active_head: None,
        };

        let output = execute_session_diff_command(&runtime, None);
        assert!(output.contains("session diff error: active head is not set"));
    }

    #[test]
    fn regression_execute_session_diff_command_reports_malformed_graph() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("malformed-session.jsonl");
        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({
                "record_type":"entry",
                "id":1,
                "parent_id":2,
                "message": Message::system("orphan")
            })
            .to_string(),
        ]
        .join("\n");
        std::fs::write(&session_path, format!("{raw}\n")).expect("write session");
        let store = SessionStore::load(&session_path).expect("load session");
        let runtime = SessionRuntime {
            store,
            active_head: Some(1),
        };

        let output = execute_session_diff_command(&runtime, None);
        assert!(output.contains("session diff error: unknown session id 2"));
    }

    #[test]
    fn unit_compute_session_entry_depths_calculates_branch_depths() {
        let entries = vec![
            crate::session::SessionEntry {
                id: 3,
                parent_id: Some(2),
                message: Message::assistant_text("leaf"),
            },
            crate::session::SessionEntry {
                id: 1,
                parent_id: None,
                message: Message::system("root"),
            },
            crate::session::SessionEntry {
                id: 2,
                parent_id: Some(1),
                message: Message::user("middle"),
            },
        ];
        let depths = compute_session_entry_depths(&entries).expect("depth computation");
        assert_eq!(depths.get(&1), Some(&1));
        assert_eq!(depths.get(&2), Some(&2));
        assert_eq!(depths.get(&3), Some(&3));
    }

    #[test]
    fn unit_compute_session_stats_calculates_core_counts() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let active_head = store
            .append_messages(Some(root), &[Message::user("user one")])
            .expect("append user")
            .expect("active head");
        let runtime = SessionRuntime {
            store,
            active_head: Some(active_head),
        };

        let stats = compute_session_stats(&runtime).expect("compute stats");
        assert_eq!(stats.entries, 2);
        assert_eq!(stats.branch_tips, 1);
        assert_eq!(stats.roots, 1);
        assert_eq!(stats.max_depth, 2);
        assert_eq!(stats.active_depth, Some(2));
        assert_eq!(stats.latest_depth, Some(2));
        assert!(stats.active_is_latest);
        assert_eq!(stats.role_counts.get("system"), Some(&1));
        assert_eq!(stats.role_counts.get("user"), Some(&1));
    }

    #[test]
    fn functional_render_session_stats_includes_heads_depths_and_roles() {
        let mut role_counts = BTreeMap::new();
        role_counts.insert("assistant".to_string(), 2);
        role_counts.insert("user".to_string(), 1);
        let stats = SessionStats {
            entries: 3,
            branch_tips: 1,
            roots: 1,
            max_depth: 3,
            active_depth: Some(3),
            latest_depth: Some(3),
            active_head: Some(3),
            latest_head: Some(3),
            active_is_latest: true,
            role_counts,
        };

        let rendered = render_session_stats(&stats);
        assert!(rendered.contains("session stats: entries=3 branch_tips=1 roots=1 max_depth=3"));
        assert!(rendered.contains("heads: active=3 latest=3 active_is_latest=true"));
        assert!(rendered.contains("depth: active=3 latest=3"));
        assert!(rendered.contains("role: assistant=2"));
        assert!(rendered.contains("role: user=1"));
    }

    #[test]
    fn unit_parse_session_stats_args_supports_default_and_json_modes() {
        assert_eq!(
            parse_session_stats_args("").expect("empty args"),
            SessionStatsOutputFormat::Text
        );
        assert_eq!(
            parse_session_stats_args("--json").expect("json flag"),
            SessionStatsOutputFormat::Json
        );
        let error = parse_session_stats_args("--bad").expect_err("invalid flag should fail");
        assert!(error.to_string().contains("usage: /session-stats [--json]"));
    }

    #[test]
    fn unit_render_session_stats_json_includes_counts_and_roles() {
        let mut role_counts = BTreeMap::new();
        role_counts.insert("assistant".to_string(), 2);
        role_counts.insert("user".to_string(), 1);
        let stats = SessionStats {
            entries: 3,
            branch_tips: 1,
            roots: 1,
            max_depth: 3,
            active_depth: Some(3),
            latest_depth: Some(3),
            active_head: Some(3),
            latest_head: Some(3),
            active_is_latest: true,
            role_counts,
        };

        let json = render_session_stats_json(&stats);
        let value = serde_json::from_str::<serde_json::Value>(&json).expect("parse json");
        assert_eq!(value["entries"], 3);
        assert_eq!(value["branch_tips"], 1);
        assert_eq!(value["active_head"], 3);
        assert_eq!(value["role_counts"]["assistant"], 2);
        assert_eq!(value["role_counts"]["user"], 1);
    }

    #[test]
    fn integration_execute_session_stats_command_summarizes_branched_session() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let main_head = store
            .append_messages(Some(root), &[Message::user("main user")])
            .expect("append main")
            .expect("main head");
        let branch_head = store
            .append_messages(Some(root), &[Message::user("branch user")])
            .expect("append branch")
            .expect("branch head");
        let latest_head = store
            .append_messages(
                Some(branch_head),
                &[Message::assistant_text("branch assistant")],
            )
            .expect("append branch assistant")
            .expect("latest head");
        let runtime = SessionRuntime {
            store,
            active_head: Some(main_head),
        };

        let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
        assert!(output.contains("session stats: entries=4"));
        assert!(output.contains("branch_tips=2"));
        assert!(output.contains("roots=1"));
        assert!(output.contains("max_depth=3"));
        assert!(output.contains(&format!(
            "heads: active={} latest={} active_is_latest=false",
            main_head, latest_head
        )));
        assert!(output.contains("role: assistant=1"));
        assert!(output.contains("role: system=1"));
        assert!(output.contains("role: user=2"));

        let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
        let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json");
        assert_eq!(value["entries"], 4);
        assert_eq!(value["branch_tips"], 2);
        assert_eq!(value["roots"], 1);
        assert_eq!(value["max_depth"], 3);
        assert_eq!(value["active_head"], main_head);
        assert_eq!(value["latest_head"], latest_head);
        assert_eq!(value["role_counts"]["assistant"], 1);
        assert_eq!(value["role_counts"]["system"], 1);
        assert_eq!(value["role_counts"]["user"], 2);
    }

    #[test]
    fn regression_execute_session_stats_command_handles_empty_session() {
        let temp = tempdir().expect("tempdir");
        let store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let runtime = SessionRuntime {
            store,
            active_head: None,
        };

        let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
        assert!(output.contains("session stats: entries=0 branch_tips=0 roots=0 max_depth=0"));
        assert!(output.contains("heads: active=none latest=none active_is_latest=true"));
        assert!(output.contains("depth: active=none latest=none"));
        assert!(output.contains("roles: none"));

        let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
        let value = serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json");
        assert_eq!(value["entries"], 0);
        assert_eq!(value["branch_tips"], 0);
        assert_eq!(value["roots"], 0);
        assert_eq!(value["max_depth"], 0);
        assert_eq!(value["active_head"], serde_json::Value::Null);
    }

    #[test]
    fn regression_execute_session_stats_command_reports_malformed_graph() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("malformed-session.jsonl");
        let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({
                "record_type":"entry",
                "id":1,
                "parent_id":2,
                "message": Message::system("orphan")
            })
            .to_string(),
        ]
        .join("\n");
        std::fs::write(&session_path, format!("{raw}\n")).expect("write session");
        let store = SessionStore::load(&session_path).expect("load session");
        let runtime = SessionRuntime {
            store,
            active_head: Some(1),
        };

        let output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Text);
        assert!(output.contains("session stats error:"));
        assert!(output.contains("missing parent id 2"));

        let json_output = execute_session_stats_command(&runtime, SessionStatsOutputFormat::Json);
        let value =
            serde_json::from_str::<serde_json::Value>(&json_output).expect("parse json error");
        assert!(value["error"]
            .as_str()
            .expect("error string")
            .contains("missing parent id 2"));
    }

    #[test]
    fn unit_build_doctor_command_config_collects_sorted_unique_provider_states() {
        let mut cli = test_cli();
        cli.no_session = true;
        cli.session = PathBuf::from("/tmp/session.jsonl");
        cli.skills_dir = PathBuf::from("/tmp/skills");
        cli.skills_lock_file = Some(PathBuf::from("/tmp/custom.lock.json"));
        cli.skill_trust_root_file = Some(PathBuf::from("/tmp/trust-roots.json"));
        cli.openai_api_key = Some("openai-key".to_string());
        cli.anthropic_api_key = Some("anthropic-key".to_string());
        cli.google_api_key = None;

        let primary = ModelRef {
            provider: Provider::OpenAi,
            model: "gpt-4o-mini".to_string(),
        };
        let fallbacks = vec![
            ModelRef {
                provider: Provider::Google,
                model: "gemini-2.5-pro".to_string(),
            },
            ModelRef {
                provider: Provider::Anthropic,
                model: "claude-sonnet-4".to_string(),
            },
            ModelRef {
                provider: Provider::OpenAi,
                model: "gpt-4.1-mini".to_string(),
            },
        ];
        let lock_path = PathBuf::from("/tmp/skills.lock.json");

        let config = build_doctor_command_config(&cli, &primary, &fallbacks, &lock_path);
        assert_eq!(config.model, "openai/gpt-4o-mini");
        assert!(!config.session_enabled);
        assert_eq!(config.session_path, PathBuf::from("/tmp/session.jsonl"));
        assert_eq!(config.skills_dir, PathBuf::from("/tmp/skills"));
        assert_eq!(config.skills_lock_path, lock_path);
        assert_eq!(
            config.trust_root_path,
            Some(PathBuf::from("/tmp/trust-roots.json"))
        );

        let provider_rows = config
            .provider_keys
            .iter()
            .map(|item| {
                (
                    item.provider.clone(),
                    item.key_env_var.clone(),
                    item.present,
                    item.auth_mode.as_str().to_string(),
                    item.mode_supported,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            provider_rows,
            vec![
                (
                    "anthropic".to_string(),
                    "ANTHROPIC_API_KEY".to_string(),
                    true,
                    "api_key".to_string(),
                    true
                ),
                (
                    "google".to_string(),
                    "GEMINI_API_KEY".to_string(),
                    false,
                    "api_key".to_string(),
                    true
                ),
                (
                    "openai".to_string(),
                    "OPENAI_API_KEY".to_string(),
                    true,
                    "api_key".to_string(),
                    true
                ),
            ]
        );
    }

    #[test]
    fn unit_render_doctor_report_summarizes_counts_and_rows() {
        let report = render_doctor_report(&[
            DoctorCheckResult {
                key: "model".to_string(),
                status: DoctorStatus::Pass,
                code: "openai/gpt-4o-mini".to_string(),
                path: None,
                action: None,
            },
            DoctorCheckResult {
                key: "provider_key.openai".to_string(),
                status: DoctorStatus::Fail,
                code: "missing".to_string(),
                path: None,
                action: Some("set OPENAI_API_KEY".to_string()),
            },
            DoctorCheckResult {
                key: "skills_lock".to_string(),
                status: DoctorStatus::Warn,
                code: "missing".to_string(),
                path: Some("/tmp/skills.lock.json".to_string()),
                action: Some("run /skills-lock-write to generate lockfile".to_string()),
            },
        ]);

        assert!(report.contains("doctor summary: checks=3 pass=1 warn=1 fail=1"));
        assert!(report.contains(
            "doctor check: key=model status=pass code=openai/gpt-4o-mini path=none action=none"
        ));
        assert!(report.contains(
            "doctor check: key=provider_key.openai status=fail code=missing path=none action=set OPENAI_API_KEY"
        ));
        assert!(report.contains("doctor check: key=skills_lock status=warn code=missing path=/tmp/skills.lock.json action=run /skills-lock-write to generate lockfile"));
    }

    #[test]
    fn unit_parse_doctor_command_args_supports_default_and_json_modes() {
        assert_eq!(
            parse_doctor_command_args("").expect("parse empty"),
            DoctorCommandOutputFormat::Text
        );
        assert_eq!(
            parse_doctor_command_args("--json").expect("parse json"),
            DoctorCommandOutputFormat::Json
        );

        let error =
            parse_doctor_command_args("--json --extra").expect_err("extra args should fail");
        assert!(error.to_string().contains("usage: /doctor [--json]"));
    }

    #[test]
    fn unit_render_doctor_report_json_contains_summary_and_rows() {
        let report = render_doctor_report_json(&[
            DoctorCheckResult {
                key: "model".to_string(),
                status: DoctorStatus::Pass,
                code: "openai/gpt-4o-mini".to_string(),
                path: None,
                action: None,
            },
            DoctorCheckResult {
                key: "provider_key.openai".to_string(),
                status: DoctorStatus::Fail,
                code: "missing".to_string(),
                path: None,
                action: Some("set OPENAI_API_KEY".to_string()),
            },
        ]);
        let value = serde_json::from_str::<serde_json::Value>(&report).expect("parse json");
        assert_eq!(value["summary"]["checks"], 2);
        assert_eq!(value["summary"]["pass"], 1);
        assert_eq!(value["summary"]["warn"], 0);
        assert_eq!(value["summary"]["fail"], 1);
        assert_eq!(value["checks"][0]["key"], "model");
        assert_eq!(value["checks"][0]["status"], "pass");
        assert_eq!(value["checks"][1]["key"], "provider_key.openai");
        assert_eq!(value["checks"][1]["status"], "fail");
    }

    #[test]
    fn functional_execute_doctor_command_supports_text_and_json_modes() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let skills_dir = temp.path().join("skills");
        let lock_path = temp.path().join("skills.lock.json");
        let trust_root_path = temp.path().join("trust-roots.json");
        std::fs::create_dir_all(&skills_dir).expect("mkdir skills");
        std::fs::write(&session_path, "{}\n").expect("write session");
        std::fs::write(&lock_path, "{}\n").expect("write lock");
        std::fs::write(&trust_root_path, "[]\n").expect("write trust");

        let config = DoctorCommandConfig {
            model: "openai/gpt-4o-mini".to_string(),
            provider_keys: vec![
                DoctorProviderKeyStatus {
                    provider_kind: Provider::Anthropic,
                    provider: "anthropic".to_string(),
                    key_env_var: "ANTHROPIC_API_KEY".to_string(),
                    present: false,
                    auth_mode: ProviderAuthMethod::ApiKey,
                    mode_supported: true,
                },
                DoctorProviderKeyStatus {
                    provider_kind: Provider::OpenAi,
                    provider: "openai".to_string(),
                    key_env_var: "OPENAI_API_KEY".to_string(),
                    present: true,
                    auth_mode: ProviderAuthMethod::ApiKey,
                    mode_supported: true,
                },
            ],
            session_enabled: true,
            session_path,
            skills_dir,
            skills_lock_path: lock_path,
            trust_root_path: Some(trust_root_path),
        };

        let report = execute_doctor_command(&config, DoctorCommandOutputFormat::Text);
        assert!(report.contains("doctor summary: checks=9 pass=8 warn=0 fail=1"));

        let keys = report
            .lines()
            .skip(1)
            .map(|line| {
                line.split("key=")
                    .nth(1)
                    .expect("key section")
                    .split(" status=")
                    .next()
                    .expect("key value")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            vec![
                "model".to_string(),
                "provider_auth_mode.anthropic".to_string(),
                "provider_key.anthropic".to_string(),
                "provider_auth_mode.openai".to_string(),
                "provider_key.openai".to_string(),
                "session_path".to_string(),
                "skills_dir".to_string(),
                "skills_lock".to_string(),
                "trust_root".to_string(),
            ]
        );

        let json_report = execute_doctor_command(&config, DoctorCommandOutputFormat::Json);
        let value =
            serde_json::from_str::<serde_json::Value>(&json_report).expect("parse json report");
        assert_eq!(value["summary"]["checks"], 9);
        assert_eq!(value["summary"]["pass"], 8);
        assert_eq!(value["summary"]["warn"], 0);
        assert_eq!(value["summary"]["fail"], 1);
        assert_eq!(value["checks"][0]["key"], "model");
        assert_eq!(value["checks"][1]["key"], "provider_auth_mode.anthropic");
    }

    #[test]
    fn integration_run_doctor_checks_identifies_missing_runtime_prerequisites() {
        let temp = tempdir().expect("tempdir");
        let config = DoctorCommandConfig {
            model: "openai/gpt-4o-mini".to_string(),
            provider_keys: vec![DoctorProviderKeyStatus {
                provider_kind: Provider::OpenAi,
                provider: "openai".to_string(),
                key_env_var: "OPENAI_API_KEY".to_string(),
                present: false,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
            }],
            session_enabled: true,
            session_path: temp.path().join("missing-parent").join("session.jsonl"),
            skills_dir: temp.path().join("missing-skills"),
            skills_lock_path: temp.path().join("missing-lock.json"),
            trust_root_path: Some(temp.path().join("missing-trust-roots.json")),
        };

        let checks = run_doctor_checks(&config);
        let by_key = checks
            .into_iter()
            .map(|check| (check.key.clone(), check))
            .collect::<HashMap<_, _>>();

        assert_eq!(
            by_key.get("model").map(|item| item.status),
            Some(DoctorStatus::Pass)
        );
        assert_eq!(
            by_key
                .get("provider_auth_mode.openai")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Pass, "api_key".to_string()))
        );
        assert_eq!(
            by_key
                .get("provider_key.openai")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Fail, "missing".to_string()))
        );
        assert_eq!(
            by_key
                .get("session_path")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Fail, "missing_parent".to_string()))
        );
        assert_eq!(
            by_key
                .get("skills_dir")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Warn, "missing".to_string()))
        );
        assert_eq!(
            by_key
                .get("skills_lock")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Warn, "missing".to_string()))
        );
        assert_eq!(
            by_key
                .get("trust_root")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Warn, "missing".to_string()))
        );
    }

    #[test]
    fn integration_doctor_command_preserves_session_runtime() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write skill");
        let lock_path = default_skills_lock_path(&skills_dir);
        std::fs::write(&lock_path, "{}\n").expect("write lock");
        let trust_root_path = temp.path().join("trust-roots.json");
        std::fs::write(&trust_root_path, "[]\n").expect("write trust");

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let mut skills_command_config =
            skills_command_config(&skills_dir, &lock_path, Some(&trust_root_path));
        skills_command_config.doctor_config.session_path = temp.path().join("session.jsonl");

        let action = handle_command_with_session_import_mode(
            "/doctor",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &auth_command_config,
        )
        .expect("doctor command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn regression_run_doctor_checks_reports_type_and_readability_errors() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session-as-dir");
        std::fs::create_dir_all(&session_path).expect("mkdir session dir");
        let skills_dir = temp.path().join("skills-as-file");
        std::fs::write(&skills_dir, "not a directory").expect("write skills file");
        let lock_path = temp.path().join("lock-as-dir");
        std::fs::create_dir_all(&lock_path).expect("mkdir lock dir");
        let trust_root_path = temp.path().join("trust-as-dir");
        std::fs::create_dir_all(&trust_root_path).expect("mkdir trust dir");

        let config = DoctorCommandConfig {
            model: "openai/gpt-4o-mini".to_string(),
            provider_keys: vec![DoctorProviderKeyStatus {
                provider_kind: Provider::OpenAi,
                provider: "openai".to_string(),
                key_env_var: "OPENAI_API_KEY".to_string(),
                present: true,
                auth_mode: ProviderAuthMethod::ApiKey,
                mode_supported: true,
            }],
            session_enabled: true,
            session_path,
            skills_dir,
            skills_lock_path: lock_path,
            trust_root_path: Some(trust_root_path),
        };

        let checks = run_doctor_checks(&config);
        let by_key = checks
            .into_iter()
            .map(|check| (check.key.clone(), check))
            .collect::<HashMap<_, _>>();

        assert_eq!(
            by_key
                .get("provider_auth_mode.openai")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Pass, "api_key".to_string()))
        );

        assert_eq!(
            by_key
                .get("session_path")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Fail, "not_file".to_string()))
        );
        assert_eq!(
            by_key
                .get("skills_dir")
                .map(|item| (item.status, item.code.clone())),
            Some((DoctorStatus::Fail, "not_dir".to_string()))
        );
        let lock = by_key.get("skills_lock").expect("skills lock check");
        assert_eq!(lock.status, DoctorStatus::Fail);
        assert!(lock.code.starts_with("read_error:"));
        let trust = by_key.get("trust_root").expect("trust root check");
        assert_eq!(trust.status, DoctorStatus::Fail);
        assert!(trust.code.starts_with("read_error:"));
    }

    #[test]
    fn unit_resolve_session_graph_format_and_escape_label_behaviors() {
        assert_eq!(
            resolve_session_graph_format(Path::new("/tmp/graph.dot")),
            SessionGraphFormat::Dot
        );
        assert_eq!(
            resolve_session_graph_format(Path::new("/tmp/graph.mmd")),
            SessionGraphFormat::Mermaid
        );
        assert_eq!(escape_graph_label("a\"b\\c"), "a\\\"b\\\\c".to_string());
    }

    #[test]
    fn unit_render_session_graph_mermaid_and_dot_include_deterministic_edges() {
        let entries = vec![
            crate::session::SessionEntry {
                id: 2,
                parent_id: Some(1),
                message: Message::user("child"),
            },
            crate::session::SessionEntry {
                id: 1,
                parent_id: None,
                message: Message::system("root"),
            },
        ];

        let mermaid = render_session_graph_mermaid(&entries);
        assert!(mermaid.contains("graph TD"));
        let root_index = mermaid.find("n1[\"1: system | root\"]").expect("root node");
        let child_index = mermaid.find("n2[\"2: user | child\"]").expect("child node");
        assert!(root_index < child_index);
        assert!(mermaid.contains("n1 --> n2"));

        let dot = render_session_graph_dot(&entries);
        assert!(dot.contains("digraph session"));
        assert!(dot.contains("n1 [label=\"1: system | root\"];"));
        assert!(dot.contains("n2 [label=\"2: user | child\"];"));
        assert!(dot.contains("n1 -> n2;"));
    }

    #[test]
    fn functional_execute_session_graph_export_command_writes_mermaid_file() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let _head = store
            .append_messages(Some(root), &[Message::user("child")])
            .expect("append child")
            .expect("head id");
        let runtime = SessionRuntime {
            store,
            active_head: Some(root + 1),
        };
        let destination = temp.path().join("session-graph.mmd");

        let output = execute_session_graph_export_command(
            &runtime,
            destination.to_str().expect("utf8 path"),
        );
        assert!(output.contains("session graph export: path="));
        assert!(output.contains("format=mermaid"));
        assert!(output.contains("nodes=2"));
        assert!(output.contains("edges=1"));

        let raw = std::fs::read_to_string(destination).expect("read graph");
        assert!(raw.contains("graph TD"));
        assert!(raw.contains("n1 --> n2"));
    }

    #[test]
    fn integration_execute_session_graph_export_command_supports_dot_for_branched_session() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let _main = store
            .append_messages(Some(root), &[Message::user("main")])
            .expect("append main")
            .expect("main id");
        let _branch = store
            .append_messages(Some(root), &[Message::user("branch")])
            .expect("append branch")
            .expect("branch id");
        let runtime = SessionRuntime {
            store,
            active_head: Some(root + 2),
        };
        let destination = temp.path().join("session-graph.dot");

        let output = execute_session_graph_export_command(
            &runtime,
            destination.to_str().expect("utf8 path"),
        );
        assert!(output.contains("format=dot"));
        assert!(output.contains("nodes=3"));
        assert!(output.contains("edges=2"));

        let raw = std::fs::read_to_string(destination).expect("read graph");
        assert!(raw.contains("digraph session"));
        assert!(raw.contains("n1 -> n2;"));
        assert!(raw.contains("n1 -> n3;"));
    }

    #[test]
    fn regression_execute_session_graph_export_command_rejects_directory_destination() {
        let temp = tempdir().expect("tempdir");
        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };
        let destination_dir = temp.path().join("graph-dir");
        std::fs::create_dir_all(&destination_dir).expect("mkdir");

        let output = execute_session_graph_export_command(
            &runtime,
            destination_dir.to_str().expect("utf8 path"),
        );
        assert!(output.contains("session graph export error: path="));
        assert!(output.contains("is a directory"));
    }

    #[test]
    fn unit_default_macro_config_path_uses_project_local_file_location() {
        let path = default_macro_config_path().expect("resolve macro path");
        assert!(path.ends_with(Path::new(".pi").join("macros.json")));
    }

    #[test]
    fn unit_validate_macro_name_accepts_and_rejects_expected_inputs() {
        validate_macro_name("quick_check").expect("valid macro name");

        let error = validate_macro_name("").expect_err("empty macro name should fail");
        assert!(error.to_string().contains("must not be empty"));

        let error =
            validate_macro_name("9check").expect_err("macro name starting with digit should fail");
        assert!(error
            .to_string()
            .contains("must start with an ASCII letter"));

        let error = validate_macro_name("check.list")
            .expect_err("macro name with unsupported punctuation should fail");
        assert!(error
            .to_string()
            .contains("must contain only ASCII letters, digits, '-' or '_'"));
    }

    #[test]
    fn functional_parse_macro_command_supports_lifecycle_and_usage_rules() {
        assert_eq!(
            parse_macro_command("list").expect("parse list"),
            MacroCommand::List
        );
        assert_eq!(
            parse_macro_command("save quick /tmp/quick.commands").expect("parse save"),
            MacroCommand::Save {
                name: "quick".to_string(),
                commands_file: PathBuf::from("/tmp/quick.commands"),
            }
        );
        assert_eq!(
            parse_macro_command("run quick").expect("parse run"),
            MacroCommand::Run {
                name: "quick".to_string(),
                dry_run: false,
            }
        );
        assert_eq!(
            parse_macro_command("run quick --dry-run").expect("parse dry run"),
            MacroCommand::Run {
                name: "quick".to_string(),
                dry_run: true,
            }
        );
        assert_eq!(
            parse_macro_command("show quick").expect("parse show"),
            MacroCommand::Show {
                name: "quick".to_string(),
            }
        );
        assert_eq!(
            parse_macro_command("delete quick").expect("parse delete"),
            MacroCommand::Delete {
                name: "quick".to_string(),
            }
        );

        let error = parse_macro_command("").expect_err("missing args should fail");
        assert!(error.to_string().contains(MACRO_USAGE));

        let error =
            parse_macro_command("run quick --apply").expect_err("unknown run flag should fail");
        assert!(error
            .to_string()
            .contains("usage: /macro run <name> [--dry-run]"));

        let error =
            parse_macro_command("list extra").expect_err("list with extra arguments should fail");
        assert!(error.to_string().contains("usage: /macro list"));

        let error = parse_macro_command("show").expect_err("show without name should fail");
        assert!(error.to_string().contains("usage: /macro show <name>"));
    }

    #[test]
    fn unit_validate_macro_command_entry_rejects_nested_unknown_and_exit_commands() {
        validate_macro_command_entry("/session").expect("known command should validate");

        let error =
            validate_macro_command_entry("session").expect_err("command without slash should fail");
        assert!(error.to_string().contains("must start with '/'"));

        let error = validate_macro_command_entry("/does-not-exist")
            .expect_err("unknown command should fail");
        assert!(error
            .to_string()
            .contains("unknown command '/does-not-exist'"));

        let error = validate_macro_command_entry("/macro list")
            .expect_err("nested macro command should fail");
        assert!(error
            .to_string()
            .contains("nested /macro commands are not allowed"));

        let error = validate_macro_command_entry("/quit").expect_err("exit commands should fail");
        assert!(error.to_string().contains("exit commands are not allowed"));
    }

    #[test]
    fn unit_save_and_load_macro_file_round_trip_schema_and_values() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".pi").join("macros.json");
        let macros = BTreeMap::from([
            (
                "quick".to_string(),
                vec!["/session".to_string(), "/session-stats".to_string()],
            ),
            ("review".to_string(), vec!["/help session".to_string()]),
        ]);

        save_macro_file(&macro_path, &macros).expect("save macro file");

        let loaded = load_macro_file(&macro_path).expect("load macro file");
        assert_eq!(loaded, macros);

        let raw = std::fs::read_to_string(&macro_path).expect("read macro file");
        let parsed = serde_json::from_str::<MacroFile>(&raw).expect("parse macro file");
        assert_eq!(parsed.schema_version, MACRO_SCHEMA_VERSION);
        assert_eq!(parsed.macros, macros);
    }

    #[test]
    fn functional_render_macro_helpers_support_empty_and_deterministic_order() {
        let path = Path::new("/tmp/macros.json");
        let empty = render_macro_list(path, &BTreeMap::new());
        assert!(empty.contains("count=0"));
        assert!(empty.contains("macros: none"));

        let macros = BTreeMap::from([
            ("beta".to_string(), vec!["/session".to_string()]),
            (
                "alpha".to_string(),
                vec!["/session".to_string(), "/session-stats".to_string()],
            ),
        ]);
        let output = render_macro_list(path, &macros);
        let alpha_index = output.find("macro: name=alpha").expect("alpha row");
        let beta_index = output.find("macro: name=beta").expect("beta row");
        assert!(alpha_index < beta_index);

        let show = render_macro_show(path, "alpha", macros.get("alpha").expect("alpha commands"));
        assert!(show.contains("macro show: path=/tmp/macros.json name=alpha commands=2"));
        assert!(show.contains("command: index=0 value=/session"));
        assert!(show.contains("command: index=1 value=/session-stats"));
    }

    #[test]
    fn integration_execute_macro_command_save_show_run_delete_lifecycle() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".pi").join("macros.json");
        let commands_file = temp.path().join("rewind.commands");
        std::fs::write(&commands_file, "/branch 1\n/session\n").expect("write commands file");

        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[Message::assistant_text("leaf")])
            .expect("append leaf")
            .expect("head id");
        let mut session_runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = session_runtime
            .as_ref()
            .expect("runtime")
            .store
            .lineage_messages(Some(head))
            .expect("lineage");
        agent.replace_messages(lineage);

        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
        };

        let save_output = execute_macro_command(
            &format!("save rewind {}", commands_file.display()),
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(save_output.contains("macro save: path="));
        assert!(save_output.contains("name=rewind"));
        assert!(save_output.contains("commands=2"));

        let dry_run_output = execute_macro_command(
            "run rewind --dry-run",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(dry_run_output.contains("mode=dry-run"));
        assert!(dry_run_output.contains("plan: command=/branch 1"));
        assert_eq!(
            session_runtime
                .as_ref()
                .and_then(|runtime| runtime.active_head),
            Some(head)
        );

        let show_output = execute_macro_command(
            "show rewind",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(show_output.contains("macro show: path="));
        assert!(show_output.contains("name=rewind commands=2"));
        assert!(show_output.contains("command: index=0 value=/branch 1"));
        assert!(show_output.contains("command: index=1 value=/session"));

        let run_output = execute_macro_command(
            "run rewind",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(run_output.contains("macro run: path="));
        assert!(run_output.contains("mode=apply"));
        assert!(run_output.contains("executed=2"));
        assert_eq!(
            session_runtime
                .as_ref()
                .and_then(|runtime| runtime.active_head),
            Some(root)
        );

        let list_output = execute_macro_command(
            "list",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(list_output.contains("macro list: path="));
        assert!(list_output.contains("count=1"));
        assert!(list_output.contains("macro: name=rewind commands=2"));

        let delete_output = execute_macro_command(
            "delete rewind",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(delete_output.contains("macro delete: path="));
        assert!(delete_output.contains("name=rewind"));
        assert!(delete_output.contains("status=deleted"));
        assert!(delete_output.contains("remaining=0"));

        let final_list = execute_macro_command(
            "list",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(final_list.contains("count=0"));
        assert!(final_list.contains("macros: none"));
    }

    #[test]
    fn regression_execute_macro_command_reports_missing_commands_file() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".pi").join("macros.json");
        let missing_commands_file = temp.path().join("missing.commands");
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
        };
        let mut session_runtime = None;
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

        let output = execute_macro_command(
            &format!("save quick {}", missing_commands_file.display()),
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(output.contains("macro error: path="));
        assert!(output.contains("failed to read commands file"));
    }

    #[test]
    fn regression_execute_macro_command_reports_corrupt_macro_file() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".pi").join("macros.json");
        std::fs::create_dir_all(
            macro_path
                .parent()
                .expect("macro path should include a parent"),
        )
        .expect("create macro config dir");
        std::fs::write(&macro_path, "{invalid-json").expect("write malformed macro file");

        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
        };
        let mut session_runtime = None;
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

        let output = execute_macro_command(
            "list",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(output.contains("macro error: path="));
        assert!(output.contains("failed to parse macro file"));
    }

    #[test]
    fn regression_execute_macro_command_rejects_unknown_macro_and_invalid_entries() {
        let temp = tempdir().expect("tempdir");
        let macro_path = temp.path().join(".pi").join("macros.json");
        let macros = BTreeMap::from([("broken".to_string(), vec!["/macro list".to_string()])]);
        save_macro_file(&macro_path, &macros).expect("save macro file");

        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
        };
        let mut session_runtime = None;
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());

        let missing_output = execute_macro_command(
            "run missing",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(missing_output.contains("unknown macro 'missing'"));

        let missing_show = execute_macro_command(
            "show missing",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(missing_show.contains("unknown macro 'missing'"));

        let missing_delete = execute_macro_command(
            "delete missing",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(missing_delete.contains("unknown macro 'missing'"));

        let invalid_output = execute_macro_command(
            "run broken",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(invalid_output.contains("macro command #0 failed validation"));

        let delete_broken = execute_macro_command(
            "delete broken",
            &macro_path,
            &mut agent,
            &mut session_runtime,
            command_context,
        );
        assert!(delete_broken.contains("status=deleted"));
        assert!(delete_broken.contains("remaining=0"));
    }

    #[test]
    fn unit_validate_profile_name_accepts_and_rejects_expected_inputs() {
        validate_profile_name("baseline_1").expect("valid profile name");

        let error = validate_profile_name("").expect_err("empty profile name should fail");
        assert!(error.to_string().contains("must not be empty"));

        let error = validate_profile_name("1baseline")
            .expect_err("profile name starting with digit should fail");
        assert!(error
            .to_string()
            .contains("must start with an ASCII letter"));

        let error = validate_profile_name("baseline.json")
            .expect_err("profile name with punctuation should fail");
        assert!(error
            .to_string()
            .contains("must contain only ASCII letters, digits, '-' or '_'"));
    }

    #[test]
    fn functional_parse_profile_command_supports_lifecycle_subcommands_and_usage_errors() {
        assert_eq!(
            parse_profile_command("save baseline").expect("parse save"),
            ProfileCommand::Save {
                name: "baseline".to_string(),
            }
        );
        assert_eq!(
            parse_profile_command("load baseline").expect("parse load"),
            ProfileCommand::Load {
                name: "baseline".to_string(),
            }
        );
        assert_eq!(
            parse_profile_command("list").expect("parse list"),
            ProfileCommand::List
        );
        assert_eq!(
            parse_profile_command("show baseline").expect("parse show"),
            ProfileCommand::Show {
                name: "baseline".to_string(),
            }
        );
        assert_eq!(
            parse_profile_command("delete baseline").expect("parse delete"),
            ProfileCommand::Delete {
                name: "baseline".to_string(),
            }
        );

        let error = parse_profile_command("").expect_err("empty args should fail");
        assert!(error.to_string().contains(PROFILE_USAGE));

        let error = parse_profile_command("save").expect_err("missing name should fail");
        assert!(error.to_string().contains("usage: /profile save <name>"));

        let error = parse_profile_command("list extra")
            .expect_err("list with trailing arguments should fail");
        assert!(error.to_string().contains("usage: /profile list"));

        let error = parse_profile_command("show").expect_err("show missing name should fail");
        assert!(error.to_string().contains("usage: /profile show <name>"));

        let error =
            parse_profile_command("unknown baseline").expect_err("unknown subcommand should fail");
        assert!(error.to_string().contains("unknown subcommand 'unknown'"));
    }

    #[test]
    fn unit_save_and_load_profile_store_round_trip_schema_and_values() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".pi").join("profiles.json");
        let mut alternate = test_profile_defaults();
        alternate.model = "google/gemini-2.5-pro".to_string();
        let profiles = BTreeMap::from([
            ("baseline".to_string(), test_profile_defaults()),
            ("alt".to_string(), alternate.clone()),
        ]);

        save_profile_store(&profile_path, &profiles).expect("save profiles");
        let loaded = load_profile_store(&profile_path).expect("load profiles");
        assert_eq!(loaded, profiles);

        let raw = std::fs::read_to_string(&profile_path).expect("read profile file");
        let parsed = serde_json::from_str::<ProfileStoreFile>(&raw).expect("parse profile file");
        assert_eq!(parsed.schema_version, PROFILE_SCHEMA_VERSION);
        assert_eq!(parsed.profiles, profiles);
    }

    #[test]
    fn regression_load_profile_store_backfills_auth_defaults_for_legacy_profiles() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".pi").join("profiles.json");
        std::fs::create_dir_all(
            profile_path
                .parent()
                .expect("profile path should have parent"),
        )
        .expect("mkdir profile dir");
        std::fs::write(
            &profile_path,
            serde_json::json!({
                "schema_version": PROFILE_SCHEMA_VERSION,
                "profiles": {
                    "legacy": {
                        "model": "openai/gpt-4o-mini",
                        "fallback_models": [],
                        "session": {
                            "enabled": true,
                            "path": ".pi/sessions/default.jsonl",
                            "import_mode": "merge"
                        },
                        "policy": {
                            "tool_policy_preset": "balanced",
                            "bash_profile": "balanced",
                            "bash_dry_run": false,
                            "os_sandbox_mode": "off",
                            "enforce_regular_files": true,
                            "bash_timeout_ms": 500,
                            "max_command_length": 4096,
                            "max_tool_output_bytes": 1024,
                            "max_file_read_bytes": 2048,
                            "max_file_write_bytes": 2048,
                            "allow_command_newlines": true
                        }
                    }
                }
            })
            .to_string(),
        )
        .expect("write legacy profile store");

        let loaded = load_profile_store(&profile_path).expect("load legacy profiles");
        let legacy = loaded.get("legacy").expect("legacy profile");
        assert_eq!(legacy.auth.openai, ProviderAuthMethod::ApiKey);
        assert_eq!(legacy.auth.anthropic, ProviderAuthMethod::ApiKey);
        assert_eq!(legacy.auth.google, ProviderAuthMethod::ApiKey);
    }

    #[test]
    fn functional_render_profile_diffs_reports_changed_fields() {
        let current = test_profile_defaults();
        let mut loaded = current.clone();
        loaded.model = "google/gemini-2.5-pro".to_string();
        loaded.policy.max_command_length = 2048;
        loaded.session.import_mode = "replace".to_string();

        let diffs = render_profile_diffs(&current, &loaded);
        assert_eq!(diffs.len(), 3);
        assert!(diffs.iter().any(|line| line
            .contains("field=model current=openai/gpt-4o-mini loaded=google/gemini-2.5-pro")));
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=session.import_mode current=merge loaded=replace")));
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=policy.max_command_length current=4096 loaded=2048")));
    }

    #[test]
    fn functional_render_profile_diffs_reports_changed_auth_modes() {
        let current = test_profile_defaults();
        let mut loaded = current.clone();
        loaded.auth.openai = ProviderAuthMethod::OauthToken;
        loaded.auth.google = ProviderAuthMethod::Adc;

        let diffs = render_profile_diffs(&current, &loaded);
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=auth.openai current=api_key loaded=oauth_token")));
        assert!(diffs
            .iter()
            .any(|line| line.contains("field=auth.google current=api_key loaded=adc")));
    }

    #[test]
    fn unit_render_profile_list_and_show_produce_deterministic_output() {
        let profile_path = PathBuf::from("/tmp/profiles.json");
        let mut alternate = test_profile_defaults();
        alternate.model = "google/gemini-2.5-pro".to_string();
        let profiles = BTreeMap::from([
            ("zeta".to_string(), test_profile_defaults()),
            ("alpha".to_string(), alternate.clone()),
        ]);

        let list_output = render_profile_list(&profile_path, &profiles);
        assert!(list_output.contains("profile list: path=/tmp/profiles.json profiles=2"));
        let alpha_index = list_output.find("profile: name=alpha").expect("alpha row");
        let zeta_index = list_output.find("profile: name=zeta").expect("zeta row");
        assert!(alpha_index < zeta_index);

        let show_output = render_profile_show(&profile_path, "alpha", &alternate);
        assert!(
            show_output.contains("profile show: path=/tmp/profiles.json name=alpha status=found")
        );
        assert!(show_output.contains("value: model=google/gemini-2.5-pro"));
        assert!(show_output.contains("value: fallback_models=none"));
        assert!(show_output.contains("value: session.path=.pi/sessions/default.jsonl"));
        assert!(show_output.contains("value: policy.max_command_length=4096"));
        assert!(show_output.contains("value: auth.openai=api_key"));
    }

    #[test]
    fn integration_execute_profile_command_full_lifecycle_roundtrip() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".pi").join("profiles.json");
        let current = test_profile_defaults();

        let save_output = execute_profile_command("save baseline", &profile_path, &current);
        assert!(save_output.contains("profile save: path="));
        assert!(save_output.contains("name=baseline"));
        assert!(save_output.contains("status=saved"));

        let load_output = execute_profile_command("load baseline", &profile_path, &current);
        assert!(load_output.contains("profile load: path="));
        assert!(load_output.contains("name=baseline"));
        assert!(load_output.contains("status=in_sync"));
        assert!(load_output.contains("diffs=0"));

        let list_output = execute_profile_command("list", &profile_path, &current);
        assert!(list_output.contains("profile list: path="));
        assert!(list_output.contains("profiles=1"));
        assert!(list_output.contains("profile: name=baseline"));

        let show_output = execute_profile_command("show baseline", &profile_path, &current);
        assert!(show_output.contains("profile show: path="));
        assert!(show_output.contains("name=baseline status=found"));
        assert!(show_output.contains("value: model=openai/gpt-4o-mini"));

        let mut changed = current.clone();
        changed.model = "anthropic/claude-sonnet-4-20250514".to_string();
        let diff_output = execute_profile_command("load baseline", &profile_path, &changed);
        assert!(diff_output.contains("status=diff"));
        assert!(diff_output.contains("diff: field=model"));

        let delete_output = execute_profile_command("delete baseline", &profile_path, &current);
        assert!(delete_output.contains("profile delete: path="));
        assert!(delete_output.contains("name=baseline"));
        assert!(delete_output.contains("status=deleted"));
        assert!(delete_output.contains("remaining=0"));

        let list_after_delete = execute_profile_command("list", &profile_path, &current);
        assert!(list_after_delete.contains("profiles=0"));
        assert!(list_after_delete.contains("names=none"));
    }

    #[test]
    fn regression_execute_profile_command_reports_unknown_profile_and_schema_errors() {
        let temp = tempdir().expect("tempdir");
        let profile_path = temp.path().join(".pi").join("profiles.json");
        let current = test_profile_defaults();

        let missing_output = execute_profile_command("load missing", &profile_path, &current);
        assert!(missing_output.contains("profile error: path="));
        assert!(missing_output.contains("unknown profile 'missing'"));

        let missing_show = execute_profile_command("show missing", &profile_path, &current);
        assert!(missing_show.contains("profile error: path="));
        assert!(missing_show.contains("unknown profile 'missing'"));

        let missing_delete = execute_profile_command("delete missing", &profile_path, &current);
        assert!(missing_delete.contains("profile error: path="));
        assert!(missing_delete.contains("unknown profile 'missing'"));

        std::fs::create_dir_all(
            profile_path
                .parent()
                .expect("profile path should include parent dir"),
        )
        .expect("create profile dir");
        let invalid = serde_json::json!({
            "schema_version": 999,
            "profiles": {
                "baseline": current
            }
        });
        std::fs::write(&profile_path, format!("{invalid}\n")).expect("write invalid schema");

        let schema_output = execute_profile_command("load baseline", &profile_path, &current);
        assert!(schema_output.contains("profile error: path="));
        assert!(schema_output.contains("unsupported profile schema_version 999"));
    }

    #[test]
    fn regression_default_profile_store_path_uses_project_local_profiles_file() {
        let path = default_profile_store_path().expect("resolve profile store path");
        assert!(path.ends_with(Path::new(".pi").join("profiles.json")));
    }

    #[test]
    fn unit_command_file_error_mode_label_matches_cli_values() {
        assert_eq!(
            command_file_error_mode_label(CliCommandFileErrorMode::FailFast),
            "fail-fast"
        );
        assert_eq!(
            command_file_error_mode_label(CliCommandFileErrorMode::ContinueOnError),
            "continue-on-error"
        );
    }

    #[test]
    fn unit_parse_command_file_skips_comments_blanks_and_keeps_line_numbers() {
        let temp = tempdir().expect("tempdir");
        let command_file = temp.path().join("commands.txt");
        std::fs::write(
            &command_file,
            "# comment\n\n  /session  \nnot-command\n   # another comment\n/help session\n",
        )
        .expect("write command file");

        let entries = parse_command_file(&command_file).expect("parse command file");
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[0],
            CommandFileEntry {
                line_number: 3,
                command: "/session".to_string(),
            }
        );
        assert_eq!(
            entries[1],
            CommandFileEntry {
                line_number: 4,
                command: "not-command".to_string(),
            }
        );
        assert_eq!(
            entries[2],
            CommandFileEntry {
                line_number: 6,
                command: "/help session".to_string(),
            }
        );
    }

    #[test]
    fn functional_execute_command_file_runs_script_and_returns_summary() {
        let temp = tempdir().expect("tempdir");
        let command_file = temp.path().join("commands.txt");
        std::fs::write(&command_file, "/session\n/help session\n").expect("write command file");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut session_runtime = None;
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = test_command_context(
            &tool_policy_json,
            &profile_defaults,
            &skills_command_config,
            &auth_command_config,
        );

        let report = execute_command_file(
            &command_file,
            CliCommandFileErrorMode::FailFast,
            &mut agent,
            &mut session_runtime,
            command_context,
        )
        .expect("execute command file");

        assert_eq!(
            report,
            CommandFileReport {
                total: 2,
                executed: 2,
                succeeded: 2,
                failed: 0,
                halted_early: false,
            }
        );
    }

    #[test]
    fn integration_execute_command_file_continue_on_error_runs_remaining_commands() {
        let temp = tempdir().expect("tempdir");
        let command_file = temp.path().join("commands.txt");
        std::fs::write(&command_file, "/session\nnot-command\n/help session\n")
            .expect("write command file");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut session_runtime = None;
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = test_command_context(
            &tool_policy_json,
            &profile_defaults,
            &skills_command_config,
            &auth_command_config,
        );

        let report = execute_command_file(
            &command_file,
            CliCommandFileErrorMode::ContinueOnError,
            &mut agent,
            &mut session_runtime,
            command_context,
        )
        .expect("execute command file");

        assert_eq!(
            report,
            CommandFileReport {
                total: 3,
                executed: 3,
                succeeded: 2,
                failed: 1,
                halted_early: false,
            }
        );
    }

    #[test]
    fn regression_execute_command_file_fail_fast_stops_on_malformed_line() {
        let temp = tempdir().expect("tempdir");
        let command_file = temp.path().join("commands.txt");
        std::fs::write(&command_file, "/session\nnot-command\n/help session\n")
            .expect("write command file");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut session_runtime = None;
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let auth_command_config = test_auth_command_config();
        let skills_dir = temp.path().join("skills");
        let lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);
        let command_context = test_command_context(
            &tool_policy_json,
            &profile_defaults,
            &skills_command_config,
            &auth_command_config,
        );

        let error = execute_command_file(
            &command_file,
            CliCommandFileErrorMode::FailFast,
            &mut agent,
            &mut session_runtime,
            command_context,
        )
        .expect_err("fail-fast should stop on malformed command line");
        assert!(error.to_string().contains("command file execution failed"));
    }

    #[test]
    fn regression_parse_command_file_reports_missing_file() {
        let temp = tempdir().expect("tempdir");
        let missing = temp.path().join("missing-commands.txt");
        let error = parse_command_file(&missing).expect_err("missing command file should fail");
        assert!(error.to_string().contains("failed to read command file"));
    }

    #[test]
    fn unit_validate_branch_alias_name_accepts_and_rejects_expected_inputs() {
        validate_branch_alias_name("hotfix_1").expect("valid alias");

        let error = validate_branch_alias_name("").expect_err("empty alias should fail");
        assert!(error.to_string().contains("must not be empty"));

        let error = validate_branch_alias_name("1hotfix")
            .expect_err("alias starting with a digit should fail");
        assert!(error
            .to_string()
            .contains("must start with an ASCII letter"));

        let error = validate_branch_alias_name("hotfix.bad")
            .expect_err("alias with unsupported punctuation should fail");
        assert!(error
            .to_string()
            .contains("must contain only ASCII letters, digits, '-' or '_'"));
    }

    #[test]
    fn functional_parse_branch_alias_command_supports_core_subcommands() {
        assert_eq!(
            parse_branch_alias_command("list").expect("parse list"),
            BranchAliasCommand::List
        );
        assert_eq!(
            parse_branch_alias_command("set hotfix 42").expect("parse set"),
            BranchAliasCommand::Set {
                name: "hotfix".to_string(),
                id: 42,
            }
        );
        assert_eq!(
            parse_branch_alias_command("use hotfix").expect("parse use"),
            BranchAliasCommand::Use {
                name: "hotfix".to_string(),
            }
        );

        let error = parse_branch_alias_command("").expect_err("missing args should fail");
        assert!(error.to_string().contains(BRANCH_ALIAS_USAGE));

        let error =
            parse_branch_alias_command("set hotfix nope").expect_err("invalid id should fail");
        assert!(error.to_string().contains("invalid branch id 'nope'"));

        let error = parse_branch_alias_command("delete hotfix")
            .expect_err("unknown subcommand should fail");
        assert!(error.to_string().contains("unknown subcommand 'delete'"));
    }

    #[test]
    fn unit_save_and_load_branch_aliases_round_trip_schema_and_values() {
        let temp = tempdir().expect("tempdir");
        let alias_path = temp.path().join("session.aliases.json");
        let aliases = BTreeMap::from([
            ("hotfix".to_string(), 7_u64),
            ("rollback".to_string(), 12_u64),
        ]);

        save_branch_aliases(&alias_path, &aliases).expect("save aliases");

        let loaded = load_branch_aliases(&alias_path).expect("load aliases");
        assert_eq!(loaded, aliases);

        let raw = std::fs::read_to_string(&alias_path).expect("read alias file");
        let parsed = serde_json::from_str::<BranchAliasFile>(&raw).expect("parse alias file");
        assert_eq!(parsed.schema_version, BRANCH_ALIAS_SCHEMA_VERSION);
        assert_eq!(parsed.aliases, aliases);
    }

    #[test]
    fn integration_execute_branch_alias_command_supports_set_use_and_list_flow() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let stable = store
            .append_messages(Some(root), &[Message::assistant_text("stable branch")])
            .expect("append stable")
            .expect("stable id");
        let hot = store
            .append_messages(Some(root), &[Message::assistant_text("hot branch")])
            .expect("append hot")
            .expect("hot id");
        let mut runtime = SessionRuntime {
            store,
            active_head: Some(hot),
        };
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = runtime
            .store
            .lineage_messages(runtime.active_head)
            .expect("lineage");
        agent.replace_messages(lineage);

        let set_output =
            execute_branch_alias_command(&format!("set hotfix {stable}"), &mut agent, &mut runtime);
        assert!(set_output.contains("branch alias set: path="));
        assert!(set_output.contains("name=hotfix"));
        assert_eq!(runtime.active_head, Some(hot));

        let list_output = execute_branch_alias_command("list", &mut agent, &mut runtime);
        assert!(list_output.contains("branch alias list: path="));
        assert!(list_output.contains("count=1"));
        assert!(list_output.contains(&format!("alias: name=hotfix id={} status=ok", stable)));

        let use_output = execute_branch_alias_command("use hotfix", &mut agent, &mut runtime);
        assert!(use_output.contains("branch alias use: path="));
        assert!(use_output.contains(&format!("id={stable}")));
        assert_eq!(runtime.active_head, Some(stable));

        let alias_path = branch_alias_path_for_session(&session_path);
        let aliases = load_branch_aliases(&alias_path).expect("load aliases");
        assert_eq!(aliases.get("hotfix"), Some(&stable));
    }

    #[test]
    fn regression_execute_branch_alias_command_reports_stale_alias_ids() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let mut runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let alias_path = branch_alias_path_for_session(&session_path);
        let aliases = BTreeMap::from([("legacy".to_string(), 999_u64)]);
        save_branch_aliases(&alias_path, &aliases).expect("save stale alias");

        let list_output = execute_branch_alias_command("list", &mut agent, &mut runtime);
        assert!(list_output.contains("count=1"));
        assert!(list_output.contains("alias: name=legacy id=999 status=stale"));

        let use_output = execute_branch_alias_command("use legacy", &mut agent, &mut runtime);
        assert!(use_output.contains("branch alias error: path="));
        assert!(use_output.contains("alias points to unknown session id 999"));
    }

    #[test]
    fn regression_execute_branch_alias_command_reports_corrupt_alias_file() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let mut runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let alias_path = branch_alias_path_for_session(&session_path);
        std::fs::write(&alias_path, "{invalid-json").expect("write malformed alias file");

        let output = execute_branch_alias_command("list", &mut agent, &mut runtime);
        assert!(output.contains("branch alias error: path="));
        assert!(output.contains("failed to parse alias file"));
    }

    #[test]
    fn functional_parse_session_bookmark_command_supports_lifecycle_subcommands() {
        assert_eq!(
            parse_session_bookmark_command("list").expect("parse list"),
            SessionBookmarkCommand::List
        );
        assert_eq!(
            parse_session_bookmark_command("set checkpoint 42").expect("parse set"),
            SessionBookmarkCommand::Set {
                name: "checkpoint".to_string(),
                id: 42,
            }
        );
        assert_eq!(
            parse_session_bookmark_command("use checkpoint").expect("parse use"),
            SessionBookmarkCommand::Use {
                name: "checkpoint".to_string(),
            }
        );
        assert_eq!(
            parse_session_bookmark_command("delete checkpoint").expect("parse delete"),
            SessionBookmarkCommand::Delete {
                name: "checkpoint".to_string(),
            }
        );

        let error = parse_session_bookmark_command("").expect_err("empty args should fail");
        assert!(error.to_string().contains(SESSION_BOOKMARK_USAGE));

        let error = parse_session_bookmark_command("set checkpoint nope")
            .expect_err("invalid id should fail");
        assert!(error.to_string().contains("invalid bookmark id 'nope'"));

        let error =
            parse_session_bookmark_command("unknown checkpoint").expect_err("unknown subcommand");
        assert!(error.to_string().contains("unknown subcommand 'unknown'"));
    }

    #[test]
    fn unit_save_and_load_session_bookmarks_round_trip_schema_and_values() {
        let temp = tempdir().expect("tempdir");
        let bookmark_path = temp.path().join("session.bookmarks.json");
        let bookmarks = BTreeMap::from([
            ("checkpoint".to_string(), 7_u64),
            ("investigation".to_string(), 42_u64),
        ]);

        save_session_bookmarks(&bookmark_path, &bookmarks).expect("save bookmarks");
        let loaded = load_session_bookmarks(&bookmark_path).expect("load bookmarks");
        assert_eq!(loaded, bookmarks);

        let raw = std::fs::read_to_string(&bookmark_path).expect("read bookmark file");
        let parsed =
            serde_json::from_str::<SessionBookmarkFile>(&raw).expect("parse bookmark file");
        assert_eq!(parsed.schema_version, SESSION_BOOKMARK_SCHEMA_VERSION);
        assert_eq!(parsed.bookmarks, bookmarks);
    }

    #[test]
    fn integration_execute_session_bookmark_command_supports_set_use_list_delete_flow() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let stable = store
            .append_messages(Some(root), &[Message::user("stable branch")])
            .expect("append stable branch")
            .expect("stable id");

        let mut runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let initial_lineage = runtime
            .store
            .lineage_messages(runtime.active_head)
            .expect("initial lineage");
        agent.replace_messages(initial_lineage);

        let set_output = execute_session_bookmark_command(
            &format!("set checkpoint {stable}"),
            &mut agent,
            &mut runtime,
        );
        assert!(set_output.contains("session bookmark set: path="));
        assert!(set_output.contains("name=checkpoint"));
        assert!(set_output.contains(&format!("id={stable}")));

        let list_output = execute_session_bookmark_command("list", &mut agent, &mut runtime);
        assert!(list_output.contains("session bookmark list: path="));
        assert!(list_output.contains("count=1"));
        assert!(list_output.contains(&format!("bookmark: name=checkpoint id={stable} status=ok")));

        let use_output =
            execute_session_bookmark_command("use checkpoint", &mut agent, &mut runtime);
        assert!(use_output.contains("session bookmark use: path="));
        assert!(use_output.contains(&format!("id={stable}")));
        assert_eq!(runtime.active_head, Some(stable));

        let delete_output =
            execute_session_bookmark_command("delete checkpoint", &mut agent, &mut runtime);
        assert!(delete_output.contains("session bookmark delete: path="));
        assert!(delete_output.contains("status=deleted"));
        assert!(delete_output.contains("remaining=0"));

        let final_list = execute_session_bookmark_command("list", &mut agent, &mut runtime);
        assert!(final_list.contains("count=0"));
        assert!(final_list.contains("bookmarks: none"));
    }

    #[test]
    fn regression_execute_session_bookmark_command_reports_stale_ids() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let mut runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let bookmark_path = session_bookmark_path_for_session(&session_path);
        let bookmarks = BTreeMap::from([("legacy".to_string(), 999_u64)]);
        save_session_bookmarks(&bookmark_path, &bookmarks).expect("save stale bookmark");

        let list_output = execute_session_bookmark_command("list", &mut agent, &mut runtime);
        assert!(list_output.contains("count=1"));
        assert!(list_output.contains("bookmark: name=legacy id=999 status=stale"));

        let use_output = execute_session_bookmark_command("use legacy", &mut agent, &mut runtime);
        assert!(use_output.contains("session bookmark error: path="));
        assert!(use_output.contains("bookmark points to unknown session id 999"));
    }

    #[test]
    fn regression_execute_session_bookmark_command_reports_corrupt_bookmark_file() {
        let temp = tempdir().expect("tempdir");
        let session_path = temp.path().join("session.jsonl");
        let mut store = SessionStore::load(&session_path).expect("load");
        let root = store
            .append_messages(None, &[Message::system("root")])
            .expect("append root")
            .expect("root id");
        let mut runtime = SessionRuntime {
            store,
            active_head: Some(root),
        };
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let bookmark_path = session_bookmark_path_for_session(&session_path);
        std::fs::write(&bookmark_path, "{invalid-json").expect("write malformed bookmark file");

        let output = execute_session_bookmark_command("list", &mut agent, &mut runtime);
        assert!(output.contains("session bookmark error: path="));
        assert!(output.contains("failed to parse session bookmark file"));
    }

    #[test]
    fn functional_render_help_overview_lists_known_commands() {
        let help = render_help_overview();
        assert!(help.contains("/help [command]"));
        assert!(help.contains("/session"));
        assert!(help.contains("/session-search <query> [--role <role>] [--limit <n>]"));
        assert!(help.contains("/session-stats"));
        assert!(help.contains("/session-diff [<left-id> <right-id>]"));
        assert!(help.contains("/doctor"));
        assert!(help.contains("/session-graph-export <path>"));
        assert!(help.contains("/session-export <path>"));
        assert!(help.contains("/session-import <path>"));
        assert!(help.contains("/audit-summary <path>"));
        assert!(help.contains("/skills-search <query> [max_results]"));
        assert!(help.contains("/skills-show <name>"));
        assert!(help.contains("/skills-list"));
        assert!(help.contains("/skills-lock-diff [lockfile_path] [--json]"));
        assert!(help.contains("/skills-prune [lockfile_path] [--dry-run|--apply]"));
        assert!(help.contains("/skills-trust-list [trust_root_file]"));
        assert!(help.contains("/skills-trust-add <id=base64_key> [trust_root_file]"));
        assert!(help.contains("/skills-trust-revoke <id> [trust_root_file]"));
        assert!(help.contains("/skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]"));
        assert!(help.contains("/skills-verify [lockfile_path] [trust_root_file] [--json]"));
        assert!(help.contains("/skills-lock-write [lockfile_path]"));
        assert!(help.contains("/skills-sync [lockfile_path]"));
        assert!(help.contains("/macro <save|run|list|show|delete> ..."));
        assert!(help.contains("/auth <login|status|logout> ..."));
        assert!(help.contains("/integration-auth <set|status|rotate|revoke> ..."));
        assert!(help.contains("/profile <save|load|list|show|delete> ..."));
        assert!(help.contains("/branch <id>"));
        assert!(help.contains("/branch-alias <set|list|use> ..."));
        assert!(help.contains("/session-bookmark <set|list|use|delete> ..."));
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
    fn functional_render_command_help_supports_branch_alias_topic_without_slash() {
        let help = render_command_help("branch-alias").expect("render help");
        assert!(help.contains("command: /branch-alias"));
        assert!(help.contains("usage: /branch-alias <set|list|use> ..."));
        assert!(help.contains("example: /branch-alias set hotfix 42"));
    }

    #[test]
    fn functional_render_command_help_supports_session_bookmark_topic_without_slash() {
        let help = render_command_help("session-bookmark").expect("render help");
        assert!(help.contains("command: /session-bookmark"));
        assert!(help.contains("usage: /session-bookmark <set|list|use|delete> ..."));
        assert!(help.contains("example: /session-bookmark set investigation 42"));
    }

    #[test]
    fn functional_render_command_help_supports_macro_topic_without_slash() {
        let help = render_command_help("macro").expect("render help");
        assert!(help.contains("command: /macro"));
        assert!(help.contains("usage: /macro <save|run|list|show|delete> ..."));
        assert!(help.contains("example: /macro save quick-check /tmp/quick-check.commands"));
    }

    #[test]
    fn functional_render_command_help_supports_integration_auth_topic_without_slash() {
        let help = render_command_help("integration-auth").expect("render help");
        assert!(help.contains("command: /integration-auth"));
        assert!(help.contains("usage: /integration-auth <set|status|rotate|revoke> ..."));
        assert!(help.contains("example: /integration-auth status github-token --json"));
    }

    #[test]
    fn functional_render_command_help_supports_profile_topic_without_slash() {
        let help = render_command_help("profile").expect("render help");
        assert!(help.contains("command: /profile"));
        assert!(help.contains("usage: /profile <save|load|list|show|delete> ..."));
        assert!(help.contains("example: /profile save baseline"));
    }

    #[test]
    fn functional_render_command_help_supports_session_search_topic_without_slash() {
        let help = render_command_help("session-search").expect("render help");
        assert!(help.contains("command: /session-search"));
        assert!(help.contains("usage: /session-search <query> [--role <role>] [--limit <n>]"));
    }

    #[test]
    fn functional_render_command_help_supports_session_stats_topic_without_slash() {
        let help = render_command_help("session-stats").expect("render help");
        assert!(help.contains("command: /session-stats"));
        assert!(help.contains("usage: /session-stats [--json]"));
    }

    #[test]
    fn functional_render_command_help_supports_session_diff_topic_without_slash() {
        let help = render_command_help("session-diff").expect("render help");
        assert!(help.contains("command: /session-diff"));
        assert!(help.contains("usage: /session-diff [<left-id> <right-id>]"));
    }

    #[test]
    fn functional_render_command_help_supports_doctor_topic_without_slash() {
        let help = render_command_help("doctor").expect("render help");
        assert!(help.contains("command: /doctor"));
        assert!(help.contains("usage: /doctor [--json]"));
        assert!(help.contains("example: /doctor"));
    }

    #[test]
    fn functional_render_command_help_supports_session_graph_export_topic_without_slash() {
        let help = render_command_help("session-graph-export").expect("render help");
        assert!(help.contains("command: /session-graph-export"));
        assert!(help.contains("usage: /session-graph-export <path>"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_sync_topic_without_slash() {
        let help = render_command_help("skills-sync").expect("render help");
        assert!(help.contains("command: /skills-sync"));
        assert!(help.contains("usage: /skills-sync [lockfile_path]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_lock_write_topic_without_slash() {
        let help = render_command_help("skills-lock-write").expect("render help");
        assert!(help.contains("command: /skills-lock-write"));
        assert!(help.contains("usage: /skills-lock-write [lockfile_path]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_list_topic_without_slash() {
        let help = render_command_help("skills-list").expect("render help");
        assert!(help.contains("command: /skills-list"));
        assert!(help.contains("usage: /skills-list"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_show_topic_without_slash() {
        let help = render_command_help("skills-show").expect("render help");
        assert!(help.contains("command: /skills-show"));
        assert!(help.contains("usage: /skills-show <name>"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_search_topic_without_slash() {
        let help = render_command_help("skills-search").expect("render help");
        assert!(help.contains("command: /skills-search"));
        assert!(help.contains("usage: /skills-search <query> [max_results]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_lock_diff_topic_without_slash() {
        let help = render_command_help("skills-lock-diff").expect("render help");
        assert!(help.contains("command: /skills-lock-diff"));
        assert!(help.contains("usage: /skills-lock-diff [lockfile_path] [--json]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_prune_topic_without_slash() {
        let help = render_command_help("skills-prune").expect("render help");
        assert!(help.contains("command: /skills-prune"));
        assert!(help.contains("usage: /skills-prune [lockfile_path] [--dry-run|--apply]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_trust_list_topic_without_slash() {
        let help = render_command_help("skills-trust-list").expect("render help");
        assert!(help.contains("command: /skills-trust-list"));
        assert!(help.contains("usage: /skills-trust-list [trust_root_file]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_trust_add_topic_without_slash() {
        let help = render_command_help("skills-trust-add").expect("render help");
        assert!(help.contains("command: /skills-trust-add"));
        assert!(help.contains("usage: /skills-trust-add <id=base64_key> [trust_root_file]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_trust_revoke_topic_without_slash() {
        let help = render_command_help("skills-trust-revoke").expect("render help");
        assert!(help.contains("command: /skills-trust-revoke"));
        assert!(help.contains("usage: /skills-trust-revoke <id> [trust_root_file]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_trust_rotate_topic_without_slash() {
        let help = render_command_help("skills-trust-rotate").expect("render help");
        assert!(help.contains("command: /skills-trust-rotate"));
        assert!(help
            .contains("usage: /skills-trust-rotate <old_id:new_id=base64_key> [trust_root_file]"));
    }

    #[test]
    fn functional_render_command_help_supports_skills_verify_topic_without_slash() {
        let help = render_command_help("skills-verify").expect("render help");
        assert!(help.contains("command: /skills-verify"));
        assert!(help.contains("usage: /skills-verify [lockfile_path] [trust_root_file] [--json]"));
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
    fn unit_resolve_skills_lock_path_uses_default_and_explicit_values() {
        let default_lock_path = PathBuf::from(".pi/skills/skills.lock.json");
        assert_eq!(
            resolve_skills_lock_path("", &default_lock_path),
            default_lock_path
        );
        assert_eq!(
            resolve_skills_lock_path("custom/lock.json", &default_lock_path),
            PathBuf::from("custom/lock.json")
        );
    }

    #[test]
    fn unit_render_skills_sync_drift_details_uses_none_placeholders() {
        let report = crate::skills::SkillsSyncReport {
            expected_entries: 2,
            actual_entries: 2,
            ..crate::skills::SkillsSyncReport::default()
        };
        assert_eq!(
            render_skills_sync_drift_details(&report),
            "expected_entries=2 actual_entries=2 missing=none extra=none changed=none metadata=none"
        );
    }

    #[test]
    fn unit_render_skills_lock_write_success_formats_path_and_entry_count() {
        let rendered = render_skills_lock_write_success(Path::new("skills.lock.json"), 3);
        assert_eq!(
            rendered,
            "skills lock write: path=skills.lock.json entries=3"
        );
    }

    #[test]
    fn unit_render_skills_list_handles_empty_catalog() {
        let rendered = render_skills_list(Path::new(".pi/skills"), &[]);
        assert!(rendered.contains("skills list: path=.pi/skills count=0"));
        assert!(rendered.contains("skills: none"));
    }

    #[test]
    fn unit_render_skills_show_includes_metadata_and_content() {
        let skill = crate::skills::Skill {
            name: "checklist".to_string(),
            content: "line one\nline two".to_string(),
            path: PathBuf::from("checklist.md"),
        };
        let rendered = render_skills_show(Path::new(".pi/skills"), &skill);
        assert!(rendered.contains("skills show: path=.pi/skills"));
        assert!(rendered.contains("name=checklist"));
        assert!(rendered.contains("file=checklist.md"));
        assert!(rendered.contains("content_bytes=17"));
        assert!(rendered.contains("---\nline one\nline two"));
    }

    #[test]
    fn unit_parse_skills_search_args_defaults_and_supports_optional_limit() {
        assert_eq!(
            parse_skills_search_args("checklist").expect("parse default"),
            ("checklist".to_string(), 20)
        );
        assert_eq!(
            parse_skills_search_args("checklist 5").expect("parse explicit"),
            ("checklist".to_string(), 5)
        );
        assert_eq!(
            parse_skills_search_args("secure review 7").expect("parse multiword query"),
            ("secure review".to_string(), 7)
        );
    }

    #[test]
    fn regression_parse_skills_search_args_rejects_missing_query_and_zero_limit() {
        let missing_query = parse_skills_search_args("").expect_err("empty query must fail");
        assert!(missing_query.to_string().contains("query is required"));

        let zero_limit = parse_skills_search_args("checklist 0").expect_err("zero limit must fail");
        assert!(zero_limit
            .to_string()
            .contains("max_results must be greater than zero"));
    }

    #[test]
    fn unit_parse_skills_lock_diff_args_supports_defaults_path_override_and_json() {
        let default_lock = PathBuf::from(".pi/skills/skills.lock.json");
        assert_eq!(
            parse_skills_lock_diff_args("", &default_lock).expect("default parse"),
            (default_lock.clone(), false)
        );
        assert_eq!(
            parse_skills_lock_diff_args("--json", &default_lock).expect("json parse"),
            (default_lock.clone(), true)
        );
        assert_eq!(
            parse_skills_lock_diff_args("/tmp/custom.lock.json --json", &default_lock)
                .expect("path + json parse"),
            (PathBuf::from("/tmp/custom.lock.json"), true)
        );
    }

    #[test]
    fn regression_parse_skills_lock_diff_args_rejects_extra_positional_args() {
        let default_lock = PathBuf::from(".pi/skills/skills.lock.json");
        let error = parse_skills_lock_diff_args("one two", &default_lock).expect_err("must fail");
        assert!(error
            .to_string()
            .contains("usage: /skills-lock-diff [lockfile_path] [--json]"));
    }

    #[test]
    fn unit_parse_skills_prune_args_defaults_and_supports_mode_flags() {
        let default_lock = PathBuf::from(".pi/skills/skills.lock.json");
        assert_eq!(
            parse_skills_prune_args("", &default_lock).expect("default parse"),
            (default_lock.clone(), SkillsPruneMode::DryRun)
        );
        assert_eq!(
            parse_skills_prune_args("--apply", &default_lock).expect("apply parse"),
            (default_lock.clone(), SkillsPruneMode::Apply)
        );
        assert_eq!(
            parse_skills_prune_args("/tmp/custom.lock.json --dry-run", &default_lock)
                .expect("path + dry-run parse"),
            (
                PathBuf::from("/tmp/custom.lock.json"),
                SkillsPruneMode::DryRun
            )
        );
    }

    #[test]
    fn regression_parse_skills_prune_args_rejects_conflicts_and_extra_positionals() {
        let default_lock = PathBuf::from(".pi/skills/skills.lock.json");

        let conflict = parse_skills_prune_args("--apply --dry-run", &default_lock)
            .expect_err("conflicting flags should fail");
        assert!(conflict.to_string().contains(SKILLS_PRUNE_USAGE));

        let extra = parse_skills_prune_args("one two", &default_lock)
            .expect_err("extra positional args should fail");
        assert!(extra.to_string().contains(SKILLS_PRUNE_USAGE));
    }

    #[test]
    fn unit_validate_skills_prune_file_name_rejects_unsafe_paths() {
        validate_skills_prune_file_name("checklist.md").expect("simple markdown name should pass");
        assert!(validate_skills_prune_file_name("../checklist.md").is_err());
        assert!(validate_skills_prune_file_name("nested/checklist.md").is_err());
        assert!(validate_skills_prune_file_name(r"nested\checklist.md").is_err());
    }

    #[test]
    fn unit_derive_skills_prune_candidates_filters_tracked_and_sorts() {
        let skills_dir = Path::new(".pi/skills");
        let catalog = vec![
            crate::skills::Skill {
                name: "zeta".to_string(),
                content: "zeta".to_string(),
                path: PathBuf::from(".pi/skills/zeta.md"),
            },
            crate::skills::Skill {
                name: "alpha".to_string(),
                content: "alpha".to_string(),
                path: PathBuf::from(".pi/skills/alpha.md"),
            },
            crate::skills::Skill {
                name: "beta".to_string(),
                content: "beta".to_string(),
                path: PathBuf::from(".pi/skills/beta.md"),
            },
        ];
        let tracked = HashSet::from([String::from("alpha.md")]);
        let candidates = derive_skills_prune_candidates(skills_dir, &catalog, &tracked)
            .expect("derive candidates");
        let files = candidates
            .iter()
            .map(|candidate| candidate.file.as_str())
            .collect::<Vec<_>>();
        assert_eq!(files, vec!["beta.md", "zeta.md"]);
    }

    #[test]
    fn regression_resolve_prunable_skill_file_name_rejects_nested_paths() {
        let skills_dir = Path::new(".pi/skills");
        let error =
            resolve_prunable_skill_file_name(skills_dir, Path::new(".pi/skills/nested/a.md"))
                .expect_err("nested path should fail");
        assert!(error.to_string().contains("nested paths are not allowed"));
    }

    #[test]
    fn unit_parse_skills_trust_mutation_args_supports_configured_and_explicit_paths() {
        let configured = PathBuf::from("/tmp/trust-roots.json");
        assert_eq!(
            parse_skills_trust_mutation_args(
                "root=YQ==",
                Some(configured.as_path()),
                SKILLS_TRUST_ADD_USAGE
            )
            .expect("configured path should be used"),
            ("root=YQ==".to_string(), configured)
        );

        assert_eq!(
            parse_skills_trust_mutation_args(
                "root=YQ== /tmp/override.json",
                Some(Path::new("/tmp/default.json")),
                SKILLS_TRUST_ADD_USAGE
            )
            .expect("explicit path should override configured path"),
            ("root=YQ==".to_string(), PathBuf::from("/tmp/override.json"))
        );
    }

    #[test]
    fn regression_parse_skills_trust_mutation_args_requires_path_without_configuration() {
        let missing = parse_skills_trust_mutation_args("root=YQ==", None, SKILLS_TRUST_ADD_USAGE)
            .expect_err("command should fail without configured/default path");
        assert!(missing.to_string().contains(SKILLS_TRUST_ADD_USAGE));

        let extra = parse_skills_trust_mutation_args(
            "one two three",
            Some(Path::new("/tmp/default.json")),
            SKILLS_TRUST_ADD_USAGE,
        )
        .expect_err("extra positional args should fail");
        assert!(extra.to_string().contains(SKILLS_TRUST_ADD_USAGE));
    }

    #[test]
    fn unit_parse_skills_verify_args_supports_defaults_overrides_and_json() {
        let default_lock = Path::new("/tmp/default.lock.json");
        let default_trust = Path::new("/tmp/default-trust.json");

        let parsed = parse_skills_verify_args("", default_lock, Some(default_trust))
            .expect("parse defaults");
        assert_eq!(parsed.lock_path, PathBuf::from(default_lock));
        assert_eq!(parsed.trust_root_path, Some(PathBuf::from(default_trust)));
        assert!(!parsed.json_output);

        let parsed = parse_skills_verify_args(
            "/tmp/custom.lock.json /tmp/custom-trust.json --json",
            default_lock,
            Some(default_trust),
        )
        .expect("parse explicit args");
        assert_eq!(parsed.lock_path, PathBuf::from("/tmp/custom.lock.json"));
        assert_eq!(
            parsed.trust_root_path,
            Some(PathBuf::from("/tmp/custom-trust.json"))
        );
        assert!(parsed.json_output);
    }

    #[test]
    fn regression_parse_skills_verify_args_rejects_unexpected_extra_positionals() {
        let error = parse_skills_verify_args(
            "a b c",
            Path::new("/tmp/default.lock.json"),
            Some(Path::new("/tmp/default-trust.json")),
        )
        .expect_err("unexpected positional arguments should fail");
        assert!(error.to_string().contains(SKILLS_VERIFY_USAGE));
    }

    #[test]
    fn unit_parse_skills_trust_list_args_supports_configured_and_explicit_paths() {
        let configured = PathBuf::from("/tmp/trust-roots.json");
        assert_eq!(
            parse_skills_trust_list_args("", Some(configured.as_path()))
                .expect("configured path should be used"),
            configured
        );

        assert_eq!(
            parse_skills_trust_list_args(
                "/tmp/override.json",
                Some(Path::new("/tmp/default.json"))
            )
            .expect("explicit path should override configured path"),
            PathBuf::from("/tmp/override.json")
        );
    }

    #[test]
    fn regression_parse_skills_trust_list_args_requires_path_without_configuration() {
        let missing = parse_skills_trust_list_args("", None)
            .expect_err("command should fail without configured/default path");
        assert!(missing.to_string().contains(SKILLS_TRUST_LIST_USAGE));

        let extra = parse_skills_trust_list_args("one two", Some(Path::new("/tmp/default.json")))
            .expect_err("extra positional args should fail");
        assert!(extra.to_string().contains(SKILLS_TRUST_LIST_USAGE));
    }

    #[test]
    fn unit_trust_record_status_reports_active_revoked_and_expired() {
        let active = TrustedRootRecord {
            id: "active".to_string(),
            public_key: "YQ==".to_string(),
            revoked: false,
            expires_unix: None,
            rotated_from: None,
        };
        let revoked = TrustedRootRecord {
            id: "revoked".to_string(),
            public_key: "Yg==".to_string(),
            revoked: true,
            expires_unix: None,
            rotated_from: None,
        };
        let expired = TrustedRootRecord {
            id: "expired".to_string(),
            public_key: "Yw==".to_string(),
            revoked: false,
            expires_unix: Some(1),
            rotated_from: None,
        };

        assert_eq!(trust_record_status(&active, 10), "active");
        assert_eq!(trust_record_status(&revoked, 10), "revoked");
        assert_eq!(trust_record_status(&expired, 10), "expired");
    }

    #[test]
    fn unit_render_skills_trust_list_handles_empty_records() {
        let rendered = render_skills_trust_list(Path::new(".pi/trust-roots.json"), &[]);
        assert!(rendered.contains("skills trust list: path=.pi/trust-roots.json count=0"));
        assert!(rendered.contains("roots: none"));
    }

    #[test]
    fn unit_render_skills_lock_diff_helpers_include_expected_prefixes() {
        let report = crate::skills::SkillsSyncReport {
            expected_entries: 1,
            actual_entries: 1,
            ..crate::skills::SkillsSyncReport::default()
        };
        let in_sync = render_skills_lock_diff_in_sync(Path::new("skills.lock.json"), &report);
        assert!(in_sync.contains("skills lock diff: in-sync"));

        let drift = render_skills_lock_diff_drift(Path::new("skills.lock.json"), &report);
        assert!(drift.contains("skills lock diff: drift"));
    }

    #[test]
    fn unit_render_skills_search_handles_empty_results() {
        let rendered = render_skills_search(Path::new(".pi/skills"), "missing", 10, &[], 0);
        assert!(rendered.contains("skills search: path=.pi/skills"));
        assert!(rendered.contains("query=\"missing\""));
        assert!(rendered.contains("matched=0"));
        assert!(rendered.contains("shown=0"));
        assert!(rendered.contains("skills: none"));
    }

    #[test]
    fn functional_execute_skills_list_command_reports_sorted_inventory() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("zeta.md"), "zeta").expect("write zeta");
        std::fs::write(skills_dir.join("alpha.md"), "alpha").expect("write alpha");
        std::fs::write(skills_dir.join("ignored.txt"), "ignored").expect("write ignored");

        let output = execute_skills_list_command(&skills_dir);
        assert!(output.contains("count=2"));
        let alpha_index = output
            .find("skill: name=alpha file=alpha.md")
            .expect("alpha");
        let zeta_index = output.find("skill: name=zeta file=zeta.md").expect("zeta");
        assert!(alpha_index < zeta_index);
    }

    #[test]
    fn regression_execute_skills_list_command_reports_errors_without_panicking() {
        let temp = tempdir().expect("tempdir");
        let not_a_dir = temp.path().join("skills.md");
        std::fs::write(&not_a_dir, "not a directory").expect("write file");

        let output = execute_skills_list_command(&not_a_dir);
        assert!(output.contains("skills list error: path="));
        assert!(output.contains("is not a directory"));
    }

    #[test]
    fn functional_execute_skills_search_command_ranks_name_hits_before_content_hits() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("checklist.md"), "Always run tests")
            .expect("write checklist");
        std::fs::write(skills_dir.join("quality.md"), "Use checklist for review")
            .expect("write quality");

        let output = execute_skills_search_command(&skills_dir, "checklist");
        assert!(output.contains("skills search: path="));
        assert!(output.contains("matched=2"));
        let checklist_index = output
            .find("skill: name=checklist file=checklist.md match=name")
            .expect("checklist row");
        let quality_index = output
            .find("skill: name=quality file=quality.md match=content")
            .expect("quality row");
        assert!(checklist_index < quality_index);
    }

    #[test]
    fn regression_execute_skills_search_command_reports_invalid_args_without_panicking() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

        let output = execute_skills_search_command(&skills_dir, "checklist 0");
        assert!(output.contains("skills search error: path="));
        assert!(output.contains("max_results must be greater than zero"));
    }

    #[test]
    fn functional_execute_skills_lock_diff_command_supports_human_and_json_output() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let lock_path = default_skills_lock_path(&skills_dir);
        let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "focus",
                "file": "focus.md",
                "sha256": sha,
                "source": {
                    "kind": "unknown"
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

        let human = execute_skills_lock_diff_command(&skills_dir, &lock_path, "");
        assert!(human.contains("skills lock diff: in-sync"));
        assert!(human.contains("expected_entries=1"));

        let json_output = execute_skills_lock_diff_command(&skills_dir, &lock_path, "--json");
        let payload: serde_json::Value =
            serde_json::from_str(&json_output).expect("parse json output");
        assert_eq!(payload["status"], "in_sync");
        assert_eq!(payload["in_sync"], true);
        assert_eq!(payload["expected_entries"], 1);
        assert_eq!(payload["actual_entries"], 1);
    }

    #[test]
    fn regression_execute_skills_lock_diff_command_reports_missing_lockfile_errors() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let missing_lock_path = temp.path().join("missing.lock.json");
        let output = execute_skills_lock_diff_command(
            &skills_dir,
            &default_skills_lock_path(&skills_dir),
            missing_lock_path.to_str().expect("utf8 path"),
        );
        assert!(output.contains("skills lock diff error: path="));
        assert!(output.contains("failed to read skills lockfile"));
    }

    #[test]
    fn functional_execute_skills_prune_command_supports_dry_run_and_apply() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("tracked.md"), "tracked body").expect("write tracked");
        std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

        let lock_path = default_skills_lock_path(&skills_dir);
        let tracked_sha = format!("{:x}", Sha256::digest("tracked body".as_bytes()));
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "tracked",
                "file": "tracked.md",
                "sha256": tracked_sha,
                "source": {
                    "kind": "unknown"
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

        let dry_run = execute_skills_prune_command(&skills_dir, &lock_path, "");
        assert!(dry_run.contains("skills prune: mode=dry-run"));
        assert!(dry_run.contains("prune: file=stale.md action=would_delete"));
        assert!(skills_dir.join("stale.md").exists());

        let apply = execute_skills_prune_command(&skills_dir, &lock_path, "--apply");
        assert!(apply.contains("skills prune: mode=apply"));
        assert!(apply.contains("prune: file=stale.md action=delete"));
        assert!(apply.contains("prune: file=stale.md status=deleted"));
        assert!(apply.contains("skills prune result: mode=apply deleted=1 failed=0"));
        assert!(skills_dir.join("tracked.md").exists());
        assert!(!skills_dir.join("stale.md").exists());
    }

    #[test]
    fn regression_execute_skills_prune_command_reports_missing_lockfile_errors() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

        let missing_lock_path = temp.path().join("missing.lock.json");
        let output = execute_skills_prune_command(
            &skills_dir,
            &default_skills_lock_path(&skills_dir),
            missing_lock_path.to_str().expect("utf8 path"),
        );
        assert!(output.contains("skills prune error: path="));
        assert!(output.contains("failed to read skills lockfile"));
    }

    #[test]
    fn regression_execute_skills_prune_command_rejects_unsafe_lockfile_entries() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("stale.md"), "stale body").expect("write stale");

        let lock_path = default_skills_lock_path(&skills_dir);
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "escape",
                "file": "../escape.md",
                "sha256": "abc123",
                "source": {
                    "kind": "unknown"
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

        let output = execute_skills_prune_command(&skills_dir, &lock_path, "");
        assert!(output.contains("skills prune error: path="));
        assert!(output.contains("unsafe lockfile entry '../escape.md'"));
    }

    #[test]
    fn functional_execute_skills_trust_list_command_supports_default_and_explicit_paths() {
        let temp = tempdir().expect("tempdir");
        let default_trust_path = temp.path().join("trust-roots.json");
        let explicit_trust_path = temp.path().join("explicit-trust-roots.json");
        let payload = serde_json::json!({
            "roots": [
                {
                    "id": "zeta",
                    "public_key": "eg==",
                    "revoked": false,
                    "expires_unix": 1,
                    "rotated_from": null
                },
                {
                    "id": "alpha",
                    "public_key": "YQ==",
                    "revoked": false,
                    "expires_unix": null,
                    "rotated_from": null
                },
                {
                    "id": "beta",
                    "public_key": "Yg==",
                    "revoked": true,
                    "expires_unix": null,
                    "rotated_from": "alpha"
                }
            ]
        });
        std::fs::write(&default_trust_path, format!("{payload}\n")).expect("write default trust");
        std::fs::write(&explicit_trust_path, format!("{payload}\n")).expect("write explicit trust");

        let default_output =
            execute_skills_trust_list_command(Some(default_trust_path.as_path()), "");
        assert!(default_output.contains("skills trust list: path="));
        assert!(default_output.contains("count=3"));
        let alpha_index = default_output.find("root: id=alpha").expect("alpha row");
        let beta_index = default_output.find("root: id=beta").expect("beta row");
        let zeta_index = default_output.find("root: id=zeta").expect("zeta row");
        assert!(alpha_index < beta_index);
        assert!(beta_index < zeta_index);
        assert!(default_output.contains(
            "root: id=beta revoked=true expires_unix=none rotated_from=alpha status=revoked"
        ));
        assert!(default_output.contains(
            "root: id=zeta revoked=false expires_unix=1 rotated_from=none status=expired"
        ));

        let explicit_output = execute_skills_trust_list_command(
            None,
            explicit_trust_path.to_str().expect("utf8 path"),
        );
        assert!(explicit_output.contains("skills trust list: path="));
        assert!(explicit_output.contains("count=3"));
    }

    #[test]
    fn functional_render_skills_verify_report_includes_summary_sync_and_entries() {
        let report = SkillsVerifyReport {
            lock_path: "/tmp/skills.lock.json".to_string(),
            trust_root_path: Some("/tmp/trust-roots.json".to_string()),
            expected_entries: 2,
            actual_entries: 2,
            missing: vec![],
            extra: vec![],
            changed: vec![],
            metadata_mismatch: vec![],
            trust: Some(SkillsVerifyTrustSummary {
                total: 1,
                active: 1,
                revoked: 0,
                expired: 0,
            }),
            summary: SkillsVerifySummary {
                entries: 2,
                pass: 2,
                warn: 0,
                fail: 0,
                status: SkillsVerifyStatus::Pass,
            },
            entries: vec![SkillsVerifyEntry {
                file: "focus.md".to_string(),
                name: "focus".to_string(),
                status: SkillsVerifyStatus::Pass,
                checks: vec![
                    "sync=ok".to_string(),
                    "signature=trusted key=root".to_string(),
                ],
            }],
        };

        let rendered = render_skills_verify_report(&report);
        assert!(rendered.contains(
            "skills verify: status=pass lock_path=/tmp/skills.lock.json trust_root_path=/tmp/trust-roots.json"
        ));
        assert!(rendered.contains(
            "sync: expected_entries=2 actual_entries=2 missing=none extra=none changed=none metadata=none"
        ));
        assert!(rendered.contains("trust: total=1 active=1 revoked=0 expired=0"));
        assert!(rendered.contains(
            "entry: file=focus.md name=focus status=pass checks=sync=ok;signature=trusted key=root"
        ));
    }

    #[test]
    fn integration_execute_skills_verify_command_reports_pass_and_json_modes() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let lock_path = default_skills_lock_path(&skills_dir);
        let trust_path = temp.path().join("trust-roots.json");
        let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
        let signature = "c2ln";
        let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "focus",
                "file": "focus.md",
                "sha256": skill_sha,
                "source": {
                    "kind": "remote",
                    "url": "https://example.com/focus.md",
                    "expected_sha256": skill_sha,
                    "signing_key_id": "root",
                    "signature": signature,
                    "signer_public_key": "YQ==",
                    "signature_sha256": signature_sha
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
        let trust = serde_json::json!({
            "roots": [{
                "id": "root",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            }]
        });
        std::fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

        let output =
            execute_skills_verify_command(&skills_dir, &lock_path, Some(trust_path.as_path()), "");
        assert!(output.contains("skills verify: status=pass"));
        assert!(output.contains("sync: expected_entries=1 actual_entries=1"));
        assert!(output.contains("entry: file=focus.md name=focus status=pass"));
        assert!(output.contains("signature=trusted key=root"));

        let json_output = execute_skills_verify_command(
            &skills_dir,
            &lock_path,
            Some(trust_path.as_path()),
            "--json",
        );
        let payload: serde_json::Value =
            serde_json::from_str(&json_output).expect("parse verify json");
        assert_eq!(payload["summary"]["status"], "pass");
        assert_eq!(payload["summary"]["fail"], 0);
        assert_eq!(payload["entries"][0]["status"], "pass");
    }

    #[test]
    fn regression_execute_skills_verify_command_reports_untrusted_signing_key() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let lock_path = default_skills_lock_path(&skills_dir);
        let trust_path = temp.path().join("trust-roots.json");
        let skill_sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
        let signature = "c2ln";
        let signature_sha = format!("{:x}", Sha256::digest(signature.as_bytes()));
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "focus",
                "file": "focus.md",
                "sha256": skill_sha,
                "source": {
                    "kind": "remote",
                    "url": "https://example.com/focus.md",
                    "expected_sha256": skill_sha,
                    "signing_key_id": "unknown",
                    "signature": signature,
                    "signer_public_key": "YQ==",
                    "signature_sha256": signature_sha
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");
        let trust = serde_json::json!({
            "roots": [{
                "id": "root",
                "public_key": "YQ==",
                "revoked": false,
                "expires_unix": null,
                "rotated_from": null
            }]
        });
        std::fs::write(&trust_path, format!("{trust}\n")).expect("write trust");

        let output =
            execute_skills_verify_command(&skills_dir, &lock_path, Some(trust_path.as_path()), "");
        assert!(output.contains("skills verify: status=fail"));
        assert!(output.contains("signature=untrusted key=unknown"));
    }

    #[test]
    fn regression_execute_skills_verify_command_reports_missing_lockfile() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        let lock_path = temp.path().join("missing.lock.json");

        let output = execute_skills_verify_command(&skills_dir, &lock_path, None, "");
        assert!(output.contains("skills verify error: path="));
        assert!(output.contains("failed to read skills lockfile"));
    }

    #[test]
    fn functional_execute_skills_trust_mutation_commands_round_trip_updates_store() {
        let temp = tempdir().expect("tempdir");
        let trust_path = temp.path().join("trust-roots.json");
        let payload = serde_json::json!({
            "roots": [
                {
                    "id": "old",
                    "public_key": "YQ==",
                    "revoked": false,
                    "expires_unix": null,
                    "rotated_from": null
                }
            ]
        });
        std::fs::write(&trust_path, format!("{payload}\n")).expect("write trust file");

        let add_output = execute_skills_trust_add_command(Some(trust_path.as_path()), "extra=Yg==");
        assert!(add_output.contains("skills trust add: path="));
        assert!(add_output.contains("id=extra"));
        assert!(add_output.contains("added=1"));

        let revoke_output =
            execute_skills_trust_revoke_command(Some(trust_path.as_path()), "extra");
        assert!(revoke_output.contains("skills trust revoke: path="));
        assert!(revoke_output.contains("id=extra"));
        assert!(revoke_output.contains("revoked=1"));

        let rotate_output =
            execute_skills_trust_rotate_command(Some(trust_path.as_path()), "old:new=Yw==");
        assert!(rotate_output.contains("skills trust rotate: path="));
        assert!(rotate_output.contains("old_id=old"));
        assert!(rotate_output.contains("new_id=new"));
        assert!(rotate_output.contains("rotated=1"));

        let list_output = execute_skills_trust_list_command(Some(trust_path.as_path()), "");
        assert!(list_output.contains("skills trust list: path="));
        assert!(list_output.contains("root: id=old"));
        assert!(list_output.contains("status=revoked"));
        assert!(list_output.contains("root: id=new"));
        assert!(list_output.contains("rotated_from=old status=active"));
        assert!(list_output.contains("root: id=extra"));
        assert!(list_output.contains("status=revoked"));
    }

    #[test]
    fn regression_execute_skills_trust_add_command_requires_path_without_configuration() {
        let output = execute_skills_trust_add_command(None, "root=YQ==");
        assert!(output.contains("skills trust add error: path=none"));
        assert!(output.contains(SKILLS_TRUST_ADD_USAGE));
    }

    #[test]
    fn regression_execute_skills_trust_revoke_command_reports_unknown_id() {
        let temp = tempdir().expect("tempdir");
        let trust_path = temp.path().join("trust-roots.json");
        std::fs::write(&trust_path, "[]\n").expect("write trust file");

        let output = execute_skills_trust_revoke_command(Some(trust_path.as_path()), "missing");
        assert!(output.contains("skills trust revoke error: path="));
        assert!(output.contains("cannot revoke unknown trust key id 'missing'"));
    }

    #[test]
    fn regression_execute_skills_trust_rotate_command_reports_invalid_spec() {
        let temp = tempdir().expect("tempdir");
        let trust_path = temp.path().join("trust-roots.json");
        std::fs::write(&trust_path, "[]\n").expect("write trust file");

        let output = execute_skills_trust_rotate_command(Some(trust_path.as_path()), "bad-shape");
        assert!(output.contains("skills trust rotate error: path="));
        assert!(output.contains("expected old_id:new_id=base64_key"));
    }

    #[test]
    fn regression_execute_skills_trust_list_command_reports_malformed_json() {
        let temp = tempdir().expect("tempdir");
        let trust_path = temp.path().join("trust-roots.json");
        std::fs::write(&trust_path, "{not-json").expect("write malformed trust file");

        let output =
            execute_skills_trust_list_command(None, trust_path.to_str().expect("utf8 path"));
        assert!(output.contains("skills trust list error: path="));
        assert!(output.contains("failed to parse trusted root file"));
    }

    #[test]
    fn functional_execute_skills_show_command_displays_selected_skill() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("checklist.md"), "Always run tests").expect("write skill");

        let output = execute_skills_show_command(&skills_dir, "checklist");
        assert!(output.contains("skills show: path="));
        assert!(output.contains("name=checklist"));
        assert!(output.contains("file=checklist.md"));
        assert!(output.contains("Always run tests"));
    }

    #[test]
    fn regression_execute_skills_show_command_reports_unknown_skill_without_panicking() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("known.md"), "Known").expect("write skill");

        let output = execute_skills_show_command(&skills_dir, "missing");
        assert!(output.contains("skills show error: path="));
        assert!(output.contains("error=unknown skill 'missing'"));
    }

    #[test]
    fn functional_execute_skills_lock_write_command_writes_default_lockfile() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let lock_path = default_skills_lock_path(&skills_dir);
        let output = execute_skills_lock_write_command(&skills_dir, &lock_path, "");
        assert!(output.contains("skills lock write: path="));
        assert!(output.contains("entries=1"));

        let lock_raw = std::fs::read_to_string(lock_path).expect("read lockfile");
        assert!(lock_raw.contains("\"file\": \"focus.md\""));
    }

    #[test]
    fn regression_execute_skills_lock_write_command_reports_write_errors_without_panicking() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let blocking_path = temp.path().join("lock-as-dir");
        std::fs::create_dir_all(&blocking_path).expect("create blocking dir");

        let output = execute_skills_lock_write_command(
            &skills_dir,
            &default_skills_lock_path(&skills_dir),
            blocking_path.to_str().expect("utf8 path"),
        );
        assert!(output.contains("skills lock write error: path="));
        assert!(
            output.contains("failed to read skills lockfile")
                || output.contains("failed to write skills lockfile")
        );
    }

    #[test]
    fn functional_execute_skills_sync_command_reports_in_sync_for_default_lock_path() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let lock_path = default_skills_lock_path(&skills_dir);
        let sha = format!("{:x}", Sha256::digest("deterministic body".as_bytes()));
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "focus",
                "file": "focus.md",
                "sha256": sha,
                "source": {
                    "kind": "unknown"
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lockfile");

        let output = execute_skills_sync_command(&skills_dir, &lock_path, "");
        assert!(output.contains("skills sync: in-sync"));
        assert!(output.contains("expected_entries=1"));
        assert!(output.contains("actual_entries=1"));
    }

    #[test]
    fn regression_execute_skills_sync_command_reports_lockfile_errors_without_panicking() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "deterministic body").expect("write skill");

        let missing_lock_path = temp.path().join("missing.lock.json");
        let output = execute_skills_sync_command(
            &skills_dir,
            &default_skills_lock_path(&skills_dir),
            missing_lock_path.to_str().expect("utf8 path"),
        );

        assert!(output.contains("skills sync error: path="));
        assert!(output.contains("failed to read skills lockfile"));
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
    fn functional_audit_summary_command_without_path_returns_continue_action() {
        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let mut runtime = None;
        let tool_policy_json = test_tool_policy_json();

        let action = handle_command(
            "/audit-summary",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
        )
        .expect("audit summary usage should not fail");
        assert_eq!(action, CommandAction::Continue);
    }

    #[test]
    fn integration_skills_sync_command_preserves_session_runtime_on_drift() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
        let lock_path = default_skills_lock_path(&skills_dir);
        let lockfile = serde_json::json!({
            "schema_version": 1,
            "entries": [{
                "name": "focus",
                "file": "focus.md",
                "sha256": "deadbeef",
                "source": {
                    "kind": "unknown"
                }
            }]
        });
        std::fs::write(&lock_path, format!("{lockfile}\n")).expect("write lock");

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-sync",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills sync command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_lock_write_command_preserves_session_runtime_on_error() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("focus.md"), "actual body").expect("write skill");
        let lock_path = default_skills_lock_path(&skills_dir);
        let blocking_path = temp.path().join("lock-as-dir");
        std::fs::create_dir_all(&blocking_path).expect("blocking dir");

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            &format!("/skills-lock-write {}", blocking_path.display()),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills lock write command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_list_command_preserves_session_runtime() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        std::fs::write(skills_dir.join("beta.md"), "beta body").expect("write beta");
        let lock_path = default_skills_lock_path(&skills_dir);

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-list",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills list command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_show_command_preserves_session_runtime_on_unknown_skill() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-show missing",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills show command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_search_command_preserves_session_runtime_on_invalid_args() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-search alpha 0",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills search command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_lock_diff_command_preserves_session_runtime_on_error() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-lock-diff /tmp/missing.lock.json",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills lock diff command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_verify_command_preserves_session_runtime_on_error() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-verify /tmp/missing.lock.json",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills verify command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_prune_command_preserves_session_runtime_on_error() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

        let action = handle_command_with_session_import_mode(
            "/skills-prune /tmp/missing.lock.json --apply",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills prune command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_trust_list_command_preserves_session_runtime_on_error() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);
        let trust_path = temp.path().join("trust-roots.json");
        std::fs::write(&trust_path, "{invalid-json").expect("write malformed trust file");

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config =
            skills_command_config(&skills_dir, &lock_path, Some(trust_path.as_path()));

        let action = handle_command_with_session_import_mode(
            "/skills-trust-list",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills trust list command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());
    }

    #[test]
    fn integration_skills_trust_mutation_commands_update_store_and_preserve_runtime() {
        let temp = tempdir().expect("tempdir");
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).expect("mkdir");
        std::fs::write(skills_dir.join("alpha.md"), "alpha body").expect("write alpha");
        let lock_path = default_skills_lock_path(&skills_dir);
        let trust_path = temp.path().join("trust-roots.json");
        std::fs::write(&trust_path, "[]\n").expect("write empty trust file");

        let mut store = SessionStore::load(temp.path().join("session.jsonl")).expect("load");
        let root = store
            .append_messages(None, &[pi_ai::Message::system("sys")])
            .expect("append root")
            .expect("root id");
        let head = store
            .append_messages(Some(root), &[pi_ai::Message::user("hello")])
            .expect("append user")
            .expect("head id");

        let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
        let lineage = store.lineage_messages(Some(head)).expect("lineage");
        agent.replace_messages(lineage.clone());

        let mut runtime = Some(SessionRuntime {
            store,
            active_head: Some(head),
        });
        let tool_policy_json = test_tool_policy_json();
        let profile_defaults = test_profile_defaults();
        let skills_command_config =
            skills_command_config(&skills_dir, &lock_path, Some(trust_path.as_path()));

        let action = handle_command_with_session_import_mode(
            "/skills-trust-add root=YQ==",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills trust add command should continue");
        assert_eq!(action, CommandAction::Continue);

        let action = handle_command_with_session_import_mode(
            "/skills-trust-revoke root",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills trust revoke command should continue");
        assert_eq!(action, CommandAction::Continue);

        let action = handle_command_with_session_import_mode(
            "/skills-trust-rotate root:new=Yg==",
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Merge,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
        )
        .expect("skills trust rotate command should continue");
        assert_eq!(action, CommandAction::Continue);

        let runtime = runtime.expect("runtime");
        assert_eq!(runtime.active_head, Some(head));
        assert_eq!(runtime.store.entries().len(), 2);
        assert_eq!(agent.messages().len(), lineage.len());

        let records = load_trust_root_records(&trust_path).expect("load trust records");
        let root_record = records
            .iter()
            .find(|record| record.id == "root")
            .expect("root");
        let new_record = records
            .iter()
            .find(|record| record.id == "new")
            .expect("new");
        assert!(root_record.revoked);
        assert!(!new_record.revoked);
        assert_eq!(new_record.rotated_from.as_deref(), Some("root"));
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
    fn functional_resolve_secret_from_cli_or_store_id_reads_integration_secret() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        write_test_integration_credential(
            &store_path,
            CredentialStoreEncryptionMode::None,
            None,
            "github-token",
            IntegrationCredentialStoreRecord {
                secret: Some("ghp_store_secret".to_string()),
                revoked: false,
                updated_unix: Some(current_unix_timestamp()),
            },
        );

        let mut cli = test_cli();
        cli.credential_store = store_path;
        let resolved = resolve_secret_from_cli_or_store_id(
            &cli,
            None,
            Some("github-token"),
            "--github-token-id",
        )
        .expect("resolve secret")
        .expect("secret should be present");
        assert_eq!(resolved, "ghp_store_secret");
    }

    #[test]
    fn regression_resolve_secret_from_cli_or_store_id_rejects_revoked_secret() {
        let temp = tempdir().expect("tempdir");
        let store_path = temp.path().join("credentials.json");
        write_test_integration_credential(
            &store_path,
            CredentialStoreEncryptionMode::None,
            None,
            "slack-app-token",
            IntegrationCredentialStoreRecord {
                secret: Some("xapp_secret".to_string()),
                revoked: true,
                updated_unix: Some(current_unix_timestamp()),
            },
        );

        let mut cli = test_cli();
        cli.credential_store = store_path;
        let error = resolve_secret_from_cli_or_store_id(
            &cli,
            None,
            Some("slack-app-token"),
            "--slack-app-token-id",
        )
        .expect_err("revoked secret should fail");
        assert!(error.to_string().contains("is revoked"));
    }

    #[test]
    fn unit_resolve_secret_from_cli_or_store_id_prefers_direct_secret() {
        let cli = test_cli();
        let resolved = resolve_secret_from_cli_or_store_id(
            &cli,
            Some("direct-token"),
            Some("missing-id"),
            "--github-token-id",
        )
        .expect("resolve direct secret")
        .expect("secret");
        assert_eq!(resolved, "direct-token");
    }

    #[test]
    fn unit_validate_github_issues_bridge_cli_accepts_minimum_configuration() {
        let mut cli = test_cli();
        cli.github_issues_bridge = true;
        cli.github_repo = Some("owner/repo".to_string());
        cli.github_token = Some("token".to_string());

        validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
    }

    #[test]
    fn unit_validate_github_issues_bridge_cli_accepts_token_id_configuration() {
        let mut cli = test_cli();
        cli.github_issues_bridge = true;
        cli.github_repo = Some("owner/repo".to_string());
        cli.github_token_id = Some("github-token".to_string());

        validate_github_issues_bridge_cli(&cli).expect("bridge config should validate");
    }

    #[test]
    fn functional_validate_github_issues_bridge_cli_rejects_prompt_conflicts() {
        let mut cli = test_cli();
        cli.github_issues_bridge = true;
        cli.github_repo = Some("owner/repo".to_string());
        cli.github_token = Some("token".to_string());
        cli.prompt = Some("conflict".to_string());

        let error = validate_github_issues_bridge_cli(&cli).expect_err("prompt conflict");
        assert!(error
            .to_string()
            .contains("--github-issues-bridge cannot be combined"));
    }

    #[test]
    fn regression_validate_github_issues_bridge_cli_requires_credentials() {
        let mut cli = test_cli();
        cli.github_issues_bridge = true;
        cli.github_repo = Some("owner/repo".to_string());
        cli.github_token = None;
        cli.github_token_id = None;

        let error = validate_github_issues_bridge_cli(&cli).expect_err("missing token");
        assert!(error
            .to_string()
            .contains("--github-token (or --github-token-id) is required"));
    }

    #[test]
    fn unit_validate_slack_bridge_cli_accepts_minimum_configuration() {
        let mut cli = test_cli();
        cli.slack_bridge = true;
        cli.slack_app_token = Some("xapp-test".to_string());
        cli.slack_bot_token = Some("xoxb-test".to_string());

        validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
    }

    #[test]
    fn unit_validate_slack_bridge_cli_accepts_token_id_configuration() {
        let mut cli = test_cli();
        cli.slack_bridge = true;
        cli.slack_app_token_id = Some("slack-app-token".to_string());
        cli.slack_bot_token_id = Some("slack-bot-token".to_string());

        validate_slack_bridge_cli(&cli).expect("slack bridge config should validate");
    }

    #[test]
    fn functional_validate_slack_bridge_cli_rejects_prompt_conflicts() {
        let mut cli = test_cli();
        cli.slack_bridge = true;
        cli.slack_app_token = Some("xapp-test".to_string());
        cli.slack_bot_token = Some("xoxb-test".to_string());
        cli.prompt = Some("conflict".to_string());

        let error = validate_slack_bridge_cli(&cli).expect_err("prompt conflict");
        assert!(error
            .to_string()
            .contains("--slack-bridge cannot be combined"));
    }

    #[test]
    fn regression_validate_slack_bridge_cli_rejects_missing_tokens() {
        let mut cli = test_cli();
        cli.slack_bridge = true;
        cli.slack_app_token = Some("xapp-test".to_string());
        cli.slack_bot_token = None;
        cli.slack_app_token_id = None;
        cli.slack_bot_token_id = None;

        let error = validate_slack_bridge_cli(&cli).expect_err("missing slack bot token");
        assert!(error
            .to_string()
            .contains("--slack-bot-token (or --slack-bot-token-id) is required"));
    }

    #[test]
    fn unit_validate_events_runner_cli_accepts_minimum_configuration() {
        let mut cli = test_cli();
        cli.events_runner = true;
        validate_events_runner_cli(&cli).expect("events runner config should validate");
    }

    #[test]
    fn functional_validate_events_runner_cli_rejects_prompt_conflicts() {
        let mut cli = test_cli();
        cli.events_runner = true;
        cli.prompt = Some("conflict".to_string());
        let error = validate_events_runner_cli(&cli).expect_err("prompt conflict");
        assert!(error
            .to_string()
            .contains("--events-runner cannot be combined"));
    }

    #[test]
    fn regression_validate_event_webhook_ingest_cli_requires_channel() {
        let mut cli = test_cli();
        cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
        cli.event_webhook_channel = None;
        let error = validate_event_webhook_ingest_cli(&cli).expect_err("missing channel");
        assert!(error
            .to_string()
            .contains("--event-webhook-channel is required"));
    }

    #[test]
    fn functional_validate_event_webhook_ingest_cli_requires_signing_arguments_together() {
        let mut cli = test_cli();
        cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
        cli.event_webhook_channel = Some("slack/C123".to_string());
        cli.event_webhook_signature = Some("sha256=abcd".to_string());
        cli.event_webhook_secret = Some("secret".to_string());

        let error =
            validate_event_webhook_ingest_cli(&cli).expect_err("algorithm should be required");
        assert!(error
            .to_string()
            .contains("--event-webhook-signature-algorithm is required"));
    }

    #[test]
    fn functional_validate_event_webhook_ingest_cli_accepts_secret_id_configuration() {
        let mut cli = test_cli();
        cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
        cli.event_webhook_channel = Some("slack/C123".to_string());
        cli.event_webhook_signature = Some("sha256=abcd".to_string());
        cli.event_webhook_secret_id = Some("event-webhook-secret".to_string());
        cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

        validate_event_webhook_ingest_cli(&cli).expect("webhook config should validate");
    }

    #[test]
    fn regression_validate_event_webhook_ingest_cli_requires_timestamp_for_slack_v0() {
        let mut cli = test_cli();
        cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
        cli.event_webhook_channel = Some("slack/C123".to_string());
        cli.event_webhook_signature = Some("v0=abcd".to_string());
        cli.event_webhook_secret = Some("secret".to_string());
        cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::SlackV0);
        cli.event_webhook_timestamp = None;

        let error =
            validate_event_webhook_ingest_cli(&cli).expect_err("timestamp should be required");
        assert!(error
            .to_string()
            .contains("--event-webhook-timestamp is required"));
    }

    #[test]
    fn unit_validate_event_webhook_ingest_cli_accepts_signed_github_configuration() {
        let mut cli = test_cli();
        cli.event_webhook_ingest_file = Some(PathBuf::from("payload.json"));
        cli.event_webhook_channel = Some("github/owner/repo#1".to_string());
        cli.event_webhook_signature = Some("sha256=abcd".to_string());
        cli.event_webhook_secret = Some("secret".to_string());
        cli.event_webhook_signature_algorithm = Some(CliWebhookSignatureAlgorithm::GithubSha256);

        validate_event_webhook_ingest_cli(&cli).expect("signed github webhook config should pass");
    }

    #[test]
    fn functional_execute_channel_store_admin_inspect_succeeds() {
        let temp = tempdir().expect("tempdir");
        let store = crate::channel_store::ChannelStore::open(temp.path(), "github", "issue-1")
            .expect("open channel store");
        store
            .append_log_entry(&crate::channel_store::ChannelLogEntry {
                timestamp_unix_ms: 1,
                direction: "inbound".to_string(),
                event_key: Some("e1".to_string()),
                source: "github".to_string(),
                payload: serde_json::json!({"body":"hello"}),
            })
            .expect("append log");

        let mut cli = test_cli();
        cli.channel_store_root = temp.path().to_path_buf();
        cli.channel_store_inspect = Some("github/issue-1".to_string());

        execute_channel_store_admin_command(&cli).expect("inspect should succeed");
    }

    #[test]
    fn regression_execute_channel_store_admin_repair_removes_invalid_lines() {
        let temp = tempdir().expect("tempdir");
        let store = crate::channel_store::ChannelStore::open(temp.path(), "slack", "C123")
            .expect("open channel store");
        std::fs::write(store.log_path(), "{\"ok\":true}\ninvalid-json-line\n")
            .expect("seed invalid log");

        let mut cli = test_cli();
        cli.channel_store_root = temp.path().to_path_buf();
        cli.channel_store_repair = Some("slack/C123".to_string());
        execute_channel_store_admin_command(&cli).expect("repair should succeed");

        let report = store.inspect().expect("inspect after repair");
        assert_eq!(report.invalid_log_lines, 0);
        assert_eq!(report.log_records, 1);
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

    #[test]
    fn unit_percentile_duration_ms_handles_empty_and_unsorted_values() {
        assert_eq!(percentile_duration_ms(&[], 50), 0);
        assert_eq!(percentile_duration_ms(&[9], 95), 9);
        assert_eq!(percentile_duration_ms(&[50, 10, 20, 40, 30], 50), 30);
        assert_eq!(percentile_duration_ms(&[50, 10, 20, 40, 30], 95), 50);
    }

    #[test]
    fn functional_summarize_audit_file_aggregates_tool_and_provider_metrics() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("audit.jsonl");
        let rows = [
            serde_json::json!({
                "event": "tool_execution_end",
                "tool_name": "bash",
                "duration_ms": 12,
                "is_error": false
            }),
            serde_json::json!({
                "event": "tool_execution_end",
                "tool_name": "bash",
                "duration_ms": 32,
                "is_error": true
            }),
            serde_json::json!({
                "record_type": "prompt_telemetry_v1",
                "provider": "openai",
                "status": "completed",
                "success": true,
                "duration_ms": 100,
                "token_usage": {
                    "input_tokens": 4,
                    "output_tokens": 2,
                    "total_tokens": 6
                }
            }),
            serde_json::json!({
                "record_type": "prompt_telemetry_v1",
                "provider": "openai",
                "status": "interrupted",
                "success": false,
                "duration_ms": 180,
                "token_usage": {
                    "input_tokens": 1,
                    "output_tokens": 1,
                    "total_tokens": 2
                }
            }),
        ]
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        std::fs::write(&log_path, format!("{rows}\n")).expect("write audit log");

        let summary = summarize_audit_file(&log_path).expect("summary");
        assert_eq!(summary.record_count, 4);
        assert_eq!(summary.tool_event_count, 2);
        assert_eq!(summary.prompt_record_count, 2);

        let tool = summary.tools.get("bash").expect("tool aggregate");
        assert_eq!(tool.count, 2);
        assert_eq!(tool.error_count, 1);
        assert_eq!(percentile_duration_ms(&tool.durations_ms, 50), 12);
        assert_eq!(percentile_duration_ms(&tool.durations_ms, 95), 32);

        let provider = summary.providers.get("openai").expect("provider aggregate");
        assert_eq!(provider.count, 2);
        assert_eq!(provider.error_count, 1);
        assert_eq!(provider.input_tokens, 5);
        assert_eq!(provider.output_tokens, 3);
        assert_eq!(provider.total_tokens, 8);
    }

    #[test]
    fn functional_render_audit_summary_includes_expected_sections() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("audit.jsonl");
        std::fs::write(&path, "").expect("write empty log");
        let summary = summarize_audit_file(path.as_path()).expect("empty summary should parse");
        let output = render_audit_summary(&path, &summary);
        assert!(output.contains("audit summary:"));
        assert!(output.contains("tool_breakdown:"));
        assert!(output.contains("provider_breakdown:"));
    }

    #[test]
    fn integration_prompt_telemetry_logger_persists_completed_record() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("prompt-telemetry.jsonl");
        let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
            .expect("logger open");

        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("agent start");
        logger
            .log_event(&AgentEvent::TurnEnd {
                turn: 1,
                tool_results: 0,
                request_duration_ms: 44,
                usage: ChatUsage {
                    input_tokens: 4,
                    output_tokens: 2,
                    total_tokens: 6,
                },
                finish_reason: Some("stop".to_string()),
            })
            .expect("turn end");
        logger
            .log_event(&AgentEvent::AgentEnd { new_messages: 2 })
            .expect("agent end");

        let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 1);
        let record: serde_json::Value = serde_json::from_str(lines[0]).expect("parse record");
        assert_eq!(record["record_type"], "prompt_telemetry_v1");
        assert_eq!(record["provider"], "openai");
        assert_eq!(record["model"], "gpt-4o-mini");
        assert_eq!(record["status"], "completed");
        assert_eq!(record["success"], true);
        assert_eq!(record["finish_reason"], "stop");
        assert_eq!(record["token_usage"]["total_tokens"], 6);
        assert_eq!(record["redaction_policy"]["prompt_content"], "omitted");
    }

    #[test]
    fn regression_prompt_telemetry_logger_marks_interrupted_runs() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("prompt-telemetry.jsonl");
        let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
            .expect("logger open");

        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("first start");
        logger
            .log_event(&AgentEvent::TurnEnd {
                turn: 1,
                tool_results: 0,
                request_duration_ms: 11,
                usage: ChatUsage {
                    input_tokens: 1,
                    output_tokens: 1,
                    total_tokens: 2,
                },
                finish_reason: Some("length".to_string()),
            })
            .expect("first turn");
        logger
            .log_event(&AgentEvent::AgentStart)
            .expect("second start");
        logger
            .log_event(&AgentEvent::AgentEnd { new_messages: 1 })
            .expect("finalize second run");

        let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
        let lines = raw.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first record");
        let second: serde_json::Value = serde_json::from_str(lines[1]).expect("second record");
        assert_eq!(first["status"], "interrupted");
        assert_eq!(first["success"], false);
        assert_eq!(second["status"], "completed");
        assert_eq!(second["success"], true);
    }

    #[test]
    fn regression_summarize_audit_file_remains_compatible_with_tool_audit_logs() {
        let temp = tempdir().expect("tempdir");
        let log_path = temp.path().join("tool-audit.jsonl");
        let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");
        logger
            .log_event(&AgentEvent::ToolExecutionStart {
                tool_call_id: "call-1".to_string(),
                tool_name: "read".to_string(),
                arguments: serde_json::json!({ "path": "README.md" }),
            })
            .expect("start");
        logger
            .log_event(&AgentEvent::ToolExecutionEnd {
                tool_call_id: "call-1".to_string(),
                tool_name: "read".to_string(),
                result: ToolExecutionResult::ok(serde_json::json!({ "ok": true })),
            })
            .expect("end");

        let summary = summarize_audit_file(&log_path).expect("summarize");
        assert_eq!(summary.record_count, 2);
        assert_eq!(summary.tool_event_count, 1);
        assert_eq!(summary.prompt_record_count, 0);
        assert!(summary.providers.is_empty());
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
        let profile_defaults = test_profile_defaults();
        let skills_dir = PathBuf::from(".pi/skills");
        let skills_lock_path = default_skills_lock_path(&skills_dir);
        let skills_command_config = skills_command_config(&skills_dir, &skills_lock_path, None);

        let action = handle_command_with_session_import_mode(
            &format!("/session-import {}", import_path.display()),
            &mut agent,
            &mut runtime,
            &tool_policy_json,
            SessionImportMode::Replace,
            &profile_defaults,
            &skills_command_config,
            &test_auth_command_config(),
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
        assert_eq!(policy.policy_preset, ToolPolicyPreset::Balanced);
        assert!(!policy.bash_dry_run);
        assert!(!policy.tool_policy_trace);
    }

    #[test]
    fn unit_tool_policy_to_json_includes_key_limits_and_modes() {
        let mut cli = test_cli();
        cli.bash_profile = CliBashProfile::Strict;
        cli.os_sandbox_mode = CliOsSandboxMode::Auto;
        cli.max_file_write_bytes = 4096;

        let policy = build_tool_policy(&cli).expect("policy should build");
        let payload = tool_policy_to_json(&policy);
        assert_eq!(payload["schema_version"], 2);
        assert_eq!(payload["preset"], "balanced");
        assert_eq!(payload["bash_profile"], "strict");
        assert_eq!(payload["os_sandbox_mode"], "auto");
        assert_eq!(payload["max_file_write_bytes"], 4096);
        assert_eq!(payload["enforce_regular_files"], true);
        assert_eq!(payload["bash_dry_run"], false);
        assert_eq!(payload["tool_policy_trace"], false);
    }

    #[test]
    fn functional_build_tool_policy_hardened_preset_applies_hardened_defaults() {
        let mut cli = test_cli();
        cli.bash_timeout_ms = 120_000;
        cli.max_tool_output_bytes = 16_000;
        cli.max_file_read_bytes = 1_000_000;
        cli.max_file_write_bytes = 1_000_000;
        cli.max_command_length = 4_096;
        cli.allow_command_newlines = false;
        cli.bash_profile = CliBashProfile::Balanced;
        cli.os_sandbox_mode = CliOsSandboxMode::Off;
        cli.enforce_regular_files = true;
        cli.tool_policy_preset = CliToolPolicyPreset::Hardened;

        let policy = build_tool_policy(&cli).expect("policy should build");
        assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
        assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
        assert_eq!(policy.max_command_length, 1_024);
        assert_eq!(policy.max_command_output_bytes, 4_000);
        assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Force);
    }

    #[test]
    fn regression_build_tool_policy_explicit_profile_overrides_preset_profile() {
        let mut cli = test_cli();
        cli.bash_timeout_ms = 120_000;
        cli.max_tool_output_bytes = 16_000;
        cli.max_file_read_bytes = 1_000_000;
        cli.max_file_write_bytes = 1_000_000;
        cli.max_command_length = 4_096;
        cli.allow_command_newlines = false;
        cli.os_sandbox_mode = CliOsSandboxMode::Off;
        cli.enforce_regular_files = true;
        cli.tool_policy_preset = CliToolPolicyPreset::Hardened;
        cli.bash_profile = CliBashProfile::Permissive;

        let policy = build_tool_policy(&cli).expect("policy should build");
        assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
        assert_eq!(policy.bash_profile, BashCommandProfile::Permissive);
        assert!(policy.allowed_commands.is_empty());
    }

    #[test]
    fn functional_build_tool_policy_enables_trace_when_flag_set() {
        let mut cli = test_cli();
        cli.tool_policy_trace = true;
        let policy = build_tool_policy(&cli).expect("policy should build");
        assert!(policy.tool_policy_trace);
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
