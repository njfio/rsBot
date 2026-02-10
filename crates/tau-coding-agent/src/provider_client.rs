use super::*;
use crate::claude_cli_client::{ClaudeCliClient, ClaudeCliConfig};
use crate::codex_cli_client::{CodexCliClient, CodexCliConfig};
use crate::gemini_cli_client::{GeminiCliClient, GeminiCliConfig};
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

fn should_use_subscription_backend_fallback(
    cli: &Cli,
    provider: Provider,
    auth_mode: ProviderAuthMethod,
    error: &anyhow::Error,
) -> bool {
    if auth_mode != ProviderAuthMethod::ApiKey {
        return false;
    }
    if !error
        .to_string()
        .contains(missing_provider_api_key_message(provider))
    {
        return false;
    }
    match provider {
        Provider::OpenAi => cli.openai_codex_backend,
        Provider::Anthropic => cli.anthropic_claude_backend,
        Provider::Google => cli.google_gemini_backend,
    }
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

fn anthropic_claude_backend_enabled(cli: &Cli, auth_mode: ProviderAuthMethod) -> bool {
    cli.anthropic_claude_backend
        && matches!(
            auth_mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
        )
}

fn build_anthropic_claude_client(
    cli: &Cli,
    auth_mode: ProviderAuthMethod,
) -> Result<Arc<dyn LlmClient>> {
    let client = ClaudeCliClient::new(ClaudeCliConfig {
        executable: cli.anthropic_claude_cli.clone(),
        extra_args: cli.anthropic_claude_args.clone(),
        timeout_ms: cli.anthropic_claude_timeout_ms.max(1),
    })?;
    tracing::debug!(
        provider = Provider::Anthropic.as_str(),
        auth_mode = auth_mode.as_str(),
        auth_source = "claude_cli",
        "provider auth resolved"
    );
    Ok(Arc::new(client))
}

fn google_gemini_backend_enabled(cli: &Cli, auth_mode: ProviderAuthMethod) -> bool {
    cli.google_gemini_backend
        && matches!(
            auth_mode,
            ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc
        )
}

fn build_google_gemini_client(
    cli: &Cli,
    auth_mode: ProviderAuthMethod,
) -> Result<Arc<dyn LlmClient>> {
    let client = GeminiCliClient::new(GeminiCliConfig {
        executable: cli.google_gemini_cli.clone(),
        extra_args: cli.google_gemini_args.clone(),
        timeout_ms: cli.google_gemini_timeout_ms.max(1),
    })?;
    tracing::debug!(
        provider = Provider::Google.as_str(),
        auth_mode = auth_mode.as_str(),
        auth_source = "gemini_cli",
        "provider auth resolved"
    );
    Ok(Arc::new(client))
}

fn api_key_fallback_blocked_by_subscription_strict_mode(
    cli: &Cli,
    auth_mode: ProviderAuthMethod,
) -> bool {
    cli.provider_subscription_strict && auth_mode != ProviderAuthMethod::ApiKey
}

fn resolve_api_key_fallback_credential(
    cli: &Cli,
    provider: Provider,
    auth_mode: ProviderAuthMethod,
) -> Option<ProviderAuthCredential> {
    if auth_mode == ProviderAuthMethod::ApiKey {
        return None;
    }
    if api_key_fallback_blocked_by_subscription_strict_mode(cli, auth_mode) {
        tracing::debug!(
            provider = provider.as_str(),
            auth_mode = auth_mode.as_str(),
            fallback_auth_mode = ProviderAuthMethod::ApiKey.as_str(),
            "primary auth mode unavailable; api-key fallback blocked by strict subscription mode"
        );
        return None;
    }
    let resolver = CliProviderCredentialResolver { cli };
    let resolved = resolver
        .resolve(provider, ProviderAuthMethod::ApiKey)
        .ok()?;
    tracing::debug!(
        provider = provider.as_str(),
        auth_mode = auth_mode.as_str(),
        fallback_auth_mode = ProviderAuthMethod::ApiKey.as_str(),
        fallback_auth_source = resolved.source.as_deref().unwrap_or("none"),
        "primary auth mode unavailable; falling back to api-key auth"
    );
    Some(resolved)
}

fn build_openai_http_client(
    cli: &Cli,
    resolved: &ProviderAuthCredential,
) -> Result<Arc<dyn LlmClient>> {
    let api_key = resolved_secret_for_provider(resolved, Provider::OpenAi)?;
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
    log_provider_auth_resolution(Provider::OpenAi, resolved, auth_source);
    Ok(Arc::new(client))
}

fn build_anthropic_http_client(
    cli: &Cli,
    resolved: &ProviderAuthCredential,
) -> Result<Arc<dyn LlmClient>> {
    let api_key = resolved_secret_for_provider(resolved, Provider::Anthropic)?;
    let client = AnthropicClient::new(AnthropicConfig {
        api_base: cli.anthropic_api_base.clone(),
        api_key,
        request_timeout_ms: cli.request_timeout_ms.max(1),
        max_retries: cli.provider_max_retries,
        retry_budget_ms: cli.provider_retry_budget_ms,
        retry_jitter: cli.provider_retry_jitter,
    })?;
    let auth_source = resolved.source.as_deref().unwrap_or("none");
    log_provider_auth_resolution(Provider::Anthropic, resolved, auth_source);
    Ok(Arc::new(client))
}

fn build_google_http_client(
    cli: &Cli,
    resolved: &ProviderAuthCredential,
) -> Result<Arc<dyn LlmClient>> {
    let api_key = resolved_secret_for_provider(resolved, Provider::Google)?;
    let client = GoogleClient::new(GoogleConfig {
        api_base: cli.google_api_base.clone(),
        api_key,
        request_timeout_ms: cli.request_timeout_ms.max(1),
        max_retries: cli.provider_max_retries,
        retry_budget_ms: cli.provider_retry_budget_ms,
        retry_jitter: cli.provider_retry_jitter,
    })?;
    let auth_source = resolved.source.as_deref().unwrap_or("none");
    log_provider_auth_resolution(Provider::Google, resolved, auth_source);
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
                    if let Some(fallback) =
                        resolve_api_key_fallback_credential(cli, provider, auth_mode)
                    {
                        Some(fallback)
                    } else if should_use_subscription_backend_fallback(
                        cli, provider, auth_mode, &error,
                    ) {
                        tracing::debug!(
                            provider = provider.as_str(),
                            auth_mode = auth_mode.as_str(),
                            auth_source = "codex_cli",
                            "openai api-key auth missing key; falling back to codex cli backend"
                        );
                        None
                    } else if openai_codex_backend_enabled(cli, auth_mode)
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
                return build_openai_http_client(cli, &resolved);
            }

            build_openai_codex_client(cli)
        }
        Provider::Anthropic => {
            if matches!(
                auth_mode,
                ProviderAuthMethod::OauthToken | ProviderAuthMethod::SessionToken
            ) {
                let backend_result = if anthropic_claude_backend_enabled(cli, auth_mode) {
                    build_anthropic_claude_client(cli, auth_mode)
                } else {
                    Err(anyhow!(
                        "anthropic auth mode '{}' requires Claude Code backend (enable --anthropic-claude-backend=true or set --anthropic-auth-mode api-key)",
                        auth_mode.as_str()
                    ))
                };
                match backend_result {
                    Ok(client) => return Ok(client),
                    Err(error) => {
                        if let Some(fallback) =
                            resolve_api_key_fallback_credential(cli, provider, auth_mode)
                        {
                            return build_anthropic_http_client(cli, &fallback);
                        }
                        return Err(error);
                    }
                }
            }

            let resolver = CliProviderCredentialResolver { cli };
            let resolved = match resolver.resolve(provider, auth_mode) {
                Ok(resolved) => Some(resolved),
                Err(error) => {
                    if should_use_subscription_backend_fallback(cli, provider, auth_mode, &error) {
                        tracing::debug!(
                            provider = provider.as_str(),
                            auth_mode = auth_mode.as_str(),
                            auth_source = "claude_cli",
                            "anthropic api-key auth missing key; falling back to claude cli backend"
                        );
                        None
                    } else {
                        return Err(error);
                    }
                }
            };
            let Some(resolved) = resolved else {
                return build_anthropic_claude_client(cli, ProviderAuthMethod::OauthToken);
            };
            build_anthropic_http_client(cli, &resolved)
        }
        Provider::Google => {
            if matches!(
                auth_mode,
                ProviderAuthMethod::OauthToken | ProviderAuthMethod::Adc
            ) {
                let backend_result = if google_gemini_backend_enabled(cli, auth_mode) {
                    build_google_gemini_client(cli, auth_mode)
                } else {
                    Err(anyhow!(
                        "google auth mode '{}' requires Gemini CLI backend (enable --google-gemini-backend=true or set --google-auth-mode api-key)",
                        auth_mode.as_str()
                    ))
                };
                match backend_result {
                    Ok(client) => return Ok(client),
                    Err(error) => {
                        if let Some(fallback) =
                            resolve_api_key_fallback_credential(cli, provider, auth_mode)
                        {
                            return build_google_http_client(cli, &fallback);
                        }
                        return Err(error);
                    }
                }
            }

            let resolver = CliProviderCredentialResolver { cli };
            let resolved = match resolver.resolve(provider, auth_mode) {
                Ok(resolved) => Some(resolved),
                Err(error) => {
                    if should_use_subscription_backend_fallback(cli, provider, auth_mode, &error) {
                        tracing::debug!(
                            provider = provider.as_str(),
                            auth_mode = auth_mode.as_str(),
                            auth_source = "gemini_cli",
                            "google api-key auth missing key; falling back to gemini cli backend"
                        );
                        None
                    } else {
                        return Err(error);
                    }
                }
            };
            let Some(resolved) = resolved else {
                return build_google_gemini_client(cli, ProviderAuthMethod::OauthToken);
            };
            build_google_http_client(cli, &resolved)
        }
    }
}
