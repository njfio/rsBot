use anyhow::{bail, Context, Result};
use serde::Serialize;

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

fn secret_configured(direct_secret: Option<&str>, secret_id: Option<&str>) -> bool {
    has_non_empty(direct_secret) || has_non_empty(secret_id)
}

fn gateway_remote_profile_config_for(
    cli: &Cli,
    profile: GatewayRemoteProfile,
) -> GatewayRemoteProfileConfig {
    GatewayRemoteProfileConfig {
        bind: cli.gateway_openresponses_bind.clone(),
        auth_mode: map_auth_mode(cli.gateway_openresponses_auth_mode),
        profile,
        auth_token_configured: secret_configured(
            cli.gateway_openresponses_auth_token.as_deref(),
            cli.gateway_openresponses_auth_token_id.as_deref(),
        ),
        auth_password_configured: secret_configured(
            cli.gateway_openresponses_auth_password.as_deref(),
            cli.gateway_openresponses_auth_password_id.as_deref(),
        ),
        server_enabled: cli.gateway_openresponses_server,
    }
}

pub fn evaluate_gateway_remote_profile(cli: &Cli) -> Result<GatewayRemoteProfileReport> {
    let config =
        gateway_remote_profile_config_for(cli, map_remote_profile(cli.gateway_remote_profile));
    evaluate_gateway_remote_profile_config(&config)
}

pub const GATEWAY_REMOTE_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `GatewayRemoteExposureWorkflowPlan` used across Tau components.
pub struct GatewayRemoteExposureWorkflowPlan {
    pub workflow: String,
    pub gateway_profile: String,
    pub gate: String,
    pub risk_level: String,
    pub description: String,
    pub reason_codes: Vec<String>,
    pub preflight_checks: Vec<String>,
    pub auth_requirements: Vec<String>,
    pub bind_requirements: Vec<String>,
    pub warnings: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `GatewayRemoteExposurePlanReport` used across Tau components.
pub struct GatewayRemoteExposurePlanReport {
    pub schema_version: u32,
    pub selected_profile: String,
    pub selected_profile_gate: String,
    pub selected_profile_reason_codes: Vec<String>,
    pub plans: Vec<GatewayRemoteExposureWorkflowPlan>,
}

fn format_check(name: &str, passed: bool, detail: &str) -> String {
    format!(
        "check={} status={} detail={}",
        name,
        if passed { "pass" } else { "hold" },
        detail
    )
}

fn summarize_reason_codes(reason_codes: &[String]) -> String {
    if reason_codes.is_empty() {
        "none".to_string()
    } else {
        reason_codes.join(",")
    }
}

fn build_tailscale_serve_plan(cli: &Cli) -> Result<GatewayRemoteExposureWorkflowPlan> {
    let report = evaluate_gateway_remote_profile_config(&gateway_remote_profile_config_for(
        cli,
        GatewayRemoteProfile::TailscaleServe,
    ))?;
    let auth_mode_ok = matches!(
        cli.gateway_openresponses_auth_mode,
        CliGatewayOpenResponsesAuthMode::Token | CliGatewayOpenResponsesAuthMode::PasswordSession
    );
    let auth_secret_ok = secret_configured(
        cli.gateway_openresponses_auth_token.as_deref(),
        cli.gateway_openresponses_auth_token_id.as_deref(),
    ) || secret_configured(
        cli.gateway_openresponses_auth_password.as_deref(),
        cli.gateway_openresponses_auth_password_id.as_deref(),
    );
    let preflight_checks = vec![
        format_check(
            "gateway_openresponses_server_enabled",
            cli.gateway_openresponses_server,
            "enable --gateway-openresponses-server before exposing remotely",
        ),
        format_check(
            "loopback_bind_required",
            report.loopback_bind,
            "bind should stay loopback when using tailscale serve",
        ),
        format_check(
            "auth_mode_token_or_password_session",
            auth_mode_ok,
            "supported auth modes for tailscale serve: token or password-session",
        ),
        format_check(
            "auth_secret_configured",
            auth_secret_ok,
            "set --gateway-openresponses-auth-token/--gateway-openresponses-auth-token-id or --gateway-openresponses-auth-password/--gateway-openresponses-auth-password-id",
        ),
    ];
    Ok(GatewayRemoteExposureWorkflowPlan {
        workflow: "tailscale-serve".to_string(),
        gateway_profile: "tailscale-serve".to_string(),
        gate: report.gate,
        risk_level: report.risk_level,
        description:
            "Publish a loopback-bound gateway through tailscale serve for private network operators."
                .to_string(),
        reason_codes: report.reason_codes,
        preflight_checks,
        auth_requirements: vec![
            "token (recommended) or password-session auth mode".to_string(),
            "non-empty auth token/password value or credential-store secret id".to_string(),
        ],
        bind_requirements: vec![
            "gateway bind must stay loopback (127.0.0.1 or ::1)".to_string(),
            "avoid direct public bind for serve profile".to_string(),
        ],
        warnings: vec![
            "localhost-dev auth mode is unsupported for tailscale serve".to_string(),
            "review tailscale ACL posture before exposing operator routes".to_string(),
        ],
        commands: vec![
            "tau-rs --gateway-openresponses-server --gateway-openresponses-bind 127.0.0.1:8787 --gateway-remote-profile tailscale-serve --gateway-openresponses-auth-mode token --gateway-openresponses-auth-token <TOKEN>".to_string(),
            "tailscale serve --bg 8787 http://127.0.0.1:8787".to_string(),
            "curl -H \"Authorization: Bearer <TOKEN>\" http://127.0.0.1:8787/gateway/status".to_string(),
        ],
    })
}

fn build_tailscale_funnel_plan(cli: &Cli) -> Result<GatewayRemoteExposureWorkflowPlan> {
    let report = evaluate_gateway_remote_profile_config(&gateway_remote_profile_config_for(
        cli,
        GatewayRemoteProfile::TailscaleFunnel,
    ))?;
    let auth_mode_ok = matches!(
        cli.gateway_openresponses_auth_mode,
        CliGatewayOpenResponsesAuthMode::PasswordSession
    );
    let password_ok = secret_configured(
        cli.gateway_openresponses_auth_password.as_deref(),
        cli.gateway_openresponses_auth_password_id.as_deref(),
    );
    let preflight_checks = vec![
        format_check(
            "gateway_openresponses_server_enabled",
            cli.gateway_openresponses_server,
            "enable --gateway-openresponses-server before exposing remotely",
        ),
        format_check(
            "loopback_bind_required",
            report.loopback_bind,
            "bind should stay loopback when using tailscale funnel",
        ),
        format_check(
            "auth_mode_password_session_required",
            auth_mode_ok,
            "tailscale funnel requires --gateway-openresponses-auth-mode=password-session",
        ),
        format_check(
            "auth_password_configured",
            password_ok,
            "set --gateway-openresponses-auth-password or --gateway-openresponses-auth-password-id to a non-empty value",
        ),
    ];
    Ok(GatewayRemoteExposureWorkflowPlan {
        workflow: "tailscale-funnel".to_string(),
        gateway_profile: "tailscale-funnel".to_string(),
        gate: report.gate,
        risk_level: report.risk_level,
        description: "Expose a loopback-bound gateway through tailscale funnel with password-session auth for internet-accessible remote operators.".to_string(),
        reason_codes: report.reason_codes,
        preflight_checks,
        auth_requirements: vec![
            "password-session auth mode".to_string(),
            "non-empty auth password or credential-store secret id".to_string(),
        ],
        bind_requirements: vec![
            "gateway bind must stay loopback (127.0.0.1 or ::1)".to_string(),
            "public access should terminate through tailscale funnel, not direct bind".to_string(),
        ],
        warnings: vec![
            "funnel exposure should be treated as internet-facing and high-risk".to_string(),
            "rotate session password regularly and monitor auth/session usage".to_string(),
        ],
        commands: vec![
            "tau-rs --gateway-openresponses-server --gateway-openresponses-bind 127.0.0.1:8787 --gateway-remote-profile tailscale-funnel --gateway-openresponses-auth-mode password-session --gateway-openresponses-auth-password <PASSWORD>".to_string(),
            "tailscale funnel --bg 8787".to_string(),
            "curl -X POST http://127.0.0.1:8787/gateway/auth/session -H \"Content-Type: application/json\" -d '{\"password\":\"<PASSWORD>\"}'".to_string(),
        ],
    })
}

fn build_ssh_tunnel_fallback_plan(cli: &Cli) -> Result<GatewayRemoteExposureWorkflowPlan> {
    let report = evaluate_gateway_remote_profile_config(&gateway_remote_profile_config_for(
        cli,
        GatewayRemoteProfile::ProxyRemote,
    ))?;
    let auth_mode_ok = matches!(
        cli.gateway_openresponses_auth_mode,
        CliGatewayOpenResponsesAuthMode::Token
    );
    let token_ok = secret_configured(
        cli.gateway_openresponses_auth_token.as_deref(),
        cli.gateway_openresponses_auth_token_id.as_deref(),
    );
    let preflight_checks = vec![
        format_check(
            "gateway_openresponses_server_enabled",
            cli.gateway_openresponses_server,
            "enable --gateway-openresponses-server before exposing remotely",
        ),
        format_check(
            "loopback_bind_required",
            report.loopback_bind,
            "bind should stay loopback when using SSH tunnel fallback",
        ),
        format_check(
            "auth_mode_token_required",
            auth_mode_ok,
            "ssh fallback uses bearer-token auth for deterministic operator access",
        ),
        format_check(
            "auth_token_configured",
            token_ok,
            "set --gateway-openresponses-auth-token or --gateway-openresponses-auth-token-id to a non-empty value",
        ),
    ];
    Ok(GatewayRemoteExposureWorkflowPlan {
        workflow: "ssh-tunnel-fallback".to_string(),
        gateway_profile: "proxy-remote".to_string(),
        gate: report.gate,
        risk_level: report.risk_level,
        description:
            "Fallback workflow that keeps gateway loopback-bound and grants remote operator access over an SSH local-forward tunnel."
                .to_string(),
        reason_codes: report.reason_codes,
        preflight_checks,
        auth_requirements: vec![
            "token auth mode".to_string(),
            "non-empty bearer auth token or credential-store secret id".to_string(),
        ],
        bind_requirements: vec![
            "gateway bind must stay loopback (127.0.0.1 or ::1)".to_string(),
            "remote access must flow through SSH forwarding, not direct bind".to_string(),
        ],
        warnings: vec![
            "use short-lived SSH sessions and least-privilege host credentials".to_string(),
            "treat forwarded local port as privileged control-plane access".to_string(),
        ],
        commands: vec![
            "tau-rs --gateway-openresponses-server --gateway-openresponses-bind 127.0.0.1:8787 --gateway-remote-profile proxy-remote --gateway-openresponses-auth-mode token --gateway-openresponses-auth-token <TOKEN>".to_string(),
            "ssh -N -L 8787:127.0.0.1:8787 <user>@<host>".to_string(),
            "curl -H \"Authorization: Bearer <TOKEN>\" http://127.0.0.1:8787/gateway/status".to_string(),
        ],
    })
}

pub fn evaluate_gateway_remote_plan(cli: &Cli) -> Result<GatewayRemoteExposurePlanReport> {
    let selected_profile_report = evaluate_gateway_remote_profile(cli)?;
    if selected_profile_report.gate != "pass" {
        bail!(
            "gateway remote plan rejected: profile={} gate={} reason_codes={}",
            selected_profile_report.profile,
            selected_profile_report.gate,
            summarize_reason_codes(&selected_profile_report.reason_codes)
        );
    }
    Ok(GatewayRemoteExposurePlanReport {
        schema_version: GATEWAY_REMOTE_PLAN_SCHEMA_VERSION,
        selected_profile: selected_profile_report.profile,
        selected_profile_gate: selected_profile_report.gate,
        selected_profile_reason_codes: selected_profile_report.reason_codes,
        plans: vec![
            build_tailscale_serve_plan(cli)?,
            build_tailscale_funnel_plan(cli)?,
            build_ssh_tunnel_fallback_plan(cli)?,
        ],
    })
}

pub fn render_gateway_remote_plan_report(report: &GatewayRemoteExposurePlanReport) -> String {
    let mut lines = vec![format!(
        "gateway remote plan export: schema_version={} selected_profile={} selected_profile_gate={} selected_profile_reason_codes={}",
        report.schema_version,
        report.selected_profile,
        report.selected_profile_gate,
        summarize_reason_codes(&report.selected_profile_reason_codes)
    )];
    for plan in &report.plans {
        lines.push(format!(
            "workflow={} profile={} gate={} risk_level={} reason_codes={}",
            plan.workflow,
            plan.gateway_profile,
            plan.gate,
            plan.risk_level,
            summarize_reason_codes(&plan.reason_codes)
        ));
        lines.push(format!("description={}", plan.description));
        lines.push(format!(
            "preflight_checks={}",
            if plan.preflight_checks.is_empty() {
                "none".to_string()
            } else {
                plan.preflight_checks.join(" | ")
            }
        ));
        lines.push(format!(
            "auth_requirements={}",
            if plan.auth_requirements.is_empty() {
                "none".to_string()
            } else {
                plan.auth_requirements.join(" | ")
            }
        ));
        lines.push(format!(
            "bind_requirements={}",
            if plan.bind_requirements.is_empty() {
                "none".to_string()
            } else {
                plan.bind_requirements.join(" | ")
            }
        ));
        lines.push(format!(
            "warnings={}",
            if plan.warnings.is_empty() {
                "none".to_string()
            } else {
                plan.warnings.join(" | ")
            }
        ));
        lines.push(format!(
            "commands={}",
            if plan.commands.is_empty() {
                "none".to_string()
            } else {
                plan.commands.join(" | ")
            }
        ));
    }
    lines.join("\n")
}

pub fn render_gateway_remote_profile_report(report: &GatewayRemoteProfileReport) -> String {
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

pub fn execute_gateway_remote_profile_inspect_command(cli: &Cli) -> Result<()> {
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

pub fn execute_gateway_remote_plan_command(cli: &Cli) -> Result<()> {
    let report = evaluate_gateway_remote_plan(cli)?;
    if cli.gateway_remote_plan_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render gateway remote plan json")?
        );
    } else {
        println!("{}", render_gateway_remote_plan_report(&report));
    }
    Ok(())
}

pub fn validate_gateway_remote_profile_for_openresponses(cli: &Cli) -> Result<()> {
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
        evaluate_gateway_remote_plan, evaluate_gateway_remote_profile,
        execute_gateway_remote_plan_command, render_gateway_remote_plan_report,
        validate_gateway_remote_profile_for_openresponses,
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
    fn functional_evaluate_gateway_remote_profile_proxy_remote_accepts_token_id_profile() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "proxy-remote",
            "--gateway-openresponses-auth-mode",
            "token",
            "--gateway-openresponses-auth-token-id",
            "gateway-openresponses-auth-token",
        ]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "proxy-remote");
        assert_eq!(report.gate, "pass");
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
    fn functional_evaluate_gateway_remote_profile_tailscale_funnel_accepts_password_id_profile() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-funnel",
            "--gateway-openresponses-auth-mode",
            "password-session",
            "--gateway-openresponses-auth-password-id",
            "gateway-openresponses-auth-password",
        ]);
        let report = evaluate_gateway_remote_profile(&cli).expect("evaluate");
        assert_eq!(report.profile, "tailscale-funnel");
        assert_eq!(report.gate, "pass");
        assert!(report.auth_password_configured);
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

    #[test]
    fn unit_evaluate_gateway_remote_plan_returns_three_workflows() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-serve",
            "--gateway-openresponses-auth-mode",
            "token",
            "--gateway-openresponses-auth-token",
            "edge-token",
            "--gateway-openresponses-bind",
            "127.0.0.1:8787",
            "--gateway-remote-plan",
        ]);
        let report = evaluate_gateway_remote_plan(&cli).expect("plan should evaluate");
        assert_eq!(
            report.schema_version,
            super::GATEWAY_REMOTE_PLAN_SCHEMA_VERSION
        );
        assert_eq!(report.selected_profile, "tailscale-serve");
        assert_eq!(report.plans.len(), 3);
        assert!(report
            .plans
            .iter()
            .any(|plan| plan.workflow == "tailscale-serve"));
        assert!(report
            .plans
            .iter()
            .any(|plan| plan.workflow == "tailscale-funnel"));
        assert!(report
            .plans
            .iter()
            .any(|plan| plan.workflow == "ssh-tunnel-fallback"));
    }

    #[test]
    fn functional_render_gateway_remote_plan_report_includes_workflow_sections() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "proxy-remote",
            "--gateway-openresponses-auth-mode",
            "token",
            "--gateway-openresponses-auth-token",
            "edge-token",
            "--gateway-openresponses-bind",
            "127.0.0.1:8787",
            "--gateway-remote-plan",
        ]);
        let report = evaluate_gateway_remote_plan(&cli).expect("plan should evaluate");
        let rendered = render_gateway_remote_plan_report(&report);
        assert!(rendered.contains("gateway remote plan export:"));
        assert!(rendered.contains("workflow=tailscale-serve"));
        assert!(rendered.contains("workflow=tailscale-funnel"));
        assert!(rendered.contains("workflow=ssh-tunnel-fallback"));
        assert!(rendered.contains("commands="));
        assert!(rendered.contains("preflight_checks="));
    }

    #[test]
    fn integration_execute_gateway_remote_plan_command_supports_json_mode() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-funnel",
            "--gateway-openresponses-auth-mode",
            "password-session",
            "--gateway-openresponses-auth-password",
            "edge-password",
            "--gateway-openresponses-bind",
            "127.0.0.1:8787",
            "--gateway-remote-plan",
            "--gateway-remote-plan-json",
        ]);
        execute_gateway_remote_plan_command(&cli)
            .expect("gateway remote plan command should succeed in json mode");
    }

    #[test]
    fn regression_evaluate_gateway_remote_plan_rejects_hold_selected_profile_configuration() {
        let cli = parse_cli_with_stack(&[
            "tau-rs",
            "--gateway-openresponses-server",
            "--gateway-remote-profile",
            "tailscale-funnel",
            "--gateway-openresponses-auth-mode",
            "password-session",
            "--gateway-openresponses-bind",
            "127.0.0.1:8787",
            "--gateway-remote-plan",
        ]);
        let error = evaluate_gateway_remote_plan(&cli)
            .expect_err("missing password for selected profile should fail closed");
        assert!(error
            .to_string()
            .contains("gateway remote plan rejected: profile=tailscale-funnel gate=hold"));
        assert!(error
            .to_string()
            .contains("tailscale_funnel_missing_password"));
    }
}
