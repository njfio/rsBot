use std::path::PathBuf;

use clap::{ArgAction, Args};

use crate::CliDaemonProfile;

/// Gateway remote/service and daemon lifecycle flag group flattened into `Cli`.
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
}
