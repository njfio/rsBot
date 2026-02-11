use crate::tools::{BashCommandProfile, OsSandboxMode, ToolPolicyPreset};
use crate::ProviderAuthMethod;
pub use tau_cli::cli_types::*;

impl From<CliBashProfile> for BashCommandProfile {
    fn from(value: CliBashProfile) -> Self {
        match value {
            CliBashProfile::Permissive => BashCommandProfile::Permissive,
            CliBashProfile::Balanced => BashCommandProfile::Balanced,
            CliBashProfile::Strict => BashCommandProfile::Strict,
        }
    }
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
