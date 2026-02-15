use std::path::PathBuf;

use clap::{ArgAction, Args};

use crate::{
    CliBashProfile, CliDaemonProfile, CliOsSandboxMode, CliOsSandboxPolicyMode,
    CliSessionImportMode, CliToolPolicyPreset,
};

/// Gateway/daemon, Slack bridge, session, and tool-policy flags flattened into `Cli`.
#[derive(Debug, Args)]
pub struct CliGatewayDaemonFlags {
    // Gateway remote posture, gateway service lifecycle, and daemon lifecycle flags.
    #[arg(
        long = "gateway-remote-profile-inspect",
        env = "TAU_GATEWAY_REMOTE_PROFILE_INSPECT",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "gateway_remote_plan",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Inspect gateway remote-access posture and risk reason codes without starting the gateway"
    )]
    pub gateway_remote_profile_inspect: bool,

    #[arg(
    long = "gateway-remote-profile-json",
    env = "TAU_GATEWAY_REMOTE_PROFILE_JSON",
    default_value_t = false,
    action = ArgAction::Set,
    num_args = 0..=1,
    require_equals = true,
    default_missing_value = "true",
    requires = "gateway_remote_profile_inspect",
    help = "Emit --gateway-remote-profile-inspect output as pretty JSON"
)]
    pub gateway_remote_profile_json: bool,

    #[arg(
        long = "gateway-remote-plan",
        env = "TAU_GATEWAY_REMOTE_PLAN",
        conflicts_with = "channel_store_inspect",
        conflicts_with = "channel_store_repair",
        conflicts_with = "transport_health_inspect",
        conflicts_with = "dashboard_status_inspect",
        conflicts_with = "multi_channel_status_inspect",
        conflicts_with = "multi_agent_status_inspect",
        conflicts_with = "gateway_status_inspect",
        conflicts_with = "gateway_remote_profile_inspect",
        conflicts_with = "deployment_status_inspect",
        conflicts_with = "custom_command_status_inspect",
        conflicts_with = "voice_status_inspect",
        help = "Export deterministic remote exposure command plans for tailscale serve/funnel and SSH tunnel fallback"
    )]
    pub gateway_remote_plan: bool,

    #[arg(
    long = "gateway-remote-plan-json",
    env = "TAU_GATEWAY_REMOTE_PLAN_JSON",
    default_value_t = false,
    action = ArgAction::Set,
    num_args = 0..=1,
    require_equals = true,
    default_missing_value = "true",
    requires = "gateway_remote_plan",
    help = "Emit --gateway-remote-plan output as pretty JSON"
)]
    pub gateway_remote_plan_json: bool,

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
    pub gateway_service_start: bool,

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
    pub gateway_service_stop: bool,

    #[arg(
        long = "gateway-service-stop-reason",
        env = "TAU_GATEWAY_SERVICE_STOP_REASON",
        requires = "gateway_service_stop",
        help = "Optional reason code/message recorded with --gateway-service-stop"
    )]
    pub gateway_service_stop_reason: Option<String>,

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
    pub gateway_service_status: bool,

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
    pub gateway_service_status_json: bool,

    #[arg(
        long = "daemon-install",
        env = "TAU_DAEMON_INSTALL",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Install Tau daemon service profile files under --daemon-state-dir"
    )]
    pub daemon_install: bool,

    #[arg(
        long = "daemon-uninstall",
        env = "TAU_DAEMON_UNINSTALL",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Uninstall Tau daemon service profile files from --daemon-state-dir"
    )]
    pub daemon_uninstall: bool,

    #[arg(
        long = "daemon-start",
        env = "TAU_DAEMON_START",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_stop",
        conflicts_with = "daemon_status",
        help = "Start Tau daemon lifecycle state and create pid metadata in --daemon-state-dir"
    )]
    pub daemon_start: bool,

    #[arg(
        long = "daemon-stop",
        env = "TAU_DAEMON_STOP",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_status",
        help = "Stop Tau daemon lifecycle state and clear pid metadata in --daemon-state-dir"
    )]
    pub daemon_stop: bool,

    #[arg(
        long = "daemon-stop-reason",
        env = "TAU_DAEMON_STOP_REASON",
        requires = "daemon_stop",
        help = "Optional reason code/message recorded with --daemon-stop"
    )]
    pub daemon_stop_reason: Option<String>,

    #[arg(
        long = "daemon-status",
        env = "TAU_DAEMON_STATUS",
        conflicts_with = "daemon_install",
        conflicts_with = "daemon_uninstall",
        conflicts_with = "daemon_start",
        conflicts_with = "daemon_stop",
        help = "Inspect Tau daemon lifecycle state and diagnostics"
    )]
    pub daemon_status: bool,

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
    pub daemon_status_json: bool,

    #[arg(
    long = "daemon-profile",
    env = "TAU_DAEMON_PROFILE",
    value_enum,
    default_value_t = CliDaemonProfile::Auto,
    help = "Daemon profile target: auto, launchd, or systemd-user"
)]
    pub daemon_profile: CliDaemonProfile,

    #[arg(
        long = "daemon-state-dir",
        env = "TAU_DAEMON_STATE_DIR",
        default_value = ".tau/daemon",
        help = "Directory used for Tau daemon lifecycle state and generated service files"
    )]
    pub daemon_state_dir: PathBuf,

    #[arg(
        long = "slack-bridge",
        env = "TAU_SLACK_BRIDGE",
        default_value_t = false,
        help = "Run as a Slack Socket Mode conversational transport loop instead of interactive prompt mode"
    )]
    pub slack_bridge: bool,

    #[arg(
        long = "slack-app-token",
        env = "TAU_SLACK_APP_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack Socket Mode app token (xapp-...)"
    )]
    pub slack_app_token: Option<String>,

    #[arg(
        long = "slack-app-token-id",
        env = "TAU_SLACK_APP_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack app token"
    )]
    pub slack_app_token_id: Option<String>,

    #[arg(
        long = "slack-bot-token",
        env = "TAU_SLACK_BOT_TOKEN",
        hide_env_values = true,
        requires = "slack_bridge",
        help = "Slack bot token for Web API (xoxb-...)"
    )]
    pub slack_bot_token: Option<String>,

    #[arg(
        long = "slack-bot-token-id",
        env = "TAU_SLACK_BOT_TOKEN_ID",
        requires = "slack_bridge",
        help = "Credential-store integration id used to resolve Slack bot token"
    )]
    pub slack_bot_token_id: Option<String>,

    #[arg(
        long = "slack-bot-user-id",
        env = "TAU_SLACK_BOT_USER_ID",
        requires = "slack_bridge",
        help = "Optional bot user id used to strip self-mentions and ignore self-authored events"
    )]
    pub slack_bot_user_id: Option<String>,

    #[arg(
        long = "slack-api-base",
        env = "TAU_SLACK_API_BASE",
        default_value = "https://slack.com/api",
        requires = "slack_bridge",
        help = "Slack Web API base URL"
    )]
    pub slack_api_base: String,

    #[arg(
        long = "slack-state-dir",
        env = "TAU_SLACK_STATE_DIR",
        default_value = ".tau/slack",
        requires = "slack_bridge",
        help = "Directory for slack bridge state/session/event logs"
    )]
    pub slack_state_dir: PathBuf,

    #[arg(
        long = "slack-artifact-retention-days",
        env = "TAU_SLACK_ARTIFACT_RETENTION_DAYS",
        default_value_t = 30,
        requires = "slack_bridge",
        help = "Retention window for Slack bridge artifacts in days (0 disables expiration)"
    )]
    pub slack_artifact_retention_days: u64,

    #[arg(
        long = "slack-thread-detail-output",
        env = "TAU_SLACK_THREAD_DETAIL_OUTPUT",
        default_value_t = true,
        action = ArgAction::Set,
        requires = "slack_bridge",
        help = "When responses exceed threshold, keep summary in placeholder and post full response as a threaded detail message"
    )]
    pub slack_thread_detail_output: bool,

    #[arg(
        long = "slack-thread-detail-threshold-chars",
        env = "TAU_SLACK_THREAD_DETAIL_THRESHOLD_CHARS",
        default_value_t = 1500,
        requires = "slack_bridge",
        help = "Character threshold used with --slack-thread-detail-output"
    )]
    pub slack_thread_detail_threshold_chars: usize,

    #[arg(
        long = "slack-processed-event-cap",
        env = "TAU_SLACK_PROCESSED_EVENT_CAP",
        default_value_t = 10_000,
        requires = "slack_bridge",
        help = "Maximum processed-event keys to retain for duplicate delivery protection"
    )]
    pub slack_processed_event_cap: usize,

    #[arg(
        long = "slack-max-event-age-seconds",
        env = "TAU_SLACK_MAX_EVENT_AGE_SECONDS",
        default_value_t = 7_200,
        requires = "slack_bridge",
        help = "Ignore inbound Slack events older than this many seconds (0 disables age checks)"
    )]
    pub slack_max_event_age_seconds: u64,

    #[arg(
        long = "slack-reconnect-delay-ms",
        env = "TAU_SLACK_RECONNECT_DELAY_MS",
        default_value_t = 1_000,
        requires = "slack_bridge",
        help = "Delay before reconnecting after socket/session errors"
    )]
    pub slack_reconnect_delay_ms: u64,

    #[arg(
        long = "slack-retry-max-attempts",
        env = "TAU_SLACK_RETRY_MAX_ATTEMPTS",
        default_value_t = 4,
        requires = "slack_bridge",
        help = "Maximum attempts for retryable slack api failures (429/5xx/transport)"
    )]
    pub slack_retry_max_attempts: usize,

    #[arg(
        long = "slack-retry-base-delay-ms",
        env = "TAU_SLACK_RETRY_BASE_DELAY_MS",
        default_value_t = 500,
        requires = "slack_bridge",
        help = "Base backoff delay in milliseconds for slack api retries"
    )]
    pub slack_retry_base_delay_ms: u64,

    #[arg(
        long,
        env = "TAU_SESSION",
        default_value = ".tau/sessions/default.jsonl",
        help = "Session JSONL file"
    )]
    pub session: PathBuf,

    #[arg(long, help = "Disable session persistence")]
    pub no_session: bool,

    #[arg(
        long,
        env = "TAU_SESSION_VALIDATE",
        default_value_t = false,
        help = "Validate session graph integrity and exit"
    )]
    pub session_validate: bool,

    #[arg(
        long,
        env = "TAU_SESSION_IMPORT_MODE",
        value_enum,
        default_value = "merge",
        help = "Import mode for /session-import: merge appends with id remapping, replace overwrites the current session"
    )]
    pub session_import_mode: CliSessionImportMode,

    #[arg(long, help = "Start from a specific session entry id")]
    pub branch_from: Option<u64>,

    #[arg(
        long,
        env = "TAU_SESSION_LOCK_WAIT_MS",
        default_value_t = 5_000,
        help = "Maximum time to wait for acquiring the session lock in milliseconds"
    )]
    pub session_lock_wait_ms: u64,

    #[arg(
        long,
        env = "TAU_SESSION_LOCK_STALE_MS",
        default_value_t = 30_000,
        help = "Lock-file age threshold in milliseconds before stale session locks are reclaimed (0 disables reclaim)"
    )]
    pub session_lock_stale_ms: u64,

    #[arg(
        long = "allow-path",
        env = "TAU_ALLOW_PATH",
        value_delimiter = ',',
        help = "Allowed filesystem roots for read/write/edit/bash cwd (repeatable or comma-separated)"
    )]
    pub allow_path: Vec<PathBuf>,

    #[arg(
        long,
        env = "TAU_BASH_TIMEOUT_MS",
        default_value_t = 120_000,
        help = "Timeout for bash tool commands in milliseconds"
    )]
    pub bash_timeout_ms: u64,

    #[arg(
        long,
        env = "TAU_MAX_TOOL_OUTPUT_BYTES",
        default_value_t = 16_000,
        help = "Maximum bytes returned from tool outputs (stdout/stderr)"
    )]
    pub max_tool_output_bytes: usize,

    #[arg(
        long,
        env = "TAU_MAX_FILE_READ_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size read by the read tool"
    )]
    pub max_file_read_bytes: usize,

    #[arg(
        long,
        env = "TAU_MAX_FILE_WRITE_BYTES",
        default_value_t = 1_000_000,
        help = "Maximum file size written by write/edit tools"
    )]
    pub max_file_write_bytes: usize,

    #[arg(
        long,
        env = "TAU_MAX_COMMAND_LENGTH",
        default_value_t = 4_096,
        help = "Maximum command length accepted by the bash tool"
    )]
    pub max_command_length: usize,

    #[arg(
        long,
        env = "TAU_ALLOW_COMMAND_NEWLINES",
        default_value_t = false,
        help = "Allow newline characters in bash commands"
    )]
    pub allow_command_newlines: bool,

    #[arg(
        long,
        env = "TAU_BASH_PROFILE",
        value_enum,
        default_value = "balanced",
        help = "Command execution profile for bash tool: permissive, balanced, or strict"
    )]
    pub bash_profile: CliBashProfile,

    #[arg(
        long,
        env = "TAU_TOOL_POLICY_PRESET",
        value_enum,
        default_value = "balanced",
        help = "Tool policy preset: permissive, balanced, strict, or hardened"
    )]
    pub tool_policy_preset: CliToolPolicyPreset,

    #[arg(
        long,
        env = "TAU_BASH_DRY_RUN",
        default_value_t = false,
        help = "Validate bash commands against policy without executing them"
    )]
    pub bash_dry_run: bool,

    #[arg(
        long,
        env = "TAU_TOOL_POLICY_TRACE",
        default_value_t = false,
        help = "Include policy evaluation trace details in bash tool results"
    )]
    pub tool_policy_trace: bool,

    #[arg(
        long = "allow-command",
        env = "TAU_ALLOW_COMMAND",
        value_delimiter = ',',
        help = "Additional command executables/prefixes to allow (supports trailing '*' wildcards)"
    )]
    pub allow_command: Vec<String>,

    #[arg(
        long,
        env = "TAU_PRINT_TOOL_POLICY",
        default_value_t = false,
        help = "Print effective tool policy JSON before executing prompts"
    )]
    pub print_tool_policy: bool,

    #[arg(
        long,
        env = "TAU_TOOL_AUDIT_LOG",
        help = "Optional JSONL file path for tool execution audit events"
    )]
    pub tool_audit_log: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_TELEMETRY_LOG",
        help = "Optional JSONL file path for prompt-level telemetry summaries"
    )]
    pub telemetry_log: Option<PathBuf>,

    #[arg(
        long,
        env = "TAU_OS_SANDBOX_MODE",
        value_enum,
        default_value = "off",
        help = "OS sandbox mode for bash tool: off, auto, or force"
    )]
    pub os_sandbox_mode: CliOsSandboxMode,

    #[arg(
        long = "os-sandbox-command",
        env = "TAU_OS_SANDBOX_COMMAND",
        value_delimiter = ',',
        help = "Optional sandbox launcher command template tokens. Supports placeholders: {shell}, {command}, {cwd}"
    )]
    pub os_sandbox_command: Vec<String>,

    #[arg(
        long = "os-sandbox-policy-mode",
        env = "TAU_OS_SANDBOX_POLICY_MODE",
        value_enum,
        help = "Sandbox policy mode for bash tool: best-effort (allow unsandboxed fallback) or required (fail closed)"
    )]
    pub os_sandbox_policy_mode: Option<CliOsSandboxPolicyMode>,

    #[arg(
        long = "http-timeout-ms",
        env = "TAU_HTTP_TIMEOUT_MS",
        default_value_t = 20_000,
        help = "HTTP tool timeout in milliseconds"
    )]
    pub http_timeout_ms: u64,

    #[arg(
        long = "http-max-response-bytes",
        env = "TAU_HTTP_MAX_RESPONSE_BYTES",
        default_value_t = 256_000,
        help = "HTTP tool maximum response body bytes"
    )]
    pub http_max_response_bytes: usize,

    #[arg(
        long = "http-max-redirects",
        env = "TAU_HTTP_MAX_REDIRECTS",
        default_value_t = 5,
        help = "HTTP tool maximum redirect hops when following Location headers"
    )]
    pub http_max_redirects: usize,

    #[arg(
        long = "http-allow-http",
        env = "TAU_HTTP_ALLOW_HTTP",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Allow plain HTTP scheme in HttpTool requests (HTTPS remains allowed)"
    )]
    pub http_allow_http: bool,

    #[arg(
        long = "http-allow-private-network",
        env = "TAU_HTTP_ALLOW_PRIVATE_NETWORK",
        default_value_t = false,
        action = ArgAction::Set,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "true",
        help = "Allow HttpTool requests to private/loopback/link-local network targets"
    )]
    pub http_allow_private_network: bool,

    #[arg(
        long,
        env = "TAU_ENFORCE_REGULAR_FILES",
        default_value_t = true,
        action = ArgAction::Set,
        help = "Require read/edit targets and existing write targets to be regular files (reject symlink targets)"
    )]
    pub enforce_regular_files: bool,
}
