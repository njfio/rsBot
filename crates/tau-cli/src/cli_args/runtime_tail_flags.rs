use std::path::PathBuf;

use clap::{ArgAction, Args};

use super::{parse_positive_u64, parse_positive_usize, CliGatewayDaemonFlags};
use crate::{
    CliDeploymentWasmBrowserDidMethod, CliDeploymentWasmRuntimeProfile,
    CliGatewayOpenResponsesAuthMode, CliGatewayRemoteProfile, CliMultiChannelLiveConnectorMode,
    CliMultiChannelOutboundMode, CliMultiChannelTransport, CliWebhookSignatureAlgorithm,
};

/// Tail runtime flags (gateway/daemon flatten + custom-command, voice, GitHub bridge).
#[derive(Debug, Args)]
pub struct CliRuntimeTailFlags {
    #[command(flatten)]
    pub gateway_daemon: CliGatewayDaemonFlags,

    #[arg(
        long = "custom-command-contract-runner",
        env = "TAU_CUSTOM_COMMAND_CONTRACT_RUNNER",
        default_value_t = false,
        hide = true,
        help = "Deprecated: fixture-driven no-code custom command contract runner (removed)"
    )]
    pub custom_command_contract_runner: bool,

    #[arg(
        long = "custom-command-fixture",
        env = "TAU_CUSTOM_COMMAND_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/custom-command-contract/mixed-outcomes.json",
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: no-code custom command runtime contract fixture JSON"
    )]
    pub custom_command_fixture: PathBuf,

    #[arg(
        long = "custom-command-state-dir",
        env = "TAU_CUSTOM_COMMAND_STATE_DIR",
        default_value = ".tau/custom-command",
        help = "Directory for no-code custom command runtime state and channel-store outputs"
    )]
    pub custom_command_state_dir: PathBuf,

    #[arg(
        long = "custom-command-queue-limit",
        env = "TAU_CUSTOM_COMMAND_QUEUE_LIMIT",
        default_value_t = 64,
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: no-code custom command contract runner queue limit"
    )]
    pub custom_command_queue_limit: usize,

    #[arg(
        long = "custom-command-processed-case-cap",
        env = "TAU_CUSTOM_COMMAND_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: no-code custom command contract runner processed-case cap"
    )]
    pub custom_command_processed_case_cap: usize,

    #[arg(
        long = "custom-command-retry-max-attempts",
        env = "TAU_CUSTOM_COMMAND_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: no-code custom command contract runner retry max attempts"
    )]
    pub custom_command_retry_max_attempts: usize,

    #[arg(
        long = "custom-command-retry-base-delay-ms",
        env = "TAU_CUSTOM_COMMAND_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: no-code custom command contract runner retry base delay in milliseconds"
    )]
    pub custom_command_retry_base_delay_ms: u64,

    #[arg(
        long = "custom-command-policy-require-approval",
        env = "TAU_CUSTOM_COMMAND_POLICY_REQUIRE_APPROVAL",
        default_value_t = true,
        hide = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "custom_command_contract_runner",
        help = "Deprecated: require approval gate for custom-command contract runner RUN operations"
    )]
    pub custom_command_policy_require_approval: bool,

    #[arg(
        long = "custom-command-policy-allow-shell",
        env = "TAU_CUSTOM_COMMAND_POLICY_ALLOW_SHELL",
        default_value_t = false,
        hide = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "custom_command_contract_runner",
        help = "Deprecated: allow shell control operators in custom-command contract templates"
    )]
    pub custom_command_policy_allow_shell: bool,

    #[arg(
        long = "custom-command-policy-sandbox-profile",
        env = "TAU_CUSTOM_COMMAND_POLICY_SANDBOX_PROFILE",
        default_value = "restricted",
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: sandbox profile for custom-command contract runner policy"
    )]
    pub custom_command_policy_sandbox_profile: String,

    #[arg(
        long = "custom-command-policy-allowed-env",
        env = "TAU_CUSTOM_COMMAND_POLICY_ALLOWED_ENV",
        value_delimiter = ',',
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: allowlist of template/env keys for custom-command contract runner policy"
    )]
    pub custom_command_policy_allowed_env: Vec<String>,

    #[arg(
        long = "custom-command-policy-denied-env",
        env = "TAU_CUSTOM_COMMAND_POLICY_DENIED_ENV",
        value_delimiter = ',',
        hide = true,
        requires = "custom_command_contract_runner",
        help = "Deprecated: denylist override for custom-command contract runner policy"
    )]
    pub custom_command_policy_denied_env: Vec<String>,

    #[arg(
        long = "voice-contract-runner",
        env = "TAU_VOICE_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven voice interaction and wake-word runtime contract scenarios"
    )]
    pub voice_contract_runner: bool,

    #[arg(
        long = "voice-fixture",
        env = "TAU_VOICE_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/voice-contract/mixed-outcomes.json",
        requires = "voice_contract_runner",
        help = "Path to voice interaction and wake-word runtime contract fixture JSON"
    )]
    pub voice_fixture: PathBuf,

    #[arg(
        long = "voice-state-dir",
        env = "TAU_VOICE_STATE_DIR",
        default_value = ".tau/voice",
        help = "Directory for voice runtime state and channel-store outputs"
    )]
    pub voice_state_dir: PathBuf,

    #[arg(
        long = "voice-queue-limit",
        env = "TAU_VOICE_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "voice_contract_runner",
        help = "Maximum voice fixture cases processed per runtime cycle"
    )]
    pub voice_queue_limit: usize,

    #[arg(
        long = "voice-processed-case-cap",
        env = "TAU_VOICE_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "voice_contract_runner",
        help = "Maximum processed-case keys retained for voice duplicate suppression"
    )]
    pub voice_processed_case_cap: usize,

    #[arg(
        long = "voice-retry-max-attempts",
        env = "TAU_VOICE_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "voice_contract_runner",
        help = "Maximum retry attempts for transient voice runtime failures"
    )]
    pub voice_retry_max_attempts: usize,

    #[arg(
        long = "voice-retry-base-delay-ms",
        env = "TAU_VOICE_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "voice_contract_runner",
        help = "Base backoff delay in milliseconds for voice runtime retries (0 disables delay)"
    )]
    pub voice_retry_base_delay_ms: u64,

    #[arg(
        long = "voice-live-runner",
        env = "TAU_VOICE_LIVE_RUNNER",
        default_value_t = false,
        help = "Run live voice session runtime against a test audio-frame input fixture"
    )]
    pub voice_live_runner: bool,

    #[arg(
        long = "voice-live-input",
        env = "TAU_VOICE_LIVE_INPUT",
        default_value = "crates/tau-coding-agent/testdata/voice-live/single-turn.json",
        requires = "voice_live_runner",
        help = "Path to live voice runtime frame input JSON fixture"
    )]
    pub voice_live_input: PathBuf,

    #[arg(
        long = "voice-live-wake-word",
        env = "TAU_VOICE_LIVE_WAKE_WORD",
        default_value = "tau",
        requires = "voice_live_runner",
        help = "Wake word required for live voice turn extraction"
    )]
    pub voice_live_wake_word: String,

    #[arg(
        long = "voice-live-max-turns",
        env = "TAU_VOICE_LIVE_MAX_TURNS",
        default_value_t = 64,
        requires = "voice_live_runner",
        value_parser = parse_positive_usize,
        help = "Maximum voice frames consumed in one live runtime cycle"
    )]
    pub voice_live_max_turns: usize,

    #[arg(
        long = "voice-live-tts-output",
        env = "TAU_VOICE_LIVE_TTS_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "voice_live_runner",
        help = "Emit synthetic TTS output events for handled live turns"
    )]
    pub voice_live_tts_output: bool,

    #[arg(
        long = "github-issues-bridge",
        env = "TAU_GITHUB_ISSUES_BRIDGE",
        default_value_t = false,
        help = "Run as a GitHub Issues conversational transport loop instead of interactive prompt mode"
    )]
    pub github_issues_bridge: bool,

    #[arg(
        long = "github-repo",
        env = "TAU_GITHUB_REPO",
        requires = "github_issues_bridge",
        help = "GitHub repository in owner/repo format used by --github-issues-bridge"
    )]
    pub github_repo: Option<String>,

    #[arg(
        long = "github-token",
        env = "GITHUB_TOKEN",
        hide_env_values = true,
        requires = "github_issues_bridge",
        help = "GitHub token used for API access in --github-issues-bridge mode"
    )]
    pub github_token: Option<String>,

    #[arg(
        long = "github-token-id",
        env = "TAU_GITHUB_TOKEN_ID",
        requires = "github_issues_bridge",
        help = "Credential-store integration id used to resolve GitHub bridge token"
    )]
    pub github_token_id: Option<String>,

    #[arg(
        long = "github-bot-login",
        env = "TAU_GITHUB_BOT_LOGIN",
        requires = "github_issues_bridge",
        help = "Optional bot login used to ignore self-comments and identify already-replied events"
    )]
    pub github_bot_login: Option<String>,

    #[arg(
        long = "github-api-base",
        env = "TAU_GITHUB_API_BASE",
        default_value = "https://api.github.com",
        requires = "github_issues_bridge",
        help = "GitHub API base URL"
    )]
    pub github_api_base: String,

    #[arg(
        long = "github-state-dir",
        env = "TAU_GITHUB_STATE_DIR",
        default_value = ".tau/github-issues",
        help = "Directory for github bridge state/session/event logs"
    )]
    pub github_state_dir: PathBuf,

    #[arg(
        long = "github-poll-interval-seconds",
        env = "TAU_GITHUB_POLL_INTERVAL_SECONDS",
        default_value_t = 30,
        requires = "github_issues_bridge",
        help = "Polling interval in seconds for github bridge mode"
    )]
    pub github_poll_interval_seconds: u64,

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
    pub github_poll_once: bool,

    #[arg(
        long = "github-required-label",
        env = "TAU_GITHUB_REQUIRED_LABEL",
        value_delimiter = ',',
        requires = "github_issues_bridge",
        help = "Only process issues containing one of these labels (repeatable)"
    )]
    pub github_required_label: Vec<String>,

    #[arg(
        long = "github-issue-number",
        env = "TAU_GITHUB_ISSUE_NUMBER",
        value_delimiter = ',',
        value_parser = parse_positive_u64,
        requires = "github_issues_bridge",
        help = "Only process these GitHub issue numbers (repeatable)"
    )]
    pub github_issue_number: Vec<u64>,

    #[arg(
        long = "github-artifact-retention-days",
        env = "TAU_GITHUB_ARTIFACT_RETENTION_DAYS",
        default_value_t = 30,
        requires = "github_issues_bridge",
        help = "Retention window for GitHub bridge artifacts in days (0 disables expiration)"
    )]
    pub github_artifact_retention_days: u64,

    #[arg(
        long = "github-include-issue-body",
        env = "TAU_GITHUB_INCLUDE_ISSUE_BODY",
        default_value_t = false,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Treat the issue description itself as an initial conversation event"
    )]
    pub github_include_issue_body: bool,

    #[arg(
        long = "github-include-edited-comments",
        env = "TAU_GITHUB_INCLUDE_EDITED_COMMENTS",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "github_issues_bridge",
        help = "Process edited issue comments as new events (deduped by comment id + updated timestamp)"
    )]
    pub github_include_edited_comments: bool,

    #[arg(
        long = "github-processed-event-cap",
        env = "TAU_GITHUB_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "github_issues_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    pub github_processed_event_cap: usize,

    #[arg(
        long = "github-retry-max-attempts",
        env = "TAU_GITHUB_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "github_issues_bridge",
        help = "Maximum attempts for retryable github api failures (429/5xx/transport)"
    )]
    pub github_retry_max_attempts: usize,

    #[arg(
        long = "github-retry-base-delay-ms",
        env = "TAU_GITHUB_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "github_issues_bridge",
        help = "Base backoff delay in milliseconds for github api retries"
    )]
    pub github_retry_base_delay_ms: u64,
    #[arg(
        long = "events-runner",
        env = "TAU_EVENTS_RUNNER",
        default_value_t = false,
        help = "Run filesystem-backed scheduled events worker"
    )]
    pub events_runner: bool,

    #[arg(
        long = "events-dir",
        env = "TAU_EVENTS_DIR",
        default_value = ".tau/events",
        help = "Directory containing event definition JSON files"
    )]
    pub events_dir: PathBuf,

    #[arg(
        long = "events-state-path",
        env = "TAU_EVENTS_STATE_PATH",
        default_value = ".tau/events/state.json",
        help = "Persistent scheduler state path for periodic/debounce tracking and execution history"
    )]
    pub events_state_path: PathBuf,

    #[arg(
        long = "events-poll-interval-ms",
        env = "TAU_EVENTS_POLL_INTERVAL_MS",
        default_value_t = 1_000,
        requires = "events_runner",
        help = "Scheduler poll interval in milliseconds"
    )]
    pub events_poll_interval_ms: u64,

    #[arg(
        long = "events-queue-limit",
        env = "TAU_EVENTS_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "events_runner",
        help = "Maximum due events executed per poll cycle"
    )]
    pub events_queue_limit: usize,

    #[arg(
        long = "events-stale-immediate-max-age-seconds",
        env = "TAU_EVENTS_STALE_IMMEDIATE_MAX_AGE_SECONDS",
        default_value_t = 86_400,
        requires = "events_runner",
        help = "Maximum age for immediate events before they are skipped and removed (0 disables)"
    )]
    pub events_stale_immediate_max_age_seconds: u64,

    #[arg(
        long = "event-webhook-ingest-file",
        env = "TAU_EVENT_WEBHOOK_INGEST_FILE",
        value_name = "PATH",
        conflicts_with = "events_runner",
        help = "One-shot webhook ingestion: read payload file, enqueue debounced immediate event, and exit"
    )]
    pub event_webhook_ingest_file: Option<PathBuf>,

    #[arg(
        long = "event-webhook-channel",
        env = "TAU_EVENT_WEBHOOK_CHANNEL",
        requires = "event_webhook_ingest_file",
        value_name = "transport/channel_id",
        help = "Channel reference used for webhook-ingested immediate events"
    )]
    pub event_webhook_channel: Option<String>,

    #[arg(
        long = "event-webhook-actor-id",
        env = "TAU_EVENT_WEBHOOK_ACTOR_ID",
        requires = "event_webhook_ingest_file",
        value_name = "id",
        help = "Optional actor id/login used by pairing policy checks before webhook ingest"
    )]
    pub event_webhook_actor_id: Option<String>,

    #[arg(
        long = "event-webhook-prompt-prefix",
        env = "TAU_EVENT_WEBHOOK_PROMPT_PREFIX",
        default_value = "Handle webhook-triggered event.",
        requires = "event_webhook_ingest_file",
        help = "Prompt prefix prepended before webhook payload content"
    )]
    pub event_webhook_prompt_prefix: String,

    #[arg(
        long = "event-webhook-debounce-key",
        env = "TAU_EVENT_WEBHOOK_DEBOUNCE_KEY",
        requires = "event_webhook_ingest_file",
        help = "Optional debounce key shared across webhook ingestions"
    )]
    pub event_webhook_debounce_key: Option<String>,

    #[arg(
        long = "event-webhook-debounce-window-seconds",
        env = "TAU_EVENT_WEBHOOK_DEBOUNCE_WINDOW_SECONDS",
        default_value_t = 60,
        requires = "event_webhook_ingest_file",
        help = "Debounce window in seconds for repeated webhook ingestions with same key"
    )]
    pub event_webhook_debounce_window_seconds: u64,

    #[arg(
        long = "event-webhook-signature",
        env = "TAU_EVENT_WEBHOOK_SIGNATURE",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Raw webhook signature header value (for signed ingest verification)"
    )]
    pub event_webhook_signature: Option<String>,

    #[arg(
        long = "event-webhook-timestamp",
        env = "TAU_EVENT_WEBHOOK_TIMESTAMP",
        requires = "event_webhook_ingest_file",
        help = "Webhook timestamp header value used by signature algorithms that require timestamp checks"
    )]
    pub event_webhook_timestamp: Option<String>,

    #[arg(
        long = "event-webhook-secret",
        env = "TAU_EVENT_WEBHOOK_SECRET",
        requires = "event_webhook_ingest_file",
        hide_env_values = true,
        help = "Shared secret used to verify signed webhook payloads"
    )]
    pub event_webhook_secret: Option<String>,

    #[arg(
        long = "event-webhook-secret-id",
        env = "TAU_EVENT_WEBHOOK_SECRET_ID",
        requires = "event_webhook_ingest_file",
        help = "Credential-store integration id used to resolve webhook signing secret"
    )]
    pub event_webhook_secret_id: Option<String>,

    #[arg(
        long = "event-webhook-signature-algorithm",
        env = "TAU_EVENT_WEBHOOK_SIGNATURE_ALGORITHM",
        value_enum,
        requires = "event_webhook_ingest_file",
        help = "Webhook signature algorithm (github-sha256, slack-v0)"
    )]
    pub event_webhook_signature_algorithm: Option<CliWebhookSignatureAlgorithm>,

    #[arg(
        long = "event-webhook-signature-max-skew-seconds",
        env = "TAU_EVENT_WEBHOOK_SIGNATURE_MAX_SKEW_SECONDS",
        default_value_t = 300,
        requires = "event_webhook_ingest_file",
        help = "Maximum allowed webhook timestamp skew in seconds (0 disables skew checks)"
    )]
    pub event_webhook_signature_max_skew_seconds: u64,

    #[arg(
        long = "multi-channel-contract-runner",
        env = "TAU_MULTI_CHANNEL_CONTRACT_RUNNER",
        default_value_t = false,
        conflicts_with = "multi_channel_live_runner",
        help = "Run fixture-driven multi-channel runtime for Telegram/Discord/WhatsApp contracts"
    )]
    pub multi_channel_contract_runner: bool,

    #[arg(
        long = "multi-channel-live-runner",
        env = "TAU_MULTI_CHANNEL_LIVE_RUNNER",
        default_value_t = false,
        conflicts_with = "multi_channel_contract_runner",
        help = "Run live-ingress multi-channel runtime using local adapter inbox files for Telegram/Discord/WhatsApp"
    )]
    pub multi_channel_live_runner: bool,

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
    pub multi_channel_live_connectors_runner: bool,

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
    pub multi_channel_live_connectors_status: bool,

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
    pub multi_channel_live_connectors_status_json: bool,

    #[arg(
        long = "multi-channel-live-connectors-state-path",
        env = "TAU_MULTI_CHANNEL_LIVE_CONNECTORS_STATE_PATH",
        default_value = ".tau/multi-channel/live-connectors-state.json",
        help = "Path to live connector state/counter snapshot used by runner and status inspect"
    )]
    pub multi_channel_live_connectors_state_path: PathBuf,

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
    pub multi_channel_live_connectors_poll_once: bool,

    #[arg(
        long = "multi-channel-live-webhook-bind",
        env = "TAU_MULTI_CHANNEL_LIVE_WEBHOOK_BIND",
        default_value = "127.0.0.1:8788",
        requires = "multi_channel_live_connectors_runner",
        help = "Bind address for live connector webhook server when webhook connector modes are enabled"
    )]
    pub multi_channel_live_webhook_bind: String,

    #[arg(
        long = "multi-channel-telegram-ingress-mode",
        env = "TAU_MULTI_CHANNEL_TELEGRAM_INGRESS_MODE",
        value_enum,
        default_value_t = CliMultiChannelLiveConnectorMode::Disabled,
        requires = "multi_channel_live_connectors_runner",
        help = "Telegram connector mode for live connectors runner (disabled, polling, webhook)"
    )]
    pub multi_channel_telegram_ingress_mode: CliMultiChannelLiveConnectorMode,

    #[arg(
        long = "multi-channel-discord-ingress-mode",
        env = "TAU_MULTI_CHANNEL_DISCORD_INGRESS_MODE",
        value_enum,
        default_value_t = CliMultiChannelLiveConnectorMode::Disabled,
        requires = "multi_channel_live_connectors_runner",
        help = "Discord connector mode for live connectors runner (disabled, polling)"
    )]
    pub multi_channel_discord_ingress_mode: CliMultiChannelLiveConnectorMode,

    #[arg(
        long = "multi-channel-whatsapp-ingress-mode",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_INGRESS_MODE",
        value_enum,
        default_value_t = CliMultiChannelLiveConnectorMode::Disabled,
        requires = "multi_channel_live_connectors_runner",
        help = "WhatsApp connector mode for live connectors runner (disabled, webhook)"
    )]
    pub multi_channel_whatsapp_ingress_mode: CliMultiChannelLiveConnectorMode,

    #[arg(
        long = "multi-channel-discord-ingress-channel-id",
        env = "TAU_MULTI_CHANNEL_DISCORD_INGRESS_CHANNEL_ID",
        value_delimiter = ',',
        requires = "multi_channel_live_connectors_runner",
        help = "Discord channel ids polled when --multi-channel-discord-ingress-mode=polling (repeatable)"
    )]
    pub multi_channel_discord_ingress_channel_ids: Vec<String>,

    #[arg(
        long = "multi-channel-discord-ingress-guild-id",
        env = "TAU_MULTI_CHANNEL_DISCORD_INGRESS_GUILD_ID",
        value_delimiter = ',',
        requires = "multi_channel_live_connectors_runner",
        help = "Optional Discord guild ids allowlisted for polling ingress; non-matching guild messages are ignored"
    )]
    pub multi_channel_discord_ingress_guild_ids: Vec<String>,

    #[arg(
        long = "multi-channel-telegram-webhook-secret",
        env = "TAU_MULTI_CHANNEL_TELEGRAM_WEBHOOK_SECRET",
        hide_env_values = true,
        requires = "multi_channel_live_connectors_runner",
        help = "Optional Telegram webhook secret token required in X-Telegram-Bot-Api-Secret-Token"
    )]
    pub multi_channel_telegram_webhook_secret: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-webhook-verify-token",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_WEBHOOK_VERIFY_TOKEN",
        hide_env_values = true,
        requires = "multi_channel_live_connectors_runner",
        help = "Verify token used for WhatsApp webhook subscription challenge"
    )]
    pub multi_channel_whatsapp_webhook_verify_token: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-webhook-app-secret",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_WEBHOOK_APP_SECRET",
        hide_env_values = true,
        requires = "multi_channel_live_connectors_runner",
        help = "App secret used to verify X-Hub-Signature-256 for WhatsApp webhook posts"
    )]
    pub multi_channel_whatsapp_webhook_app_secret: Option<String>,

    #[arg(
        long = "multi-channel-live-ingest-file",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_FILE",
        help = "One-shot provider payload ingestion: normalize a Telegram/Discord/WhatsApp payload file into live-ingress NDJSON and exit"
    )]
    pub multi_channel_live_ingest_file: Option<PathBuf>,

    #[arg(
        long = "multi-channel-live-ingest-transport",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_TRANSPORT",
        value_enum,
        requires = "multi_channel_live_ingest_file",
        help = "Transport for --multi-channel-live-ingest-file (telegram, discord, whatsapp)"
    )]
    pub multi_channel_live_ingest_transport: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-live-ingest-provider",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_PROVIDER",
        default_value = "native-ingress",
        requires = "multi_channel_live_ingest_file",
        help = "Provider identifier recorded in normalized live-ingress envelopes"
    )]
    pub multi_channel_live_ingest_provider: String,

    #[arg(
        long = "multi-channel-live-ingest-dir",
        env = "TAU_MULTI_CHANNEL_LIVE_INGEST_DIR",
        default_value = ".tau/multi-channel/live-ingress",
        requires = "multi_channel_live_ingest_file",
        help = "Directory where one-shot live-ingest writes transport-specific NDJSON inbox files"
    )]
    pub multi_channel_live_ingest_dir: PathBuf,

    #[arg(
        long = "multi-channel-live-readiness-preflight",
        env = "TAU_MULTI_CHANNEL_LIVE_READINESS_PREFLIGHT",
        default_value_t = false,
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        help = "Run multi-channel live readiness preflight checks for Telegram/Discord/WhatsApp and exit"
    )]
    pub multi_channel_live_readiness_preflight: bool,

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
    pub multi_channel_live_readiness_json: bool,

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
    pub multi_channel_channel_status: Option<CliMultiChannelTransport>,

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
    pub multi_channel_channel_status_json: bool,

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
    pub multi_channel_channel_login: Option<CliMultiChannelTransport>,

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
    pub multi_channel_channel_login_json: bool,

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
    pub multi_channel_channel_logout: Option<CliMultiChannelTransport>,

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
    pub multi_channel_channel_logout_json: bool,

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
    pub multi_channel_channel_probe: Option<CliMultiChannelTransport>,

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
    pub multi_channel_channel_probe_json: bool,

    #[arg(
        long = "multi-channel-channel-probe-online",
        env = "TAU_MULTI_CHANNEL_CHANNEL_PROBE_ONLINE",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_channel_probe",
        help = "Enable provider API login validation for --multi-channel-channel-probe using bounded timeout/retry behavior"
    )]
    pub multi_channel_channel_probe_online: bool,

    #[arg(
        long = "multi-channel-send",
        env = "TAU_MULTI_CHANNEL_SEND",
        value_enum,
        conflicts_with = "multi_channel_route_inspect_file",
        conflicts_with = "multi_channel_contract_runner",
        conflicts_with = "multi_channel_live_runner",
        conflicts_with = "multi_channel_live_ingest_file",
        conflicts_with = "multi_channel_live_readiness_preflight",
        conflicts_with = "multi_channel_channel_status",
        conflicts_with = "multi_channel_channel_login",
        conflicts_with = "multi_channel_channel_logout",
        conflicts_with = "multi_channel_channel_probe",
        help = "Send one outbound message for one transport (telegram, discord, whatsapp) and exit"
    )]
    pub multi_channel_send: Option<CliMultiChannelTransport>,

    #[arg(
        long = "multi-channel-send-target",
        env = "TAU_MULTI_CHANNEL_SEND_TARGET",
        requires = "multi_channel_send",
        help = "Transport target identifier (telegram chat id, discord channel id, or whatsapp recipient)"
    )]
    pub multi_channel_send_target: Option<String>,

    #[arg(
        long = "multi-channel-send-text",
        env = "TAU_MULTI_CHANNEL_SEND_TEXT",
        requires = "multi_channel_send",
        conflicts_with = "multi_channel_send_text_file",
        help = "Outbound message text payload for --multi-channel-send"
    )]
    pub multi_channel_send_text: Option<String>,

    #[arg(
        long = "multi-channel-send-text-file",
        env = "TAU_MULTI_CHANNEL_SEND_TEXT_FILE",
        requires = "multi_channel_send",
        conflicts_with = "multi_channel_send_text",
        help = "Read outbound message text payload from file path for --multi-channel-send"
    )]
    pub multi_channel_send_text_file: Option<PathBuf>,

    #[arg(
        long = "multi-channel-send-json",
        env = "TAU_MULTI_CHANNEL_SEND_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "multi_channel_send",
        help = "Emit --multi-channel-send output as pretty JSON"
    )]
    pub multi_channel_send_json: bool,

    #[arg(
        long = "multi-channel-fixture",
        env = "TAU_MULTI_CHANNEL_FIXTURE",
        default_value = "crates/tau-multi-channel/testdata/multi-channel-contract/baseline-three-channel.json",
        requires = "multi_channel_contract_runner",
        help = "Path to multi-channel contract fixture JSON"
    )]
    pub multi_channel_fixture: PathBuf,

    #[arg(
        long = "multi-channel-live-ingress-dir",
        env = "TAU_MULTI_CHANNEL_LIVE_INGRESS_DIR",
        default_value = ".tau/multi-channel/live-ingress",
        requires = "multi_channel_live_runner",
        help = "Directory containing transport-specific live ingress NDJSON inbox files (telegram.ndjson, discord.ndjson, whatsapp.ndjson)"
    )]
    pub multi_channel_live_ingress_dir: PathBuf,

    #[arg(
        long = "multi-channel-state-dir",
        env = "TAU_MULTI_CHANNEL_STATE_DIR",
        default_value = ".tau/multi-channel",
        help = "Directory for multi-channel runtime state and channel-store outputs"
    )]
    pub multi_channel_state_dir: PathBuf,

    #[arg(
        long = "multi-channel-queue-limit",
        env = "TAU_MULTI_CHANNEL_QUEUE_LIMIT",
        default_value_t = 64,
        help = "Maximum inbound events processed per runtime cycle"
    )]
    pub multi_channel_queue_limit: usize,

    #[arg(
        long = "multi-channel-processed-event-cap",
        env = "TAU_MULTI_CHANNEL_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        help = "Maximum processed-event keys retained for duplicate suppression"
    )]
    pub multi_channel_processed_event_cap: usize,

    #[arg(
        long = "multi-channel-retry-max-attempts",
        env = "TAU_MULTI_CHANNEL_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        help = "Maximum retry attempts for transient multi-channel runtime failures"
    )]
    pub multi_channel_retry_max_attempts: usize,

    #[arg(
        long = "multi-channel-retry-base-delay-ms",
        env = "TAU_MULTI_CHANNEL_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        help = "Base backoff delay in milliseconds for multi-channel runtime retries (0 disables delay)"
    )]
    pub multi_channel_retry_base_delay_ms: u64,

    #[arg(
        long = "multi-channel-retry-jitter-ms",
        env = "TAU_MULTI_CHANNEL_RETRY_JITTER_MS",
        default_value_t = 0,
        help = "Deterministic jitter upper-bound in milliseconds added to multi-channel runtime retry delays (0 disables jitter)"
    )]
    pub multi_channel_retry_jitter_ms: u64,

    #[arg(
        long = "multi-channel-coalescing-window-ms",
        env = "TAU_MULTI_CHANNEL_COALESCING_WINDOW_MS",
        default_value_t = 2_500,
        help = "Coalescing window in milliseconds for batching same-conversation rapid inbound messages (0 disables coalescing)"
    )]
    pub multi_channel_coalescing_window_ms: u64,

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
    pub multi_channel_telemetry_typing_presence: bool,

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
    pub multi_channel_telemetry_usage_summary: bool,

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
    pub multi_channel_telemetry_include_identifiers: bool,

    #[arg(
        long = "multi-channel-telemetry-min-response-chars",
        env = "TAU_MULTI_CHANNEL_TELEMETRY_MIN_RESPONSE_CHARS",
        default_value_t = 120,
        value_parser = parse_positive_usize,
        help = "Minimum response length before typing/presence telemetry is emitted"
    )]
    pub multi_channel_telemetry_min_response_chars: usize,

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
    pub multi_channel_media_understanding: bool,

    #[arg(
        long = "multi-channel-media-max-attachments",
        env = "TAU_MULTI_CHANNEL_MEDIA_MAX_ATTACHMENTS",
        default_value_t = 4,
        value_parser = parse_positive_usize,
        help = "Maximum unique inbound attachments processed for media understanding per event"
    )]
    pub multi_channel_media_max_attachments: usize,

    #[arg(
        long = "multi-channel-media-max-summary-chars",
        env = "TAU_MULTI_CHANNEL_MEDIA_MAX_SUMMARY_CHARS",
        default_value_t = 280,
        value_parser = parse_positive_usize,
        help = "Maximum characters retained for each media understanding summary line"
    )]
    pub multi_channel_media_max_summary_chars: usize,

    #[arg(
        long = "multi-channel-outbound-mode",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_MODE",
        value_enum,
        default_value_t = CliMultiChannelOutboundMode::ChannelStore,
        help = "Outbound delivery mode for multi-channel runtime (channel-store, dry-run, provider)"
    )]
    pub multi_channel_outbound_mode: CliMultiChannelOutboundMode,

    #[arg(
        long = "multi-channel-outbound-max-chars",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_MAX_CHARS",
        default_value_t = 1200,
        help = "Maximum outbound response chunk size in characters before provider-safe chunk splitting"
    )]
    pub multi_channel_outbound_max_chars: usize,

    #[arg(
        long = "multi-channel-outbound-http-timeout-ms",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_HTTP_TIMEOUT_MS",
        default_value_t = 5000,
        help = "Provider HTTP timeout in milliseconds for multi-channel outbound mode=provider"
    )]
    pub multi_channel_outbound_http_timeout_ms: u64,

    #[arg(
        long = "multi-channel-outbound-ssrf-protection",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_SSRF_PROTECTION",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Enable SSRF guardrails for outbound provider URLs (scheme, DNS, and private-network checks)"
    )]
    pub multi_channel_outbound_ssrf_protection: bool,

    #[arg(
        long = "multi-channel-outbound-ssrf-allow-http",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_SSRF_ALLOW_HTTP",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Allow http:// outbound provider URLs when SSRF protection is enabled (default requires https://)"
    )]
    pub multi_channel_outbound_ssrf_allow_http: bool,

    #[arg(
        long = "multi-channel-outbound-ssrf-allow-private-network",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_SSRF_ALLOW_PRIVATE_NETWORK",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Allow loopback/private/link-local outbound targets when SSRF protection is enabled"
    )]
    pub multi_channel_outbound_ssrf_allow_private_network: bool,

    #[arg(
        long = "multi-channel-outbound-max-redirects",
        env = "TAU_MULTI_CHANNEL_OUTBOUND_MAX_REDIRECTS",
        default_value_t = 5,
        help = "Maximum provider HTTP redirects followed per outbound delivery request (0 disables redirects)"
    )]
    pub multi_channel_outbound_max_redirects: usize,

    #[arg(
        long = "multi-channel-telegram-api-base",
        env = "TAU_MULTI_CHANNEL_TELEGRAM_API_BASE",
        default_value = "https://api.telegram.org",
        help = "Telegram provider API base URL for multi-channel outbound mode=provider"
    )]
    pub multi_channel_telegram_api_base: String,

    #[arg(
        long = "multi-channel-discord-api-base",
        env = "TAU_MULTI_CHANNEL_DISCORD_API_BASE",
        default_value = "https://discord.com/api/v10",
        help = "Discord provider API base URL for multi-channel outbound mode=provider"
    )]
    pub multi_channel_discord_api_base: String,

    #[arg(
        long = "multi-channel-whatsapp-api-base",
        env = "TAU_MULTI_CHANNEL_WHATSAPP_API_BASE",
        default_value = "https://graph.facebook.com/v20.0",
        help = "WhatsApp provider API base URL for multi-channel outbound mode=provider"
    )]
    pub multi_channel_whatsapp_api_base: String,

    #[arg(
        long = "multi-channel-telegram-bot-token",
        env = "TAU_TELEGRAM_BOT_TOKEN",
        hide_env_values = true,
        help = "Telegram bot token for multi-channel outbound mode=provider"
    )]
    pub multi_channel_telegram_bot_token: Option<String>,

    #[arg(
        long = "multi-channel-discord-bot-token",
        env = "TAU_DISCORD_BOT_TOKEN",
        hide_env_values = true,
        help = "Discord bot token for multi-channel outbound mode=provider"
    )]
    pub multi_channel_discord_bot_token: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-access-token",
        env = "TAU_WHATSAPP_ACCESS_TOKEN",
        hide_env_values = true,
        help = "WhatsApp access token for multi-channel outbound mode=provider"
    )]
    pub multi_channel_whatsapp_access_token: Option<String>,

    #[arg(
        long = "multi-channel-whatsapp-phone-number-id",
        env = "TAU_WHATSAPP_PHONE_NUMBER_ID",
        help = "WhatsApp phone number id for multi-channel outbound mode=provider"
    )]
    pub multi_channel_whatsapp_phone_number_id: Option<String>,

    #[arg(
        long = "multi-agent-contract-runner",
        env = "TAU_MULTI_AGENT_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven multi-agent runtime contract scenarios"
    )]
    pub multi_agent_contract_runner: bool,

    #[arg(
        long = "multi-agent-fixture",
        env = "TAU_MULTI_AGENT_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/multi-agent-contract/mixed-outcomes.json",
        requires = "multi_agent_contract_runner",
        help = "Path to multi-agent runtime contract fixture JSON"
    )]
    pub multi_agent_fixture: PathBuf,

    #[arg(
        long = "multi-agent-state-dir",
        env = "TAU_MULTI_AGENT_STATE_DIR",
        default_value = ".tau/multi-agent",
        help = "Directory for multi-agent runtime state and channel-store outputs"
    )]
    pub multi_agent_state_dir: PathBuf,

    #[arg(
        long = "multi-agent-queue-limit",
        env = "TAU_MULTI_AGENT_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "multi_agent_contract_runner",
        help = "Maximum multi-agent fixture cases processed per runtime cycle"
    )]
    pub multi_agent_queue_limit: usize,

    #[arg(
        long = "multi-agent-processed-case-cap",
        env = "TAU_MULTI_AGENT_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "multi_agent_contract_runner",
        help = "Maximum processed-case keys retained for multi-agent duplicate suppression"
    )]
    pub multi_agent_processed_case_cap: usize,

    #[arg(
        long = "multi-agent-retry-max-attempts",
        env = "TAU_MULTI_AGENT_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "multi_agent_contract_runner",
        help = "Maximum retry attempts for transient multi-agent runtime failures"
    )]
    pub multi_agent_retry_max_attempts: usize,

    #[arg(
        long = "multi-agent-retry-base-delay-ms",
        env = "TAU_MULTI_AGENT_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "multi_agent_contract_runner",
        help = "Base backoff delay in milliseconds for multi-agent runtime retries (0 disables delay)"
    )]
    pub multi_agent_retry_base_delay_ms: u64,

    #[arg(
        long = "browser-automation-contract-runner",
        env = "TAU_BROWSER_AUTOMATION_CONTRACT_RUNNER",
        default_value_t = false,
        hide = true,
        conflicts_with_all = ["browser_automation_live_runner", "browser_automation_preflight"],
        help = "Deprecated: fixture-driven browser automation contract runner (removed)"
    )]
    pub browser_automation_contract_runner: bool,

    #[arg(
        long = "browser-automation-live-runner",
        env = "TAU_BROWSER_AUTOMATION_LIVE_RUNNER",
        default_value_t = false,
        conflicts_with_all = [
            "browser_automation_contract_runner",
            "browser_automation_preflight"
        ],
        help = "Run browser automation fixtures against a live Playwright CLI executor"
    )]
    pub browser_automation_live_runner: bool,

    #[arg(
        long = "browser-automation-live-fixture",
        env = "TAU_BROWSER_AUTOMATION_LIVE_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/browser-automation-live/live-sequence.json",
        requires = "browser_automation_live_runner",
        help = "Path to browser automation live fixture JSON"
    )]
    pub browser_automation_live_fixture: PathBuf,

    #[arg(
        long = "browser-automation-fixture",
        env = "TAU_BROWSER_AUTOMATION_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/browser-automation-contract/mixed-outcomes.json",
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation runtime contract fixture JSON"
    )]
    pub browser_automation_fixture: PathBuf,

    #[arg(
        long = "browser-automation-state-dir",
        env = "TAU_BROWSER_AUTOMATION_STATE_DIR",
        default_value = ".tau/browser-automation",
        help = "Directory for browser automation runtime state and channel-store outputs"
    )]
    pub browser_automation_state_dir: PathBuf,

    #[arg(
        long = "browser-automation-queue-limit",
        env = "TAU_BROWSER_AUTOMATION_QUEUE_LIMIT",
        default_value_t = 64,
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation contract runner queue limit"
    )]
    pub browser_automation_queue_limit: usize,

    #[arg(
        long = "browser-automation-processed-case-cap",
        env = "TAU_BROWSER_AUTOMATION_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation contract runner processed-case cap"
    )]
    pub browser_automation_processed_case_cap: usize,

    #[arg(
        long = "browser-automation-retry-max-attempts",
        env = "TAU_BROWSER_AUTOMATION_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation contract runner retry max attempts"
    )]
    pub browser_automation_retry_max_attempts: usize,

    #[arg(
        long = "browser-automation-retry-base-delay-ms",
        env = "TAU_BROWSER_AUTOMATION_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation contract runner retry base delay in milliseconds"
    )]
    pub browser_automation_retry_base_delay_ms: u64,

    #[arg(
        long = "browser-automation-action-timeout-ms",
        env = "TAU_BROWSER_AUTOMATION_ACTION_TIMEOUT_MS",
        default_value_t = 5_000,
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation contract runner action timeout in milliseconds"
    )]
    pub browser_automation_action_timeout_ms: u64,

    #[arg(
        long = "browser-automation-max-actions-per-case",
        env = "TAU_BROWSER_AUTOMATION_MAX_ACTIONS_PER_CASE",
        default_value_t = 8,
        hide = true,
        requires = "browser_automation_contract_runner",
        help = "Deprecated: browser automation contract runner max actions per case"
    )]
    pub browser_automation_max_actions_per_case: usize,

    #[arg(
        long = "browser-automation-allow-unsafe-actions",
        env = "TAU_BROWSER_AUTOMATION_ALLOW_UNSAFE_ACTIONS",
        default_value_t = false,
        hide = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "browser_automation_contract_runner",
        help = "Deprecated: allow unsafe browser automation contract fixture operations"
    )]
    pub browser_automation_allow_unsafe_actions: bool,

    #[arg(
        long = "browser-automation-playwright-cli",
        env = "TAU_BROWSER_AUTOMATION_PLAYWRIGHT_CLI",
        default_value = "playwright-cli",
        help = "Playwright CLI executable used for browser automation preflight, live runner, and doctor checks"
    )]
    pub browser_automation_playwright_cli: String,

    #[arg(
        long = "browser-automation-preflight",
        env = "TAU_BROWSER_AUTOMATION_PREFLIGHT",
        default_value_t = false,
        conflicts_with_all = [
            "browser_automation_contract_runner",
            "browser_automation_live_runner"
        ],
        help = "Run browser automation prerequisite checks and exit"
    )]
    pub browser_automation_preflight: bool,

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
    pub browser_automation_preflight_json: bool,

    #[arg(
        long = "memory-contract-runner",
        env = "TAU_MEMORY_CONTRACT_RUNNER",
        default_value_t = false,
        hide = true,
        help = "Deprecated: fixture-driven semantic memory contract runner (removed)"
    )]
    pub memory_contract_runner: bool,

    #[arg(
        long = "memory-fixture",
        env = "TAU_MEMORY_FIXTURE",
        default_value = "crates/tau-memory/testdata/memory-contract/mixed-outcomes.json",
        hide = true,
        requires = "memory_contract_runner",
        help = "Deprecated: semantic memory contract fixture JSON"
    )]
    pub memory_fixture: PathBuf,

    #[arg(
        long = "memory-state-dir",
        env = "TAU_MEMORY_STATE_DIR",
        default_value = ".tau/memory",
        help = "Directory for semantic memory transport-health inspection artifacts"
    )]
    pub memory_state_dir: PathBuf,

    #[arg(
        long = "jobs-enabled",
        env = "TAU_JOBS_ENABLED",
        default_value_t = true,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Enable built-in background job tooling (jobs_create/list/status/cancel)"
    )]
    pub jobs_enabled: bool,

    #[arg(
        long = "jobs-state-dir",
        env = "TAU_JOBS_STATE_DIR",
        default_value = ".tau/jobs",
        help = "Directory for persisted background job manifests, output logs, and runtime health state"
    )]
    pub jobs_state_dir: PathBuf,

    #[arg(
        long = "jobs-list-default-limit",
        env = "TAU_JOBS_LIST_DEFAULT_LIMIT",
        default_value_t = 20,
        value_parser = parse_positive_usize,
        help = "Default number of jobs returned by jobs_list when limit is omitted"
    )]
    pub jobs_list_default_limit: usize,

    #[arg(
        long = "jobs-list-max-limit",
        env = "TAU_JOBS_LIST_MAX_LIMIT",
        default_value_t = 200,
        value_parser = parse_positive_usize,
        help = "Maximum number of jobs returned by jobs_list"
    )]
    pub jobs_list_max_limit: usize,

    #[arg(
        long = "jobs-default-timeout-ms",
        env = "TAU_JOBS_DEFAULT_TIMEOUT_MS",
        default_value_t = 30_000,
        value_parser = parse_positive_u64,
        help = "Default timeout in milliseconds applied to jobs_create when timeout_ms is omitted"
    )]
    pub jobs_default_timeout_ms: u64,

    #[arg(
        long = "jobs-max-timeout-ms",
        env = "TAU_JOBS_MAX_TIMEOUT_MS",
        default_value_t = 900_000,
        value_parser = parse_positive_u64,
        help = "Maximum allowed timeout in milliseconds for jobs_create"
    )]
    pub jobs_max_timeout_ms: u64,

    #[arg(
        long = "memory-queue-limit",
        env = "TAU_MEMORY_QUEUE_LIMIT",
        default_value_t = 64,
        hide = true,
        requires = "memory_contract_runner",
        help = "Deprecated: memory contract runner queue limit"
    )]
    pub memory_queue_limit: usize,

    #[arg(
        long = "memory-processed-case-cap",
        env = "TAU_MEMORY_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        hide = true,
        requires = "memory_contract_runner",
        help = "Deprecated: memory contract runner processed-case cap"
    )]
    pub memory_processed_case_cap: usize,

    #[arg(
        long = "memory-retry-max-attempts",
        env = "TAU_MEMORY_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        hide = true,
        requires = "memory_contract_runner",
        help = "Deprecated: memory contract runner retry max attempts"
    )]
    pub memory_retry_max_attempts: usize,

    #[arg(
        long = "memory-retry-base-delay-ms",
        env = "TAU_MEMORY_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        hide = true,
        requires = "memory_contract_runner",
        help = "Deprecated: memory contract runner retry base delay in milliseconds"
    )]
    pub memory_retry_base_delay_ms: u64,

    #[arg(
        long = "dashboard-contract-runner",
        env = "TAU_DASHBOARD_CONTRACT_RUNNER",
        default_value_t = false,
        hide = true,
        help = "Deprecated: fixture-driven dashboard contract runner (removed)"
    )]
    pub dashboard_contract_runner: bool,

    #[arg(
        long = "dashboard-fixture",
        env = "TAU_DASHBOARD_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/dashboard-contract/mixed-outcomes.json",
        hide = true,
        requires = "dashboard_contract_runner",
        help = "Deprecated: dashboard runtime contract fixture JSON"
    )]
    pub dashboard_fixture: PathBuf,

    #[arg(
        long = "dashboard-state-dir",
        env = "TAU_DASHBOARD_STATE_DIR",
        default_value = ".tau/dashboard",
        help = "Directory for dashboard runtime state and channel-store outputs"
    )]
    pub dashboard_state_dir: PathBuf,

    #[arg(
        long = "dashboard-queue-limit",
        env = "TAU_DASHBOARD_QUEUE_LIMIT",
        default_value_t = 64,
        hide = true,
        requires = "dashboard_contract_runner",
        help = "Deprecated: dashboard contract runner queue limit"
    )]
    pub dashboard_queue_limit: usize,

    #[arg(
        long = "dashboard-processed-case-cap",
        env = "TAU_DASHBOARD_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        hide = true,
        requires = "dashboard_contract_runner",
        help = "Deprecated: dashboard contract runner processed-case cap"
    )]
    pub dashboard_processed_case_cap: usize,

    #[arg(
        long = "dashboard-retry-max-attempts",
        env = "TAU_DASHBOARD_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        hide = true,
        requires = "dashboard_contract_runner",
        help = "Deprecated: dashboard contract runner retry max attempts"
    )]
    pub dashboard_retry_max_attempts: usize,

    #[arg(
        long = "dashboard-retry-base-delay-ms",
        env = "TAU_DASHBOARD_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        hide = true,
        requires = "dashboard_contract_runner",
        help = "Deprecated: dashboard contract runner retry base delay in milliseconds"
    )]
    pub dashboard_retry_base_delay_ms: u64,

    #[arg(
        long = "gateway-openresponses-server",
        env = "TAU_GATEWAY_OPENRESPONSES_SERVER",
        default_value_t = false,
        help = "Run authenticated OpenResponses subset HTTP endpoint at POST /v1/responses"
    )]
    pub gateway_openresponses_server: bool,

    #[arg(
        long = "gateway-openresponses-bind",
        env = "TAU_GATEWAY_OPENRESPONSES_BIND",
        default_value = "127.0.0.1:8787",
        requires = "gateway_openresponses_server",
        help = "Socket address for --gateway-openresponses-server (host:port)"
    )]
    pub gateway_openresponses_bind: String,

    #[arg(
        long = "gateway-remote-profile",
        env = "TAU_GATEWAY_REMOTE_PROFILE",
        value_enum,
        default_value_t = CliGatewayRemoteProfile::LocalOnly,
        help = "Gateway remote-access posture: local-only, password-remote, proxy-remote, tailscale-serve, or tailscale-funnel"
    )]
    pub gateway_remote_profile: CliGatewayRemoteProfile,

    #[arg(
        long = "gateway-openresponses-auth-mode",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_MODE",
        value_enum,
        default_value_t = CliGatewayOpenResponsesAuthMode::Token,
        requires = "gateway_openresponses_server",
        help = "Gateway auth mode: token, password-session, or localhost-dev"
    )]
    pub gateway_openresponses_auth_mode: CliGatewayOpenResponsesAuthMode,

    #[arg(
        long = "gateway-openresponses-auth-token",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_TOKEN",
        requires = "gateway_openresponses_server",
        help = "Bearer token used when --gateway-openresponses-auth-mode=token"
    )]
    pub gateway_openresponses_auth_token: Option<String>,

    #[arg(
        long = "gateway-openresponses-auth-token-id",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_TOKEN_ID",
        requires = "gateway_openresponses_server",
        help = "Credential-store integration id used to resolve bearer token when --gateway-openresponses-auth-mode=token"
    )]
    pub gateway_openresponses_auth_token_id: Option<String>,

    #[arg(
        long = "gateway-openresponses-auth-password",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_PASSWORD",
        requires = "gateway_openresponses_server",
        help = "Password used by /gateway/auth/session when --gateway-openresponses-auth-mode=password-session"
    )]
    pub gateway_openresponses_auth_password: Option<String>,

    #[arg(
        long = "gateway-openresponses-auth-password-id",
        env = "TAU_GATEWAY_OPENRESPONSES_AUTH_PASSWORD_ID",
        requires = "gateway_openresponses_server",
        help = "Credential-store integration id used to resolve session password when --gateway-openresponses-auth-mode=password-session"
    )]
    pub gateway_openresponses_auth_password_id: Option<String>,

    #[arg(
        long = "gateway-openresponses-session-ttl-seconds",
        env = "TAU_GATEWAY_OPENRESPONSES_SESSION_TTL_SECONDS",
        default_value_t = 3600,
        value_parser = parse_positive_u64,
        requires = "gateway_openresponses_server",
        help = "Session token TTL in seconds for password-session auth mode"
    )]
    pub gateway_openresponses_session_ttl_seconds: u64,

    #[arg(
        long = "gateway-openresponses-rate-limit-window-seconds",
        env = "TAU_GATEWAY_OPENRESPONSES_RATE_LIMIT_WINDOW_SECONDS",
        default_value_t = 60,
        value_parser = parse_positive_u64,
        requires = "gateway_openresponses_server",
        help = "Rate-limit window size in seconds"
    )]
    pub gateway_openresponses_rate_limit_window_seconds: u64,

    #[arg(
        long = "gateway-openresponses-rate-limit-max-requests",
        env = "TAU_GATEWAY_OPENRESPONSES_RATE_LIMIT_MAX_REQUESTS",
        default_value_t = 120,
        value_parser = parse_positive_usize,
        requires = "gateway_openresponses_server",
        help = "Maximum accepted requests per auth principal within one rate-limit window"
    )]
    pub gateway_openresponses_rate_limit_max_requests: usize,

    #[arg(
        long = "gateway-openresponses-max-input-chars",
        env = "TAU_GATEWAY_OPENRESPONSES_MAX_INPUT_CHARS",
        default_value_t = 32_000,
        value_parser = parse_positive_usize,
        requires = "gateway_openresponses_server",
        help = "Maximum translated input size accepted by /v1/responses"
    )]
    pub gateway_openresponses_max_input_chars: usize,

    #[arg(
        long = "gateway-contract-runner",
        env = "TAU_GATEWAY_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven gateway runtime contract scenarios"
    )]
    pub gateway_contract_runner: bool,

    #[arg(
        long = "gateway-fixture",
        env = "TAU_GATEWAY_FIXTURE",
        default_value = "crates/tau-gateway/testdata/gateway-contract/mixed-outcomes.json",
        requires = "gateway_contract_runner",
        help = "Path to gateway runtime contract fixture JSON"
    )]
    pub gateway_fixture: PathBuf,

    #[arg(
        long = "gateway-state-dir",
        env = "TAU_GATEWAY_STATE_DIR",
        default_value = ".tau/gateway",
        help = "Directory for gateway runtime state and channel-store outputs"
    )]
    pub gateway_state_dir: PathBuf,

    #[arg(
        long = "gateway-guardrail-failure-streak-threshold",
        env = "TAU_GATEWAY_GUARDRAIL_FAILURE_STREAK_THRESHOLD",
        default_value_t = 2,
        requires = "gateway_contract_runner",
        help = "Failure streak threshold that forces gateway rollout gate to hold"
    )]
    pub gateway_guardrail_failure_streak_threshold: usize,

    #[arg(
        long = "gateway-guardrail-retryable-failures-threshold",
        env = "TAU_GATEWAY_GUARDRAIL_RETRYABLE_FAILURES_THRESHOLD",
        default_value_t = 2,
        requires = "gateway_contract_runner",
        help = "Per-cycle retryable failure threshold that forces gateway rollout gate to hold"
    )]
    pub gateway_guardrail_retryable_failures_threshold: usize,

    #[arg(
        long = "deployment-contract-runner",
        env = "TAU_DEPLOYMENT_CONTRACT_RUNNER",
        default_value_t = false,
        help = "Run fixture-driven cloud deployment and WASM runtime contract scenarios"
    )]
    pub deployment_contract_runner: bool,

    #[arg(
        long = "deployment-fixture",
        env = "TAU_DEPLOYMENT_FIXTURE",
        default_value = "crates/tau-coding-agent/testdata/deployment-contract/mixed-outcomes.json",
        requires = "deployment_contract_runner",
        help = "Path to cloud deployment + WASM runtime contract fixture JSON"
    )]
    pub deployment_fixture: PathBuf,

    #[arg(
        long = "deployment-state-dir",
        env = "TAU_DEPLOYMENT_STATE_DIR",
        default_value = ".tau/deployment",
        help = "Directory for deployment runtime state and channel-store outputs"
    )]
    pub deployment_state_dir: PathBuf,

    #[arg(
        long = "deployment-wasm-package-module",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_MODULE",
        conflicts_with = "deployment_contract_runner",
        help = "Package one WASM module into a verifiable deployment manifest/artifact and exit"
    )]
    pub deployment_wasm_package_module: Option<PathBuf>,

    #[arg(
        long = "deployment-wasm-package-blueprint-id",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_BLUEPRINT_ID",
        requires = "deployment_wasm_package_module",
        default_value = "edge-wasm",
        help = "Blueprint id recorded in WASM package manifest metadata"
    )]
    pub deployment_wasm_package_blueprint_id: String,

    #[arg(
        long = "deployment-wasm-package-runtime-profile",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_RUNTIME_PROFILE",
        value_enum,
        requires = "deployment_wasm_package_module",
        default_value_t = CliDeploymentWasmRuntimeProfile::WasmWasi,
        help = "WASM runtime profile recorded in package manifest metadata"
    )]
    pub deployment_wasm_package_runtime_profile: CliDeploymentWasmRuntimeProfile,

    #[arg(
        long = "deployment-wasm-package-output-dir",
        env = "TAU_DEPLOYMENT_WASM_PACKAGE_OUTPUT_DIR",
        requires = "deployment_wasm_package_module",
        default_value = ".tau/deployment/wasm-artifacts",
        help = "Directory where packaged WASM artifacts and manifest files are written"
    )]
    pub deployment_wasm_package_output_dir: PathBuf,

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
    pub deployment_wasm_package_json: bool,

    #[arg(
        long = "deployment-wasm-inspect-manifest",
        env = "TAU_DEPLOYMENT_WASM_INSPECT_MANIFEST",
        conflicts_with = "deployment_contract_runner",
        conflicts_with = "deployment_wasm_package_module",
        help = "Inspect a packaged deployment WASM manifest for control-plane profile compliance and exit"
    )]
    pub deployment_wasm_inspect_manifest: Option<PathBuf>,

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
    pub deployment_wasm_inspect_json: bool,

    #[arg(
        long = "deployment-wasm-browser-did-init",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_INIT",
        default_value_t = false,
        conflicts_with = "deployment_contract_runner",
        conflicts_with = "deployment_wasm_package_module",
        conflicts_with = "deployment_wasm_inspect_manifest",
        help = "Initialize a browser-native DID identity payload for WASM deployment bootstrap and exit"
    )]
    pub deployment_wasm_browser_did_init: bool,

    #[arg(
        long = "deployment-wasm-browser-did-method",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_METHOD",
        value_enum,
        default_value_t = CliDeploymentWasmBrowserDidMethod::Key,
        requires = "deployment_wasm_browser_did_init",
        help = "DID method used for browser-native WASM identity bootstrap"
    )]
    pub deployment_wasm_browser_did_method: CliDeploymentWasmBrowserDidMethod,

    #[arg(
        long = "deployment-wasm-browser-did-network",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_NETWORK",
        default_value = "tau-devnet",
        requires = "deployment_wasm_browser_did_init",
        help = "Network namespace used when deriving browser-native DID identity"
    )]
    pub deployment_wasm_browser_did_network: String,

    #[arg(
        long = "deployment-wasm-browser-did-subject",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_SUBJECT",
        default_value = "browser-agent",
        requires = "deployment_wasm_browser_did_init",
        help = "Subject identifier used when deriving browser-native DID identity"
    )]
    pub deployment_wasm_browser_did_subject: String,

    #[arg(
        long = "deployment-wasm-browser-did-entropy",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_ENTROPY",
        default_value = "tau-browser-seed",
        requires = "deployment_wasm_browser_did_init",
        hide_env_values = true,
        help = "Entropy seed used for deterministic browser-native DID derivation"
    )]
    pub deployment_wasm_browser_did_entropy: String,

    #[arg(
        long = "deployment-wasm-browser-did-output",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_OUTPUT",
        default_value = ".tau/deployment/browser-did.json",
        requires = "deployment_wasm_browser_did_init",
        help = "Output path where browser-native DID bootstrap payload is written"
    )]
    pub deployment_wasm_browser_did_output: PathBuf,

    #[arg(
        long = "deployment-wasm-browser-did-json",
        env = "TAU_DEPLOYMENT_WASM_BROWSER_DID_JSON",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        requires = "deployment_wasm_browser_did_init",
        help = "Emit --deployment-wasm-browser-did-init output as pretty JSON"
    )]
    pub deployment_wasm_browser_did_json: bool,

    #[arg(
        long = "deployment-queue-limit",
        env = "TAU_DEPLOYMENT_QUEUE_LIMIT",
        default_value_t = 64,
        requires = "deployment_contract_runner",
        help = "Maximum deployment fixture cases processed per runtime cycle"
    )]
    pub deployment_queue_limit: usize,

    #[arg(
        long = "deployment-processed-case-cap",
        env = "TAU_DEPLOYMENT_PROCESSED_CASE_CAP",
        default_value_t = 10_000,
        requires = "deployment_contract_runner",
        help = "Maximum processed-case keys retained for deployment duplicate suppression"
    )]
    pub deployment_processed_case_cap: usize,

    #[arg(
        long = "deployment-retry-max-attempts",
        env = "TAU_DEPLOYMENT_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "deployment_contract_runner",
        help = "Maximum retry attempts for transient deployment runtime failures"
    )]
    pub deployment_retry_max_attempts: usize,

    #[arg(
        long = "deployment-retry-base-delay-ms",
        env = "TAU_DEPLOYMENT_RETRY_BASE_DELAY_MS",
        default_value_t = 0,
        requires = "deployment_contract_runner",
        help = "Base backoff delay in milliseconds for deployment runtime retries (0 disables delay)"
    )]
    pub deployment_retry_base_delay_ms: u64,
}

impl std::ops::Deref for CliRuntimeTailFlags {
    type Target = CliGatewayDaemonFlags;

    fn deref(&self) -> &Self::Target {
        &self.gateway_daemon
    }
}

impl std::ops::DerefMut for CliRuntimeTailFlags {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gateway_daemon
    }
}
