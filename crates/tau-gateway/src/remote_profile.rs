//! Remote profile resolution for gateway OpenResponses deployments.
//!
//! This module captures remote-profile auth/mode selections and rendering logic
//! used by gateway bootstrap flows. It keeps auth-mode decisions explicit for
//! token and password-session paths.

use std::net::SocketAddr;

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `GatewayOpenResponsesAuthMode` values.
pub enum GatewayOpenResponsesAuthMode {
    Token,
    PasswordSession,
    LocalhostDev,
}

impl GatewayOpenResponsesAuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::PasswordSession => "password-session",
            Self::LocalhostDev => "localhost-dev",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `GatewayRemoteProfile` values.
pub enum GatewayRemoteProfile {
    LocalOnly,
    PasswordRemote,
    ProxyRemote,
    TailscaleServe,
    TailscaleFunnel,
}

impl GatewayRemoteProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnly => "local-only",
            Self::PasswordRemote => "password-remote",
            Self::ProxyRemote => "proxy-remote",
            Self::TailscaleServe => "tailscale-serve",
            Self::TailscaleFunnel => "tailscale-funnel",
        }
    }
}

#[derive(Debug, Clone)]
/// Public struct `GatewayRemoteProfileConfig` used across Tau components.
pub struct GatewayRemoteProfileConfig {
    pub bind: String,
    pub auth_mode: GatewayOpenResponsesAuthMode,
    pub profile: GatewayRemoteProfile,
    pub auth_token_configured: bool,
    pub auth_password_configured: bool,
    pub server_enabled: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `GatewayRemoteProfileReport` used across Tau components.
pub struct GatewayRemoteProfileReport {
    pub profile: String,
    pub posture: String,
    pub gate: String,
    pub risk_level: String,
    pub server_enabled: bool,
    pub bind: String,
    pub bind_ip: String,
    pub loopback_bind: bool,
    pub auth_mode: String,
    pub auth_token_configured: bool,
    pub auth_password_configured: bool,
    pub remote_enabled: bool,
    pub reason_codes: Vec<String>,
    pub recommendations: Vec<String>,
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

pub fn validate_gateway_openresponses_bind(bind: &str) -> Result<SocketAddr> {
    bind.parse::<SocketAddr>()
        .with_context(|| format!("invalid gateway socket address '{bind}'"))
}

pub fn evaluate_gateway_remote_profile_config(
    config: &GatewayRemoteProfileConfig,
) -> Result<GatewayRemoteProfileReport> {
    let bind_addr = validate_gateway_openresponses_bind(&config.bind).with_context(|| {
        format!(
            "failed to evaluate gateway remote profile bind '{}'",
            config.bind
        )
    })?;
    let loopback_bind = bind_addr.ip().is_loopback();
    let auth_mode = config.auth_mode.as_str();
    let profile = config.profile.as_str();

    let mut gate = "pass";
    let mut reason_codes = Vec::new();
    let mut recommendations = Vec::new();
    let remote_enabled = !matches!(config.profile, GatewayRemoteProfile::LocalOnly);

    push_unique(
        &mut reason_codes,
        &format!("profile_{}", profile.replace('-', "_")),
    );
    if config.server_enabled {
        push_unique(&mut reason_codes, "server_enabled");
    } else {
        push_unique(&mut reason_codes, "server_disabled_inspect_only");
    }

    match config.profile {
        GatewayRemoteProfile::LocalOnly => {
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
        GatewayRemoteProfile::PasswordRemote => {
            if config.auth_mode != GatewayOpenResponsesAuthMode::PasswordSession {
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
            if config.auth_password_configured {
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
        GatewayRemoteProfile::ProxyRemote => {
            if config.auth_mode != GatewayOpenResponsesAuthMode::Token {
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
            if config.auth_token_configured {
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
        GatewayRemoteProfile::TailscaleServe => {
            if loopback_bind {
                push_unique(&mut reason_codes, "tailscale_serve_loopback_bind");
            } else {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "tailscale_serve_non_loopback_bind",
                    "set --gateway-openresponses-bind to a loopback address and expose through tailscale serve",
                );
            }
            match config.auth_mode {
                GatewayOpenResponsesAuthMode::Token => {
                    push_unique(&mut reason_codes, "tailscale_serve_token_auth");
                    if config.auth_token_configured {
                        push_unique(&mut reason_codes, "tailscale_serve_token_configured");
                    } else {
                        mark_hold(
                            &mut gate,
                            &mut reason_codes,
                            &mut recommendations,
                            "tailscale_serve_missing_token",
                            "set --gateway-openresponses-auth-token to a non-empty bearer token",
                        );
                    }
                }
                GatewayOpenResponsesAuthMode::PasswordSession => {
                    push_unique(&mut reason_codes, "tailscale_serve_password_session_auth");
                    if config.auth_password_configured {
                        push_unique(&mut reason_codes, "tailscale_serve_password_configured");
                    } else {
                        mark_hold(
                            &mut gate,
                            &mut reason_codes,
                            &mut recommendations,
                            "tailscale_serve_missing_password",
                            "set --gateway-openresponses-auth-password to a non-empty value",
                        );
                    }
                }
                GatewayOpenResponsesAuthMode::LocalhostDev => {
                    mark_hold(
                        &mut gate,
                        &mut reason_codes,
                        &mut recommendations,
                        "tailscale_serve_localhost_dev_auth_unsupported",
                        "set --gateway-openresponses-auth-mode token or password-session for tailscale serve",
                    );
                }
            }
            push_unique(
                &mut recommendations,
                "use tailscale serve to publish gateway while keeping loopback bind on host",
            );
        }
        GatewayRemoteProfile::TailscaleFunnel => {
            if loopback_bind {
                push_unique(&mut reason_codes, "tailscale_funnel_loopback_bind");
            } else {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "tailscale_funnel_non_loopback_bind",
                    "set --gateway-openresponses-bind to a loopback address and expose through tailscale funnel",
                );
            }
            if config.auth_mode != GatewayOpenResponsesAuthMode::PasswordSession {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "tailscale_funnel_auth_mode_mismatch",
                    "set --gateway-openresponses-auth-mode password-session",
                );
            } else {
                push_unique(&mut reason_codes, "tailscale_funnel_password_session_auth");
            }
            if config.auth_password_configured {
                push_unique(&mut reason_codes, "tailscale_funnel_password_configured");
            } else {
                mark_hold(
                    &mut gate,
                    &mut reason_codes,
                    &mut recommendations,
                    "tailscale_funnel_missing_password",
                    "set --gateway-openresponses-auth-password to a non-empty value",
                );
            }
            push_unique(
                &mut recommendations,
                "require password-session auth before enabling tailscale funnel exposure",
            );
        }
    }

    let posture = if remote_enabled {
        "remote-enabled"
    } else {
        "local-only"
    };
    let risk_level = if gate == "hold" {
        "high"
    } else if remote_enabled && !loopback_bind {
        "elevated"
    } else if remote_enabled {
        "moderate"
    } else {
        "low"
    };

    Ok(GatewayRemoteProfileReport {
        profile: profile.to_string(),
        posture: posture.to_string(),
        gate: gate.to_string(),
        risk_level: risk_level.to_string(),
        server_enabled: config.server_enabled,
        bind: config.bind.clone(),
        bind_ip: bind_addr.ip().to_string(),
        loopback_bind,
        auth_mode: auth_mode.to_string(),
        auth_token_configured: config.auth_token_configured,
        auth_password_configured: config.auth_password_configured,
        remote_enabled,
        reason_codes,
        recommendations,
    })
}
