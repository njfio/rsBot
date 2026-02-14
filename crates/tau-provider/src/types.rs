use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tau_cli::CliProviderAuthMode;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `CredentialStoreEncryptionMode` values.
pub enum CredentialStoreEncryptionMode {
    None,
    Keyed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `ProviderAuthMethod` values.
pub enum ProviderAuthMethod {
    ApiKey,
    OauthToken,
    Adc,
    SessionToken,
}

impl ProviderAuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderAuthMethod::ApiKey => "api_key",
            ProviderAuthMethod::OauthToken => "oauth_token",
            ProviderAuthMethod::Adc => "adc",
            ProviderAuthMethod::SessionToken => "session_token",
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

#[derive(Debug, Clone)]
/// Public struct `AuthCommandConfig` used across Tau components.
pub struct AuthCommandConfig {
    pub credential_store: PathBuf,
    pub credential_store_key: Option<String>,
    pub credential_store_encryption: CredentialStoreEncryptionMode,
    pub api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub google_api_key: Option<String>,
    pub openai_auth_mode: ProviderAuthMethod,
    pub anthropic_auth_mode: ProviderAuthMethod,
    pub google_auth_mode: ProviderAuthMethod,
    pub provider_subscription_strict: bool,
    pub openai_codex_backend: bool,
    pub openai_codex_cli: String,
    pub anthropic_claude_backend: bool,
    pub anthropic_claude_cli: String,
    pub google_gemini_backend: bool,
    pub google_gemini_cli: String,
    pub google_gcloud_cli: String,
}
