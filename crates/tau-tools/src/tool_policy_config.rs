use anyhow::{anyhow, Context, Result};
use tau_cli::cli_args::Cli;
use tau_cli::cli_types::{CliBashProfile, CliOsSandboxMode, CliToolPolicyPreset};

use crate::tools::{
    tool_policy_preset_name, tool_rate_limit_behavior_name, BashCommandProfile, OsSandboxMode,
    ToolPolicy,
};

const TOOL_POLICY_SCHEMA_VERSION: u32 = 4;
const PROTECTED_PATHS_ENV: &str = "TAU_PROTECTED_PATHS";
const ALLOW_PROTECTED_PATH_MUTATIONS_ENV: &str = "TAU_ALLOW_PROTECTED_PATH_MUTATIONS";

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

    if let Some(allow_mutations) = parse_optional_env_bool(ALLOW_PROTECTED_PATH_MUTATIONS_ENV)? {
        policy.allow_protected_path_mutations = allow_mutations;
    }

    for protected_path in parse_protected_paths_env(PROTECTED_PATHS_ENV)? {
        policy.add_protected_path(protected_path);
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
    let rate_limit_counters = policy.rate_limit_counters();
    serde_json::json!({
        "schema_version": TOOL_POLICY_SCHEMA_VERSION,
        "preset": tool_policy_preset_name(policy.policy_preset),
        "allowed_roots": policy
            .allowed_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "protected_paths": policy
            .protected_paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "allow_protected_path_mutations": policy.allow_protected_path_mutations,
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
        "tool_rate_limit": {
            "max_requests": policy.tool_rate_limit_max_requests,
            "window_ms": policy.tool_rate_limit_window_ms,
            "exceeded_behavior": tool_rate_limit_behavior_name(policy.tool_rate_limit_exceeded_behavior),
            "throttle_events_total": rate_limit_counters.throttle_events_total,
            "tracked_principals": rate_limit_counters.tracked_principals,
        },
    })
}

fn parse_optional_env_bool(name: &str) -> Result<Option<bool>> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(None);
    };
    let raw = raw.to_string_lossy();
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }
    match value.as_str() {
        "1" | "true" | "yes" | "on" => Ok(Some(true)),
        "0" | "false" | "no" | "off" => Ok(Some(false)),
        _ => Err(anyhow!(
            "invalid {} value '{}': expected one of 1,true,yes,on,0,false,no,off",
            name,
            raw.trim()
        )),
    }
}

fn parse_protected_paths_env(name: &str) -> Result<Vec<std::path::PathBuf>> {
    let Some(raw) = std::env::var_os(name) else {
        return Ok(Vec::new());
    };
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    let mut paths = Vec::new();
    for token in raw.to_string_lossy().split(',') {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = std::path::PathBuf::from(trimmed);
        let absolute = if parsed.is_absolute() {
            parsed
        } else {
            cwd.join(parsed)
        };
        if !paths.iter().any(|existing| existing == &absolute) {
            paths.push(absolute);
        }
    }
    Ok(paths)
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

#[cfg(test)]
mod tests {
    use super::tool_policy_to_json;
    use crate::tools::ToolPolicy;
    use tempfile::tempdir;

    #[test]
    fn unit_tool_policy_json_exposes_protected_path_controls() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.allow_protected_path_mutations = true;
        let payload = tool_policy_to_json(&policy);

        assert_eq!(payload["schema_version"], 4);
        assert_eq!(payload["allow_protected_path_mutations"], true);
        assert!(payload["protected_paths"]
            .as_array()
            .map(|paths| {
                paths.iter().any(|path| {
                    path.as_str()
                        .map(|value| value.ends_with("AGENTS.md"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false));
    }
}
