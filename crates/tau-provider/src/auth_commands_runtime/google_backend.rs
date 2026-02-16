//! Google-specific auth login runtime behavior.

use tau_ai::Provider;

use crate::{is_executable_available, AuthCommandConfig, ProviderAuthMethod};

use super::shared_runtime_core::{
    build_auth_login_launch_spec, execute_auth_login_launch, render_launch_command,
};

pub(super) fn execute_google_login_backend_ready(
    config: &AuthCommandConfig,
    mode: ProviderAuthMethod,
    launch: bool,
    json_output: bool,
) -> String {
    if !config.google_gemini_backend {
        let reason =
            "google gemini backend is disabled; set --google-gemini-backend=true".to_string();
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": Provider::Google.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} launch_requested={} launch_executed=false error={reason}",
            Provider::Google.as_str(),
            mode.as_str(),
            launch
        );
    }

    if !is_executable_available(&config.google_gemini_cli) {
        let reason = format!(
            "gemini cli executable '{}' is not available",
            config.google_gemini_cli
        );
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": Provider::Google.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} launch_requested={} launch_executed=false error={reason}",
            Provider::Google.as_str(),
            mode.as_str(),
            launch
        );
    }

    if mode == ProviderAuthMethod::Adc && !is_executable_available(&config.google_gcloud_cli) {
        let reason = format!(
            "gcloud executable '{}' is not available",
            config.google_gcloud_cli
        );
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": Provider::Google.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
                "launch_requested": launch,
                "launch_executed": false,
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} launch_requested={} launch_executed=false error={reason}",
            Provider::Google.as_str(),
            mode.as_str(),
            launch
        );
    }

    let action = if mode == ProviderAuthMethod::Adc {
        "run gcloud auth application-default login and set GOOGLE_CLOUD_PROJECT/GOOGLE_CLOUD_LOCATION"
    } else {
        "run gemini and select Login with Google"
    };
    let launch_spec = match build_auth_login_launch_spec(config, Provider::Google, mode) {
        Ok(spec) => spec,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": Provider::Google.as_str(),
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
                Provider::Google.as_str(),
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
                        "provider": Provider::Google.as_str(),
                        "mode": mode.as_str(),
                        "status": "launched",
                        "source": "gemini_cli",
                        "backend_cli": config.google_gemini_cli,
                        "persisted": false,
                        "action": action,
                        "launch_requested": true,
                        "launch_executed": true,
                        "launch_command": result.command,
                    })
                    .to_string()
                } else {
                    format!(
                        "auth login: provider={} mode={} status=launched source=gemini_cli backend_cli={} persisted=false action={} launch_requested=true launch_executed=true launch_command={}",
                        Provider::Google.as_str(),
                        mode.as_str(),
                        config.google_gemini_cli,
                        action,
                        result.command
                    )
                }
            }
            Err(error) => {
                if json_output {
                    serde_json::json!({
                        "command": "auth.login",
                        "provider": Provider::Google.as_str(),
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
                        Provider::Google.as_str(),
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
            "provider": Provider::Google.as_str(),
            "mode": mode.as_str(),
            "status": "ready",
            "source": "gemini_cli",
            "backend_cli": config.google_gemini_cli,
            "persisted": false,
            "action": action,
            "launch_requested": false,
            "launch_executed": false,
            "launch_command": launch_command,
        })
        .to_string();
    }
    format!(
        "auth login: provider={} mode={} status=ready source=gemini_cli backend_cli={} persisted=false action={} launch_requested=false launch_executed=false launch_command={}",
        Provider::Google.as_str(),
        mode.as_str(),
        config.google_gemini_cli,
        action,
        launch_command
    )
}
