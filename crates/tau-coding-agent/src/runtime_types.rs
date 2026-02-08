use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tau_ai::Provider;

use crate::extension_manifest::ExtensionRegisteredCommand;
use crate::session::{SessionImportMode, SessionStore};
use crate::{
    default_provider_auth_method, Cli, CredentialStoreEncryptionMode, ModelCatalog,
    ProviderAuthMethod,
};

#[derive(Debug)]
pub(crate) struct SessionRuntime {
    pub(crate) store: SessionStore,
    pub(crate) active_head: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct SkillsSyncCommandConfig {
    pub(crate) skills_dir: PathBuf,
    pub(crate) default_lock_path: PathBuf,
    pub(crate) default_trust_root_path: Option<PathBuf>,
    pub(crate) doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorProviderKeyStatus {
    pub(crate) provider_kind: Provider,
    pub(crate) provider: String,
    pub(crate) key_env_var: String,
    pub(crate) present: bool,
    pub(crate) auth_mode: ProviderAuthMethod,
    pub(crate) mode_supported: bool,
    pub(crate) login_backend_enabled: bool,
    pub(crate) login_backend_executable: Option<String>,
    pub(crate) login_backend_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorCommandConfig {
    pub(crate) model: String,
    pub(crate) provider_keys: Vec<DoctorProviderKeyStatus>,
    pub(crate) session_enabled: bool,
    pub(crate) session_path: PathBuf,
    pub(crate) skills_dir: PathBuf,
    pub(crate) skills_lock_path: PathBuf,
    pub(crate) trust_root_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProfileSessionDefaults {
    pub(crate) enabled: bool,
    pub(crate) path: Option<String>,
    pub(crate) import_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProfilePolicyDefaults {
    pub(crate) tool_policy_preset: String,
    pub(crate) bash_profile: String,
    pub(crate) bash_dry_run: bool,
    pub(crate) os_sandbox_mode: String,
    pub(crate) enforce_regular_files: bool,
    pub(crate) bash_timeout_ms: u64,
    pub(crate) max_command_length: usize,
    pub(crate) max_tool_output_bytes: usize,
    pub(crate) max_file_read_bytes: usize,
    pub(crate) max_file_write_bytes: usize,
    pub(crate) allow_command_newlines: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProfileAuthDefaults {
    #[serde(default = "default_provider_auth_method")]
    pub(crate) openai: ProviderAuthMethod,
    #[serde(default = "default_provider_auth_method")]
    pub(crate) anthropic: ProviderAuthMethod,
    #[serde(default = "default_provider_auth_method")]
    pub(crate) google: ProviderAuthMethod,
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
pub(crate) struct ProfileDefaults {
    pub(crate) model: String,
    pub(crate) fallback_models: Vec<String>,
    pub(crate) session: ProfileSessionDefaults,
    pub(crate) policy: ProfilePolicyDefaults,
    #[serde(default)]
    pub(crate) auth: ProfileAuthDefaults,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RenderOptions {
    pub(crate) stream_output: bool,
    pub(crate) stream_delay_ms: u64,
}

impl RenderOptions {
    pub(crate) fn from_cli(cli: &Cli) -> Self {
        Self {
            stream_output: cli.stream_output,
            stream_delay_ms: cli.stream_delay_ms,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CommandExecutionContext<'a> {
    pub(crate) tool_policy_json: &'a serde_json::Value,
    pub(crate) session_import_mode: SessionImportMode,
    pub(crate) profile_defaults: &'a ProfileDefaults,
    pub(crate) skills_command_config: &'a SkillsSyncCommandConfig,
    pub(crate) auth_command_config: &'a AuthCommandConfig,
    pub(crate) model_catalog: &'a ModelCatalog,
    pub(crate) extension_commands: &'a [ExtensionRegisteredCommand],
}

#[derive(Debug, Clone)]
pub(crate) struct AuthCommandConfig {
    pub(crate) credential_store: PathBuf,
    pub(crate) credential_store_key: Option<String>,
    pub(crate) credential_store_encryption: CredentialStoreEncryptionMode,
    pub(crate) api_key: Option<String>,
    pub(crate) openai_api_key: Option<String>,
    pub(crate) anthropic_api_key: Option<String>,
    pub(crate) google_api_key: Option<String>,
    pub(crate) openai_auth_mode: ProviderAuthMethod,
    pub(crate) anthropic_auth_mode: ProviderAuthMethod,
    pub(crate) google_auth_mode: ProviderAuthMethod,
    pub(crate) openai_codex_backend: bool,
    pub(crate) openai_codex_cli: String,
    pub(crate) anthropic_claude_backend: bool,
    pub(crate) anthropic_claude_cli: String,
    pub(crate) google_gemini_backend: bool,
    pub(crate) google_gemini_cli: String,
    pub(crate) google_gcloud_cli: String,
}
