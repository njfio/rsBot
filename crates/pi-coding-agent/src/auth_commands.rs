use super::*;

pub(crate) const AUTH_USAGE: &str = "usage: /auth <login|status|logout|matrix> ...";
pub(crate) const AUTH_LOGIN_USAGE: &str = "usage: /auth login <provider> [--mode <mode>] [--json]";
pub(crate) const AUTH_STATUS_USAGE: &str =
    "usage: /auth status [provider] [--mode <mode>] [--mode-support <all|supported|unsupported>] [--availability <all|available|unavailable>] [--state <state>] [--source-kind <all|flag|env|credential-store|none>] [--json]";
pub(crate) const AUTH_LOGOUT_USAGE: &str = "usage: /auth logout <provider> [--json]";
pub(crate) const AUTH_MATRIX_USAGE: &str =
    "usage: /auth matrix [provider] [--mode <mode>] [--mode-support <all|supported|unsupported>] [--availability <all|available|unavailable>] [--state <state>] [--source-kind <all|flag|env|credential-store|none>] [--json]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthMatrixAvailabilityFilter {
    All,
    Available,
    Unavailable,
}

impl AuthMatrixAvailabilityFilter {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthMatrixModeSupportFilter {
    All,
    Supported,
    Unsupported,
}

impl AuthMatrixModeSupportFilter {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Supported => "supported",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthSourceKindFilter {
    All,
    Flag,
    Env,
    CredentialStore,
    None,
}

impl AuthSourceKindFilter {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Flag => "flag",
            Self::Env => "env",
            Self::CredentialStore => "credential_store",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthCommand {
    Login {
        provider: Provider,
        mode: Option<ProviderAuthMethod>,
        json_output: bool,
    },
    Status {
        provider: Option<Provider>,
        mode: Option<ProviderAuthMethod>,
        mode_support: AuthMatrixModeSupportFilter,
        availability: AuthMatrixAvailabilityFilter,
        state: Option<String>,
        source_kind: AuthSourceKindFilter,
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
        json_output: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthQueryFilters {
    pub(crate) mode: Option<ProviderAuthMethod>,
    pub(crate) mode_support: AuthMatrixModeSupportFilter,
    pub(crate) availability: AuthMatrixAvailabilityFilter,
    pub(crate) state: Option<String>,
    pub(crate) source_kind: AuthSourceKindFilter,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AuthStatusRow {
    provider: String,
    mode: String,
    mode_supported: bool,
    available: bool,
    state: String,
    source: String,
    reason: String,
    expires_unix: Option<u64>,
    revoked: bool,
}

const AUTH_MATRIX_PROVIDERS: [Provider; 3] =
    [Provider::OpenAi, Provider::Anthropic, Provider::Google];
const AUTH_MATRIX_MODES: [ProviderAuthMethod; 4] = [
    ProviderAuthMethod::ApiKey,
    ProviderAuthMethod::OauthToken,
    ProviderAuthMethod::Adc,
    ProviderAuthMethod::SessionToken,
];

pub(crate) fn parse_auth_provider(token: &str) -> Result<Provider> {
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

pub(crate) fn parse_provider_auth_method_token(token: &str) -> Result<ProviderAuthMethod> {
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

pub(crate) fn parse_auth_command(command_args: &str) -> Result<AuthCommand> {
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
                    other => bail!("unexpected argument '{}'; {AUTH_LOGIN_USAGE}", other),
                }
            }

            Ok(AuthCommand::Login {
                provider,
                mode,
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
                json_output,
            })
        }
        other => bail!("unknown subcommand '{}'; {AUTH_USAGE}", other),
    }
}

pub(crate) fn parse_auth_matrix_availability_filter(
    token: &str,
) -> Result<AuthMatrixAvailabilityFilter> {
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

pub(crate) fn parse_auth_matrix_mode_support_filter(
    token: &str,
) -> Result<AuthMatrixModeSupportFilter> {
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

pub(crate) fn parse_auth_matrix_state_filter(token: &str) -> Result<String> {
    let normalized = token.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        bail!("state filter must not be empty");
    }
    Ok(normalized)
}

pub(crate) fn parse_auth_source_kind_filter(token: &str) -> Result<AuthSourceKindFilter> {
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

pub(crate) fn provider_api_key_candidates_from_auth_config(
    config: &AuthCommandConfig,
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    provider_api_key_candidates_with_inputs(
        provider,
        config.api_key.clone(),
        config.openai_api_key.clone(),
        config.anthropic_api_key.clone(),
        config.google_api_key.clone(),
    )
}

pub(crate) fn provider_login_access_token_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "PI_AUTH_ACCESS_TOKEN",
                std::env::var("PI_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "OPENAI_ACCESS_TOKEN",
                std::env::var("OPENAI_ACCESS_TOKEN").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "PI_AUTH_ACCESS_TOKEN",
                std::env::var("PI_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "ANTHROPIC_ACCESS_TOKEN",
                std::env::var("ANTHROPIC_ACCESS_TOKEN").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "PI_AUTH_ACCESS_TOKEN",
                std::env::var("PI_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "GOOGLE_ACCESS_TOKEN",
                std::env::var("GOOGLE_ACCESS_TOKEN").ok(),
            ),
        ],
    }
}

pub(crate) fn provider_login_refresh_token_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "PI_AUTH_REFRESH_TOKEN",
                std::env::var("PI_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "OPENAI_REFRESH_TOKEN",
                std::env::var("OPENAI_REFRESH_TOKEN").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "PI_AUTH_REFRESH_TOKEN",
                std::env::var("PI_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "ANTHROPIC_REFRESH_TOKEN",
                std::env::var("ANTHROPIC_REFRESH_TOKEN").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "PI_AUTH_REFRESH_TOKEN",
                std::env::var("PI_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "GOOGLE_REFRESH_TOKEN",
                std::env::var("GOOGLE_REFRESH_TOKEN").ok(),
            ),
        ],
    }
}

pub(crate) fn provider_login_expires_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "PI_AUTH_EXPIRES_UNIX",
                std::env::var("PI_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "OPENAI_AUTH_EXPIRES_UNIX",
                std::env::var("OPENAI_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "PI_AUTH_EXPIRES_UNIX",
                std::env::var("PI_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "ANTHROPIC_AUTH_EXPIRES_UNIX",
                std::env::var("ANTHROPIC_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "PI_AUTH_EXPIRES_UNIX",
                std::env::var("PI_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "GOOGLE_AUTH_EXPIRES_UNIX",
                std::env::var("GOOGLE_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
    }
}

pub(crate) fn resolve_auth_login_expires_unix(provider: Provider) -> Result<Option<u64>> {
    for (source, value) in provider_login_expires_candidates(provider) {
        let Some(value) = value else {
            continue;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = trimmed
            .parse::<u64>()
            .with_context(|| format!("invalid unix timestamp in {}", source))?;
        return Ok(Some(parsed));
    }
    Ok(None)
}

pub(crate) fn execute_auth_login_command(
    config: &AuthCommandConfig,
    provider: Provider,
    mode_override: Option<ProviderAuthMethod>,
    json_output: bool,
) -> String {
    let mode = mode_override
        .unwrap_or_else(|| configured_provider_auth_method_from_config(config, provider));
    let capability = provider_auth_capability(provider, mode);
    if !capability.supported {
        let reason = format!(
            "auth mode '{}' is not supported for provider '{}': {}",
            mode.as_str(),
            provider.as_str(),
            capability.reason
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
        return format!(
            "auth login error: provider={} mode={} error={reason}",
            provider.as_str(),
            mode.as_str()
        );
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
                    let reason = missing_provider_api_key_message(provider).to_string();
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
        ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken => {
            let Some((access_token, access_source)) = resolve_non_empty_secret_with_source(
                provider_login_access_token_candidates(provider),
            ) else {
                let reason = "missing access token for login. Set PI_AUTH_ACCESS_TOKEN or provider-specific *_ACCESS_TOKEN env var".to_string();
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
                return format!(
                    "auth login error: provider={} mode={} error={reason}",
                    provider.as_str(),
                    mode.as_str()
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
                            "reason": error.to_string(),
                        })
                        .to_string();
                    }
                    return format!(
                        "auth login error: provider={} mode={} error={error}",
                        provider.as_str(),
                        mode.as_str()
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
                            "reason": error.to_string(),
                        })
                        .to_string();
                    }
                    return format!(
                        "auth login error: provider={} mode={} error={error}",
                        provider.as_str(),
                        mode.as_str()
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
                        "reason": error.to_string(),
                    })
                    .to_string();
                }
                return format!(
                    "auth login error: provider={} mode={} error={error}",
                    provider.as_str(),
                    mode.as_str()
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
            let reason = "adc login flow is not implemented".to_string();
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

pub(crate) fn auth_status_row_for_provider(
    config: &AuthCommandConfig,
    provider: Provider,
    store: Option<&CredentialStoreData>,
    store_error: Option<&str>,
) -> AuthStatusRow {
    let mode = configured_provider_auth_method_from_config(config, provider);
    let capability = provider_auth_capability(provider, mode);
    if !capability.supported {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: false,
            available: false,
            state: "unsupported_mode".to_string(),
            source: "none".to_string(),
            reason: capability.reason.to_string(),
            expires_unix: None,
            revoked: false,
        };
    }

    if mode == ProviderAuthMethod::ApiKey {
        if let Some((_secret, source)) = resolve_non_empty_secret_with_source(
            provider_api_key_candidates_from_auth_config(config, provider),
        ) {
            return AuthStatusRow {
                provider: provider.as_str().to_string(),
                mode: mode.as_str().to_string(),
                mode_supported: true,
                available: true,
                state: "ready".to_string(),
                source,
                reason: "api_key_available".to_string(),
                expires_unix: None,
                revoked: false,
            };
        }
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_api_key".to_string(),
            source: "none".to_string(),
            reason: missing_provider_api_key_message(provider).to_string(),
            expires_unix: None,
            revoked: false,
        };
    }

    if let Some(error) = store_error {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "store_error".to_string(),
            source: "none".to_string(),
            reason: error.to_string(),
            expires_unix: None,
            revoked: false,
        };
    }

    let Some(store) = store else {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_credential_store".to_string(),
            source: "none".to_string(),
            reason: "credential store is unavailable".to_string(),
            expires_unix: None,
            revoked: false,
        };
    };

    let Some(entry) = store.providers.get(provider.as_str()) else {
        if let Some((_access_token, source)) =
            resolve_non_empty_secret_with_source(provider_login_access_token_candidates(provider))
        {
            let expires_unix = match resolve_auth_login_expires_unix(provider) {
                Ok(value) => value,
                Err(error) => {
                    return AuthStatusRow {
                        provider: provider.as_str().to_string(),
                        mode: mode.as_str().to_string(),
                        mode_supported: true,
                        available: false,
                        state: "invalid_env_expires".to_string(),
                        source,
                        reason: error.to_string(),
                        expires_unix: None,
                        revoked: false,
                    };
                }
            };
            if expires_unix
                .map(|value| value <= current_unix_timestamp())
                .unwrap_or(false)
            {
                return AuthStatusRow {
                    provider: provider.as_str().to_string(),
                    mode: mode.as_str().to_string(),
                    mode_supported: true,
                    available: false,
                    state: "expired_env_access_token".to_string(),
                    source,
                    reason: "environment access token is expired".to_string(),
                    expires_unix,
                    revoked: false,
                };
            }
            return AuthStatusRow {
                provider: provider.as_str().to_string(),
                mode: mode.as_str().to_string(),
                mode_supported: true,
                available: true,
                state: "ready".to_string(),
                source,
                reason: "env_access_token_available".to_string(),
                expires_unix,
                revoked: false,
            };
        }

        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_credential".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry is missing".to_string(),
            expires_unix: None,
            revoked: false,
        };
    };
    if entry.auth_method != mode {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "mode_mismatch".to_string(),
            source: "credential_store".to_string(),
            reason: format!(
                "credential store entry mode '{}' does not match configured mode '{}'",
                entry.auth_method.as_str(),
                mode.as_str()
            ),
            expires_unix: entry.expires_unix,
            revoked: entry.revoked,
        };
    }
    if entry.revoked {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "revoked".to_string(),
            source: "credential_store".to_string(),
            reason: "credential has been revoked".to_string(),
            expires_unix: entry.expires_unix,
            revoked: true,
        };
    }
    if entry
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: "missing_access_token".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry has no access token".to_string(),
            expires_unix: entry.expires_unix,
            revoked: false,
        };
    }

    let now_unix = current_unix_timestamp();
    if entry
        .expires_unix
        .map(|value| value <= now_unix)
        .unwrap_or(false)
    {
        let refresh_pending = entry
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some();
        return AuthStatusRow {
            provider: provider.as_str().to_string(),
            mode: mode.as_str().to_string(),
            mode_supported: true,
            available: false,
            state: if refresh_pending {
                "expired_refresh_pending".to_string()
            } else {
                "expired".to_string()
            },
            source: "credential_store".to_string(),
            reason: if refresh_pending {
                "access token expired; refresh will run on next provider use".to_string()
            } else {
                "access token expired and no refresh token is available".to_string()
            },
            expires_unix: entry.expires_unix,
            revoked: false,
        };
    }

    AuthStatusRow {
        provider: provider.as_str().to_string(),
        mode: mode.as_str().to_string(),
        mode_supported: true,
        available: true,
        state: "ready".to_string(),
        source: "credential_store".to_string(),
        reason: "credential available".to_string(),
        expires_unix: entry.expires_unix,
        revoked: false,
    }
}

pub(crate) fn auth_state_counts(
    rows: &[AuthStatusRow],
) -> std::collections::BTreeMap<String, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for row in rows {
        *counts.entry(row.state.clone()).or_insert(0) += 1;
    }
    counts
}

pub(crate) fn auth_source_kind(source: &str) -> &'static str {
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

pub(crate) fn auth_source_kind_counts(
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

pub(crate) fn format_auth_state_counts(
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

pub(crate) fn execute_auth_status_command(
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
    let state_counts_total = auth_state_counts(&rows);
    let source_kind_counts_total = auth_source_kind_counts(&rows);
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

    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);
    let mode_supported = rows.iter().filter(|row| row.mode_supported).count();
    let mode_unsupported = rows.len().saturating_sub(mode_supported);
    let state_counts = auth_state_counts(&rows);
    let source_kind_counts = auth_source_kind_counts(&rows);

    if json_output {
        return serde_json::json!({
            "command": "auth.status",
            "provider_filter": provider_filter,
            "mode_filter": mode_filter,
            "mode_support_filter": mode_support.as_str(),
            "availability_filter": availability.as_str(),
            "state_filter": state_filter.as_deref().unwrap_or("all"),
            "source_kind_filter": source_kind.as_str(),
            "providers": selected_providers.len(),
            "rows_total": total_rows,
            "rows": rows.len(),
            "mode_supported": mode_supported,
            "mode_unsupported": mode_unsupported,
            "mode_supported_total": mode_supported_total,
            "mode_unsupported_total": mode_unsupported_total,
            "available": available,
            "unavailable": unavailable,
            "state_counts_total": state_counts_total,
            "state_counts": state_counts,
            "source_kind_counts_total": source_kind_counts_total,
            "source_kind_counts": source_kind_counts,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "auth status: providers={} rows={} mode_supported={} mode_unsupported={} available={} unavailable={} provider_filter={} mode_filter={} mode_support_filter={} availability_filter={} state_filter={} source_kind_filter={} rows_total={} mode_supported_total={} mode_unsupported_total={} state_counts={} state_counts_total={} source_kind_counts={} source_kind_counts_total={}",
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
        total_rows,
        mode_supported_total,
        mode_unsupported_total,
        format_auth_state_counts(&state_counts),
        format_auth_state_counts(&state_counts_total),
        format_auth_state_counts(&source_kind_counts),
        format_auth_state_counts(&source_kind_counts_total)
    )];
    for row in rows {
        lines.push(format!(
            "auth provider: name={} mode={} mode_supported={} available={} state={} source={} reason={} expires_unix={} revoked={}",
            row.provider,
            row.mode,
            row.mode_supported,
            row.available,
            row.state,
            row.source,
            row.reason,
            row.expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.revoked
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

pub(crate) fn execute_auth_matrix_command(
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
    let state_counts_total = auth_state_counts(&rows);
    let source_kind_counts_total = auth_source_kind_counts(&rows);
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

    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);
    let mode_supported = rows.iter().filter(|row| row.mode_supported).count();
    let mode_unsupported = rows.len().saturating_sub(mode_supported);
    let state_counts = auth_state_counts(&rows);
    let source_kind_counts = auth_source_kind_counts(&rows);

    if json_output {
        return serde_json::json!({
            "command": "auth.matrix",
            "provider_filter": provider_filter,
            "mode_filter": mode_filter,
            "mode_support_filter": mode_support.as_str(),
            "availability_filter": availability.as_str(),
            "state_filter": state_filter.as_deref().unwrap_or("all"),
            "source_kind_filter": source_kind.as_str(),
            "providers": selected_providers.len(),
            "modes": selected_modes.len(),
            "rows_total": total_rows,
            "rows": rows.len(),
            "mode_supported": mode_supported,
            "mode_unsupported": mode_unsupported,
            "mode_supported_total": mode_supported_total,
            "mode_unsupported_total": mode_unsupported_total,
            "available": available,
            "unavailable": unavailable,
            "state_counts_total": state_counts_total,
            "state_counts": state_counts,
            "source_kind_counts_total": source_kind_counts_total,
            "source_kind_counts": source_kind_counts,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "auth matrix: providers={} modes={} rows={} mode_supported={} mode_unsupported={} available={} unavailable={} provider_filter={} mode_filter={} mode_support_filter={} availability_filter={} state_filter={} source_kind_filter={} rows_total={} mode_supported_total={} mode_unsupported_total={} state_counts={} state_counts_total={} source_kind_counts={} source_kind_counts_total={}",
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
        total_rows,
        mode_supported_total,
        mode_unsupported_total,
        format_auth_state_counts(&state_counts),
        format_auth_state_counts(&state_counts_total),
        format_auth_state_counts(&source_kind_counts),
        format_auth_state_counts(&source_kind_counts_total)
    )];
    for row in rows {
        lines.push(format!(
            "auth matrix row: provider={} mode={} mode_supported={} available={} state={} source={} reason={} expires_unix={} revoked={}",
            row.provider,
            row.mode,
            row.mode_supported,
            row.available,
            row.state,
            row.source,
            row.reason,
            row.expires_unix
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.revoked
        ));
    }
    lines.join("\n")
}

pub(crate) fn execute_auth_logout_command(
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

pub(crate) fn execute_auth_command(config: &AuthCommandConfig, command_args: &str) -> String {
    let command = match parse_auth_command(command_args) {
        Ok(command) => command,
        Err(error) => return format!("auth error: {error}"),
    };

    match command {
        AuthCommand::Login {
            provider,
            mode,
            json_output,
        } => execute_auth_login_command(config, provider, mode, json_output),
        AuthCommand::Status {
            provider,
            mode,
            mode_support,
            availability,
            state,
            source_kind,
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
            },
            json_output,
        ),
    }
}
