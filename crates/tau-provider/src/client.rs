//! Provider client factory and runtime selection helpers.
//!
//! This module builds concrete `LlmClient` instances from resolved provider/auth
//! configuration and model refs. It enforces unsupported/missing-auth failure
//! checks before returning a runnable client stack.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use tau_ai::{
    AnthropicClient, AnthropicConfig, GoogleClient, GoogleConfig, LlmClient, OpenAiAuthScheme,
    OpenAiClient, OpenAiConfig, Provider,
};
use tau_cli::Cli;

use crate::auth::{
    configured_provider_auth_method, missing_provider_api_key_message, provider_auth_capability,
    provider_auth_mode_flag,
};
use crate::claude_cli_client::{ClaudeCliClient, ClaudeCliConfig};
use crate::codex_cli_client::{CodexCliClient, CodexCliConfig};
use crate::credential_store::{load_credential_store, resolve_credential_store_encryption_mode};
use crate::credentials::{
    CliProviderCredentialResolver, ProviderAuthCredential, ProviderCredentialResolver,
};
use crate::gemini_cli_client::{GeminiCliClient, GeminiCliConfig};
use crate::types::ProviderAuthMethod;

const DEFAULT_OPENAI_API_BASE: &str = "https://api.openai.com/v1";
const DEFAULT_OPENROUTER_API_BASE: &str = "https://openrouter.ai/api/v1";

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn resolve_openrouter_api_base(configured_api_base: &str) -> String {
    non_empty_env("TAU_OPENROUTER_API_BASE").unwrap_or_else(|| {
        if configured_api_base
            .trim()
            .eq_ignore_ascii_case(DEFAULT_OPENAI_API_BASE)
        {
            DEFAULT_OPENROUTER_API_BASE.to_string()
        } else {
            configured_api_base.to_string()
        }
    })
}

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
        Provider::OpenAi | Provider::OpenRouter => cli.openai_codex_backend,
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

fn openai_credential_store_missing_provider_entry(cli: &Cli, provider: Provider) -> bool {
    let key = cli.credential_store_key.as_deref();
    let default_mode = resolve_credential_store_encryption_mode(cli);
    load_credential_store(&cli.credential_store, default_mode, key)
        .map(|store| !store.providers.contains_key(provider.as_str()))
        .unwrap_or(false)
}

fn build_openai_codex_client(cli: &Cli, provider: Provider) -> Result<Arc<dyn LlmClient>> {
    let client = CodexCliClient::new(CodexCliConfig {
        executable: cli.openai_codex_cli.clone(),
        extra_args: cli.openai_codex_args.clone(),
        timeout_ms: cli.openai_codex_timeout_ms.max(1),
    })?;
    tracing::debug!(
        provider = provider.as_str(),
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
    provider: Provider,
) -> Result<Arc<dyn LlmClient>> {
    let api_key = resolved_secret_for_provider(resolved, provider)?;
    let api_base = match provider {
        Provider::OpenRouter => resolve_openrouter_api_base(&cli.api_base),
        Provider::OpenAi | Provider::Anthropic | Provider::Google => cli.api_base.clone(),
    };
    let azure_mode = provider == Provider::OpenAi && is_azure_openai_endpoint(&api_base);
    let client = OpenAiClient::new(OpenAiConfig {
        api_base,
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
    log_provider_auth_resolution(provider, resolved, auth_source);
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

/// Public `fn` `build_provider_client` in `tau-provider`.
///
/// This item is part of the Wave 2 API surface for M23 documentation uplift.
/// Callers rely on its contract and failure semantics remaining stable.
/// Update this comment if behavior or integration expectations change.
pub fn build_provider_client(cli: &Cli, provider: Provider) -> Result<Arc<dyn LlmClient>> {
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
        Provider::OpenAi | Provider::OpenRouter => {
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
                            "openai-compatible api-key auth missing key; falling back to codex cli backend"
                        );
                        None
                    } else if openai_codex_backend_enabled(cli, auth_mode)
                        && openai_credential_store_missing_provider_entry(cli, provider)
                    {
                        tracing::debug!(
                            provider = provider.as_str(),
                            auth_mode = auth_mode.as_str(),
                            "openai-compatible credential store entry missing; falling back to codex cli backend"
                        );
                        None
                    } else {
                        return Err(error);
                    }
                }
            };

            if let Some(resolved) = resolved {
                return build_openai_http_client(cli, &resolved, provider);
            }

            build_openai_codex_client(cli, provider)
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

#[cfg(test)]
mod tests {
    use super::resolve_openrouter_api_base;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn spec_c05_openrouter_uses_default_route_when_openai_default_is_configured() {
        let _guard = env_lock().lock().expect("acquire env lock");
        let prior = std::env::var("TAU_OPENROUTER_API_BASE").ok();
        std::env::remove_var("TAU_OPENROUTER_API_BASE");

        let resolved = resolve_openrouter_api_base("https://api.openai.com/v1");
        assert_eq!(resolved, "https://openrouter.ai/api/v1");

        match prior {
            Some(value) => std::env::set_var("TAU_OPENROUTER_API_BASE", value),
            None => std::env::remove_var("TAU_OPENROUTER_API_BASE"),
        }
    }

    #[test]
    fn regression_openrouter_respects_explicit_base_override() {
        let _guard = env_lock().lock().expect("acquire env lock");
        let prior = std::env::var("TAU_OPENROUTER_API_BASE").ok();
        std::env::remove_var("TAU_OPENROUTER_API_BASE");

        let resolved = resolve_openrouter_api_base("http://127.0.0.1:8080/v1");
        assert_eq!(resolved, "http://127.0.0.1:8080/v1");

        match prior {
            Some(value) => std::env::set_var("TAU_OPENROUTER_API_BASE", value),
            None => std::env::remove_var("TAU_OPENROUTER_API_BASE"),
        }
    }
}
