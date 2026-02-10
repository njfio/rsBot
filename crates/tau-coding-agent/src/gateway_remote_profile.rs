use super::*;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GatewayRemoteProfileReport {
    pub(crate) profile: String,
    pub(crate) posture: String,
    pub(crate) gate: String,
    pub(crate) risk_level: String,
    pub(crate) server_enabled: bool,
    pub(crate) bind: String,
    pub(crate) bind_ip: String,
    pub(crate) loopback_bind: bool,
    pub(crate) auth_mode: String,
    pub(crate) auth_token_configured: bool,
    pub(crate) auth_password_configured: bool,
    pub(crate) remote_enabled: bool,
    pub(crate) reason_codes: Vec<String>,
    pub(crate) recommendations: Vec<String>,
}

fn has_non_empty(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .map(|candidate| !candidate.is_empty())
        .unwrap_or(false)
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if list.iter().any(|existing| existing == value) {
        return;
    }
    list.push(value.to_string());
}

fn mark_hold(
    gate: &mut &'static str,
    reason_codes: &mut Vec<String>,
    recommendations: &mut Vec<String>,
    reason_code: &str,
    recommendation: &str,
) {
    *gate = "hold";
    push_unique(reason_codes, reason_code);
    push_unique(recommendations, recommendation);
}

pub(crate) fn evaluate_gateway_remote_profile(cli: &Cli) -> Result<GatewayRemoteProfileReport> {
    let bind_addr = crate::gateway_openresponses::validate_gateway_openresponses_bind(
        &cli.gateway_openresponses_bind,
    )
    .with_context(|| {
        format!(
            "failed to evaluate gateway remote profile bind '{}'",
            cli.gateway_openresponses_bind
        )
    })?;
    let loopback_bind = bind_addr.ip().is_loopback();
    let auth_mode = cli.gateway_openresponses_auth_mode.as_str();
    let profile = cli.gateway_remote_profile.as_str();
    let auth_token_configured = has_non_empty(cli.gateway_openresponses_auth_token.as_deref());
    let auth_password_configured =
        has_non_empty(cli.gateway_openresponses_auth_password.as_deref());

    let mut gate = "pass";
    let mut reason_codes = Vec::new();
    let mut recommendations = Vec::new();
    let remote_enabled = !matches!(
        cli.gateway_remote_profile,
        CliGatewayRemoteProfile::LocalOnly
    );

    push_unique(
        &mut reason_codes,
        &format!("profile_{}", profile.replace('-', "_")),
    );
    if cli.gateway_openresponses_server {
        push_unique(&mut reason_codes, "server_enabled");
    } else {
        push_unique(&mut reason_codes, "server_disabled_inspect_only");
    }

    match cli.gateway_remote_profile {
        CliGatewayRemoteProfile::LocalOnly => {
            if loopback_bind {
                push_unique(&mut reason_codes, "local_only_loopback_bind");
            } else {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "local_only_non_loopback_bind",
                    "set --gateway-openresponses-bind to a loopback address for local-only profile",
                );
            }
            push_unique(
                &mut reason_codes,
                &format!("local_only_auth_{}", auth_mode.replace('-', "_")),
            );
        }
        CliGatewayRemoteProfile::PasswordRemote => {
            if cli.gateway_openresponses_auth_mode
                != CliGatewayOpenResponsesAuthMode::PasswordSession
            {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "password_remote_auth_mode_mismatch",
                    "set --gateway-openresponses-auth-mode password-session",
                );
            } else {
                push_unique(&mut reason_codes, "password_remote_password_session_auth");
            }
            if auth_password_configured {
                push_unique(&mut reason_codes, "password_remote_password_configured");
            } else {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "password_remote_missing_password",
                    "set --gateway-openresponses-auth-password to a non-empty value",
                );
            }
            if loopback_bind {
                push_unique(&mut reason_codes, "password_remote_loopback_bind");
                push_unique(
                    &mut recommendations,
                    "publish loopback bind through a trusted tunnel or reverse proxy for remote operators",
                );
            } else {
                push_unique(&mut reason_codes, "password_remote_non_loopback_bind");
            }
        }
        CliGatewayRemoteProfile::ProxyRemote => {
            if cli.gateway_openresponses_auth_mode != CliGatewayOpenResponsesAuthMode::Token {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "proxy_remote_auth_mode_mismatch",
                    "set --gateway-openresponses-auth-mode token",
                );
            } else {
                push_unique(&mut reason_codes, "proxy_remote_token_auth");
            }
            if auth_token_configured {
                push_unique(&mut reason_codes, "proxy_remote_token_configured");
            } else {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "proxy_remote_missing_token",
                    "set --gateway-openresponses-auth-token to a non-empty bearer token",
                );
            }
            if loopback_bind {
                push_unique(&mut reason_codes, "proxy_remote_loopback_bind");
                push_unique(
                    &mut recommendations,
                    "keep loopback bind and expose access through a trusted reverse proxy/tunnel",
                );
            } else {
                push_unique(&mut reason_codes, "proxy_remote_non_loopback_bind");
            }
        }
    }

    let posture = if remote_enabled {
        "remote-enabled"
    } else {
        "local-only"
    }
    .to_string();
    let risk_level = if gate == "hold" {
        "high"
    } else if remote_enabled && !loopback_bind {
        "elevated"
    } else if remote_enabled {
        "moderate"
    } else {
        "low"
    }
    .to_string();

    Ok(GatewayRemoteProfileReport {
        profile: profile.to_string(),
        posture,
        gate: gate.to_string(),
        risk_level,
        server_enabled: cli.gateway_openresponses_server,
        bind: cli.gateway_openresponses_bind.clone(),
        bind_ip: bind_addr.ip().to_string(),
        loopback_bind,
        auth_mode: auth_mode.to_string(),
        auth_token_configured,
        auth_password_configured,
        remote_enabled,
        reason_codes,
        recommendations,
    })
}

pub(crate) fn render_gateway_remote_profile_report(report: &GatewayRemoteProfileReport) -> String {
    let reason_codes = if report.reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.reason_codes.join(",")
    };
    let recommendations = if report.recommendations.is_empty() {
        "none".to_string()
    } else {
        report.recommendations.join(" | ")
    };
    format!(
        "gateway remote profile inspect: profile={} posture={} gate={} risk_level={} server_enabled={} remote_enabled={} bind={} bind_ip={} loopback_bind={} auth_mode={} auth_token_configured={} auth_password_configured={} reason_codes={} recommendations={}",
        report.profile,
        report.posture,
        report.gate,
        report.risk_level,
        report.server_enabled,
        report.remote_enabled,
        report.bind,
        report.bind_ip,
        report.loopback_bind,
        report.auth_mode,
        report.auth_token_configured,
        report.auth_password_configured,
        reason_codes,
        recommendations
    )
}

pub(crate) fn execute_gateway_remote_profile_inspect_command(cli: &Cli) -> Result<()> {
    let report = evaluate_gateway_remote_profile(cli)?;
    if cli.gateway_remote_profile_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render gateway remote profile inspect json")?
        );
    } else {
        println!("{}", render_gateway_remote_profile_report(&report));
    }
    Ok(())
}

pub(crate) fn validate_gateway_remote_profile_for_openresponses(cli: &Cli) -> Result<()> {
    if !cli.gateway_openresponses_server {
        return Ok(());
    }
    let report = evaluate_gateway_remote_profile(cli)?;
    if report.gate == "pass" {
        return Ok(());
    }
    let reason_codes = if report.reason_codes.is_empty() {
        "unknown".to_string()
    } else {
        report.reason_codes.join(",")
    };
    bail!(
        "gateway remote profile rejected: profile={} gate={} reason_codes={} (run --gateway-remote-profile-inspect for full posture details)",
        report.profile,
        report.gate,
        reason_codes
    );
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_gateway_remote_profile, validate_gateway_remote_profile_for_openresponses,
    };
    use crate::Cli;
    use clap::Parser;

    fn parse_cli_with_stack(args: &[&str]) -> Cli {
        let owned_args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(owned_args))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    #[test]
    fn unit_evaluate_gateway_remote_profile_default_is_local_only_pass() {
        let cli = parse_cli_with_stack(&["tau-rs"]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "local-only");
        assert_eq!(report.posture, "local-only");
        assert_eq!(report.gate, "pass");
        assert!(report.loopback_bind);
        assert!(!report.remote_enabled);
        assert!(report
            .reason_codes
            .contains(&"local_only_loopback_bind".to_string()));
    }

    #[test]
    fn unit_evaluate_gateway_remote_profile_password_remote_requires_password_session() {
        let cli = parse_cli_with_stack(&["tau-rs", "--gateway-remote-profile", "password-remote"]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "password-remote");
        assert_eq!(report.gate, "hold");
        assert!(report
            .reason_codes
            .contains(&"password_remote_auth_mode_mismatch".to_string()));
    }

    #[test]
    fn functional_evaluate_gateway_remote_profile_proxy_remote_accepts_loopback_token_profile() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "proxy-remote",
            "--gateway-openresponses-auth-mode",
            "token",
            "--gateway-openresponses-auth-token",
            "edge-proxy-token",
        ]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "proxy-remote");
        assert_eq!(report.posture, "remote-enabled");
        assert_eq!(report.gate, "pass");
        assert_eq!(report.auth_mode, "token");
        assert!(report.auth_token_configured);
        assert!(report
            .reason_codes
            .contains(&"proxy_remote_token_configured".to_string()));
    }

    #[test]
    fn regression_validate_gateway_remote_profile_for_openresponses_rejects_unsafe_combo() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-openresponses-bind",
            "0.0.0.0:8787",
            "--gateway-openresponses-auth-mode",
            "token",
            "--gateway-openresponses-auth-token",
            "token-value",
            "--gateway-remote-profile",
            "local-only",
        ]);
        let error = validate_gateway_remote_profile_for_openresponses(&cli)
            .expect_err("unsafe local-only non-loopback bind should fail");
        assert!(error
            .to_string()
            .contains("gateway remote profile rejected"));
        assert!(error.to_string().contains("local_only_non_loopback_bind"));
    }
}
