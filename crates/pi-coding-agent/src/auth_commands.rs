use super::*;

pub(crate) const AUTH_USAGE: &str = "usage: /auth <login|status|logout> ...";
pub(crate) const AUTH_LOGIN_USAGE: &str = "usage: /auth login <provider> [--mode <mode>] [--json]";
pub(crate) const AUTH_STATUS_USAGE: &str = "usage: /auth status [provider] [--json]";
pub(crate) const AUTH_LOGOUT_USAGE: &str = "usage: /auth logout <provider> [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthCommand {
    Login {
        provider: Provider,
        mode: Option<ProviderAuthMethod>,
        json_output: bool,
    },
    Status {
        provider: Option<Provider>,
        json_output: bool,
    },
    Logout {
        provider: Provider,
        json_output: bool,
    },
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

pub(crate) fn parse_auth_provider(token: &str) -> Result<Provider> {
    match token.trim().to_ascii_lowercase().as_str() {
        "openai" => Ok(Provider::OpenAi),
        "anthropic" => Ok(Provider::Anthropic),
        "google" => Ok(Provider::Google),
        other => bail!(
            "unknown provider '{}'; supported providers: openai, anthropic, google",
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
            let mut json_output = false;
            for token in tokens.into_iter().skip(1) {
                if token == "--json" {
                    json_output = true;
                    continue;
                }
                if provider.is_some() {
                    bail!("unexpected argument '{}'; {AUTH_STATUS_USAGE}", token);
                }
                provider = Some(parse_auth_provider(token)?);
            }
            Ok(AuthCommand::Status {
                provider,
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
        other => bail!("unknown subcommand '{}'; {AUTH_USAGE}", other),
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

pub(crate) fn execute_auth_status_command(
    config: &AuthCommandConfig,
    provider: Option<Provider>,
    json_output: bool,
) -> String {
    let providers = if let Some(provider) = provider {
        vec![provider]
    } else {
        vec![Provider::OpenAi, Provider::Anthropic, Provider::Google]
    };

    let requires_store = providers.iter().any(|provider| {
        configured_provider_auth_method_from_config(config, *provider) != ProviderAuthMethod::ApiKey
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

    let rows = providers
        .iter()
        .map(|provider| {
            auth_status_row_for_provider(config, *provider, store.as_ref(), store_error.as_deref())
        })
        .collect::<Vec<_>>();
    let available = rows.iter().filter(|row| row.available).count();
    let unavailable = rows.len().saturating_sub(available);

    if json_output {
        return serde_json::json!({
            "command": "auth.status",
            "providers": rows.len(),
            "available": available,
            "unavailable": unavailable,
            "entries": rows,
        })
        .to_string();
    }

    let mut lines = vec![format!(
        "auth status: providers={} available={} unavailable={}",
        rows.len(),
        available,
        unavailable
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
            json_output,
        } => execute_auth_status_command(config, provider, json_output),
        AuthCommand::Logout {
            provider,
            json_output,
        } => execute_auth_logout_command(config, provider, json_output),
    }
}
