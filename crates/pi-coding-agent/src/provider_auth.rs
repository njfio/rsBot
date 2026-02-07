use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderAuthCapability {
    pub(crate) method: ProviderAuthMethod,
    pub(crate) supported: bool,
    pub(crate) reason: &'static str,
}

const OPENAI_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
    ProviderAuthCapability {
        method: ProviderAuthMethod::ApiKey,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::OauthToken,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::Adc,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::SessionToken,
        supported: true,
        reason: "supported",
    },
];

const ANTHROPIC_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
    ProviderAuthCapability {
        method: ProviderAuthMethod::ApiKey,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::OauthToken,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::Adc,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::SessionToken,
        supported: false,
        reason: "unsupported",
    },
];

const GOOGLE_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
    ProviderAuthCapability {
        method: ProviderAuthMethod::ApiKey,
        supported: true,
        reason: "supported",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::OauthToken,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::Adc,
        supported: false,
        reason: "not_implemented",
    },
    ProviderAuthCapability {
        method: ProviderAuthMethod::SessionToken,
        supported: false,
        reason: "unsupported",
    },
];

fn provider_auth_capabilities(provider: Provider) -> &'static [ProviderAuthCapability] {
    match provider {
        Provider::OpenAi => OPENAI_AUTH_CAPABILITIES,
        Provider::Anthropic => ANTHROPIC_AUTH_CAPABILITIES,
        Provider::Google => GOOGLE_AUTH_CAPABILITIES,
    }
}

pub(crate) fn provider_auth_capability(
    provider: Provider,
    method: ProviderAuthMethod,
) -> ProviderAuthCapability {
    provider_auth_capabilities(provider)
        .iter()
        .find(|capability| capability.method == method)
        .copied()
        .unwrap_or(ProviderAuthCapability {
            method,
            supported: false,
            reason: "unknown",
        })
}

pub(crate) fn configured_provider_auth_method(cli: &Cli, provider: Provider) -> ProviderAuthMethod {
    match provider {
        Provider::OpenAi => cli.openai_auth_mode.into(),
        Provider::Anthropic => cli.anthropic_auth_mode.into(),
        Provider::Google => cli.google_auth_mode.into(),
    }
}

pub(crate) fn configured_provider_auth_method_from_config(
    config: &AuthCommandConfig,
    provider: Provider,
) -> ProviderAuthMethod {
    match provider {
        Provider::OpenAi => config.openai_auth_mode,
        Provider::Anthropic => config.anthropic_auth_mode,
        Provider::Google => config.google_auth_mode,
    }
}

pub(crate) fn provider_auth_mode_flag(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "--openai-auth-mode",
        Provider::Anthropic => "--anthropic-auth-mode",
        Provider::Google => "--google-auth-mode",
    }
}

pub(crate) fn missing_provider_api_key_message(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => {
            "missing OpenAI-compatible API key. Set OPENAI_API_KEY, OPENROUTER_API_KEY, GROQ_API_KEY, XAI_API_KEY, PI_API_KEY, --openai-api-key, or --api-key"
        }
        Provider::Anthropic => {
            "missing Anthropic API key. Set ANTHROPIC_API_KEY, PI_API_KEY, --anthropic-api-key, or --api-key"
        }
        Provider::Google => {
            "missing Google API key. Set GEMINI_API_KEY, GOOGLE_API_KEY, PI_API_KEY, --google-api-key, or --api-key"
        }
    }
}

pub(crate) fn provider_api_key_candidates_with_inputs(
    provider: Provider,
    api_key: Option<String>,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
    google_api_key: Option<String>,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            ("--openai-api-key", openai_api_key),
            ("--api-key", api_key),
            ("OPENAI_API_KEY", std::env::var("OPENAI_API_KEY").ok()),
            (
                "OPENROUTER_API_KEY",
                std::env::var("OPENROUTER_API_KEY").ok(),
            ),
            ("GROQ_API_KEY", std::env::var("GROQ_API_KEY").ok()),
            ("XAI_API_KEY", std::env::var("XAI_API_KEY").ok()),
            ("PI_API_KEY", std::env::var("PI_API_KEY").ok()),
        ],
        Provider::Anthropic => vec![
            ("--anthropic-api-key", anthropic_api_key),
            ("--api-key", api_key),
            ("ANTHROPIC_API_KEY", std::env::var("ANTHROPIC_API_KEY").ok()),
            ("PI_API_KEY", std::env::var("PI_API_KEY").ok()),
        ],
        Provider::Google => vec![
            ("--google-api-key", google_api_key),
            ("--api-key", api_key),
            ("GEMINI_API_KEY", std::env::var("GEMINI_API_KEY").ok()),
            ("GOOGLE_API_KEY", std::env::var("GOOGLE_API_KEY").ok()),
            ("PI_API_KEY", std::env::var("PI_API_KEY").ok()),
        ],
    }
}

pub(crate) fn provider_api_key_candidates(
    cli: &Cli,
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    provider_api_key_candidates_with_inputs(
        provider,
        cli.api_key.clone(),
        cli.openai_api_key.clone(),
        cli.anthropic_api_key.clone(),
        cli.google_api_key.clone(),
    )
}

pub(crate) fn resolve_api_key(candidates: Vec<Option<String>>) -> Option<String> {
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}
