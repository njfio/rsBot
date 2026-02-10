use clap::ValueEnum;

use crate::events::WebhookSignatureAlgorithm;
use crate::multi_channel_contract::MultiChannelTransport;
use crate::multi_channel_outbound::MultiChannelOutboundMode;
use crate::session::SessionImportMode;
use crate::tools::{BashCommandProfile, OsSandboxMode, ToolPolicyPreset};
use crate::ProviderAuthMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliBashProfile {
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
pub(crate) enum CliOsSandboxMode {
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
pub(crate) enum CliSessionImportMode {
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
pub(crate) enum CliCommandFileErrorMode {
    FailFast,
    ContinueOnError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliWebhookSignatureAlgorithm {
    GithubSha256,
    SlackV0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliEventTemplateSchedule {
    Immediate,
    At,
    Periodic,
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
pub(crate) enum CliToolPolicyPreset {
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
pub(crate) enum CliProviderAuthMode {
    ApiKey,
    OauthToken,
    Adc,
    SessionToken,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliGatewayOpenResponsesAuthMode {
    Token,
    PasswordSession,
    LocalhostDev,
}

impl CliGatewayOpenResponsesAuthMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CliGatewayOpenResponsesAuthMode::Token => "token",
            CliGatewayOpenResponsesAuthMode::PasswordSession => "password-session",
            CliGatewayOpenResponsesAuthMode::LocalhostDev => "localhost-dev",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliCredentialStoreEncryptionMode {
    Auto,
    None,
    Keyed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliOrchestratorMode {
    Off,
    PlanFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliMultiChannelTransport {
    Telegram,
    Discord,
    Whatsapp,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum CliMultiChannelOutboundMode {
    ChannelStore,
    DryRun,
    Provider,
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
pub(crate) enum CliDeploymentWasmRuntimeProfile {
    WasmWasi,
}

impl CliDeploymentWasmRuntimeProfile {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            CliDeploymentWasmRuntimeProfile::WasmWasi => "wasm_wasi",
        }
    }
}
