mod atomic_io;
mod auth_commands;
mod auth_types;
mod bootstrap_helpers;
mod channel_store;
mod channel_store_admin;
mod cli_types;
mod commands;
mod credentials;
mod diagnostics_commands;
mod events;
mod github_issues;
mod macro_profile_commands;
mod observability_loggers;
mod provider_auth;
mod provider_client;
mod provider_credentials;
mod provider_fallback;
mod runtime_cli_validation;
mod runtime_loop;
mod runtime_output;
mod session;
mod session_commands;
mod session_graph_commands;
mod session_navigation_commands;
mod session_runtime_helpers;
mod skills;
mod skills_commands;
mod slack;
mod startup_config;
mod startup_resolution;
mod time_utils;
mod tool_policy_config;
mod tools;
#[cfg(test)]
mod transport_conformance;
mod trust_roots;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use clap::{ArgAction, Parser};
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::{
    AnthropicClient, AnthropicConfig, ChatRequest, ChatResponse, GoogleClient, GoogleConfig,
    LlmClient, Message, MessageRole, ModelRef, OpenAiClient, OpenAiConfig, PiAiError, Provider,
    StreamDeltaHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) use crate::atomic_io::write_text_atomic;
pub(crate) use crate::auth_commands::execute_auth_command;
#[cfg(test)]
pub(crate) use crate::auth_commands::{parse_auth_command, AuthCommand};
pub(crate) use crate::auth_types::{CredentialStoreEncryptionMode, ProviderAuthMethod};
pub(crate) use crate::bootstrap_helpers::{command_file_error_mode_label, init_tracing};
use crate::channel_store::ChannelStore;
pub(crate) use crate::channel_store_admin::execute_channel_store_admin_command;
pub(crate) use crate::cli_types::{
    CliBashProfile, CliCommandFileErrorMode, CliCredentialStoreEncryptionMode, CliOsSandboxMode,
    CliProviderAuthMode, CliSessionImportMode, CliToolPolicyPreset, CliWebhookSignatureAlgorithm,
};
#[cfg(test)]
pub(crate) use crate::commands::handle_command;
pub(crate) use crate::commands::{
    canonical_command_name, execute_command_file, handle_command_with_session_import_mode,
    parse_command, CommandAction, COMMAND_NAMES,
};
#[cfg(test)]
pub(crate) use crate::commands::{
    parse_command_file, render_command_help, render_help_overview, unknown_command_message,
    CommandFileEntry, CommandFileReport,
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
#[cfg(test)]
pub(crate) use crate::diagnostics_commands::execute_doctor_command;
pub(crate) use crate::diagnostics_commands::{
    build_doctor_command_config, execute_audit_summary_command, execute_doctor_cli_command,
    execute_policy_command,
};
#[cfg(test)]
pub(crate) use crate::diagnostics_commands::{
    parse_doctor_command_args, percentile_duration_ms, render_audit_summary, render_doctor_report,
    render_doctor_report_json, run_doctor_checks, summarize_audit_file, DoctorCheckResult,
    DoctorCommandOutputFormat, DoctorStatus,
};
use crate::events::{
    ingest_webhook_immediate_event, run_event_scheduler, EventSchedulerConfig,
    EventWebhookIngestConfig,
};
pub(crate) use crate::macro_profile_commands::{
    default_macro_config_path, default_profile_store_path, execute_macro_command,
    execute_profile_command,
};
#[cfg(test)]
pub(crate) use crate::macro_profile_commands::{
    load_macro_file, load_profile_store, parse_macro_command, parse_profile_command,
    render_macro_list, render_macro_show, render_profile_diffs, render_profile_list,
    render_profile_show, save_macro_file, save_profile_store, validate_macro_command_entry,
    validate_macro_name, validate_profile_name, MacroCommand, MacroFile, ProfileCommand,
    ProfileStoreFile, MACRO_SCHEMA_VERSION, MACRO_USAGE, PROFILE_SCHEMA_VERSION, PROFILE_USAGE,
};
#[cfg(test)]
pub(crate) use crate::observability_loggers::tool_audit_event_json;
pub(crate) use crate::observability_loggers::{PromptTelemetryLogger, ToolAuditLogger};
pub(crate) use crate::provider_auth::{
    configured_provider_auth_method, configured_provider_auth_method_from_config,
    missing_provider_api_key_message, provider_api_key_candidates,
    provider_api_key_candidates_with_inputs, provider_auth_capability, provider_auth_mode_flag,
    resolve_api_key,
};
pub(crate) use crate::provider_client::build_provider_client;
#[cfg(test)]
pub(crate) use crate::provider_credentials::resolve_store_backed_provider_credential;
pub(crate) use crate::provider_credentials::{
    resolve_non_empty_secret_with_source, CliProviderCredentialResolver, ProviderCredentialResolver,
};
pub(crate) use crate::provider_fallback::{build_client_with_fallbacks, resolve_fallback_models};
#[cfg(test)]
pub(crate) use crate::provider_fallback::{
    is_retryable_provider_error, ClientRoute, FallbackRoutingClient,
};
pub(crate) use crate::runtime_cli_validation::{
    validate_event_webhook_ingest_cli, validate_events_runner_cli,
    validate_github_issues_bridge_cli, validate_slack_bridge_cli,
};
pub(crate) use crate::runtime_loop::{
    resolve_prompt_input, run_interactive, run_prompt, run_prompt_with_cancellation,
    InteractiveRuntimeConfig, PromptRunStatus,
};
#[cfg(test)]
pub(crate) use crate::runtime_output::stream_text_chunks;
pub(crate) use crate::runtime_output::{
    event_to_json, persist_messages, print_assistant_messages, summarize_message,
};
use crate::session::{SessionImportMode, SessionStore};
#[cfg(test)]
pub(crate) use crate::session_commands::{
    compute_session_entry_depths, compute_session_stats, execute_session_diff_command,
    execute_session_search_command, execute_session_stats_command, parse_session_diff_args,
    parse_session_search_args, parse_session_stats_args, render_session_diff, render_session_stats,
    render_session_stats_json, search_session_entries, shared_lineage_prefix_depth,
    SessionDiffEntry, SessionDiffReport, SessionSearchArgs, SessionStats, SessionStatsOutputFormat,
    SESSION_SEARCH_DEFAULT_RESULTS, SESSION_SEARCH_PREVIEW_CHARS,
};
pub(crate) use crate::session_commands::{session_message_preview, session_message_role};
pub(crate) use crate::session_graph_commands::execute_session_graph_export_command;
#[cfg(test)]
pub(crate) use crate::session_graph_commands::{
    escape_graph_label, render_session_graph_dot, render_session_graph_mermaid,
    resolve_session_graph_format, SessionGraphFormat,
};
#[cfg(test)]
pub(crate) use crate::session_navigation_commands::{
    branch_alias_path_for_session, load_branch_aliases, load_session_bookmarks,
    parse_branch_alias_command, parse_session_bookmark_command, save_branch_aliases,
    save_session_bookmarks, session_bookmark_path_for_session, validate_branch_alias_name,
    BranchAliasCommand, BranchAliasFile, SessionBookmarkCommand, SessionBookmarkFile,
    BRANCH_ALIAS_SCHEMA_VERSION, BRANCH_ALIAS_USAGE, SESSION_BOOKMARK_SCHEMA_VERSION,
    SESSION_BOOKMARK_USAGE,
};
pub(crate) use crate::session_navigation_commands::{
    execute_branch_alias_command, execute_session_bookmark_command,
};
pub(crate) use crate::session_runtime_helpers::{
    format_id_list, format_remap_ids, initialize_session, reload_agent_from_active_head,
    validate_session_file,
};
use crate::skills::{
    augment_system_prompt, build_local_skill_lock_hints, build_registry_skill_lock_hints,
    build_remote_skill_lock_hints, default_skills_cache_dir, default_skills_lock_path,
    fetch_registry_manifest_with_cache, install_remote_skills_with_cache, install_skills,
    load_catalog, load_skills_lockfile, resolve_registry_skill_sources,
    resolve_remote_skill_sources, resolve_selected_skills, sync_skills_with_lockfile,
    write_skills_lockfile, SkillsDownloadOptions, TrustedKey,
};
#[cfg(test)]
pub(crate) use crate::skills_commands::{
    derive_skills_prune_candidates, parse_skills_lock_diff_args, parse_skills_prune_args,
    parse_skills_search_args, parse_skills_trust_list_args, parse_skills_trust_mutation_args,
    parse_skills_verify_args, render_skills_list, render_skills_lock_diff_drift,
    render_skills_lock_diff_in_sync, render_skills_search, render_skills_show,
    render_skills_trust_list, render_skills_verify_report, resolve_prunable_skill_file_name,
    resolve_skills_lock_path, trust_record_status, validate_skills_prune_file_name,
    SkillsPruneMode, SkillsVerifyEntry, SkillsVerifyReport, SkillsVerifyStatus,
    SkillsVerifySummary, SkillsVerifyTrustSummary, SKILLS_PRUNE_USAGE, SKILLS_TRUST_ADD_USAGE,
    SKILLS_TRUST_LIST_USAGE, SKILLS_VERIFY_USAGE,
};
pub(crate) use crate::skills_commands::{
    execute_skills_list_command, execute_skills_lock_diff_command,
    execute_skills_lock_write_command, execute_skills_prune_command, execute_skills_search_command,
    execute_skills_show_command, execute_skills_sync_command, execute_skills_trust_add_command,
    execute_skills_trust_list_command, execute_skills_trust_revoke_command,
    execute_skills_trust_rotate_command, execute_skills_verify_command,
    render_skills_lock_write_success, render_skills_sync_drift_details, render_skills_sync_in_sync,
};
pub(crate) use crate::startup_config::{
    build_auth_command_config, build_profile_defaults, default_provider_auth_method,
};
pub(crate) use crate::startup_resolution::{
    ensure_non_empty_text, resolve_skill_trust_roots, resolve_system_prompt,
};
pub(crate) use crate::time_utils::{
    current_unix_timestamp, current_unix_timestamp_ms, is_expired_unix,
};
#[cfg(test)]
pub(crate) use crate::tool_policy_config::parse_sandbox_command_tokens;
pub(crate) use crate::tool_policy_config::{build_tool_policy, tool_policy_to_json};
use crate::tools::{tool_policy_preset_name, ToolPolicy};
pub(crate) use crate::trust_roots::{
    apply_trust_root_mutation_specs, apply_trust_root_mutations, load_trust_root_records,
    parse_trust_rotation_spec, parse_trusted_root_spec, save_trust_root_records, TrustedRootRecord,
};
use github_issues::{run_github_issues_bridge, GithubIssuesBridgeRuntimeConfig};
use slack::{run_slack_bridge, SlackBridgeRuntimeConfig};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ProfileDefaults {
    model: String,
    fallback_models: Vec<String>,
    session: ProfileSessionDefaults,
    policy: ProfilePolicyDefaults,
    #[serde(default)]
    auth: ProfileAuthDefaults,
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

#[cfg(test)]
mod tests;
