use crate::extension_manifest::ExtensionRegisteredCommand;
use crate::ModelCatalog;
pub(crate) use tau_onboarding::startup_config::ProfileDefaults;
#[cfg(test)]
pub(crate) use tau_onboarding::startup_config::{
    ProfileAuthDefaults, ProfileMcpDefaults, ProfilePolicyDefaults, ProfileSessionDefaults,
};
use tau_session::SessionImportMode;
pub(crate) use tau_startup::runtime_types::{
    AuthCommandConfig, RenderOptions, SkillsSyncCommandConfig,
};
#[cfg(test)]
pub(crate) use tau_startup::runtime_types::{
    DoctorCommandConfig, DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus,
};

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
