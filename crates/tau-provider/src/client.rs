//! Provider client factory and runtime selection helpers.
//!
//! This module builds concrete `LlmClient` instances from resolved provider/auth
//! configuration and model refs. It enforces unsupported/missing-auth failure
//! checks before returning a runnable client stack.

use std::{
    collections::HashMap,
    convert::TryFrom,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
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
const PROVIDER_RATE_LIMIT_CONFIG_KEY_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProviderOutboundRateLimitConfig {
    capacity: u32,
    refill_per_second: u32,
    max_wait_ms: u64,
}

impl ProviderOutboundRateLimitConfig {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            capacity: cli.provider_rate_limit_capacity,
            refill_per_second: cli.provider_rate_limit_refill_per_second,
            max_wait_ms: cli.provider_rate_limit_max_wait_ms,
        }
    }

    fn enabled(self) -> bool {
        self.capacity > 0 && self.refill_per_second > 0
    }

    fn cache_key(self, provider: Provider) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            PROVIDER_RATE_LIMIT_CONFIG_KEY_VERSION,
            provider.as_str(),
            self.capacity,
            self.refill_per_second,
            self.max_wait_ms
        )
    }
}

#[derive(Debug)]
struct ProviderTokenBucketState {
    tokens: f64,
    last_refill: Instant,
}

#[derive(Debug)]
struct ProviderTokenBucketLimiter {
    config: ProviderOutboundRateLimitConfig,
    state: Mutex<ProviderTokenBucketState>,
}

impl ProviderTokenBucketLimiter {
    fn new(config: ProviderOutboundRateLimitConfig) -> Self {
        Self {
            config,
            state: Mutex::new(ProviderTokenBucketState {
                tokens: config.capacity as f64,
                last_refill: Instant::now(),
            }),
        }
    }

    fn registry() -> &'static Mutex<HashMap<String, Arc<ProviderTokenBucketLimiter>>> {
        static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<ProviderTokenBucketLimiter>>>> =
            OnceLock::new();
        REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn shared(provider: Provider, config: ProviderOutboundRateLimitConfig) -> Arc<Self> {
        let key = config.cache_key(provider);
        let mut registry = Self::registry()
            .lock()
            .expect("provider rate limiter registry lock poisoned");
        if let Some(existing) = registry.get(&key) {
            return Arc::clone(existing);
        }
        let created = Arc::new(Self::new(config));
        registry.insert(key, Arc::clone(&created));
        created
    }

    fn refill_locked(&self, state: &mut ProviderTokenBucketState, now: Instant) {
        let elapsed = now
            .saturating_duration_since(state.last_refill)
            .as_secs_f64();
        if elapsed <= f64::EPSILON {
            return;
        }
        let refill = elapsed * self.config.refill_per_second as f64;
        state.tokens = (state.tokens + refill).min(self.config.capacity as f64);
        state.last_refill = now;
    }

    fn wait_budget_exceeded(&self, elapsed_ms: u64, wait_duration: Duration) -> bool {
        if self.config.max_wait_ms == 0 {
            return true;
        }

        let wait_ms = u64::try_from(wait_duration.as_millis()).unwrap_or(u64::MAX);
        elapsed_ms.saturating_add(wait_ms) > self.config.max_wait_ms
    }

    async fn acquire(&self, provider: Provider) -> Result<(), tau_ai::TauAiError> {
        if !self.config.enabled() {
            return Ok(());
        }

        let start = Instant::now();
        loop {
            let wait = {
                let mut state = self
                    .state
                    .lock()
                    .expect("provider rate limiter lock poisoned");
                let now = Instant::now();
                self.refill_locked(&mut state, now);
                if state.tokens >= 1.0 {
                    state.tokens -= 1.0;
                    None
                } else {
                    let missing = (1.0 - state.tokens).max(0.0);
                    let wait_seconds = missing / self.config.refill_per_second as f64;
                    Some(Duration::from_secs_f64(wait_seconds))
                }
            };

            match wait {
                None => return Ok(()),
                Some(wait_duration) => {
                    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    if self.wait_budget_exceeded(elapsed_ms, wait_duration) {
                        return Err(tau_ai::TauAiError::InvalidResponse(format!(
                            "provider outbound rate limit exceeded for '{}' (retry_after_ms={}, max_wait_ms={})",
                            provider.as_str(),
                            wait_duration.as_millis(),
                            self.config.max_wait_ms
                        )));
                    }
                    tokio::time::sleep(wait_duration).await;
                }
            }
        }
    }
}

#[derive(Clone)]
struct ProviderRateLimitedClient {
    provider: Provider,
    inner: Arc<dyn LlmClient>,
    limiter: Arc<ProviderTokenBucketLimiter>,
}

impl ProviderRateLimitedClient {
    fn new(
        provider: Provider,
        inner: Arc<dyn LlmClient>,
        config: ProviderOutboundRateLimitConfig,
    ) -> Self {
        Self {
            provider,
            inner,
            limiter: ProviderTokenBucketLimiter::shared(provider, config),
        }
    }
}

#[async_trait]
impl LlmClient for ProviderRateLimitedClient {
    async fn complete(
        &self,
        request: tau_ai::ChatRequest,
    ) -> Result<tau_ai::ChatResponse, tau_ai::TauAiError> {
        self.limiter.acquire(self.provider).await?;
        self.inner.complete(request).await
    }

    async fn complete_with_stream(
        &self,
        request: tau_ai::ChatRequest,
        on_delta: Option<tau_ai::StreamDeltaHandler>,
    ) -> Result<tau_ai::ChatResponse, tau_ai::TauAiError> {
        self.limiter.acquire(self.provider).await?;
        self.inner.complete_with_stream(request, on_delta).await
    }
}

fn maybe_wrap_provider_rate_limited_client(
    config: ProviderOutboundRateLimitConfig,
    provider: Provider,
    client: Arc<dyn LlmClient>,
) -> Arc<dyn LlmClient> {
    if !config.enabled() {
        return client;
    }

    Arc::new(ProviderRateLimitedClient::new(provider, client, config))
}

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
    let rate_limit_config = ProviderOutboundRateLimitConfig::from_cli(cli);
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
    Ok(maybe_wrap_provider_rate_limited_client(
        rate_limit_config,
        provider,
        Arc::new(client),
    ))
}

fn build_anthropic_http_client(
    cli: &Cli,
    resolved: &ProviderAuthCredential,
) -> Result<Arc<dyn LlmClient>> {
    let rate_limit_config = ProviderOutboundRateLimitConfig::from_cli(cli);
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
    Ok(maybe_wrap_provider_rate_limited_client(
        rate_limit_config,
        Provider::Anthropic,
        Arc::new(client),
    ))
}

fn build_google_http_client(
    cli: &Cli,
    resolved: &ProviderAuthCredential,
) -> Result<Arc<dyn LlmClient>> {
    let rate_limit_config = ProviderOutboundRateLimitConfig::from_cli(cli);
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
    Ok(maybe_wrap_provider_rate_limited_client(
        rate_limit_config,
        Provider::Google,
        Arc::new(client),
    ))
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
    use super::{
        is_azure_openai_endpoint, maybe_wrap_provider_rate_limited_client,
        resolve_openrouter_api_base, resolved_secret_for_provider, ProviderOutboundRateLimitConfig,
        ProviderRateLimitedClient, ProviderTokenBucketLimiter,
    };
    use crate::credentials::ProviderAuthCredential;
    use crate::types::ProviderAuthMethod;
    use async_trait::async_trait;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    };
    use std::time::{Duration, Instant};
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, Provider, TauAiError};

    struct StubLlmClient {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmClient for StubLlmClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ChatResponse {
                message: Message::assistant_text("ok"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage::default(),
            })
        }
    }

    fn request_fixture() -> ChatRequest {
        ChatRequest {
            model: "gpt-4o-mini".to_string(),
            messages: vec![Message::user("hello")],
            tools: Vec::new(),
            tool_choice: None,
            json_mode: false,
            max_tokens: Some(16),
            temperature: Some(0.0),
            prompt_cache: Default::default(),
        }
    }

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

    #[test]
    fn unit_spec_2609_c05_provider_client_auth_helper_decisions() {
        assert!(is_azure_openai_endpoint(
            "https://example.openai.azure.com/openai/deployments/deploy/chat/completions"
        ));
        assert!(is_azure_openai_endpoint(
            "https://proxy.local/openai/deployments/deploy/chat/completions"
        ));
        assert!(!is_azure_openai_endpoint("https://api.openai.com/v1"));

        let resolved = ProviderAuthCredential {
            method: ProviderAuthMethod::ApiKey,
            secret: Some("sk-test".to_string()),
            source: Some("unit-test".to_string()),
            expires_unix: None,
            refreshable: false,
            revoked: false,
        };
        let secret = resolved_secret_for_provider(&resolved, Provider::OpenAi)
            .expect("resolved credential should include secret");
        assert_eq!(secret, "sk-test");

        let missing_secret = ProviderAuthCredential {
            method: ProviderAuthMethod::OauthToken,
            secret: None,
            source: Some("credential_store".to_string()),
            expires_unix: None,
            refreshable: true,
            revoked: false,
        };
        let error = resolved_secret_for_provider(&missing_secret, Provider::OpenAi)
            .expect_err("missing secret must fail closed");
        assert!(error.to_string().contains("did not provide a credential"));
    }

    #[test]
    fn unit_spec_2611_c01_limiter_enablement_requires_positive_capacity_and_refill() {
        assert!(!ProviderOutboundRateLimitConfig {
            capacity: 0,
            refill_per_second: 1,
            max_wait_ms: 1,
        }
        .enabled());
        assert!(!ProviderOutboundRateLimitConfig {
            capacity: 1,
            refill_per_second: 0,
            max_wait_ms: 1,
        }
        .enabled());
        assert!(!ProviderOutboundRateLimitConfig {
            capacity: 0,
            refill_per_second: 0,
            max_wait_ms: 1,
        }
        .enabled());
        assert!(ProviderOutboundRateLimitConfig {
            capacity: 1,
            refill_per_second: 1,
            max_wait_ms: 1,
        }
        .enabled());
    }

    #[test]
    fn unit_spec_2611_c03_wait_budget_boundary_allows_exact_budget() {
        let limiter = ProviderTokenBucketLimiter::new(ProviderOutboundRateLimitConfig {
            capacity: 1,
            refill_per_second: 20,
            max_wait_ms: 50,
        });
        assert!(
            !limiter.wait_budget_exceeded(0, Duration::from_millis(50)),
            "exact wait budget should be allowed"
        );
        assert!(
            limiter.wait_budget_exceeded(1, Duration::from_millis(50)),
            "elapsed plus wait beyond budget must fail closed"
        );
    }

    #[test]
    fn regression_spec_2611_c01_disabled_limiter_preserves_original_client_arc() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner: Arc<dyn LlmClient> = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let wrapped = maybe_wrap_provider_rate_limited_client(
            ProviderOutboundRateLimitConfig {
                capacity: 0,
                refill_per_second: 10,
                max_wait_ms: 100,
            },
            Provider::OpenAi,
            Arc::clone(&inner),
        );
        assert!(
            Arc::ptr_eq(&inner, &wrapped),
            "disabled limiter should return original client without wrapping"
        );
    }

    #[tokio::test]
    async fn functional_spec_2611_c02_provider_rate_limiter_delays_burst_calls() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let client = ProviderRateLimitedClient::new(
            Provider::OpenAi,
            inner,
            ProviderOutboundRateLimitConfig {
                capacity: 1,
                refill_per_second: 20,
                max_wait_ms: 500,
            },
        );

        let request = request_fixture();
        client
            .complete(request.clone())
            .await
            .expect("first call should pass immediately");

        let start = Instant::now();
        client
            .complete(request)
            .await
            .expect("second call should wait then succeed");
        assert!(
            start.elapsed() >= Duration::from_millis(40),
            "second call should be delayed by limiter refill interval"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn regression_spec_2611_c03_provider_rate_limiter_fails_closed_when_wait_budget_exceeded()
    {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let client = ProviderRateLimitedClient::new(
            Provider::OpenAi,
            inner,
            ProviderOutboundRateLimitConfig {
                capacity: 1,
                refill_per_second: 1,
                max_wait_ms: 5,
            },
        );

        let request = request_fixture();
        client
            .complete(request.clone())
            .await
            .expect("first call should pass immediately");
        let error = client
            .complete(request)
            .await
            .expect_err("second call should fail closed when max wait is exceeded");
        assert!(matches!(error, TauAiError::InvalidResponse(_)));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "inner client should not receive rejected call"
        );
    }

    #[tokio::test]
    async fn regression_spec_2611_c03_partial_refill_wait_within_budget_succeeds() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let client = ProviderRateLimitedClient::new(
            Provider::OpenAi,
            inner,
            ProviderOutboundRateLimitConfig {
                capacity: 1,
                refill_per_second: 5,
                max_wait_ms: 200,
            },
        );

        let request = request_fixture();
        client
            .complete(request.clone())
            .await
            .expect("first call should consume initial token");
        tokio::time::sleep(Duration::from_millis(60)).await;

        client
            .complete(request)
            .await
            .expect("partial refill should still complete within wait budget");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn regression_spec_2611_c03_registry_state_is_shared_per_provider_and_config() {
        let calls = Arc::new(AtomicUsize::new(0));
        let first = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let second = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let config = ProviderOutboundRateLimitConfig {
            capacity: 1,
            refill_per_second: 1,
            max_wait_ms: 10,
        };
        let first_client = ProviderRateLimitedClient::new(Provider::OpenRouter, first, config);
        let second_client = ProviderRateLimitedClient::new(Provider::OpenRouter, second, config);

        first_client
            .complete(request_fixture())
            .await
            .expect("first call should consume shared token");
        let error = second_client
            .complete(request_fixture())
            .await
            .expect_err("shared limiter should reject immediate second call");
        assert!(matches!(error, TauAiError::InvalidResponse(_)));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn integration_spec_2611_c04_provider_rate_limited_client_preserves_success_semantics() {
        let calls = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(StubLlmClient {
            calls: Arc::clone(&calls),
        });
        let client = ProviderRateLimitedClient::new(
            Provider::OpenAi,
            inner,
            ProviderOutboundRateLimitConfig {
                capacity: 2,
                refill_per_second: 1,
                max_wait_ms: 100,
            },
        );

        let response = client
            .complete(request_fixture())
            .await
            .expect("allowed call should preserve inner success response");
        assert_eq!(response.message.text_content(), "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
