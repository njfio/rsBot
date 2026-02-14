use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const DEFAULT_PROVIDER_TIMEOUT_MS: u64 = 15_000;
const MAX_ERROR_BODY_CHARS: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `VoiceProviderErrorCode` values.
pub enum VoiceProviderErrorCode {
    InvalidInput,
    InvalidResponse,
    AuthFailed,
    Timeout,
    RateLimited,
    BackendUnavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `VoiceProviderError` used across Tau components.
pub struct VoiceProviderError {
    pub code: VoiceProviderErrorCode,
    pub provider: String,
    pub retryable: bool,
    pub message: String,
}

impl VoiceProviderError {
    fn invalid_input(provider: &str, message: impl Into<String>) -> Self {
        Self {
            code: VoiceProviderErrorCode::InvalidInput,
            provider: provider.to_string(),
            retryable: false,
            message: message.into(),
        }
    }

    fn invalid_response(provider: &str, message: impl Into<String>) -> Self {
        Self {
            code: VoiceProviderErrorCode::InvalidResponse,
            provider: provider.to_string(),
            retryable: false,
            message: message.into(),
        }
    }

    fn backend_unavailable(provider: &str, message: impl Into<String>) -> Self {
        Self {
            code: VoiceProviderErrorCode::BackendUnavailable,
            provider: provider.to_string(),
            retryable: true,
            message: message.into(),
        }
    }

    fn timeout(provider: &str, message: impl Into<String>) -> Self {
        Self {
            code: VoiceProviderErrorCode::Timeout,
            provider: provider.to_string(),
            retryable: true,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for VoiceProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "voice provider error: provider={} code={:?} retryable={} message={}",
            self.provider, self.code, self.retryable, self.message
        )
    }
}

impl std::error::Error for VoiceProviderError {}

pub type VoiceProviderResult<T> = Result<T, VoiceProviderError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `SttRequest` used across Tau components.
pub struct SttRequest {
    pub audio_bytes: Vec<u8>,
    pub mime_type: String,
    pub locale: Option<String>,
    pub sample_rate_hz: Option<u32>,
    pub timeout_ms: u64,
}

impl SttRequest {
    pub fn new(audio_bytes: Vec<u8>) -> Self {
        Self {
            audio_bytes,
            mime_type: "audio/wav".to_string(),
            locale: None,
            sample_rate_hz: None,
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Public struct `SttResponse` used across Tau components.
pub struct SttResponse {
    pub transcript: String,
    pub confidence: Option<f32>,
    pub language: Option<String>,
    pub provider_metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `TtsRequest` used across Tau components.
pub struct TtsRequest {
    pub text: String,
    pub voice_id: Option<String>,
    pub locale: Option<String>,
    pub mime_type: String,
    pub timeout_ms: u64,
}

impl TtsRequest {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice_id: None,
            locale: None,
            mime_type: "audio/wav".to_string(),
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `TtsResponse` used across Tau components.
pub struct TtsResponse {
    pub audio_bytes: Vec<u8>,
    pub mime_type: String,
    pub duration_ms: Option<u64>,
    pub provider_metadata: Value,
}

#[async_trait]
/// Trait contract for `SttProvider` behavior.
pub trait SttProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn transcribe(&self, request: SttRequest) -> VoiceProviderResult<SttResponse>;
}

#[async_trait]
/// Trait contract for `TtsProvider` behavior.
pub trait TtsProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;

    async fn synthesize(&self, request: TtsRequest) -> VoiceProviderResult<TtsResponse>;
}

#[derive(Debug, Clone, Default)]
/// Public struct `DeterministicVoiceProvider` used across Tau components.
pub struct DeterministicVoiceProvider;

impl DeterministicVoiceProvider {
    const PROVIDER_NAME: &'static str = "deterministic-mock";
}

#[async_trait]
impl SttProvider for DeterministicVoiceProvider {
    fn provider_name(&self) -> &'static str {
        Self::PROVIDER_NAME
    }

    async fn transcribe(&self, request: SttRequest) -> VoiceProviderResult<SttResponse> {
        let provider = Self::PROVIDER_NAME;
        if request.audio_bytes.is_empty() {
            return Err(VoiceProviderError::invalid_input(
                provider,
                "audio_bytes must not be empty",
            ));
        }

        let transcript = String::from_utf8(request.audio_bytes).map_err(|_| {
            VoiceProviderError::invalid_input(provider, "audio bytes must decode as utf-8 text")
        })?;
        let trimmed = transcript.trim();
        if trimmed.is_empty() {
            return Err(VoiceProviderError::invalid_input(
                provider,
                "transcript is empty after normalization",
            ));
        }

        Ok(SttResponse {
            transcript: trimmed.to_string(),
            confidence: Some(1.0),
            language: request.locale.clone(),
            provider_metadata: json!({
                "adapter": provider,
                "sample_rate_hz": request.sample_rate_hz,
            }),
        })
    }
}

#[async_trait]
impl TtsProvider for DeterministicVoiceProvider {
    fn provider_name(&self) -> &'static str {
        Self::PROVIDER_NAME
    }

    async fn synthesize(&self, request: TtsRequest) -> VoiceProviderResult<TtsResponse> {
        let provider = Self::PROVIDER_NAME;
        let text = request.text.trim();
        if text.is_empty() {
            return Err(VoiceProviderError::invalid_input(
                provider,
                "text must not be empty",
            ));
        }

        let voice_id = request.voice_id.as_deref().unwrap_or("default");
        let rendered = format!("voice={voice_id};text={text}");
        Ok(TtsResponse {
            audio_bytes: rendered.into_bytes(),
            mime_type: request.mime_type,
            duration_ms: Some(0),
            provider_metadata: json!({
                "adapter": provider,
                "voice_id": voice_id,
            }),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `HttpVoiceProviderConfig` used across Tau components.
pub struct HttpVoiceProviderConfig {
    pub provider_name: String,
    pub api_base: String,
    pub api_key: Option<String>,
    pub stt_path: String,
    pub tts_path: String,
    pub timeout_ms: u64,
}

impl Default for HttpVoiceProviderConfig {
    fn default() -> Self {
        Self {
            provider_name: "http-voice".to_string(),
            api_base: String::new(),
            api_key: None,
            stt_path: "/stt".to_string(),
            tts_path: "/tts".to_string(),
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone)]
/// Public struct `HttpVoiceProvider` used across Tau components.
pub struct HttpVoiceProvider {
    config: HttpVoiceProviderConfig,
    client: Client,
}

impl HttpVoiceProvider {
    pub fn new(config: HttpVoiceProviderConfig) -> VoiceProviderResult<Self> {
        if config.provider_name.trim().is_empty() {
            return Err(VoiceProviderError::invalid_input(
                "http-voice",
                "provider_name must not be empty",
            ));
        }
        if config.api_base.trim().is_empty() {
            return Err(VoiceProviderError::invalid_input(
                config.provider_name.trim(),
                "api_base must not be empty",
            ));
        }
        if config.stt_path.trim().is_empty() || config.tts_path.trim().is_empty() {
            return Err(VoiceProviderError::invalid_input(
                config.provider_name.trim(),
                "stt_path and tts_path must not be empty",
            ));
        }

        let client = Client::builder().build().map_err(|error| {
            VoiceProviderError::backend_unavailable(
                config.provider_name.trim(),
                format!("failed to initialize http client: {error}"),
            )
        })?;

        let mut normalized = config;
        normalized.api_base = normalized.api_base.trim().trim_end_matches('/').to_string();
        normalized.timeout_ms = normalized.timeout_ms.max(1);

        Ok(Self {
            config: normalized,
            client,
        })
    }

    fn provider_name_internal(&self) -> &str {
        self.config.provider_name.trim()
    }

    fn endpoint_url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.config.api_base, path)
        } else {
            format!("{}/{}", self.config.api_base, path)
        }
    }

    fn authorize(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let Some(api_key) = self.config.api_key.as_deref() else {
            return builder;
        };
        if api_key.trim().is_empty() {
            builder
        } else {
            builder.bearer_auth(api_key.trim())
        }
    }

    async fn parse_json_response(
        &self,
        operation: &str,
        response: reqwest::Response,
    ) -> VoiceProviderResult<Value> {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(map_http_status_error(
                self.provider_name_internal(),
                operation,
                status,
                &body,
            ));
        }
        serde_json::from_str(&body).map_err(|error| {
            VoiceProviderError::invalid_response(
                self.provider_name_internal(),
                format!("operation={operation} invalid json response: {error}"),
            )
        })
    }

    fn map_request_error(&self, operation: &str, error: reqwest::Error) -> VoiceProviderError {
        if error.is_timeout() {
            return VoiceProviderError::timeout(
                self.provider_name_internal(),
                format!("operation={operation} request timed out"),
            );
        }
        VoiceProviderError::backend_unavailable(
            self.provider_name_internal(),
            format!("operation={operation} request failed: {error}"),
        )
    }
}

#[async_trait]
impl SttProvider for HttpVoiceProvider {
    fn provider_name(&self) -> &'static str {
        // This provider can be configured by user; use static label on the trait API.
        "http-voice"
    }

    async fn transcribe(&self, request: SttRequest) -> VoiceProviderResult<SttResponse> {
        if request.audio_bytes.is_empty() {
            return Err(VoiceProviderError::invalid_input(
                self.provider_name_internal(),
                "audio_bytes must not be empty",
            ));
        }

        let timeout_ms = request.timeout_ms.max(1);
        let payload = json!({
            "audio_base64": BASE64_STANDARD.encode(request.audio_bytes),
            "mime_type": request.mime_type,
            "locale": request.locale,
            "sample_rate_hz": request.sample_rate_hz,
        });

        let response = self
            .authorize(
                self.client
                    .post(self.endpoint_url(self.config.stt_path.as_str()))
                    .timeout(Duration::from_millis(timeout_ms))
                    .json(&payload),
            )
            .send()
            .await
            .map_err(|error| self.map_request_error("stt", error))?;

        let parsed = self.parse_json_response("stt", response).await?;
        let transcript = parsed
            .get("transcript")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();

        if transcript.is_empty() {
            return Err(VoiceProviderError::invalid_response(
                self.provider_name_internal(),
                "operation=stt missing transcript field",
            ));
        }

        Ok(SttResponse {
            transcript,
            confidence: parsed
                .get("confidence")
                .and_then(Value::as_f64)
                .map(|value| value as f32),
            language: parsed
                .get("language")
                .and_then(Value::as_str)
                .map(str::to_string),
            provider_metadata: parsed,
        })
    }
}

#[async_trait]
impl TtsProvider for HttpVoiceProvider {
    fn provider_name(&self) -> &'static str {
        "http-voice"
    }

    async fn synthesize(&self, request: TtsRequest) -> VoiceProviderResult<TtsResponse> {
        let text = request.text.trim();
        if text.is_empty() {
            return Err(VoiceProviderError::invalid_input(
                self.provider_name_internal(),
                "text must not be empty",
            ));
        }

        let timeout_ms = request.timeout_ms.max(1);
        let payload = json!({
            "text": text,
            "voice_id": request.voice_id,
            "locale": request.locale,
            "mime_type": request.mime_type,
        });

        let response = self
            .authorize(
                self.client
                    .post(self.endpoint_url(self.config.tts_path.as_str()))
                    .timeout(Duration::from_millis(timeout_ms))
                    .json(&payload),
            )
            .send()
            .await
            .map_err(|error| self.map_request_error("tts", error))?;
        let parsed = self.parse_json_response("tts", response).await?;

        let audio_base64 = parsed
            .get("audio_base64")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if audio_base64.trim().is_empty() {
            return Err(VoiceProviderError::invalid_response(
                self.provider_name_internal(),
                "operation=tts missing audio_base64 field",
            ));
        }

        let audio_bytes = BASE64_STANDARD.decode(audio_base64).map_err(|error| {
            VoiceProviderError::invalid_response(
                self.provider_name_internal(),
                format!("operation=tts audio_base64 decode failed: {error}"),
            )
        })?;

        Ok(TtsResponse {
            audio_bytes,
            mime_type: parsed
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("audio/wav")
                .to_string(),
            duration_ms: parsed.get("duration_ms").and_then(Value::as_u64),
            provider_metadata: parsed,
        })
    }
}

fn map_http_status_error(
    provider: &str,
    operation: &str,
    status: StatusCode,
    body: &str,
) -> VoiceProviderError {
    let message = format!(
        "operation={operation} status={} body={}",
        status.as_u16(),
        truncate_error_body(body)
    );

    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return VoiceProviderError {
            code: VoiceProviderErrorCode::AuthFailed,
            provider: provider.to_string(),
            retryable: false,
            message,
        };
    }
    if status == StatusCode::REQUEST_TIMEOUT || status == StatusCode::GATEWAY_TIMEOUT {
        return VoiceProviderError {
            code: VoiceProviderErrorCode::Timeout,
            provider: provider.to_string(),
            retryable: true,
            message,
        };
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return VoiceProviderError {
            code: VoiceProviderErrorCode::RateLimited,
            provider: provider.to_string(),
            retryable: true,
            message,
        };
    }
    if status.is_server_error() {
        return VoiceProviderError {
            code: VoiceProviderErrorCode::BackendUnavailable,
            provider: provider.to_string(),
            retryable: true,
            message,
        };
    }
    if status.is_client_error() {
        return VoiceProviderError {
            code: VoiceProviderErrorCode::InvalidInput,
            provider: provider.to_string(),
            retryable: false,
            message,
        };
    }
    VoiceProviderError {
        code: VoiceProviderErrorCode::Unknown,
        provider: provider.to_string(),
        retryable: false,
        message,
    }
}

fn truncate_error_body(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    if trimmed.chars().count() <= MAX_ERROR_BODY_CHARS {
        return trimmed.to_string();
    }
    let truncated = trimmed
        .chars()
        .take(MAX_ERROR_BODY_CHARS)
        .collect::<String>();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::{
        map_http_status_error, DeterministicVoiceProvider, HttpVoiceProvider,
        HttpVoiceProviderConfig, SttProvider, SttRequest, TtsProvider, TtsRequest,
        VoiceProviderErrorCode,
    };
    use reqwest::StatusCode;

    #[tokio::test]
    async fn functional_deterministic_stt_round_trips_transcript_payload() {
        let provider = DeterministicVoiceProvider;
        let mut request = SttRequest::new(b"tau open dashboard".to_vec());
        request.locale = Some("en-US".to_string());

        let response = provider.transcribe(request).await.expect("stt");
        assert_eq!(response.transcript, "tau open dashboard");
        assert_eq!(response.language.as_deref(), Some("en-US"));
    }

    #[tokio::test]
    async fn regression_deterministic_stt_rejects_empty_audio() {
        let provider = DeterministicVoiceProvider;
        let request = SttRequest::new(Vec::new());
        let error = provider
            .transcribe(request)
            .await
            .expect_err("expected error");
        assert_eq!(error.code, VoiceProviderErrorCode::InvalidInput);
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn functional_deterministic_tts_renders_audio_payload() {
        let provider = DeterministicVoiceProvider;
        let response = provider
            .synthesize(TtsRequest::new("deploy release"))
            .await
            .expect("tts");
        let rendered = String::from_utf8(response.audio_bytes).expect("utf8");
        assert!(rendered.contains("deploy release"));
    }

    #[tokio::test]
    async fn regression_deterministic_tts_rejects_empty_text() {
        let provider = DeterministicVoiceProvider;
        let mut request = TtsRequest::new("");
        request.text = "   ".to_string();
        let error = provider
            .synthesize(request)
            .await
            .expect_err("expected error");
        assert_eq!(error.code, VoiceProviderErrorCode::InvalidInput);
        assert!(!error.retryable);
    }

    #[test]
    fn unit_http_provider_rejects_empty_api_base() {
        let config = HttpVoiceProviderConfig::default();
        let error = HttpVoiceProvider::new(config).expect_err("expected config error");
        assert_eq!(error.code, VoiceProviderErrorCode::InvalidInput);
    }

    #[test]
    fn unit_http_error_mapping_marks_retryable_statuses() {
        let timeout = map_http_status_error("http", "stt", StatusCode::REQUEST_TIMEOUT, "");
        assert_eq!(timeout.code, VoiceProviderErrorCode::Timeout);
        assert!(timeout.retryable);

        let ratelimit = map_http_status_error("http", "stt", StatusCode::TOO_MANY_REQUESTS, "");
        assert_eq!(ratelimit.code, VoiceProviderErrorCode::RateLimited);
        assert!(ratelimit.retryable);

        let auth = map_http_status_error("http", "stt", StatusCode::UNAUTHORIZED, "");
        assert_eq!(auth.code, VoiceProviderErrorCode::AuthFailed);
        assert!(!auth.retryable);
    }
}
