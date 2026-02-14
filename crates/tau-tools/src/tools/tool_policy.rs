use std::path::PathBuf;

const BALANCED_COMMAND_ALLOWLIST: &[&str] = &[
    "awk", "cargo", "cat", "cp", "cut", "du", "echo", "env", "fd", "find", "git", "grep", "head",
    "ls", "mkdir", "mv", "printf", "pwd", "rg", "rm", "rustc", "rustup", "sed", "sleep", "sort",
    "stat", "tail", "touch", "tr", "uniq", "wc",
];

const STRICT_COMMAND_ALLOWLIST: &[&str] = &[
    "awk", "cat", "cut", "du", "echo", "env", "fd", "find", "grep", "head", "ls", "printf", "pwd",
    "rg", "sed", "sort", "stat", "tail", "tr", "uniq", "wc",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `BashCommandProfile` values.
pub enum BashCommandProfile {
    Permissive,
    Balanced,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ToolPolicyPreset` values.
pub enum ToolPolicyPreset {
    Permissive,
    Balanced,
    Strict,
    Hardened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `OsSandboxMode` values.
pub enum OsSandboxMode {
    Off,
    Auto,
    Force,
}

#[derive(Debug, Clone)]
/// Public struct `ToolPolicy` used across Tau components.
pub struct ToolPolicy {
    pub allowed_roots: Vec<PathBuf>,
    pub policy_preset: ToolPolicyPreset,
    pub max_file_read_bytes: usize,
    pub max_file_write_bytes: usize,
    pub max_command_output_bytes: usize,
    pub bash_timeout_ms: u64,
    pub max_command_length: usize,
    pub allow_command_newlines: bool,
    pub bash_profile: BashCommandProfile,
    pub allowed_commands: Vec<String>,
    pub os_sandbox_mode: OsSandboxMode,
    pub os_sandbox_command: Vec<String>,
    pub enforce_regular_files: bool,
    pub bash_dry_run: bool,
    pub tool_policy_trace: bool,
    pub extension_policy_override_root: Option<PathBuf>,
    pub rbac_principal: Option<String>,
    pub rbac_policy_path: Option<PathBuf>,
}

impl ToolPolicy {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        let mut policy = Self {
            allowed_roots,
            policy_preset: ToolPolicyPreset::Balanced,
            max_file_read_bytes: 1_000_000,
            max_file_write_bytes: 1_000_000,
            max_command_output_bytes: 16_000,
            bash_timeout_ms: 120_000,
            max_command_length: 4_096,
            allow_command_newlines: false,
            bash_profile: BashCommandProfile::Balanced,
            allowed_commands: BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            os_sandbox_mode: OsSandboxMode::Off,
            os_sandbox_command: Vec::new(),
            enforce_regular_files: true,
            bash_dry_run: false,
            tool_policy_trace: false,
            extension_policy_override_root: None,
            rbac_principal: None,
            rbac_policy_path: None,
        };
        policy.apply_preset(ToolPolicyPreset::Balanced);
        policy
    }

    pub fn set_bash_profile(&mut self, profile: BashCommandProfile) {
        self.bash_profile = profile;
        self.allowed_commands = match profile {
            BashCommandProfile::Permissive => Vec::new(),
            BashCommandProfile::Balanced => BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            BashCommandProfile::Strict => STRICT_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
        };
    }

    pub fn apply_preset(&mut self, preset: ToolPolicyPreset) {
        self.policy_preset = preset;
        match preset {
            ToolPolicyPreset::Permissive => {
                self.max_file_read_bytes = 2_000_000;
                self.max_file_write_bytes = 2_000_000;
                self.max_command_output_bytes = 32_000;
                self.bash_timeout_ms = 180_000;
                self.max_command_length = 8_192;
                self.allow_command_newlines = true;
                self.set_bash_profile(BashCommandProfile::Permissive);
                self.os_sandbox_mode = OsSandboxMode::Off;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = false;
            }
            ToolPolicyPreset::Balanced => {
                self.max_file_read_bytes = 1_000_000;
                self.max_file_write_bytes = 1_000_000;
                self.max_command_output_bytes = 16_000;
                self.bash_timeout_ms = 120_000;
                self.max_command_length = 4_096;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Balanced);
                self.os_sandbox_mode = OsSandboxMode::Off;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = true;
            }
            ToolPolicyPreset::Strict => {
                self.max_file_read_bytes = 750_000;
                self.max_file_write_bytes = 750_000;
                self.max_command_output_bytes = 8_000;
                self.bash_timeout_ms = 90_000;
                self.max_command_length = 2_048;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Strict);
                self.os_sandbox_mode = OsSandboxMode::Auto;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = true;
            }
            ToolPolicyPreset::Hardened => {
                self.max_file_read_bytes = 500_000;
                self.max_file_write_bytes = 500_000;
                self.max_command_output_bytes = 4_000;
                self.bash_timeout_ms = 60_000;
                self.max_command_length = 1_024;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Strict);
                self.os_sandbox_mode = OsSandboxMode::Force;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = true;
            }
        }
    }
}

pub fn tool_policy_preset_name(preset: ToolPolicyPreset) -> &'static str {
    match preset {
        ToolPolicyPreset::Permissive => "permissive",
        ToolPolicyPreset::Balanced => "balanced",
        ToolPolicyPreset::Strict => "strict",
        ToolPolicyPreset::Hardened => "hardened",
    }
}
