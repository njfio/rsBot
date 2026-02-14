use std::collections::{BTreeMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::multi_channel_contract::{event_contract_key, MultiChannelTransport};
use crate::multi_channel_live_ingress::{
    build_multi_channel_live_envelope_from_raw_payload, default_multi_channel_live_provider_label,
    parse_multi_channel_live_inbound_envelope_value, MultiChannelLiveInboundEnvelope,
};
use tau_core::current_unix_timestamp_ms;

const LIVE_CONNECTORS_SCHEMA_VERSION: u32 = 1;
const MAX_POLL_BATCH_SIZE: usize = 50;
const CONNECTOR_BREAKER_STATE_CLOSED: &str = "closed";
const CONNECTOR_BREAKER_STATE_OPEN: &str = "open";
const CONNECTOR_BREAKER_STATE_HALF_OPEN: &str = "half_open";
const CONNECTOR_BREAKER_STATE_DISABLED: &str = "disabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelLiveConnectorErrorCode` values.
pub enum MultiChannelLiveConnectorErrorCode {
    MissingConfig,
    AuthFailed,
    RateLimited,
    ProviderUnavailable,
    TransportError,
    ParseFailed,
    InvalidSignature,
    InvalidWebhookVerification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `MultiChannelLiveConnectorMode` values.
pub enum MultiChannelLiveConnectorMode {
    Disabled,
    Polling,
    Webhook,
}

impl MultiChannelLiveConnectorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Polling => "polling",
            Self::Webhook => "webhook",
        }
    }

    pub fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled)
    }

    pub fn is_polling(self) -> bool {
        matches!(self, Self::Polling)
    }

    pub fn is_webhook(self) -> bool {
        matches!(self, Self::Webhook)
    }
}

impl MultiChannelLiveConnectorErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingConfig => "missing_config",
            Self::AuthFailed => "auth_failed",
            Self::RateLimited => "rate_limited",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::TransportError => "transport_error",
            Self::ParseFailed => "parse_failed",
            Self::InvalidSignature => "invalid_signature",
            Self::InvalidWebhookVerification => "invalid_webhook_verification",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Public struct `MultiChannelLiveConnectorChannelState` used across Tau components.
pub struct MultiChannelLiveConnectorChannelState {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub liveness: String,
    #[serde(default)]
    pub events_ingested: u64,
    #[serde(default)]
    pub duplicates_skipped: u64,
    #[serde(default)]
    pub retry_attempts: u64,
    #[serde(default)]
    pub auth_failures: u64,
    #[serde(default)]
    pub parse_failures: u64,
    #[serde(default)]
    pub provider_failures: u64,
    #[serde(default)]
    pub consecutive_failures: u64,
    #[serde(default)]
    pub retry_budget_remaining: u64,
    #[serde(default)]
    pub breaker_state: String,
    #[serde(default)]
    pub breaker_open_until_unix_ms: u64,
    #[serde(default)]
    pub breaker_last_open_reason: String,
    #[serde(default)]
    pub breaker_open_count: u64,
    #[serde(default)]
    pub last_error_code: String,
    #[serde(default)]
    pub last_error_message: String,
    #[serde(default)]
    pub last_success_unix_ms: u64,
    #[serde(default)]
    pub last_error_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `MultiChannelLiveConnectorStateFile` used across Tau components.
pub struct MultiChannelLiveConnectorStateFile {
    #[serde(default = "multi_channel_live_connectors_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub processed_event_keys: Vec<String>,
    #[serde(default)]
    pub telegram_next_update_offset: Option<u64>,
    #[serde(default)]
    pub discord_last_message_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub channels: BTreeMap<String, MultiChannelLiveConnectorChannelState>,
}

impl Default for MultiChannelLiveConnectorStateFile {
    fn default() -> Self {
        Self {
            schema_version: LIVE_CONNECTORS_SCHEMA_VERSION,
            processed_event_keys: Vec::new(),
            telegram_next_update_offset: None,
            discord_last_message_ids: BTreeMap::new(),
            channels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelLiveConnectorsStatusReport` used across Tau components.
pub struct MultiChannelLiveConnectorsStatusReport {
    pub state_path: String,
    pub state_present: bool,
    pub schema_version: u32,
    pub processed_event_count: usize,
    pub channels: BTreeMap<String, MultiChannelLiveConnectorChannelState>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
/// Public struct `MultiChannelLiveConnectorCycleSummary` used across Tau components.
pub struct MultiChannelLiveConnectorCycleSummary {
    pub ingested_events: u64,
    pub duplicate_events: u64,
    pub retry_attempts: u64,
    pub auth_failures: u64,
    pub parse_failures: u64,
    pub provider_failures: u64,
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelLiveConnectorsConfig` used across Tau components.
pub struct MultiChannelLiveConnectorsConfig {
    pub state_path: PathBuf,
    pub ingress_dir: PathBuf,
    pub processed_event_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub poll_once: bool,
    pub webhook_bind: String,
    pub telegram_mode: MultiChannelLiveConnectorMode,
    pub telegram_api_base: String,
    pub telegram_bot_token: Option<String>,
    pub telegram_webhook_secret: Option<String>,
    pub discord_mode: MultiChannelLiveConnectorMode,
    pub discord_api_base: String,
    pub discord_bot_token: Option<String>,
    pub discord_ingress_channel_ids: Vec<String>,
    pub whatsapp_mode: MultiChannelLiveConnectorMode,
    pub whatsapp_webhook_verify_token: Option<String>,
    pub whatsapp_webhook_app_secret: Option<String>,
}

#[derive(Debug, Clone)]
struct LiveConnectorServerState {
    config: MultiChannelLiveConnectorsConfig,
    state: Arc<Mutex<MultiChannelLiveConnectorStateFile>>,
}

#[derive(Debug, Deserialize)]
struct WhatsAppVerifyQuery {
    #[serde(rename = "hub.mode")]
    hub_mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    hub_verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    hub_challenge: Option<String>,
}

#[derive(Debug, Clone)]
struct ConnectorError {
    code: MultiChannelLiveConnectorErrorCode,
    message: String,
    retryable: bool,
}

impl ConnectorError {
    fn new(
        code: MultiChannelLiveConnectorErrorCode,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

fn multi_channel_live_connectors_schema_version() -> u32 {
    LIVE_CONNECTORS_SCHEMA_VERSION
}

pub fn load_multi_channel_live_connectors_status_report(
    state_path: &Path,
) -> Result<MultiChannelLiveConnectorsStatusReport> {
    if !state_path.exists() {
        return Ok(MultiChannelLiveConnectorsStatusReport {
            state_path: state_path.display().to_string(),
            state_present: false,
            schema_version: LIVE_CONNECTORS_SCHEMA_VERSION,
            processed_event_count: 0,
            channels: BTreeMap::new(),
        });
    }
    let state = load_multi_channel_live_connectors_state(state_path)?;
    Ok(MultiChannelLiveConnectorsStatusReport {
        state_path: state_path.display().to_string(),
        state_present: true,
        schema_version: state.schema_version,
        processed_event_count: state.processed_event_keys.len(),
        channels: state.channels,
    })
}

pub async fn run_multi_channel_live_connectors_runner(
    config: MultiChannelLiveConnectorsConfig,
) -> Result<()> {
    create_parent_dir_if_needed(&config.state_path)?;
    std::fs::create_dir_all(&config.ingress_dir).with_context(|| {
        format!(
            "failed to create live ingress directory {}",
            config.ingress_dir.display()
        )
    })?;

    let mut state = load_multi_channel_live_connectors_state(&config.state_path)?;
    initialize_channel_modes(&config, &mut state);
    let client = Client::new();
    let summary = run_multi_channel_live_connectors_poll_cycle(&config, &client, &mut state).await;
    save_multi_channel_live_connectors_state(&config.state_path, &state)?;

    let has_webhook_mode = config.telegram_mode.is_webhook() || config.whatsapp_mode.is_webhook();
    if !has_webhook_mode {
        println!(
            "multi-channel live connectors summary: ingested_events={} duplicate_events={} retries={} auth_failures={} parse_failures={} provider_failures={}",
            summary.ingested_events,
            summary.duplicate_events,
            summary.retry_attempts,
            summary.auth_failures,
            summary.parse_failures,
            summary.provider_failures
        );
        return Ok(());
    }
    if config.poll_once {
        bail!(
            "--multi-channel-live-connectors-poll-once cannot be used when webhook connector modes are enabled"
        );
    }

    let state = Arc::new(Mutex::new(state));
    let server_state = Arc::new(LiveConnectorServerState {
        config: config.clone(),
        state: state.clone(),
    });
    let listener = TcpListener::bind(config.webhook_bind.as_str())
        .await
        .with_context(|| format!("failed to bind {}", config.webhook_bind))?;
    let local_addr = listener
        .local_addr()
        .context("failed to resolve live webhook bound address")?;
    println!(
        "multi-channel live webhook server listening: addr={} telegram_mode={} whatsapp_mode={}",
        local_addr,
        config.telegram_mode.as_str(),
        config.whatsapp_mode.as_str()
    );

    let app = build_multi_channel_live_webhook_router(server_state.clone());
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .context("multi-channel live webhook server exited unexpectedly")?;

    let final_state = state.lock().await.clone();
    save_multi_channel_live_connectors_state(&config.state_path, &final_state)?;
    Ok(())
}

async fn run_multi_channel_live_connectors_poll_cycle(
    config: &MultiChannelLiveConnectorsConfig,
    client: &Client,
    state: &mut MultiChannelLiveConnectorStateFile,
) -> MultiChannelLiveConnectorCycleSummary {
    let mut summary = MultiChannelLiveConnectorCycleSummary::default();
    if config.telegram_mode.is_polling() {
        let _ = poll_telegram_updates(config, client, state, &mut summary).await;
    }
    if config.discord_mode.is_polling() {
        let _ = poll_discord_messages(config, client, state, &mut summary).await;
    }
    update_channel_liveness(state);
    summary
}

fn build_multi_channel_live_webhook_router(state: Arc<LiveConnectorServerState>) -> Router {
    Router::new()
        .route("/webhooks/telegram", post(handle_telegram_webhook))
        .route(
            "/webhooks/whatsapp",
            get(handle_whatsapp_webhook_verify).post(handle_whatsapp_webhook),
        )
        .route("/healthz", get(handle_webhook_health))
        .with_state(state)
}

async fn handle_webhook_health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status":"ok"})))
}

async fn handle_telegram_webhook(
    State(state): State<Arc<LiveConnectorServerState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if !state.config.telegram_mode.is_webhook() {
        return (
            StatusCode::NOT_FOUND,
            Json(
                json!({"error":{"code":"connector_disabled","message":"telegram webhook connector mode is disabled"}}),
            ),
        );
    }
    if let Some(expected_secret) = state.config.telegram_webhook_secret.as_deref() {
        let observed = headers
            .get("x-telegram-bot-api-secret-token")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if observed != expected_secret.trim() {
            let mut guard = state.state.lock().await;
            record_channel_error(
                &state.config,
                &mut guard,
                "telegram",
                MultiChannelLiveConnectorErrorCode::AuthFailed,
                "telegram webhook secret mismatch",
                false,
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(
                    json!({"error":{"code":"auth_failed","message":"invalid telegram webhook secret"}}),
                ),
            );
        }
    }

    let result = {
        let mut guard = state.state.lock().await;
        ingest_raw_payload(
            &state.config,
            &mut guard,
            MultiChannelTransport::Telegram,
            default_multi_channel_live_provider_label(MultiChannelTransport::Telegram),
            body.as_str(),
        )
    };
    match result {
        Ok((event_key, duplicate)) => (
            StatusCode::OK,
            Json(json!({"status":"accepted","event_key":event_key,"duplicate":duplicate})),
        ),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":{"code":error.code.as_str(),"message":error.message}})),
        ),
    }
}

async fn handle_whatsapp_webhook_verify(
    State(state): State<Arc<LiveConnectorServerState>>,
    Query(query): Query<WhatsAppVerifyQuery>,
) -> impl IntoResponse {
    if !state.config.whatsapp_mode.is_webhook() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error":{"code":"connector_disabled","message":"whatsapp webhook connector mode is disabled"}})),
        )
            .into_response();
    }
    let expected_verify_token = state
        .config
        .whatsapp_webhook_verify_token
        .as_deref()
        .map(str::trim)
        .unwrap_or("");
    let observed_verify_token = query
        .hub_verify_token
        .as_deref()
        .map(str::trim)
        .unwrap_or("");
    let challenge = query.hub_challenge.unwrap_or_default();
    let mode = query.hub_mode.unwrap_or_default();

    if mode == "subscribe"
        && !expected_verify_token.is_empty()
        && observed_verify_token == expected_verify_token
    {
        return (StatusCode::OK, challenge).into_response();
    }

    let mut guard = state.state.lock().await;
    record_channel_error(
        &state.config,
        &mut guard,
        "whatsapp",
        MultiChannelLiveConnectorErrorCode::InvalidWebhookVerification,
        "whatsapp webhook verification failed",
        false,
    );
    (
        StatusCode::FORBIDDEN,
        Json(json!({"error":{"code":"invalid_webhook_verification","message":"whatsapp webhook verification failed"}})),
    )
        .into_response()
}

async fn handle_whatsapp_webhook(
    State(state): State<Arc<LiveConnectorServerState>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if !state.config.whatsapp_mode.is_webhook() {
        return (
            StatusCode::NOT_FOUND,
            Json(
                json!({"error":{"code":"connector_disabled","message":"whatsapp webhook connector mode is disabled"}}),
            ),
        );
    }

    if let Some(app_secret) = state.config.whatsapp_webhook_app_secret.as_deref() {
        let signature = headers
            .get("x-hub-signature-256")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if verify_sha256_hmac_signature(body.as_bytes(), signature, app_secret).is_err() {
            let mut guard = state.state.lock().await;
            record_channel_error(
                &state.config,
                &mut guard,
                "whatsapp",
                MultiChannelLiveConnectorErrorCode::InvalidSignature,
                "whatsapp signature verification failed",
                false,
            );
            return (
                StatusCode::UNAUTHORIZED,
                Json(
                    json!({"error":{"code":"invalid_signature","message":"whatsapp webhook signature verification failed"}}),
                ),
            );
        }
    }

    let payload = match serde_json::from_str::<Value>(body.as_str()) {
        Ok(payload) => payload,
        Err(error) => {
            let mut guard = state.state.lock().await;
            record_channel_error(
                &state.config,
                &mut guard,
                "whatsapp",
                MultiChannelLiveConnectorErrorCode::ParseFailed,
                format!("invalid whatsapp webhook payload json: {error}"),
                false,
            );
            return (
                StatusCode::BAD_REQUEST,
                Json(
                    json!({"error":{"code":"parse_failed","message":"invalid whatsapp webhook payload"}}),
                ),
            );
        }
    };
    let value_objects = extract_whatsapp_value_objects(&payload);
    if value_objects.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                json!({"error":{"code":"parse_failed","message":"whatsapp webhook payload did not contain entry[].changes[].value objects"}}),
            ),
        );
    }

    let mut ingested = 0u64;
    let mut duplicates = 0u64;
    let mut guard = state.state.lock().await;
    for value_object in value_objects {
        let raw = match serde_json::to_string(&value_object) {
            Ok(raw) => raw,
            Err(error) => {
                record_channel_error(
                    &state.config,
                    &mut guard,
                    "whatsapp",
                    MultiChannelLiveConnectorErrorCode::ParseFailed,
                    format!("failed to encode whatsapp value object: {error}"),
                    false,
                );
                continue;
            }
        };
        match ingest_raw_payload(
            &state.config,
            &mut guard,
            MultiChannelTransport::Whatsapp,
            default_multi_channel_live_provider_label(MultiChannelTransport::Whatsapp),
            raw.as_str(),
        ) {
            Ok((_, duplicate)) => {
                if duplicate {
                    duplicates = duplicates.saturating_add(1);
                } else {
                    ingested = ingested.saturating_add(1);
                }
            }
            Err(error) => {
                record_channel_error(
                    &state.config,
                    &mut guard,
                    "whatsapp",
                    error.code,
                    error.message,
                    false,
                );
            }
        }
    }
    save_multi_channel_live_connectors_state(&state.config.state_path, &guard).ok();
    (
        StatusCode::OK,
        Json(json!({"status":"accepted","ingested":ingested,"duplicates":duplicates})),
    )
}

async fn poll_telegram_updates(
    config: &MultiChannelLiveConnectorsConfig,
    client: &Client,
    state: &mut MultiChannelLiveConnectorStateFile,
    summary: &mut MultiChannelLiveConnectorCycleSummary,
) -> Result<()> {
    if !begin_channel_poll(config, state, "telegram") {
        return Ok(());
    }
    let token = config
        .telegram_bot_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing telegram bot token for polling mode"))?;
    let base = config.telegram_api_base.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("telegram api base cannot be empty");
    }
    let offset = state.telegram_next_update_offset.unwrap_or(0);
    let url = format!("{base}/bot{token}/getUpdates");
    let response = request_json_with_retry(
        config.retry_max_attempts,
        config.retry_base_delay_ms,
        summary,
        "telegram",
        || {
            client
                .get(url.as_str())
                .query(&[("timeout", "0"), ("offset", offset.to_string().as_str())])
        },
    )
    .await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            record_channel_error(
                config,
                state,
                "telegram",
                error.code,
                error.message,
                error.retryable,
            );
            return Ok(());
        }
    };

    let updates = response
        .get("result")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("telegram getUpdates response missing result[]"))?;
    let mut max_update_id = state.telegram_next_update_offset.unwrap_or(0);
    for update in updates {
        let update_id = update.get("update_id").and_then(Value::as_u64).unwrap_or(0);
        if update_id > 0 {
            max_update_id = max_update_id.max(update_id.saturating_add(1));
        }
        let raw = serde_json::to_string(update).context("encode telegram update payload")?;
        match ingest_raw_payload(
            config,
            state,
            MultiChannelTransport::Telegram,
            default_multi_channel_live_provider_label(MultiChannelTransport::Telegram),
            raw.as_str(),
        ) {
            Ok((_, duplicate)) => {
                if duplicate {
                    summary.duplicate_events = summary.duplicate_events.saturating_add(1);
                } else {
                    summary.ingested_events = summary.ingested_events.saturating_add(1);
                }
            }
            Err(error) => {
                record_channel_error(config, state, "telegram", error.code, error.message, false);
                summary.parse_failures = summary.parse_failures.saturating_add(1);
            }
        }
    }
    if max_update_id > 0 {
        state.telegram_next_update_offset = Some(max_update_id);
    }
    record_channel_success(config, state, "telegram");
    Ok(())
}

async fn poll_discord_messages(
    config: &MultiChannelLiveConnectorsConfig,
    client: &Client,
    state: &mut MultiChannelLiveConnectorStateFile,
    summary: &mut MultiChannelLiveConnectorCycleSummary,
) -> Result<()> {
    if !begin_channel_poll(config, state, "discord") {
        return Ok(());
    }
    let token = config
        .discord_bot_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing discord bot token for polling mode"))?;
    if config.discord_ingress_channel_ids.is_empty() {
        bail!("discord polling mode requires at least one channel id");
    }
    let base = config.discord_api_base.trim().trim_end_matches('/');
    if base.is_empty() {
        bail!("discord api base cannot be empty");
    }

    let auth_header = format!("Bot {token}");
    for channel_id in &config.discord_ingress_channel_ids {
        if channel_id.trim().is_empty() {
            continue;
        }
        let channel_id = channel_id.trim().to_string();
        let url = format!("{base}/channels/{channel_id}/messages");
        let response = request_json_with_retry(
            config.retry_max_attempts,
            config.retry_base_delay_ms,
            summary,
            "discord",
            || {
                client
                    .get(url.as_str())
                    .query(&[("limit", MAX_POLL_BATCH_SIZE.to_string().as_str())])
                    .header("authorization", auth_header.as_str())
            },
        )
        .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                record_channel_error(
                    config,
                    state,
                    "discord",
                    error.code,
                    error.message,
                    error.retryable,
                );
                continue;
            }
        };

        let mut messages = response
            .as_array()
            .cloned()
            .ok_or_else(|| anyhow!("discord messages response must be a JSON array"))?;
        messages.sort_by(|left, right| {
            let left_id = left.get("id").and_then(Value::as_str).unwrap_or_default();
            let right_id = right.get("id").and_then(Value::as_str).unwrap_or_default();
            compare_discord_message_ids(left_id, right_id)
        });
        let previous_id = state
            .discord_last_message_ids
            .get(channel_id.as_str())
            .cloned();
        let mut latest_seen = previous_id.clone().unwrap_or_default();
        for message in &messages {
            let message_id = message
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if !is_newer_discord_message(message_id.as_str(), previous_id.as_deref()) {
                continue;
            }
            let raw = serde_json::to_string(message).context("encode discord message payload")?;
            match ingest_raw_payload(
                config,
                state,
                MultiChannelTransport::Discord,
                default_multi_channel_live_provider_label(MultiChannelTransport::Discord),
                raw.as_str(),
            ) {
                Ok((_, duplicate)) => {
                    if duplicate {
                        summary.duplicate_events = summary.duplicate_events.saturating_add(1);
                    } else {
                        summary.ingested_events = summary.ingested_events.saturating_add(1);
                    }
                }
                Err(error) => {
                    record_channel_error(
                        config,
                        state,
                        "discord",
                        error.code,
                        error.message,
                        false,
                    );
                    summary.parse_failures = summary.parse_failures.saturating_add(1);
                }
            }
            if is_newer_discord_message(message_id.as_str(), Some(latest_seen.as_str())) {
                latest_seen = message_id;
            }
        }
        if !latest_seen.trim().is_empty() {
            state
                .discord_last_message_ids
                .insert(channel_id.clone(), latest_seen);
        }
        record_channel_success(config, state, "discord");
    }
    Ok(())
}

async fn request_json_with_retry<F>(
    retry_max_attempts: usize,
    retry_base_delay_ms: u64,
    summary: &mut MultiChannelLiveConnectorCycleSummary,
    channel: &str,
    build_request: F,
) -> Result<Value, ConnectorError>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    let max_attempts = retry_max_attempts.max(1);
    let mut attempt = 0usize;
    loop {
        attempt = attempt.saturating_add(1);
        let response = build_request().send().await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                let retryable = attempt < max_attempts;
                if retryable {
                    summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                    sleep_retry_backoff(retry_base_delay_ms, attempt).await;
                    continue;
                }
                summary.provider_failures = summary.provider_failures.saturating_add(1);
                return Err(ConnectorError::new(
                    MultiChannelLiveConnectorErrorCode::TransportError,
                    format!("{channel} transport error: {error}"),
                    true,
                ));
            }
        };

        let status = response.status();
        if status.is_success() {
            let parsed = response.json::<Value>().await.map_err(|error| {
                ConnectorError::new(
                    MultiChannelLiveConnectorErrorCode::ParseFailed,
                    format!("{channel} response parse error: {error}"),
                    false,
                )
            })?;
            return Ok(parsed);
        }

        let code = if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            MultiChannelLiveConnectorErrorCode::AuthFailed
        } else if status == StatusCode::TOO_MANY_REQUESTS {
            MultiChannelLiveConnectorErrorCode::RateLimited
        } else {
            MultiChannelLiveConnectorErrorCode::ProviderUnavailable
        };
        let retryable = (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
            && attempt < max_attempts;
        if retryable {
            summary.retry_attempts = summary.retry_attempts.saturating_add(1);
            sleep_retry_backoff(retry_base_delay_ms, attempt).await;
            continue;
        }
        if matches!(code, MultiChannelLiveConnectorErrorCode::AuthFailed) {
            summary.auth_failures = summary.auth_failures.saturating_add(1);
        } else {
            summary.provider_failures = summary.provider_failures.saturating_add(1);
        }
        return Err(ConnectorError::new(
            code,
            format!("{channel} request failed with status {}", status.as_u16()),
            matches!(
                code,
                MultiChannelLiveConnectorErrorCode::RateLimited
                    | MultiChannelLiveConnectorErrorCode::ProviderUnavailable
            ),
        ));
    }
}

async fn sleep_retry_backoff(retry_base_delay_ms: u64, attempt: usize) {
    if retry_base_delay_ms == 0 {
        return;
    }
    let delay_ms = retry_base_delay_ms.saturating_mul(u64::try_from(attempt).unwrap_or(1));
    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

fn initialize_channel_modes(
    config: &MultiChannelLiveConnectorsConfig,
    state: &mut MultiChannelLiveConnectorStateFile,
) {
    for (channel, mode) in [
        ("telegram", config.telegram_mode),
        ("discord", config.discord_mode),
        ("whatsapp", config.whatsapp_mode),
    ] {
        let entry = state.channels.entry(channel.to_string()).or_default();
        entry.mode = mode.as_str().to_string();
        ensure_channel_resilience_state(config, entry, mode.as_str() == "disabled");
    }
    update_channel_liveness(state);
}

fn update_channel_liveness(state: &mut MultiChannelLiveConnectorStateFile) {
    let now_unix_ms = current_unix_timestamp_ms();
    for channel_state in state.channels.values_mut() {
        channel_state.liveness = if channel_state.mode == "disabled" {
            "disabled".to_string()
        } else if channel_state.breaker_state == CONNECTOR_BREAKER_STATE_OPEN {
            if channel_state.breaker_open_until_unix_ms > 0
                && now_unix_ms >= channel_state.breaker_open_until_unix_ms
            {
                "recovering".to_string()
            } else {
                "open".to_string()
            }
        } else if channel_state.breaker_state == CONNECTOR_BREAKER_STATE_HALF_OPEN {
            "recovering".to_string()
        } else if channel_state.last_success_unix_ms > 0
            && channel_state.last_success_unix_ms >= channel_state.last_error_unix_ms
        {
            "healthy".to_string()
        } else if channel_state.last_error_unix_ms > 0 {
            "degraded".to_string()
        } else {
            "idle".to_string()
        };
    }
}

fn connector_retry_budget_max(config: &MultiChannelLiveConnectorsConfig) -> u64 {
    u64::try_from(config.retry_max_attempts.max(1)).unwrap_or(u64::MAX)
}

fn connector_breaker_failure_threshold(config: &MultiChannelLiveConnectorsConfig) -> u64 {
    connector_retry_budget_max(config).max(2)
}

fn connector_breaker_cooldown_ms(config: &MultiChannelLiveConnectorsConfig) -> u64 {
    config.retry_base_delay_ms.saturating_mul(4).max(1_000)
}

fn ensure_channel_resilience_state(
    config: &MultiChannelLiveConnectorsConfig,
    entry: &mut MultiChannelLiveConnectorChannelState,
    disabled_mode: bool,
) {
    let budget_max = connector_retry_budget_max(config);
    if entry.retry_budget_remaining == 0 || entry.retry_budget_remaining > budget_max {
        entry.retry_budget_remaining = budget_max;
    }
    if disabled_mode {
        entry.breaker_state = CONNECTOR_BREAKER_STATE_DISABLED.to_string();
        entry.breaker_open_until_unix_ms = 0;
        entry.breaker_last_open_reason.clear();
        return;
    }
    if entry.breaker_state.trim().is_empty()
        || entry.breaker_state == CONNECTOR_BREAKER_STATE_DISABLED
    {
        entry.breaker_state = CONNECTOR_BREAKER_STATE_CLOSED.to_string();
    }
}

fn begin_channel_poll(
    config: &MultiChannelLiveConnectorsConfig,
    state: &mut MultiChannelLiveConnectorStateFile,
    channel: &str,
) -> bool {
    let now_unix_ms = current_unix_timestamp_ms();
    let entry = state.channels.entry(channel.to_string()).or_default();
    ensure_channel_resilience_state(config, entry, entry.mode == "disabled");
    if entry.breaker_state != CONNECTOR_BREAKER_STATE_OPEN {
        return true;
    }
    if entry.breaker_open_until_unix_ms > now_unix_ms {
        entry.last_error_unix_ms = now_unix_ms;
        entry.last_error_code = "circuit_open".to_string();
        entry.last_error_message = format!(
            "circuit breaker open until {}",
            entry.breaker_open_until_unix_ms
        );
        return false;
    }
    entry.breaker_state = CONNECTOR_BREAKER_STATE_HALF_OPEN.to_string();
    entry.retry_budget_remaining = 1;
    true
}

fn open_channel_breaker(
    config: &MultiChannelLiveConnectorsConfig,
    entry: &mut MultiChannelLiveConnectorChannelState,
    reason: &str,
) {
    let now_unix_ms = current_unix_timestamp_ms();
    entry.breaker_state = CONNECTOR_BREAKER_STATE_OPEN.to_string();
    entry.breaker_open_until_unix_ms =
        now_unix_ms.saturating_add(connector_breaker_cooldown_ms(config));
    entry.breaker_last_open_reason = reason.to_string();
    entry.breaker_open_count = entry.breaker_open_count.saturating_add(1);
}

fn record_channel_success(
    config: &MultiChannelLiveConnectorsConfig,
    state: &mut MultiChannelLiveConnectorStateFile,
    channel: &str,
) {
    let entry = state.channels.entry(channel.to_string()).or_default();
    ensure_channel_resilience_state(config, entry, entry.mode == "disabled");
    entry.last_success_unix_ms = current_unix_timestamp_ms();
    entry.consecutive_failures = 0;
    entry.retry_budget_remaining = connector_retry_budget_max(config);
    if entry.breaker_state != CONNECTOR_BREAKER_STATE_DISABLED {
        entry.breaker_state = CONNECTOR_BREAKER_STATE_CLOSED.to_string();
    }
    entry.breaker_open_until_unix_ms = 0;
}

fn record_channel_error(
    config: &MultiChannelLiveConnectorsConfig,
    state: &mut MultiChannelLiveConnectorStateFile,
    channel: &str,
    code: MultiChannelLiveConnectorErrorCode,
    message: impl Into<String>,
    retryable: bool,
) {
    let entry = state.channels.entry(channel.to_string()).or_default();
    ensure_channel_resilience_state(config, entry, entry.mode == "disabled");
    let message = message.into();
    entry.last_error_unix_ms = current_unix_timestamp_ms();
    entry.last_error_code = code.as_str().to_string();
    entry.last_error_message = message;
    entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
    if retryable {
        entry.retry_attempts = entry.retry_attempts.saturating_add(1);
        entry.retry_budget_remaining = entry.retry_budget_remaining.saturating_sub(1);
    }
    match code {
        MultiChannelLiveConnectorErrorCode::AuthFailed
        | MultiChannelLiveConnectorErrorCode::InvalidSignature
        | MultiChannelLiveConnectorErrorCode::InvalidWebhookVerification => {
            entry.auth_failures = entry.auth_failures.saturating_add(1);
        }
        MultiChannelLiveConnectorErrorCode::ParseFailed => {
            entry.parse_failures = entry.parse_failures.saturating_add(1);
        }
        _ => {
            entry.provider_failures = entry.provider_failures.saturating_add(1);
        }
    }

    if entry.breaker_state == CONNECTOR_BREAKER_STATE_DISABLED || !retryable {
        return;
    }
    let should_open_from_half_open = entry.breaker_state == CONNECTOR_BREAKER_STATE_HALF_OPEN;
    let should_open_from_budget = entry.consecutive_failures
        >= connector_breaker_failure_threshold(config)
        && entry.retry_budget_remaining == 0;
    if should_open_from_half_open || should_open_from_budget {
        open_channel_breaker(config, entry, code.as_str());
    }
}

fn ingest_raw_payload(
    config: &MultiChannelLiveConnectorsConfig,
    state: &mut MultiChannelLiveConnectorStateFile,
    transport: MultiChannelTransport,
    provider: &str,
    raw_payload: &str,
) -> Result<(String, bool), ConnectorError> {
    let envelope =
        build_multi_channel_live_envelope_from_raw_payload(transport, provider, raw_payload)
            .map_err(|error| {
                ConnectorError::new(
                    MultiChannelLiveConnectorErrorCode::ParseFailed,
                    format!(
                        "failed to normalize {} payload: reason_code={} detail={}",
                        transport.as_str(),
                        error.code.as_str(),
                        error.message
                    ),
                    false,
                )
            })?;
    ingest_envelope(config, state, &envelope)
}

fn ingest_envelope(
    config: &MultiChannelLiveConnectorsConfig,
    state: &mut MultiChannelLiveConnectorStateFile,
    envelope: &MultiChannelLiveInboundEnvelope,
) -> Result<(String, bool), ConnectorError> {
    let normalized =
        parse_multi_channel_live_inbound_envelope_value(envelope).map_err(|error| {
            ConnectorError::new(
                MultiChannelLiveConnectorErrorCode::ParseFailed,
                format!(
                    "failed to validate normalized payload: reason_code={} detail={}",
                    error.code.as_str(),
                    error.message
                ),
                false,
            )
        })?;
    let event_key = event_contract_key(&normalized);
    let existing: HashSet<&str> = state
        .processed_event_keys
        .iter()
        .map(String::as_str)
        .collect();
    let channel_key = normalized.transport.as_str().to_string();
    if existing.contains(event_key.as_str()) {
        let channel_state = state.channels.entry(channel_key).or_default();
        channel_state.duplicates_skipped = channel_state.duplicates_skipped.saturating_add(1);
        return Ok((event_key, true));
    }

    let transport_file = transport_file_name(normalized.transport);
    let ingress_path = config.ingress_dir.join(transport_file);
    let encoded = serde_json::to_string(envelope).map_err(|error| {
        ConnectorError::new(
            MultiChannelLiveConnectorErrorCode::ParseFailed,
            format!("failed to encode normalized envelope: {error}"),
            false,
        )
    })?;
    append_ndjson_line(&ingress_path, encoded.as_str()).map_err(|error| {
        ConnectorError::new(
            MultiChannelLiveConnectorErrorCode::TransportError,
            format!("failed to append {}: {error}", ingress_path.display()),
            false,
        )
    })?;

    state.processed_event_keys.push(event_key.clone());
    while state.processed_event_keys.len() > config.processed_event_cap.max(1) {
        state.processed_event_keys.remove(0);
    }
    let channel_state = state.channels.entry(channel_key).or_default();
    channel_state.events_ingested = channel_state.events_ingested.saturating_add(1);
    channel_state.last_success_unix_ms = current_unix_timestamp_ms();
    channel_state.consecutive_failures = 0;
    Ok((event_key, false))
}

fn append_ndjson_line(path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to append newline {}", path.display()))?;
    Ok(())
}

fn transport_file_name(transport: MultiChannelTransport) -> &'static str {
    match transport {
        MultiChannelTransport::Telegram => "telegram.ndjson",
        MultiChannelTransport::Discord => "discord.ndjson",
        MultiChannelTransport::Whatsapp => "whatsapp.ndjson",
    }
}

fn compare_discord_message_ids(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<u128>(), right.parse::<u128>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn is_newer_discord_message(candidate: &str, previous: Option<&str>) -> bool {
    let Some(previous) = previous else {
        return !candidate.trim().is_empty();
    };
    compare_discord_message_ids(candidate.trim(), previous.trim()).is_gt()
}

fn verify_sha256_hmac_signature(
    payload: &[u8],
    signature_header: &str,
    secret: &str,
) -> Result<()> {
    let digest_hex = signature_header
        .strip_prefix("sha256=")
        .ok_or_else(|| anyhow!("signature must use sha256=<hex> format"))?;
    let signature_bytes = decode_hex(digest_hex)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .context("failed to initialize hmac verifier")?;
    mac.update(payload);
    mac.verify_slice(&signature_bytes)
        .map_err(|_| anyhow!("signature verification failed"))
}

fn decode_hex(raw: &str) -> Result<Vec<u8>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("signature digest cannot be empty");
    }
    if !trimmed.len().is_multiple_of(2) {
        bail!("signature digest must have an even number of hex characters");
    }
    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let mut index = 0usize;
    while index < trimmed.len() {
        let next = index.saturating_add(2);
        let chunk = &trimmed[index..next];
        let byte = u8::from_str_radix(chunk, 16)
            .with_context(|| format!("invalid hex byte '{}' in signature digest", chunk))?;
        bytes.push(byte);
        index = next;
    }
    Ok(bytes)
}

fn extract_whatsapp_value_objects(payload: &Value) -> Vec<Value> {
    let mut values = Vec::new();
    if let Some(entries) = payload.get("entry").and_then(Value::as_array) {
        for entry in entries {
            if let Some(changes) = entry.get("changes").and_then(Value::as_array) {
                for change in changes {
                    if let Some(value) = change.get("value").and_then(Value::as_object) {
                        values.push(Value::Object(value.clone()));
                    }
                }
            }
        }
    }
    if values.is_empty() && payload.get("messages").is_some() {
        values.push(payload.clone());
    }
    values
}

fn load_multi_channel_live_connectors_state(
    path: &Path,
) -> Result<MultiChannelLiveConnectorStateFile> {
    if !path.exists() {
        return Ok(MultiChannelLiveConnectorStateFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = serde_json::from_str::<MultiChannelLiveConnectorStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(parsed)
}

fn save_multi_channel_live_connectors_state(
    path: &Path,
    state: &MultiChannelLiveConnectorStateFile,
) -> Result<()> {
    create_parent_dir_if_needed(path)?;
    let encoded = serde_json::to_string_pretty(state)
        .context("failed to encode live connector state file")?;
    std::fs::write(path, encoded).with_context(|| format!("failed to write {}", path.display()))
}

fn create_parent_dir_if_needed(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use httpmock::prelude::*;
    use tempfile::tempdir;

    fn build_connector_config(temp: &Path) -> MultiChannelLiveConnectorsConfig {
        MultiChannelLiveConnectorsConfig {
            state_path: temp.join("connectors-state.json"),
            ingress_dir: temp.join("live-ingress"),
            processed_event_cap: 128,
            retry_max_attempts: 2,
            retry_base_delay_ms: 0,
            poll_once: true,
            webhook_bind: "127.0.0.1:0".to_string(),
            telegram_mode: MultiChannelLiveConnectorMode::Disabled,
            telegram_api_base: "https://api.telegram.org".to_string(),
            telegram_bot_token: None,
            telegram_webhook_secret: None,
            discord_mode: MultiChannelLiveConnectorMode::Disabled,
            discord_api_base: "https://discord.com/api/v10".to_string(),
            discord_bot_token: None,
            discord_ingress_channel_ids: Vec::new(),
            whatsapp_mode: MultiChannelLiveConnectorMode::Disabled,
            whatsapp_webhook_verify_token: None,
            whatsapp_webhook_app_secret: None,
        }
    }

    fn read_ndjson(path: &Path) -> Vec<Value> {
        let raw = std::fs::read_to_string(path).expect("read ndjson");
        raw.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<Value>(line).expect("parse line"))
            .collect()
    }

    fn whatsapp_cloud_payload(message_id: &str) -> Value {
        json!({
            "entry": [{
                "changes": [{
                    "value": {
                        "metadata": {"phone_number_id":"15551230000"},
                        "messages": [{
                            "id": message_id,
                            "from": "15551238888",
                            "timestamp": "1760100000",
                            "text": {"body":"hello from whatsapp"}
                        }]
                    }
                }]
            }]
        })
    }

    #[test]
    fn unit_load_status_report_returns_default_when_state_is_missing() {
        let temp = tempdir().expect("tempdir");
        let status = load_multi_channel_live_connectors_status_report(
            temp.path().join("missing.json").as_path(),
        )
        .expect("status report should load");
        assert!(!status.state_present);
        assert_eq!(status.processed_event_count, 0);
        assert!(status.channels.is_empty());
    }

    #[test]
    fn unit_breaker_opens_when_retry_budget_is_exhausted() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_connector_config(temp.path());
        config.telegram_mode = MultiChannelLiveConnectorMode::Polling;

        let mut state = MultiChannelLiveConnectorStateFile::default();
        initialize_channel_modes(&config, &mut state);

        record_channel_error(
            &config,
            &mut state,
            "telegram",
            MultiChannelLiveConnectorErrorCode::ProviderUnavailable,
            "provider unavailable",
            true,
        );
        let telegram = state.channels.get("telegram").expect("telegram channel");
        assert_eq!(telegram.breaker_state, CONNECTOR_BREAKER_STATE_CLOSED);
        assert_eq!(telegram.retry_budget_remaining, 1);
        assert_eq!(telegram.breaker_open_count, 0);

        record_channel_error(
            &config,
            &mut state,
            "telegram",
            MultiChannelLiveConnectorErrorCode::ProviderUnavailable,
            "provider unavailable",
            true,
        );
        let telegram = state.channels.get("telegram").expect("telegram channel");
        assert_eq!(telegram.breaker_state, CONNECTOR_BREAKER_STATE_OPEN);
        assert_eq!(
            telegram.breaker_last_open_reason,
            MultiChannelLiveConnectorErrorCode::ProviderUnavailable.as_str()
        );
        assert_eq!(telegram.breaker_open_count, 1);
        assert_eq!(telegram.retry_budget_remaining, 0);
    }

    #[tokio::test]
    async fn functional_status_snapshot_includes_breaker_posture_fields() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let telegram_mock = server.mock(|when, then| {
            when.method(GET).path("/bottelegram-token/getUpdates");
            then.status(503).body(r#"{"ok":false}"#);
        });

        let mut config = build_connector_config(temp.path());
        config.telegram_mode = MultiChannelLiveConnectorMode::Polling;
        config.telegram_api_base = server.base_url();
        config.telegram_bot_token = Some("telegram-token".to_string());
        config.retry_max_attempts = 1;
        config.retry_base_delay_ms = 1;

        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("first cycle should run");
        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("second cycle should run");

        let status = load_multi_channel_live_connectors_status_report(config.state_path.as_path())
            .expect("status");
        let telegram = status.channels.get("telegram").expect("telegram channel");
        assert_eq!(telegram.breaker_state, CONNECTOR_BREAKER_STATE_OPEN);
        assert_eq!(telegram.liveness, "open");
        assert_eq!(telegram.retry_budget_remaining, 0);
        assert_eq!(
            telegram.breaker_last_open_reason,
            MultiChannelLiveConnectorErrorCode::ProviderUnavailable.as_str()
        );
        assert!(telegram.breaker_open_until_unix_ms >= current_unix_timestamp_ms());
        telegram_mock.assert_calls(2);
    }

    #[tokio::test]
    async fn integration_breaker_skips_polling_until_cooldown_then_recovers() {
        let temp = tempdir().expect("tempdir");
        let failing_server = MockServer::start();
        let failing_mock = failing_server.mock(|when, then| {
            when.method(GET).path("/bottelegram-token/getUpdates");
            then.status(503).body(r#"{"ok":false}"#);
        });

        let mut config = build_connector_config(temp.path());
        config.telegram_mode = MultiChannelLiveConnectorMode::Polling;
        config.telegram_api_base = failing_server.base_url();
        config.telegram_bot_token = Some("telegram-token".to_string());
        config.retry_max_attempts = 1;
        config.retry_base_delay_ms = 10_000;

        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("first cycle should run");
        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("second cycle should run");

        let status = load_multi_channel_live_connectors_status_report(config.state_path.as_path())
            .expect("status");
        let telegram = status.channels.get("telegram").expect("telegram channel");
        assert_eq!(telegram.breaker_state, CONNECTOR_BREAKER_STATE_OPEN);

        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("third cycle should run");
        failing_mock.assert_calls(2);

        let mut state =
            load_multi_channel_live_connectors_state(config.state_path.as_path()).expect("state");
        let telegram = state
            .channels
            .get_mut("telegram")
            .expect("telegram channel state");
        telegram.breaker_open_until_unix_ms = current_unix_timestamp_ms().saturating_sub(1);
        save_multi_channel_live_connectors_state(config.state_path.as_path(), &state)
            .expect("save state");

        let recovery_server = MockServer::start();
        let recovery_mock = recovery_server.mock(|when, then| {
            when.method(GET).path("/bottelegram-token/getUpdates");
            then.status(200)
                .body(json!({"ok":true,"result":[]}).to_string());
        });
        config.telegram_api_base = recovery_server.base_url();
        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("recovery cycle should run");

        let status = load_multi_channel_live_connectors_status_report(config.state_path.as_path())
            .expect("status");
        let telegram = status.channels.get("telegram").expect("telegram channel");
        assert_eq!(telegram.breaker_state, CONNECTOR_BREAKER_STATE_CLOSED);
        assert_eq!(telegram.retry_budget_remaining, 1);
        assert_eq!(telegram.consecutive_failures, 0);
        recovery_mock.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_single_retryable_failure_does_not_open_breaker() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let telegram_mock = server.mock(|when, then| {
            when.method(GET).path("/bottelegram-token/getUpdates");
            then.status(503).body(r#"{"ok":false}"#);
        });

        let mut config = build_connector_config(temp.path());
        config.telegram_mode = MultiChannelLiveConnectorMode::Polling;
        config.telegram_api_base = server.base_url();
        config.telegram_bot_token = Some("telegram-token".to_string());
        config.retry_max_attempts = 1;
        config.retry_base_delay_ms = 0;

        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("cycle should run");

        let status = load_multi_channel_live_connectors_status_report(config.state_path.as_path())
            .expect("status");
        let telegram = status.channels.get("telegram").expect("telegram channel");
        assert_eq!(telegram.breaker_state, CONNECTOR_BREAKER_STATE_CLOSED);
        assert_eq!(telegram.consecutive_failures, 1);
        assert_eq!(telegram.breaker_open_count, 0);
        telegram_mock.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_poll_cycle_ingests_telegram_and_discord_events() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let telegram_mock = server.mock(|when, then| {
            when.method(GET).path("/bottelegram-token/getUpdates");
            then.status(200).body(
                json!({
                    "ok": true,
                    "result": [{
                        "update_id": 1001,
                        "message": {
                            "message_id": 42,
                            "date": 1760100000u64,
                            "text": "hello from telegram",
                            "chat": {"id":"chat-100"},
                            "from": {"id":"user-7", "username":"ops"}
                        }
                    }]
                })
                .to_string(),
            );
        });
        let discord_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/channels/discord-room/messages")
                .header("authorization", "Bot discord-token");
            then.status(200).body(
                json!([{
                    "id":"1900000000000000001",
                    "channel_id":"discord-room",
                    "content":"/status",
                    "timestamp":"2025-10-10T12:30:00Z",
                    "author":{"id":"discord-user-1","username":"n"}
                }])
                .to_string(),
            );
        });

        let mut config = build_connector_config(temp.path());
        config.telegram_mode = MultiChannelLiveConnectorMode::Polling;
        config.telegram_api_base = server.base_url();
        config.telegram_bot_token = Some("telegram-token".to_string());
        config.discord_mode = MultiChannelLiveConnectorMode::Polling;
        config.discord_api_base = server.base_url();
        config.discord_bot_token = Some("discord-token".to_string());
        config.discord_ingress_channel_ids = vec!["discord-room".to_string()];

        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("poll cycle should succeed");

        let telegram_lines = read_ndjson(&config.ingress_dir.join("telegram.ndjson"));
        assert_eq!(telegram_lines.len(), 1);
        assert_eq!(telegram_lines[0]["transport"].as_str(), Some("telegram"));

        let discord_lines = read_ndjson(&config.ingress_dir.join("discord.ndjson"));
        assert_eq!(discord_lines.len(), 1);
        assert_eq!(discord_lines[0]["transport"].as_str(), Some("discord"));

        telegram_mock.assert();
        discord_mock.assert();
    }

    #[tokio::test]
    async fn integration_whatsapp_webhook_ingests_signed_cloud_payload() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_connector_config(temp.path());
        config.whatsapp_mode = MultiChannelLiveConnectorMode::Webhook;
        config.whatsapp_webhook_app_secret = Some("secret".to_string());
        config.whatsapp_webhook_verify_token = Some("verify".to_string());
        config.poll_once = false;

        let state = Arc::new(Mutex::new(MultiChannelLiveConnectorStateFile::default()));
        let server_state = Arc::new(LiveConnectorServerState {
            config: config.clone(),
            state: state.clone(),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let app = build_multi_channel_live_webhook_router(server_state);
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        tokio::time::sleep(Duration::from_millis(25)).await;

        let payload = whatsapp_cloud_payload("wamid.1");
        let raw = payload.to_string();
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").expect("hmac");
        mac.update(raw.as_bytes());
        let signature_bytes = mac.finalize().into_bytes();
        let signature = format!(
            "sha256={}",
            signature_bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );

        let client = Client::new();
        let response = client
            .post(format!("http://{addr}/webhooks/whatsapp"))
            .header(
                "x-hub-signature-256",
                HeaderValue::from_str(signature.as_str()).expect("signature header"),
            )
            .body(raw)
            .send()
            .await
            .expect("send webhook");
        assert_eq!(response.status(), StatusCode::OK);

        let whatsapp_lines = read_ndjson(&config.ingress_dir.join("whatsapp.ndjson"));
        assert_eq!(whatsapp_lines.len(), 1);
        assert_eq!(whatsapp_lines[0]["transport"].as_str(), Some("whatsapp"));

        handle.abort();
    }

    #[tokio::test]
    async fn regression_poll_cycle_skips_duplicate_events() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let telegram_mock = server.mock(|when, then| {
            when.method(GET).path("/bottelegram-token/getUpdates");
            then.status(200).body(
                json!({
                    "ok": true,
                    "result": [{
                        "update_id": 1001,
                        "message": {
                            "message_id": 42,
                            "date": 1760100000u64,
                            "text": "hello from telegram",
                            "chat": {"id":"chat-100"},
                            "from": {"id":"user-7"}
                        }
                    }]
                })
                .to_string(),
            );
        });

        let mut config = build_connector_config(temp.path());
        config.telegram_mode = MultiChannelLiveConnectorMode::Polling;
        config.telegram_api_base = server.base_url();
        config.telegram_bot_token = Some("telegram-token".to_string());

        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("first cycle should succeed");
        run_multi_channel_live_connectors_runner(config.clone())
            .await
            .expect("second cycle should succeed");

        let telegram_lines = read_ndjson(&config.ingress_dir.join("telegram.ndjson"));
        assert_eq!(telegram_lines.len(), 1);
        telegram_mock.assert_calls(2);

        let status = load_multi_channel_live_connectors_status_report(config.state_path.as_path())
            .expect("status");
        let telegram = status.channels.get("telegram").expect("telegram channel");
        assert!(telegram.duplicates_skipped >= 1);
    }

    #[tokio::test]
    async fn regression_whatsapp_webhook_rejects_invalid_signature() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_connector_config(temp.path());
        config.whatsapp_mode = MultiChannelLiveConnectorMode::Webhook;
        config.whatsapp_webhook_app_secret = Some("secret".to_string());
        config.whatsapp_webhook_verify_token = Some("verify".to_string());

        let state = Arc::new(Mutex::new(MultiChannelLiveConnectorStateFile::default()));
        let server_state = Arc::new(LiveConnectorServerState {
            config: config.clone(),
            state: state.clone(),
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let app = build_multi_channel_live_webhook_router(server_state);
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        tokio::time::sleep(Duration::from_millis(25)).await;

        let response = Client::new()
            .post(format!("http://{addr}/webhooks/whatsapp"))
            .header("x-hub-signature-256", "sha256=deadbeef")
            .body(whatsapp_cloud_payload("wamid.bad").to_string())
            .send()
            .await
            .expect("send webhook");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        handle.abort();
    }
}
