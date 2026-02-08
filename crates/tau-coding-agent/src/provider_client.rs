use super::*;
use crate::codex_cli_client::{CodexCliClient, CodexCliConfig};
use crate::provider_credentials::ProviderAuthCredential;

fn resolved_secret_for_provider(
    resolved: &ProviderAuthCredential,
    provider: Provider,
) -> Result<String> {
    resolved.secret.clone().ok_or_else(|| {
        anyhow!(
            "resolved auth mode '{}' for '{}' did not provide a credential",
            resolved.method.as_str(),
            provider.as_str()
        )
    })
}

fn log_provider_auth_resolution(
    provider: Provider,
    resolved: &ProviderAuthCredential,
    auth_source: &str,
) {
    tracing::debug!(
        provider = provider.as_str(),
        auth_mode = resolved.method.as_str(),
        auth_source = auth_source,
        "provider auth resolved"
    );
}

fn is_azure_openai_endpoint(api_base: &str) -> bool {
    let normalized = api_base.trim().to_ascii_lowercase();
    normalized.contains(".openai.azure.com") || normalized.contains("/openai/deployments/")
}

fn openai_codex_backend_enabled(cli: &Cli, auth_mode: ProviderAuthMethod) -> bool {
    cli.openai_codex_backend
        && matches!(
            auth_mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
        )
}

fn openai_credential_store_missing_provider_entry(cli: &Cli) -> bool {
    let key = cli.credential_store_key.as_deref();
    let default_mode = resolve_credential_store_encryption_mode(cli);
    load_credential_store(&cli.credential_store, default_mode, key)
        .map(|store| !store.providers.contains_key(Provider::OpenAi.as_str()))
        .unwrap_or(false)
}

fn build_openai_codex_client(cli: &Cli) -> Result<Arc<dyn LlmClient>> {
    let client = CodexCliClient::new(CodexCliConfig {
        executable: cli.openai_codex_cli.clone(),
        extra_args: cli.openai_codex_args.clone(),
        timeout_ms: cli.openai_codex_timeout_ms.max(1),
    })?;
    tracing::debug!(
        provider = Provider::OpenAi.as_str(),
        auth_mode = "codex_backend",
        auth_source = "codex_cli",
        "provider auth resolved"
    );
    Ok(Arc::new(client))
}

pub(crate) fn build_provider_client(cli: &Cli, provider: Provider) -> Result<Arc<dyn LlmClient>> {
    let auth_mode = configured_provider_auth_method(cli, provider);
    let capability = provider_auth_capability(provider, auth_mode);
    if !capability.supported {
        bail!(
            "unsupported auth mode '{}' for provider '{}': {} (set {} api-key)",
            auth_mode.as_str(),
            provider.as_str(),
            capability.reason,
            provider_auth_mode_flag(provider),
        );
    }

    match provider {
        Provider::OpenAi => {
            let resolver = CliProviderCredentialResolver { cli };
            let resolved = match resolver.resolve(provider, auth_mode) {
                Ok(resolved) => Some(resolved),
                Err(error) => {
                    if openai_codex_backend_enabled(cli, auth_mode)
                        && openai_credential_store_missing_provider_entry(cli)
                    {
                        tracing::debug!(
                            provider = provider.as_str(),
                            auth_mode = auth_mode.as_str(),
                            "openai credential store entry missing; falling back to codex cli backend"
                        );
                        None
                    } else {
                        return Err(error);
                    }
                }
            };

            if let Some(resolved) = resolved {
                let api_key = resolved_secret_for_provider(&resolved, provider)?;
                let azure_mode = is_azure_openai_endpoint(&cli.api_base);
                let client = OpenAiClient::new(OpenAiConfig {
                    api_base: cli.api_base.clone(),
                    api_key,
                    organization: None,
                    request_timeout_ms: cli.request_timeout_ms.max(1),
                    max_retries: cli.provider_max_retries,
                    retry_budget_ms: cli.provider_retry_budget_ms,
                    retry_jitter: cli.provider_retry_jitter,
                    auth_scheme: if azure_mode {
                        OpenAiAuthScheme::ApiKeyHeader
                    } else {
                        OpenAiAuthScheme::Bearer
                    },
                    api_version: if azure_mode {
                        Some(cli.azure_openai_api_version.clone())
                    } else {
                        None
                    },
                })?;
                let auth_source = resolved.source.as_deref().unwrap_or("none");
                log_provider_auth_resolution(provider, &resolved, auth_source);
                return Ok(Arc::new(client));
            }

            build_openai_codex_client(cli)
        }
        Provider::Anthropic => {
            let resolver = CliProviderCredentialResolver { cli };
            let resolved = resolver.resolve(provider, auth_mode)?;
            let api_key = resolved_secret_for_provider(&resolved, provider)?;
            let client = AnthropicClient::new(AnthropicConfig {
                api_base: cli.anthropic_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            let auth_source = resolved.source.as_deref().unwrap_or("none");
            log_provider_auth_resolution(provider, &resolved, auth_source);
            Ok(Arc::new(client))
        }
        Provider::Google => {
            let resolver = CliProviderCredentialResolver { cli };
            let resolved = resolver.resolve(provider, auth_mode)?;
            let api_key = resolved_secret_for_provider(&resolved, provider)?;
            let client = GoogleClient::new(GoogleConfig {
                api_base: cli.google_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            let auth_source = resolved.source.as_deref().unwrap_or("none");
            log_provider_auth_resolution(provider, &resolved, auth_source);
            Ok(Arc::new(client))
        }
    }
}
