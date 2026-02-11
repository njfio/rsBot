use std::path::PathBuf;

use crate::extension_manifest::ExtensionRegisteredCommand;
use crate::{Cli, ModelCatalog};
pub(crate) use tau_diagnostics::DoctorCommandConfig;
#[cfg(test)]
pub(crate) use tau_diagnostics::{DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus};
pub(crate) use tau_onboarding::startup_config::ProfileDefaults;
#[cfg(test)]
pub(crate) use tau_onboarding::startup_config::{
    ProfileAuthDefaults, ProfileMcpDefaults, ProfilePolicyDefaults, ProfileSessionDefaults,
};
pub(crate) use tau_provider::AuthCommandConfig;
use tau_session::SessionImportMode;

#[derive(Debug, Clone)]
pub(crate) struct SkillsSyncCommandConfig {
    pub(crate) skills_dir: PathBuf,
    pub(crate) default_lock_path: PathBuf,
    pub(crate) default_trust_root_path: Option<PathBuf>,
    pub(crate) doctor_config: DoctorCommandConfig,
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
