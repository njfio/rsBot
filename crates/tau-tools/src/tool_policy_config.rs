use anyhow::{anyhow, Context, Result};
use tau_cli::cli_args::Cli;
use tau_cli::cli_types::{CliBashProfile, CliOsSandboxMode, CliToolPolicyPreset};

use crate::tools::{tool_policy_preset_name, BashCommandProfile, OsSandboxMode, ToolPolicy};

const TOOL_POLICY_SCHEMA_VERSION: u32 = 2;

pub fn build_tool_policy(cli: &Cli) -> Result<ToolPolicy> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut roots = vec![cwd];
    roots.extend(cli.allow_path.clone());

    let mut policy = ToolPolicy::new(roots);
    policy.apply_preset(map_cli_tool_policy_preset(cli.tool_policy_preset));

    if cli.bash_timeout_ms != 120_000 {
        policy.bash_timeout_ms = cli.bash_timeout_ms.max(1);
    }
    if cli.max_tool_output_bytes != 16_000 {
        policy.max_command_output_bytes = cli.max_tool_output_bytes.max(128);
    }
    if cli.max_file_read_bytes != 1_000_000 {
        policy.max_file_read_bytes = cli.max_file_read_bytes.max(1_024);
    }
    if cli.max_file_write_bytes != 1_000_000 {
        policy.max_file_write_bytes = cli.max_file_write_bytes.max(1_024);
    }
    if cli.max_command_length != 4_096 {
        policy.max_command_length = cli.max_command_length.max(8);
    }
    if cli.allow_command_newlines {
        policy.allow_command_newlines = true;
    }
    if cli.bash_profile != CliBashProfile::Balanced {
        policy.set_bash_profile(map_cli_bash_profile(cli.bash_profile));
    }
    if cli.os_sandbox_mode != CliOsSandboxMode::Off {
        policy.os_sandbox_mode = map_cli_os_sandbox_mode(cli.os_sandbox_mode);
    }
    if !cli.os_sandbox_command.is_empty() {
        policy.os_sandbox_command = parse_sandbox_command_tokens(&cli.os_sandbox_command)?;
    }
    if !cli.enforce_regular_files {
        policy.enforce_regular_files = false;
    }
    if cli.bash_dry_run {
        policy.bash_dry_run = true;
    }
    if cli.tool_policy_trace {
        policy.tool_policy_trace = true;
    }
    if cli.extension_runtime_hooks {
        policy.extension_policy_override_root = Some(cli.extension_runtime_root.clone());
    }
    if !cli.allow_command.is_empty() {
        for command in &cli.allow_command {
            let command = command.trim();
            if command.is_empty() {
                continue;
            }
            if !policy
                .allowed_commands
                .iter()
                .any(|existing| existing == command)
            {
                policy.allowed_commands.push(command.to_string());
            }
        }
    }
    Ok(policy)
}

pub fn parse_sandbox_command_tokens(raw_tokens: &[String]) -> Result<Vec<String>> {
    let mut parsed = Vec::new();
    for raw in raw_tokens {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens = shell_words::split(trimmed).map_err(|error| {
            anyhow!("invalid --os-sandbox-command token '{}': {error}", trimmed)
        })?;
        if tokens.is_empty() {
            continue;
        }
        parsed.extend(tokens);
    }
    Ok(parsed)
}

pub fn tool_policy_to_json(policy: &ToolPolicy) -> serde_json::Value {
    serde_json::json!({
        "schema_version": TOOL_POLICY_SCHEMA_VERSION,
        "preset": tool_policy_preset_name(policy.policy_preset),
        "allowed_roots": policy
            .allowed_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "max_file_read_bytes": policy.max_file_read_bytes,
        "max_file_write_bytes": policy.max_file_write_bytes,
        "max_command_output_bytes": policy.max_command_output_bytes,
        "bash_timeout_ms": policy.bash_timeout_ms,
        "max_command_length": policy.max_command_length,
        "allow_command_newlines": policy.allow_command_newlines,
        "bash_profile": format!("{:?}", policy.bash_profile).to_lowercase(),
        "allowed_commands": policy.allowed_commands.clone(),
        "os_sandbox_mode": format!("{:?}", policy.os_sandbox_mode).to_lowercase(),
        "os_sandbox_command": policy.os_sandbox_command.clone(),
        "enforce_regular_files": policy.enforce_regular_files,
        "bash_dry_run": policy.bash_dry_run,
        "tool_policy_trace": policy.tool_policy_trace,
        "extension_policy_override_root": policy
            .extension_policy_override_root
            .as_ref()
            .map(|path| path.display().to_string()),
        "rbac_principal": policy.rbac_principal.clone(),
        "rbac_policy_path": policy
            .rbac_policy_path
            .as_ref()
            .map(|path| path.display().to_string()),
    })
}

fn map_cli_bash_profile(value: CliBashProfile) -> BashCommandProfile {
    match value {
        CliBashProfile::Permissive => BashCommandProfile::Permissive,
        CliBashProfile::Balanced => BashCommandProfile::Balanced,
        CliBashProfile::Strict => BashCommandProfile::Strict,
    }
}

fn map_cli_os_sandbox_mode(value: CliOsSandboxMode) -> OsSandboxMode {
    match value {
        CliOsSandboxMode::Off => OsSandboxMode::Off,
        CliOsSandboxMode::Auto => OsSandboxMode::Auto,
        CliOsSandboxMode::Force => OsSandboxMode::Force,
    }
}

fn map_cli_tool_policy_preset(value: CliToolPolicyPreset) -> crate::tools::ToolPolicyPreset {
    match value {
        CliToolPolicyPreset::Permissive => crate::tools::ToolPolicyPreset::Permissive,
        CliToolPolicyPreset::Balanced => crate::tools::ToolPolicyPreset::Balanced,
        CliToolPolicyPreset::Strict => crate::tools::ToolPolicyPreset::Strict,
        CliToolPolicyPreset::Hardened => crate::tools::ToolPolicyPreset::Hardened,
    }
}
