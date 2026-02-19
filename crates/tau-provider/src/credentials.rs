//! Provider credential resolution from CLI flags, env, and credential store.
//!
//! Resolution order and expiration checks are centralized here so startup/client
//! code consumes deterministic auth material. Missing or expired credentials are
//! surfaced with provider-specific messages and fail-closed semantics.

use anyhow::{anyhow, Context, Result};
use tau_ai::Provider;
use tau_cli::Cli;
use tau_core::current_unix_timestamp;

use crate::auth::{
    configured_provider_auth_method_from_config, missing_provider_api_key_message,
    provider_api_key_candidates, provider_api_key_candidates_from_auth_config,
    provider_auth_capability, provider_login_access_token_candidates,
    resolve_auth_login_expires_unix,
};
use crate::cli_executable::is_executable_available;
use crate::credential_store::{
    load_credential_store, reauth_required_error, refresh_provider_access_token,
    resolve_credential_store_encryption_mode, save_credential_store,
};
use crate::types::{AuthCommandConfig, ProviderAuthMethod};
use crate::CredentialStoreData;

/// Public `fn` `resolve_store_backed_provider_credential` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn resolve_store_backed_provider_credential(
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

/// Public `fn` `resolve_non_empty_secret_with_source` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn resolve_non_empty_secret_with_source(
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
/// Public struct `ProviderAuthCredential` used across Tau components.
pub struct ProviderAuthCredential {
    pub method: ProviderAuthMethod,
    pub secret: Option<String>,
    pub source: Option<String>,
    pub expires_unix: Option<u64>,
    pub refreshable: bool,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `ProviderAuthSnapshot` used across Tau components.
pub struct ProviderAuthSnapshot {
    pub provider: Provider,
    pub method: ProviderAuthMethod,
    pub mode_supported: bool,
    pub available: bool,
    pub state: String,
    pub source: String,
    pub reason: String,
    pub expires_unix: Option<u64>,
    pub revoked: bool,
    pub refreshable: bool,
    pub secret: Option<String>,
}

fn google_gemini_backend_snapshot(
    config: &AuthCommandConfig,
    mode: ProviderAuthMethod,
) -> ProviderAuthSnapshot {
    if !config.google_gemini_backend {
        return ProviderAuthSnapshot {
            provider: Provider::Google,
            method: mode,
            mode_supported: true,
            available: false,
            state: "backend_disabled".to_string(),
            source: "none".to_string(),
            reason: "google gemini backend is disabled".to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    if !is_executable_available(&config.google_gemini_cli) {
        return ProviderAuthSnapshot {
            provider: Provider::Google,
            method: mode,
            mode_supported: true,
            available: false,
            state: "backend_unavailable".to_string(),
            source: "gemini_cli".to_string(),
            reason: format!(
                "gemini cli executable '{}' is not available",
                config.google_gemini_cli
            ),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    let reason = if mode == ProviderAuthMethod::Adc {
        "google_adc_backend_available"
    } else {
        "google_oauth_backend_available"
    };

    ProviderAuthSnapshot {
        provider: Provider::Google,
        method: mode,
        mode_supported: true,
        available: true,
        state: "ready".to_string(),
        source: "gemini_cli".to_string(),
        reason: reason.to_string(),
        expires_unix: None,
        revoked: false,
        refreshable: false,
        secret: None,
    }
}

fn openai_codex_backend_snapshot(
    config: &AuthCommandConfig,
    mode: ProviderAuthMethod,
) -> ProviderAuthSnapshot {
    if !config.openai_codex_backend {
        return ProviderAuthSnapshot {
            provider: Provider::OpenAi,
            method: mode,
            mode_supported: true,
            available: false,
            state: "backend_disabled".to_string(),
            source: "none".to_string(),
            reason: "openai codex backend is disabled".to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    if !is_executable_available(&config.openai_codex_cli) {
        return ProviderAuthSnapshot {
            provider: Provider::OpenAi,
            method: mode,
            mode_supported: true,
            available: false,
            state: "backend_unavailable".to_string(),
            source: "codex_cli".to_string(),
            reason: format!(
                "codex cli executable '{}' is not available",
                config.openai_codex_cli
            ),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    let reason = if mode == ProviderAuthMethod::SessionToken {
        "openai_session_backend_available"
    } else {
        "openai_oauth_backend_available"
    };

    ProviderAuthSnapshot {
        provider: Provider::OpenAi,
        method: mode,
        mode_supported: true,
        available: true,
        state: "ready".to_string(),
        source: "codex_cli".to_string(),
        reason: reason.to_string(),
        expires_unix: None,
        revoked: false,
        refreshable: false,
        secret: None,
    }
}

fn anthropic_claude_backend_snapshot(
    config: &AuthCommandConfig,
    mode: ProviderAuthMethod,
) -> ProviderAuthSnapshot {
    if !config.anthropic_claude_backend {
        return ProviderAuthSnapshot {
            provider: Provider::Anthropic,
            method: mode,
            mode_supported: true,
            available: false,
            state: "backend_disabled".to_string(),
            source: "none".to_string(),
            reason: "anthropic claude backend is disabled".to_string(),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    if !is_executable_available(&config.anthropic_claude_cli) {
        return ProviderAuthSnapshot {
            provider: Provider::Anthropic,
            method: mode,
            mode_supported: true,
            available: false,
            state: "backend_unavailable".to_string(),
            source: "claude_cli".to_string(),
            reason: format!(
                "claude cli executable '{}' is not available",
                config.anthropic_claude_cli
            ),
            expires_unix: None,
            revoked: false,
            refreshable: false,
            secret: None,
        };
    }

    let reason = if mode == ProviderAuthMethod::SessionToken {
        "anthropic_session_backend_available"
    } else {
        "anthropic_oauth_backend_available"
    };

    ProviderAuthSnapshot {
        provider: Provider::Anthropic,
        method: mode,
        mode_supported: true,
        available: true,
        state: "ready".to_string(),
        source: "claude_cli".to_string(),
        reason: reason.to_string(),
        expires_unix: None,
        revoked: false,
        refreshable: false,
        secret: None,
    }
}

/// Public `fn` `provider_auth_snapshot_for_status` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn provider_auth_snapshot_for_status(
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

    if provider == Provider::Google
        && matches!(
            mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc
        )
    {
        return google_gemini_backend_snapshot(config, mode);
    }

    if provider == Provider::Anthropic
        && matches!(
            mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
        )
    {
        return anthropic_claude_backend_snapshot(config, mode);
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

        if let Some(store) = store {
            if let Some(entry) = store.providers.get(provider.as_str()) {
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
                        refreshable: false,
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
                        refreshable: false,
                        secret: None,
                    };
                }
                if let Some(secret) = entry
                    .access_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                {
                    return ProviderAuthSnapshot {
                        provider,
                        method: mode,
                        mode_supported: true,
                        available: true,
                        state: "ready".to_string(),
                        source: "credential_store".to_string(),
                        reason: "credential available".to_string(),
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
                    state: "missing_access_token".to_string(),
                    source: "credential_store".to_string(),
                    reason: "credential store entry has no access token".to_string(),
                    expires_unix: None,
                    revoked: false,
                    refreshable: false,
                    secret: None,
                };
            }
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

        if matches!(provider, Provider::OpenAi | Provider::OpenRouter)
            && matches!(
                mode,
                ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
            )
        {
            return openai_codex_backend_snapshot(config, mode);
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

/// Trait contract for `ProviderCredentialResolver` behavior.
pub trait ProviderCredentialResolver {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ProviderAuthCredential>;
}

/// Public struct `CliProviderCredentialResolver` used across Tau components.
pub struct CliProviderCredentialResolver<'a> {
    pub cli: &'a Cli,
}

impl ProviderCredentialResolver for CliProviderCredentialResolver<'_> {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ProviderAuthCredential> {
        match method {
            ProviderAuthMethod::ApiKey => {
                if let Some((secret, source)) = resolve_non_empty_secret_with_source(
                    provider_api_key_candidates(self.cli, provider),
                ) {
                    return Ok(ProviderAuthCredential {
                        method,
                        secret: Some(secret),
                        source: Some(source),
                        expires_unix: None,
                        refreshable: false,
                        revoked: false,
                    });
                }

                let resolved_from_store =
                    resolve_store_backed_provider_credential(self.cli, provider, method);
                let (secret, source) = match resolved_from_store {
                    Ok(resolved) => (
                        resolved
                            .secret
                            .ok_or_else(|| anyhow!(missing_provider_api_key_message(provider)))?,
                        resolved
                            .source
                            .unwrap_or_else(|| "credential_store".to_string()),
                    ),
                    Err(error) => {
                        if error.to_string().contains("entry is missing") {
                            return Err(anyhow!(missing_provider_api_key_message(provider)));
                        }
                        return Err(error);
                    }
                };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialStoreEncryptionMode, ProviderCredentialStoreRecord};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn test_auth_config() -> AuthCommandConfig {
        let temp_dir = tempdir().expect("tempdir");
        AuthCommandConfig {
            credential_store: temp_dir.path().join("credentials.toml"),
            credential_store_key: Some("unit-test-store-key".to_string()),
            credential_store_encryption: CredentialStoreEncryptionMode::Keyed,
            api_key: None,
            openai_api_key: None,
            anthropic_api_key: None,
            google_api_key: None,
            openai_auth_mode: ProviderAuthMethod::ApiKey,
            anthropic_auth_mode: ProviderAuthMethod::ApiKey,
            google_auth_mode: ProviderAuthMethod::ApiKey,
            provider_subscription_strict: false,
            openai_codex_backend: true,
            openai_codex_cli: "codex".to_string(),
            anthropic_claude_backend: true,
            anthropic_claude_cli: "claude".to_string(),
            google_gemini_backend: true,
            google_gemini_cli: "gemini".to_string(),
            google_gcloud_cli: "gcloud".to_string(),
        }
    }

    #[test]
    fn unit_provider_auth_snapshot_for_status_api_key_mode_mismatch_returns_mode_mismatch_state() {
        let config = test_auth_config();
        let mut providers = BTreeMap::new();
        providers.insert(
            Provider::Google.as_str().to_string(),
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::OauthToken,
                access_token: Some("store-access-token".to_string()),
                refresh_token: None,
                expires_unix: None,
                revoked: false,
            },
        );
        let store = CredentialStoreData {
            encryption: CredentialStoreEncryptionMode::Keyed,
            providers,
            integrations: BTreeMap::new(),
        };

        let snapshot =
            provider_auth_snapshot_for_status(&config, Provider::Google, Some(&store), None);

        assert!(!snapshot.available);
        assert_eq!(snapshot.state, "mode_mismatch");
        assert_eq!(snapshot.source, "credential_store");
        assert!(snapshot.reason.contains("does not match configured mode"));
    }

    #[test]
    fn unit_provider_auth_snapshot_for_status_api_key_store_entry_non_empty_token_is_ready() {
        let config = test_auth_config();
        let mut providers = BTreeMap::new();
        providers.insert(
            Provider::Google.as_str().to_string(),
            ProviderCredentialStoreRecord {
                auth_method: ProviderAuthMethod::ApiKey,
                access_token: Some("store-api-key".to_string()),
                refresh_token: None,
                expires_unix: None,
                revoked: false,
            },
        );
        let store = CredentialStoreData {
            encryption: CredentialStoreEncryptionMode::Keyed,
            providers,
            integrations: BTreeMap::new(),
        };

        let snapshot =
            provider_auth_snapshot_for_status(&config, Provider::Google, Some(&store), None);

        assert!(snapshot.available);
        assert_eq!(snapshot.state, "ready");
        assert_eq!(snapshot.source, "credential_store");
        assert_eq!(snapshot.secret.as_deref(), Some("store-api-key"));
    }
}
