use super::*;

pub(crate) fn resolve_store_backed_provider_credential(
    cli: &Cli,
    provider: Provider,
    method: ProviderAuthMethod,
) -> Result<ResolvedProviderCredential> {
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

    Ok(ResolvedProviderCredential {
        method,
        secret: Some(access_token),
        source: Some("credential_store".to_string()),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProviderCredential {
    pub(crate) method: ProviderAuthMethod,
    pub(crate) secret: Option<String>,
    pub(crate) source: Option<String>,
}

pub(crate) trait ProviderCredentialResolver {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ResolvedProviderCredential>;
}

pub(crate) struct CliProviderCredentialResolver<'a> {
    pub(crate) cli: &'a Cli,
}

impl ProviderCredentialResolver for CliProviderCredentialResolver<'_> {
    fn resolve(
        &self,
        provider: Provider,
        method: ProviderAuthMethod,
    ) -> Result<ResolvedProviderCredential> {
        match method {
            ProviderAuthMethod::ApiKey => {
                let (secret, source) = resolve_non_empty_secret_with_source(
                    provider_api_key_candidates(self.cli, provider),
                )
                .ok_or_else(|| anyhow!(missing_provider_api_key_message(provider)))?;
                Ok(ResolvedProviderCredential {
                    method,
                    secret: Some(secret),
                    source: Some(source),
                })
            }
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken => {
                resolve_store_backed_provider_credential(self.cli, provider, method)
            }
            ProviderAuthMethod::Adc => Ok(ResolvedProviderCredential {
                method,
                secret: None,
                source: None,
            }),
        }
    }
}
