use std::path::PathBuf;

use clap::{ArgAction, Parser};

use crate::{
    CliCommandFileErrorMode, CliCredentialStoreEncryptionMode, CliOrchestratorMode,
    CliPromptSanitizerMode, CliProviderAuthMode, CliShellCompletion,
};

mod execution_domain_flags;
mod gateway_daemon_flags;
mod runtime_tail_flags;

pub use execution_domain_flags::CliExecutionDomainFlags;
pub use gateway_daemon_flags::CliGatewayDaemonFlags;
pub use runtime_tail_flags::CliRuntimeTailFlags;

const RELEASE_LOOKUP_CACHE_TTL_MS: u64 = 15 * 60 * 1_000;

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

fn parse_positive_f64(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|error| format!("failed to parse float: {error}"))?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err("value must be a finite number greater than 0".to_string());
    }
    Ok(parsed)
}

fn parse_threshold_percent(value: &str) -> Result<u8, String> {
    let parsed = value
        .parse::<u8>()
        .map_err(|error| format!("failed to parse percent: {error}"))?;
    if !(1..=100).contains(&parsed) {
        return Err("value must be in range 1..=100".to_string());
    }
    Ok(parsed)
}

#[derive(Debug, Parser)]
#[command(
    name = "tau-rs",
    about = "Pure Rust coding agent inspired by upstream mono",
    version
)]
/// Public struct `Cli` used across Tau components.
pub struct Cli {
    #[arg(
        long = "shell-completion",
        value_enum,
        help = "Print shell completion script to stdout and exit (supported: bash, zsh, fish)"
    )]
    pub shell_completion: Option<CliShellCompletion>,

    #[arg(
        long,
        env = "TAU_MODEL",
        default_value = "openai/gpt-4o-mini",
        help = "Model in provider/model format. Supported providers: openai, openrouter, deepseek (alias), groq (alias), xai (alias), mistral (alias), azure/azure-openai (alias), anthropic, google."
    )]
    pub model: String,

    #[arg(
        long = "fallback-model",
        env = "TAU_FALLBACK_MODEL",
        value_delimiter = ',',
        help = "Optional fallback model chain in provider/model format. Triggered only on retriable provider failures."
    )]
    pub fallback_model: Vec<String>,

    #[arg(
        long,
        env = "TAU_API_BASE",
        default_value = "https://api.openai.com/v1",
        help = "Base URL for OpenAI-compatible APIs"
    )]
    pub api_base: String,

    #[arg(
        long = "azure-openai-api-version",
        env = "TAU_AZURE_OPENAI_API_VERSION",
        default_value = "2024-10-21",
        help = "Azure OpenAI api-version query value used when --api-base points to an Azure deployment endpoint"
    )]
    pub azure_openai_api_version: String,

    #[arg(
        long = "model-catalog-url",
        env = "TAU_MODEL_CATALOG_URL",
        help = "Optional remote URL for model catalog refresh (JSON payload)"
    )]
    pub model_catalog_url: Option<String>,

    #[arg(
        long = "model-catalog-cache",
        env = "TAU_MODEL_CATALOG_CACHE",
        default_value = ".tau/models/catalog.json",
        help = "Model catalog cache path used for startup and interactive model discovery"
    )]
    pub model_catalog_cache: PathBuf,

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
    pub model_catalog_offline: bool,

    #[arg(
        long = "model-catalog-stale-after-hours",
        env = "TAU_MODEL_CATALOG_STALE_AFTER_HOURS",
        default_value_t = 24,
        help = "Cache staleness threshold in hours for model catalog diagnostics"
    )]
    pub model_catalog_stale_after_hours: u64,

    #[arg(
        long,
        env = "TAU_ANTHROPIC_API_BASE",
        default_value = "https://api.anthropic.com/v1",
        help = "Base URL for Anthropic Messages API"
    )]
    pub anthropic_api_base: String,

    #[arg(
        long,
        env = "TAU_GOOGLE_API_BASE",
        default_value = "https://generativelanguage.googleapis.com/v1beta",
        help = "Base URL for Google Gemini API"
    )]
    pub google_api_base: String,

    #[arg(
        long,
        env = "TAU_API_KEY",
        hide_env_values = true,
        help = "Generic API key fallback"
    )]
    pub api_key: Option<String>,

    #[arg(
        long,
        env = "OPENAI_API_KEY",
        hide_env_values = true,
        help = "API key for OpenAI-compatible APIs"
    )]
    pub openai_api_key: Option<String>,

    #[arg(
        long,
        env = "ANTHROPIC_API_KEY",
        hide_env_values = true,
        help = "API key for Anthropic"
    )]
    pub anthropic_api_key: Option<String>,

    #[arg(
        long,
        env = "GEMINI_API_KEY",
        hide_env_values = true,
        help = "API key for Google Gemini"
    )]
    pub google_api_key: Option<String>,

    #[arg(
        long = "openai-auth-mode",
        env = "TAU_OPENAI_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for OpenAI provider"
    )]
    pub openai_auth_mode: CliProviderAuthMode,

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
    pub openai_codex_backend: bool,

    #[arg(
        long = "openai-codex-cli",
        env = "TAU_OPENAI_CODEX_CLI",
        default_value = "codex",
        help = "Codex CLI executable path used by OpenAI oauth/session backend"
    )]
    pub openai_codex_cli: String,

    #[arg(
        long = "openai-codex-args",
        env = "TAU_OPENAI_CODEX_ARGS",
        value_delimiter = ',',
        help = "Additional argument(s) forwarded to codex exec when OpenAI Codex backend is enabled"
    )]
    pub openai_codex_args: Vec<String>,

    #[arg(
        long = "openai-codex-timeout-ms",
        env = "TAU_OPENAI_CODEX_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout in milliseconds for each Codex CLI request"
    )]
    pub openai_codex_timeout_ms: u64,

    #[arg(
        long = "anthropic-auth-mode",
        env = "TAU_ANTHROPIC_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Anthropic provider"
    )]
    pub anthropic_auth_mode: CliProviderAuthMode,

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
    pub anthropic_claude_backend: bool,

    #[arg(
        long = "anthropic-claude-cli",
        env = "TAU_ANTHROPIC_CLAUDE_CLI",
        default_value = "claude",
        help = "Claude Code CLI executable path used by Anthropic oauth/session backend"
    )]
    pub anthropic_claude_cli: String,

    #[arg(
        long = "anthropic-claude-args",
        env = "TAU_ANTHROPIC_CLAUDE_ARGS",
        value_delimiter = ',',
        help = "Additional argument(s) forwarded to claude for Anthropic oauth/session backend"
    )]
    pub anthropic_claude_args: Vec<String>,

    #[arg(
        long = "anthropic-claude-timeout-ms",
        env = "TAU_ANTHROPIC_CLAUDE_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout in milliseconds for each Claude Code CLI request"
    )]
    pub anthropic_claude_timeout_ms: u64,

    #[arg(
        long = "google-auth-mode",
        env = "TAU_GOOGLE_AUTH_MODE",
        value_enum,
        default_value_t = CliProviderAuthMode::ApiKey,
        help = "Authentication mode preference for Google provider"
    )]
    pub google_auth_mode: CliProviderAuthMode,

    #[arg(
        long = "provider-subscription-strict",
        env = "TAU_PROVIDER_SUBSCRIPTION_STRICT",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Fail closed for non-api-key auth modes by disabling automatic API-key fallback"
    )]
    pub provider_subscription_strict: bool,

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
    pub google_gemini_backend: bool,

    #[arg(
        long = "google-gemini-cli",
        env = "TAU_GOOGLE_GEMINI_CLI",
        default_value = "gemini",
        help = "Gemini CLI executable path used by Google oauth/adc backend"
    )]
    pub google_gemini_cli: String,

    #[arg(
        long = "google-gcloud-cli",
        env = "TAU_GOOGLE_GCLOUD_CLI",
        default_value = "gcloud",
        help = "gcloud executable path used for Google ADC login bootstrap"
    )]
    pub google_gcloud_cli: String,

    #[arg(
        long = "google-gemini-args",
        env = "TAU_GOOGLE_GEMINI_ARGS",
        value_delimiter = ',',
        help = "Additional argument(s) forwarded to gemini for Google oauth/adc backend"
    )]
    pub google_gemini_args: Vec<String>,

    #[arg(
        long = "google-gemini-timeout-ms",
        env = "TAU_GOOGLE_GEMINI_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout in milliseconds for each Gemini CLI request"
    )]
    pub google_gemini_timeout_ms: u64,

    #[arg(
        long = "credential-store",
        env = "TAU_CREDENTIAL_STORE",
        default_value = ".tau/credentials.json",
        help = "Credential store file path for non-API-key provider auth modes"
    )]
    pub credential_store: PathBuf,

    #[arg(
        long = "credential-store-key",
        env = "TAU_CREDENTIAL_STORE_KEY",
        hide_env_values = true,
        help = "Optional encryption key for credential store entries when keyed encryption is enabled"
    )]
    pub credential_store_key: Option<String>,

    #[arg(
        long = "credential-store-encryption",
        env = "TAU_CREDENTIAL_STORE_ENCRYPTION",
        value_enum,
        default_value_t = CliCredentialStoreEncryptionMode::Auto,
        help = "Credential store encryption mode: auto, none, or keyed"
    )]
    pub credential_store_encryption: CliCredentialStoreEncryptionMode,

    #[arg(
        long,
        env = "TAU_SYSTEM_PROMPT",
        default_value = "You are a focused coding assistant. Prefer concrete steps and safe edits.",
        help = "System prompt"
    )]
    pub system_prompt: String,

    #[arg(
        long,
        env = "TAU_SYSTEM_PROMPT_FILE",
        help = "Load system prompt from a UTF-8 text file (overrides --system-prompt)"
    )]
    pub system_prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_SKILLS_DIR",
        default_value = ".tau/skills",
        help = "Directory containing skill markdown files"
    )]
    pub skills_dir: PathBuf,

    #[arg(
        long = "skill",
        env = "TAU_SKILL",
        value_delimiter = ',',
        help = "Skill name(s) to include in the system prompt"
    )]
    pub skills: Vec<String>,

    #[arg(
        long = "install-skill",
        env = "TAU_INSTALL_SKILL",
        value_delimiter = ',',
        help = "Skill markdown file(s) to install into --skills-dir before startup"
    )]
    pub install_skill: Vec<PathBuf>,

    #[arg(
        long = "install-skill-url",
        env = "TAU_INSTALL_SKILL_URL",
        value_delimiter = ',',
        help = "Skill URL(s) to install into --skills-dir before startup"
    )]
    pub install_skill_url: Vec<String>,

    #[arg(
        long = "install-skill-sha256",
        env = "TAU_INSTALL_SKILL_SHA256",
        value_delimiter = ',',
        help = "Optional sha256 value(s) matching --install-skill-url entries"
    )]
    pub install_skill_sha256: Vec<String>,

    #[arg(
        long = "skill-registry-url",
        env = "TAU_SKILL_REGISTRY_URL",
        help = "Remote registry manifest URL for skills"
    )]
    pub skill_registry_url: Option<String>,

    #[arg(
        long = "skill-registry-sha256",
        env = "TAU_SKILL_REGISTRY_SHA256",
        help = "Optional sha256 checksum for the registry manifest"
    )]
    pub skill_registry_sha256: Option<String>,

    #[arg(
        long = "install-skill-from-registry",
        env = "TAU_INSTALL_SKILL_FROM_REGISTRY",
        value_delimiter = ',',
        help = "Skill name(s) to install from the remote registry"
    )]
    pub install_skill_from_registry: Vec<String>,

    #[arg(
        long = "skills-cache-dir",
        env = "TAU_SKILLS_CACHE_DIR",
        help = "Cache directory for downloaded registry manifests and remote skill artifacts (defaults to <skills-dir>/.cache)"
    )]
    pub skills_cache_dir: Option<PathBuf>,

    #[arg(
        long = "skills-offline",
        env = "TAU_SKILLS_OFFLINE",
        default_value_t = false,
        help = "Disable network fetches for remote/registry skills and require cache hits"
    )]
    pub skills_offline: bool,

    #[arg(
        long = "skill-trust-root",
        env = "TAU_SKILL_TRUST_ROOT",
        value_delimiter = ',',
        help = "Trusted root key(s) for skill signature verification in key_id=base64_public_key format"
    )]
    pub skill_trust_root: Vec<String>,

    #[arg(
        long = "skill-trust-root-file",
        env = "TAU_SKILL_TRUST_ROOT_FILE",
        help = "JSON file containing trusted root keys for skill signature verification"
    )]
    pub skill_trust_root_file: Option<PathBuf>,

    #[arg(
        long = "skill-trust-add",
        env = "TAU_SKILL_TRUST_ADD",
        value_delimiter = ',',
        help = "Add or update trusted key(s) in --skill-trust-root-file (key_id=base64_public_key)"
    )]
    pub skill_trust_add: Vec<String>,

    #[arg(
        long = "skill-trust-revoke",
        env = "TAU_SKILL_TRUST_REVOKE",
        value_delimiter = ',',
        help = "Revoke trusted key id(s) in --skill-trust-root-file"
    )]
    pub skill_trust_revoke: Vec<String>,

    #[arg(
        long = "skill-trust-rotate",
        env = "TAU_SKILL_TRUST_ROTATE",
        value_delimiter = ',',
        help = "Rotate trusted key(s) in --skill-trust-root-file using old_id:new_id=base64_public_key"
    )]
    pub skill_trust_rotate: Vec<String>,

    #[arg(
        long = "require-signed-skills",
        env = "TAU_REQUIRE_SIGNED_SKILLS",
        default_value_t = false,
        help = "Require selected registry skills to provide signature metadata and validate against trusted roots"
    )]
    pub require_signed_skills: bool,

    #[arg(
        long = "require-signed-packages",
        env = "TAU_REQUIRE_SIGNED_PACKAGES",
        default_value_t = false,
        help = "Require package manifests to provide signing metadata and validate against trusted roots"
    )]
    pub require_signed_packages: bool,

    #[arg(
        long = "skills-lock-file",
        env = "TAU_SKILLS_LOCK_FILE",
        help = "Path to skills lockfile (defaults to <skills-dir>/skills.lock.json)"
    )]
    pub skills_lock_file: Option<PathBuf>,

    #[arg(
        long = "skills-lock-write",
        env = "TAU_SKILLS_LOCK_WRITE",
        default_value_t = false,
        help = "Write/update skills lockfile from the current installed skills"
    )]
    pub skills_lock_write: bool,

    #[arg(
        long = "skills-sync",
        env = "TAU_SKILLS_SYNC",
        default_value_t = false,
        help = "Verify installed skills match the lockfile and fail on drift"
    )]
    pub skills_sync: bool,

    #[arg(long, env = "TAU_MAX_TURNS", default_value_t = 8)]
    pub max_turns: usize,

    #[arg(
        long = "agent-max-parallel-tool-calls",
        env = "TAU_AGENT_MAX_PARALLEL_TOOL_CALLS",
        default_value_t = 4,
        value_parser = parse_positive_usize,
        help = "Maximum number of tool calls executed concurrently within one assistant turn"
    )]
    pub agent_max_parallel_tool_calls: usize,

    #[arg(
        long = "agent-max-context-messages",
        env = "TAU_AGENT_MAX_CONTEXT_MESSAGES",
        value_parser = parse_positive_usize,
        help = "Optional rolling message window sent to the model per turn (pins system message when possible)"
    )]
    pub agent_max_context_messages: Option<usize>,

    #[arg(
        long = "agent-request-max-retries",
        env = "TAU_AGENT_REQUEST_MAX_RETRIES",
        default_value_t = 2,
        help = "Maximum agent-level retries for retryable model request failures"
    )]
    pub agent_request_max_retries: usize,

    #[arg(
        long = "agent-request-retry-initial-backoff-ms",
        env = "TAU_AGENT_REQUEST_RETRY_INITIAL_BACKOFF_MS",
        default_value_t = 200,
        value_parser = parse_positive_u64,
        help = "Initial backoff delay in milliseconds for agent-level retryable request failures"
    )]
    pub agent_request_retry_initial_backoff_ms: u64,

    #[arg(
        long = "agent-request-retry-max-backoff-ms",
        env = "TAU_AGENT_REQUEST_RETRY_MAX_BACKOFF_MS",
        default_value_t = 2_000,
        value_parser = parse_positive_u64,
        help = "Maximum backoff delay in milliseconds for agent-level retryable request failures"
    )]
    pub agent_request_retry_max_backoff_ms: u64,

    #[arg(
        long = "agent-cost-budget-usd",
        env = "TAU_AGENT_COST_BUDGET_USD",
        value_parser = parse_positive_f64,
        help = "Optional cumulative estimated model cost budget in USD that triggers alert events at configured thresholds"
    )]
    pub agent_cost_budget_usd: Option<f64>,

    #[arg(
        long = "agent-cost-alert-threshold-percent",
        env = "TAU_AGENT_COST_ALERT_THRESHOLD_PERCENT",
        value_delimiter = ',',
        value_parser = parse_threshold_percent,
        default_values_t = [80_u8, 100_u8],
        help = "Comma-delimited budget alert thresholds as percentages (1-100)"
    )]
    pub agent_cost_alert_threshold_percent: Vec<u8>,

    #[arg(
        long = "prompt-sanitizer-enabled",
        env = "TAU_PROMPT_SANITIZER_ENABLED",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Enable prompt/tool-output safety checks before model dispatch and tool-result reinjection"
    )]
    pub prompt_sanitizer_enabled: bool,

    #[arg(
        long = "prompt-sanitizer-mode",
        env = "TAU_PROMPT_SANITIZER_MODE",
        value_enum,
        default_value_t = CliPromptSanitizerMode::Warn,
        help = "Safety action for matched prompt-injection patterns (warn, redact, block)"
    )]
    pub prompt_sanitizer_mode: CliPromptSanitizerMode,

    #[arg(
        long = "prompt-sanitizer-redaction-token",
        env = "TAU_PROMPT_SANITIZER_REDACTION_TOKEN",
        default_value = "[TAU-SAFETY-REDACTED]",
        help = "Replacement token used when --prompt-sanitizer-mode=redact"
    )]
    pub prompt_sanitizer_redaction_token: String,

    #[arg(
        long = "secret-leak-detector-enabled",
        env = "TAU_SECRET_LEAK_DETECTOR_ENABLED",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Enable secret-leak detection for tool results and outbound request payloads"
    )]
    pub secret_leak_detector_enabled: bool,

    #[arg(
        long = "secret-leak-detector-mode",
        env = "TAU_SECRET_LEAK_DETECTOR_MODE",
        value_enum,
        default_value_t = CliPromptSanitizerMode::Warn,
        help = "Action for detected secret leaks (warn, redact, block)"
    )]
    pub secret_leak_detector_mode: CliPromptSanitizerMode,

    #[arg(
        long = "secret-leak-redaction-token",
        env = "TAU_SECRET_LEAK_REDACTION_TOKEN",
        default_value = "[TAU-SECRET-REDACTED]",
        help = "Replacement token used when --secret-leak-detector-mode=redact"
    )]
    pub secret_leak_redaction_token: String,

    #[arg(
        long,
        env = "TAU_REQUEST_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "HTTP request timeout for provider API calls in milliseconds"
    )]
    pub request_timeout_ms: u64,

    #[arg(
        long,
        env = "TAU_PROVIDER_MAX_RETRIES",
        default_value_t = 2,
        help = "Maximum retry attempts for retryable provider HTTP failures"
    )]
    pub provider_max_retries: usize,

    #[arg(
        long,
        env = "TAU_PROVIDER_RETRY_BUDGET_MS",
        default_value_t = 0,
        help = "Optional cumulative retry backoff budget in milliseconds (0 disables budget)"
    )]
    pub provider_retry_budget_ms: u64,

    #[arg(
        long,
        env = "TAU_PROVIDER_RETRY_JITTER",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Enable bounded jitter for provider retry backoff delays"
    )]
    pub provider_retry_jitter: bool,

    #[arg(
        long = "provider-rate-limit-capacity",
        env = "TAU_PROVIDER_RATE_LIMIT_CAPACITY",
        default_value_t = 0,
        help = "Outbound provider token-bucket capacity (0 disables provider rate limiting)"
    )]
    pub provider_rate_limit_capacity: u32,

    #[arg(
        long = "provider-rate-limit-refill-per-second",
        env = "TAU_PROVIDER_RATE_LIMIT_REFILL_PER_SECOND",
        default_value_t = 0,
        help = "Outbound provider token refill rate per second (0 disables provider rate limiting)"
    )]
    pub provider_rate_limit_refill_per_second: u32,

    #[arg(
        long = "provider-rate-limit-max-wait-ms",
        env = "TAU_PROVIDER_RATE_LIMIT_MAX_WAIT_MS",
        default_value_t = 0,
        help = "Maximum wait budget per outbound provider request when rate-limited (0 fails closed immediately)"
    )]
    pub provider_rate_limit_max_wait_ms: u64,

    #[arg(
        long,
        env = "TAU_TURN_TIMEOUT_MS",
        default_value_t = 0,
        help = "Optional per-prompt timeout in milliseconds (0 disables timeout)"
    )]
    pub turn_timeout_ms: u64,

    #[arg(long, help = "Print agent lifecycle events as JSON")]
    pub json_events: bool,

    #[arg(
        long,
        env = "TAU_STREAM_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Render assistant text output token-by-token"
    )]
    pub stream_output: bool,

    #[arg(
        long,
        env = "TAU_STREAM_DELAY_MS",
        default_value_t = 0,
        help = "Delay between streamed output chunks in milliseconds"
    )]
    pub stream_delay_ms: u64,

    #[arg(long, help = "Run one prompt and exit")]
    pub prompt: Option<String>,

    #[arg(
        long = "orchestrator-mode",
        env = "TAU_ORCHESTRATOR_MODE",
        value_enum,
        default_value_t = CliOrchestratorMode::Off,
        help = "Optional orchestration mode for prompt execution"
    )]
    pub orchestrator_mode: CliOrchestratorMode,

    #[arg(
        long = "orchestrator-max-plan-steps",
        env = "TAU_ORCHESTRATOR_MAX_PLAN_STEPS",
        default_value_t = 8,
        help = "Maximum planner step count allowed in plan-first orchestrator mode"
    )]
    pub orchestrator_max_plan_steps: usize,

    #[arg(
        long = "orchestrator-max-delegated-steps",
        env = "TAU_ORCHESTRATOR_MAX_DELEGATED_STEPS",
        default_value_t = 8,
        value_parser = parse_positive_usize,
        help = "Maximum delegated step count allowed when --orchestrator-delegate-steps is enabled"
    )]
    pub orchestrator_max_delegated_steps: usize,

    #[arg(
        long = "orchestrator-max-executor-response-chars",
        env = "TAU_ORCHESTRATOR_MAX_EXECUTOR_RESPONSE_CHARS",
        default_value_t = 20000,
        value_parser = parse_positive_usize,
        help = "Maximum executor response length (characters) allowed in plan-first orchestrator mode"
    )]
    pub orchestrator_max_executor_response_chars: usize,

    #[arg(
        long = "orchestrator-max-delegated-step-response-chars",
        env = "TAU_ORCHESTRATOR_MAX_DELEGATED_STEP_RESPONSE_CHARS",
        default_value_t = 20000,
        value_parser = parse_positive_usize,
        help = "Maximum delegated step response length (characters) allowed when --orchestrator-delegate-steps is enabled"
    )]
    pub orchestrator_max_delegated_step_response_chars: usize,

    #[arg(
        long = "orchestrator-max-delegated-total-response-chars",
        env = "TAU_ORCHESTRATOR_MAX_DELEGATED_TOTAL_RESPONSE_CHARS",
        default_value_t = 160000,
        value_parser = parse_positive_usize,
        help = "Maximum cumulative delegated response length (characters) allowed when --orchestrator-delegate-steps is enabled"
    )]
    pub orchestrator_max_delegated_total_response_chars: usize,

    #[arg(
        long = "orchestrator-delegate-steps",
        env = "TAU_ORCHESTRATOR_DELEGATE_STEPS",
        default_value_t = false,
        help = "Enable delegated step execution and final consolidation in plan-first orchestrator mode"
    )]
    pub orchestrator_delegate_steps: bool,

    #[arg(
        long = "orchestrator-route-table",
        env = "TAU_ORCHESTRATOR_ROUTE_TABLE",
        value_name = "path",
        help = "Optional JSON route-table path for multi-agent planner/delegated/review role routing"
    )]
    pub orchestrator_route_table: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_PROMPT_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_template_file",
        help = "Read one prompt from a UTF-8 text file and exit"
    )]
    pub prompt_file: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_PROMPT_TEMPLATE_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_file",
        help = "Read one prompt template from a UTF-8 text file and render placeholders like {{name}} before executing"
    )]
    pub prompt_template_file: Option<PathBuf>,

    #[arg(
        long = "prompt-template-var",
        value_name = "key=value",
        requires = "prompt_template_file",
        help = "Template variable assignment for --prompt-template-file (repeatable)"
    )]
    pub prompt_template_var: Vec<String>,

    #[arg(
        long,
        env = "TAU_COMMAND_FILE",
        conflicts_with = "prompt",
        conflicts_with = "prompt_file",
        conflicts_with = "prompt_template_file",
        help = "Execute slash commands from a UTF-8 file and exit"
    )]
    pub command_file: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_COMMAND_FILE_ERROR_MODE",
        value_enum,
        default_value = "fail-fast",
        requires = "command_file",
        help = "Behavior when command-file execution hits malformed or failing commands"
    )]
    pub command_file_error_mode: CliCommandFileErrorMode,

    #[arg(
        long,
        env = "TAU_ONBOARD",
        default_value_t = false,
        help = "Run onboarding wizard and bootstrap Tau workspace assets, then exit"
    )]
    pub onboard: bool,

    #[arg(
        long = "onboard-non-interactive",
        env = "TAU_ONBOARD_NON_INTERACTIVE",
        default_value_t = false,
        requires = "onboard",
        help = "Disable interactive onboarding prompts and apply deterministic defaults"
    )]
    pub onboard_non_interactive: bool,

    #[arg(
        long = "onboard-profile",
        env = "TAU_ONBOARD_PROFILE",
        default_value = "default",
        requires = "onboard",
        help = "Profile name created/updated by onboarding"
    )]
    pub onboard_profile: String,

    #[arg(
        long = "onboard-release-channel",
        env = "TAU_ONBOARD_RELEASE_CHANNEL",
        requires = "onboard",
        help = "Optional release channel initialized by onboarding (stable|beta|dev)"
    )]
    pub onboard_release_channel: Option<String>,

    #[arg(
        long = "onboard-install-daemon",
        env = "TAU_ONBOARD_INSTALL_DAEMON",
        default_value_t = false,
        requires = "onboard",
        help = "Install Tau daemon profile files during onboarding using --daemon-profile and --daemon-state-dir"
    )]
    pub onboard_install_daemon: bool,

    #[arg(
        long = "onboard-start-daemon",
        env = "TAU_ONBOARD_START_DAEMON",
        default_value_t = false,
        requires = "onboard",
        requires = "onboard_install_daemon",
        help = "Start Tau daemon lifecycle state after onboarding daemon install"
    )]
    pub onboard_start_daemon: bool,

    #[arg(
        long = "doctor-release-cache-file",
        env = "TAU_DOCTOR_RELEASE_CACHE_FILE",
        default_value = ".tau/release-lookup-cache.json",
        help = "Cache file path used by /doctor --online release metadata lookup"
    )]
    pub doctor_release_cache_file: PathBuf,

    #[arg(
        long = "doctor-release-cache-ttl-ms",
        env = "TAU_DOCTOR_RELEASE_CACHE_TTL_MS",
        default_value_t = RELEASE_LOOKUP_CACHE_TTL_MS,
        value_parser = parse_positive_u64,
        help = "Cache freshness TTL in milliseconds for /doctor --online release metadata lookups"
    )]
    pub doctor_release_cache_ttl_ms: u64,

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
    pub project_index_build: bool,

    #[arg(
        long = "project-index-query",
        env = "TAU_PROJECT_INDEX_QUERY",
        conflicts_with = "project_index_build",
        conflicts_with = "project_index_inspect",
        value_name = "query",
        help = "Query the local project index for symbol/path/token matches and exit"
    )]
    pub project_index_query: Option<String>,

    #[arg(
        long = "project-index-inspect",
        env = "TAU_PROJECT_INDEX_INSPECT",
        conflicts_with = "project_index_build",
        conflicts_with = "project_index_query",
        help = "Inspect local project index metadata and exit"
    )]
    pub project_index_inspect: bool,

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
    pub project_index_json: bool,

    #[arg(
        long = "project-index-root",
        env = "TAU_PROJECT_INDEX_ROOT",
        default_value = ".",
        help = "Workspace root directory scanned by --project-index-build and resolved by query/inspect operations"
    )]
    pub project_index_root: PathBuf,

    #[arg(
        long = "project-index-state-dir",
        env = "TAU_PROJECT_INDEX_STATE_DIR",
        default_value = ".tau/index",
        help = "Directory containing project index state artifacts"
    )]
    pub project_index_state_dir: PathBuf,

    #[arg(
        long = "project-index-limit",
        env = "TAU_PROJECT_INDEX_LIMIT",
        default_value_t = 25,
        value_parser = parse_positive_usize,
        help = "Maximum number of query results returned by --project-index-query"
    )]
    pub project_index_limit: usize,

    #[arg(
        long = "channel-store-root",
        env = "TAU_CHANNEL_STORE_ROOT",
        default_value = ".tau/channel-store",
        help = "Base directory for transport-agnostic ChannelStore data"
    )]
    pub channel_store_root: PathBuf,

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
    pub channel_store_inspect: Option<String>,

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
    pub channel_store_repair: Option<String>,

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
    pub transport_health_inspect: Option<String>,

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
    pub transport_health_json: bool,

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
    pub github_status_inspect: Option<String>,

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
    pub github_status_json: bool,

    #[arg(
        long = "operator-control-summary",
        env = "TAU_OPERATOR_CONTROL_SUMMARY",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "github_status_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "gateway_remote_profile_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        conflicts_with = "gateway_service_start",
        conflicts_with = "gateway_service_stop",
        conflicts_with = "gateway_service_status",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Inspect a unified operator control-plane summary (transports, gateway, daemon, release channel, policy posture) and exit"
    )]
    pub operator_control_summary: bool,

    #[arg(
        long = "operator-control-summary-json",
        env = "TAU_OPERATOR_CONTROL_SUMMARY_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "operator_control_summary",
        help = "Emit --operator-control-summary output as pretty JSON"
    )]
    pub operator_control_summary_json: bool,

    #[arg(
        long = "operator-control-summary-snapshot-out",
        env = "TAU_OPERATOR_CONTROL_SUMMARY_SNAPSHOT_OUT",
        value_name = "PATH",
        requires = "operator_control_summary",
        help = "Write current --operator-control-summary report as JSON snapshot to PATH"
    )]
    pub operator_control_summary_snapshot_out: Option<PathBuf>,

    #[arg(
        long = "operator-control-summary-compare",
        env = "TAU_OPERATOR_CONTROL_SUMMARY_COMPARE",
        value_name = "PATH",
        requires = "operator_control_summary",
        help = "Compare current --operator-control-summary report against a baseline snapshot JSON at PATH"
    )]
    pub operator_control_summary_compare: Option<PathBuf>,

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
    pub dashboard_status_inspect: bool,

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
    pub dashboard_status_json: bool,

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
    pub multi_channel_status_inspect: bool,

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
    pub multi_channel_status_json: bool,

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
    pub multi_channel_route_inspect_file: Option<PathBuf>,

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
    pub multi_channel_route_inspect_json: bool,

    #[arg(
        long = "multi-channel-incident-timeline",
        env = "TAU_MULTI_CHANNEL_INCIDENT_TIMELINE",
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
        help = "Build bounded multi-channel incident timeline and optional replay export from channel-store logs"
    )]
    pub multi_channel_incident_timeline: bool,

    #[arg(
        long = "multi-channel-incident-timeline-json",
        env = "TAU_MULTI_CHANNEL_INCIDENT_TIMELINE_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_incident_timeline",
        help = "Emit --multi-channel-incident-timeline output as pretty JSON"
    )]
    pub multi_channel_incident_timeline_json: bool,

    #[arg(
        long = "multi-channel-incident-start-unix-ms",
        env = "TAU_MULTI_CHANNEL_INCIDENT_START_UNIX_MS",
        requires = "multi_channel_incident_timeline",
        value_name = "unix_ms",
        help = "Lower bound (inclusive) unix timestamp in milliseconds for incident timeline filtering"
    )]
    pub multi_channel_incident_start_unix_ms: Option<u64>,

    #[arg(
        long = "multi-channel-incident-end-unix-ms",
        env = "TAU_MULTI_CHANNEL_INCIDENT_END_UNIX_MS",
        requires = "multi_channel_incident_timeline",
        value_name = "unix_ms",
        help = "Upper bound (inclusive) unix timestamp in milliseconds for incident timeline filtering"
    )]
    pub multi_channel_incident_end_unix_ms: Option<u64>,

    #[arg(
        long = "multi-channel-incident-event-limit",
        env = "TAU_MULTI_CHANNEL_INCIDENT_EVENT_LIMIT",
        requires = "multi_channel_incident_timeline",
        value_name = "count",
        help = "Maximum number of incident timeline events to include after filtering (default: 200)"
    )]
    pub multi_channel_incident_event_limit: Option<usize>,

    #[arg(
        long = "multi-channel-incident-replay-export",
        env = "TAU_MULTI_CHANNEL_INCIDENT_REPLAY_EXPORT",
        requires = "multi_channel_incident_timeline",
        value_name = "path",
        help = "Write incident replay export artifact JSON to PATH without mutating runtime state"
    )]
    pub multi_channel_incident_replay_export: Option<PathBuf>,

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
    pub multi_agent_status_inspect: bool,

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
    pub multi_agent_status_json: bool,

    #[arg(
        long = "gateway-status-inspect",
        env = "TAU_GATEWAY_STATUS_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_remote_profile_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect gateway runtime status/guardrail report and exit"
    )]
    pub gateway_status_inspect: bool,

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
    pub gateway_status_json: bool,

    #[command(flatten)]
    pub runtime_tail: CliRuntimeTailFlags,

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
    pub deployment_status_inspect: bool,

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
    pub deployment_status_json: bool,

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
    pub custom_command_status_inspect: bool,

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
    pub custom_command_status_json: bool,

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
    pub voice_status_inspect: bool,

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
    pub voice_status_json: bool,

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
    pub extension_exec_manifest: Option<PathBuf>,

    #[arg(
        long = "extension-exec-hook",
        env = "TAU_EXTENSION_EXEC_HOOK",
        requires = "extension_exec_manifest",
        value_name = "hook",
        help = "Hook name used by --extension-exec-manifest (for example run-start)"
    )]
    pub extension_exec_hook: Option<String>,

    #[arg(
        long = "extension-exec-payload-file",
        env = "TAU_EXTENSION_EXEC_PAYLOAD_FILE",
        requires = "extension_exec_manifest",
        value_name = "path",
        help = "JSON payload file for --extension-exec-manifest hook invocation"
    )]
    pub extension_exec_payload_file: Option<PathBuf>,

    #[arg(
        long = "extension-validate",
        env = "TAU_EXTENSION_VALIDATE",
        conflicts_with = "extension_exec_manifest",
        conflicts_with = "extension_list",
        conflicts_with = "extension_show",
        value_name = "path",
        help = "Validate an extension manifest JSON file and exit"
    )]
    pub extension_validate: Option<PathBuf>,

    #[arg(
        long = "extension-list",
        env = "TAU_EXTENSION_LIST",
        conflicts_with = "extension_exec_manifest",
        conflicts_with = "extension_validate",
        conflicts_with = "extension_show",
        help = "List discovered extension manifests from a root path and exit"
    )]
    pub extension_list: bool,

    #[arg(
        long = "extension-list-root",
        env = "TAU_EXTENSION_LIST_ROOT",
        default_value = ".tau/extensions",
        requires = "extension_list",
        value_name = "path",
        help = "Root directory scanned by --extension-list"
    )]
    pub extension_list_root: PathBuf,

    #[arg(
        long = "extension-show",
        env = "TAU_EXTENSION_SHOW",
        conflicts_with = "extension_exec_manifest",
        conflicts_with = "extension_list",
        conflicts_with = "extension_validate",
        value_name = "path",
        help = "Print extension manifest metadata and inventory"
    )]
    pub extension_show: Option<PathBuf>,

    #[arg(
        long = "extension-runtime-hooks",
        env = "TAU_EXTENSION_RUNTIME_HOOKS",
        default_value_t = false,
        help = "Enable runtime run-start/run-end extension hook dispatch for prompt turns"
    )]
    pub extension_runtime_hooks: bool,

    #[arg(
        long = "extension-runtime-root",
        env = "TAU_EXTENSION_RUNTIME_ROOT",
        default_value = ".tau/extensions",
        requires = "extension_runtime_hooks",
        value_name = "path",
        help = "Root directory scanned for runtime extension hooks when --extension-runtime-hooks is enabled"
    )]
    pub extension_runtime_root: PathBuf,

    #[arg(
        long = "tool-builder-enabled",
        env = "TAU_TOOL_BUILDER_ENABLED",
        default_value_t = false,
        help = "Enable the built-in tool_builder workflow for generated wasm tools"
    )]
    pub tool_builder_enabled: bool,

    #[arg(
        long = "tool-builder-output-root",
        env = "TAU_TOOL_BUILDER_OUTPUT_ROOT",
        default_value = ".tau/generated-tools",
        requires = "tool_builder_enabled",
        value_name = "path",
        help = "Root directory for generated tool build artifacts and metadata"
    )]
    pub tool_builder_output_root: PathBuf,

    #[arg(
        long = "tool-builder-extension-root",
        env = "TAU_TOOL_BUILDER_EXTENSION_ROOT",
        default_value = ".tau/extensions/generated",
        requires = "tool_builder_enabled",
        value_name = "path",
        help = "Root directory where generated extension manifests/modules are registered"
    )]
    pub tool_builder_extension_root: PathBuf,

    #[arg(
        long = "tool-builder-max-attempts",
        env = "TAU_TOOL_BUILDER_MAX_ATTEMPTS",
        default_value_t = 3,
        requires = "tool_builder_enabled",
        value_name = "count",
        help = "Maximum compile/retry attempts for tool_builder generated wasm modules"
    )]
    pub tool_builder_max_attempts: usize,

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
    pub package_validate: Option<PathBuf>,

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
    pub package_show: Option<PathBuf>,

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
    pub package_install: Option<PathBuf>,

    #[arg(
        long = "package-install-root",
        env = "TAU_PACKAGE_INSTALL_ROOT",
        default_value = ".tau/packages",
        requires = "package_install",
        value_name = "path",
        help = "Destination root for installed package bundles"
    )]
    pub package_install_root: PathBuf,

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
    pub package_update: Option<PathBuf>,

    #[arg(
        long = "package-update-root",
        env = "TAU_PACKAGE_UPDATE_ROOT",
        default_value = ".tau/packages",
        requires = "package_update",
        value_name = "path",
        help = "Destination root containing installed package bundles for update"
    )]
    pub package_update_root: PathBuf,

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
    pub package_list: bool,

    #[arg(
        long = "package-list-root",
        env = "TAU_PACKAGE_LIST_ROOT",
        default_value = ".tau/packages",
        requires = "package_list",
        value_name = "path",
        help = "Source root to scan for installed package bundles"
    )]
    pub package_list_root: PathBuf,

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
    pub package_remove: Option<String>,

    #[arg(
        long = "package-remove-root",
        env = "TAU_PACKAGE_REMOVE_ROOT",
        default_value = ".tau/packages",
        requires = "package_remove",
        value_name = "path",
        help = "Source root containing installed package bundles for removal"
    )]
    pub package_remove_root: PathBuf,

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
    pub package_rollback: Option<String>,

    #[arg(
        long = "package-rollback-root",
        env = "TAU_PACKAGE_ROLLBACK_ROOT",
        default_value = ".tau/packages",
        requires = "package_rollback",
        value_name = "path",
        help = "Source root containing installed package versions for rollback"
    )]
    pub package_rollback_root: PathBuf,

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
    pub package_conflicts: bool,

    #[arg(
        long = "package-conflicts-root",
        env = "TAU_PACKAGE_CONFLICTS_ROOT",
        default_value = ".tau/packages",
        requires = "package_conflicts",
        value_name = "path",
        help = "Source root containing installed package bundles for conflict audit"
    )]
    pub package_conflicts_root: PathBuf,

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
    pub package_activate: bool,

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
    pub package_activate_on_startup: bool,

    #[arg(
        long = "package-activate-root",
        env = "TAU_PACKAGE_ACTIVATE_ROOT",
        default_value = ".tau/packages",
        value_name = "path",
        help = "Source root containing installed package bundles for activation"
    )]
    pub package_activate_root: PathBuf,

    #[arg(
        long = "package-activate-destination",
        env = "TAU_PACKAGE_ACTIVATE_DESTINATION",
        default_value = ".tau/packages-active",
        value_name = "path",
        help = "Destination root where resolved package components are materialized"
    )]
    pub package_activate_destination: PathBuf,

    #[arg(
        long = "package-activate-conflict-policy",
        env = "TAU_PACKAGE_ACTIVATE_CONFLICT_POLICY",
        default_value = "error",
        value_name = "error|keep-first|keep-last",
        help = "Conflict strategy when multiple packages contain the same kind/path component"
    )]
    pub package_activate_conflict_policy: String,

    #[arg(
        long = "qa-loop",
        env = "TAU_QA_LOOP",
        default_value_t = false,
        help = "Run staged quality pipeline (fmt/lint/test by default) and exit"
    )]
    pub qa_loop: bool,

    #[arg(
        long = "qa-loop-config",
        env = "TAU_QA_LOOP_CONFIG",
        requires = "qa_loop",
        value_name = "path",
        help = "Optional JSON pipeline config file for --qa-loop"
    )]
    pub qa_loop_config: Option<PathBuf>,

    #[arg(
        long = "qa-loop-json",
        env = "TAU_QA_LOOP_JSON",
        requires = "qa_loop",
        default_value_t = false,
        help = "Emit qa-loop report as JSON"
    )]
    pub qa_loop_json: bool,

    #[arg(
        long = "qa-loop-stage-timeout-ms",
        env = "TAU_QA_LOOP_STAGE_TIMEOUT_MS",
        requires = "qa_loop",
        value_parser = parse_positive_u64,
        help = "Override per-stage timeout for --qa-loop in milliseconds"
    )]
    pub qa_loop_stage_timeout_ms: Option<u64>,

    #[arg(
        long = "qa-loop-retry-failures",
        env = "TAU_QA_LOOP_RETRY_FAILURES",
        requires = "qa_loop",
        help = "Override retry count for failed stages in --qa-loop"
    )]
    pub qa_loop_retry_failures: Option<usize>,

    #[arg(
        long = "qa-loop-max-output-bytes",
        env = "TAU_QA_LOOP_MAX_OUTPUT_BYTES",
        requires = "qa_loop",
        value_parser = parse_positive_usize,
        help = "Override bounded stdout/stderr bytes captured per stage in --qa-loop reports"
    )]
    pub qa_loop_max_output_bytes: Option<usize>,

    #[arg(
        long = "qa-loop-changed-file-limit",
        env = "TAU_QA_LOOP_CHANGED_FILE_LIMIT",
        requires = "qa_loop",
        value_parser = parse_positive_usize,
        help = "Override maximum changed files included in --qa-loop git summary"
    )]
    pub qa_loop_changed_file_limit: Option<usize>,

    #[arg(
        long = "prompt-optimization-config",
        env = "TAU_PROMPT_OPTIMIZATION_CONFIG",
        value_name = "path",
        help = "Run rollout prompt-optimization mode from JSON config and exit"
    )]
    pub prompt_optimization_config: Option<PathBuf>,

    #[arg(
        long = "prompt-optimization-store-sqlite",
        env = "TAU_PROMPT_OPTIMIZATION_STORE_SQLITE",
        requires = "prompt_optimization_config",
        default_value = ".tau/training/store.sqlite",
        value_name = "path",
        help = "SQLite file path for durable rollout/tracing state in prompt-optimization mode"
    )]
    pub prompt_optimization_store_sqlite: PathBuf,

    #[arg(
        long = "prompt-optimization-json",
        env = "TAU_PROMPT_OPTIMIZATION_JSON",
        requires = "prompt_optimization_config",
        default_value_t = false,
        help = "Emit prompt-optimization summary output as JSON"
    )]
    pub prompt_optimization_json: bool,

    #[arg(
        long = "prompt-optimization-proxy-server",
        env = "TAU_PROMPT_OPTIMIZATION_PROXY_SERVER",
        default_value_t = false,
        help = "Run OpenAI-compatible prompt-optimization attribution proxy mode and exit"
    )]
    pub prompt_optimization_proxy_server: bool,

    #[arg(
        long = "prompt-optimization-proxy-bind",
        env = "TAU_PROMPT_OPTIMIZATION_PROXY_BIND",
        requires = "prompt_optimization_proxy_server",
        default_value = "127.0.0.1:8788",
        value_name = "host:port",
        help = "Bind address for prompt-optimization proxy mode"
    )]
    pub prompt_optimization_proxy_bind: String,

    #[arg(
        long = "prompt-optimization-proxy-upstream-url",
        env = "TAU_PROMPT_OPTIMIZATION_PROXY_UPSTREAM_URL",
        requires = "prompt_optimization_proxy_server",
        value_name = "url",
        help = "Upstream OpenAI-compatible base URL used by prompt-optimization proxy forwarding"
    )]
    pub prompt_optimization_proxy_upstream_url: Option<String>,

    #[arg(
        long = "prompt-optimization-proxy-state-dir",
        env = "TAU_PROMPT_OPTIMIZATION_PROXY_STATE_DIR",
        requires = "prompt_optimization_proxy_server",
        default_value = ".tau",
        value_name = "path",
        help = "State root for prompt-optimization proxy attribution logs"
    )]
    pub prompt_optimization_proxy_state_dir: PathBuf,

    #[arg(
        long = "prompt-optimization-proxy-timeout-ms",
        env = "TAU_PROMPT_OPTIMIZATION_PROXY_TIMEOUT_MS",
        requires = "prompt_optimization_proxy_server",
        default_value_t = 30_000,
        value_parser = parse_positive_u64,
        help = "Upstream request timeout in milliseconds for prompt-optimization proxy mode"
    )]
    pub prompt_optimization_proxy_timeout_ms: u64,

    #[arg(
        long = "prompt-optimization-control-status",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_STATUS",
        default_value_t = false,
        help = "Show prompt-optimization lifecycle control status and exit"
    )]
    pub prompt_optimization_control_status: bool,

    #[arg(
        long = "prompt-optimization-control-pause",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_PAUSE",
        default_value_t = false,
        conflicts_with = "prompt_optimization_control_status",
        conflicts_with = "prompt_optimization_control_resume",
        conflicts_with = "prompt_optimization_control_cancel",
        conflicts_with = "prompt_optimization_control_rollback",
        help = "Request pause for prompt-optimization lifecycle control and exit"
    )]
    pub prompt_optimization_control_pause: bool,

    #[arg(
        long = "prompt-optimization-control-resume",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_RESUME",
        default_value_t = false,
        conflicts_with = "prompt_optimization_control_status",
        conflicts_with = "prompt_optimization_control_pause",
        conflicts_with = "prompt_optimization_control_cancel",
        conflicts_with = "prompt_optimization_control_rollback",
        help = "Request resume for prompt-optimization lifecycle control and exit"
    )]
    pub prompt_optimization_control_resume: bool,

    #[arg(
        long = "prompt-optimization-control-cancel",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_CANCEL",
        default_value_t = false,
        conflicts_with = "prompt_optimization_control_status",
        conflicts_with = "prompt_optimization_control_pause",
        conflicts_with = "prompt_optimization_control_resume",
        conflicts_with = "prompt_optimization_control_rollback",
        help = "Request cancel for prompt-optimization lifecycle control and exit"
    )]
    pub prompt_optimization_control_cancel: bool,

    #[arg(
        long = "prompt-optimization-control-rollback",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_ROLLBACK",
        value_name = "path",
        conflicts_with = "prompt_optimization_control_status",
        conflicts_with = "prompt_optimization_control_pause",
        conflicts_with = "prompt_optimization_control_resume",
        conflicts_with = "prompt_optimization_control_cancel",
        help = "Request rollback lifecycle action to a checkpoint payload path and exit"
    )]
    pub prompt_optimization_control_rollback: Option<PathBuf>,

    #[arg(
        long = "prompt-optimization-control-state-dir",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_STATE_DIR",
        default_value = ".tau/training",
        value_name = "path",
        help = "State directory for prompt-optimization lifecycle control state and audit artifacts"
    )]
    pub prompt_optimization_control_state_dir: PathBuf,

    #[arg(
        long = "prompt-optimization-control-json",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_JSON",
        default_value_t = false,
        help = "Render prompt-optimization lifecycle control output as JSON"
    )]
    pub prompt_optimization_control_json: bool,

    #[arg(
        long = "prompt-optimization-control-principal",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_PRINCIPAL",
        value_name = "principal",
        help = "Principal override used for lifecycle control RBAC checks (defaults to local principal)"
    )]
    pub prompt_optimization_control_principal: Option<String>,

    #[arg(
        long = "prompt-optimization-control-rbac-policy",
        env = "TAU_PROMPT_OPTIMIZATION_CONTROL_RBAC_POLICY",
        default_value = ".tau/security/rbac.json",
        value_name = "path",
        help = "RBAC policy path for prompt-optimization lifecycle control authorization"
    )]
    pub prompt_optimization_control_rbac_policy: PathBuf,

    #[arg(
        long = "mcp-server",
        env = "TAU_MCP_SERVER",
        default_value_t = false,
        conflicts_with = "mcp_client",
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
    pub mcp_server: bool,

    #[arg(
        long = "mcp-client",
        env = "TAU_MCP_CLIENT",
        default_value_t = false,
        conflicts_with = "mcp_server",
        help = "Enable MCP client mode and register external MCP tools for the local agent runtime"
    )]
    pub mcp_client: bool,

    #[arg(
        long = "mcp-client-inspect",
        env = "TAU_MCP_CLIENT_INSPECT",
        default_value_t = false,
        requires = "mcp_client",
        help = "Run MCP client discovery/diagnostics and exit"
    )]
    pub mcp_client_inspect: bool,

    #[arg(
        long = "mcp-client-inspect-json",
        env = "TAU_MCP_CLIENT_INSPECT_JSON",
        default_value_t = false,
        requires = "mcp_client_inspect",
        help = "Render MCP client inspect output as JSON"
    )]
    pub mcp_client_inspect_json: bool,

    #[arg(
        long = "mcp-external-server-config",
        env = "TAU_MCP_EXTERNAL_SERVER_CONFIG",
        value_name = "path",
        help = "External MCP server/client config JSON used for MCP server passthrough and MCP client discovery/registration"
    )]
    pub mcp_external_server_config: Option<PathBuf>,

    #[arg(
        long = "mcp-context-provider",
        env = "TAU_MCP_CONTEXT_PROVIDER",
        value_name = "name",
        requires = "mcp_server",
        action = ArgAction::Append,
        help = "Enable MCP context providers (repeatable): session, skills, channel-store"
    )]
    pub mcp_context_provider: Vec<String>,

    #[arg(
        long = "rpc-capabilities",
        env = "TAU_RPC_CAPABILITIES",
        default_value_t = false,
        help = "Print versioned RPC protocol capabilities JSON and exit"
    )]
    pub rpc_capabilities: bool,

    #[arg(
        long = "rpc-validate-frame-file",
        env = "TAU_RPC_VALIDATE_FRAME_FILE",
        value_name = "path",
        help = "Validate one RPC frame JSON file and exit"
    )]
    pub rpc_validate_frame_file: Option<PathBuf>,

    #[arg(
        long = "rpc-dispatch-frame-file",
        env = "TAU_RPC_DISPATCH_FRAME_FILE",
        value_name = "path",
        help = "Dispatch one RPC request frame JSON file and print a response frame JSON"
    )]
    pub rpc_dispatch_frame_file: Option<PathBuf>,

    #[arg(
        long = "rpc-dispatch-ndjson-file",
        env = "TAU_RPC_DISPATCH_NDJSON_FILE",
        value_name = "path",
        help = "Dispatch newline-delimited RPC request frames and print one response JSON line per frame"
    )]
    pub rpc_dispatch_ndjson_file: Option<PathBuf>,

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
    pub rpc_serve_ndjson: bool,

    #[command(flatten)]
    pub execution_domain: CliExecutionDomainFlags,
}

impl std::ops::Deref for Cli {
    type Target = CliRuntimeTailFlags;

    fn deref(&self) -> &Self::Target {
        &self.runtime_tail
    }
}

impl std::ops::DerefMut for Cli {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime_tail
    }
}
