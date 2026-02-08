use std::path::PathBuf;

use clap::{ArgAction, Parser};

use crate::{
    CliBashProfile, CliCommandFileErrorMode, CliCredentialStoreEncryptionMode, CliOrchestratorMode,
    CliOsSandboxMode, CliProviderAuthMode, CliSessionImportMode, CliToolPolicyPreset,
    CliWebhookSignatureAlgorithm,
};

#[derive(Debug, Parser)]
#[command(
    name = "pi-rs",
    about = "Pure Rust coding agent inspired by pi-mono",
    version
)]
pub(crate) struct Cli {
    #[arg(
        long,
        env = "PI_MODEL",
        default_value = "openai/gpt-4o-mini",
        help = "Model in provider/model format. Supported providers: openai, openrouter (alias), groq (alias), xai (alias), mistral (alias), azure/azure-openai (alias), anthropic, google."
    )]
    pub(crate) model: String,

    #[arg(
        long = "fallback-model",
        env = "PI_FALLBACK_MODEL",
        value_delimiter = ',',
        help = "Optional fallback model chain in provider/model format. Triggered only on retriable provider failures."
    )]
    pub(crate) fallback_model: Vec<String>,

    #[arg(
        long,
        env = "PI_API_BASE",
        default_value = "https://api.openai.com/v1",
        help = "Base URL for OpenAI-compatible APIs"
    )]
    pub(crate) api_base: String,

    #[arg(
        long = "azure-openai-api-version",
        env = "PI_AZURE_OPENAI_API_VERSION",
        default_value = "2024-10-21",
        help = "Azure OpenAI api-version query value used when --api-base points to an Azure deployment endpoint"
    )]
    pub(crate) azure_openai_api_version: String,

    #[arg(
        long = "model-catalog-url",
        env = "PI_MODEL_CATALOG_URL",
        help = "Optional remote URL for model catalog refresh (JSON payload)"
    )]
    pub(crate) model_catalog_url: Option<String>,

    #[arg(
        long = "model-catalog-cache",
        env = "PI_MODEL_CATALOG_CACHE",
        default_value = ".pi/models/catalog.json",
        help = "Model catalog cache path used for startup and interactive model discovery"
    )]
    pub(crate) model_catalog_cache: PathBuf,

    #[arg(
        long = "model-catalog-offline",
        env = "PI_MODEL_CATALOG_OFFLINE",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Disable remote model catalog refresh and rely on local cache/built-in entries"
    )]
    pub(crate) model_catalog_offline: bool,

    #[arg(
        long = "model-catalog-stale-after-hours",
        env = "PI_MODEL_CATALOG_STALE_AFTER_HOURS",
        default_value_t = 24,
        help = "Cache staleness threshold in hours for model catalog diagnostics"
    )]
    pub(crate) model_catalog_stale_after_hours: u64,

    #[arg(
        long,
        env = "PI_ANTHROPIC_API_BASE",
        default_value = "https://api.anthropic.com/v1",
        help = "Base URL for Anthropic Messages API"
    )]
    pub(crate) anthropic_api_base: String,

    #[arg(
        long,
        env = "PI_GOOGLE_API_BASE",
        default_value = "https://generativelanguage.googleapis.com/v1beta",
        help = "Base URL for Google Gemini API"
    )]
    pub(crate) google_api_base: String,

    #[arg(
        long,
        env = "PI_API_KEY",
        hide_env_values = true,
        help = "Generic API key fallback"
    )]
    pub(crate) api_key: Option<String>,

    #[arg(
        long,
        env = "OPENAI_API_KEY",
        hide_env_values = true,
        help = "API key for OpenAI-compatible APIs"
    )]
    pub(crate) openai_api_key: Option<String>,

    #[arg(
        long,
        env = "ANTHROPIC_API_KEY",
        hide_env_values = true,
        help = "API key for Anthropic"
    )]
    pub(crate) anthropic_api_key: Option<String>,

    #[arg(
        long,
        env = "GEMINI_API_KEY",
        hide_env_values = true,
        help = "API key for Google Gemini"
    )]
    pub(crate) google_api_key: Option<String>,

    #[arg(
        long = "openai-auth-mode",
        env = "PI_OPENAI_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for OpenAI provider"
    )]
    pub(crate) openai_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "anthropic-auth-mode",
        env = "PI_ANTHROPIC_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Anthropic provider"
    )]
    pub(crate) anthropic_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "google-auth-mode",
        env = "PI_GOOGLE_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Google provider"
    )]
    pub(crate) google_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "credential-store",
        env = "PI_CREDENTIAL_STORE",
        default_value = ".pi/credentials.json",
        help = "Credential store file path for non-API-key provider auth modes"
    )]
    pub(crate) credential_store: PathBuf,

    #[arg(
        long = "credential-store-key",
        env = "PI_CREDENTIAL_STORE_KEY",
        hide_env_values = true,
        help = "Optional encryption key for credential store entries when keyed encryption is enabled"
    )]
    pub(crate) credential_store_key: Option<String>,

    #[arg(
        long = "credential-store-encryption",
        env = "PI_CREDENTIAL_STORE_ENCRYPTION",
        value_enum,
        default_value_t = CliCredentialStoreEncryptionMode::Auto,
        help = "Credential store encryption mode: auto, none, or keyed"
    )]
    pub(crate) credential_store_encryption: CliCredentialStoreEncryptionMode,

    #[arg(
        long,
        env = "PI_SYSTEM_PROMPT",
        default_value = "You are a focused coding assistant. Prefer concrete steps and safe edits.",
        help = "System prompt"
    )]
    pub(crate) system_prompt: String,

    #[arg(
        long,
        env = "PI_SYSTEM_PROMPT_FILE",
        help = "Load system prompt from a UTF-8 text file (overrides --system-prompt)"
    )]
    pub(crate) system_prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_SKILLS_DIR",
        default_value = ".pi/skills",
        help = "Directory containing skill markdown files"
    )]
    pub(crate) skills_dir: PathBuf,

    #[arg(
        long = "skill",
        env = "PI_SKILL",
        value_delimiter = ',',
        help = "Skill name(s) to include in the system prompt"
    )]
    pub(crate) skills: Vec<String>,

    #[arg(
        long = "install-skill",
        env = "PI_INSTALL_SKILL",
        value_delimiter = ',',
        help = "Skill markdown file(s) to install into --skills-dir before startup"
    )]
    pub(crate) install_skill: Vec<PathBuf>,

    #[arg(
        long = "install-skill-url",
        env = "PI_INSTALL_SKILL_URL",
        value_delimiter = ',',
        help = "Skill URL(s) to install into --skills-dir before startup"
    )]
    pub(crate) install_skill_url: Vec<String>,

    #[arg(
        long = "install-skill-sha256",
        env = "PI_INSTALL_SKILL_SHA256",
        value_delimiter = ',',
        help = "Optional sha256 value(s) matching --install-skill-url entries"
    )]
    pub(crate) install_skill_sha256: Vec<String>,

    #[arg(
        long = "skill-registry-url",
        env = "PI_SKILL_REGISTRY_URL",
        help = "Remote registry manifest URL for skills"
    )]
    pub(crate) skill_registry_url: Option<String>,

    #[arg(
        long = "skill-registry-sha256",
        env = "PI_SKILL_REGISTRY_SHA256",
        help = "Optional sha256 checksum for the registry manifest"
    )]
    pub(crate) skill_registry_sha256: Option<String>,

    #[arg(
        long = "install-skill-from-registry",
        env = "PI_INSTALL_SKILL_FROM_REGISTRY",
        value_delimiter = ',',
        help = "Skill name(s) to install from the remote registry"
    )]
    pub(crate) install_skill_from_registry: Vec<String>,

    #[arg(
        long = "skills-cache-dir",
        env = "PI_SKILLS_CACHE_DIR",
        help = "Cache directory for downloaded registry manifests and remote skill artifacts (defaults to <skills-dir>/.cache)"
    )]
    pub(crate) skills_cache_dir: Option<PathBuf>,

    #[arg(
        long = "skills-offline",
        env = "PI_SKILLS_OFFLINE",
        default_value_t = false,
        help = "Disable network fetches for remote/registry skills and require cache hits"
    )]
    pub(crate) skills_offline: bool,

    #[arg(
        long = "skill-trust-root",
        env = "PI_SKILL_TRUST_ROOT",
        value_delimiter = ',',
        help = "Trusted root key(s) for skill signature verification in key_id=base64_public_key format"
    )]
    pub(crate) skill_trust_root: Vec<String>,

    #[arg(
        long = "skill-trust-root-file",
        env = "PI_SKILL_TRUST_ROOT_FILE",
        help = "JSON file containing trusted root keys for skill signature verification"
    )]
    pub(crate) skill_trust_root_file: Option<PathBuf>,

    #[arg(
        long = "skill-trust-add",
        env = "PI_SKILL_TRUST_ADD",
        value_delimiter = ',',
        help = "Add or update trusted key(s) in --skill-trust-root-file (key_id=base64_public_key)"
    )]
    pub(crate) skill_trust_add: Vec<String>,

    #[arg(
        long = "skill-trust-revoke",
        env = "PI_SKILL_TRUST_REVOKE",
        value_delimiter = ',',
        help = "Revoke trusted key id(s) in --skill-trust-root-file"
    )]
    pub(crate) skill_trust_revoke: Vec<String>,

    #[arg(
        long = "skill-trust-rotate",
        env = "PI_SKILL_TRUST_ROTATE",
        value_delimiter = ',',
        help = "Rotate trusted key(s) in --skill-trust-root-file using old_id:new_id=base64_public_key"
    )]
    pub(crate) skill_trust_rotate: Vec<String>,

    #[arg(
        long = "require-signed-skills",
        env = "PI_REQUIRE_SIGNED_SKILLS",
        default_value_t = false,
        help = "Require selected registry skills to provide signature metadata and validate against trusted roots"
    )]
    pub(crate) require_signed_skills: bool,

    #[arg(
        long = "skills-lock-file",
        env = "PI_SKILLS_LOCK_FILE",
        help = "Path to skills lockfile (defaults to <skills-dir>/skills.lock.json)"
    )]
    pub(crate) skills_lock_file: Option<PathBuf>,

    #[arg(
        long = "skills-lock-write",
        env = "PI_SKILLS_LOCK_WRITE",
        default_value_t = false,
        help = "Write/update skills lockfile from the current installed skills"
    )]
    pub(crate) skills_lock_write: bool,

    #[arg(
        long = "skills-sync",
        env = "PI_SKILLS_SYNC",
        default_value_t = false,
        help = "Verify installed skills match the lockfile and fail on drift"
    )]
    pub(crate) skills_sync: bool,

    #[arg(long, env = "PI_MAX_TURNS", default_value_t = 8)]
    pub(crate) max_turns: usize,

    #[arg(
        long,
        env = "PI_REQUEST_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "HTTP request timeout for provider API calls in milliseconds"
    )]
    pub(crate) request_timeout_ms: u64,

    #[arg(
        long,
        env = "PI_PROVIDER_MAX_RETRIES",
        default_value_t = 2,
        help = "Maximum retry attempts for retryable provider HTTP failures"
    )]
    pub(crate) provider_max_retries: usize,

    #[arg(
        long,
        env = "PI_PROVIDER_RETRY_BUDGET_MS",
        default_value_t = 0,
        help = "Optional cumulative retry backoff budget in milliseconds (0 disables budget)"
    )]
    pub(crate) provider_retry_budget_ms: u64,

    #[arg(
        long,
        env = "PI_PROVIDER_RETRY_JITTER",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Enable bounded jitter for provider retry backoff delays"
    )]
    pub(crate) provider_retry_jitter: bool,

    #[arg(
        long,
        env = "PI_TURN_TIMEOUT_MS",
        default_value_t = 0,
        help = "Optional per-prompt timeout in milliseconds (0 disables timeout)"
    )]
    pub(crate) turn_timeout_ms: u64,

    #[arg(long, help = "Print agent lifecycle events as JSON")]
    pub(crate) json_events: bool,

    #[arg(
        long,
        env = "PI_STREAM_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Render assistant text output token-by-token"
    )]
    pub(crate) stream_output: bool,

    #[arg(
        long,
        env = "PI_STREAM_DELAY_MS",
        default_value_t = 0,
        help = "Delay between streamed output chunks in milliseconds"
    )]
    pub(crate) stream_delay_ms: u64,

    #[arg(long, help = "Run one prompt and exit")]
    pub(crate) prompt: Option<String>,

    #[arg(
        long = "orchestrator-mode",
        env = "PI_ORCHESTRATOR_MODE",
        value_enum,
        default_value_t = CliOrchestratorMode::Off,
        help = "Optional orchestration mode for prompt execution"
    )]
    pub(crate) orchestrator_mode: CliOrchestratorMode,

    #[arg(
        long = "orchestrator-max-plan-steps",
        env = "PI_ORCHESTRATOR_MAX_PLAN_STEPS",
        default_value_t = 8,
        help = "Maximum planner step count allowed in plan-first orchestrator mode"
    )]
    pub(crate) orchestrator_max_plan_steps: usize,

    #[arg(
        long,
        env = "PI_PROMPT_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_template_file",
        help = "Read one prompt from a UTF-8 text file and exit"
    )]
    pub(crate) prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_PROMPT_TEMPLATE_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_file",
        help = "Read one prompt template from a UTF-8 text file and render placeholders like {{name}} before executing"
    )]
    pub(crate) prompt_template_file: Option<PathBuf>,

    #[arg(
        long = "prompt-template-var",
        value_name = "key=value",
        requires = "prompt_template_file",
        help = "Template variable assignment for --prompt-template-file (repeatable)"
    )]
    pub(crate) prompt_template_var: Vec<String>,

    #[arg(
        long,
        env = "PI_COMMAND_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_file",
        conflicts_with = "prompt_template_file",
        help = "Execute slash commands from a UTF-8 file and exit"
    )]
    pub(crate) command_file: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_COMMAND_FILE_ERROR_MODE",
        value_enum,
        default_value = "fail-fast",
        requires = "command_file",
        help = "Behavior when command-file execution hits malformed or failing commands"
    )]
    pub(crate) command_file_error_mode: CliCommandFileErrorMode,

    #[arg(
        long = "channel-store-root",
        env = "PI_CHANNEL_STORE_ROOT",
        default_value = ".pi/channel-store",
        help = "Base directory for transport-agnostic ChannelStore data"
    )]
    pub(crate) channel_store_root: PathBuf,

    #[arg(
        long = "channel-store-inspect",
        env = "PI_CHANNEL_STORE_INSPECT",
        conflicts_with = "channel_store_repair",
        value_name = "transport/channel_id",
        help = "Inspect ChannelStore state for one channel and exit"
    )]
    pub(crate) channel_store_inspect: Option<String>,

    #[arg(
        long = "channel-store-repair",
        env = "PI_CHANNEL_STORE_REPAIR",
        conflicts_with = "channel_store_inspect",
        value_name = "transport/channel_id",
        help = "Repair malformed ChannelStore JSONL files for one channel and exit"
    )]
    pub(crate) channel_store_repair: Option<String>,

    #[arg(
        long = "package-validate",
        env = "PI_PACKAGE_VALIDATE",
        conflicts_with = "package_show",
        value_name = "path",
        help = "Validate a package manifest JSON file and exit"
    )]
    pub(crate) package_validate: Option<PathBuf>,

    #[arg(
        long = "package-show",
        env = "PI_PACKAGE_SHOW",
        conflicts_with = "package_validate",
        value_name = "path",
        help = "Print package manifest metadata and component inventory"
    )]
    pub(crate) package_show: Option<PathBuf>,

    #[arg(
        long = "rpc-capabilities",
        env = "PI_RPC_CAPABILITIES",
        default_value_t = false,
        help = "Print versioned RPC protocol capabilities JSON and exit"
    )]
    pub(crate) rpc_capabilities: bool,

    #[arg(
        long = "rpc-validate-frame-file",
        env = "PI_RPC_VALIDATE_FRAME_FILE",
        value_name = "path",
        help = "Validate one RPC frame JSON file and exit"
    )]
    pub(crate) rpc_validate_frame_file: Option<PathBuf>,

    #[arg(
        long = "rpc-dispatch-frame-file",
        env = "PI_RPC_DISPATCH_FRAME_FILE",
        value_name = "path",
        help = "Dispatch one RPC request frame JSON file and print a response frame JSON"
    )]
    pub(crate) rpc_dispatch_frame_file: Option<PathBuf>,

    #[arg(
        long = "rpc-dispatch-ndjson-file",
        env = "PI_RPC_DISPATCH_NDJSON_FILE",
        value_name = "path",
        help = "Dispatch newline-delimited RPC request frames and print one response JSON line per frame"
    )]
    pub(crate) rpc_dispatch_ndjson_file: Option<PathBuf>,

    #[arg(
        long = "events-runner",
        env = "PI_EVENTS_RUNNER",
        default_value_t = false,
        help = "Run filesystem-backed scheduled events worker"
    )]
    pub(crate) events_runner: bool,

    #[arg(
        long = "events-dir",
        env = "PI_EVENTS_DIR",
        default_value = ".pi/events",
        help = "Directory containing event definition JSON files"
    )]
    pub(crate) events_dir: PathBuf,

    #[arg(
        long = "events-state-path",
        env = "PI_EVENTS_STATE_PATH",
        default_value = ".pi/events/state.json",
        help = "Persistent scheduler state path for periodic/debounce tracking"
    )]
    pub(crate) events_state_path: PathBuf,

    #[arg(
        long = "events-poll-interval-ms",
        env = "PI_EVENTS_POLL_INTERVAL_MS",
        default_value_t = 1_000,
        requires = "events_runner",
        help = "Scheduler poll interval in milliseconds"
    )]
    pub(crate) events_poll_interval_ms: u64,

    #[arg(
        long = "events-queue-limit",
        env = "PI_EVENTS_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "events_runner",
        help = "Maximum due events executed per poll cycle"
    )]
    pub(crate) events_queue_limit: usize,

    #[arg(
        long = "events-stale-immediate-max-age-seconds",
        env = "PI_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS",
        default_value_t = 86_400,
        requires = "events_runner",
        help = "Maximum age for immediate events before they are skipped and removed (0 disables)"
    )]
    pub(crate) events_stale_immediate_max_age_seconds: u64,

    #[arg(
        long = "event-webhook-ingest-file",
        env = "PI_EVENT_WEBHOOK_INGEST_FILE",
        value_name = "PATH",
        conflicts_with = "events_runner",
        help = "One-shot webhook ingestion: read payload file, enqueue debounced immediate event, and exit"
    )]
    pub(crate) event_webhook_ingest_file: Option<PathBuf>,

    #[arg(
        long = "event-webhook-channel",
        env = "PI_EVENT_WEBHOOK_CHANNEL",
        requires = "event_webhook_ingest_file",
        value_name = "transport/channel_id",
        help = "Channel reference used for webhook-ingested immediate events"
    )]
    pub(crate) event_webhook_channel: Option<String>,

    #[arg(
        long = "event-webhook-prompt-prefix",
        env = "PI_EVENT_WEBHOOK_PROMPT_PREFIX",
        default_value = "Handle webhook-triggered event.",
        requires = "event_webhook_ingest_file",
        help = "Prompt prefix prepended before webhook payload content"
    )]
    pub(crate) event_webhook_prompt_prefix: String,

    #[arg(
        long = "event-webhook-debounce-key",
        env = "PI_EVENT_WEBHOOK_DEBOUNCE_KEY",
        requires = "event_webhook_ingest_file",
        help = "Optional debounce key shared across webhook ingestions"
    )]
    pub(crate) event_webhook_debounce_key: Option<String>,

    #[arg(
        long = "event-webhook-debounce-window-seconds",
        env = "PI_EVENT_WEBHOOK_DEBOUNCE_WINDOW_SECONDS",
        default_value_t = 60,
        requires = "event_webhook_ingest_file",
        help = "Debounce window in seconds for repeated webhook ingestions with same key"
    )]
    pub(crate) event_webhook_debounce_window_seconds: u64,

    #[arg(
        long = "event-webhook-signature",
        env = "PI_EVENT_WEBHOOK_SIGNATURE",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Raw webhook signature header value (for signed ingest verification)"
    )]
    pub(crate) event_webhook_signature: Option<String>,

    #[arg(
        long = "event-webhook-timestamp",
        env = "PI_EVENT_WEBHOOK_TIMESTAMP",
        requires = "event_webhook_ingest_file",
        help = "Webhook timestamp header value used by signature algorithms that require timestamp checks"
    )]
    pub(crate) event_webhook_timestamp: Option<String>,

    #[arg(
        long = "event-webhook-secret",
        env = "PI_EVENT_WEBHOOK_SECRET",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Shared secret used to verify signed webhook payloads"
    )]
    pub(crate) event_webhook_secret: Option<String>,

    #[arg(
        long = "event-webhook-secret-id",
        env = "PI_EVENT_WEBHOOK_SECRET_ID",
        requires = "event_webhook_ingest_file",
        help = "Credential-store integration id used to resolve webhook signing secret"
    )]
    pub(crate) event_webhook_secret_id: Option<String>,

    #[arg(
        long = "event-webhook-signature-algorithm",
        env = "PI_EVENT_WEBHOOK_SIGNATURE_ALGORITHM",
        value_enum,
        requires = "event_webhook_ingest_file",
        help = "Webhook signature algorithm (github-sha256, slack-v0)"
    )]
    pub(crate) event_webhook_signature_algorithm: Option<CliWebhookSignatureAlgorithm>,

    #[arg(
        long = "event-webhook-signature-max-skew-seconds",
        env = "PI_EVENT_WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS",
        default_value_t = 300,
        requires = "event_webhook_ingest_file",
        help = "Maximum allowed webhook timestamp skew in seconds (0 disables skew checks)"
    )]
    pub(crate) event_webhook_signature_max_skew_seconds: u64,

    #[arg(
        long = "github-issues-bridge",
        env = "PI_GITHUB_ISSUES_BRIDGE",
        default_value_t = false,
        help = "Run as a GitHub Issues conversational transport loop instead of interactive prompt mode"
    )]
    pub(crate) github_issues_bridge: bool,

    #[arg(
        long = "github-repo",
        env = "PI_GITHUB_REPO",
        requires = "github_issues_bridge",
        help = "GitHub repository in owner/repo format used by --github-issues-bridge"
    )]
    pub(crate) github_repo: Option<String>,

    #[arg(
        long = "github-token",
        env = "GITHUB_TOKEN",
        hide_env_values = true,
        requires = "github_issues_bridge",
        help = "GitHub token used for API access in --github-issues-bridge mode"
    )]
    pub(crate) github_token: Option<String>,

    #[arg(
        long = "github-token-id",
        env = "PI_GITHUB_TOKEN_ID",
        requires = "github_issues_bridge",
        help = "Credential-store integration id used to resolve GitHub bridge token"
    )]
    pub(crate) github_token_id: Option<String>,

    #[arg(
        long = "github-bot-login",
        env = "PI_GITHUB_BOT_LOGIN",
        requires = "github_issues_bridge",
        help = "Optional bot login used to ignore self-comments and identify already-replied events"
    )]
    pub(crate) github_bot_login: Option<String>,

    #[arg(
        long = "github-api-base",
        env = "PI_GITHUB_API_BASE",
        default_value = "https://api.github.com",
        requires = "github_issues_bridge",
        help = "GitHub API base URL"
    )]
    pub(crate) github_api_base: String,

    #[arg(
        long = "github-state-dir",
        env = "PI_GITHUB_STATE_DIR",
        default_value = ".pi/github-issues",
        requires = "github_issues_bridge",
        help = "Directory for github bridge state/session/event logs"
    )]
    pub(crate) github_state_dir: PathBuf,

    #[arg(
        long = "github-poll-interval-seconds",
        env = "PI_GITHUB_POLL_INTERVAL_SECONDS",
        default_value_t = 30,
        requires = "github_issues_bridge",
        help = "Polling interval in seconds for github bridge mode"
    )]
    pub(crate) github_poll_interval_seconds: u64,

    #[arg(
        long = "github-include-issue-body",
        env = "PI_GITHUB_INCLUDE_ISSUE_BODY",
        default_value_t = false,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Treat the issue description itself as an initial conversation event"
    )]
    pub(crate) github_include_issue_body: bool,

    #[arg(
        long = "github-include-edited-comments",
        env = "PI_GITHUB_INCLUDE_EDITED_COMMENTS",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Process edited issue comments as new events (deduped by comment id + updated timestamp)"
    )]
    pub(crate) github_include_edited_comments: bool,

    #[arg(
        long = "github-processed-event-cap",
        env = "PI_GITHUB_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "github_issues_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    pub(crate) github_processed_event_cap: usize,

    #[arg(
        long = "github-retry-max-attempts",
        env = "PI_GITHUB_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "github_issues_bridge",
        help = "Maximum attempts for retryable github api failures (429/5xx/transport)"
    )]
    pub(crate) github_retry_max_attempts: usize,

    #[arg(
        long = "github-retry-base-delay-ms",
        env = "PI_GITHUB_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "github_issues_bridge",
        help = "Base backoff delay in milliseconds for github api retries"
    )]
    pub(crate) github_retry_base_delay_ms: u64,

    #[arg(
        long = "slack-bridge",
        env = "PI_SLACK_BRIDGE",
        default_value_t = false,
        help = "Run as a Slack Socket Mode conversational transport loop instead of interactive prompt mode"
    )]
    pub(crate) slack_bridge: bool,

    #[arg(
        long = "slack-app-token",
        env = "PI_SLACK_APP_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack Socket Mode app token (xapp-...)"
    )]
    pub(crate) slack_app_token: Option<String>,

    #[arg(
        long = "slack-app-token-id",
        env = "PI_SLACK_APP_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack app token"
    )]
    pub(crate) slack_app_token_id: Option<String>,

    #[arg(
        long = "slack-bot-token",
        env = "PI_SLACK_BOT_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack bot token for Web API (xoxb-...)"
    )]
    pub(crate) slack_bot_token: Option<String>,

    #[arg(
        long = "slack-bot-token-id",
        env = "PI_SLACK_BOT_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack bot token"
    )]
    pub(crate) slack_bot_token_id: Option<String>,

    #[arg(
        long = "slack-bot-user-id",
        env = "PI_SLACK_BOT_USER_ID",
        requires = "slack_bridge",
        help = "Optional bot user id used to strip self-mentions and ignore self-authored events"
    )]
    pub(crate) slack_bot_user_id: Option<String>,

    #[arg(
        long = "slack-api-base",
        env = "PI_SLACK_API_BASE",
        default_value = "https://slack.com/api",
        requires = "slack_bridge",
        help = "Slack Web API base URL"
    )]
    pub(crate) slack_api_base: String,

    #[arg(
        long = "slack-state-dir",
        env = "PI_SLACK_STATE_DIR",
        default_value = ".pi/slack",
        requires = "slack_bridge",
        help = "Directory for slack bridge state/session/event logs"
    )]
    pub(crate) slack_state_dir: PathBuf,

    #[arg(
        long = "slack-thread-detail-output",
        env = "PI_SLACK_THREAD_DETAIL_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "slack_bridge",
        help = "When responses exceed threshold, keep summary in placeholder and post full response as a threaded detail message"
    )]
    pub(crate) slack_thread_detail_output: bool,

    #[arg(
        long = "slack-thread-detail-threshold-chars",
        env = "PI_SLACK_THREAD_DETAIL_THRESHOLD_CHARS",
        default_value_t = 1500,
        requires = "slack_bridge",
        help = "Character threshold used with --slack-thread-detail-output"
    )]
    pub(crate) slack_thread_detail_threshold_chars: usize,

    #[arg(
        long = "slack-processed-event-cap",
        env = "PI_SLACK_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "slack_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    pub(crate) slack_processed_event_cap: usize,

    #[arg(
        long = "slack-max-event-age-seconds",
        env = "PI_SLACK_MAX_EVENT_AGE_SECONDS",
        default_value_t = 7_200,
        requires = "slack_bridge",
        help = "Ignore inbound Slack events older than this many seconds (0 disables age checks)"
    )]
    pub(crate) slack_max_event_age_seconds: u64,

    #[arg(
        long = "slack-reconnect-delay-ms",
        env = "PI_SLACK_RECONNECT_DELAY_MS",
        default_value_t = 1_000,
        requires = "slack_bridge",
        help = "Delay before reconnecting after socket/session errors"
    )]
    pub(crate) slack_reconnect_delay_ms: u64,

    #[arg(
        long = "slack-retry-max-attempts",
        env = "PI_SLACK_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "slack_bridge",
        help = "Maximum attempts for retryable slack api failures (429/5xx/transport)"
    )]
    pub(crate) slack_retry_max_attempts: usize,

    #[arg(
        long = "slack-retry-base-delay-ms",
        env = "PI_SLACK_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "slack_bridge",
        help = "Base backoff delay in milliseconds for slack api retries"
    )]
    pub(crate) slack_retry_base_delay_ms: u64,

    #[arg(
        long,
        env = "PI_SESSION",
        default_value = ".pi/sessions/default.jsonl",
        help = "Session JSONL file"
    )]
    pub(crate) session: PathBuf,

    #[arg(long, help = "Disable session persistence")]
    pub(crate) no_session: bool,

    #[arg(
        long,
        env = "PI_SESSION_VALIDATE",
        default_value_t = false,
        help = "Validate session graph integrity and exit"
    )]
    pub(crate) session_validate: bool,

    #[arg(
        long,
        env = "PI_SESSION_IMPORT_MODE",
        value_enum,
        default_value = "merge",
        help = "Import mode for /session-import: merge appends with id remapping, replace overwrites the current session"
    )]
    pub(crate) session_import_mode: CliSessionImportMode,

    #[arg(long, help = "Start from a specific session entry id")]
    pub(crate) branch_from: Option<u64>,

    #[arg(
        long,
        env = "PI_SESSION_LOCK_WAIT_MS",
        default_value_t = 5_000,
        help = "Maximum time to wait for acquiring the session lock in milliseconds"
    )]
    pub(crate) session_lock_wait_ms: u64,

    #[arg(
        long,
        env = "PI_SESSION_LOCK_STALE_MS",
        default_value_t = 30_000,
        help = "Lock-file age threshold in milliseconds before stale session locks are reclaimed (0 disables reclaim)"
    )]
    pub(crate) session_lock_stale_ms: u64,

    #[arg(
        long = "allow-path",
        env = "PI_ALLOW_PATH",
        value_delimiter = ',',
        help = "Allowed filesystem roots for read/write/edit/bash cwd (repeatable or comma-separated)"
    )]
    pub(crate) allow_path: Vec<PathBuf>,

    #[arg(
        long,
        env = "PI_BASH_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout for bash tool commands in milliseconds"
    )]
    pub(crate) bash_timeout_ms: u64,

    #[arg(
        long,
        env = "PI_MAX_TOOL_OUTPUT_BYTES",
        default_value_t = 16_000,
        help = "Maximum bytes returned from tool outputs (stdout/stderr)"
    )]
    pub(crate) max_tool_output_bytes: usize,

    #[arg(
        long,
        env = "PI_MAX_FILE_READ_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size read by the read tool"
    )]
    pub(crate) max_file_read_bytes: usize,

    #[arg(
        long,
        env = "PI_MAX_FILE_WRITE_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size written by write/edit tools"
    )]
    pub(crate) max_file_write_bytes: usize,

    #[arg(
        long,
        env = "PI_MAX_COMMAND_LENGTH",
        default_value_t = 4_096,
        help = "Maximum command length accepted by the bash tool"
    )]
    pub(crate) max_command_length: usize,

    #[arg(
        long,
        env = "PI_ALLOW_COMMAND_NEWLINES",
        default_value_t = false,
        help = "Allow newline characters in bash commands"
    )]
    pub(crate) allow_command_newlines: bool,

    #[arg(
        long,
        env = "PI_BASH_PROFILE",
        value_enum,
        default_value = "balanced",
        help = "Command execution profile for bash tool: permissive, balanced, or strict"
    )]
    pub(crate) bash_profile: CliBashProfile,

    #[arg(
        long,
        env = "PI_TOOL_POLICY_PRESET",
        value_enum,
        default_value = "balanced",
        help = "Tool policy preset: permissive, balanced, strict, or hardened"
    )]
    pub(crate) tool_policy_preset: CliToolPolicyPreset,

    #[arg(
        long,
        env = "PI_BASH_DRY_RUN",
        default_value_t = false,
        help = "Validate bash commands against policy without executing them"
    )]
    pub(crate) bash_dry_run: bool,

    #[arg(
        long,
        env = "PI_TOOL_POLICY_TRACE",
        default_value_t = false,
        help = "Include policy evaluation trace details in bash tool results"
    )]
    pub(crate) tool_policy_trace: bool,

    #[arg(
        long = "allow-command",
        env = "PI_ALLOW_COMMAND",
        value_delimiter = ',',
        help = "Additional command executables/prefixes to allow (supports trailing '*' wildcards)"
    )]
    pub(crate) allow_command: Vec<String>,

    #[arg(
        long,
        env = "PI_PRINT_TOOL_POLICY",
        default_value_t = false,
        help = "Print effective tool policy JSON before executing prompts"
    )]
    pub(crate) print_tool_policy: bool,

    #[arg(
        long,
        env = "PI_TOOL_AUDIT_LOG",
        help = "Optional JSONL file path for tool execution audit events"
    )]
    pub(crate) tool_audit_log: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_TELEMETRY_LOG",
        help = "Optional JSONL file path for prompt-level telemetry summaries"
    )]
    pub(crate) telemetry_log: Option<PathBuf>,

    #[arg(
        long,
        env = "PI_OS_SANDBOX_MODE",
        value_enum,
        default_value = "off",
        help = "OS sandbox mode for bash tool: off, auto, or force"
    )]
    pub(crate) os_sandbox_mode: CliOsSandboxMode,

    #[arg(
        long = "os-sandbox-command",
        env = "PI_OS_SANDBOX_COMMAND",
        value_delimiter = ',',
        help = "Optional sandbox launcher command template tokens. Supports placeholders: {shell}, {command}, {cwd}"
    )]
    pub(crate) os_sandbox_command: Vec<String>,

    #[arg(
        long,
        env = "PI_ENFORCE_REGULAR_FILES",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Require read/edit targets and existing write targets to be regular files (reject symlink targets)"
    )]
    pub(crate) enforce_regular_files: bool,
}
