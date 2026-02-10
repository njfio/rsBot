use clap::ValueEnum;

use tau_multi_channel::multi_channel_contract::MultiChannelTransport;
use tau_multi_channel::multi_channel_live_connectors::MultiChannelLiveConnectorMode;
use tau_multi_channel::multi_channel_outbound::MultiChannelOutboundMode;
use tau_session::SessionImportMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliBashProfile {
    Permissive,
    Balanced,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliOsSandboxMode {
    Off,
    Auto,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliSessionImportMode {
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
pub enum CliCommandFileErrorMode {
    FailFast,
    ContinueOnError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliWebhookSignatureAlgorithm {
    GithubSha256,
    SlackV0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliEventTemplateSchedule {
    Immediate,
    At,
    Periodic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliToolPolicyPreset {
    Permissive,
    Balanced,
    Strict,
    Hardened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliProviderAuthMode {
    ApiKey,
    OauthToken,
    Adc,
    SessionToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliGatewayOpenResponsesAuthMode {
    Token,
    PasswordSession,
    LocalhostDev,
}

impl CliGatewayOpenResponsesAuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CliGatewayOpenResponsesAuthMode::Token => "token",
            CliGatewayOpenResponsesAuthMode::PasswordSession => "password-session",
            CliGatewayOpenResponsesAuthMode::LocalhostDev => "localhost-dev",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliGatewayRemoteProfile {
    LocalOnly,
    PasswordRemote,
    ProxyRemote,
    TailscaleServe,
    TailscaleFunnel,
}

impl CliGatewayRemoteProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            CliGatewayRemoteProfile::LocalOnly => "local-only",
            CliGatewayRemoteProfile::PasswordRemote => "password-remote",
            CliGatewayRemoteProfile::ProxyRemote => "proxy-remote",
            CliGatewayRemoteProfile::TailscaleServe => "tailscale-serve",
            CliGatewayRemoteProfile::TailscaleFunnel => "tailscale-funnel",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliDaemonProfile {
    Auto,
    Launchd,
    SystemdUser,
}

impl CliDaemonProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            CliDaemonProfile::Auto => "auto",
            CliDaemonProfile::Launchd => "launchd",
            CliDaemonProfile::SystemdUser => "systemd-user",
        }
    }

    pub fn supported_on_host(self) -> bool {
        match self {
            CliDaemonProfile::Auto => true,
            CliDaemonProfile::Launchd => cfg!(target_os = "macos"),
            CliDaemonProfile::SystemdUser => cfg!(target_os = "linux"),
        }
    }

    pub fn from_str_label(label: &str) -> Option<Self> {
        match label.trim() {
            "auto" => Some(CliDaemonProfile::Auto),
            "launchd" => Some(CliDaemonProfile::Launchd),
            "systemd-user" => Some(CliDaemonProfile::SystemdUser),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliCredentialStoreEncryptionMode {
    Auto,
    None,
    Keyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliOrchestratorMode {
    Off,
    PlanFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliMultiChannelTransport {
    Telegram,
    Discord,
    Whatsapp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliMultiChannelLiveConnectorMode {
    Disabled,
    Polling,
    Webhook,
}

impl CliMultiChannelLiveConnectorMode {
    pub fn is_disabled(self) -> bool {
        matches!(self, CliMultiChannelLiveConnectorMode::Disabled)
    }

    pub fn is_polling(self) -> bool {
        matches!(self, CliMultiChannelLiveConnectorMode::Polling)
    }

    pub fn is_webhook(self) -> bool {
        matches!(self, CliMultiChannelLiveConnectorMode::Webhook)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliMultiChannelOutboundMode {
    ChannelStore,
    DryRun,
    Provider,
}

impl From<CliMultiChannelTransport> for MultiChannelTransport {
    fn from(value: CliMultiChannelTransport) -> Self {
        match value {
            CliMultiChannelTransport::Telegram => MultiChannelTransport::Telegram,
            CliMultiChannelTransport::Discord => MultiChannelTransport::Discord,
            CliMultiChannelTransport::Whatsapp => MultiChannelTransport::Whatsapp,
        }
    }
}

impl From<CliMultiChannelLiveConnectorMode> for MultiChannelLiveConnectorMode {
    fn from(value: CliMultiChannelLiveConnectorMode) -> Self {
        match value {
            CliMultiChannelLiveConnectorMode::Disabled => MultiChannelLiveConnectorMode::Disabled,
            CliMultiChannelLiveConnectorMode::Polling => MultiChannelLiveConnectorMode::Polling,
            CliMultiChannelLiveConnectorMode::Webhook => MultiChannelLiveConnectorMode::Webhook,
        }
    }
}

impl From<CliMultiChannelOutboundMode> for MultiChannelOutboundMode {
    fn from(value: CliMultiChannelOutboundMode) -> Self {
        match value {
            CliMultiChannelOutboundMode::ChannelStore => MultiChannelOutboundMode::ChannelStore,
            CliMultiChannelOutboundMode::DryRun => MultiChannelOutboundMode::DryRun,
            CliMultiChannelOutboundMode::Provider => MultiChannelOutboundMode::Provider,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliDeploymentWasmRuntimeProfile {
    WasmWasi,
    ChannelAutomationWasi,
}

impl CliDeploymentWasmRuntimeProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            CliDeploymentWasmRuntimeProfile::WasmWasi => "wasm_wasi",
            CliDeploymentWasmRuntimeProfile::ChannelAutomationWasi => "channel_automation_wasi",
        }
    }
}
