use std::path::PathBuf;

use clap::{ArgAction, Args};

use super::{parse_positive_u64, parse_positive_usize, CliGatewayDaemonFlags};

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
