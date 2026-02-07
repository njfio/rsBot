use std::{fmt, str::FromStr};

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenAi,
    Anthropic,
    Google,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Google => "google",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModelRefParseError {
    #[error("missing model identifier")]
    MissingModel,
    #[error("unsupported provider '{0}'. Supported providers: openai, openrouter (alias), groq (alias), xai (alias), mistral (alias), azure/azure-openai (alias), anthropic, google")]
    UnsupportedProvider(String),
}

impl FromStr for Provider {
    type Err = ModelRefParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let normalized = value.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "openai" | "openrouter" | "groq" | "xai" | "mistral" | "azure" | "azure-openai" => {
                Ok(Provider::OpenAi)
            }
            "anthropic" => Ok(Provider::Anthropic),
            "google" | "gemini" => Ok(Provider::Google),
            _ => Err(ModelRefParseError::UnsupportedProvider(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: Provider,
    pub model: String,
}

impl ModelRef {
    pub fn parse(input: &str) -> Result<Self, ModelRefParseError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ModelRefParseError::MissingModel);
        }

        if let Some((provider, model)) = trimmed.split_once('/') {
            let model = model.trim();
            if model.is_empty() {
                return Err(ModelRefParseError::MissingModel);
            }

            return Ok(Self {
                provider: Provider::from_str(provider)?,
                model: model.to_string(),
            });
        }

        Ok(Self {
            provider: Provider::OpenAi,
            model: trimmed.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelRef, ModelRefParseError, Provider};

    #[test]
    fn parses_provider_and_model() {
        let parsed = ModelRef::parse("anthropic/claude-sonnet-4").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::Anthropic);
        assert_eq!(parsed.model, "claude-sonnet-4");
    }

    #[test]
    fn defaults_to_openai_when_provider_missing() {
        let parsed = ModelRef::parse("gpt-4o-mini").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::OpenAi);
        assert_eq!(parsed.model, "gpt-4o-mini");
    }

    #[test]
    fn parses_openrouter_as_openai_alias() {
        let parsed = ModelRef::parse("openrouter/openai/gpt-4o-mini").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::OpenAi);
        assert_eq!(parsed.model, "openai/gpt-4o-mini");
    }

    #[test]
    fn parses_groq_as_openai_alias() {
        let parsed = ModelRef::parse("groq/llama-3.3-70b").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::OpenAi);
        assert_eq!(parsed.model, "llama-3.3-70b");
    }

    #[test]
    fn parses_xai_as_openai_alias() {
        let parsed = ModelRef::parse("xai/grok-4").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::OpenAi);
        assert_eq!(parsed.model, "grok-4");
    }

    #[test]
    fn parses_mistral_as_openai_alias() {
        let parsed = ModelRef::parse("mistral/mistral-large-latest").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::OpenAi);
        assert_eq!(parsed.model, "mistral-large-latest");
    }

    #[test]
    fn parses_azure_openai_as_openai_alias() {
        let parsed = ModelRef::parse("azure/gpt-4o").expect("valid model ref");
        assert_eq!(parsed.provider, Provider::OpenAi);
        assert_eq!(parsed.model, "gpt-4o");
    }

    #[test]
    fn errors_on_unsupported_provider() {
        let error = ModelRef::parse("foo/model").expect_err("must reject unknown provider");
        assert_eq!(
            error,
            ModelRefParseError::UnsupportedProvider("foo".to_string())
        );
    }
}
