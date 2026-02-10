use std::path::PathBuf;

use clap::{ArgAction, Parser};

use crate::{
    release_channel_commands::RELEASE_LOOKUP_CACHE_TTL_MS, CliBashProfile, CliCommandFileErrorMode,
    CliCredentialStoreEncryptionMode, CliDaemonProfile, CliDeploymentWasmRuntimeProfile,
    CliEventTemplateSchedule, CliGatewayOpenResponsesAuthMode, CliMultiChannelLiveConnectorMode,
    CliMultiChannelOutboundMode, CliMultiChannelTransport, CliOrchestratorMode, CliOsSandboxMode,
    CliProviderAuthMode, CliSessionImportMode, CliToolPolicyPreset, CliWebhookSignatureAlgorithm,
};

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| format!("failed to parse integer: {error}"))?;
    if parsed == 0 {
        return Err("value must be greater than 0".to_string());
    }
    Ok(parsed)
}

fn parse_positive_u64(value: &str) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|error| format!("failed to parse integer: {error}"))?;
    if parsed == 0 {
        return Err("value must be greater than 0".to_string());
    }
    Ok(parsed)
}

#[derive(Debug, Parser)]
#[command(
    name = "tau-rs",
    about = "Pure Rust coding agent inspired by upstream mono",
    version
)]
pub(crate) struct Cli {
    #[arg(
        long,
        env = "TAU_MODEL",
        default_value = "openai/gpt-4o-mini",
        help = "Model in provider/model format. Supported providers: openai, openrouter (alias), groq (alias), xai (alias), mistral (alias), azure/azure-openai (alias), anthropic, google."
    )]
    pub(crate) model: String,

    #[arg(
        long = "fallback-model",
        env = "TAU_FALLBACK_MODEL",
        value_delimiter = ',',
        help = "Optional fallback model chain in provider/model format. Triggered only on retriable provider failures."
    )]
    pub(crate) fallback_model: Vec<String>,

    #[arg(
        long,
        env = "TAU_API_BASE",
        default_value = "https://api.openai.com/v1",
        help = "Base URL for OpenAI-compatible APIs"
    )]
    pub(crate) api_base: String,

    #[arg(
        long = "azure-openai-api-version",
        env = "TAU_AZURE_OPENAI_API_VERSION",
        default_value = "2024-10-21",
        help = "Azure OpenAI api-version query value used when --api-base points to an Azure deployment endpoint"
    )]
    pub(crate) azure_openai_api_version: String,

    #[arg(
        long = "model-catalog-url",
        env = "TAU_MODEL_CATALOG_URL",
        help = "Optional remote URL for model catalog refresh (JSON payload)"
    )]
    pub(crate) model_catalog_url: Option<String>,

    #[arg(
        long = "model-catalog-cache",
        env = "TAU_MODEL_CATALOG_CACHE",
        default_value = ".tau/models/catalog.json",
        help = "Model catalog cache path used for startup and interactive model discovery"
    )]
    pub(crate) model_catalog_cache: PathBuf,

    #[arg(
        long = "model-catalog-offline",
        env = "TAU_MODEL_CATALOG_OFFLINE",
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
        env = "TAU_MODEL_CATALOG_STALE_AFTER_HOURS",
        default_value_t = 24,
        help = "Cache staleness threshold in hours for model catalog diagnostics"
    )]
    pub(crate) model_catalog_stale_after_hours: u64,

    #[arg(
        long,
        env = "TAU_ANTHROPIC_API_BASE",
        default_value = "https://api.anthropic.com/v1",
        help = "Base URL for Anthropic Messages API"
    )]
    pub(crate) anthropic_api_base: String,

    #[arg(
        long,
        env = "TAU_GOOGLE_API_BASE",
        default_value = "https://generativelanguage.googleapis.com/v1beta",
        help = "Base URL for Google Gemini API"
    )]
    pub(crate) google_api_base: String,

    #[arg(
        long,
        env = "TAU_API_KEY",
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
        env = "TAU_OPENAI_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for OpenAI provider"
    )]
    pub(crate) openai_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "openai-codex-backend",
        env = "TAU_OPENAI_CODEX_BACKEND",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Enable Codex CLI backend for OpenAI oauth/session auth modes"
    )]
    pub(crate) openai_codex_backend: bool,

    #[arg(
        long = "openai-codex-cli",
        env = "TAU_OPENAI_CODEX_CLI",
        default_value = "codex",
        help = "Codex CLI executable path used by OpenAI oauth/session backend"
    )]
    pub(crate) openai_codex_cli: String,

    #[arg(
        long = "openai-codex-args",
        env = "TAU_OPENAI_CODEX_ARGS",
        value_delimiter = ',',
        help = "Additional argument(s) forwarded to codex exec when OpenAI Codex backend is enabled"
    )]
    pub(crate) openai_codex_args: Vec<String>,

    #[arg(
        long = "openai-codex-timeout-ms",
        env = "TAU_OPENAI_CODEX_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout in milliseconds for each Codex CLI request"
    )]
    pub(crate) openai_codex_timeout_ms: u64,

    #[arg(
        long = "anthropic-auth-mode",
        env = "TAU_ANTHROPIC_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Anthropic provider"
    )]
    pub(crate) anthropic_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "anthropic-claude-backend",
        env = "TAU_ANTHROPIC_CLAUDE_BACKEND",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Enable Claude Code CLI backend for Anthropic oauth/session auth modes"
    )]
    pub(crate) anthropic_claude_backend: bool,

    #[arg(
        long = "anthropic-claude-cli",
        env = "TAU_ANTHROPIC_CLAUDE_CLI",
        default_value = "claude",
        help = "Claude Code CLI executable path used by Anthropic oauth/session backend"
    )]
    pub(crate) anthropic_claude_cli: String,

    #[arg(
        long = "anthropic-claude-args",
        env = "TAU_ANTHROPIC_CLAUDE_ARGS",
        value_delimiter = ',',
        help = "Additional argument(s) forwarded to claude for Anthropic oauth/session backend"
    )]
    pub(crate) anthropic_claude_args: Vec<String>,

    #[arg(
        long = "anthropic-claude-timeout-ms",
        env = "TAU_ANTHROPIC_CLAUDE_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout in milliseconds for each Claude Code CLI request"
    )]
    pub(crate) anthropic_claude_timeout_ms: u64,

    #[arg(
        long = "google-auth-mode",
        env = "TAU_GOOGLE_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Google provider"
    )]
    pub(crate) google_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "google-gemini-backend",
        env = "TAU_GOOGLE_GEMINI_BACKEND",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Enable Gemini CLI backend for Google oauth/adc auth modes"
    )]
    pub(crate) google_gemini_backend: bool,

    #[arg(
        long = "google-gemini-cli",
        env = "TAU_GOOGLE_GEMINI_CLI",
        default_value = "gemini",
        help = "Gemini CLI executable path used by Google oauth/adc backend"
    )]
    pub(crate) google_gemini_cli: String,

    #[arg(
        long = "google-gcloud-cli",
        env = "TAU_GOOGLE_GCLOUD_CLI",
        default_value = "gcloud",
        help = "gcloud executable path used for Google ADC login bootstrap"
    )]
    pub(crate) google_gcloud_cli: String,

    #[arg(
        long = "google-gemini-args",
        env = "TAU_GOOGLE_GEMINI_ARGS",
        value_delimiter = ',',
        help = "Additional argument(s) forwarded to gemini for Google oauth/adc backend"
    )]
    pub(crate) google_gemini_args: Vec<String>,

    #[arg(
        long = "google-gemini-timeout-ms",
        env = "TAU_GOOGLE_GEMINI_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout in milliseconds for each Gemini CLI request"
    )]
    pub(crate) google_gemini_timeout_ms: u64,

    #[arg(
        long = "credential-store",
        env = "TAU_CREDENTIAL_STORE",
        default_value = ".tau/credentials.json",
        help = "Credential store file path for non-API-key provider auth modes"
    )]
    pub(crate) credential_store: PathBuf,

    #[arg(
        long = "credential-store-key",
        env = "TAU_CREDENTIAL_STORE_KEY",
        hide_env_values = true,
        help = "Optional encryption key for credential store entries when keyed encryption is enabled"
    )]
    pub(crate) credential_store_key: Option<String>,

    #[arg(
        long = "credential-store-encryption",
        env = "TAU_CREDENTIAL_STORE_ENCRYPTION",
        value_enum,
        default_value_t = CliCredentialStoreEncryptionMode::Auto,
        help = "Credential store encryption mode: auto, none, or keyed"
    )]
    pub(crate) credential_store_encryption: CliCredentialStoreEncryptionMode,

    #[arg(
        long,
        env = "TAU_SYSTEM_PROMPT",
        default_value = "You are a focused coding assistant. Prefer concrete steps and safe edits.",
        help = "System prompt"
    )]
    pub(crate) system_prompt: String,

    #[arg(
        long,
        env = "TAU_SYSTEM_PROMPT_FILE",
        help = "Load system prompt from a UTF-8 text file (overrides --system-prompt)"
    )]
    pub(crate) system_prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_SKILLS_DIR",
        default_value = ".tau/skills",
        help = "Directory containing skill markdown files"
    )]
    pub(crate) skills_dir: PathBuf,

    #[arg(
        long = "skill",
        env = "TAU_SKILL",
        value_delimiter = ',',
        help = "Skill name(s) to include in the system prompt"
    )]
    pub(crate) skills: Vec<String>,

    #[arg(
        long = "install-skill",
        env = "TAU_INSTALL_SKILL",
        value_delimiter = ',',
        help = "Skill markdown file(s) to install into --skills-dir before startup"
    )]
    pub(crate) install_skill: Vec<PathBuf>,

    #[arg(
        long = "install-skill-url",
        env = "TAU_INSTALL_SKILL_URL",
        value_delimiter = ',',
        help = "Skill URL(s) to install into --skills-dir before startup"
    )]
    pub(crate) install_skill_url: Vec<String>,

    #[arg(
        long = "install-skill-sha256",
        env = "TAU_INSTALL_SKILL_SHA256",
        value_delimiter = ',',
        help = "Optional sha256 value(s) matching --install-skill-url entries"
    )]
    pub(crate) install_skill_sha256: Vec<String>,

    #[arg(
        long = "skill-registry-url",
        env = "TAU_SKILL_REGISTRY_URL",
        help = "Remote registry manifest URL for skills"
    )]
    pub(crate) skill_registry_url: Option<String>,

    #[arg(
        long = "skill-registry-sha256",
        env = "TAU_SKILL_REGISTRY_SHA256",
        help = "Optional sha256 checksum for the registry manifest"
    )]
    pub(crate) skill_registry_sha256: Option<String>,

    #[arg(
        long = "install-skill-from-registry",
        env = "TAU_INSTALL_SKILL_FROM_REGISTRY",
        value_delimiter = ',',
        help = "Skill name(s) to install from the remote registry"
    )]
    pub(crate) install_skill_from_registry: Vec<String>,

    #[arg(
        long = "skills-cache-dir",
        env = "TAU_SKILLS_CACHE_DIR",
        help = "Cache directory for downloaded registry manifests and remote skill artifacts (defaults to <skills-dir>/.cache)"
    )]
    pub(crate) skills_cache_dir: Option<PathBuf>,

    #[arg(
        long = "skills-offline",
        env = "TAU_SKILLS_OFFLINE",
        default_value_t = false,
        help = "Disable network fetches for remote/registry skills and require cache hits"
    )]
    pub(crate) skills_offline: bool,

    #[arg(
        long = "skill-trust-root",
        env = "TAU_SKILL_TRUST_ROOT",
        value_delimiter = ',',
        help = "Trusted root key(s) for skill signature verification in key_id=base64_public_key format"
    )]
    pub(crate) skill_trust_root: Vec<String>,

    #[arg(
        long = "skill-trust-root-file",
        env = "TAU_SKILL_TRUST_ROOT_FILE",
        help = "JSON file containing trusted root keys for skill signature verification"
    )]
    pub(crate) skill_trust_root_file: Option<PathBuf>,

    #[arg(
        long = "skill-trust-add",
        env = "TAU_SKILL_TRUST_ADD",
        value_delimiter = ',',
        help = "Add or update trusted key(s) in --skill-trust-root-file (key_id=base64_public_key)"
    )]
    pub(crate) skill_trust_add: Vec<String>,

    #[arg(
        long = "skill-trust-revoke",
        env = "TAU_SKILL_TRUST_REVOKE",
        value_delimiter = ',',
        help = "Revoke trusted key id(s) in --skill-trust-root-file"
    )]
    pub(crate) skill_trust_revoke: Vec<String>,

    #[arg(
        long = "skill-trust-rotate",
        env = "TAU_SKILL_TRUST_ROTATE",
        value_delimiter = ',',
        help = "Rotate trusted key(s) in --skill-trust-root-file using old_id:new_id=base64_public_key"
    )]
    pub(crate) skill_trust_rotate: Vec<String>,

    #[arg(
        long = "require-signed-skills",
        env = "TAU_REQUIRE_SIGNED_SKILLS",
        default_value_t = false,
        help = "Require selected registry skills to provide signature metadata and validate against trusted roots"
    )]
    pub(crate) require_signed_skills: bool,

    #[arg(
        long = "require-signed-packages",
        env = "TAU_REQUIRE_SIGNED_PACKAGES",
        default_value_t = false,
        help = "Require package manifests to provide signing metadata and validate against trusted roots"
    )]
    pub(crate) require_signed_packages: bool,

    #[arg(
        long = "skills-lock-file",
        env = "TAU_SKILLS_LOCK_FILE",
        help = "Path to skills lockfile (defaults to <skills-dir>/skills.lock.json)"
    )]
    pub(crate) skills_lock_file: Option<PathBuf>,

    #[arg(
        long = "skills-lock-write",
        env = "TAU_SKILLS_LOCK_WRITE",
        default_value_t = false,
        help = "Write/update skills lockfile from the current installed skills"
    )]
    pub(crate) skills_lock_write: bool,

    #[arg(
        long = "skills-sync",
        env = "TAU_SKILLS_SYNC",
        default_value_t = false,
        help = "Verify installed skills match the lockfile and fail on drift"
    )]
    pub(crate) skills_sync: bool,

    #[arg(long, env = "TAU_MAX_TURNS", default_value_t = 8)]
    pub(crate) max_turns: usize,

    #[arg(
        long,
        env = "TAU_REQUEST_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "HTTP request timeout for provider API calls in milliseconds"
    )]
    pub(crate) request_timeout_ms: u64,

    #[arg(
        long,
        env = "TAU_PROVIDER_MAX_RETRIES",
        default_value_t = 2,
        help = "Maximum retry attempts for retryable provider HTTP failures"
    )]
    pub(crate) provider_max_retries: usize,

    #[arg(
        long,
        env = "TAU_PROVIDER_RETRY_BUDGET_MS",
        default_value_t = 0,
        help = "Optional cumulative retry backoff budget in milliseconds (0 disables budget)"
    )]
    pub(crate) provider_retry_budget_ms: u64,

    #[arg(
        long,
        env = "TAU_PROVIDER_RETRY_JITTER",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Enable bounded jitter for provider retry backoff delays"
    )]
    pub(crate) provider_retry_jitter: bool,

    #[arg(
        long,
        env = "TAU_TURN_TIMEOUT_MS",
        default_value_t = 0,
        help = "Optional per-prompt timeout in milliseconds (0 disables timeout)"
    )]
    pub(crate) turn_timeout_ms: u64,

    #[arg(long, help = "Print agent lifecycle events as JSON")]
    pub(crate) json_events: bool,

    #[arg(
        long,
        env = "TAU_STREAM_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Render assistant text output token-by-token"
    )]
    pub(crate) stream_output: bool,

    #[arg(
        long,
        env = "TAU_STREAM_DELAY_MS",
        default_value_t = 0,
        help = "Delay between streamed output chunks in milliseconds"
    )]
    pub(crate) stream_delay_ms: u64,

    #[arg(long, help = "Run one prompt and exit")]
    pub(crate) prompt: Option<String>,

    #[arg(
        long = "orchestrator-mode",
        env = "TAU_ORCHESTRATOR_MODE",
        value_enum,
        default_value_t = CliOrchestratorMode::Off,
        help = "Optional orchestration mode for prompt execution"
    )]
    pub(crate) orchestrator_mode: CliOrchestratorMode,

    #[arg(
        long = "orchestrator-max-plan-steps",
        env = "TAU_ORCHESTRATOR_MAX_PLAN_STEPS",
        default_value_t = 8,
        help = "Maximum planner step count allowed in plan-first orchestrator mode"
    )]
    pub(crate) orchestrator_max_plan_steps: usize,

    #[arg(
        long = "orchestrator-max-delegated-steps",
        env = "TAU_ORCHESTRATOR_MAX_DELEGATED_STEPS",
        default_value_t = 8,
        value_parser = parse_positive_usize,
        help = "Maximum delegated step count allowed when --orchestrator-delegate-steps is enabled"
    )]
    pub(crate) orchestrator_max_delegated_steps: usize,

    #[arg(
        long = "orchestrator-max-executor-response-chars",
        env = "TAU_ORCHESTRATOR_MAX_EXECUTOR_RESPONSE_CHARS",
        default_value_t = 20000,
        value_parser = parse_positive_usize,
        help = "Maximum executor response length (characters) allowed in plan-first orchestrator mode"
    )]
    pub(crate) orchestrator_max_executor_response_chars: usize,

    #[arg(
        long = "orchestrator-max-delegated-step-response-chars",
        env = "TAU_ORCHESTRATOR_MAX_DELEGATED_STEP_RESPONSE_CHARS",
        default_value_t = 20000,
        value_parser = parse_positive_usize,
        help = "Maximum delegated step response length (characters) allowed when --orchestrator-delegate-steps is enabled"
    )]
    pub(crate) orchestrator_max_delegated_step_response_chars: usize,

    #[arg(
        long = "orchestrator-max-delegated-total-response-chars",
        env = "TAU_ORCHESTRATOR_MAX_DELEGATED_TOTAL_RESPONSE_CHARS",
        default_value_t = 160000,
        value_parser = parse_positive_usize,
        help = "Maximum cumulative delegated response length (characters) allowed when --orchestrator-delegate-steps is enabled"
    )]
    pub(crate) orchestrator_max_delegated_total_response_chars: usize,

    #[arg(
        long = "orchestrator-delegate-steps",
        env = "TAU_ORCHESTRATOR_DELEGATE_STEPS",
        default_value_t = false,
        help = "Enable delegated step execution and final consolidation in plan-first orchestrator mode"
    )]
    pub(crate) orchestrator_delegate_steps: bool,

    #[arg(
        long = "orchestrator-route-table",
        env = "TAU_ORCHESTRATOR_ROUTE_TABLE",
        value_name = "path",
        help = "Optional JSON route-table path for multi-agent planner/delegated/review role routing"
    )]
    pub(crate) orchestrator_route_table: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_PROMPT_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_template_file",
        help = "Read one prompt from a UTF-8 text file and exit"
    )]
    pub(crate) prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_PROMPT_TEMPLATE_FILE",
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
        env = "TAU_COMMAND_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_file",
        conflicts_with = "prompt_template_file",
        help = "Execute slash commands from a UTF-8 file and exit"
    )]
    pub(crate) command_file: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_COMMAND_FILE_ERROR_MODE",
        value_enum,
        default_value = "fail-fast",
        requires = "command_file",
        help = "Behavior when command-file execution hits malformed or failing commands"
    )]
    pub(crate) command_file_error_mode: CliCommandFileErrorMode,

    #[arg(
        long,
        env = "TAU_ONBOARD",
        default_value_t = false,
        help = "Run onboarding wizard and bootstrap Tau workspace assets, then exit"
    )]
    pub(crate) onboard: bool,

    #[arg(
        long = "onboard-non-interactive",
        env = "TAU_ONBOARD_NON_INTERACTIVE",
        default_value_t = false,
        requires = "onboard",
        help = "Disable interactive onboarding prompts and apply deterministic defaults"
    )]
    pub(crate) onboard_non_interactive: bool,

    #[arg(
        long = "onboard-profile",
        env = "TAU_ONBOARD_PROFILE",
        default_value = "default",
        requires = "onboard",
        help = "Profile name created/updated by onboarding"
    )]
    pub(crate) onboard_profile: String,

    #[arg(
        long = "onboard-release-channel",
        env = "TAU_ONBOARD_RELEASE_CHANNEL",
        requires = "onboard",
        help = "Optional release channel initialized by onboarding (stable|beta|dev)"
    )]
    pub(crate) onboard_release_channel: Option<String>,

    #[arg(
        long = "onboard-install-daemon",
        env = "TAU_ONBOARD_INSTALL_DAEMON",
        default_value_t = false,
        requires = "onboard",
        help = "Install Tau daemon profile files during onboarding using --daemon-profile and --daemon-state-dir"
    )]
    pub(crate) onboard_install_daemon: bool,

    #[arg(
        long = "onboard-start-daemon",
        env = "TAU_ONBOARD_START_DAEMON",
        default_value_t = false,
        requires = "onboard",
        requires = "onboard_install_daemon",
        help = "Start Tau daemon lifecycle state after onboarding daemon install"
    )]
    pub(crate) onboard_start_daemon: bool,

    #[arg(
        long = "doctor-release-cache-file",
        env = "TAU_DOCTOR_RELEASE_CACHE_FILE",
        default_value = ".tau/release-lookup-cache.json",
        help = "Cache file path used by /doctor --online release metadata lookup"
    )]
    pub(crate) doctor_release_cache_file: PathBuf,

    #[arg(
        long = "doctor-release-cache-ttl-ms",
        env = "TAU_DOCTOR_RELEASE_CACHE_TTL_MS",
        default_value_t = RELEASE_LOOKUP_CACHE_TTL_MS,
        value_parser = parse_positive_u64,
        help = "Cache freshness TTL in milliseconds for /doctor --online release metadata lookups"
    )]
    pub(crate) doctor_release_cache_ttl_ms: u64,

    #[arg(
        long = "project-index-build",
        env = "TAU_PROJECT_INDEX_BUILD",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        conflicts_with = "project_index_query",
        conflicts_with = "project_index_inspect",
        help = "Build or refresh the local project index under --project-index-state-dir and exit"
    )]
    pub(crate) project_index_build: bool,

    #[arg(
        long = "project-index-query",
        env = "TAU_PROJECT_INDEX_QUERY",
        conflicts_with = "project_index_build",
        conflicts_with = "project_index_inspect",
        value_name = "query",
        help = "Query the local project index for symbol/path/token matches and exit"
    )]
    pub(crate) project_index_query: Option<String>,

    #[arg(
        long = "project-index-inspect",
        env = "TAU_PROJECT_INDEX_INSPECT",
        conflicts_with = "project_index_build",
        conflicts_with = "project_index_query",
        help = "Inspect local project index metadata and exit"
    )]
    pub(crate) project_index_inspect: bool,

    #[arg(
        long = "project-index-json",
        env = "TAU_PROJECT_INDEX_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Emit project index build/query/inspect output as pretty JSON"
    )]
    pub(crate) project_index_json: bool,

    #[arg(
        long = "project-index-root",
        env = "TAU_PROJECT_INDEX_ROOT",
        default_value = ".",
        help = "Workspace root directory scanned by --project-index-build and resolved by query/inspect operations"
    )]
    pub(crate) project_index_root: PathBuf,

    #[arg(
        long = "project-index-state-dir",
        env = "TAU_PROJECT_INDEX_STATE_DIR",
        default_value = ".tau/index",
        help = "Directory containing project index state artifacts"
    )]
    pub(crate) project_index_state_dir: PathBuf,

    #[arg(
        long = "project-index-limit",
        env = "TAU_PROJECT_INDEX_LIMIT",
        default_value_t = 25,
        value_parser = parse_positive_usize,
        help = "Maximum number of query results returned by --project-index-query"
    )]
    pub(crate) project_index_limit: usize,

    #[arg(
        long = "channel-store-root",
        env = "TAU_CHANNEL_STORE_ROOT",
        default_value = ".tau/channel-store",
        help = "Base directory for transport-agnostic ChannelStore data"
    )]
    pub(crate) channel_store_root: PathBuf,

    #[arg(
        long = "channel-store-inspect",
        env = "TAU_CHANNEL_STORE_INSPECT",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        value_name = "transport/channel_id",
        help = "Inspect ChannelStore state for one channel and exit"
    )]
    pub(crate) channel_store_inspect: Option<String>,

    #[arg(
        long = "channel-store-repair",
        env = "TAU_CHANNEL_STORE_REPAIR",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        value_name = "transport/channel_id",
        help = "Repair malformed ChannelStore JSONL files for one channel and exit"
    )]
    pub(crate) channel_store_repair: Option<String>,

    #[arg(
        long = "transport-health-inspect",
        env = "TAU_TRANSPORT_HEALTH_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        value_name = "target",
        help = "Inspect transport health snapshot(s) and exit. Targets: slack, github, github:owner/repo, multi-channel, multi-agent, browser-automation, memory, dashboard, gateway, deployment, custom-command, voice"
    )]
    pub(crate) transport_health_inspect: Option<String>,

    #[arg(
        long = "transport-health-json",
        env = "TAU_TRANSPORT_HEALTH_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "transport_health_inspect",
        help = "Emit --transport-health-inspect output as pretty JSON"
    )]
    pub(crate) transport_health_json: bool,

    #[arg(
        long = "github-status-inspect",
        env = "TAU_GITHUB_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        conflicts_with = "gateway_service_start",
        conflicts_with = "gateway_service_stop",
        conflicts_with = "gateway_service_status",
        value_name = "owner/repo",
        help = "Inspect GitHub issues bridge state and event logs for one repository and exit"
    )]
    pub(crate) github_status_inspect: Option<String>,

    #[arg(
        long = "github-status-json",
        env = "TAU_GITHUB_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "github_status_inspect",
        help = "Emit --github-status-inspect output as pretty JSON"
    )]
    pub(crate) github_status_json: bool,

    #[arg(
        long = "dashboard-status-inspect",
        env = "TAU_DASHBOARD_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect dashboard runtime status/guardrail report and exit"
    )]
    pub(crate) dashboard_status_inspect: bool,

    #[arg(
        long = "dashboard-status-json",
        env = "TAU_DASHBOARD_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "dashboard_status_inspect",
        help = "Emit --dashboard-status-inspect output as pretty JSON"
    )]
    pub(crate) dashboard_status_json: bool,

    #[arg(
        long = "multi-channel-status-inspect",
        env = "TAU_MULTI_CHANNEL_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect multi-channel runtime status/guardrail report and exit"
    )]
    pub(crate) multi_channel_status_inspect: bool,

    #[arg(
        long = "multi-channel-status-json",
        env = "TAU_MULTI_CHANNEL_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_status_inspect",
        help = "Emit --multi-channel-status-inspect output as pretty JSON"
    )]
    pub(crate) multi_channel_status_json: bool,

    #[arg(
        long = "multi-channel-route-inspect-file",
        env = "TAU_MULTI_CHANNEL_ROUTE_INSPECT_FILE",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        value_name = "path",
        help = "Evaluate multi-channel route binding and multi-agent route-table selection for one event JSON file and exit"
    )]
    pub(crate) multi_channel_route_inspect_file: Option<PathBuf>,

    #[arg(
        long = "multi-channel-route-inspect-json",
        env = "TAU_MULTI_CHANNEL_ROUTE_INSPECT_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_route_inspect_file",
        help = "Emit --multi-channel-route-inspect-file output as pretty JSON"
    )]
    pub(crate) multi_channel_route_inspect_json: bool,

    #[arg(
        long = "multi-agent-status-inspect",
        env = "TAU_MULTI_AGENT_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect multi-agent runtime status/guardrail report and exit"
    )]
    pub(crate) multi_agent_status_inspect: bool,

    #[arg(
        long = "multi-agent-status-json",
        env = "TAU_MULTI_AGENT_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_agent_status_inspect",
        help = "Emit --multi-agent-status-inspect output as pretty JSON"
    )]
    pub(crate) multi_agent_status_json: bool,

    #[arg(
        long = "gateway-status-inspect",
        env = "TAU_GATEWAY_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect gateway runtime status/guardrail report and exit"
    )]
    pub(crate) gateway_status_inspect: bool,

    #[arg(
        long = "gateway-status-json",
        env = "TAU_GATEWAY_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "gateway_status_inspect",
        help = "Emit --gateway-status-inspect output as pretty JSON"
    )]
    pub(crate) gateway_status_json: bool,

    #[arg(
        long = "gateway-service-start",
        env = "TAU_GATEWAY_SERVICE_START",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        conflicts_with = "gateway_service_stop",
        conflicts_with = "gateway_service_status",
        help = "Start gateway service mode and persist lifecycle state"
    )]
    pub(crate) gateway_service_start: bool,

    #[arg(
        long = "gateway-service-stop",
        env = "TAU_GATEWAY_SERVICE_STOP",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        conflicts_with = "gateway_service_start",
        conflicts_with = "gateway_service_status",
        help = "Stop gateway service mode and persist lifecycle state"
    )]
    pub(crate) gateway_service_stop: bool,

    #[arg(
        long = "gateway-service-stop-reason",
        env = "TAU_GATEWAY_SERVICE_STOP_REASON",
        requires = "gateway_service_stop",
        help = "Optional reason code/message recorded with --gateway-service-stop"
    )]
    pub(crate) gateway_service_stop_reason: Option<String>,

    #[arg(
        long = "gateway-service-status",
        env = "TAU_GATEWAY_SERVICE_STATUS",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        conflicts_with = "gateway_service_start",
        conflicts_with = "gateway_service_stop",
        help = "Inspect gateway service lifecycle state and exit"
    )]
    pub(crate) gateway_service_status: bool,

    #[arg(
        long = "gateway-service-status-json",
        env = "TAU_GATEWAY_SERVICE_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "gateway_service_status",
        help = "Emit --gateway-service-status output as pretty JSON"
    )]
    pub(crate) gateway_service_status_json: bool,

    #[arg(
        long = "daemon-install",
        env = "TAU_DAEMON_INSTALL",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Install Tau daemon service profile files under --daemon-state-dir"
    )]
    pub(crate) daemon_install: bool,

    #[arg(
        long = "daemon-uninstall",
        env = "TAU_DAEMON_UNINSTALL",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Uninstall Tau daemon service profile files from --daemon-state-dir"
    )]
    pub(crate) daemon_uninstall: bool,

    #[arg(
        long = "daemon-start",
        env = "TAU_DAEMON_START",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Start Tau daemon lifecycle state and create pid metadata in --daemon-state-dir"
    )]
    pub(crate) daemon_start: bool,

    #[arg(
        long = "daemon-stop",
        env = "TAU_DAEMON_STOP",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_status",
        help = "Stop Tau daemon lifecycle state and clear pid metadata in --daemon-state-dir"
    )]
    pub(crate) daemon_stop: bool,

    #[arg(
        long = "daemon-stop-reason",
        env = "TAU_DAEMON_STOP_REASON",
        requires = "daemon_stop",
        help = "Optional reason code/message recorded with --daemon-stop"
    )]
    pub(crate) daemon_stop_reason: Option<String>,

    #[arg(
        long = "daemon-status",
        env = "TAU_DAEMON_STATUS",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        help = "Inspect Tau daemon lifecycle state and diagnostics"
    )]
    pub(crate) daemon_status: bool,

    #[arg(
        long = "daemon-status-json",
        env = "TAU_DAEMON_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "daemon_status",
        help = "Emit --daemon-status output as pretty JSON"
    )]
    pub(crate) daemon_status_json: bool,

    #[arg(
        long = "daemon-profile",
        env = "TAU_DAEMON_PROFILE",
        value_enum,
        default_value_t = CliDaemonProfile::Auto,
        help = "Daemon profile target: auto, launchd, or systemd-user"
    )]
    pub(crate) daemon_profile: CliDaemonProfile,

    #[arg(
        long = "daemon-state-dir",
        env = "TAU_DAEMON_STATE_DIR",
        default_value = ".tau/daemon",
        help = "Directory used for Tau daemon lifecycle state and generated service files"
    )]
    pub(crate) daemon_state_dir: PathBuf,

    #[arg(
        long = "deployment-status-inspect",
        env = "TAU_DEPLOYMENT_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect deployment runtime status/guardrail report and exit"
    )]
    pub(crate) deployment_status_inspect: bool,

    #[arg(
        long = "deployment-status-json",
        env = "TAU_DEPLOYMENT_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "deployment_status_inspect",
        help = "Emit --deployment-status-inspect output as pretty JSON"
    )]
    pub(crate) deployment_status_json: bool,

    #[arg(
        long = "custom-command-status-inspect",
        env = "TAU_CUSTOM_COMMAND_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect no-code custom command runtime status/guardrail report and exit"
    )]
    pub(crate) custom_command_status_inspect: bool,

    #[arg(
        long = "custom-command-status-json",
        env = "TAU_CUSTOM_COMMAND_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "custom_command_status_inspect",
        help = "Emit --custom-command-status-inspect output as pretty JSON"
    )]
    pub(crate) custom_command_status_json: bool,

    #[arg(
        long = "voice-status-inspect",
        env = "TAU_VOICE_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        help = "Inspect voice runtime status/guardrail report and exit"
    )]
    pub(crate) voice_status_inspect: bool,

    #[arg(
        long = "voice-status-json",
        env = "TAU_VOICE_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "voice_status_inspect",
        help = "Emit --voice-status-inspect output as pretty JSON"
    )]
    pub(crate) voice_status_json: bool,

    #[arg(
        long = "extension-exec-manifest",
        env = "TAU_EXTENSION_EXEC_MANIFEST",
        conflicts_with = "extension_validate",
        conflicts_with = "extension_list",
        conflicts_with = "extension_show",
        requires = "extension_exec_hook",
        requires = "extension_exec_payload_file",
        value_name = "path",
        help = "Execute one process-runtime extension hook from a manifest and exit"
    )]
    pub(crate) extension_exec_manifest: Option<PathBuf>,

    #[arg(
        long = "extension-exec-hook",
        env = "TAU_EXTENSION_EXEC_HOOK",
        requires = "extension_exec_manifest",
        value_name = "hook",
        help = "Hook name used by --extension-exec-manifest (for example run-start)"
    )]
    pub(crate) extension_exec_hook: Option<String>,

    #[arg(
        long = "extension-exec-payload-file",
        env = "TAU_EXTENSION_EXEC_PAYLOAD_FILE",
        requires = "extension_exec_manifest",
        value_name = "path",
        help = "JSON payload file for --extension-exec-manifest hook invocation"
    )]
    pub(crate) extension_exec_payload_file: Option<PathBuf>,

    #[arg(
        long = "extension-validate",
        env = "TAU_EXTENSION_VALIDATE",
        conflicts_with = "extension_exec_manifest",
        conflicts_with = "extension_list",
        conflicts_with = "extension_show",
        value_name = "path",
        help = "Validate an extension manifest JSON file and exit"
    )]
    pub(crate) extension_validate: Option<PathBuf>,

    #[arg(
        long = "extension-list",
        env = "TAU_EXTENSION_LIST",
        conflicts_with = "extension_exec_manifest",
        conflicts_with = "extension_validate",
        conflicts_with = "extension_show",
        help = "List discovered extension manifests from a root path and exit"
    )]
    pub(crate) extension_list: bool,

    #[arg(
        long = "extension-list-root",
        env = "TAU_EXTENSION_LIST_ROOT",
        default_value = ".tau/extensions",
        requires = "extension_list",
        value_name = "path",
        help = "Root directory scanned by --extension-list"
    )]
    pub(crate) extension_list_root: PathBuf,

    #[arg(
        long = "extension-show",
        env = "TAU_EXTENSION_SHOW",
        conflicts_with = "extension_exec_manifest",
        conflicts_with = "extension_list",
        conflicts_with = "extension_validate",
        value_name = "path",
        help = "Print extension manifest metadata and inventory"
    )]
    pub(crate) extension_show: Option<PathBuf>,

    #[arg(
        long = "extension-runtime-hooks",
        env = "TAU_EXTENSION_RUNTIME_HOOKS",
        default_value_t = false,
        help = "Enable runtime run-start/run-end extension hook dispatch for prompt turns"
    )]
    pub(crate) extension_runtime_hooks: bool,

    #[arg(
        long = "extension-runtime-root",
        env = "TAU_EXTENSION_RUNTIME_ROOT",
        default_value = ".tau/extensions",
        requires = "extension_runtime_hooks",
        value_name = "path",
        help = "Root directory scanned for runtime extension hooks when --extension-runtime-hooks is enabled"
    )]
    pub(crate) extension_runtime_root: PathBuf,

    #[arg(
        long = "package-validate",
        env = "TAU_PACKAGE_VALIDATE",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        value_name = "path",
        help = "Validate a package manifest JSON file and exit"
    )]
    pub(crate) package_validate: Option<PathBuf>,

    #[arg(
        long = "package-show",
        env = "TAU_PACKAGE_SHOW",
        conflicts_with = "package_validate",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        value_name = "path",
        help = "Print package manifest metadata and component inventory"
    )]
    pub(crate) package_show: Option<PathBuf>,

    #[arg(
        long = "package-install",
        env = "TAU_PACKAGE_INSTALL",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        value_name = "path",
        help = "Install a local package manifest bundle and exit"
    )]
    pub(crate) package_install: Option<PathBuf>,

    #[arg(
        long = "package-install-root",
        env = "TAU_PACKAGE_INSTALL_ROOT",
        default_value = ".tau/packages",
        requires = "package_install",
        value_name = "path",
        help = "Destination root for installed package bundles"
    )]
    pub(crate) package_install_root: PathBuf,

    #[arg(
        long = "package-update",
        env = "TAU_PACKAGE_UPDATE",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        value_name = "path",
        help = "Update an already installed package bundle from a manifest and exit"
    )]
    pub(crate) package_update: Option<PathBuf>,

    #[arg(
        long = "package-update-root",
        env = "TAU_PACKAGE_UPDATE_ROOT",
        default_value = ".tau/packages",
        requires = "package_update",
        value_name = "path",
        help = "Destination root containing installed package bundles for update"
    )]
    pub(crate) package_update_root: PathBuf,

    #[arg(
        long = "package-list",
        env = "TAU_PACKAGE_LIST",
        conflicts_with = "package_update",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        default_value_t = false,
        help = "List installed package bundles from a package root and exit"
    )]
    pub(crate) package_list: bool,

    #[arg(
        long = "package-list-root",
        env = "TAU_PACKAGE_LIST_ROOT",
        default_value = ".tau/packages",
        requires = "package_list",
        value_name = "path",
        help = "Source root to scan for installed package bundles"
    )]
    pub(crate) package_list_root: PathBuf,

    #[arg(
        long = "package-remove",
        env = "TAU_PACKAGE_REMOVE",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        value_name = "name@version",
        help = "Remove one installed package bundle by coordinate and exit"
    )]
    pub(crate) package_remove: Option<String>,

    #[arg(
        long = "package-remove-root",
        env = "TAU_PACKAGE_REMOVE_ROOT",
        default_value = ".tau/packages",
        requires = "package_remove",
        value_name = "path",
        help = "Source root containing installed package bundles for removal"
    )]
    pub(crate) package_remove_root: PathBuf,

    #[arg(
        long = "package-rollback",
        env = "TAU_PACKAGE_ROLLBACK",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_conflicts",
        conflicts_with = "package_activate",
        value_name = "name@version",
        help = "Rollback one package to a target installed version and remove sibling versions"
    )]
    pub(crate) package_rollback: Option<String>,

    #[arg(
        long = "package-rollback-root",
        env = "TAU_PACKAGE_ROLLBACK_ROOT",
        default_value = ".tau/packages",
        requires = "package_rollback",
        value_name = "path",
        help = "Source root containing installed package versions for rollback"
    )]
    pub(crate) package_rollback_root: PathBuf,

    #[arg(
        long = "package-conflicts",
        env = "TAU_PACKAGE_CONFLICTS",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_activate",
        default_value_t = false,
        help = "Audit installed package component path conflicts and exit"
    )]
    pub(crate) package_conflicts: bool,

    #[arg(
        long = "package-conflicts-root",
        env = "TAU_PACKAGE_CONFLICTS_ROOT",
        default_value = ".tau/packages",
        requires = "package_conflicts",
        value_name = "path",
        help = "Source root containing installed package bundles for conflict audit"
    )]
    pub(crate) package_conflicts_root: PathBuf,

    #[arg(
        long = "package-activate",
        env = "TAU_PACKAGE_ACTIVATE",
        conflicts_with = "package_activate_on_startup",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        default_value_t = false,
        help = "Materialize installed package components into an activation destination and exit"
    )]
    pub(crate) package_activate: bool,

    #[arg(
        long = "package-activate-on-startup",
        env = "TAU_PACKAGE_ACTIVATE_ON_STARTUP",
        conflicts_with = "package_activate",
        conflicts_with = "package_validate",
        conflicts_with = "package_show",
        conflicts_with = "package_install",
        conflicts_with = "package_update",
        conflicts_with = "package_list",
        conflicts_with = "package_remove",
        conflicts_with = "package_rollback",
        conflicts_with = "package_conflicts",
        default_value_t = false,
        help = "Activate installed package components during startup before runtime execution"
    )]
    pub(crate) package_activate_on_startup: bool,

    #[arg(
        long = "package-activate-root",
        env = "TAU_PACKAGE_ACTIVATE_ROOT",
        default_value = ".tau/packages",
        value_name = "path",
        help = "Source root containing installed package bundles for activation"
    )]
    pub(crate) package_activate_root: PathBuf,

    #[arg(
        long = "package-activate-destination",
        env = "TAU_PACKAGE_ACTIVATE_DESTINATION",
        default_value = ".tau/packages-active",
        value_name = "path",
        help = "Destination root where resolved package components are materialized"
    )]
    pub(crate) package_activate_destination: PathBuf,

    #[arg(
        long = "package-activate-conflict-policy",
        env = "TAU_PACKAGE_ACTIVATE_CONFLICT_POLICY",
        default_value = "error",
        value_name = "error|keep-first|keep-last",
        help = "Conflict strategy when multiple packages contain the same kind/path component"
    )]
    pub(crate) package_activate_conflict_policy: String,

    #[arg(
        long = "qa-loop",
        env = "TAU_QA_LOOP",
        default_value_t = false,
        help = "Run staged quality pipeline (fmt/lint/test by default) and exit"
    )]
    pub(crate) qa_loop: bool,

    #[arg(
        long = "qa-loop-config",
        env = "TAU_QA_LOOP_CONFIG",
        requires = "qa_loop",
        value_name = "path",
        help = "Optional JSON pipeline config file for --qa-loop"
    )]
    pub(crate) qa_loop_config: Option<PathBuf>,

    #[arg(
        long = "qa-loop-json",
        env = "TAU_QA_LOOP_JSON",
        requires = "qa_loop",
        default_value_t = false,
        help = "Emit qa-loop report as JSON"
    )]
    pub(crate) qa_loop_json: bool,

    #[arg(
        long = "qa-loop-stage-timeout-ms",
        env = "TAU_QA_LOOP_STAGE_TIMEOUT_MS",
        requires = "qa_loop",
        value_parser = parse_positive_u64,
        help = "Override per-stage timeout for --qa-loop in milliseconds"
    )]
    pub(crate) qa_loop_stage_timeout_ms: Option<u64>,

    #[arg(
        long = "qa-loop-retry-failures",
        env = "TAU_QA_LOOP_RETRY_FAILURES",
        requires = "qa_loop",
        help = "Override retry count for failed stages in --qa-loop"
    )]
    pub(crate) qa_loop_retry_failures: Option<usize>,

    #[arg(
        long = "qa-loop-max-output-bytes",
        env = "TAU_QA_LOOP_MAX_OUTPUT_BYTES",
        requires = "qa_loop",
        value_parser = parse_positive_usize,
        help = "Override bounded stdout/stderr bytes captured per stage in --qa-loop reports"
    )]
    pub(crate) qa_loop_max_output_bytes: Option<usize>,

    #[arg(
        long = "qa-loop-changed-file-limit",
        env = "TAU_QA_LOOP_CHANGED_FILE_LIMIT",
        requires = "qa_loop",
        value_parser = parse_positive_usize,
        help = "Override maximum changed files included in --qa-loop git summary"
    )]
    pub(crate) qa_loop_changed_file_limit: Option<usize>,

    #[arg(
        long = "mcp-server",
        env = "TAU_MCP_SERVER",
        default_value_t = false,
        conflicts_with = "rpc_capabilities",
        conflicts_with = "rpc_validate_frame_file",
        conflicts_with = "rpc_dispatch_frame_file",
        conflicts_with = "rpc_dispatch_ndjson_file",
        conflicts_with = "rpc_serve_ndjson",
        conflicts_with = "github_issues_bridge",
        conflicts_with = "slack_bridge",
        conflicts_with = "events_runner",
        help = "Run MCP server mode over stdin/stdout using JSON-RPC framing"
    )]
    pub(crate) mcp_server: bool,

    #[arg(
        long = "mcp-external-server-config",
        env = "TAU_MCP_EXTERNAL_SERVER_CONFIG",
        value_name = "path",
        requires = "mcp_server",
        help = "Optional external MCP server config JSON used for discovery and tool forwarding"
    )]
    pub(crate) mcp_external_server_config: Option<PathBuf>,

    #[arg(
        long = "mcp-context-provider",
        env = "TAU_MCP_CONTEXT_PROVIDER",
        value_name = "name",
        requires = "mcp_server",
        action = ArgAction::Append,
        help = "Enable MCP context providers (repeatable): session, skills, channel-store"
    )]
    pub(crate) mcp_context_provider: Vec<String>,

    #[arg(
        long = "rpc-capabilities",
        env = "TAU_RPC_CAPABILITIES",
        default_value_t = false,
        help = "Print versioned RPC protocol capabilities JSON and exit"
    )]
    pub(crate) rpc_capabilities: bool,

    #[arg(
        long = "rpc-validate-frame-file",
        env = "TAU_RPC_VALIDATE_FRAME_FILE",
        value_name = "path",
        help = "Validate one RPC frame JSON file and exit"
    )]
    pub(crate) rpc_validate_frame_file: Option<PathBuf>,

    #[arg(
        long = "rpc-dispatch-frame-file",
        env = "TAU_RPC_DISPATCH_FRAME_FILE",
        value_name = "path",
        help = "Dispatch one RPC request frame JSON file and print a response frame JSON"
    )]
    pub(crate) rpc_dispatch_frame_file: Option<PathBuf>,

    #[arg(
        long = "rpc-dispatch-ndjson-file",
        env = "TAU_RPC_DISPATCH_NDJSON_FILE",
        value_name = "path",
        help = "Dispatch newline-delimited RPC request frames and print one response JSON line per frame"
    )]
    pub(crate) rpc_dispatch_ndjson_file: Option<PathBuf>,

    #[arg(
        long = "rpc-serve-ndjson",
        env = "TAU_RPC_SERVE_NDJSON",
        default_value_t = false,
        conflicts_with = "rpc_capabilities",
        conflicts_with = "rpc_validate_frame_file",
        conflicts_with = "rpc_dispatch_frame_file",
        conflicts_with = "rpc_dispatch_ndjson_file",
        help = "Run long-lived RPC NDJSON server mode over stdin/stdout"
    )]
    pub(crate) rpc_serve_ndjson: bool,

    #[arg(
        long = "events-inspect",
        env = "TAU_EVENTS_INSPECT",
        default_value_t = false,
        conflicts_with = "events_validate",
        conflicts_with = "events_simulate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Inspect scheduled events state and due/queue diagnostics, then exit"
    )]
    pub(crate) events_inspect: bool,

    #[arg(
        long = "events-inspect-json",
        env = "TAU_EVENTS_INSPECT_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_inspect",
        help = "Emit --events-inspect output as pretty JSON"
    )]
    pub(crate) events_inspect_json: bool,

    #[arg(
        long = "events-validate",
        env = "TAU_EVENTS_VALIDATE",
        default_value_t = false,
        conflicts_with = "events_inspect",
        conflicts_with = "events_simulate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Validate scheduled event definition files and exit non-zero on invalid entries"
    )]
    pub(crate) events_validate: bool,

    #[arg(
        long = "events-validate-json",
        env = "TAU_EVENTS_VALIDATE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_validate",
        help = "Emit --events-validate output as pretty JSON"
    )]
    pub(crate) events_validate_json: bool,

    #[arg(
        long = "events-simulate",
        env = "TAU_EVENTS_SIMULATE",
        default_value_t = false,
        conflicts_with = "events_inspect",
        conflicts_with = "events_validate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Simulate next event due timings and horizon posture, then exit"
    )]
    pub(crate) events_simulate: bool,

    #[arg(
        long = "events-simulate-json",
        env = "TAU_EVENTS_SIMULATE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_simulate",
        help = "Emit --events-simulate output as pretty JSON"
    )]
    pub(crate) events_simulate_json: bool,

    #[arg(
        long = "events-simulate-horizon-seconds",
        env = "TAU_EVENTS_SIMULATE_HORIZON_SECONDS",
        default_value_t = 3_600,
        requires = "events_simulate",
        help = "Horizon window used to classify event next-due timing"
    )]
    pub(crate) events_simulate_horizon_seconds: u64,

    #[arg(
        long = "events-dry-run",
        env = "TAU_EVENTS_DRY_RUN",
        default_value_t = false,
        conflicts_with = "events_inspect",
        conflicts_with = "events_validate",
        conflicts_with = "events_simulate",
        conflicts_with = "events_template_write",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Preview which events would execute now without mutating state or files"
    )]
    pub(crate) events_dry_run: bool,

    #[arg(
        long = "events-dry-run-json",
        env = "TAU_EVENTS_DRY_RUN_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_dry_run",
        help = "Emit --events-dry-run output as pretty JSON"
    )]
    pub(crate) events_dry_run_json: bool,

    #[arg(
        long = "events-dry-run-strict",
        env = "TAU_EVENTS_DRY_RUN_STRICT",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_dry_run",
        help = "Exit non-zero when --events-dry-run reports malformed or invalid definitions"
    )]
    pub(crate) events_dry_run_strict: bool,

    #[arg(
        long = "events-dry-run-max-error-rows",
        env = "TAU_EVENTS_DRY_RUN_MAX_ERROR_ROWS",
        requires = "events_dry_run",
        value_name = "count",
        help = "Fail dry-run when error row count exceeds this threshold"
    )]
    pub(crate) events_dry_run_max_error_rows: Option<u64>,

    #[arg(
        long = "events-dry-run-max-execute-rows",
        env = "TAU_EVENTS_DRY_RUN_MAX_EXECUTE_ROWS",
        requires = "events_dry_run",
        value_name = "count",
        help = "Fail dry-run when execute row count exceeds this threshold"
    )]
    pub(crate) events_dry_run_max_execute_rows: Option<u64>,

    #[arg(
        long = "events-template-write",
        env = "TAU_EVENTS_TEMPLATE_WRITE",
        value_name = "PATH",
        conflicts_with = "events_inspect",
        conflicts_with = "events_validate",
        conflicts_with = "events_simulate",
        conflicts_with = "events_dry_run",
        conflicts_with = "events_runner",
        conflicts_with = "event_webhook_ingest_file",
        help = "Write a schedule-specific event template JSON file and exit"
    )]
    pub(crate) events_template_write: Option<PathBuf>,

    #[arg(
        long = "events-template-schedule",
        env = "TAU_EVENTS_TEMPLATE_SCHEDULE",
        value_enum,
        default_value_t = CliEventTemplateSchedule::Immediate,
        requires = "events_template_write",
        help = "Schedule variant for --events-template-write: immediate, at, periodic"
    )]
    pub(crate) events_template_schedule: CliEventTemplateSchedule,

    #[arg(
        long = "events-template-overwrite",
        env = "TAU_EVENTS_TEMPLATE_OVERWRITE",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "events_template_write",
        help = "Allow overwriting an existing template file path"
    )]
    pub(crate) events_template_overwrite: bool,

    #[arg(
        long = "events-template-id",
        env = "TAU_EVENTS_TEMPLATE_ID",
        requires = "events_template_write",
        help = "Optional event id override for generated template"
    )]
    pub(crate) events_template_id: Option<String>,

    #[arg(
        long = "events-template-channel",
        env = "TAU_EVENTS_TEMPLATE_CHANNEL",
        requires = "events_template_write",
        value_name = "transport/channel_id",
        help = "Optional channel ref override for generated template"
    )]
    pub(crate) events_template_channel: Option<String>,

    #[arg(
        long = "events-template-prompt",
        env = "TAU_EVENTS_TEMPLATE_PROMPT",
        requires = "events_template_write",
        help = "Optional prompt override for generated template"
    )]
    pub(crate) events_template_prompt: Option<String>,

    #[arg(
        long = "events-template-at-unix-ms",
        env = "TAU_EVENTS_TEMPLATE_AT_UNIX_MS",
        requires = "events_template_write",
        help = "Optional unix timestamp (ms) used for --events-template-schedule at"
    )]
    pub(crate) events_template_at_unix_ms: Option<u64>,

    #[arg(
        long = "events-template-cron",
        env = "TAU_EVENTS_TEMPLATE_CRON",
        requires = "events_template_write",
        help = "Optional cron override used for --events-template-schedule periodic"
    )]
    pub(crate) events_template_cron: Option<String>,

    #[arg(
        long = "events-template-timezone",
        env = "TAU_EVENTS_TEMPLATE_TIMEZONE",
        default_value = "UTC",
        requires = "events_template_write",
        help = "Timezone used for --events-template-schedule periodic"
    )]
    pub(crate) events_template_timezone: String,

    #[arg(
        long = "events-runner",
        env = "TAU_EVENTS_RUNNER",
        default_value_t = false,
        help = "Run filesystem-backed scheduled events worker"
    )]
    pub(crate) events_runner: bool,

    #[arg(
        long = "events-dir",
        env = "TAU_EVENTS_DIR",
        default_value = ".tau/events",
        help = "Directory containing event definition JSON files"
    )]
    pub(crate) events_dir: PathBuf,

    #[arg(
        long = "events-state-path",
        env = "TAU_EVENTS_STATE_PATH",
        default_value = ".tau/events/state.json",
        help = "Persistent scheduler state path for periodic/debounce tracking"
    )]
    pub(crate) events_state_path: PathBuf,

    #[arg(
        long = "events-poll-interval-ms",
        env = "TAU_EVENTS_POLL_INTERVAL_MS",
        default_value_t = 1_000,
        requires = "events_runner",
        help = "Scheduler poll interval in milliseconds"
    )]
    pub(crate) events_poll_interval_ms: u64,

    #[arg(
        long = "events-queue-limit",
        env = "TAU_EVENTS_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "events_runner",
        help = "Maximum due events executed per poll cycle"
    )]
    pub(crate) events_queue_limit: usize,

    #[arg(
        long = "events-stale-immediate-max-age-seconds",
        env = "TAU_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS",
        default_value_t = 86_400,
        requires = "events_runner",
        help = "Maximum age for immediate events before they are skipped and removed (0 disables)"
    )]
    pub(crate) events_stale_immediate_max_age_seconds: u64,

    #[arg(
        long = "event-webhook-ingest-file",
        env = "TAU_EVENT_WEBHOOK_INGEST_FILE",
        value_name = "PATH",
        conflicts_with = "events_runner",
        help = "One-shot webhook ingestion: read payload file, enqueue debounced immediate event, and exit"
    )]
    pub(crate) event_webhook_ingest_file: Option<PathBuf>,

    #[arg(
        long = "event-webhook-channel",
        env = "TAU_EVENT_WEBHOOK_CHANNEL",
        requires = "event_webhook_ingest_file",
        value_name = "transport/channel_id",
        help = "Channel reference used for webhook-ingested immediate events"
    )]
    pub(crate) event_webhook_channel: Option<String>,

    #[arg(
        long = "event-webhook-actor-id",
        env = "TAU_EVENT_WEBHOOK_ACTOR_ID",
        requires = "event_webhook_ingest_file",
        value_name = "id",
        help = "Optional actor id/login used by pairing policy checks before webhook ingest"
    )]
    pub(crate) event_webhook_actor_id: Option<String>,

    #[arg(
        long = "event-webhook-prompt-prefix",
        env = "TAU_EVENT_WEBHOOK_PROMPT_PREFIX",
        default_value = "Handle webhook-triggered event.",
        requires = "event_webhook_ingest_file",
        help = "Prompt prefix prepended before webhook payload content"
    )]
    pub(crate) event_webhook_prompt_prefix: String,

    #[arg(
        long = "event-webhook-debounce-key",
        env = "TAU_EVENT_WEBHOOK_DEBOUNCE_KEY",
        requires = "event_webhook_ingest_file",
        help = "Optional debounce key shared across webhook ingestions"
    )]
    pub(crate) event_webhook_debounce_key: Option<String>,

    #[arg(
        long = "event-webhook-debounce-window-seconds",
        env = "TAU_EVENT_WEBHOOK_DEBOUNCE_WINDOW_SECONDS",
        default_value_t = 60,
        requires = "event_webhook_ingest_file",
        help = "Debounce window in seconds for repeated webhook ingestions with same key"
    )]
    pub(crate) event_webhook_debounce_window_seconds: u64,

    #[arg(
        long = "event-webhook-signature",
        env = "TAU_EVENT_WEBHOOK_SIGNATURE",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Raw webhook signature header value (for signed ingest verification)"
    )]
    pub(crate) event_webhook_signature: Option<String>,

    #[arg(
        long = "event-webhook-timestamp",
        env = "TAU_EVENT_WEBHOOK_TIMESTAMP",
        requires = "event_webhook_ingest_file",
        help = "Webhook timestamp header value used by signature algorithms that require timestamp checks"
    )]
    pub(crate) event_webhook_timestamp: Option<String>,

    #[arg(
        long = "event-webhook-secret",
        env = "TAU_EVENT_WEBHOOK_SECRET",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Shared secret used to verify signed webhook payloads"
    )]
    pub(crate) event_webhook_secret: Option<String>,

    #[arg(
        long = "event-webhook-secret-id",
        env = "TAU_EVENT_WEBHOOK_SECRET_ID",
        requires = "event_webhook_ingest_file",
        help = "Credential-store integration id used to resolve webhook signing secret"
    )]
    pub(crate) event_webhook_secret_id: Option<String>,

    #[arg(
        long = "event-webhook-signature-algorithm",
        env = "TAU_EVENT_WEBHOOK_SIGNATURE_ALGORITHM",
        value_enum,
        requires = "event_webhook_ingest_file",
        help = "Webhook signature algorithm (github-sha256, slack-v0)"
    )]
    pub(crate) event_webhook_signature_algorithm: Option<CliWebhookSignatureAlgorithm>,

    #[arg(
        long = "event-webhook-signature-max-skew-seconds",
        env = "TAU_EVENT_WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS",
        default_value_t = 300,
        requires = "event_webhook_ingest_file",
        help = "Maximum allowed webhook timestamp skew in seconds (0 disables skew checks)"
    )]
    pub(crate) event_webhook_signature_max_skew_seconds: u64,

    #[arg(
        long = "multi-channel-contract-runner",
        env = "TAU_MULTI_CHANNEL_CONTRACT_RUNNER",
        default_value_t = false,
        conflicts_with = "multi_channel_live_runner",
        help = "Run fixture-driven multi-channel runtime for Telegram/Discord/WhatsApp contracts"
    )]
    pub(crate) multi_channel_contract_runner: bool,

    #[arg(
        long = "multi-channel-live-runner",
        env = "TAU_MULTI_CHANNEL_LIVE_RUNNER",
        default_value_t = false,
        conflicts_with = "multi_channel_contract_runner",
        help = "Run live-ingress multi-channel runtime using local adapter inbox files for Telegram/Discord/WhatsApp"
    )]
    pub(crate) multi_channel_live_runner: bool,

    #[arg(
        long = "multi-channel-live-connectors-runner",
        env = "TAU_MULTI_CHANNEL_LIVE_CONNECTORS_RUNNER",
        default_value_t = false,
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        help = "Run live ingress connectors for Telegram/Discord/WhatsApp (polling and/or webhook bridges)"
    )]
    pub(crate) multi_channel_live_connectors_runner: bool,

    #[arg(
        long = "multi-channel-live-connectors-status",
        env = "TAU_MULTI_CHANNEL_LIVE_CONNECTORS_STATUS",
        default_value_t = false,
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_connectors_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        help = "Inspect persisted live connector liveness/error counters and exit"
    )]
    pub(crate) multi_channel_live_connectors_status: bool,

    #[arg(
        long = "multi-channel-live-connectors-status-json",
        env = "TAU_MULTI_CHANNEL_LIVE_CONNECTORS_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_live_connectors_status",
        help = "Emit --multi-channel-live-connectors-status output as pretty JSON"
    )]
    pub(crate) multi_channel_live_connectors_status_json: bool,

    #[arg(
        long = "multi-channel-live-connectors-state-path",
        env = "TAU_MULTI_CHANNEL_LIVE_CONNECTORS_STATE_PATH",
        default_value = ".tau/multi-channel/live-connectors-state.json",
        help = "Path to live connector state/counter snapshot used by runner and status inspect"
    )]
    pub(crate) multi_channel_live_connectors_state_path: PathBuf,

    #[arg(
        long = "multi-channel-live-connectors-poll-once",
        env = "TAU_MULTI_CHANNEL_LIVE_CONNECTORS_POLL_ONCE",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_live_connectors_runner",
        help = "Run one polling connector cycle and exit (cannot be combined with webhook connector modes)"
    )]
    pub(crate) multi_channel_live_connectors_poll_once: bool,

    #[arg(
        long = "multi-channel-live-webhook-bind",
        env = "TAU_MULTI_CHANNEL_LIVE_WEBHOOK_BIND",
        default_value = "127.0.0.1:8788",
        requires = "multi_channel_live_connectors_runner",
        help = "Bind address for live connector webhook server when webhook connector modes are enabled"
    )]
    pub(crate) multi_channel_live_webhook_bind: String,

    #[arg(
        long = "multi-channel-telegram-ingress-mode",
        env = "TAU_MULTI_CHANNEL_TELEGRAM_INGRESS_MODE",
        value_enum,
        default_value_t = CliMultiChannelLiveConnectorMode::Disabled,
        requires = "multi_channel_live_connectors_runner",
        help = "Telegram connector mode for live connectors runner (disabled, polling, webhook)"
    )]
    pub(crate) multi_channel_telegram_ingress_mode: CliMultiChannelLiveConnectorMode,

    #[arg(
        long = "multi-channel-discord-ingress-mode",
        env = "TAU_MULTI_CHANNEL_DISCORD_INGRESS_MODE",
        value_enum,
        default_value_t = CliMultiChannelLiveConnectorMode::Disabled,
        requires = "multi_channel_live_connectors_runner",
        help = "Discord connector mode for live connectors runner (disabled, polling)"
    )]
    pub(crate) multi_channel_discord_ingress_mode: CliMultiChannelLiveConnectorMode,

    #[arg(
        long = "multi-channel-whatsapp-ingress-mode",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_INGRESS_MODE",
        value_enum,
        default_value_t = CliMultiChannelLiveConnectorMode::Disabled,
        requires = "multi_channel_live_connectors_runner",
        help = "WhatsApp connector mode for live connectors runner (disabled, webhook)"
    )]
    pub(crate) multi_channel_whatsapp_ingress_mode: CliMultiChannelLiveConnectorMode,

    #[arg(
        long = "multi-channel-discord-ingress-channel-id",
        env = "TAU_MULTI_CHANNEL_DISCORD_INGRESS_CHANNEL_ID",
        value_delimiter = ',',
        requires = "multi_channel_live_connectors_runner",
        help = "Discord channel ids polled when --multi-channel-discord-ingress-mode=polling (repeatable)"
    )]
    pub(crate) multi_channel_discord_ingress_channel_ids: Vec<String>,

    #[arg(
        long = "multi-channel-telegram-webhook-secret",
        env = "TAU_MULTI_CHANNEL_TELEGRAM_WEBHOOK_SECRET",
        hide_env_values = true,
        requires = "multi_channel_live_connectors_runner",
        help = "Optional Telegram webhook secret token required in X-Telegram-Bot-Api-Secret-Token"
    )]
    pub(crate) multi_channel_telegram_webhook_secret: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-webhook-verify-token",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_WEBHOOK_VERIFY_TOKEN",
        hide_env_values = true,
        requires = "multi_channel_live_connectors_runner",
        help = "Verify token used for WhatsApp webhook subscription challenge"
    )]
    pub(crate) multi_channel_whatsapp_webhook_verify_token: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-webhook-app-secret",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_WEBHOOK_APP_SECRET",
        hide_env_values = true,
        requires = "multi_channel_live_connectors_runner",
        help = "App secret used to verify X-Hub-Signature-256 for WhatsApp webhook posts"
    )]
    pub(crate) multi_channel_whatsapp_webhook_app_secret: Option<String>,

    #[arg(
        long = "multi-channel-live-ingest-file",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_FILE",
        help = "One-shot provider payload ingestion: normalize a Telegram/Discord/WhatsApp payload file into live-ingress NDJSON and exit"
    )]
    pub(crate) multi_channel_live_ingest_file: Option<PathBuf>,

    #[arg(
        long = "multi-channel-live-ingest-transport",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_TRANSPORT",
        value_enum,
        requires = "multi_channel_live_ingest_file",
        help = "Transport for --multi-channel-live-ingest-file (telegram, discord, whatsapp)"
    )]
    pub(crate) multi_channel_live_ingest_transport: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-live-ingest-provider",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_PROVIDER",
        default_value = "native-ingress",
        requires = "multi_channel_live_ingest_file",
        help = "Provider identifier recorded in normalized live-ingress envelopes"
    )]
    pub(crate) multi_channel_live_ingest_provider: String,

    #[arg(
        long = "multi-channel-live-ingest-dir",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_DIR",
        default_value = ".tau/multi-channel/live-ingress",
        requires = "multi_channel_live_ingest_file",
        help = "Directory where one-shot live-ingest writes transport-specific NDJSON inbox files"
    )]
    pub(crate) multi_channel_live_ingest_dir: PathBuf,

    #[arg(
        long = "multi-channel-live-readiness-preflight",
        env = "TAU_MULTI_CHANNEL_LIVE_READINESS_PREFLIGHT",
        default_value_t = false,
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        help = "Run multi-channel live readiness preflight checks for Telegram/Discord/WhatsApp and exit"
    )]
    pub(crate) multi_channel_live_readiness_preflight: bool,

    #[arg(
        long = "multi-channel-live-readiness-json",
        env = "TAU_MULTI_CHANNEL_LIVE_READINESS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_live_readiness_preflight",
        help = "Emit --multi-channel-live-readiness-preflight output as pretty JSON"
    )]
    pub(crate) multi_channel_live_readiness_json: bool,

    #[arg(
        long = "multi-channel-channel-status",
        env = "TAU_MULTI_CHANNEL_CHANNEL_STATUS",
        value_enum,
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        conflicts_with = "multi_channel_channel_login",
        conflicts_with = "multi_channel_channel_logout",
        conflicts_with = "multi_channel_channel_probe",
        help = "Inspect channel lifecycle/readiness status for one transport (telegram, discord, whatsapp) and exit"
    )]
    pub(crate) multi_channel_channel_status: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-channel-status-json",
        env = "TAU_MULTI_CHANNEL_CHANNEL_STATUS_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_channel_status",
        help = "Emit --multi-channel-channel-status output as pretty JSON"
    )]
    pub(crate) multi_channel_channel_status_json: bool,

    #[arg(
        long = "multi-channel-channel-login",
        env = "TAU_MULTI_CHANNEL_CHANNEL_LOGIN",
        value_enum,
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        conflicts_with = "multi_channel_channel_status",
        conflicts_with = "multi_channel_channel_logout",
        conflicts_with = "multi_channel_channel_probe",
        help = "Initialize one channel lifecycle entry and ingress path for one transport (telegram, discord, whatsapp)"
    )]
    pub(crate) multi_channel_channel_login: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-channel-login-json",
        env = "TAU_MULTI_CHANNEL_CHANNEL_LOGIN_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_channel_login",
        help = "Emit --multi-channel-channel-login output as pretty JSON"
    )]
    pub(crate) multi_channel_channel_login_json: bool,

    #[arg(
        long = "multi-channel-channel-logout",
        env = "TAU_MULTI_CHANNEL_CHANNEL_LOGOUT",
        value_enum,
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        conflicts_with = "multi_channel_channel_status",
        conflicts_with = "multi_channel_channel_login",
        conflicts_with = "multi_channel_channel_probe",
        help = "Mark one channel lifecycle entry logged_out for one transport (telegram, discord, whatsapp)"
    )]
    pub(crate) multi_channel_channel_logout: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-channel-logout-json",
        env = "TAU_MULTI_CHANNEL_CHANNEL_LOGOUT_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_channel_logout",
        help = "Emit --multi-channel-channel-logout output as pretty JSON"
    )]
    pub(crate) multi_channel_channel_logout_json: bool,

    #[arg(
        long = "multi-channel-channel-probe",
        env = "TAU_MULTI_CHANNEL_CHANNEL_PROBE",
        value_enum,
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        conflicts_with = "multi_channel_channel_status",
        conflicts_with = "multi_channel_channel_login",
        conflicts_with = "multi_channel_channel_logout",
        help = "Run readiness probe for one transport (telegram, discord, whatsapp) and persist lifecycle probe state"
    )]
    pub(crate) multi_channel_channel_probe: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-channel-probe-json",
        env = "TAU_MULTI_CHANNEL_CHANNEL_PROBE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_channel_probe",
        help = "Emit --multi-channel-channel-probe output as pretty JSON"
    )]
    pub(crate) multi_channel_channel_probe_json: bool,

    #[arg(
        long = "multi-channel-fixture",
        env = "TAU_MULTI_CHANNEL_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/multi-channel-contract/baseline-three-channel.json",
        requires = "multi_channel_contract_runner",
        help = "Path to multi-channel contract fixture JSON"
    )]
    pub(crate) multi_channel_fixture: PathBuf,

    #[arg(
        long = "multi-channel-live-ingress-dir",
        env = "TAU_MULTI_CHANNEL_LIVE_INGRESS_DIR",
        default_value = ".tau/multi-channel/live-ingress",
        requires = "multi_channel_live_runner",
        help = "Directory containing transport-specific live ingress NDJSON inbox files (telegram.ndjson, discord.ndjson, whatsapp.ndjson)"
    )]
    pub(crate) multi_channel_live_ingress_dir: PathBuf,

    #[arg(
        long = "multi-channel-state-dir",
        env = "TAU_MULTI_CHANNEL_STATE_DIR",
        default_value = ".tau/multi-channel",
        help = "Directory for multi-channel runtime state and channel-store outputs"
    )]
    pub(crate) multi_channel_state_dir: PathBuf,

    #[arg(
        long = "multi-channel-queue-limit",
        env = "TAU_MULTI_CHANNEL_QUEUE_LIMIT",
        default_value_t = 64,
        help = "Maximum inbound events processed per runtime cycle"
    )]
    pub(crate) multi_channel_queue_limit: usize,

    #[arg(
        long = "multi-channel-processed-event-cap",
        env = "TAU_MULTI_CHANNEL_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        help = "Maximum processed-event keys retained for duplicate suppression"
    )]
    pub(crate) multi_channel_processed_event_cap: usize,

    #[arg(
        long = "multi-channel-retry-max-attempts",
        env = "TAU_MULTI_CHANNEL_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        help = "Maximum retry attempts for transient multi-channel runtime failures"
    )]
    pub(crate) multi_channel_retry_max_attempts: usize,

    #[arg(
        long = "multi-channel-retry-base-delay-ms",
        env = "TAU_MULTI_CHANNEL_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        help = "Base backoff delay in milliseconds for multi-channel runtime retries (0 disables delay)"
    )]
    pub(crate) multi_channel_retry_base_delay_ms: u64,

    #[arg(
        long = "multi-channel-retry-jitter-ms",
        env = "TAU_MULTI_CHANNEL_RETRY_JITTER_MS",
        default_value_t = 0,
        help = "Deterministic jitter upper-bound in milliseconds added to multi-channel runtime retry delays (0 disables jitter)"
    )]
    pub(crate) multi_channel_retry_jitter_ms: u64,

    #[arg(
        long = "multi-channel-telemetry-typing-presence",
        env = "TAU_MULTI_CHANNEL_TELEMETRY_TYPING_PRESENCE",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Emit typing/presence lifecycle telemetry for long-running multi-channel replies"
    )]
    pub(crate) multi_channel_telemetry_typing_presence: bool,

    #[arg(
        long = "multi-channel-telemetry-usage-summary",
        env = "TAU_MULTI_CHANNEL_TELEMETRY_USAGE_SUMMARY",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Emit usage summary telemetry records for multi-channel replies"
    )]
    pub(crate) multi_channel_telemetry_usage_summary: bool,

    #[arg(
        long = "multi-channel-telemetry-include-identifiers",
        env = "TAU_MULTI_CHANNEL_TELEMETRY_INCLUDE_IDENTIFIERS",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Include actor/conversation identifiers in multi-channel telemetry payloads (default is privacy-safe false)"
    )]
    pub(crate) multi_channel_telemetry_include_identifiers: bool,

    #[arg(
        long = "multi-channel-telemetry-min-response-chars",
        env = "TAU_MULTI_CHANNEL_TELEMETRY_MIN_RESPONSE_CHARS",
        default_value_t = 120,
        value_parser = parse_positive_usize,
        help = "Minimum response length before typing/presence telemetry is emitted"
    )]
    pub(crate) multi_channel_telemetry_min_response_chars: usize,

    #[arg(
        long = "multi-channel-media-understanding",
        env = "TAU_MULTI_CHANNEL_MEDIA_UNDERSTANDING",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Enable deterministic media understanding for inbound attachment prompt/context enrichment"
    )]
    pub(crate) multi_channel_media_understanding: bool,

    #[arg(
        long = "multi-channel-media-max-attachments",
        env = "TAU_MULTI_CHANNEL_MEDIA_MAX_ATTACHMENTS",
        default_value_t = 4,
        value_parser = parse_positive_usize,
        help = "Maximum unique inbound attachments processed for media understanding per event"
    )]
    pub(crate) multi_channel_media_max_attachments: usize,

    #[arg(
        long = "multi-channel-media-max-summary-chars",
        env = "TAU_MULTI_CHANNEL_MEDIA_MAX_SUMMARY_CHARS",
        default_value_t = 280,
        value_parser = parse_positive_usize,
        help = "Maximum characters retained for each media understanding summary line"
    )]
    pub(crate) multi_channel_media_max_summary_chars: usize,

    #[arg(
        long = "multi-channel-outbound-mode",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_MODE",
        value_enum,
        default_value_t = CliMultiChannelOutboundMode::ChannelStore,
        help = "Outbound delivery mode for multi-channel runtime (channel-store, dry-run, provider)"
    )]
    pub(crate) multi_channel_outbound_mode: CliMultiChannelOutboundMode,

    #[arg(
        long = "multi-channel-outbound-max-chars",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_MAX_CHARS",
        default_value_t = 1200,
        help = "Maximum outbound response chunk size in characters before provider-safe chunk splitting"
    )]
    pub(crate) multi_channel_outbound_max_chars: usize,

    #[arg(
        long = "multi-channel-outbound-http-timeout-ms",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_HTTP_TIMEOUT_MS",
        default_value_t = 5000,
        help = "Provider HTTP timeout in milliseconds for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_outbound_http_timeout_ms: u64,

    #[arg(
        long = "multi-channel-telegram-api-base",
        env = "TAU_MULTI_CHANNEL_TELEGRAM_API_BASE",
        default_value = "https://api.telegram.org",
        help = "Telegram provider API base URL for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_telegram_api_base: String,

    #[arg(
        long = "multi-channel-discord-api-base",
        env = "TAU_MULTI_CHANNEL_DISCORD_API_BASE",
        default_value = "https://discord.com/api/v10",
        help = "Discord provider API base URL for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_discord_api_base: String,

    #[arg(
        long = "multi-channel-whatsapp-api-base",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_API_BASE",
        default_value = "https://graph.facebook.com/v20.0",
        help = "WhatsApp provider API base URL for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_whatsapp_api_base: String,

    #[arg(
        long = "multi-channel-telegram-bot-token",
        env = "TAU_TELEGRAM_BOT_TOKEN",
        hide_env_values = true,
        help = "Telegram bot token for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_telegram_bot_token: Option<String>,

    #[arg(
        long = "multi-channel-discord-bot-token",
        env = "TAU_DISCORD_BOT_TOKEN",
        hide_env_values = true,
        help = "Discord bot token for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_discord_bot_token: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-access-token",
        env = "TAU_WHATSAPP_ACCESS_TOKEN",
        hide_env_values = true,
        help = "WhatsApp access token for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_whatsapp_access_token: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-phone-number-id",
        env = "TAU_WHATSAPP_PHONE_NUMBER_ID",
        help = "WhatsApp phone number id for multi-channel outbound mode=provider"
    )]
    pub(crate) multi_channel_whatsapp_phone_number_id: Option<String>,

    #[arg(
        long = "multi-agent-contract-runner",
        env = "TAU_MULTI_AGENT_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven multi-agent runtime contract scenarios"
    )]
    pub(crate) multi_agent_contract_runner: bool,

    #[arg(
        long = "multi-agent-fixture",
        env = "TAU_MULTI_AGENT_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/multi-agent-contract/mixed-outcomes.json",
        requires = "multi_agent_contract_runner",
        help = "Path to multi-agent runtime contract fixture JSON"
    )]
    pub(crate) multi_agent_fixture: PathBuf,

    #[arg(
        long = "multi-agent-state-dir",
        env = "TAU_MULTI_AGENT_STATE_DIR",
        default_value = ".tau/multi-agent",
        help = "Directory for multi-agent runtime state and channel-store outputs"
    )]
    pub(crate) multi_agent_state_dir: PathBuf,

    #[arg(
        long = "multi-agent-queue-limit",
        env = "TAU_MULTI_AGENT_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "multi_agent_contract_runner",
        help = "Maximum multi-agent fixture cases processed per runtime cycle"
    )]
    pub(crate) multi_agent_queue_limit: usize,

    #[arg(
        long = "multi-agent-processed-case-cap",
        env = "TAU_MULTI_AGENT_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "multi_agent_contract_runner",
        help = "Maximum processed-case keys retained for multi-agent duplicate suppression"
    )]
    pub(crate) multi_agent_processed_case_cap: usize,

    #[arg(
        long = "multi-agent-retry-max-attempts",
        env = "TAU_MULTI_AGENT_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "multi_agent_contract_runner",
        help = "Maximum retry attempts for transient multi-agent runtime failures"
    )]
    pub(crate) multi_agent_retry_max_attempts: usize,

    #[arg(
        long = "multi-agent-retry-base-delay-ms",
        env = "TAU_MULTI_AGENT_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "multi_agent_contract_runner",
        help = "Base backoff delay in milliseconds for multi-agent runtime retries (0 disables delay)"
    )]
    pub(crate) multi_agent_retry_base_delay_ms: u64,

    #[arg(
        long = "browser-automation-contract-runner",
        env = "TAU_BROWSER_AUTOMATION_CONTRACT_RUNNER",
        default_value_t = false,
        conflicts_with = "browser_automation_preflight",
        help = "Run fixture-driven browser automation runtime contract scenarios"
    )]
    pub(crate) browser_automation_contract_runner: bool,

    #[arg(
        long = "browser-automation-fixture",
        env = "TAU_BROWSER_AUTOMATION_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/browser-automation-contract/mixed-outcomes.json",
        requires = "browser_automation_contract_runner",
        help = "Path to browser automation runtime contract fixture JSON"
    )]
    pub(crate) browser_automation_fixture: PathBuf,

    #[arg(
        long = "browser-automation-state-dir",
        env = "TAU_BROWSER_AUTOMATION_STATE_DIR",
        default_value = ".tau/browser-automation",
        help = "Directory for browser automation runtime state and channel-store outputs"
    )]
    pub(crate) browser_automation_state_dir: PathBuf,

    #[arg(
        long = "browser-automation-queue-limit",
        env = "TAU_BROWSER_AUTOMATION_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "browser_automation_contract_runner",
        help = "Maximum browser automation fixture cases processed per runtime cycle"
    )]
    pub(crate) browser_automation_queue_limit: usize,

    #[arg(
        long = "browser-automation-processed-case-cap",
        env = "TAU_BROWSER_AUTOMATION_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "browser_automation_contract_runner",
        help = "Maximum processed-case keys retained for browser automation duplicate suppression"
    )]
    pub(crate) browser_automation_processed_case_cap: usize,

    #[arg(
        long = "browser-automation-retry-max-attempts",
        env = "TAU_BROWSER_AUTOMATION_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "browser_automation_contract_runner",
        help = "Maximum retry attempts for transient browser automation runtime failures"
    )]
    pub(crate) browser_automation_retry_max_attempts: usize,

    #[arg(
        long = "browser-automation-retry-base-delay-ms",
        env = "TAU_BROWSER_AUTOMATION_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "browser_automation_contract_runner",
        help = "Base backoff delay in milliseconds for browser automation runtime retries (0 disables delay)"
    )]
    pub(crate) browser_automation_retry_base_delay_ms: u64,

    #[arg(
        long = "browser-automation-action-timeout-ms",
        env = "TAU_BROWSER_AUTOMATION_ACTION_TIMEOUT_MS",
        default_value_t = 5_000,
        requires = "browser_automation_contract_runner",
        help = "Maximum allowed action timeout in milliseconds for one browser automation case"
    )]
    pub(crate) browser_automation_action_timeout_ms: u64,

    #[arg(
        long = "browser-automation-max-actions-per-case",
        env = "TAU_BROWSER_AUTOMATION_MAX_ACTIONS_PER_CASE",
        default_value_t = 8,
        requires = "browser_automation_contract_runner",
        help = "Maximum allowed repeated action count for one browser automation fixture case"
    )]
    pub(crate) browser_automation_max_actions_per_case: usize,

    #[arg(
        long = "browser-automation-allow-unsafe-actions",
        env = "TAU_BROWSER_AUTOMATION_ALLOW_UNSAFE_ACTIONS",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "browser_automation_contract_runner",
        help = "Allow unsafe browser automation fixture operations (default deny)"
    )]
    pub(crate) browser_automation_allow_unsafe_actions: bool,

    #[arg(
        long = "browser-automation-playwright-cli",
        env = "TAU_BROWSER_AUTOMATION_PLAYWRIGHT_CLI",
        default_value = "playwright-cli",
        help = "Playwright CLI executable used for browser automation preflight and doctor checks"
    )]
    pub(crate) browser_automation_playwright_cli: String,

    #[arg(
        long = "browser-automation-preflight",
        env = "TAU_BROWSER_AUTOMATION_PREFLIGHT",
        default_value_t = false,
        conflicts_with = "browser_automation_contract_runner",
        help = "Run browser automation prerequisite checks and exit"
    )]
    pub(crate) browser_automation_preflight: bool,

    #[arg(
        long = "browser-automation-preflight-json",
        env = "TAU_BROWSER_AUTOMATION_PREFLIGHT_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "browser_automation_preflight",
        help = "Emit --browser-automation-preflight output as pretty JSON"
    )]
    pub(crate) browser_automation_preflight_json: bool,

    #[arg(
        long = "memory-contract-runner",
        env = "TAU_MEMORY_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven semantic memory runtime contract scenarios"
    )]
    pub(crate) memory_contract_runner: bool,

    #[arg(
        long = "memory-fixture",
        env = "TAU_MEMORY_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/memory-contract/mixed-outcomes.json",
        requires = "memory_contract_runner",
        help = "Path to semantic memory contract fixture JSON"
    )]
    pub(crate) memory_fixture: PathBuf,

    #[arg(
        long = "memory-state-dir",
        env = "TAU_MEMORY_STATE_DIR",
        default_value = ".tau/memory",
        help = "Directory for semantic memory runtime state and channel-store outputs"
    )]
    pub(crate) memory_state_dir: PathBuf,

    #[arg(
        long = "memory-queue-limit",
        env = "TAU_MEMORY_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "memory_contract_runner",
        help = "Maximum memory fixture cases processed per runtime cycle"
    )]
    pub(crate) memory_queue_limit: usize,

    #[arg(
        long = "memory-processed-case-cap",
        env = "TAU_MEMORY_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "memory_contract_runner",
        help = "Maximum processed-case keys retained for duplicate suppression"
    )]
    pub(crate) memory_processed_case_cap: usize,

    #[arg(
        long = "memory-retry-max-attempts",
        env = "TAU_MEMORY_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "memory_contract_runner",
        help = "Maximum retry attempts for transient semantic memory runtime failures"
    )]
    pub(crate) memory_retry_max_attempts: usize,

    #[arg(
        long = "memory-retry-base-delay-ms",
        env = "TAU_MEMORY_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "memory_contract_runner",
        help = "Base backoff delay in milliseconds for semantic memory runtime retries (0 disables delay)"
    )]
    pub(crate) memory_retry_base_delay_ms: u64,

    #[arg(
        long = "dashboard-contract-runner",
        env = "TAU_DASHBOARD_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven dashboard runtime contract scenarios"
    )]
    pub(crate) dashboard_contract_runner: bool,

    #[arg(
        long = "dashboard-fixture",
        env = "TAU_DASHBOARD_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/dashboard-contract/mixed-outcomes.json",
        requires = "dashboard_contract_runner",
        help = "Path to dashboard runtime contract fixture JSON"
    )]
    pub(crate) dashboard_fixture: PathBuf,

    #[arg(
        long = "dashboard-state-dir",
        env = "TAU_DASHBOARD_STATE_DIR",
        default_value = ".tau/dashboard",
        help = "Directory for dashboard runtime state and channel-store outputs"
    )]
    pub(crate) dashboard_state_dir: PathBuf,

    #[arg(
        long = "dashboard-queue-limit",
        env = "TAU_DASHBOARD_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "dashboard_contract_runner",
        help = "Maximum dashboard fixture cases processed per runtime cycle"
    )]
    pub(crate) dashboard_queue_limit: usize,

    #[arg(
        long = "dashboard-processed-case-cap",
        env = "TAU_DASHBOARD_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "dashboard_contract_runner",
        help = "Maximum processed-case keys retained for dashboard duplicate suppression"
    )]
    pub(crate) dashboard_processed_case_cap: usize,

    #[arg(
        long = "dashboard-retry-max-attempts",
        env = "TAU_DASHBOARD_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "dashboard_contract_runner",
        help = "Maximum retry attempts for transient dashboard runtime failures"
    )]
    pub(crate) dashboard_retry_max_attempts: usize,

    #[arg(
        long = "dashboard-retry-base-delay-ms",
        env = "TAU_DASHBOARD_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "dashboard_contract_runner",
        help = "Base backoff delay in milliseconds for dashboard runtime retries (0 disables delay)"
    )]
    pub(crate) dashboard_retry_base_delay_ms: u64,

    #[arg(
        long = "gateway-openresponses-server",
        env = "TAU_GATEWAY_OPENRESPONSES_SERVER",
        default_value_t = false,
        help = "Run authenticated OpenResponses subset HTTP endpoint at POST /v1/responses"
    )]
    pub(crate) gateway_openresponses_server: bool,

    #[arg(
        long = "gateway-openresponses-bind",
        env = "TAU_GATEWAY_OPENRESPONSES_BIND",
        default_value = "127.0.0.1:8787",
        requires = "gateway_openresponses_server",
        help = "Socket address for --gateway-openresponses-server (host:port)"
    )]
    pub(crate) gateway_openresponses_bind: String,

    #[arg(
        long = "gateway-openresponses-auth-mode",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_MODE",
        value_enum,
        default_value_t = CliGatewayOpenResponsesAuthMode::Token,
        requires = "gateway_openresponses_server",
        help = "Gateway auth mode: token, password-session, or localhost-dev"
    )]
    pub(crate) gateway_openresponses_auth_mode: CliGatewayOpenResponsesAuthMode,

    #[arg(
        long = "gateway-openresponses-auth-token",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_TOKEN",
        requires = "gateway_openresponses_server",
        help = "Bearer token used when --gateway-openresponses-auth-mode=token"
    )]
    pub(crate) gateway_openresponses_auth_token: Option<String>,

    #[arg(
        long = "gateway-openresponses-auth-password",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_PASSWORD",
        requires = "gateway_openresponses_server",
        help = "Password used by /gateway/auth/session when --gateway-openresponses-auth-mode=password-session"
    )]
    pub(crate) gateway_openresponses_auth_password: Option<String>,

    #[arg(
        long = "gateway-openresponses-session-ttl-seconds",
        env = "TAU_GATEWAY_OPENRESPONSES_SESSION_TTL_SECONDS",
        default_value_t = 3600,
        value_parser = parse_positive_u64,
        requires = "gateway_openresponses_server",
        help = "Session token TTL in seconds for password-session auth mode"
    )]
    pub(crate) gateway_openresponses_session_ttl_seconds: u64,

    #[arg(
        long = "gateway-openresponses-rate-limit-window-seconds",
        env = "TAU_GATEWAY_OPENRESPONSES_RATE_LIMIT_WINDOW_SECONDS",
        default_value_t = 60,
        value_parser = parse_positive_u64,
        requires = "gateway_openresponses_server",
        help = "Rate-limit window size in seconds"
    )]
    pub(crate) gateway_openresponses_rate_limit_window_seconds: u64,

    #[arg(
        long = "gateway-openresponses-rate-limit-max-requests",
        env = "TAU_GATEWAY_OPENRESPONSES_RATE_LIMIT_MAX_REQUESTS",
        default_value_t = 120,
        value_parser = parse_positive_usize,
        requires = "gateway_openresponses_server",
        help = "Maximum accepted requests per auth principal within one rate-limit window"
    )]
    pub(crate) gateway_openresponses_rate_limit_max_requests: usize,

    #[arg(
        long = "gateway-openresponses-max-input-chars",
        env = "TAU_GATEWAY_OPENRESPONSES_MAX_INPUT_CHARS",
        default_value_t = 32_000,
        value_parser = parse_positive_usize,
        requires = "gateway_openresponses_server",
        help = "Maximum translated input size accepted by /v1/responses"
    )]
    pub(crate) gateway_openresponses_max_input_chars: usize,

    #[arg(
        long = "gateway-contract-runner",
        env = "TAU_GATEWAY_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven gateway runtime contract scenarios"
    )]
    pub(crate) gateway_contract_runner: bool,

    #[arg(
        long = "gateway-fixture",
        env = "TAU_GATEWAY_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/gateway-contract/mixed-outcomes.json",
        requires = "gateway_contract_runner",
        help = "Path to gateway runtime contract fixture JSON"
    )]
    pub(crate) gateway_fixture: PathBuf,

    #[arg(
        long = "gateway-state-dir",
        env = "TAU_GATEWAY_STATE_DIR",
        default_value = ".tau/gateway",
        help = "Directory for gateway runtime state and channel-store outputs"
    )]
    pub(crate) gateway_state_dir: PathBuf,

    #[arg(
        long = "gateway-guardrail-failure-streak-threshold",
        env = "TAU_GATEWAY_GUARDRAIL_FAILURE_STREAK_THRESHOLD",
        default_value_t = 2,
        requires = "gateway_contract_runner",
        help = "Failure streak threshold that forces gateway rollout gate to hold"
    )]
    pub(crate) gateway_guardrail_failure_streak_threshold: usize,

    #[arg(
        long = "gateway-guardrail-retryable-failures-threshold",
        env = "TAU_GATEWAY_GUARDRAIL_RETRYABLE_FAILURES_THRESHOLD",
        default_value_t = 2,
        requires = "gateway_contract_runner",
        help = "Per-cycle retryable failure threshold that forces gateway rollout gate to hold"
    )]
    pub(crate) gateway_guardrail_retryable_failures_threshold: usize,

    #[arg(
        long = "deployment-contract-runner",
        env = "TAU_DEPLOYMENT_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven cloud deployment and WASM runtime contract scenarios"
    )]
    pub(crate) deployment_contract_runner: bool,

    #[arg(
        long = "deployment-fixture",
        env = "TAU_DEPLOYMENT_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/deployment-contract/mixed-outcomes.json",
        requires = "deployment_contract_runner",
        help = "Path to cloud deployment + WASM runtime contract fixture JSON"
    )]
    pub(crate) deployment_fixture: PathBuf,

    #[arg(
        long = "deployment-state-dir",
        env = "TAU_DEPLOYMENT_STATE_DIR",
        default_value = ".tau/deployment",
        help = "Directory for deployment runtime state and channel-store outputs"
    )]
    pub(crate) deployment_state_dir: PathBuf,

    #[arg(
        long = "deployment-wasm-package-module",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_MODULE",
        conflicts_with = "deployment_contract_runner",
        help = "Package one WASM module into a verifiable deployment manifest/artifact and exit"
    )]
    pub(crate) deployment_wasm_package_module: Option<PathBuf>,

    #[arg(
        long = "deployment-wasm-package-blueprint-id",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_BLUEPRINT_ID",
        requires = "deployment_wasm_package_module",
        default_value = "edge-wasm",
        help = "Blueprint id recorded in WASM package manifest metadata"
    )]
    pub(crate) deployment_wasm_package_blueprint_id: String,

    #[arg(
        long = "deployment-wasm-package-runtime-profile",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_RUNTIME_PROFILE",
        value_enum,
        requires = "deployment_wasm_package_module",
        default_value_t = CliDeploymentWasmRuntimeProfile::WasmWasi,
        help = "WASM runtime profile recorded in package manifest metadata"
    )]
    pub(crate) deployment_wasm_package_runtime_profile: CliDeploymentWasmRuntimeProfile,

    #[arg(
        long = "deployment-wasm-package-output-dir",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_OUTPUT_DIR",
        requires = "deployment_wasm_package_module",
        default_value = ".tau/deployment/wasm-artifacts",
        help = "Directory where packaged WASM artifacts and manifest files are written"
    )]
    pub(crate) deployment_wasm_package_output_dir: PathBuf,

    #[arg(
        long = "deployment-wasm-package-json",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "deployment_wasm_package_module",
        help = "Emit --deployment-wasm-package-module output as pretty JSON"
    )]
    pub(crate) deployment_wasm_package_json: bool,

    #[arg(
        long = "deployment-wasm-inspect-manifest",
        env = "TAU_DEPLOYMENT_WASM_INSPECT_MANIFEST",
        conflicts_with = "deployment_contract_runner",
        conflicts_with = "deployment_wasm_package_module",
        help = "Inspect a packaged deployment WASM manifest for control-plane profile compliance and exit"
    )]
    pub(crate) deployment_wasm_inspect_manifest: Option<PathBuf>,

    #[arg(
        long = "deployment-wasm-inspect-json",
        env = "TAU_DEPLOYMENT_WASM_INSPECT_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "deployment_wasm_inspect_manifest",
        help = "Emit --deployment-wasm-inspect-manifest output as pretty JSON"
    )]
    pub(crate) deployment_wasm_inspect_json: bool,

    #[arg(
        long = "deployment-queue-limit",
        env = "TAU_DEPLOYMENT_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "deployment_contract_runner",
        help = "Maximum deployment fixture cases processed per runtime cycle"
    )]
    pub(crate) deployment_queue_limit: usize,

    #[arg(
        long = "deployment-processed-case-cap",
        env = "TAU_DEPLOYMENT_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "deployment_contract_runner",
        help = "Maximum processed-case keys retained for deployment duplicate suppression"
    )]
    pub(crate) deployment_processed_case_cap: usize,

    #[arg(
        long = "deployment-retry-max-attempts",
        env = "TAU_DEPLOYMENT_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "deployment_contract_runner",
        help = "Maximum retry attempts for transient deployment runtime failures"
    )]
    pub(crate) deployment_retry_max_attempts: usize,

    #[arg(
        long = "deployment-retry-base-delay-ms",
        env = "TAU_DEPLOYMENT_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "deployment_contract_runner",
        help = "Base backoff delay in milliseconds for deployment runtime retries (0 disables delay)"
    )]
    pub(crate) deployment_retry_base_delay_ms: u64,

    #[arg(
        long = "custom-command-contract-runner",
        env = "TAU_CUSTOM_COMMAND_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven no-code custom command runtime contract scenarios"
    )]
    pub(crate) custom_command_contract_runner: bool,

    #[arg(
        long = "custom-command-fixture",
        env = "TAU_CUSTOM_COMMAND_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/custom-command-contract/mixed-outcomes.json",
        requires = "custom_command_contract_runner",
        help = "Path to no-code custom command runtime contract fixture JSON"
    )]
    pub(crate) custom_command_fixture: PathBuf,

    #[arg(
        long = "custom-command-state-dir",
        env = "TAU_CUSTOM_COMMAND_STATE_DIR",
        default_value = ".tau/custom-command",
        help = "Directory for no-code custom command runtime state and channel-store outputs"
    )]
    pub(crate) custom_command_state_dir: PathBuf,

    #[arg(
        long = "custom-command-queue-limit",
        env = "TAU_CUSTOM_COMMAND_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "custom_command_contract_runner",
        help = "Maximum no-code custom command fixture cases processed per runtime cycle"
    )]
    pub(crate) custom_command_queue_limit: usize,

    #[arg(
        long = "custom-command-processed-case-cap",
        env = "TAU_CUSTOM_COMMAND_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "custom_command_contract_runner",
        help = "Maximum processed-case keys retained for no-code custom command duplicate suppression"
    )]
    pub(crate) custom_command_processed_case_cap: usize,

    #[arg(
        long = "custom-command-retry-max-attempts",
        env = "TAU_CUSTOM_COMMAND_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "custom_command_contract_runner",
        help = "Maximum retry attempts for transient no-code custom command runtime failures"
    )]
    pub(crate) custom_command_retry_max_attempts: usize,

    #[arg(
        long = "custom-command-retry-base-delay-ms",
        env = "TAU_CUSTOM_COMMAND_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "custom_command_contract_runner",
        help = "Base backoff delay in milliseconds for no-code custom command runtime retries (0 disables delay)"
    )]
    pub(crate) custom_command_retry_base_delay_ms: u64,

    #[arg(
        long = "voice-contract-runner",
        env = "TAU_VOICE_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven voice interaction and wake-word runtime contract scenarios"
    )]
    pub(crate) voice_contract_runner: bool,

    #[arg(
        long = "voice-fixture",
        env = "TAU_VOICE_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/voice-contract/mixed-outcomes.json",
        requires = "voice_contract_runner",
        help = "Path to voice interaction and wake-word runtime contract fixture JSON"
    )]
    pub(crate) voice_fixture: PathBuf,

    #[arg(
        long = "voice-state-dir",
        env = "TAU_VOICE_STATE_DIR",
        default_value = ".tau/voice",
        help = "Directory for voice runtime state and channel-store outputs"
    )]
    pub(crate) voice_state_dir: PathBuf,

    #[arg(
        long = "voice-queue-limit",
        env = "TAU_VOICE_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "voice_contract_runner",
        help = "Maximum voice fixture cases processed per runtime cycle"
    )]
    pub(crate) voice_queue_limit: usize,

    #[arg(
        long = "voice-processed-case-cap",
        env = "TAU_VOICE_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "voice_contract_runner",
        help = "Maximum processed-case keys retained for voice duplicate suppression"
    )]
    pub(crate) voice_processed_case_cap: usize,

    #[arg(
        long = "voice-retry-max-attempts",
        env = "TAU_VOICE_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "voice_contract_runner",
        help = "Maximum retry attempts for transient voice runtime failures"
    )]
    pub(crate) voice_retry_max_attempts: usize,

    #[arg(
        long = "voice-retry-base-delay-ms",
        env = "TAU_VOICE_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "voice_contract_runner",
        help = "Base backoff delay in milliseconds for voice runtime retries (0 disables delay)"
    )]
    pub(crate) voice_retry_base_delay_ms: u64,

    #[arg(
        long = "github-issues-bridge",
        env = "TAU_GITHUB_ISSUES_BRIDGE",
        default_value_t = false,
        help = "Run as a GitHub Issues conversational transport loop instead of interactive prompt mode"
    )]
    pub(crate) github_issues_bridge: bool,

    #[arg(
        long = "github-repo",
        env = "TAU_GITHUB_REPO",
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
        env = "TAU_GITHUB_TOKEN_ID",
        requires = "github_issues_bridge",
        help = "Credential-store integration id used to resolve GitHub bridge token"
    )]
    pub(crate) github_token_id: Option<String>,

    #[arg(
        long = "github-bot-login",
        env = "TAU_GITHUB_BOT_LOGIN",
        requires = "github_issues_bridge",
        help = "Optional bot login used to ignore self-comments and identify already-replied events"
    )]
    pub(crate) github_bot_login: Option<String>,

    #[arg(
        long = "github-api-base",
        env = "TAU_GITHUB_API_BASE",
        default_value = "https://api.github.com",
        requires = "github_issues_bridge",
        help = "GitHub API base URL"
    )]
    pub(crate) github_api_base: String,

    #[arg(
        long = "github-state-dir",
        env = "TAU_GITHUB_STATE_DIR",
        default_value = ".tau/github-issues",
        help = "Directory for github bridge state/session/event logs"
    )]
    pub(crate) github_state_dir: PathBuf,

    #[arg(
        long = "github-poll-interval-seconds",
        env = "TAU_GITHUB_POLL_INTERVAL_SECONDS",
        default_value_t = 30,
        requires = "github_issues_bridge",
        help = "Polling interval in seconds for github bridge mode"
    )]
    pub(crate) github_poll_interval_seconds: u64,

    #[arg(
        long = "github-poll-once",
        env = "TAU_GITHUB_POLL_ONCE",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "github_issues_bridge",
        help = "Run one GitHub bridge poll cycle, drain spawned runs, and exit"
    )]
    pub(crate) github_poll_once: bool,

    #[arg(
        long = "github-required-label",
        env = "TAU_GITHUB_REQUIRED_LABEL",
        value_delimiter = ',',
        requires = "github_issues_bridge",
        help = "Only process issues containing one of these labels (repeatable)"
    )]
    pub(crate) github_required_label: Vec<String>,

    #[arg(
        long = "github-issue-number",
        env = "TAU_GITHUB_ISSUE_NUMBER",
        value_delimiter = ',',
        value_parser = parse_positive_u64,
        requires = "github_issues_bridge",
        help = "Only process these GitHub issue numbers (repeatable)"
    )]
    pub(crate) github_issue_number: Vec<u64>,

    #[arg(
        long = "github-artifact-retention-days",
        env = "TAU_GITHUB_ARTIFACT_RETENTION_DAYS",
        default_value_t = 30,
        requires = "github_issues_bridge",
        help = "Retention window for GitHub bridge artifacts in days (0 disables expiration)"
    )]
    pub(crate) github_artifact_retention_days: u64,

    #[arg(
        long = "github-include-issue-body",
        env = "TAU_GITHUB_INCLUDE_ISSUE_BODY",
        default_value_t = false,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Treat the issue description itself as an initial conversation event"
    )]
    pub(crate) github_include_issue_body: bool,

    #[arg(
        long = "github-include-edited-comments",
        env = "TAU_GITHUB_INCLUDE_EDITED_COMMENTS",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Process edited issue comments as new events (deduped by comment id + updated timestamp)"
    )]
    pub(crate) github_include_edited_comments: bool,

    #[arg(
        long = "github-processed-event-cap",
        env = "TAU_GITHUB_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "github_issues_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    pub(crate) github_processed_event_cap: usize,

    #[arg(
        long = "github-retry-max-attempts",
        env = "TAU_GITHUB_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "github_issues_bridge",
        help = "Maximum attempts for retryable github api failures (429/5xx/transport)"
    )]
    pub(crate) github_retry_max_attempts: usize,

    #[arg(
        long = "github-retry-base-delay-ms",
        env = "TAU_GITHUB_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "github_issues_bridge",
        help = "Base backoff delay in milliseconds for github api retries"
    )]
    pub(crate) github_retry_base_delay_ms: u64,

    #[arg(
        long = "slack-bridge",
        env = "TAU_SLACK_BRIDGE",
        default_value_t = false,
        help = "Run as a Slack Socket Mode conversational transport loop instead of interactive prompt mode"
    )]
    pub(crate) slack_bridge: bool,

    #[arg(
        long = "slack-app-token",
        env = "TAU_SLACK_APP_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack Socket Mode app token (xapp-...)"
    )]
    pub(crate) slack_app_token: Option<String>,

    #[arg(
        long = "slack-app-token-id",
        env = "TAU_SLACK_APP_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack app token"
    )]
    pub(crate) slack_app_token_id: Option<String>,

    #[arg(
        long = "slack-bot-token",
        env = "TAU_SLACK_BOT_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack bot token for Web API (xoxb-...)"
    )]
    pub(crate) slack_bot_token: Option<String>,

    #[arg(
        long = "slack-bot-token-id",
        env = "TAU_SLACK_BOT_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack bot token"
    )]
    pub(crate) slack_bot_token_id: Option<String>,

    #[arg(
        long = "slack-bot-user-id",
        env = "TAU_SLACK_BOT_USER_ID",
        requires = "slack_bridge",
        help = "Optional bot user id used to strip self-mentions and ignore self-authored events"
    )]
    pub(crate) slack_bot_user_id: Option<String>,

    #[arg(
        long = "slack-api-base",
        env = "TAU_SLACK_API_BASE",
        default_value = "https://slack.com/api",
        requires = "slack_bridge",
        help = "Slack Web API base URL"
    )]
    pub(crate) slack_api_base: String,

    #[arg(
        long = "slack-state-dir",
        env = "TAU_SLACK_STATE_DIR",
        default_value = ".tau/slack",
        requires = "slack_bridge",
        help = "Directory for slack bridge state/session/event logs"
    )]
    pub(crate) slack_state_dir: PathBuf,

    #[arg(
        long = "slack-artifact-retention-days",
        env = "TAU_SLACK_ARTIFACT_RETENTION_DAYS",
        default_value_t = 30,
        requires = "slack_bridge",
        help = "Retention window for Slack bridge artifacts in days (0 disables expiration)"
    )]
    pub(crate) slack_artifact_retention_days: u64,

    #[arg(
        long = "slack-thread-detail-output",
        env = "TAU_SLACK_THREAD_DETAIL_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "slack_bridge",
        help = "When responses exceed threshold, keep summary in placeholder and post full response as a threaded detail message"
    )]
    pub(crate) slack_thread_detail_output: bool,

    #[arg(
        long = "slack-thread-detail-threshold-chars",
        env = "TAU_SLACK_THREAD_DETAIL_THRESHOLD_CHARS",
        default_value_t = 1500,
        requires = "slack_bridge",
        help = "Character threshold used with --slack-thread-detail-output"
    )]
    pub(crate) slack_thread_detail_threshold_chars: usize,

    #[arg(
        long = "slack-processed-event-cap",
        env = "TAU_SLACK_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "slack_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    pub(crate) slack_processed_event_cap: usize,

    #[arg(
        long = "slack-max-event-age-seconds",
        env = "TAU_SLACK_MAX_EVENT_AGE_SECONDS",
        default_value_t = 7_200,
        requires = "slack_bridge",
        help = "Ignore inbound Slack events older than this many seconds (0 disables age checks)"
    )]
    pub(crate) slack_max_event_age_seconds: u64,

    #[arg(
        long = "slack-reconnect-delay-ms",
        env = "TAU_SLACK_RECONNECT_DELAY_MS",
        default_value_t = 1_000,
        requires = "slack_bridge",
        help = "Delay before reconnecting after socket/session errors"
    )]
    pub(crate) slack_reconnect_delay_ms: u64,

    #[arg(
        long = "slack-retry-max-attempts",
        env = "TAU_SLACK_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "slack_bridge",
        help = "Maximum attempts for retryable slack api failures (429/5xx/transport)"
    )]
    pub(crate) slack_retry_max_attempts: usize,

    #[arg(
        long = "slack-retry-base-delay-ms",
        env = "TAU_SLACK_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "slack_bridge",
        help = "Base backoff delay in milliseconds for slack api retries"
    )]
    pub(crate) slack_retry_base_delay_ms: u64,

    #[arg(
        long,
        env = "TAU_SESSION",
        default_value = ".tau/sessions/default.jsonl",
        help = "Session JSONL file"
    )]
    pub(crate) session: PathBuf,

    #[arg(long, help = "Disable session persistence")]
    pub(crate) no_session: bool,

    #[arg(
        long,
        env = "TAU_SESSION_VALIDATE",
        default_value_t = false,
        help = "Validate session graph integrity and exit"
    )]
    pub(crate) session_validate: bool,

    #[arg(
        long,
        env = "TAU_SESSION_IMPORT_MODE",
        value_enum,
        default_value = "merge",
        help = "Import mode for /session-import: merge appends with id remapping, replace overwrites the current session"
    )]
    pub(crate) session_import_mode: CliSessionImportMode,

    #[arg(long, help = "Start from a specific session entry id")]
    pub(crate) branch_from: Option<u64>,

    #[arg(
        long,
        env = "TAU_SESSION_LOCK_WAIT_MS",
        default_value_t = 5_000,
        help = "Maximum time to wait for acquiring the session lock in milliseconds"
    )]
    pub(crate) session_lock_wait_ms: u64,

    #[arg(
        long,
        env = "TAU_SESSION_LOCK_STALE_MS",
        default_value_t = 30_000,
        help = "Lock-file age threshold in milliseconds before stale session locks are reclaimed (0 disables reclaim)"
    )]
    pub(crate) session_lock_stale_ms: u64,

    #[arg(
        long = "allow-path",
        env = "TAU_ALLOW_PATH",
        value_delimiter = ',',
        help = "Allowed filesystem roots for read/write/edit/bash cwd (repeatable or comma-separated)"
    )]
    pub(crate) allow_path: Vec<PathBuf>,

    #[arg(
        long,
        env = "TAU_BASH_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout for bash tool commands in milliseconds"
    )]
    pub(crate) bash_timeout_ms: u64,

    #[arg(
        long,
        env = "TAU_MAX_TOOL_OUTPUT_BYTES",
        default_value_t = 16_000,
        help = "Maximum bytes returned from tool outputs (stdout/stderr)"
    )]
    pub(crate) max_tool_output_bytes: usize,

    #[arg(
        long,
        env = "TAU_MAX_FILE_READ_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size read by the read tool"
    )]
    pub(crate) max_file_read_bytes: usize,

    #[arg(
        long,
        env = "TAU_MAX_FILE_WRITE_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size written by write/edit tools"
    )]
    pub(crate) max_file_write_bytes: usize,

    #[arg(
        long,
        env = "TAU_MAX_COMMAND_LENGTH",
        default_value_t = 4_096,
        help = "Maximum command length accepted by the bash tool"
    )]
    pub(crate) max_command_length: usize,

    #[arg(
        long,
        env = "TAU_ALLOW_COMMAND_NEWLINES",
        default_value_t = false,
        help = "Allow newline characters in bash commands"
    )]
    pub(crate) allow_command_newlines: bool,

    #[arg(
        long,
        env = "TAU_BASH_PROFILE",
        value_enum,
        default_value = "balanced",
        help = "Command execution profile for bash tool: permissive, balanced, or strict"
    )]
    pub(crate) bash_profile: CliBashProfile,

    #[arg(
        long,
        env = "TAU_TOOL_POLICY_PRESET",
        value_enum,
        default_value = "balanced",
        help = "Tool policy preset: permissive, balanced, strict, or hardened"
    )]
    pub(crate) tool_policy_preset: CliToolPolicyPreset,

    #[arg(
        long,
        env = "TAU_BASH_DRY_RUN",
        default_value_t = false,
        help = "Validate bash commands against policy without executing them"
    )]
    pub(crate) bash_dry_run: bool,

    #[arg(
        long,
        env = "TAU_TOOL_POLICY_TRACE",
        default_value_t = false,
        help = "Include policy evaluation trace details in bash tool results"
    )]
    pub(crate) tool_policy_trace: bool,

    #[arg(
        long = "allow-command",
        env = "TAU_ALLOW_COMMAND",
        value_delimiter = ',',
        help = "Additional command executables/prefixes to allow (supports trailing '*' wildcards)"
    )]
    pub(crate) allow_command: Vec<String>,

    #[arg(
        long,
        env = "TAU_PRINT_TOOL_POLICY",
        default_value_t = false,
        help = "Print effective tool policy JSON before executing prompts"
    )]
    pub(crate) print_tool_policy: bool,

    #[arg(
        long,
        env = "TAU_TOOL_AUDIT_LOG",
        help = "Optional JSONL file path for tool execution audit events"
    )]
    pub(crate) tool_audit_log: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_TELEMETRY_LOG",
        help = "Optional JSONL file path for prompt-level telemetry summaries"
    )]
    pub(crate) telemetry_log: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_OS_SANDBOX_MODE",
        value_enum,
        default_value = "off",
        help = "OS sandbox mode for bash tool: off, auto, or force"
    )]
    pub(crate) os_sandbox_mode: CliOsSandboxMode,

    #[arg(
        long = "os-sandbox-command",
        env = "TAU_OS_SANDBOX_COMMAND",
        value_delimiter = ',',
        help = "Optional sandbox launcher command template tokens. Supports placeholders: {shell}, {command}, {cwd}"
    )]
    pub(crate) os_sandbox_command: Vec<String>,

    #[arg(
        long,
        env = "TAU_ENFORCE_REGULAR_FILES",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Require read/edit targets and existing write targets to be regular files (reject symlink targets)"
    )]
    pub(crate) enforce_regular_files: bool,
}
