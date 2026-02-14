use anyhow::{Context, Result};
use tau_ai::Provider;
use tau_cli::Cli;

use crate::types::{AuthCommandConfig, ProviderAuthMethod};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `ProviderAuthCapability` used across Tau components.
pub struct ProviderAuthCapability {
    pub method: ProviderAuthMethod,
    pub supported: bool,
    pub reason: &'static str,
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

const GOOGLE_AUTH_CAPABILITIES: &[ProviderAuthCapability] = &[
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
        supported: true,
        reason: "supported",
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

pub fn provider_auth_capability(
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

pub fn provider_supported_auth_modes(provider: Provider) -> Vec<ProviderAuthMethod> {
    provider_auth_capabilities(provider)
        .iter()
        .filter(|capability| capability.supported)
        .map(|capability| capability.method)
        .collect()
}

pub fn configured_provider_auth_method(cli: &Cli, provider: Provider) -> ProviderAuthMethod {
    match provider {
        Provider::OpenAi => cli.openai_auth_mode.into(),
        Provider::Anthropic => cli.anthropic_auth_mode.into(),
        Provider::Google => cli.google_auth_mode.into(),
    }
}

pub fn configured_provider_auth_method_from_config(
    config: &AuthCommandConfig,
    provider: Provider,
) -> ProviderAuthMethod {
    match provider {
        Provider::OpenAi => config.openai_auth_mode,
        Provider::Anthropic => config.anthropic_auth_mode,
        Provider::Google => config.google_auth_mode,
    }
}

pub fn provider_auth_mode_flag(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "--openai-auth-mode",
        Provider::Anthropic => "--anthropic-auth-mode",
        Provider::Google => "--google-auth-mode",
    }
}

pub fn missing_provider_api_key_message(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => {
            "missing OpenAI-compatible API key. Set OPENAI_API_KEY, OPENROUTER_API_KEY, GROQ_API_KEY, XAI_API_KEY, MISTRAL_API_KEY, AZURE_OPENAI_API_KEY, TAU_API_KEY, --openai-api-key, or --api-key"
        }
        Provider::Anthropic => {
            "missing Anthropic API key. Set ANTHROPIC_API_KEY, TAU_API_KEY, --anthropic-api-key, or --api-key"
        }
        Provider::Google => {
            "missing Google API key. Set GEMINI_API_KEY, GOOGLE_API_KEY, TAU_API_KEY, --google-api-key, or --api-key"
        }
    }
}

pub fn provider_api_key_candidates_with_inputs(
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
            ("MISTRAL_API_KEY", std::env::var("MISTRAL_API_KEY").ok()),
            (
                "AZURE_OPENAI_API_KEY",
                std::env::var("AZURE_OPENAI_API_KEY").ok(),
            ),
            ("TAU_API_KEY", std::env::var("TAU_API_KEY").ok()),
        ],
        Provider::Anthropic => vec![
            ("--anthropic-api-key", anthropic_api_key),
            ("--api-key", api_key),
            ("ANTHROPIC_API_KEY", std::env::var("ANTHROPIC_API_KEY").ok()),
            ("TAU_API_KEY", std::env::var("TAU_API_KEY").ok()),
        ],
        Provider::Google => vec![
            ("--google-api-key", google_api_key),
            ("--api-key", api_key),
            ("GEMINI_API_KEY", std::env::var("GEMINI_API_KEY").ok()),
            ("GOOGLE_API_KEY", std::env::var("GOOGLE_API_KEY").ok()),
            ("TAU_API_KEY", std::env::var("TAU_API_KEY").ok()),
        ],
    }
}

pub fn provider_api_key_candidates(
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

pub fn resolve_api_key(candidates: Vec<Option<String>>) -> Option<String> {
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
}

pub fn provider_api_key_candidates_from_auth_config(
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

pub fn provider_login_access_token_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "TAU_AUTH_ACCESS_TOKEN",
                std::env::var("TAU_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "OPENAI_ACCESS_TOKEN",
                std::env::var("OPENAI_ACCESS_TOKEN").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "TAU_AUTH_ACCESS_TOKEN",
                std::env::var("TAU_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "ANTHROPIC_ACCESS_TOKEN",
                std::env::var("ANTHROPIC_ACCESS_TOKEN").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "TAU_AUTH_ACCESS_TOKEN",
                std::env::var("TAU_AUTH_ACCESS_TOKEN").ok(),
            ),
            (
                "GOOGLE_ACCESS_TOKEN",
                std::env::var("GOOGLE_ACCESS_TOKEN").ok(),
            ),
        ],
    }
}

pub fn provider_login_refresh_token_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "TAU_AUTH_REFRESH_TOKEN",
                std::env::var("TAU_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "OPENAI_REFRESH_TOKEN",
                std::env::var("OPENAI_REFRESH_TOKEN").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "TAU_AUTH_REFRESH_TOKEN",
                std::env::var("TAU_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "ANTHROPIC_REFRESH_TOKEN",
                std::env::var("ANTHROPIC_REFRESH_TOKEN").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "TAU_AUTH_REFRESH_TOKEN",
                std::env::var("TAU_AUTH_REFRESH_TOKEN").ok(),
            ),
            (
                "GOOGLE_REFRESH_TOKEN",
                std::env::var("GOOGLE_REFRESH_TOKEN").ok(),
            ),
        ],
    }
}

pub fn provider_login_expires_candidates(
    provider: Provider,
) -> Vec<(&'static str, Option<String>)> {
    match provider {
        Provider::OpenAi => vec![
            (
                "TAU_AUTH_EXPIRES_UNIX",
                std::env::var("TAU_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "OPENAI_AUTH_EXPIRES_UNIX",
                std::env::var("OPENAI_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
        Provider::Anthropic => vec![
            (
                "TAU_AUTH_EXPIRES_UNIX",
                std::env::var("TAU_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "ANTHROPIC_AUTH_EXPIRES_UNIX",
                std::env::var("ANTHROPIC_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
        Provider::Google => vec![
            (
                "TAU_AUTH_EXPIRES_UNIX",
                std::env::var("TAU_AUTH_EXPIRES_UNIX").ok(),
            ),
            (
                "GOOGLE_AUTH_EXPIRES_UNIX",
                std::env::var("GOOGLE_AUTH_EXPIRES_UNIX").ok(),
            ),
        ],
    }
}

pub fn resolve_auth_login_expires_unix(provider: Provider) -> Result<Option<u64>> {
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
