use anyhow::Result;
use serde_json::Value;
use tau_cli::Cli;
use tau_tools::tool_policy_config::{build_tool_policy, tool_policy_to_json};
use tau_tools::tools::ToolPolicy;

const STARTUP_SAFETY_POLICY_PRECEDENCE: [&str; 3] = [
    "profile_preset",
    "cli_flags_and_cli_env",
    "runtime_env_overrides",
];

/// Public struct `StartupSafetyPolicyResolution` used across Tau components.
#[derive(Debug, Clone)]
pub struct StartupSafetyPolicyResolution {
    pub tool_policy: ToolPolicy,
    pub tool_policy_json: Value,
    pub precedence_layers: Vec<String>,
}

/// Returns the canonical safety-policy precedence contract.
pub fn startup_safety_policy_precedence_layers() -> Vec<String> {
    STARTUP_SAFETY_POLICY_PRECEDENCE
        .iter()
        .map(|layer| (*layer).to_string())
        .collect()
}

/// Resolves startup safety policy using a single precedence contract.
pub fn resolve_startup_safety_policy(cli: &Cli) -> Result<StartupSafetyPolicyResolution> {
    let tool_policy = build_tool_policy(cli)?;
    let tool_policy_json = tool_policy_to_json(&tool_policy);
    if cli.print_tool_policy {
        println!("{tool_policy_json}");
    }
    Ok(StartupSafetyPolicyResolution {
        tool_policy,
        tool_policy_json,
        precedence_layers: startup_safety_policy_precedence_layers(),
    })
}

#[cfg(test)]
mod tests {
    use super::{resolve_startup_safety_policy, startup_safety_policy_precedence_layers};
    use clap::Parser;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use tau_cli::Cli;
    use tau_tools::tools::{BashCommandProfile, ToolPolicyPreset};

    #[test]
    fn unit_startup_safety_policy_precedence_layers_match_contract() {
        assert_eq!(
            startup_safety_policy_precedence_layers(),
            vec![
                "profile_preset".to_string(),
                "cli_flags_and_cli_env".to_string(),
                "runtime_env_overrides".to_string(),
            ]
        );
    }

    #[test]
    fn functional_resolve_startup_safety_policy_cli_flag_overrides_env_and_preset() {
        let _guard = env_lock().lock().expect("env lock");
        let _snapshot = EnvSnapshot::capture(&["TAU_BASH_PROFILE"]);
        std::env::set_var("TAU_BASH_PROFILE", "strict");

        let cli = parse_cli_with_stack_args(vec![
            "tau-rs",
            "--tool-policy-preset",
            "hardened",
            "--bash-profile",
            "permissive",
        ]);
        let resolved = resolve_startup_safety_policy(&cli).expect("resolve startup safety policy");

        assert_eq!(
            resolved.tool_policy.policy_preset,
            ToolPolicyPreset::Hardened
        );
        assert_eq!(
            resolved.tool_policy.bash_profile,
            BashCommandProfile::Permissive
        );
    }

    #[test]
    fn regression_resolve_startup_safety_policy_env_overrides_preset_when_cli_flag_unset() {
        let _guard = env_lock().lock().expect("env lock");
        let _snapshot = EnvSnapshot::capture(&["TAU_BASH_PROFILE"]);
        std::env::set_var("TAU_BASH_PROFILE", "permissive");

        let cli = parse_cli_with_stack_args(vec!["tau-rs", "--tool-policy-preset", "hardened"]);
        let resolved = resolve_startup_safety_policy(&cli).expect("resolve startup safety policy");

        assert_eq!(
            resolved.tool_policy.policy_preset,
            ToolPolicyPreset::Hardened
        );
        assert_eq!(
            resolved.tool_policy.bash_profile,
            BashCommandProfile::Permissive
        );
        assert_eq!(
            resolved.precedence_layers,
            startup_safety_policy_precedence_layers()
        );
    }

    #[test]
    fn regression_resolve_startup_safety_policy_applies_runtime_env_overrides() {
        let _guard = env_lock().lock().expect("env lock");
        let _snapshot = EnvSnapshot::capture(&["TAU_MEMORY_EMBEDDING_PROVIDER"]);
        std::env::set_var("TAU_MEMORY_EMBEDDING_PROVIDER", "openai-compatible");

        let cli = parse_cli_with_stack();
        let resolved = resolve_startup_safety_policy(&cli).expect("resolve startup safety policy");

        assert_eq!(
            resolved.tool_policy.memory_embedding_provider.as_deref(),
            Some("openai-compatible")
        );
        assert_eq!(
            resolved.tool_policy_json["memory_embedding_provider"],
            "openai-compatible"
        );
    }

    #[test]
    fn regression_resolve_startup_safety_policy_defaults_memory_embedding_provider_local() {
        let _guard = env_lock().lock().expect("env lock");
        let vars = [
            "TAU_MEMORY_EMBEDDING_PROVIDER",
            "TAU_MEMORY_EMBEDDING_MODEL",
            "TAU_MEMORY_EMBEDDING_API_BASE",
            "TAU_MEMORY_EMBEDDING_API_KEY",
        ];
        let _snapshot = EnvSnapshot::capture(&vars);
        for name in vars {
            std::env::remove_var(name);
        }

        let cli = parse_cli_with_stack();
        let resolved = resolve_startup_safety_policy(&cli).expect("resolve startup safety policy");

        assert_eq!(
            resolved.tool_policy.memory_embedding_provider.as_deref(),
            Some("local")
        );
        assert_eq!(
            resolved.tool_policy_json["memory_embedding_provider"],
            "local"
        );
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    fn parse_cli_with_stack_args(args: Vec<&'static str>) -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse-args".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(args))
            .expect("spawn cli parse args thread")
            .join()
            .expect("join cli parse args thread")
    }

    struct EnvSnapshot {
        values: Vec<(String, Option<OsString>)>,
    }

    impl EnvSnapshot {
        fn capture(names: &[&str]) -> Self {
            Self {
                values: names
                    .iter()
                    .map(|name| ((*name).to_string(), std::env::var_os(name)))
                    .collect(),
            }
        }
    }

    impl Drop for EnvSnapshot {
        fn drop(&mut self) {
            for (name, value) in self.values.drain(..) {
                match value {
                    Some(previous) => std::env::set_var(name, previous),
                    None => std::env::remove_var(name),
                }
            }
        }
    }
}
