//! Runtime wrappers for provider auth command execution.
//!
//! These helpers run external auth commands with timeout enforcement and parse
//! output into structured records. Command failures and malformed output are
//! surfaced as typed errors for login/status/logout flows.

use anyhow::{bail, Result};
use serde::Serialize;
use tau_ai::Provider;
use tau_core::current_unix_timestamp;

use crate::*;

mod anthropic_backend;
mod google_backend;
mod openai_backend;
mod shared_runtime_core;

use anthropic_backend::execute_anthropic_login_backend_ready;
use google_backend::execute_google_login_backend_ready;
use openai_backend::execute_openai_login_backend_ready;
use shared_runtime_core::{
    build_auth_login_launch_spec, collect_non_empty_secrets, redact_known_secrets,
};

/// Public `const` `AUTH_USAGE` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub const AUTH_USAGE: &str = "usage: /auth <login|reauth|status|logout|matrix> ...";
/// Public `const` `AUTH_LOGIN_USAGE` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub const AUTH_LOGIN_USAGE: &str =
    "usage: /auth login <provider> [--mode <mode>] [--launch] [--json]";
/// Public `const` `AUTH_REAUTH_USAGE` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub const AUTH_REAUTH_USAGE: &str =
    "usage: /auth reauth <provider> [--mode <mode>] [--launch] [--json]";
/// Public `const` `AUTH_STATUS_USAGE` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub const AUTH_STATUS_USAGE: &str =
    "usage: /auth status [provider] [--mode <mode>] [--mode-support <all|supported|unsupported>] [--availability <all|available|unavailable>] [--state <state>] [--source-kind <all|flag|env|credential-store|none>] [--revoked <all|revoked|not-revoked>] [--json]";
/// Public `const` `AUTH_LOGOUT_USAGE` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub const AUTH_LOGOUT_USAGE: &str = "usage: /auth logout <provider> [--json]";
/// Public `const` `AUTH_MATRIX_USAGE` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub const AUTH_MATRIX_USAGE: &str =
    "usage: /auth matrix [provider] [--mode <mode>] [--mode-support <all|supported|unsupported>] [--availability <all|available|unavailable>] [--state <state>] [--source-kind <all|flag|env|credential-store|none>] [--revoked <all|revoked|not-revoked>] [--json]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `AuthMatrixAvailabilityFilter` values.
pub enum AuthMatrixAvailabilityFilter {
    All,
    Available,
    Unavailable,
}

impl AuthMatrixAvailabilityFilter {
    /// Public `fn` `as_str` in `tau-provider`.
    ///
    /// This item is part of the Wave 2 API surface for M23 documentation uplift.
    /// Callers rely on its contract and failure semantics remaining stable.
    /// Update this comment if behavior or integration expectations change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `AuthMatrixModeSupportFilter` values.
pub enum AuthMatrixModeSupportFilter {
    All,
    Supported,
    Unsupported,
}

impl AuthMatrixModeSupportFilter {
    /// Public `fn` `as_str` in `tau-provider`.
    ///
    /// This item is part of the Wave 2 API surface for M23 documentation uplift.
    /// Callers rely on its contract and failure semantics remaining stable.
    /// Update this comment if behavior or integration expectations change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `AuthSourceKindFilter` values.
pub enum AuthSourceKindFilter {
    All,
    Flag,
    Env,
    CredentialStore,
    None,
}

impl AuthSourceKindFilter {
    /// Public `fn` `as_str` in `tau-provider`.
    ///
    /// This item is part of the Wave 2 API surface for M23 documentation uplift.
    /// Callers rely on its contract and failure semantics remaining stable.
    /// Update this comment if behavior or integration expectations change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Flag => "flag",
            Self::Env => "env",
            Self::CredentialStore => "credential_store",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `AuthRevokedFilter` values.
pub enum AuthRevokedFilter {
    All,
    Revoked,
    NotRevoked,
}

impl AuthRevokedFilter {
    /// Public `fn` `as_str` in `tau-provider`.
    ///
    /// This item is part of the Wave 2 API surface for M23 documentation uplift.
    /// Callers rely on its contract and failure semantics remaining stable.
    /// Update this comment if behavior or integration expectations change.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Revoked => "revoked",
            Self::NotRevoked => "not_revoked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `AuthCommand` values.
pub enum AuthCommand {
    Login {
        provider: Provider,
        mode: Option<ProviderAuthMethod>,
        launch: bool,
        json_output: bool,
    },
    Reauth {
        provider: Provider,
        mode: Option<ProviderAuthMethod>,
        launch: bool,
        json_output: bool,
    },
    Status {
        provider: Option<Provider>,
        mode: Option<ProviderAuthMethod>,
        mode_support: AuthMatrixModeSupportFilter,
        availability: AuthMatrixAvailabilityFilter,
        state: Option<String>,
        source_kind: AuthSourceKindFilter,
        revoked: AuthRevokedFilter,
        json_output: bool,
    },
    Logout {
        provider: Provider,
        json_output: bool,
    },
    Matrix {
        provider: Option<Provider>,
        mode: Option<ProviderAuthMethod>,
        mode_support: AuthMatrixModeSupportFilter,
        availability: AuthMatrixAvailabilityFilter,
        state: Option<String>,
        source_kind: AuthSourceKindFilter,
        revoked: AuthRevokedFilter,
        json_output: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `AuthQueryFilters` used across Tau components.
pub struct AuthQueryFilters {
    pub mode: Option<ProviderAuthMethod>,
    pub mode_support: AuthMatrixModeSupportFilter,
    pub availability: AuthMatrixAvailabilityFilter,
    pub state: Option<String>,
    pub source_kind: AuthSourceKindFilter,
    pub revoked: AuthRevokedFilter,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `AuthStatusRow` used across Tau components.
pub struct AuthStatusRow {
    provider: String,
    mode: String,
    mode_supported: bool,
    supported: bool,
    available: bool,
    state: String,
    source: String,
    reason_code: String,
    reason: String,
    expires_unix: Option<u64>,
    expires: Option<u64>,
    expires_in_seconds: Option<i64>,
    expiry_state: String,
    revoked: bool,
    refreshable: bool,
    backend_required: bool,
    backend: String,
    backend_health: String,
    backend_reason_code: String,
    fallback_order: String,
    fallback_mode: String,
    fallback_available: bool,
    fallback_reason_code: String,
    fallback_hint: String,
    reauth_prerequisites: String,
    reauth_required: bool,
    reauth_hint: String,
    next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthBackendProbe {
    required: bool,
    backend: String,
    health: String,
    reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthFallbackPlan {
    order: String,
    mode: String,
    available: bool,
    reason_code: String,
    hint: String,
    reauth_prerequisites: String,
}

const AUTH_MATRIX_PROVIDERS: [Provider; 3] =
    [Provider::OpenAi, Provider::Anthropic, Provider::Google];
const AUTH_MATRIX_MODES: [ProviderAuthMethod; 4] = [
    ProviderAuthMethod::ApiKey,
    ProviderAuthMethod::OauthToken,
    ProviderAuthMethod::Adc,
    ProviderAuthMethod::SessionToken,
];

/// Public `fn` `parse_auth_provider` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_provider(token: &str) -> Result<Provider> {
    match token.trim().to_ascii_lowercase().as_str() {
        "openai" | "openrouter" | "groq" | "xai" | "mistral" | "azure" | "azure-openai" => {
            Ok(Provider::OpenAi)
        }
        "anthropic" => Ok(Provider::Anthropic),
        "google" => Ok(Provider::Google),
        other => bail!(
            "unknown provider '{}'; supported providers: openai, openrouter (alias), groq (alias), xai (alias), mistral (alias), azure/azure-openai (alias), anthropic, google",
            other
        ),
    }
}

/// Public `fn` `parse_provider_auth_method_token` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_provider_auth_method_token(token: &str) -> Result<ProviderAuthMethod> {
    match token.trim().to_ascii_lowercase().as_str() {
        "api-key" | "api_key" => Ok(ProviderAuthMethod::ApiKey),
        "oauth-token" | "oauth_token" => Ok(ProviderAuthMethod::OauthToken),
        "adc" => Ok(ProviderAuthMethod::Adc),
        "session-token" | "session_token" => Ok(ProviderAuthMethod::SessionToken),
        other => bail!(
            "unknown auth mode '{}'; supported modes: api-key, oauth-token, adc, session-token",
            other
        ),
    }
}

/// Public `fn` `parse_auth_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_command(command_args: &str) -> Result<AuthCommand> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{AUTH_USAGE}");
    }

    match tokens[0] {
        "login" => {
            if tokens.len() < 2 {
                bail!("{AUTH_LOGIN_USAGE}");
            }
            let provider = parse_auth_provider(tokens[1])?;
            let mut mode = None;
            let mut launch = false;
            let mut json_output = false;

            let mut index = 2usize;
            while index < tokens.len() {
                match tokens[index] {
                    "--json" => {
                        json_output = true;
                        index += 1;
                    }
                    "--mode" => {
                        if mode.is_some() {
                            bail!("duplicate --mode flag; {AUTH_LOGIN_USAGE}");
                        }
                        let Some(raw_mode) = tokens.get(index + 1) else {
                            bail!("missing auth mode after --mode; {AUTH_LOGIN_USAGE}");
                        };
                        mode = Some(parse_provider_auth_method_token(raw_mode)?);
                        index += 2;
                    }
                    "--launch" => {
                        if launch {
                            bail!("duplicate --launch flag; {AUTH_LOGIN_USAGE}");
                        }
                        launch = true;
                        index += 1;
                    }
                    other => bail!("unexpected argument '{}'; {AUTH_LOGIN_USAGE}", other),
                }
            }

            Ok(AuthCommand::Login {
                provider,
                mode,
                launch,
                json_output,
            })
        }
        "reauth" => {
            if tokens.len() < 2 {
                bail!("{AUTH_REAUTH_USAGE}");
            }
            let provider = parse_auth_provider(tokens[1])?;
            let mut mode = None;
            let mut launch = false;
            let mut json_output = false;

            let mut index = 2usize;
            while index < tokens.len() {
                match tokens[index] {
                    "--json" => {
                        json_output = true;
                        index += 1;
                    }
                    "--mode" => {
                        if mode.is_some() {
                            bail!("duplicate --mode flag; {AUTH_REAUTH_USAGE}");
                        }
                        let Some(raw_mode) = tokens.get(index + 1) else {
                            bail!("missing auth mode after --mode; {AUTH_REAUTH_USAGE}");
                        };
                        mode = Some(parse_provider_auth_method_token(raw_mode)?);
                        index += 2;
                    }
                    "--launch" => {
                        if launch {
                            bail!("duplicate --launch flag; {AUTH_REAUTH_USAGE}");
                        }
                        launch = true;
                        index += 1;
                    }
                    other => bail!("unexpected argument '{}'; {AUTH_REAUTH_USAGE}", other),
                }
            }

            Ok(AuthCommand::Reauth {
                provider,
                mode,
                launch,
                json_output,
            })
        }
        "status" => {
            let mut provider: Option<Provider> = None;
            let mut mode: Option<ProviderAuthMethod> = None;
            let mut mode_support = AuthMatrixModeSupportFilter::All;
            let mut mode_support_explicit = false;
            let mut availability = AuthMatrixAvailabilityFilter::All;
            let mut availability_explicit = false;
            let mut state: Option<String> = None;
            let mut source_kind = AuthSourceKindFilter::All;
            let mut source_kind_explicit = false;
            let mut revoked = AuthRevokedFilter::All;
            let mut revoked_explicit = false;
            let mut json_output = false;
            let mut index = 1usize;
            while index < tokens.len() {
                match tokens[index] {
                    "--json" => {
                        json_output = true;
                        index += 1;
                    }
                    "--mode" => {
                        if mode.is_some() {
                            bail!("duplicate --mode flag; {AUTH_STATUS_USAGE}");
                        }
                        let Some(raw_mode) = tokens.get(index + 1) else {
                            bail!("missing auth mode after --mode; {AUTH_STATUS_USAGE}");
                        };
                        mode = Some(parse_provider_auth_method_token(raw_mode)?);
                        index += 2;
                    }
                    "--mode-support" => {
                        if mode_support_explicit {
                            bail!("duplicate --mode-support flag; {AUTH_STATUS_USAGE}");
                        }
                        let Some(raw_mode_support) = tokens.get(index + 1) else {
                            bail!("missing mode-support filter after --mode-support; {AUTH_STATUS_USAGE}");
                        };
                        mode_support = parse_auth_matrix_mode_support_filter(raw_mode_support)?;
                        mode_support_explicit = true;
                        index += 2;
                    }
                    "--availability" => {
                        if availability_explicit {
                            bail!("duplicate --availability flag; {AUTH_STATUS_USAGE}");
                        }
                        let Some(raw_availability) = tokens.get(index + 1) else {
                            bail!("missing availability filter after --availability; {AUTH_STATUS_USAGE}");
                        };
                        availability = parse_auth_matrix_availability_filter(raw_availability)?;
                        availability_explicit = true;
                        index += 2;
                    }
                    "--state" => {
                        if state.is_some() {
                            bail!("duplicate --state flag; {AUTH_STATUS_USAGE}");
                        }
                        let Some(raw_state) = tokens.get(index + 1) else {
                            bail!("missing state filter after --state; {AUTH_STATUS_USAGE}");
                        };
                        state = Some(parse_auth_matrix_state_filter(raw_state)?);
                        index += 2;
                    }
                    "--source-kind" => {
                        if source_kind_explicit {
                            bail!("duplicate --source-kind flag; {AUTH_STATUS_USAGE}");
                        }
                        let Some(raw_source_kind) = tokens.get(index + 1) else {
                            bail!("missing source-kind filter after --source-kind; {AUTH_STATUS_USAGE}");
                        };
                        source_kind = parse_auth_source_kind_filter(raw_source_kind)?;
                        source_kind_explicit = true;
                        index += 2;
                    }
                    "--revoked" => {
                        if revoked_explicit {
                            bail!("duplicate --revoked flag; {AUTH_STATUS_USAGE}");
                        }
                        let Some(raw_revoked) = tokens.get(index + 1) else {
                            bail!("missing revoked filter after --revoked; {AUTH_STATUS_USAGE}");
                        };
                        revoked = parse_auth_revoked_filter(raw_revoked)?;
                        revoked_explicit = true;
                        index += 2;
                    }
                    token if token.starts_with("--") => {
                        bail!("unexpected argument '{}'; {AUTH_STATUS_USAGE}", token);
                    }
                    token => {
                        if provider.is_some() {
                            bail!("unexpected argument '{}'; {AUTH_STATUS_USAGE}", token);
                        }
                        provider = Some(parse_auth_provider(token)?);
                        index += 1;
                    }
                }
            }
            Ok(AuthCommand::Status {
                provider,
                mode,
                mode_support,
                availability,
                state,
                source_kind,
                revoked,
                json_output,
            })
        }
        "logout" => {
            if tokens.len() < 2 {
                bail!("{AUTH_LOGOUT_USAGE}");
            }
            let provider = parse_auth_provider(tokens[1])?;
            let mut json_output = false;
            for token in tokens.into_iter().skip(2) {
                if token == "--json" {
                    json_output = true;
                } else {
                    bail!("unexpected argument '{}'; {AUTH_LOGOUT_USAGE}", token);
                }
            }
            Ok(AuthCommand::Logout {
                provider,
                json_output,
            })
        }
        "matrix" => {
            let mut provider: Option<Provider> = None;
            let mut mode: Option<ProviderAuthMethod> = None;
            let mut mode_support = AuthMatrixModeSupportFilter::All;
            let mut mode_support_explicit = false;
            let mut availability = AuthMatrixAvailabilityFilter::All;
            let mut availability_explicit = false;
            let mut state: Option<String> = None;
            let mut source_kind = AuthSourceKindFilter::All;
            let mut source_kind_explicit = false;
            let mut revoked = AuthRevokedFilter::All;
            let mut revoked_explicit = false;
            let mut json_output = false;
            let mut index = 1usize;
            while index < tokens.len() {
                match tokens[index] {
                    "--json" => {
                        json_output = true;
                        index += 1;
                    }
                    "--mode" => {
                        if mode.is_some() {
                            bail!("duplicate --mode flag; {AUTH_MATRIX_USAGE}");
                        }
                        let Some(raw_mode) = tokens.get(index + 1) else {
                            bail!("missing auth mode after --mode; {AUTH_MATRIX_USAGE}");
                        };
                        mode = Some(parse_provider_auth_method_token(raw_mode)?);
                        index += 2;
                    }
                    "--mode-support" => {
                        if mode_support_explicit {
                            bail!("duplicate --mode-support flag; {AUTH_MATRIX_USAGE}");
                        }
                        let Some(raw_mode_support) = tokens.get(index + 1) else {
                            bail!("missing mode-support filter after --mode-support; {AUTH_MATRIX_USAGE}");
                        };
                        mode_support = parse_auth_matrix_mode_support_filter(raw_mode_support)?;
                        mode_support_explicit = true;
                        index += 2;
                    }
                    "--availability" => {
                        if availability_explicit {
                            bail!("duplicate --availability flag; {AUTH_MATRIX_USAGE}");
                        }
                        let Some(raw_availability) = tokens.get(index + 1) else {
                            bail!("missing availability filter after --availability; {AUTH_MATRIX_USAGE}");
                        };
                        availability = parse_auth_matrix_availability_filter(raw_availability)?;
                        availability_explicit = true;
                        index += 2;
                    }
                    "--state" => {
                        if state.is_some() {
                            bail!("duplicate --state flag; {AUTH_MATRIX_USAGE}");
                        }
                        let Some(raw_state) = tokens.get(index + 1) else {
                            bail!("missing state filter after --state; {AUTH_MATRIX_USAGE}");
                        };
                        state = Some(parse_auth_matrix_state_filter(raw_state)?);
                        index += 2;
                    }
                    "--source-kind" => {
                        if source_kind_explicit {
                            bail!("duplicate --source-kind flag; {AUTH_MATRIX_USAGE}");
                        }
                        let Some(raw_source_kind) = tokens.get(index + 1) else {
                            bail!("missing source-kind filter after --source-kind; {AUTH_MATRIX_USAGE}");
                        };
                        source_kind = parse_auth_source_kind_filter(raw_source_kind)?;
                        source_kind_explicit = true;
                        index += 2;
                    }
                    "--revoked" => {
                        if revoked_explicit {
                            bail!("duplicate --revoked flag; {AUTH_MATRIX_USAGE}");
                        }
                        let Some(raw_revoked) = tokens.get(index + 1) else {
                            bail!("missing revoked filter after --revoked; {AUTH_MATRIX_USAGE}");
                        };
                        revoked = parse_auth_revoked_filter(raw_revoked)?;
                        revoked_explicit = true;
                        index += 2;
                    }
                    token if token.starts_with("--") => {
                        bail!("unexpected argument '{}'; {AUTH_MATRIX_USAGE}", token);
                    }
                    token => {
                        if provider.is_some() {
                            bail!("unexpected argument '{}'; {AUTH_MATRIX_USAGE}", token);
                        }
                        provider = Some(parse_auth_provider(token)?);
                        index += 1;
                    }
                }
            }
            Ok(AuthCommand::Matrix {
                provider,
                mode,
                mode_support,
                availability,
                state,
                source_kind,
                revoked,
                json_output,
            })
        }
        other => bail!("unknown subcommand '{}'; {AUTH_USAGE}", other),
    }
}

/// Public `fn` `parse_auth_matrix_availability_filter` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_matrix_availability_filter(token: &str) -> Result<AuthMatrixAvailabilityFilter> {
    match token.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(AuthMatrixAvailabilityFilter::All),
        "available" => Ok(AuthMatrixAvailabilityFilter::Available),
        "unavailable" => Ok(AuthMatrixAvailabilityFilter::Unavailable),
        other => bail!(
            "unknown availability filter '{}'; supported values: all, available, unavailable",
            other
        ),
    }
}

/// Public `fn` `parse_auth_matrix_mode_support_filter` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_matrix_mode_support_filter(token: &str) -> Result<AuthMatrixModeSupportFilter> {
    match token.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(AuthMatrixModeSupportFilter::All),
        "supported" => Ok(AuthMatrixModeSupportFilter::Supported),
        "unsupported" => Ok(AuthMatrixModeSupportFilter::Unsupported),
        other => bail!(
            "unknown mode-support filter '{}'; supported values: all, supported, unsupported",
            other
        ),
    }
}

/// Public `fn` `parse_auth_matrix_state_filter` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_matrix_state_filter(token: &str) -> Result<String> {
    let normalized = token.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        bail!("state filter must not be empty");
    }
    Ok(normalized)
}

/// Public `fn` `parse_auth_source_kind_filter` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_source_kind_filter(token: &str) -> Result<AuthSourceKindFilter> {
    match token.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(AuthSourceKindFilter::All),
        "flag" => Ok(AuthSourceKindFilter::Flag),
        "env" => Ok(AuthSourceKindFilter::Env),
        "credential-store" | "credential_store" => Ok(AuthSourceKindFilter::CredentialStore),
        "none" => Ok(AuthSourceKindFilter::None),
        other => bail!(
            "unknown source-kind filter '{}'; supported values: all, flag, env, credential-store, none",
            other
        ),
    }
}

/// Public `fn` `parse_auth_revoked_filter` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn parse_auth_revoked_filter(token: &str) -> Result<AuthRevokedFilter> {
    match token.trim().to_ascii_lowercase().as_str() {
        "all" => Ok(AuthRevokedFilter::All),
        "revoked" => Ok(AuthRevokedFilter::Revoked),
        "not-revoked" | "not_revoked" => Ok(AuthRevokedFilter::NotRevoked),
        other => bail!(
            "unknown revoked filter '{}'; supported values: all, revoked, not-revoked",
            other
        ),
    }
}

/// Public `fn` `execute_auth_login_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn execute_auth_login_command(
    config: &AuthCommandConfig,
    provider: Provider,
    mode_override: Option<ProviderAuthMethod>,
    launch: bool,
    json_output: bool,
) -> String {
    let mode = mode_override
        .unwrap_or_else(|| configured_provider_auth_method_from_config(config, provider));
    let mut redaction_secrets = collect_non_empty_secrets(
        provider_api_key_candidates_from_auth_config(config, provider),
    );
    redaction_secrets.extend(collect_non_empty_secrets(
        provider_login_access_token_candidates(provider),
    ));
    redaction_secrets.extend(collect_non_empty_secrets(
        provider_login_refresh_token_candidates(provider),
    ));
    let capability = provider_auth_capability(provider, mode);
    if !capability.supported {
        let supported_modes = provider_supported_auth_modes(provider);
        let supported_mode_list = supported_modes
            .iter()
            .map(|method| method.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let reason = redact_known_secrets(
            format!(
                "auth mode '{}' is not supported for provider '{}': {} (supported_modes={})",
                mode.as_str(),
                provider.as_str(),
                capability.reason,
                supported_mode_list
            ),
            &redaction_secrets,
        );
        if json_output {
            return serde_json::json!({
                "command": "auth.login",
                "provider": provider.as_str(),
                "mode": mode.as_str(),
                "status": "error",
                "reason": reason,
                "supported_modes": supported_modes
                    .iter()
                    .map(|method| method.as_str())
                    .collect::<Vec<_>>(),
            })
            .to_string();
        }
        return format!(
            "auth login error: provider={} mode={} error={reason}",
            provider.as_str(),
            mode.as_str()
        );
    }

    if launch {
        if let Err(error) = build_auth_login_launch_spec(config, provider, mode) {
            let reason = redact_known_secrets(error.to_string(), &redaction_secrets);
            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": provider.as_str(),
                    "mode": mode.as_str(),
                    "status": "error",
                    "reason": reason,
                    "launch_requested": true,
                    "launch_executed": false,
                })
                .to_string();
            }
            return format!(
                "auth login error: provider={} mode={} launch_requested=true launch_executed=false error={reason}",
                provider.as_str(),
                mode.as_str()
            );
        }
    }

    if provider == Provider::Google
        && matches!(
            mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc
        )
    {
        return execute_google_login_backend_ready(config, mode, launch, json_output);
    }

    if provider == Provider::OpenAi
        && matches!(
            mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
        )
    {
        let has_env_access_token =
            resolve_non_empty_secret_with_source(provider_login_access_token_candidates(provider))
                .is_some();
        if launch || !has_env_access_token {
            return execute_openai_login_backend_ready(config, mode, launch, json_output);
        }
    }

    if provider == Provider::Anthropic
        && matches!(
            mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
        )
    {
        return execute_anthropic_login_backend_ready(config, mode, launch, json_output);
    }

    match mode {
        ProviderAuthMethod::ApiKey => {
            match resolve_non_empty_secret_with_source(
                provider_api_key_candidates_from_auth_config(config, provider),
            ) {
                Some((_secret, source)) => {
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "ready",
                            "source": source,
                            "persisted": false,
                        })
                        .to_string();
                    }
                    format!(
                        "auth login: provider={} mode={} status=ready source={} persisted=false",
                        provider.as_str(),
                        mode.as_str(),
                        source
                    )
                }
                None => {
                    let reason = redact_known_secrets(
                        missing_provider_api_key_message(provider).to_string(),
                        &redaction_secrets,
                    );
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "error",
                            "reason": reason,
                        })
                        .to_string();
                    }
                    redact_known_secrets(
                        format!(
                            "auth login error: provider={} mode={} error={reason}",
                            provider.as_str(),
                            mode.as_str()
                        ),
                        &redaction_secrets,
                    )
                }
            }
        }
        ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken => {
            let Some((access_token, access_source)) = resolve_non_empty_secret_with_source(
                provider_login_access_token_candidates(provider),
            ) else {
                let reason = redact_known_secrets(
                    "missing access token for login. Set TAU_AUTH_ACCESS_TOKEN or provider-specific *_ACCESS_TOKEN env var".to_string(),
                    &redaction_secrets,
                );
                if json_output {
                    return serde_json::json!({
                        "command": "auth.login",
                        "provider": provider.as_str(),
                        "mode": mode.as_str(),
                        "status": "error",
                        "reason": reason,
                    })
                    .to_string();
                }
                return redact_known_secrets(
                    format!(
                        "auth login error: provider={} mode={} error={reason}",
                        provider.as_str(),
                        mode.as_str()
                    ),
                    &redaction_secrets,
                );
            };

            let refresh_token = resolve_non_empty_secret_with_source(
                provider_login_refresh_token_candidates(provider),
            )
            .map(|(secret, _source)| secret);
            let expires_unix = match resolve_auth_login_expires_unix(provider)
                .map(|value| value.unwrap_or_else(|| current_unix_timestamp().saturating_add(3600)))
            {
                Ok(value) => value,
                Err(error) => {
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "error",
                            "reason": redact_known_secrets(error.to_string(), &redaction_secrets),
                        })
                        .to_string();
                    }
                    return redact_known_secrets(
                        format!(
                            "auth login error: provider={} mode={} error={error}",
                            provider.as_str(),
                            mode.as_str()
                        ),
                        &redaction_secrets,
                    );
                }
            };

            let mut store = match load_credential_store(
                &config.credential_store,
                config.credential_store_encryption,
                config.credential_store_key.as_deref(),
            ) {
                Ok(store) => store,
                Err(error) => {
                    if json_output {
                        return serde_json::json!({
                            "command": "auth.login",
                            "provider": provider.as_str(),
                            "mode": mode.as_str(),
                            "status": "error",
                            "reason": redact_known_secrets(error.to_string(), &redaction_secrets),
                        })
                        .to_string();
                    }
                    return redact_known_secrets(
                        format!(
                            "auth login error: provider={} mode={} error={error}",
                            provider.as_str(),
                            mode.as_str()
                        ),
                        &redaction_secrets,
                    );
                }
            };
            store.providers.insert(
                provider.as_str().to_string(),
                ProviderCredentialStoreRecord {
                    auth_method: mode,
                    access_token: Some(access_token),
                    refresh_token,
                    expires_unix: Some(expires_unix),
                    revoked: false,
                },
            );
            if let Err(error) = save_credential_store(
                &config.credential_store,
                &store,
                config.credential_store_key.as_deref(),
            ) {
                if json_output {
                    return serde_json::json!({
                        "command": "auth.login",
                        "provider": provider.as_str(),
                        "mode": mode.as_str(),
                        "status": "error",
                        "reason": redact_known_secrets(error.to_string(), &redaction_secrets),
                    })
                    .to_string();
                }
                return redact_known_secrets(
                    format!(
                        "auth login error: provider={} mode={} error={error}",
                        provider.as_str(),
                        mode.as_str()
                    ),
                    &redaction_secrets,
                );
            }

            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": provider.as_str(),
                    "mode": mode.as_str(),
                    "status": "saved",
                    "source": access_source,
                    "credential_store": config.credential_store.display().to_string(),
                    "expires_unix": expires_unix,
                })
                .to_string();
            }
            format!(
                "auth login: provider={} mode={} status=saved source={} credential_store={} expires_unix={}",
                provider.as_str(),
                mode.as_str(),
                access_source,
                config.credential_store.display(),
                expires_unix
            )
        }
        ProviderAuthMethod::Adc => {
            let reason = redact_known_secrets(
                "adc login flow is not implemented".to_string(),
                &redaction_secrets,
            );
            if json_output {
                return serde_json::json!({
                    "command": "auth.login",
                    "provider": provider.as_str(),
                    "mode": mode.as_str(),
                    "status": "error",
                    "reason": reason,
                })
                .to_string();
            }
            format!(
                "auth login error: provider={} mode={} error={reason}",
                provider.as_str(),
                mode.as_str()
            )
        }
    }
}

fn auth_reauth_status_label(row: &AuthStatusRow) -> &'static str {
    if row.available {
        "ready"
    } else if row.reauth_required {
        "reauth_required"
    } else {
        "recovery_required"
    }
}

/// Public `fn` `execute_auth_reauth_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn execute_auth_reauth_command(
    config: &AuthCommandConfig,
    provider: Provider,
    mode_override: Option<ProviderAuthMethod>,
    launch: bool,
    json_output: bool,
) -> String {
    let mode = mode_override
        .unwrap_or_else(|| configured_provider_auth_method_from_config(config, provider));
    let mode_config = auth_config_with_provider_mode(config, provider, mode);
    let requires_store = mode != ProviderAuthMethod::ApiKey;
    let (store, store_error) = if requires_store {
        match load_credential_store(
            &config.credential_store,
            config.credential_store_encryption,
            config.credential_store_key.as_deref(),
        ) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };
    let row = auth_status_row_for_provider(
        &mode_config,
        provider,
        store.as_ref(),
        store_error.as_deref(),
    );
    let status_label = auth_reauth_status_label(&row);
    let reauth_command = format_auth_reauth_command(provider, mode);
    let launch_supported = build_auth_login_launch_spec(config, provider, mode).is_ok();

    if launch {
        let login_result = execute_auth_login_command(config, provider, Some(mode), true, true);
        let parsed_login_result: serde_json::Value = serde_json::from_str(&login_result)
            .unwrap_or_else(|_| serde_json::json!({ "raw_result": login_result }));
        let login_status = parsed_login_result
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let launch_executed = parsed_login_result
            .get("launch_executed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if json_output {
            return serde_json::json!({
                "command": "auth.reauth",
                "provider": provider.as_str(),
                "mode": mode.as_str(),
                "status": status_label,
                "launch_requested": true,
                "launch_executed": launch_executed,
                "launch_supported": launch_supported,
                "reauth_command": reauth_command,
                "fallback_order": row.fallback_order,
                "fallback_mode": row.fallback_mode,
                "fallback_available": row.fallback_available,
                "fallback_reason_code": row.fallback_reason_code,
                "fallback_hint": row.fallback_hint,
                "reauth_prerequisites": row.reauth_prerequisites,
                "next_action": row.next_action,
                "entry": row,
                "login": parsed_login_result,
            })
            .to_string();
        }
        return format!(
            "auth reauth: provider={} mode={} status={} launch_requested=true launch_executed={} launch_supported={} login_status={} reauth_command={} fallback_mode={} fallback_available={} fallback_hint={} next_action={}",
            provider.as_str(),
            mode.as_str(),
            status_label,
            launch_executed,
            launch_supported,
            login_status,
            reauth_command,
            row.fallback_mode,
            row.fallback_available,
            row.fallback_hint,
            row.next_action
        );
    }

    if json_output {
        return serde_json::json!({
            "command": "auth.reauth",
            "provider": provider.as_str(),
            "mode": mode.as_str(),
            "status": status_label,
            "launch_requested": false,
            "launch_executed": false,
            "launch_supported": launch_supported,
            "reauth_command": reauth_command,
            "fallback_order": row.fallback_order,
            "fallback_mode": row.fallback_mode,
            "fallback_available": row.fallback_available,
            "fallback_reason_code": row.fallback_reason_code,
            "fallback_hint": row.fallback_hint,
            "reauth_prerequisites": row.reauth_prerequisites,
            "next_action": row.next_action,
            "entry": row,
        })
        .to_string();
    }

    format!(
        "auth reauth: provider={} mode={} status={} launch_requested=false launch_executed=false launch_supported={} reauth_command={} fallback_order={} fallback_mode={} fallback_available={} fallback_hint={} reauth_prerequisites={} next_action={}",
        provider.as_str(),
        mode.as_str(),
        status_label,
        launch_supported,
        reauth_command,
        row.fallback_order,
        row.fallback_mode,
        row.fallback_available,
        row.fallback_hint,
        row.reauth_prerequisites,
        row.next_action
    )
}

/// Public `fn` `auth_status_row_for_provider` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_status_row_for_provider(
    config: &AuthCommandConfig,
    provider: Provider,
    store: Option<&CredentialStoreData>,
    store_error: Option<&str>,
) -> AuthStatusRow {
    let snapshot = provider_auth_snapshot_for_status(config, provider, store, store_error);
    let backend_probe = auth_backend_probe(config, provider, snapshot.method);
    let fallback_plan = auth_fallback_plan(config, provider, snapshot.method, store, store_error);
    auth_status_row_from_snapshot(&snapshot, &backend_probe, &fallback_plan)
}

fn auth_status_row_from_snapshot(
    snapshot: &ProviderAuthSnapshot,
    backend_probe: &AuthBackendProbe,
    fallback_plan: &AuthFallbackPlan,
) -> AuthStatusRow {
    let now_unix = current_unix_timestamp();
    let expires_in_seconds = snapshot
        .expires_unix
        .map(|expires_unix| expires_unix as i64 - now_unix as i64);
    let expiry_state = auth_expiry_state(snapshot, now_unix);
    let reauth_required = auth_reauth_required(snapshot);
    let reauth_hint = if reauth_required {
        format_auth_reauth_command(snapshot.provider, snapshot.method)
    } else {
        "none".to_string()
    };
    AuthStatusRow {
        provider: snapshot.provider.as_str().to_string(),
        mode: snapshot.method.as_str().to_string(),
        mode_supported: snapshot.mode_supported,
        supported: snapshot.mode_supported,
        available: snapshot.available,
        state: snapshot.state.clone(),
        source: snapshot.source.clone(),
        reason_code: auth_reason_code(snapshot),
        reason: snapshot.reason.clone(),
        expires_unix: snapshot.expires_unix,
        expires: snapshot.expires_unix,
        expires_in_seconds,
        expiry_state,
        revoked: snapshot.revoked,
        refreshable: snapshot.refreshable,
        backend_required: backend_probe.required,
        backend: backend_probe.backend.clone(),
        backend_health: backend_probe.health.clone(),
        backend_reason_code: backend_probe.reason_code.clone(),
        fallback_order: fallback_plan.order.clone(),
        fallback_mode: fallback_plan.mode.clone(),
        fallback_available: fallback_plan.available,
        fallback_reason_code: fallback_plan.reason_code.clone(),
        fallback_hint: fallback_plan.hint.clone(),
        reauth_prerequisites: fallback_plan.reauth_prerequisites.clone(),
        reauth_required,
        reauth_hint,
        next_action: auth_next_action(snapshot),
    }
}

fn auth_backend_probe(
    config: &AuthCommandConfig,
    provider: Provider,
    method: ProviderAuthMethod,
) -> AuthBackendProbe {
    match (provider, method) {
        (Provider::OpenAi, ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken) => {
            auth_backend_probe_from_cli_state(
                "codex_cli",
                config.openai_codex_backend,
                &config.openai_codex_cli,
            )
        }
        (
            Provider::Anthropic,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken,
        ) => auth_backend_probe_from_cli_state(
            "claude_cli",
            config.anthropic_claude_backend,
            &config.anthropic_claude_cli,
        ),
        (Provider::Google, ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc) => {
            auth_backend_probe_from_cli_state(
                "gemini_cli",
                config.google_gemini_backend,
                &config.google_gemini_cli,
            )
        }
        _ => AuthBackendProbe {
            required: false,
            backend: "none".to_string(),
            health: "not_required".to_string(),
            reason_code: "backend_not_required".to_string(),
        },
    }
}

fn auth_backend_probe_from_cli_state(
    backend: &str,
    enabled: bool,
    executable: &str,
) -> AuthBackendProbe {
    if !enabled {
        return AuthBackendProbe {
            required: true,
            backend: backend.to_string(),
            health: "disabled".to_string(),
            reason_code: "backend_disabled".to_string(),
        };
    }
    if !is_executable_available(executable) {
        return AuthBackendProbe {
            required: true,
            backend: backend.to_string(),
            health: "unavailable".to_string(),
            reason_code: "backend_unavailable".to_string(),
        };
    }
    AuthBackendProbe {
        required: true,
        backend: backend.to_string(),
        health: "ready".to_string(),
        reason_code: "backend_ready".to_string(),
    }
}

fn provider_auth_recovery_order(provider: Provider) -> Vec<ProviderAuthMethod> {
    match provider {
        Provider::OpenAi | Provider::Anthropic => vec![
            ProviderAuthMethod::OauthToken,
            ProviderAuthMethod::SessionToken,
            ProviderAuthMethod::ApiKey,
        ],
        Provider::Google => vec![
            ProviderAuthMethod::OauthToken,
            ProviderAuthMethod::Adc,
            ProviderAuthMethod::ApiKey,
        ],
    }
}

fn format_auth_reauth_command(provider: Provider, method: ProviderAuthMethod) -> String {
    format!(
        "run /auth reauth {} --mode {}",
        provider.as_str(),
        method.as_str()
    )
}

fn provider_reauth_prerequisites(provider: Provider, method: ProviderAuthMethod) -> String {
    match (provider, method) {
        (Provider::OpenAi, ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken) => {
            "codex cli installed; run codex --login; --openai-codex-backend=true".to_string()
        }
        (
            Provider::Anthropic,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken,
        ) => "claude cli installed; run claude then /login; --anthropic-claude-backend=true"
            .to_string(),
        (Provider::Google, ProviderAuthMethod::OauthToken) => {
            "gemini cli installed; login with Google in gemini; --google-gemini-backend=true"
                .to_string()
        }
        (Provider::Google, ProviderAuthMethod::Adc) => "gcloud installed; run gcloud auth application-default login; set GOOGLE_CLOUD_PROJECT and GOOGLE_CLOUD_LOCATION".to_string(),
        (Provider::Google, ProviderAuthMethod::SessionToken) => {
            "google session-token mode is unsupported; prefer oauth-token, adc, or api-key"
                .to_string()
        }
        (_, ProviderAuthMethod::ApiKey) => missing_provider_api_key_message(provider).to_string(),
        (_, ProviderAuthMethod::Adc) => {
            "adc flow requires cloud credential bootstrap in the active environment".to_string()
        }
    }
}

fn auth_fallback_plan(
    config: &AuthCommandConfig,
    provider: Provider,
    method: ProviderAuthMethod,
    store: Option<&CredentialStoreData>,
    store_error: Option<&str>,
) -> AuthFallbackPlan {
    let fallback_order_modes = provider_auth_recovery_order(provider);
    let fallback_order = fallback_order_modes
        .iter()
        .map(|candidate| candidate.as_str())
        .collect::<Vec<_>>()
        .join(">");
    let reauth_prerequisites = provider_reauth_prerequisites(provider, method);
    let fallback_candidate = fallback_order_modes.iter().copied().find(|candidate| {
        *candidate != method && provider_auth_capability(provider, *candidate).supported
    });

    let Some(fallback_mode) = fallback_candidate else {
        return AuthFallbackPlan {
            order: fallback_order,
            mode: "none".to_string(),
            available: false,
            reason_code: "no_supported_fallback".to_string(),
            hint: "none".to_string(),
            reauth_prerequisites,
        };
    };

    let fallback_config = auth_config_with_provider_mode(config, provider, fallback_mode);
    let fallback_snapshot =
        provider_auth_snapshot_for_status(&fallback_config, provider, store, store_error);
    let fallback_flag = provider_auth_mode_flag(provider);
    let fallback_command = format!("set {} {}", fallback_flag, fallback_mode.as_str());
    let fallback_ready = fallback_snapshot.available;
    let fallback_hint = if fallback_ready {
        format!(
            "{}; then {}",
            fallback_command,
            format_auth_reauth_command(provider, fallback_mode)
        )
    } else {
        format!(
            "{}; then {}",
            fallback_command,
            auth_next_action(&fallback_snapshot)
        )
    };

    AuthFallbackPlan {
        order: fallback_order,
        mode: fallback_mode.as_str().to_string(),
        available: fallback_ready,
        reason_code: if fallback_ready {
            "fallback_ready".to_string()
        } else {
            "fallback_unavailable".to_string()
        },
        hint: fallback_hint,
        reauth_prerequisites,
    }
}

fn auth_reason_code(snapshot: &ProviderAuthSnapshot) -> String {
    snapshot.state.clone()
}

fn auth_expiry_state(snapshot: &ProviderAuthSnapshot, now_unix: u64) -> String {
    let Some(expires_unix) = snapshot.expires_unix else {
        return if snapshot.method == ProviderAuthMethod::ApiKey
            || snapshot.method == ProviderAuthMethod::Adc
        {
            "not_applicable".to_string()
        } else {
            "unknown".to_string()
        };
    };
    if expires_unix <= now_unix {
        return "expired".to_string();
    }
    let remaining = expires_unix.saturating_sub(now_unix);
    if remaining <= 3_600 {
        "expiring_soon".to_string()
    } else {
        "valid".to_string()
    }
}

fn auth_reauth_required(snapshot: &ProviderAuthSnapshot) -> bool {
    matches!(
        snapshot.state.as_str(),
        "missing_credential"
            | "missing_access_token"
            | "invalid_env_expires"
            | "expired_env_access_token"
            | "expired"
            | "expired_refresh_pending"
            | "revoked"
            | "mode_mismatch"
    )
}

fn auth_next_action(snapshot: &ProviderAuthSnapshot) -> String {
    if snapshot.available {
        return "none".to_string();
    }

    match snapshot.state.as_str() {
        "missing_api_key" => missing_provider_api_key_message(snapshot.provider).to_string(),
        "backend_disabled" => match snapshot.provider {
            Provider::OpenAi => "set --openai-codex-backend=true".to_string(),
            Provider::Anthropic => "set --anthropic-claude-backend=true".to_string(),
            Provider::Google => "set --google-gemini-backend=true".to_string(),
        },
        "backend_unavailable" => match snapshot.provider {
            Provider::OpenAi => {
                "install codex or set --openai-codex-cli to an available executable".to_string()
            }
            Provider::Anthropic => {
                "install claude or set --anthropic-claude-cli to an available executable"
                    .to_string()
            }
            Provider::Google => {
                "install gemini or set --google-gemini-cli to an available executable".to_string()
            }
        },
        "unsupported_mode" => format!("set {} api-key", provider_auth_mode_flag(snapshot.provider)),
        "missing_credential"
        | "missing_access_token"
        | "invalid_env_expires"
        | "expired_env_access_token"
        | "expired"
        | "expired_refresh_pending"
        | "revoked"
        | "mode_mismatch" => format_auth_reauth_command(snapshot.provider, snapshot.method),
        "store_error" => {
            "fix credential store accessibility and retry /auth status or /auth reauth".to_string()
        }
        _ => "inspect /auth status reason field".to_string(),
    }
}

/// Public `fn` `auth_state_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_state_counts(rows: &[AuthStatusRow]) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.state.clone()).or_insert(0) += 1;
    }
    counts
}

/// Public `fn` `auth_mode_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_mode_counts(rows: &[AuthStatusRow]) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.mode.clone()).or_insert(0) += 1;
    }
    counts
}

/// Public `fn` `auth_provider_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_provider_counts(rows: &[AuthStatusRow]) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.provider.clone()).or_insert(0) += 1;
    }
    counts
}

/// Public `fn` `auth_availability_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_availability_counts(
    rows: &[AuthStatusRow],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        let key = if row.available {
            "available"
        } else {
            "unavailable"
        };
        *counts.entry(key.to_string()).or_insert(0) += 1;
    }
    counts
}

/// Public `fn` `auth_source_kind` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_source_kind(source: &str) -> &'static str {
    let normalized = source.trim();
    if normalized == "credential_store" {
        return "credential_store";
    }
    if normalized == "none" {
        return "none";
    }
    if normalized.starts_with("--") {
        return "flag";
    }
    "env"
}

fn auth_source_kind_matches_filter(source: &str, source_kind_filter: AuthSourceKindFilter) -> bool {
    match source_kind_filter {
        AuthSourceKindFilter::All => true,
        AuthSourceKindFilter::Flag => auth_source_kind(source) == "flag",
        AuthSourceKindFilter::Env => auth_source_kind(source) == "env",
        AuthSourceKindFilter::CredentialStore => auth_source_kind(source) == "credential_store",
        AuthSourceKindFilter::None => auth_source_kind(source) == "none",
    }
}

/// Public `fn` `auth_source_kind_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_source_kind_counts(
    rows: &[AuthStatusRow],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts
            .entry(auth_source_kind(&row.source).to_string())
            .or_insert(0) += 1;
    }
    counts
}

/// Public `fn` `auth_revoked_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn auth_revoked_counts(rows: &[AuthStatusRow]) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        let key = if row.revoked {
            "revoked"
        } else {
            "not_revoked"
        };
        *counts.entry(key.to_string()).or_insert(0) += 1;
    }
    counts
}

/// Public `fn` `format_auth_state_counts` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn format_auth_state_counts(
    state_counts: &std::collections::BTreeMap<String, usize>,
) -> String {
    if state_counts.is_empty() {
        return "none".to_string();
    }
    state_counts
        .iter()
        .map(|(state, count)| format!("{state}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Public `fn` `execute_auth_status_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn execute_auth_status_command(
    config: &AuthCommandConfig,
    provider: Option<Provider>,
    filters: AuthQueryFilters,
    json_output: bool,
) -> String {
    let AuthQueryFilters {
        mode,
        mode_support,
        availability,
        state,
        source_kind,
        revoked,
    } = filters;
    let provider_filter = provider.map(|value| value.as_str()).unwrap_or("all");
    let selected_providers = if let Some(provider) = provider {
        vec![provider]
    } else {
        vec![Provider::OpenAi, Provider::Anthropic, Provider::Google]
    };

    let mode_filter = mode.map(|value| value.as_str()).unwrap_or("all");
    let requires_store = selected_providers.iter().any(|provider| {
        mode.unwrap_or_else(|| configured_provider_auth_method_from_config(config, *provider))
            != ProviderAuthMethod::ApiKey
    });
    let (store, store_error) = if requires_store {
        match load_credential_store(
            &config.credential_store,
            config.credential_store_encryption,
            config.credential_store_key.as_deref(),
        ) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };

    let mut rows = selected_providers
        .iter()
        .map(|provider| {
            let mode_for_provider = mode
                .unwrap_or_else(|| configured_provider_auth_method_from_config(config, *provider));
            let mode_config = auth_config_with_provider_mode(config, *provider, mode_for_provider);
            auth_status_row_for_provider(
                &mode_config,
                *provider,
                store.as_ref(),
                store_error.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    let total_rows = rows.len();
    let mode_supported_total = rows.iter().filter(|row| row.mode_supported).count();
    let mode_unsupported_total = total_rows.saturating_sub(mode_supported_total);
    let mode_counts_total = auth_mode_counts(&rows);
    let provider_counts_total = auth_provider_counts(&rows);
    let availability_counts_total = auth_availability_counts(&rows);
    let state_counts_total = auth_state_counts(&rows);
    let source_kind_counts_total = auth_source_kind_counts(&rows);
    let revoked_counts_total = auth_revoked_counts(&rows);
    rows = match mode_support {
        AuthMatrixModeSupportFilter::All => rows,
        AuthMatrixModeSupportFilter::Supported => {
            rows.into_iter().filter(|row| row.mode_supported).collect()
        }
        AuthMatrixModeSupportFilter::Unsupported => {
            rows.into_iter().filter(|row| !row.mode_supported).collect()
        }
    };
    rows = match availability {
        AuthMatrixAvailabilityFilter::All => rows,
        AuthMatrixAvailabilityFilter::Available => {
            rows.into_iter().filter(|row| row.available).collect()
        }
        AuthMatrixAvailabilityFilter::Unavailable => {
            rows.into_iter().filter(|row| !row.available).collect()
        }
    };
    let state_filter = state
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    if let Some(state_filter) = state_filter.as_deref() {
        rows.retain(|row| row.state == state_filter);
    }
    rows.retain(|row| auth_source_kind_matches_filter(&row.source, source_kind));
    rows = match revoked {
        AuthRevokedFilter::All => rows,
        AuthRevokedFilter::Revoked => rows.into_iter().filter(|row| row.revoked).collect(),
        AuthRevokedFilter::NotRevoked => rows.into_iter().filter(|row| !row.revoked).collect(),
    };

    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);
    let mode_supported = rows.iter().filter(|row| row.mode_supported).count();
    let mode_unsupported = rows.len().saturating_sub(mode_supported);
    let mode_counts = auth_mode_counts(&rows);
    let provider_counts = auth_provider_counts(&rows);
    let availability_counts = auth_availability_counts(&rows);
    let state_counts = auth_state_counts(&rows);
    let source_kind_counts = auth_source_kind_counts(&rows);
    let revoked_counts = auth_revoked_counts(&rows);

    if json_output {
        return serde_json::json!({
            "command": "auth.status",
            "provider_filter": provider_filter,
            "mode_filter": mode_filter,
            "mode_support_filter": mode_support.as_str(),
            "availability_filter": availability.as_str(),
            "state_filter": state_filter.as_deref().unwrap_or("all"),
            "source_kind_filter": source_kind.as_str(),
            "revoked_filter": revoked.as_str(),
            "subscription_strict": config.provider_subscription_strict,
            "providers": selected_providers.len(),
            "rows_total": total_rows,
            "rows": rows.len(),
            "mode_supported": mode_supported,
            "mode_unsupported": mode_unsupported,
            "mode_supported_total": mode_supported_total,
            "mode_unsupported_total": mode_unsupported_total,
            "mode_counts_total": mode_counts_total,
            "mode_counts": mode_counts,
            "provider_counts_total": provider_counts_total,
            "provider_counts": provider_counts,
            "available": available,
            "unavailable": unavailable,
            "availability_counts_total": availability_counts_total,
            "availability_counts": availability_counts,
            "state_counts_total": state_counts_total,
            "state_counts": state_counts,
            "source_kind_counts_total": source_kind_counts_total,
            "source_kind_counts": source_kind_counts,
            "revoked_counts_total": revoked_counts_total,
            "revoked_counts": revoked_counts,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "auth status: providers={} rows={} mode_supported={} mode_unsupported={} available={} unavailable={} provider_filter={} mode_filter={} mode_support_filter={} availability_filter={} state_filter={} source_kind_filter={} revoked_filter={} subscription_strict={} rows_total={} mode_supported_total={} mode_unsupported_total={} mode_counts={} mode_counts_total={} provider_counts={} provider_counts_total={} availability_counts={} availability_counts_total={} state_counts={} state_counts_total={} source_kind_counts={} source_kind_counts_total={} revoked_counts={} revoked_counts_total={}",
        selected_providers.len(),
        rows.len(),
        mode_supported,
        mode_unsupported,
        available,
        unavailable,
        provider_filter,
        mode_filter,
        mode_support.as_str(),
        availability.as_str(),
        state_filter.as_deref().unwrap_or("all"),
        source_kind.as_str(),
        revoked.as_str(),
        config.provider_subscription_strict,
        total_rows,
        mode_supported_total,
        mode_unsupported_total,
        format_auth_state_counts(&mode_counts),
        format_auth_state_counts(&mode_counts_total),
        format_auth_state_counts(&provider_counts),
        format_auth_state_counts(&provider_counts_total),
        format_auth_state_counts(&availability_counts),
        format_auth_state_counts(&availability_counts_total),
        format_auth_state_counts(&state_counts),
        format_auth_state_counts(&state_counts_total),
        format_auth_state_counts(&source_kind_counts),
        format_auth_state_counts(&source_kind_counts_total),
        format_auth_state_counts(&revoked_counts),
        format_auth_state_counts(&revoked_counts_total)
    )];
    for row in rows {
        lines.push(format!(
            "auth provider: name={} mode={} mode_supported={} supported={} available={} state={} source={} reason_code={} reason={} expires_unix={} expires={} expires_in_seconds={} expiry_state={} revoked={} refreshable={} backend_required={} backend={} backend_health={} backend_reason_code={} fallback_order={} fallback_mode={} fallback_available={} fallback_reason_code={} fallback_hint={} reauth_prerequisites={} reauth_required={} reauth_hint={} next_action={}",
            row.provider,
            row.mode,
            row.mode_supported,
            row.supported,
            row.available,
            row.state,
            row.source,
            row.reason_code,
            row.reason,
            row.expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.expires
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.expires_in_seconds
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.expiry_state,
            row.revoked,
            row.refreshable,
            row.backend_required,
            row.backend,
            row.backend_health,
            row.backend_reason_code,
            row.fallback_order,
            row.fallback_mode,
            row.fallback_available,
            row.fallback_reason_code,
            row.fallback_hint,
            row.reauth_prerequisites,
            row.reauth_required,
            row.reauth_hint,
            row.next_action
        ));
    }
    lines.join("\n")
}

fn auth_config_with_provider_mode(
    config: &AuthCommandConfig,
    provider: Provider,
    mode: ProviderAuthMethod,
) -> AuthCommandConfig {
    let mut overridden = config.clone();
    match provider {
        Provider::OpenAi => overridden.openai_auth_mode = mode,
        Provider::Anthropic => overridden.anthropic_auth_mode = mode,
        Provider::Google => overridden.google_auth_mode = mode,
    }
    overridden
}

/// Public `fn` `execute_auth_matrix_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn execute_auth_matrix_command(
    config: &AuthCommandConfig,
    provider: Option<Provider>,
    filters: AuthQueryFilters,
    json_output: bool,
) -> String {
    let AuthQueryFilters {
        mode,
        mode_support,
        availability,
        state,
        source_kind,
        revoked,
    } = filters;
    let provider_filter = provider.map(|value| value.as_str()).unwrap_or("all");
    let mode_filter = mode.map(|value| value.as_str()).unwrap_or("all");
    let selected_providers = provider
        .map(|value| vec![value])
        .unwrap_or_else(|| AUTH_MATRIX_PROVIDERS.to_vec());
    let selected_modes = mode
        .map(|value| vec![value])
        .unwrap_or_else(|| AUTH_MATRIX_MODES.to_vec());

    let requires_store = selected_modes
        .iter()
        .any(|mode| *mode != ProviderAuthMethod::ApiKey);
    let (store, store_error) = if requires_store {
        match load_credential_store(
            &config.credential_store,
            config.credential_store_encryption,
            config.credential_store_key.as_deref(),
        ) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        }
    } else {
        (None, None)
    };

    let mut rows = Vec::new();
    for provider in &selected_providers {
        for mode in &selected_modes {
            let mode_config = auth_config_with_provider_mode(config, *provider, *mode);
            rows.push(auth_status_row_for_provider(
                &mode_config,
                *provider,
                store.as_ref(),
                store_error.as_deref(),
            ));
        }
    }

    let total_rows = rows.len();
    let mode_supported_total = rows.iter().filter(|row| row.mode_supported).count();
    let mode_unsupported_total = total_rows.saturating_sub(mode_supported_total);
    let mode_counts_total = auth_mode_counts(&rows);
    let provider_counts_total = auth_provider_counts(&rows);
    let availability_counts_total = auth_availability_counts(&rows);
    let state_counts_total = auth_state_counts(&rows);
    let source_kind_counts_total = auth_source_kind_counts(&rows);
    let revoked_counts_total = auth_revoked_counts(&rows);
    rows = match mode_support {
        AuthMatrixModeSupportFilter::All => rows,
        AuthMatrixModeSupportFilter::Supported => {
            rows.into_iter().filter(|row| row.mode_supported).collect()
        }
        AuthMatrixModeSupportFilter::Unsupported => {
            rows.into_iter().filter(|row| !row.mode_supported).collect()
        }
    };
    rows = match availability {
        AuthMatrixAvailabilityFilter::All => rows,
        AuthMatrixAvailabilityFilter::Available => {
            rows.into_iter().filter(|row| row.available).collect()
        }
        AuthMatrixAvailabilityFilter::Unavailable => {
            rows.into_iter().filter(|row| !row.available).collect()
        }
    };
    let state_filter = state
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    if let Some(state_filter) = state_filter.as_deref() {
        rows.retain(|row| row.state == state_filter);
    }
    rows.retain(|row| auth_source_kind_matches_filter(&row.source, source_kind));
    rows = match revoked {
        AuthRevokedFilter::All => rows,
        AuthRevokedFilter::Revoked => rows.into_iter().filter(|row| row.revoked).collect(),
        AuthRevokedFilter::NotRevoked => rows.into_iter().filter(|row| !row.revoked).collect(),
    };

    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);
    let mode_supported = rows.iter().filter(|row| row.mode_supported).count();
    let mode_unsupported = rows.len().saturating_sub(mode_supported);
    let mode_counts = auth_mode_counts(&rows);
    let provider_counts = auth_provider_counts(&rows);
    let availability_counts = auth_availability_counts(&rows);
    let state_counts = auth_state_counts(&rows);
    let source_kind_counts = auth_source_kind_counts(&rows);
    let revoked_counts = auth_revoked_counts(&rows);

    if json_output {
        return serde_json::json!({
            "command": "auth.matrix",
            "provider_filter": provider_filter,
            "mode_filter": mode_filter,
            "mode_support_filter": mode_support.as_str(),
            "availability_filter": availability.as_str(),
            "state_filter": state_filter.as_deref().unwrap_or("all"),
            "source_kind_filter": source_kind.as_str(),
            "revoked_filter": revoked.as_str(),
            "subscription_strict": config.provider_subscription_strict,
            "providers": selected_providers.len(),
            "modes": selected_modes.len(),
            "rows_total": total_rows,
            "rows": rows.len(),
            "mode_supported": mode_supported,
            "mode_unsupported": mode_unsupported,
            "mode_supported_total": mode_supported_total,
            "mode_unsupported_total": mode_unsupported_total,
            "mode_counts_total": mode_counts_total,
            "mode_counts": mode_counts,
            "provider_counts_total": provider_counts_total,
            "provider_counts": provider_counts,
            "available": available,
            "unavailable": unavailable,
            "availability_counts_total": availability_counts_total,
            "availability_counts": availability_counts,
            "state_counts_total": state_counts_total,
            "state_counts": state_counts,
            "source_kind_counts_total": source_kind_counts_total,
            "source_kind_counts": source_kind_counts,
            "revoked_counts_total": revoked_counts_total,
            "revoked_counts": revoked_counts,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "auth matrix: providers={} modes={} rows={} mode_supported={} mode_unsupported={} available={} unavailable={} provider_filter={} mode_filter={} mode_support_filter={} availability_filter={} state_filter={} source_kind_filter={} revoked_filter={} subscription_strict={} rows_total={} mode_supported_total={} mode_unsupported_total={} mode_counts={} mode_counts_total={} provider_counts={} provider_counts_total={} availability_counts={} availability_counts_total={} state_counts={} state_counts_total={} source_kind_counts={} source_kind_counts_total={} revoked_counts={} revoked_counts_total={}",
        selected_providers.len(),
        selected_modes.len(),
        rows.len(),
        mode_supported,
        mode_unsupported,
        available,
        unavailable,
        provider_filter,
        mode_filter,
        mode_support.as_str(),
        availability.as_str(),
        state_filter.as_deref().unwrap_or("all"),
        source_kind.as_str(),
        revoked.as_str(),
        config.provider_subscription_strict,
        total_rows,
        mode_supported_total,
        mode_unsupported_total,
        format_auth_state_counts(&mode_counts),
        format_auth_state_counts(&mode_counts_total),
        format_auth_state_counts(&provider_counts),
        format_auth_state_counts(&provider_counts_total),
        format_auth_state_counts(&availability_counts),
        format_auth_state_counts(&availability_counts_total),
        format_auth_state_counts(&state_counts),
        format_auth_state_counts(&state_counts_total),
        format_auth_state_counts(&source_kind_counts),
        format_auth_state_counts(&source_kind_counts_total),
        format_auth_state_counts(&revoked_counts),
        format_auth_state_counts(&revoked_counts_total)
    )];
    for row in rows {
        lines.push(format!(
            "auth matrix row: provider={} mode={} mode_supported={} available={} state={} source={} reason_code={} reason={} expires_unix={} expires_in_seconds={} expiry_state={} backend_required={} backend={} backend_health={} backend_reason_code={} fallback_order={} fallback_mode={} fallback_available={} fallback_reason_code={} fallback_hint={} reauth_prerequisites={} reauth_required={} revoked={}",
            row.provider,
            row.mode,
            row.mode_supported,
            row.available,
            row.state,
            row.source,
            row.reason_code,
            row.reason,
            row.expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.expires_in_seconds
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.expiry_state,
            row.backend_required,
            row.backend,
            row.backend_health,
            row.backend_reason_code,
            row.fallback_order,
            row.fallback_mode,
            row.fallback_available,
            row.fallback_reason_code,
            row.fallback_hint,
            row.reauth_prerequisites,
            row.reauth_required,
            row.revoked
        ));
    }
    lines.join("\n")
}

/// Public `fn` `execute_auth_logout_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn execute_auth_logout_command(
    config: &AuthCommandConfig,
    provider: Provider,
    json_output: bool,
) -> String {
    let mut store = match load_credential_store(
        &config.credential_store,
        config.credential_store_encryption,
        config.credential_store_key.as_deref(),
    ) {
        Ok(store) => store,
        Err(error) => {
            if json_output {
                return serde_json::json!({
                    "command": "auth.logout",
                    "provider": provider.as_str(),
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return format!(
                "auth logout error: provider={} error={error}",
                provider.as_str()
            );
        }
    };

    let status = if let Some(entry) = store.providers.get_mut(provider.as_str()) {
        entry.revoked = true;
        entry.access_token = None;
        entry.refresh_token = None;
        entry.expires_unix = None;
        "revoked"
    } else {
        "not_found"
    };

    if status == "revoked" {
        if let Err(error) = save_credential_store(
            &config.credential_store,
            &store,
            config.credential_store_key.as_deref(),
        ) {
            if json_output {
                return serde_json::json!({
                    "command": "auth.logout",
                    "provider": provider.as_str(),
                    "status": "error",
                    "reason": error.to_string(),
                })
                .to_string();
            }
            return format!(
                "auth logout error: provider={} error={error}",
                provider.as_str()
            );
        }
    }

    if json_output {
        return serde_json::json!({
            "command": "auth.logout",
            "provider": provider.as_str(),
            "status": status,
            "credential_store": config.credential_store.display().to_string(),
        })
        .to_string();
    }

    format!(
        "auth logout: provider={} status={} credential_store={}",
        provider.as_str(),
        status,
        config.credential_store.display()
    )
}

/// Public `fn` `execute_auth_command` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn execute_auth_command(config: &AuthCommandConfig, command_args: &str) -> String {
    let command = match parse_auth_command(command_args) {
        Ok(command) => command,
        Err(error) => return format!("auth error: {error}"),
    };

    match command {
        AuthCommand::Login {
            provider,
            mode,
            launch,
            json_output,
        } => execute_auth_login_command(config, provider, mode, launch, json_output),
        AuthCommand::Reauth {
            provider,
            mode,
            launch,
            json_output,
        } => execute_auth_reauth_command(config, provider, mode, launch, json_output),
        AuthCommand::Status {
            provider,
            mode,
            mode_support,
            availability,
            state,
            source_kind,
            revoked,
            json_output,
        } => execute_auth_status_command(
            config,
            provider,
            AuthQueryFilters {
                mode,
                mode_support,
                availability,
                state,
                source_kind,
                revoked,
            },
            json_output,
        ),
        AuthCommand::Logout {
            provider,
            json_output,
        } => execute_auth_logout_command(config, provider, json_output),
        AuthCommand::Matrix {
            provider,
            mode,
            mode_support,
            availability,
            state,
            source_kind,
            revoked,
            json_output,
        } => execute_auth_matrix_command(
            config,
            provider,
            AuthQueryFilters {
                mode,
                mode_support,
                availability,
                state,
                source_kind,
                revoked,
            },
            json_output,
        ),
    }
}
