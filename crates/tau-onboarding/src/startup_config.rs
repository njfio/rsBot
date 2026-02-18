//! Startup configuration derivation from CLI/options.
//!
//! This module defines defaults and serialization contracts for provider auth and
//! profile bootstrap settings used by startup preflight, onboarding, and runtime
//! dispatch phases.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
/// Public struct `ProfileSessionDefaults` used across Tau components.
pub struct ProfileSessionDefaults {
    pub enabled: bool,
    pub path: Option<String>,
    pub import_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `ProfilePolicyDefaults` used across Tau components.
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
    #[serde(default = "default_runtime_heartbeat_enabled")]
    pub runtime_heartbeat_enabled: bool,
    #[serde(default = "default_runtime_heartbeat_interval_ms")]
    pub runtime_heartbeat_interval_ms: u64,
    #[serde(default = "default_runtime_heartbeat_state_path")]
    pub runtime_heartbeat_state_path: String,
    #[serde(default = "default_runtime_self_repair_enabled")]
    pub runtime_self_repair_enabled: bool,
    #[serde(default = "default_runtime_self_repair_timeout_ms")]
    pub runtime_self_repair_timeout_ms: u64,
    #[serde(default = "default_runtime_self_repair_max_retries")]
    pub runtime_self_repair_max_retries: usize,
    #[serde(default = "default_runtime_self_repair_tool_builds_dir")]
    pub runtime_self_repair_tool_builds_dir: String,
    #[serde(default = "default_runtime_self_repair_orphan_max_age_seconds")]
    pub runtime_self_repair_orphan_max_age_seconds: u64,
}

fn default_runtime_heartbeat_enabled() -> bool {
    true
}

fn default_runtime_heartbeat_interval_ms() -> u64 {
    5_000
}

fn default_runtime_heartbeat_state_path() -> String {
    ".tau/runtime-heartbeat/state.json".to_string()
}

fn default_runtime_self_repair_enabled() -> bool {
    true
}

fn default_runtime_self_repair_timeout_ms() -> u64 {
    300_000
}

fn default_runtime_self_repair_max_retries() -> usize {
    2
}

fn default_runtime_self_repair_tool_builds_dir() -> String {
    ".tau/tool-builds".to_string()
}

fn default_runtime_self_repair_orphan_max_age_seconds() -> u64 {
    3_600
}

fn default_profile_mcp_context_providers() -> Vec<String> {
    vec![
        "session".to_string(),
        "skills".to_string(),
        "channel-store".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `ProfileMcpDefaults` used across Tau components.
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
/// Public struct `ProfileAuthDefaults` used across Tau components.
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

fn default_profile_routing_task_overrides() -> BTreeMap<String, String> {
    BTreeMap::new()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
/// Public struct `ProfileRoutingDefaults` used across Tau components.
pub struct ProfileRoutingDefaults {
    #[serde(default)]
    pub channel_model: Option<String>,
    #[serde(default)]
    pub branch_model: Option<String>,
    #[serde(default)]
    pub worker_model: Option<String>,
    #[serde(default)]
    pub compactor_model: Option<String>,
    #[serde(default)]
    pub cortex_model: Option<String>,
    #[serde(default = "default_profile_routing_task_overrides")]
    pub task_overrides: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `ProfileDefaults` used across Tau components.
pub struct ProfileDefaults {
    pub model: String,
    pub fallback_models: Vec<String>,
    pub session: ProfileSessionDefaults,
    pub policy: ProfilePolicyDefaults,
    #[serde(default)]
    pub mcp: ProfileMcpDefaults,
    #[serde(default)]
    pub auth: ProfileAuthDefaults,
    #[serde(default)]
    pub routing: ProfileRoutingDefaults,
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
            runtime_heartbeat_enabled: cli.runtime_heartbeat_enabled,
            runtime_heartbeat_interval_ms: cli.runtime_heartbeat_interval_ms,
            runtime_heartbeat_state_path: cli.runtime_heartbeat_state_path.display().to_string(),
            runtime_self_repair_enabled: cli.runtime_self_repair_enabled,
            runtime_self_repair_timeout_ms: cli.runtime_self_repair_timeout_ms,
            runtime_self_repair_max_retries: cli.runtime_self_repair_max_retries,
            runtime_self_repair_tool_builds_dir: cli
                .runtime_self_repair_tool_builds_dir
                .display()
                .to_string(),
            runtime_self_repair_orphan_max_age_seconds: cli
                .runtime_self_repair_orphan_max_age_seconds,
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
        routing: ProfileRoutingDefaults::default(),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use serde_json::json;
    use std::path::PathBuf;
    use std::thread;

    use super::build_profile_defaults;
    use tau_cli::Cli;

    fn parse_cli_with_stack() -> Cli {
        thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    #[test]
    fn unit_build_profile_defaults_includes_runtime_heartbeat_policy_defaults() {
        let cli = parse_cli_with_stack();
        let defaults = build_profile_defaults(&cli);
        assert!(defaults.policy.runtime_heartbeat_enabled);
        assert_eq!(defaults.policy.runtime_heartbeat_interval_ms, 5_000);
        assert_eq!(
            defaults.policy.runtime_heartbeat_state_path,
            ".tau/runtime-heartbeat/state.json".to_string()
        );
        assert!(defaults.policy.runtime_self_repair_enabled);
        assert_eq!(defaults.policy.runtime_self_repair_timeout_ms, 300_000);
        assert_eq!(defaults.policy.runtime_self_repair_max_retries, 2);
        assert_eq!(
            defaults.policy.runtime_self_repair_tool_builds_dir,
            ".tau/tool-builds".to_string()
        );
        assert_eq!(
            defaults.policy.runtime_self_repair_orphan_max_age_seconds,
            3_600
        );
    }

    #[test]
    fn functional_build_profile_defaults_applies_runtime_heartbeat_overrides() {
        let mut cli = parse_cli_with_stack();
        cli.runtime_heartbeat_enabled = false;
        cli.runtime_heartbeat_interval_ms = 1_200;
        cli.runtime_heartbeat_state_path = PathBuf::from(".tau/runtime-heartbeat/custom.json");
        cli.runtime_self_repair_enabled = false;
        cli.runtime_self_repair_timeout_ms = 45_000;
        cli.runtime_self_repair_max_retries = 4;
        cli.runtime_self_repair_tool_builds_dir = PathBuf::from(".tau/tool-builds/custom");
        cli.runtime_self_repair_orphan_max_age_seconds = 120;

        let defaults = build_profile_defaults(&cli);
        assert!(!defaults.policy.runtime_heartbeat_enabled);
        assert_eq!(defaults.policy.runtime_heartbeat_interval_ms, 1_200);
        assert_eq!(
            defaults.policy.runtime_heartbeat_state_path,
            ".tau/runtime-heartbeat/custom.json".to_string()
        );
        assert!(!defaults.policy.runtime_self_repair_enabled);
        assert_eq!(defaults.policy.runtime_self_repair_timeout_ms, 45_000);
        assert_eq!(defaults.policy.runtime_self_repair_max_retries, 4);
        assert_eq!(
            defaults.policy.runtime_self_repair_tool_builds_dir,
            ".tau/tool-builds/custom".to_string()
        );
        assert_eq!(
            defaults.policy.runtime_self_repair_orphan_max_age_seconds,
            120
        );
    }

    #[test]
    fn spec_2536_c01_profile_defaults_parse_routing_fields() {
        let parsed: super::ProfileDefaults = serde_json::from_value(json!({
            "model": "openai/gpt-4o-mini",
            "fallback_models": [],
            "session": {
                "enabled": true,
                "path": ".tau/sessions/default.sqlite",
                "import_mode": "merge"
            },
            "policy": {
                "tool_policy_preset": "balanced",
                "bash_profile": "balanced",
                "bash_dry_run": false,
                "os_sandbox_mode": "off",
                "enforce_regular_files": true,
                "bash_timeout_ms": 120000,
                "max_command_length": 8192,
                "max_tool_output_bytes": 262144,
                "max_file_read_bytes": 262144,
                "max_file_write_bytes": 262144,
                "allow_command_newlines": true,
                "runtime_heartbeat_enabled": true,
                "runtime_heartbeat_interval_ms": 5000,
                "runtime_heartbeat_state_path": ".tau/runtime-heartbeat/state.json",
                "runtime_self_repair_enabled": true,
                "runtime_self_repair_timeout_ms": 300000,
                "runtime_self_repair_max_retries": 2,
                "runtime_self_repair_tool_builds_dir": ".tau/tool-builds",
                "runtime_self_repair_orphan_max_age_seconds": 3600
            },
            "routing": {
                "channel_model": "openai/gpt-4.1-mini",
                "branch_model": "openai/o3-mini",
                "worker_model": "openai/o3-mini",
                "compactor_model": "openai/gpt-4o-mini",
                "cortex_model": "openai/gpt-4.1-mini",
                "task_overrides": {
                    "coding": "openai/o3-mini",
                    "summarization": "openai/gpt-4o-mini"
                }
            }
        }))
        .expect("parse profile defaults");
        assert_eq!(
            parsed.routing.channel_model.as_deref(),
            Some("openai/gpt-4.1-mini")
        );
        assert_eq!(
            parsed
                .routing
                .task_overrides
                .get("coding")
                .map(String::as_str),
            Some("openai/o3-mini")
        );
    }

    #[test]
    fn regression_2536_build_profile_defaults_initializes_empty_routing_task_overrides() {
        let cli = parse_cli_with_stack();
        let defaults = build_profile_defaults(&cli);
        assert!(defaults.routing.task_overrides.is_empty());
        assert_eq!(defaults.routing.task_overrides.len(), 0);
    }

    #[test]
    fn regression_2536_profile_defaults_without_routing_defaults_to_empty_task_overrides() {
        let parsed: super::ProfileDefaults = serde_json::from_value(json!({
            "model": "openai/gpt-4o-mini",
            "fallback_models": [],
            "session": {
                "enabled": true,
                "path": ".tau/sessions/default.sqlite",
                "import_mode": "merge"
            },
            "policy": {
                "tool_policy_preset": "balanced",
                "bash_profile": "balanced",
                "bash_dry_run": false,
                "os_sandbox_mode": "off",
                "enforce_regular_files": true,
                "bash_timeout_ms": 120000,
                "max_command_length": 8192,
                "max_tool_output_bytes": 262144,
                "max_file_read_bytes": 262144,
                "max_file_write_bytes": 262144,
                "allow_command_newlines": true,
                "runtime_heartbeat_enabled": true,
                "runtime_heartbeat_interval_ms": 5000,
                "runtime_heartbeat_state_path": ".tau/runtime-heartbeat/state.json",
                "runtime_self_repair_enabled": true,
                "runtime_self_repair_timeout_ms": 300000,
                "runtime_self_repair_max_retries": 2,
                "runtime_self_repair_tool_builds_dir": ".tau/tool-builds",
                "runtime_self_repair_orphan_max_age_seconds": 3600
            }
        }))
        .expect("parse profile defaults without routing");
        assert!(parsed.routing.task_overrides.is_empty());
        assert!(parsed.routing.channel_model.is_none());
        assert!(parsed.routing.branch_model.is_none());
        assert!(parsed.routing.worker_model.is_none());
        assert!(parsed.routing.compactor_model.is_none());
        assert!(parsed.routing.cortex_model.is_none());
    }

    #[test]
    fn regression_2536_routing_without_task_overrides_defaults_to_empty_map() {
        let parsed: super::ProfileDefaults = serde_json::from_value(json!({
            "model": "openai/gpt-4o-mini",
            "fallback_models": [],
            "session": {
                "enabled": true,
                "path": ".tau/sessions/default.sqlite",
                "import_mode": "merge"
            },
            "policy": {
                "tool_policy_preset": "balanced",
                "bash_profile": "balanced",
                "bash_dry_run": false,
                "os_sandbox_mode": "off",
                "enforce_regular_files": true,
                "bash_timeout_ms": 120000,
                "max_command_length": 8192,
                "max_tool_output_bytes": 262144,
                "max_file_read_bytes": 262144,
                "max_file_write_bytes": 262144,
                "allow_command_newlines": true,
                "runtime_heartbeat_enabled": true,
                "runtime_heartbeat_interval_ms": 5000,
                "runtime_heartbeat_state_path": ".tau/runtime-heartbeat/state.json",
                "runtime_self_repair_enabled": true,
                "runtime_self_repair_timeout_ms": 300000,
                "runtime_self_repair_max_retries": 2,
                "runtime_self_repair_tool_builds_dir": ".tau/tool-builds",
                "runtime_self_repair_orphan_max_age_seconds": 3600
            },
            "routing": {
                "channel_model": "openai/gpt-4.1-mini"
            }
        }))
        .expect("parse profile defaults with partial routing");
        assert_eq!(
            parsed.routing.channel_model.as_deref(),
            Some("openai/gpt-4.1-mini")
        );
        assert!(parsed.routing.task_overrides.is_empty());
    }
}
