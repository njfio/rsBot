//! Shared runtime helpers for auth command launch/redaction flows.

use std::{process::Command, time::Duration};

use anyhow::{bail, Context, Result};
use tau_ai::Provider;
use wait_timeout::ChildExt;

use crate::{AuthCommandConfig, ProviderAuthMethod};

/// Collect trimmed non-empty secret values from optional candidate list.
pub(super) fn collect_non_empty_secrets(
    candidates: Vec<(&'static str, Option<String>)>,
) -> Vec<String> {
    candidates
        .into_iter()
        .filter_map(|(_source, value)| value)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

pub(super) fn redact_known_secrets(text: String, secrets: &[String]) -> String {
    let mut redacted = text;
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "<redacted>");
        }
    }
    redacted
}

pub(super) const AUTH_LOGIN_LAUNCH_TIMEOUT_MS: u64 = 300_000;

pub(super) struct AuthLoginLaunchSpec {
    pub(super) executable: String,
    pub(super) args: Vec<String>,
    pub(super) timeout_ms: u64,
}

pub(super) struct AuthLoginLaunchResult {
    pub(super) command: String,
}

fn shell_quote_token(token: &str) -> String {
    if token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':'))
    {
        return token.to_string();
    }
    format!("'{}'", token.replace('\'', "'\"'\"'"))
}

pub(super) fn render_launch_command(executable: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len().saturating_add(1));
    parts.push(shell_quote_token(executable));
    parts.extend(args.iter().map(|arg| shell_quote_token(arg)));
    parts.join(" ")
}

/// Execute auth login launch command with timeout and explicit status handling.
pub(super) fn execute_auth_login_launch(
    spec: &AuthLoginLaunchSpec,
) -> Result<AuthLoginLaunchResult> {
    let mut command = Command::new(spec.executable.trim());
    command.args(spec.args.iter().map(String::as_str));
    command.stdin(std::process::Stdio::inherit());
    command.stdout(std::process::Stdio::inherit());
    command.stderr(std::process::Stdio::inherit());
    let command_str = render_launch_command(spec.executable.as_str(), &spec.args);

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn login command {}", command_str))?;

    let timeout = Duration::from_millis(spec.timeout_ms.max(1));
    let status = match child
        .wait_timeout(timeout)
        .with_context(|| format!("failed while waiting for login command {}", command_str))?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "login command timed out after {} ms: {}",
                spec.timeout_ms.max(1),
                command_str
            );
        }
    };
    if !status.success() {
        let code = status
            .code()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "terminated_by_signal".to_string());
        bail!("login command exited with status {}: {}", code, command_str);
    }

    Ok(AuthLoginLaunchResult {
        command: command_str,
    })
}

/// Build auth login launch spec for provider/mode pair and validate support matrix.
pub(super) fn build_auth_login_launch_spec(
    config: &AuthCommandConfig,
    provider: Provider,
    mode: ProviderAuthMethod,
) -> Result<AuthLoginLaunchSpec> {
    let timeout_ms = AUTH_LOGIN_LAUNCH_TIMEOUT_MS;
    match (provider, mode) {
        (
            Provider::OpenAi,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken,
        ) => Ok(AuthLoginLaunchSpec {
            executable: config.openai_codex_cli.clone(),
            args: vec!["--login".to_string()],
            timeout_ms,
        }),
        (
            Provider::Anthropic,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken,
        ) => Ok(AuthLoginLaunchSpec {
            executable: config.anthropic_claude_cli.clone(),
            args: Vec::new(),
            timeout_ms,
        }),
        (Provider::Google, ProviderAuthMethod::OauthToken) => Ok(AuthLoginLaunchSpec {
            executable: config.google_gemini_cli.clone(),
            args: Vec::new(),
            timeout_ms,
        }),
        (Provider::Google, ProviderAuthMethod::Adc) => Ok(AuthLoginLaunchSpec {
            executable: config.google_gcloud_cli.clone(),
            args: vec![
                "auth".to_string(),
                "application-default".to_string(),
                "login".to_string(),
            ],
            timeout_ms,
        }),
        _ => bail!(
            "--launch is only supported for openai oauth-token/session-token, anthropic oauth-token/session-token, and google oauth-token/adc"
        ),
    }
}
