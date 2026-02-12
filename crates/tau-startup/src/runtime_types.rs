use std::path::PathBuf;

use tau_cli::Cli;
pub use tau_diagnostics::DoctorCommandConfig;
pub use tau_diagnostics::{DoctorMultiChannelReadinessConfig, DoctorProviderKeyStatus};
pub use tau_provider::AuthCommandConfig;

#[derive(Debug, Clone)]
/// Public struct `SkillsSyncCommandConfig` used across Tau components.
pub struct SkillsSyncCommandConfig {
    pub skills_dir: PathBuf,
    pub default_lock_path: PathBuf,
    pub default_trust_root_path: Option<PathBuf>,
    pub doctor_config: DoctorCommandConfig,
}

#[derive(Debug, Clone, Copy)]
/// Public struct `RenderOptions` used across Tau components.
pub struct RenderOptions {
    pub stream_output: bool,
    pub stream_delay_ms: u64,
}

impl RenderOptions {
    pub fn from_cli(cli: &Cli) -> Self {
        Self {
            stream_output: cli.stream_output,
            stream_delay_ms: cli.stream_delay_ms,
        }
    }
}
