use anyhow::{Context, Result};

use crate::{Cli, CliGatewayOpenResponsesAuthMode, CliGatewayRemoteProfile};

pub use tau_gateway::remote_profile::{
    evaluate_gateway_remote_profile_config, GatewayOpenResponsesAuthMode, GatewayRemoteProfile,
    GatewayRemoteProfileConfig, GatewayRemoteProfileReport,
};

fn map_auth_mode(mode: CliGatewayOpenResponsesAuthMode) -> GatewayOpenResponsesAuthMode {
    match mode {
        CliGatewayOpenResponsesAuthMode::Token => GatewayOpenResponsesAuthMode::Token,
        CliGatewayOpenResponsesAuthMode::PasswordSession => {
            GatewayOpenResponsesAuthMode::PasswordSession
        }
        CliGatewayOpenResponsesAuthMode::LocalhostDev => GatewayOpenResponsesAuthMode::LocalhostDev,
    }
}

fn map_remote_profile(profile: CliGatewayRemoteProfile) -> GatewayRemoteProfile {
    match profile {
        CliGatewayRemoteProfile::LocalOnly => GatewayRemoteProfile::LocalOnly,
        CliGatewayRemoteProfile::PasswordRemote => GatewayRemoteProfile::PasswordRemote,
        CliGatewayRemoteProfile::ProxyRemote => GatewayRemoteProfile::ProxyRemote,
        CliGatewayRemoteProfile::TailscaleServe => GatewayRemoteProfile::TailscaleServe,
        CliGatewayRemoteProfile::TailscaleFunnel => GatewayRemoteProfile::TailscaleFunnel,
    }
}

fn has_non_empty(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .map(|candidate| !candidate.is_empty())
        .unwrap_or(false)
}

pub(crate) fn evaluate_gateway_remote_profile(cli: &Cli) -> Result<GatewayRemoteProfileReport> {
    let config = GatewayRemoteProfileConfig {
        bind: cli.gateway_openresponses_bind.clone(),
        auth_mode: map_auth_mode(cli.gateway_openresponses_auth_mode),
        profile: map_remote_profile(cli.gateway_remote_profile),
        auth_token_configured: has_non_empty(cli.gateway_openresponses_auth_token.as_deref()),
        auth_password_configured: has_non_empty(cli.gateway_openresponses_auth_password.as_deref()),
        server_enabled: cli.gateway_openresponses_server,
    };
    evaluate_gateway_remote_profile_config(&config)
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
    anyhow::bail!(
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
    fn unit_evaluate_gateway_remote_profile_tailscale_serve_rejects_localhost_dev_auth_mode() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-serve",
            "--gateway-openresponses-auth-mode",
            "localhost-dev",
        ]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "tailscale-serve");
        assert_eq!(report.gate, "hold");
        assert!(report
            .reason_codes
            .contains(&"tailscale_serve_localhost_dev_auth_unsupported".to_string()));
    }

    #[test]
    fn functional_evaluate_gateway_remote_profile_tailscale_funnel_accepts_password_session_profile(
    ) {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-funnel",
            "--gateway-openresponses-auth-mode",
            "password-session",
            "--gateway-openresponses-auth-password",
            "edge-password",
        ]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "tailscale-funnel");
        assert_eq!(report.posture, "remote-enabled");
        assert_eq!(report.gate, "pass");
        assert!(report
            .reason_codes
            .contains(&"tailscale_funnel_password_configured".to_string()));
    }

    #[test]
    fn integration_validate_gateway_remote_profile_for_openresponses_accepts_tailscale_serve_token_profile(
    ) {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-serve",
            "--gateway-openresponses-auth-mode",
            "token",
            "--gateway-openresponses-auth-token",
            "edge-token",
        ]);
        validate_gateway_remote_profile_for_openresponses(&cli)
            .expect("valid tailscale-serve profile should pass");
    }

    #[test]
    fn regression_validate_gateway_remote_profile_for_openresponses_rejects_tailscale_funnel_missing_password(
    ) {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-funnel",
            "--gateway-openresponses-auth-mode",
            "password-session",
        ]);
        let error = validate_gateway_remote_profile_for_openresponses(&cli)
            .expect_err("tailscale-funnel without password should fail");
        assert!(error
            .to_string()
            .contains("tailscale_funnel_missing_password"));
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
