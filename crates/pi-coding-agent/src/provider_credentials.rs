use super::*;
use crate::auth_commands::{
    provider_api_key_candidates_from_auth_config, provider_login_access_token_candidates,
    resolve_auth_login_expires_unix,
};

pub(crate) fn resolve_store_backed_provider_credential(
    cli: &Cli,
    provider: Provider,
    method: ProviderAuthMethod,
) -> Result<ProviderAuthCredential> {
    let key = cli.credential_store_key.as_deref();
    let default_mode = resolve_credential_store_encryption_mode(cli);
    let mut store =
        load_credential_store(&cli.credential_store, default_mode, key).with_context(|| {
            format!(
                "failed to load provider credential store {}",
                cli.credential_store.display()
            )
        })?;
    let provider_key = provider.as_str().to_string();
    let Some(mut entry) = store.providers.get(&provider_key).cloned() else {
        return Err(reauth_required_error(
            provider,
            "credential store entry is missing",
        ));
    };

    if entry.auth_method != method {
        return Err(reauth_required_error(
            provider,
            "credential store auth mode does not match requested mode",
        ));
    }
    if entry.revoked {
        return Err(reauth_required_error(provider, "credential is revoked"));
    }

    let now_unix = current_unix_timestamp();
    let is_expired = entry
        .expires_unix
        .map(|value| value <= now_unix)
        .unwrap_or(false);
    let mut store_dirty = false;
    if is_expired {
        let Some(refresh_token) = entry
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
        else {
            return Err(reauth_required_error(
                provider,
                "credential expired and no refresh token is available",
            ));
        };

        match refresh_provider_access_token(provider, &refresh_token, now_unix) {
            Ok(refreshed) => {
                entry.access_token = Some(refreshed.access_token.clone());
                entry.refresh_token = refreshed.refresh_token.clone().or(Some(refresh_token));
                entry.expires_unix = refreshed.expires_unix;
                entry.revoked = false;
                store_dirty = true;
            }
            Err(error) => {
                if error.to_string().contains("revoked") {
                    entry.revoked = true;
                    store.providers.insert(provider_key.clone(), entry.clone());
                    let _ = save_credential_store(&cli.credential_store, &store, key);
                    return Err(reauth_required_error(
                        provider,
                        "refresh token has been revoked",
                    ));
                }
                return Err(reauth_required_error(provider, "credential refresh failed"));
            }
        }
    }

    let access_token = entry
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            reauth_required_error(
                provider,
                "credential store entry does not contain an access token",
            )
        })?;

    if store_dirty {
        store.providers.insert(provider_key, entry.clone());
        save_credential_store(&cli.credential_store, &store, key).with_context(|| {
            format!(
                "failed to persist refreshed provider credential store {}",
                cli.credential_store.display()
            )
        })?;
    }

    let refreshable = !entry.revoked
        && entry
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some();

    Ok(ProviderAuthCredential {
        method,
        secret: Some(access_token),
        source: Some("credential_store".to_string()),
        expires_unix: entry.expires_unix,
        refreshable,
        revoked: entry.revoked,
    })
}

pub(crate) fn resolve_non_empty_secret_with_source(
    candidates: Vec<(&'static str, Option<String>)>,
) -> Option<(String, String)> {
    candidates.into_iter().find_map(|(source, value)| {
        let value = value?;
        if value.trim().is_empty() {
            return None;
        }
        Some((value, source.to_string()))
    })
}

fn resolve_env_backed_provider_credential(
    provider: Provider,
    method: ProviderAuthMethod,
) -> Result<Option<ProviderAuthCredential>> {
    let Some((secret, source)) =
        resolve_non_empty_secret_with_source(provider_login_access_token_candidates(provider))
    else {
        return Ok(None);
    };

    let expires_unix = resolve_auth_login_expires_unix(provider)?;
    if expires_unix
        .map(|value| value <= current_unix_timestamp())
        .unwrap_or(false)
    {
        return Err(reauth_required_error(
            provider,
            "environment access token is expired",
        ));
    }

    Ok(Some(ProviderAuthCredential {
        method,
        secret: Some(secret),
        source: Some(source),
        expires_unix,
        refreshable: false,
        revoked: false,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderAuthCredential {
    pub(crate) method: ProviderAuthMethod,
    pub(crate) secret: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) expires_unix: Option<u64>,
    pub(crate) refreshable: bool,
    pub(crate) revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderAuthSnapshot {
    pub(crate) provider: Provider,
    pub(crate) method: ProviderAuthMethod,
    pub(crate) mode_supported: bool,
    pub(crate) available: bool,
    pub(crate) state: String,
    pub(crate) source: String,
    pub(crate) reason: String,
    pub(crate) expires_unix: Option<u64>,
    pub(crate) revoked: bool,
    pub(crate) refreshable: bool,
    pub(crate) secret: Option<String>,
}

pub(crate) fn provider_auth_snapshot_for_status(
    config: &AuthCommandConfig,
    provider: Provider,
    store: Option<&CredentialStoreData>,
    store_error: Option<&str>,
) -> ProviderAuthSnapshot {
    let mode = configured_provider_auth_method_from_config(config, provider);
    let capability = provider_auth_capability(provider, mode);
    if !capability.supported {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: false,
            available: false,
            state: "unsupported_mode".to_string(),
            source: "none".to_string(),
            reason: capability.reason.to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    if mode == ProviderAuthMethod::ApiKey {
        if let Some((secret, source)) = resolve_non_empty_secret_with_source(
            provider_api_key_candidates_from_auth_config(config, provider),
        ) {
            return ProviderAuthSnapshot {
                provider,
                method: mode,
                mode_supported: true,
                available: true,
                state: "ready".to_string(),
                source,
                reason: "api_key_available".to_string(),
                expires_unix: None,
                revoked: false,
                refreshable: false,
                secret: Some(secret),
            };
        }
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: "missing_api_key".to_string(),
            source: "none".to_string(),
            reason: missing_provider_api_key_message(provider).to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    if let Some(error) = store_error {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: "store_error".to_string(),
            source: "none".to_string(),
            reason: error.to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    let Some(store) = store else {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: "missing_credential_store".to_string(),
            source: "none".to_string(),
            reason: "credential store is unavailable".to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    };

    let Some(entry) = store.providers.get(provider.as_str()) else {
        if let Some((secret, source)) =
            resolve_non_empty_secret_with_source(provider_login_access_token_candidates(provider))
        {
            let expires_unix = match resolve_auth_login_expires_unix(provider) {
                Ok(value) => value,
                Err(error) => {
                    return ProviderAuthSnapshot {
                        provider,
                        method: mode,
                        mode_supported: true,
                        available: false,
                        state: "invalid_env_expires".to_string(),
                        source,
                        reason: error.to_string(),
                        expires_unix: None,
                        revoked: false,
                        refreshable: false,
                        secret: None,
                    };
                }
            };
            if expires_unix
                .map(|value| value <= current_unix_timestamp())
                .unwrap_or(false)
            {
                return ProviderAuthSnapshot {
                    provider,
                    method: mode,
                    mode_supported: true,
                    available: false,
                    state: "expired_env_access_token".to_string(),
                    source,
                    reason: "environment access token is expired".to_string(),
                    expires_unix,
                    revoked: false,
                    refreshable: false,
                    secret: None,
                };
            }
            return ProviderAuthSnapshot {
                provider,
                method: mode,
                mode_supported: true,
                available: true,
                state: "ready".to_string(),
                source,
                reason: "env_access_token_available".to_string(),
                expires_unix,
                revoked: false,
                refreshable: false,
                secret: Some(secret),
            };
        }

        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: "missing_credential".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry is missing".to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    };

    let refreshable = !entry.revoked
        && entry
            .refresh_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some();

    if entry.auth_method != mode {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
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
            refreshable,
            secret: None,
        };
    }
    if entry.revoked {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: "revoked".to_string(),
            source: "credential_store".to_string(),
            reason: "credential has been revoked".to_string(),
            expires_unix: entry.expires_unix,
            revoked: true,
            refreshable,
            secret: None,
        };
    }
    if entry
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: "missing_access_token".to_string(),
            source: "credential_store".to_string(),
            reason: "credential store entry has no access token".to_string(),
            expires_unix: entry.expires_unix,
            revoked: false,
            refreshable,
            secret: None,
        };
    }

    let now_unix = current_unix_timestamp();
    if entry
        .expires_unix
        .map(|value| value <= now_unix)
        .unwrap_or(false)
    {
        return ProviderAuthSnapshot {
            provider,
            method: mode,
            mode_supported: true,
            available: false,
            state: if refreshable {
                "expired_refresh_pending".to_string()
            } else {
                "expired".to_string()
            },
            source: "credential_store".to_string(),
            reason: if refreshable {
                "access token expired; refresh will run on next provider use".to_string()
            } else {
                "access token expired and no refresh token is available".to_string()
            },
            expires_unix: entry.expires_unix,
            revoked: false,
            refreshable,
            secret: None,
        };
    }

    ProviderAuthSnapshot {
        provider,
        method: mode,
        mode_supported: true,
        available: true,
        state: "ready".to_string(),
        source: "credential_store".to_string(),
        reason: "credential available".to_string(),
        expires_unix: entry.expires_unix,
        revoked: false,
        refreshable,
        secret: entry.access_token.clone(),
    }
}

pub(crate) trait ProviderCredentialResolver {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ProviderAuthCredential>;
}

pub(crate) struct CliProviderCredentialResolver<'a> {
    pub(crate) cli: &'a Cli,
}

impl ProviderCredentialResolver for CliProviderCredentialResolver<'_> {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ProviderAuthCredential> {
        match method {
            ProviderAuthMethod::ApiKey => {
                let (secret, source) = resolve_non_empty_secret_with_source(
                    provider_api_key_candidates(self.cli, provider),
                )
                .ok_or_else(|| anyhow!(missing_provider_api_key_message(provider)))?;
                Ok(ProviderAuthCredential {
                    method,
                    secret: Some(secret),
                    source: Some(source),
                    expires_unix: None,
                    refreshable: false,
                    revoked: false,
                })
            }
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken => {
                let key = self.cli.credential_store_key.as_deref();
                let default_mode = resolve_credential_store_encryption_mode(self.cli);
                let store_missing_provider_entry =
                    load_credential_store(&self.cli.credential_store, default_mode, key)
                        .map(|store| !store.providers.contains_key(provider.as_str()))
                        .unwrap_or(false);
                if store_missing_provider_entry {
                    if let Some(resolved) =
                        resolve_env_backed_provider_credential(provider, method)?
                    {
                        return Ok(resolved);
                    }
                }
                resolve_store_backed_provider_credential(self.cli, provider, method)
            }
            ProviderAuthMethod::Adc => Ok(ProviderAuthCredential {
                method,
                secret: None,
                source: None,
                expires_unix: None,
                refreshable: false,
                revoked: false,
            }),
        }
    }
}
