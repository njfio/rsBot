use super::*;
use crate::provider_credentials::ResolvedProviderCredential;

fn resolved_secret_for_provider(
    resolved: &ResolvedProviderCredential,
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
    resolved: &ResolvedProviderCredential,
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

    let resolver = CliProviderCredentialResolver { cli };
    let resolved = resolver.resolve(provider, auth_mode)?;
    let auth_source = resolved.source.as_deref().unwrap_or("none");

    match provider {
        Provider::OpenAi => {
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
            log_provider_auth_resolution(provider, &resolved, auth_source);
            Ok(Arc::new(client))
        }
        Provider::Anthropic => {
            let api_key = resolved_secret_for_provider(&resolved, provider)?;
            let client = AnthropicClient::new(AnthropicConfig {
                api_base: cli.anthropic_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            log_provider_auth_resolution(provider, &resolved, auth_source);
            Ok(Arc::new(client))
        }
        Provider::Google => {
            let api_key = resolved_secret_for_provider(&resolved, provider)?;
            let client = GoogleClient::new(GoogleConfig {
                api_base: cli.google_api_base.clone(),
                api_key,
                request_timeout_ms: cli.request_timeout_ms.max(1),
                max_retries: cli.provider_max_retries,
                retry_budget_ms: cli.provider_retry_budget_ms,
                retry_jitter: cli.provider_retry_jitter,
            })?;
            log_provider_auth_resolution(provider, &resolved, auth_source);
            Ok(Arc::new(client))
        }
    }
}
