use serde::{Deserialize, Serialize};
use tau_cli::Cli;
use tau_provider::{
    resolve_credential_store_encryption_mode, AuthCommandConfig, ProviderAuthMethod,
};

pub fn default_provider_auth_method() -> ProviderAuthMethod {
    ProviderAuthMethod::ApiKey
}

pub fn build_auth_command_config(cli: &Cli) -> AuthCommandConfig {
    AuthCommandConfig {
        credential_store: cli.credential_store.clone(),
        credential_store_key: cli.credential_store_key.clone(),
        credential_store_encryption: resolve_credential_store_encryption_mode(cli),
        api_key: cli.api_key.clone(),
        openai_api_key: cli.openai_api_key.clone(),
        anthropic_api_key: cli.anthropic_api_key.clone(),
        google_api_key: cli.google_api_key.clone(),
        openai_auth_mode: cli.openai_auth_mode.into(),
        anthropic_auth_mode: cli.anthropic_auth_mode.into(),
        google_auth_mode: cli.google_auth_mode.into(),
        provider_subscription_strict: cli.provider_subscription_strict,
        openai_codex_backend: cli.openai_codex_backend,
        openai_codex_cli: cli.openai_codex_cli.clone(),
        anthropic_claude_backend: cli.anthropic_claude_backend,
        anthropic_claude_cli: cli.anthropic_claude_cli.clone(),
        google_gemini_backend: cli.google_gemini_backend,
        google_gemini_cli: cli.google_gemini_cli.clone(),
        google_gcloud_cli: cli.google_gcloud_cli.clone(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileSessionDefaults {
    pub enabled: bool,
    pub path: Option<String>,
    pub import_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfilePolicyDefaults {
    pub tool_policy_preset: String,
    pub bash_profile: String,
    pub bash_dry_run: bool,
    pub os_sandbox_mode: String,
    pub enforce_regular_files: bool,
    pub bash_timeout_ms: u64,
    pub max_command_length: usize,
    pub max_tool_output_bytes: usize,
    pub max_file_read_bytes: usize,
    pub max_file_write_bytes: usize,
    pub allow_command_newlines: bool,
}

fn default_profile_mcp_context_providers() -> Vec<String> {
    vec![
        "session".to_string(),
        "skills".to_string(),
        "channel-store".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileMcpDefaults {
    #[serde(default = "default_profile_mcp_context_providers")]
    pub context_providers: Vec<String>,
}

impl Default for ProfileMcpDefaults {
    fn default() -> Self {
        Self {
            context_providers: default_profile_mcp_context_providers(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileAuthDefaults {
    #[serde(default = "default_provider_auth_method")]
    pub openai: ProviderAuthMethod,
    #[serde(default = "default_provider_auth_method")]
    pub anthropic: ProviderAuthMethod,
    #[serde(default = "default_provider_auth_method")]
    pub google: ProviderAuthMethod,
}

impl Default for ProfileAuthDefaults {
    fn default() -> Self {
        Self {
            openai: default_provider_auth_method(),
            anthropic: default_provider_auth_method(),
            google: default_provider_auth_method(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileDefaults {
    pub model: String,
    pub fallback_models: Vec<String>,
    pub session: ProfileSessionDefaults,
    pub policy: ProfilePolicyDefaults,
    #[serde(default)]
    pub mcp: ProfileMcpDefaults,
    #[serde(default)]
    pub auth: ProfileAuthDefaults,
}

pub fn build_profile_defaults(cli: &Cli) -> ProfileDefaults {
    ProfileDefaults {
        model: cli.model.clone(),
        fallback_models: cli.fallback_model.clone(),
        session: ProfileSessionDefaults {
            enabled: !cli.no_session,
            path: if cli.no_session {
                None
            } else {
                Some(cli.session.display().to_string())
            },
            import_mode: format!("{:?}", cli.session_import_mode).to_lowercase(),
        },
        policy: ProfilePolicyDefaults {
            tool_policy_preset: format!("{:?}", cli.tool_policy_preset).to_lowercase(),
            bash_profile: format!("{:?}", cli.bash_profile).to_lowercase(),
            bash_dry_run: cli.bash_dry_run,
            os_sandbox_mode: format!("{:?}", cli.os_sandbox_mode).to_lowercase(),
            enforce_regular_files: cli.enforce_regular_files,
            bash_timeout_ms: cli.bash_timeout_ms,
            max_command_length: cli.max_command_length,
            max_tool_output_bytes: cli.max_tool_output_bytes,
            max_file_read_bytes: cli.max_file_read_bytes,
            max_file_write_bytes: cli.max_file_write_bytes,
            allow_command_newlines: cli.allow_command_newlines,
        },
        mcp: ProfileMcpDefaults {
            context_providers: if cli.mcp_context_provider.is_empty() {
                vec![
                    "session".to_string(),
                    "skills".to_string(),
                    "channel-store".to_string(),
                ]
            } else {
                cli.mcp_context_provider.clone()
            },
        },
        auth: ProfileAuthDefaults {
            openai: cli.openai_auth_mode.into(),
            anthropic: cli.anthropic_auth_mode.into(),
            google: cli.google_auth_mode.into(),
        },
    }
}
