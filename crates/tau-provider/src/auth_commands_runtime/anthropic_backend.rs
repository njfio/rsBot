//! Anthropic-specific auth login runtime behavior.

use tau_ai::Provider;

use crate::{is_executable_available, AuthCommandConfig, ProviderAuthMethod};

use super::shared_runtime_core::{
    build_auth_login_launch_spec, execute_auth_login_launch, render_launch_command,
};

pub(super) fn execute_anthropic_login_backend_ready(
    config: &AuthCommandConfig,
    mode: ProviderAuthMethod,
    launch: bool,
    json_output: bool,
) -> String {
    if !config.anthropic_claude_backend {
        let reason =
            "anthropic claude backend is disabled; set --anthropic-claude-backend=true".to_string();
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": Provider::Anthropic.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} launch_requested={} launch_executed=false error={reason}",
            Provider::Anthropic.as_str(),
            mode.as_str(),
            launch
        );
    }

    if !is_executable_available(&config.anthropic_claude_cli) {
        let reason = format!(
            "claude cli executable '{}' is not available",
            config.anthropic_claude_cli
        );
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": Provider::Anthropic.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} launch_requested={} launch_executed=false error={reason}",
            Provider::Anthropic.as_str(),
            mode.as_str(),
            launch
        );
    }

    let action = "run claude, then enter /login in the Claude prompt";
    let launch_spec = match build_auth_login_launch_spec(config, Provider::Anthropic, mode) {
        Ok(spec) => spec,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": Provider::Anthropic.as_str(),
                    "mode": mode.as_str(),
                    "status": "error",
                    "reason": error.to_string(),
                    "launch_requested": launch,
                    "launch_executed": false,
                })
                .to_string();
            }
            return format!(
                "auth login error: provider={} mode={} launch_requested={} launch_executed=false error={error}",
                Provider::Anthropic.as_str(),
                mode.as_str(),
                launch
            );
        }
    };
    let launch_command = render_launch_command(&launch_spec.executable, &launch_spec.args);

    if launch {
        return match execute_auth_login_launch(&launch_spec) {
            Ok(result) => {
                if json_output {
                    serde_json::json!({
                        "command": "auth.login",
                        "provider": Provider::Anthropic.as_str(),
                        "mode": mode.as_str(),
                        "status": "launched",
                        "source": "claude_cli",
                        "backend_cli": config.anthropic_claude_cli,
                        "persisted": false,
                        "action": action,
                        "launch_requested": true,
                        "launch_executed": true,
                        "launch_command": result.command,
                    })
                    .to_string()
                } else {
                    format!(
                        "auth login: provider={} mode={} status=launched source=claude_cli backend_cli={} persisted=false action={} launch_requested=true launch_executed=true launch_command={}",
                        Provider::Anthropic.as_str(),
                        mode.as_str(),
                        config.anthropic_claude_cli,
                        action,
                        result.command
                    )
                }
            }
            Err(error) => {
                if json_output {
                    serde_json::json!({
                        "command": "auth.login",
                        "provider": Provider::Anthropic.as_str(),
                        "mode": mode.as_str(),
                        "status": "error",
                        "reason": error.to_string(),
                        "launch_requested": true,
                        "launch_executed": false,
                        "launch_command": launch_command,
                    })
                    .to_string()
                } else {
                    format!(
                        "auth login error: provider={} mode={} launch_requested=true launch_executed=false launch_command={} error={error}",
                        Provider::Anthropic.as_str(),
                        mode.as_str(),
                        launch_command
                    )
                }
            }
        };
    }

    if json_output {
        return serde_json::json!({
            "command": "auth.login",
            "provider": Provider::Anthropic.as_str(),
            "mode": mode.as_str(),
            "status": "ready",
            "source": "claude_cli",
            "backend_cli": config.anthropic_claude_cli,
            "persisted": false,
            "action": action,
            "launch_requested": false,
            "launch_executed": false,
            "launch_command": launch_command,
        })
        .to_string();
    }
    format!(
        "auth login: provider={} mode={} status=ready source=claude_cli backend_cli={} persisted=false action={} launch_requested=false launch_executed=false launch_command={}",
        Provider::Anthropic.as_str(),
        mode.as_str(),
        config.anthropic_claude_cli,
        action,
        launch_command
    )
}
