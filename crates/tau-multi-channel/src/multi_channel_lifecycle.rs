//! Lifecycle orchestration utilities for multi-channel deployments.
//!
//! This module coordinates setup/health probes and lifecycle transitions across
//! configured channels. It documents when lifecycle checks short-circuit startup
//! versus when retry/backoff paths continue execution with diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::multi_channel_contract::MultiChannelTransport;
use crate::multi_channel_credentials::{
    resolve_secret, MultiChannelCredentialStoreSnapshot, ResolvedSecret,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};

pub const MULTI_CHANNEL_LIFECYCLE_STATE_FILE_NAME: &str = "channel-lifecycle.json";
const MULTI_CHANNEL_LIFECYCLE_STATE_SCHEMA_VERSION: u32 = 1;

const TELEGRAM_TOKEN_INTEGRATION_ID: &str = "telegram-bot-token";
const DISCORD_TOKEN_INTEGRATION_ID: &str = "discord-bot-token";
const WHATSAPP_TOKEN_INTEGRATION_ID: &str = "whatsapp-access-token";
const WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID: &str = "whatsapp-phone-number-id";
const ONLINE_PROBE_TIMEOUT_MS: u64 = 3_000;
const ONLINE_PROBE_MAX_ATTEMPTS: usize = 2;
const ONLINE_PROBE_RETRY_DELAY_MS: u64 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `MultiChannelLifecycleAction` values.
pub enum MultiChannelLifecycleAction {
    Status,
    Login,
    Logout,
    Probe,
}

impl MultiChannelLifecycleAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Login => "login",
            Self::Logout => "logout",
            Self::Probe => "probe",
        }
    }
}

#[derive(Debug, Clone)]
/// Public struct `MultiChannelLifecycleCommandConfig` used across Tau components.
pub struct MultiChannelLifecycleCommandConfig {
    pub state_dir: PathBuf,
    pub ingress_dir: PathBuf,
    pub telegram_api_base: String,
    pub discord_api_base: String,
    pub whatsapp_api_base: String,
    pub credential_store: Option<MultiChannelCredentialStoreSnapshot>,
    pub credential_store_unreadable: bool,
    pub telegram_bot_token: Option<String>,
    pub discord_bot_token: Option<String>,
    pub whatsapp_access_token: Option<String>,
    pub whatsapp_phone_number_id: Option<String>,
    pub probe_online: bool,
    pub probe_online_timeout_ms: u64,
    pub probe_online_max_attempts: usize,
    pub probe_online_retry_delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `MultiChannelLifecycleReport` used across Tau components.
pub struct MultiChannelLifecycleReport {
    pub action: String,
    pub channel: String,
    pub probe_mode: String,
    pub lifecycle_status: String,
    pub readiness_status: String,
    pub reason_codes: Vec<String>,
    pub online_probe_status: String,
    pub online_probe_reason_codes: Vec<String>,
    pub remediation_hints: Vec<String>,
    pub ingress_file: String,
    pub ingress_exists: bool,
    pub ingress_is_file: bool,
    pub token_source: String,
    pub phone_number_source: String,
    pub state_path: String,
    pub state_persisted: bool,
    pub updated_unix_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MultiChannelLifecycleStateFile {
    #[serde(default = "multi_channel_lifecycle_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    channels: BTreeMap<String, MultiChannelLifecycleChannelState>,
}

impl Default for MultiChannelLifecycleStateFile {
    fn default() -> Self {
        Self {
            schema_version: MULTI_CHANNEL_LIFECYCLE_STATE_SCHEMA_VERSION,
            channels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct MultiChannelLifecycleChannelState {
    #[serde(default)]
    lifecycle_status: String,
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    last_probe_mode: String,
    #[serde(default)]
    last_online_probe_status: String,
    #[serde(default)]
    last_online_probe_reason_codes: Vec<String>,
    #[serde(default)]
    last_remediation_hints: Vec<String>,
    #[serde(default)]
    last_action: String,
    #[serde(default)]
    last_updated_unix_ms: u64,
    #[serde(default)]
    last_login_unix_ms: u64,
    #[serde(default)]
    last_logout_unix_ms: u64,
    #[serde(default)]
    last_probe_unix_ms: u64,
}

#[derive(Debug, Clone)]
struct ChannelReadiness {
    probe_mode: String,
    readiness_status: String,
    reason_codes: Vec<String>,
    online_probe_status: String,
    online_probe_reason_codes: Vec<String>,
    remediation_hints: Vec<String>,
    ingress_file: PathBuf,
    ingress_exists: bool,
    ingress_is_file: bool,
    token_source: String,
    phone_number_source: String,
}

#[derive(Debug, Clone)]
struct OnlineProbeOutcome {
    status: String,
    reason_codes: Vec<String>,
}

#[derive(Debug, Clone)]
struct OnlineProbeError {
    reason_code: &'static str,
    retryable: bool,
}

fn multi_channel_lifecycle_state_schema_version() -> u32 {
    MULTI_CHANNEL_LIFECYCLE_STATE_SCHEMA_VERSION
}

pub fn default_probe_timeout_ms() -> u64 {
    ONLINE_PROBE_TIMEOUT_MS
}

pub fn default_probe_max_attempts() -> usize {
    ONLINE_PROBE_MAX_ATTEMPTS
}

pub fn default_probe_retry_delay_ms() -> u64 {
    ONLINE_PROBE_RETRY_DELAY_MS
}

pub fn execute_multi_channel_lifecycle_action(
    config: &MultiChannelLifecycleCommandConfig,
    action: MultiChannelLifecycleAction,
    channel: MultiChannelTransport,
) -> Result<MultiChannelLifecycleReport> {
    let state_path = lifecycle_state_path_for_dir(&config.state_dir);
    let mut state = load_multi_channel_lifecycle_state(&state_path)?;
    let channel_key = channel.as_str().to_string();
    let existing_entry = state
        .channels
        .get(&channel_key)
        .cloned()
        .unwrap_or_default();
    let online_probe = matches!(action, MultiChannelLifecycleAction::Probe) && config.probe_online;
    let mut readiness = probe_channel_readiness(
        config,
        channel,
        !matches!(action, MultiChannelLifecycleAction::Login),
        online_probe,
    );

    let now_unix_ms = current_unix_timestamp_ms();
    let mut lifecycle_status = if existing_entry.lifecycle_status.trim().is_empty() {
        "unknown".to_string()
    } else {
        existing_entry.lifecycle_status.clone()
    };
    let mut reason_codes = readiness.reason_codes.clone();
    let mut state_persisted = false;

    match action {
        MultiChannelLifecycleAction::Status => {}
        MultiChannelLifecycleAction::Login => {
            if readiness.readiness_status == "pass" {
                ensure_ingress_file_exists(&readiness.ingress_file)?;
                readiness.ingress_exists = true;
                readiness.ingress_is_file = true;
                readiness.readiness_status = "pass".to_string();
                readiness.reason_codes = vec!["ready".to_string()];
                reason_codes = readiness.reason_codes.clone();
                lifecycle_status = "initialized".to_string();
            } else {
                lifecycle_status = "login_failed".to_string();
            }
            let entry = state.channels.entry(channel_key).or_default();
            entry.lifecycle_status = lifecycle_status.clone();
            entry.reason_codes = reason_codes.clone();
            entry.last_action = action.as_str().to_string();
            entry.last_updated_unix_ms = now_unix_ms;
            entry.last_login_unix_ms = now_unix_ms;
            save_multi_channel_lifecycle_state(&state_path, &state)?;
            state_persisted = true;
        }
        MultiChannelLifecycleAction::Logout => {
            lifecycle_status = "logged_out".to_string();
            reason_codes = vec!["logout_requested".to_string()];
            let entry = state.channels.entry(channel_key).or_default();
            entry.lifecycle_status = lifecycle_status.clone();
            entry.reason_codes = reason_codes.clone();
            entry.last_action = action.as_str().to_string();
            entry.last_updated_unix_ms = now_unix_ms;
            entry.last_logout_unix_ms = now_unix_ms;
            save_multi_channel_lifecycle_state(&state_path, &state)?;
            state_persisted = true;
        }
        MultiChannelLifecycleAction::Probe => {
            lifecycle_status = if readiness.readiness_status == "pass" {
                "ready".to_string()
            } else {
                "probe_failed".to_string()
            };
            reason_codes = readiness.reason_codes.clone();
            let entry = state.channels.entry(channel_key).or_default();
            entry.lifecycle_status = lifecycle_status.clone();
            entry.reason_codes = reason_codes.clone();
            entry.last_probe_mode = readiness.probe_mode.clone();
            entry.last_online_probe_status = readiness.online_probe_status.clone();
            entry.last_online_probe_reason_codes = readiness.online_probe_reason_codes.clone();
            entry.last_remediation_hints = readiness.remediation_hints.clone();
            entry.last_action = action.as_str().to_string();
            entry.last_updated_unix_ms = now_unix_ms;
            entry.last_probe_unix_ms = now_unix_ms;
            save_multi_channel_lifecycle_state(&state_path, &state)?;
            state_persisted = true;
        }
    }

    Ok(MultiChannelLifecycleReport {
        action: action.as_str().to_string(),
        channel: channel.as_str().to_string(),
        probe_mode: readiness.probe_mode,
        lifecycle_status,
        readiness_status: readiness.readiness_status,
        reason_codes,
        online_probe_status: readiness.online_probe_status,
        online_probe_reason_codes: readiness.online_probe_reason_codes,
        remediation_hints: readiness.remediation_hints,
        ingress_file: readiness.ingress_file.display().to_string(),
        ingress_exists: readiness.ingress_exists,
        ingress_is_file: readiness.ingress_is_file,
        token_source: readiness.token_source,
        phone_number_source: readiness.phone_number_source,
        state_path: state_path.display().to_string(),
        state_persisted,
        updated_unix_ms: now_unix_ms,
    })
}

fn lifecycle_state_path_for_dir(state_dir: &Path) -> PathBuf {
    state_dir
        .join("security")
        .join(MULTI_CHANNEL_LIFECYCLE_STATE_FILE_NAME)
}

fn load_multi_channel_lifecycle_state(path: &Path) -> Result<MultiChannelLifecycleStateFile> {
    if !path.exists() {
        return Ok(MultiChannelLifecycleStateFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: MultiChannelLifecycleStateFile = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse multi-channel lifecycle state {}",
            path.display()
        )
    })?;
    if parsed.schema_version != MULTI_CHANNEL_LIFECYCLE_STATE_SCHEMA_VERSION {
        bail!(
            "unsupported multi-channel lifecycle schema {} in {}",
            parsed.schema_version,
            path.display()
        );
    }
    Ok(parsed)
}

fn save_multi_channel_lifecycle_state(
    path: &Path,
    state: &MultiChannelLifecycleStateFile,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload =
        serde_json::to_string_pretty(state).context("failed to serialize lifecycle state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

fn ensure_ingress_file_exists(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    if path.exists() && !path.is_file() {
        bail!("ingress path '{}' exists but is not a file", path.display());
    }
    if !path.exists() {
        std::fs::write(path, "").with_context(|| format!("failed to create {}", path.display()))?;
    }
    Ok(())
}

fn ingress_file_for_transport(ingress_dir: &Path, channel: MultiChannelTransport) -> PathBuf {
    ingress_dir.join(format!("{}.ndjson", channel.as_str()))
}

fn probe_channel_readiness(
    config: &MultiChannelLifecycleCommandConfig,
    channel: MultiChannelTransport,
    require_ingress_file: bool,
    online_probe: bool,
) -> ChannelReadiness {
    let ingress_file = ingress_file_for_transport(&config.ingress_dir, channel);
    let ingress_exists = ingress_file.exists();
    let ingress_is_file = ingress_file.is_file();
    let mut reason_codes = Vec::new();
    let (token_source, phone_number_source, token_value, phone_number_value) = match channel {
        MultiChannelTransport::Telegram => {
            let token = resolve_lifecycle_secret(
                config,
                config.telegram_bot_token.as_deref(),
                TELEGRAM_TOKEN_INTEGRATION_ID,
            );
            if token.credential_store_unreadable {
                reason_codes.push("credential_store_unreadable".to_string());
            }
            if token.value.is_none() {
                reason_codes.push("missing_telegram_bot_token".to_string());
            }
            (token.source, "not_required".to_string(), token.value, None)
        }
        MultiChannelTransport::Discord => {
            let token = resolve_lifecycle_secret(
                config,
                config.discord_bot_token.as_deref(),
                DISCORD_TOKEN_INTEGRATION_ID,
            );
            if token.credential_store_unreadable {
                reason_codes.push("credential_store_unreadable".to_string());
            }
            if token.value.is_none() {
                reason_codes.push("missing_discord_bot_token".to_string());
            }
            (token.source, "not_required".to_string(), token.value, None)
        }
        MultiChannelTransport::Whatsapp => {
            let token = resolve_lifecycle_secret(
                config,
                config.whatsapp_access_token.as_deref(),
                WHATSAPP_TOKEN_INTEGRATION_ID,
            );
            if token.credential_store_unreadable {
                reason_codes.push("credential_store_unreadable".to_string());
            }
            if token.value.is_none() {
                reason_codes.push("missing_whatsapp_access_token".to_string());
            }

            let phone_id = resolve_lifecycle_secret(
                config,
                config.whatsapp_phone_number_id.as_deref(),
                WHATSAPP_PHONE_NUMBER_ID_INTEGRATION_ID,
            );
            if phone_id.credential_store_unreadable {
                reason_codes.push("credential_store_unreadable".to_string());
            }
            if phone_id.value.is_none() {
                reason_codes.push("missing_whatsapp_phone_number_id".to_string());
            }

            (token.source, phone_id.source, token.value, phone_id.value)
        }
    };

    if require_ingress_file {
        if !ingress_exists {
            reason_codes.push("ingress_missing".to_string());
        } else if !ingress_is_file {
            reason_codes.push("ingress_not_file".to_string());
        }
    } else if ingress_exists && !ingress_is_file {
        reason_codes.push("ingress_not_file".to_string());
    }

    let mut online_probe_status = if online_probe {
        "pending".to_string()
    } else {
        "disabled".to_string()
    };
    let mut online_probe_reason_codes = if online_probe {
        Vec::new()
    } else {
        vec!["probe_online_disabled".to_string()]
    };
    if online_probe {
        let credentials_ready = match channel {
            MultiChannelTransport::Whatsapp => {
                token_value.as_deref().is_some() && phone_number_value.as_deref().is_some()
            }
            _ => token_value.as_deref().is_some(),
        };
        if !credentials_ready {
            online_probe_status = "fail".to_string();
            online_probe_reason_codes = vec!["probe_online_missing_prerequisites".to_string()];
        } else {
            let token = token_value.as_deref().unwrap_or_default();
            let phone_number_id = phone_number_value.as_deref();
            let outcome = run_online_probe_with_retry(config, channel, token, phone_number_id);
            online_probe_status = outcome.status;
            online_probe_reason_codes = outcome.reason_codes;
        }
        if online_probe_status != "pass" {
            append_unique_reason_codes(&mut reason_codes, &online_probe_reason_codes);
        }
    }

    let readiness_status = if reason_codes.is_empty() {
        "pass"
    } else {
        "fail"
    }
    .to_string();
    let reason_codes = if reason_codes.is_empty() {
        vec!["ready".to_string()]
    } else {
        reason_codes
    };
    let remediation_hints =
        collect_remediation_hints(channel, &reason_codes, &online_probe_reason_codes);

    ChannelReadiness {
        probe_mode: if online_probe {
            "online".to_string()
        } else {
            "offline".to_string()
        },
        readiness_status,
        reason_codes,
        online_probe_status,
        online_probe_reason_codes,
        remediation_hints,
        ingress_file,
        ingress_exists,
        ingress_is_file,
        token_source,
        phone_number_source,
    }
}

fn run_online_probe_with_retry(
    config: &MultiChannelLifecycleCommandConfig,
    channel: MultiChannelTransport,
    token: &str,
    phone_number_id: Option<&str>,
) -> OnlineProbeOutcome {
    let client = match Client::builder()
        .timeout(Duration::from_millis(config.probe_online_timeout_ms))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return OnlineProbeOutcome {
                status: "fail".to_string(),
                reason_codes: vec!["probe_online_http_client_unavailable".to_string()],
            }
        }
    };

    let max_attempts = config.probe_online_max_attempts.max(1);
    let mut last_error: Option<OnlineProbeError> = None;
    for attempt in 1..=max_attempts {
        let probe = match channel {
            MultiChannelTransport::Telegram => probe_telegram_online(&client, config, token),
            MultiChannelTransport::Discord => probe_discord_online(&client, config, token),
            MultiChannelTransport::Whatsapp => {
                let Some(phone_number_id) = phone_number_id else {
                    return OnlineProbeOutcome {
                        status: "fail".to_string(),
                        reason_codes: vec!["probe_online_missing_prerequisites".to_string()],
                    };
                };
                probe_whatsapp_online(&client, config, token, phone_number_id)
            }
        };
        match probe {
            Ok(()) => {
                return OnlineProbeOutcome {
                    status: "pass".to_string(),
                    reason_codes: vec!["probe_online_ready".to_string()],
                }
            }
            Err(error) => {
                let should_retry = error.retryable && attempt < max_attempts;
                last_error = Some(error);
                if should_retry {
                    thread::sleep(Duration::from_millis(config.probe_online_retry_delay_ms));
                    continue;
                }
                break;
            }
        }
    }

    let reason_code = last_error
        .map(|error| error.reason_code.to_string())
        .unwrap_or_else(|| "probe_online_unknown_failure".to_string());
    OnlineProbeOutcome {
        status: "fail".to_string(),
        reason_codes: vec![reason_code],
    }
}

fn probe_telegram_online(
    client: &Client,
    config: &MultiChannelLifecycleCommandConfig,
    token: &str,
) -> Result<(), OnlineProbeError> {
    let endpoint = format!(
        "{}/bot{token}/getMe",
        config.telegram_api_base.trim_end_matches('/')
    );
    let response = client.get(&endpoint).send().map_err(|error| {
        classify_transport_error(
            &error,
            "probe_online_telegram_timeout",
            "probe_online_telegram_transport_error",
        )
    })?;
    let status = response.status();
    let body_raw = response.text().unwrap_or_default();
    if status.is_success() {
        let payload = serde_json::from_str::<Value>(&body_raw).unwrap_or(Value::Null);
        let ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if ok {
            return Ok(());
        }
        return Err(OnlineProbeError {
            reason_code: "probe_online_telegram_invalid_response",
            retryable: false,
        });
    }
    Err(classify_telegram_status(status))
}

fn probe_discord_online(
    client: &Client,
    config: &MultiChannelLifecycleCommandConfig,
    token: &str,
) -> Result<(), OnlineProbeError> {
    let endpoint = format!(
        "{}/users/@me",
        config.discord_api_base.trim_end_matches('/')
    );
    let response = client
        .get(&endpoint)
        .header("Authorization", format!("Bot {token}"))
        .send()
        .map_err(|error| {
            classify_transport_error(
                &error,
                "probe_online_discord_timeout",
                "probe_online_discord_transport_error",
            )
        })?;
    let status = response.status();
    let body_raw = response.text().unwrap_or_default();
    if status.is_success() {
        let payload = serde_json::from_str::<Value>(&body_raw).unwrap_or(Value::Null);
        if payload.get("id").and_then(Value::as_str).is_some() {
            return Ok(());
        }
        return Err(OnlineProbeError {
            reason_code: "probe_online_discord_invalid_response",
            retryable: false,
        });
    }
    Err(classify_discord_status(status))
}

fn probe_whatsapp_online(
    client: &Client,
    config: &MultiChannelLifecycleCommandConfig,
    token: &str,
    phone_number_id: &str,
) -> Result<(), OnlineProbeError> {
    let endpoint = format!(
        "{}/{phone_number_id}",
        config.whatsapp_api_base.trim_end_matches('/')
    );
    let response = client
        .get(&endpoint)
        .header("Authorization", format!("Bearer {token}"))
        .query(&[("fields", "id")])
        .send()
        .map_err(|error| {
            classify_transport_error(
                &error,
                "probe_online_whatsapp_timeout",
                "probe_online_whatsapp_transport_error",
            )
        })?;
    let status = response.status();
    let body_raw = response.text().unwrap_or_default();
    let body_json = serde_json::from_str::<Value>(&body_raw).unwrap_or(Value::Null);
    if status.is_success() {
        if body_json.get("id").and_then(Value::as_str).is_some() {
            return Ok(());
        }
        return Err(OnlineProbeError {
            reason_code: "probe_online_whatsapp_invalid_response",
            retryable: false,
        });
    }
    Err(classify_whatsapp_status(status, &body_json))
}

fn classify_transport_error(
    error: &reqwest::Error,
    timeout_reason_code: &'static str,
    transport_reason_code: &'static str,
) -> OnlineProbeError {
    if error.is_timeout() {
        return OnlineProbeError {
            reason_code: timeout_reason_code,
            retryable: true,
        };
    }
    OnlineProbeError {
        reason_code: transport_reason_code,
        retryable: true,
    }
}

fn classify_telegram_status(status: StatusCode) -> OnlineProbeError {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return OnlineProbeError {
            reason_code: "probe_online_telegram_auth_failed",
            retryable: false,
        };
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return OnlineProbeError {
            reason_code: "probe_online_telegram_rate_limited",
            retryable: true,
        };
    }
    if status.is_server_error() {
        return OnlineProbeError {
            reason_code: "probe_online_telegram_provider_unavailable",
            retryable: true,
        };
    }
    OnlineProbeError {
        reason_code: "probe_online_telegram_request_rejected",
        retryable: false,
    }
}

fn classify_discord_status(status: StatusCode) -> OnlineProbeError {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return OnlineProbeError {
            reason_code: "probe_online_discord_auth_failed",
            retryable: false,
        };
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return OnlineProbeError {
            reason_code: "probe_online_discord_rate_limited",
            retryable: true,
        };
    }
    if status.is_server_error() {
        return OnlineProbeError {
            reason_code: "probe_online_discord_provider_unavailable",
            retryable: true,
        };
    }
    OnlineProbeError {
        reason_code: "probe_online_discord_request_rejected",
        retryable: false,
    }
}

fn classify_whatsapp_status(status: StatusCode, payload: &Value) -> OnlineProbeError {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return OnlineProbeError {
            reason_code: "probe_online_whatsapp_auth_failed",
            retryable: false,
        };
    }
    if status == StatusCode::NOT_FOUND {
        return OnlineProbeError {
            reason_code: "probe_online_whatsapp_phone_number_not_found",
            retryable: false,
        };
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return OnlineProbeError {
            reason_code: "probe_online_whatsapp_rate_limited",
            retryable: true,
        };
    }
    if status.is_server_error() {
        return OnlineProbeError {
            reason_code: "probe_online_whatsapp_provider_unavailable",
            retryable: true,
        };
    }
    if status == StatusCode::BAD_REQUEST {
        let error_code = payload
            .get("error")
            .and_then(|value| value.get("code"))
            .and_then(Value::as_i64);
        if error_code == Some(190) {
            return OnlineProbeError {
                reason_code: "probe_online_whatsapp_auth_failed",
                retryable: false,
            };
        }
        if error_code == Some(100) {
            return OnlineProbeError {
                reason_code: "probe_online_whatsapp_phone_number_invalid",
                retryable: false,
            };
        }
    }
    OnlineProbeError {
        reason_code: "probe_online_whatsapp_request_rejected",
        retryable: false,
    }
}

fn append_unique_reason_codes(target: &mut Vec<String>, additions: &[String]) {
    for reason in additions {
        if !target.iter().any(|existing| existing == reason) {
            target.push(reason.clone());
        }
    }
}

fn collect_remediation_hints(
    channel: MultiChannelTransport,
    reason_codes: &[String],
    online_probe_reason_codes: &[String],
) -> Vec<String> {
    let mut hints = Vec::new();
    let mut seen = BTreeSet::new();
    for reason_code in reason_codes.iter().chain(online_probe_reason_codes.iter()) {
        let Some(hint) = remediation_hint_for_reason_code(channel, reason_code.as_str()) else {
            continue;
        };
        if seen.insert(hint) {
            hints.push(hint.to_string());
        }
    }
    hints
}

fn remediation_hint_for_reason_code(
    channel: MultiChannelTransport,
    reason_code: &str,
) -> Option<&'static str> {
    match reason_code {
        "missing_telegram_bot_token" => Some(
            "Set TAU_TELEGRAM_BOT_TOKEN or configure credential-store integration telegram-bot-token.",
        ),
        "missing_discord_bot_token" => Some(
            "Set TAU_DISCORD_BOT_TOKEN or configure credential-store integration discord-bot-token.",
        ),
        "missing_whatsapp_access_token" => Some(
            "Set TAU_WHATSAPP_ACCESS_TOKEN or configure credential-store integration whatsapp-access-token.",
        ),
        "missing_whatsapp_phone_number_id" => Some(
            "Set TAU_WHATSAPP_PHONE_NUMBER_ID or configure credential-store integration whatsapp-phone-number-id.",
        ),
        "credential_store_unreadable" => Some(
            "Verify credential-store path/encryption key or pass credentials directly via CLI/env.",
        ),
        "ingress_missing" => Some(match channel {
            MultiChannelTransport::Telegram => {
                "Run --multi-channel-channel-login telegram to initialize ingress state."
            }
            MultiChannelTransport::Discord => {
                "Run --multi-channel-channel-login discord to initialize ingress state."
            }
            MultiChannelTransport::Whatsapp => {
                "Run --multi-channel-channel-login whatsapp to initialize ingress state."
            }
        }),
        "ingress_not_file" => Some(
            "Ensure the ingress path is a writable .ndjson file and rerun login/probe.",
        ),
        "probe_online_missing_prerequisites" => {
            Some("Resolve missing credentials before running --multi-channel-channel-probe-online.")
        }
        "probe_online_http_client_unavailable" => {
            Some("Check local TLS/runtime environment and retry online probe.")
        }
        "probe_online_telegram_auth_failed" => {
            Some("Rotate Telegram bot token and verify the bot still has API access.")
        }
        "probe_online_telegram_rate_limited" => {
            Some("Telegram API rate limited the probe; retry after the backoff window.")
        }
        "probe_online_telegram_provider_unavailable" => {
            Some("Telegram API is unavailable; retry probe later.")
        }
        "probe_online_telegram_transport_error" | "probe_online_telegram_timeout" => {
            Some("Check network reachability to the Telegram API endpoint.")
        }
        "probe_online_telegram_request_rejected" | "probe_online_telegram_invalid_response" => {
            Some("Verify Telegram API base URL and token scope, then retry probe.")
        }
        "probe_online_discord_auth_failed" => {
            Some("Rotate Discord bot token and confirm the bot account is active.")
        }
        "probe_online_discord_rate_limited" => {
            Some("Discord API rate limited the probe; retry after the backoff window.")
        }
        "probe_online_discord_provider_unavailable" => {
            Some("Discord API is unavailable; retry probe later.")
        }
        "probe_online_discord_transport_error" | "probe_online_discord_timeout" => {
            Some("Check network reachability to the Discord API endpoint.")
        }
        "probe_online_discord_request_rejected" | "probe_online_discord_invalid_response" => {
            Some("Verify Discord API base URL and bot token scope, then retry probe.")
        }
        "probe_online_whatsapp_auth_failed" => {
            Some("Rotate WhatsApp access token and verify Graph API permissions.")
        }
        "probe_online_whatsapp_phone_number_not_found"
        | "probe_online_whatsapp_phone_number_invalid" => {
            Some("Verify WhatsApp phone number id matches the configured business account.")
        }
        "probe_online_whatsapp_rate_limited" => {
            Some("WhatsApp API rate limited the probe; retry after the backoff window.")
        }
        "probe_online_whatsapp_provider_unavailable" => {
            Some("WhatsApp API is unavailable; retry probe later.")
        }
        "probe_online_whatsapp_transport_error" | "probe_online_whatsapp_timeout" => {
            Some("Check network reachability to the WhatsApp Graph API endpoint.")
        }
        "probe_online_whatsapp_request_rejected" | "probe_online_whatsapp_invalid_response" => {
            Some("Verify WhatsApp API base URL, token scope, and phone id configuration.")
        }
        _ => None,
    }
}

fn resolve_lifecycle_secret(
    config: &MultiChannelLifecycleCommandConfig,
    direct_secret: Option<&str>,
    integration_id: &str,
) -> ResolvedSecret {
    resolve_secret(
        direct_secret,
        integration_id,
        config.credential_store.as_ref(),
        config.credential_store_unreadable,
    )
}

pub fn render_multi_channel_lifecycle_report(report: &MultiChannelLifecycleReport) -> String {
    let reason_codes = if report.reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.reason_codes.join(",")
    };
    let online_probe_reason_codes = if report.online_probe_reason_codes.is_empty() {
        "none".to_string()
    } else {
        report.online_probe_reason_codes.join(",")
    };
    let remediation_hints = if report.remediation_hints.is_empty() {
        "none".to_string()
    } else {
        report.remediation_hints.join(" | ")
    };
    format!(
        "multi-channel lifecycle: action={} channel={} probe_mode={} lifecycle_status={} readiness_status={} reason_codes={} online_probe_status={} online_probe_reason_codes={} remediation_hints={} ingress_file={} ingress_exists={} ingress_is_file={} token_source={} phone_number_source={} state_path={} state_persisted={} updated_unix_ms={}",
        report.action,
        report.channel,
        report.probe_mode,
        report.lifecycle_status,
        report.readiness_status,
        reason_codes,
        report.online_probe_status,
        online_probe_reason_codes,
        remediation_hints,
        report.ingress_file,
        report.ingress_exists,
        report.ingress_is_file,
        report.token_source,
        report.phone_number_source,
        report.state_path,
        report.state_persisted,
        report.updated_unix_ms
    )
}

#[cfg(test)]
mod tests {
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use serde_json::json;

    use super::{
        execute_multi_channel_lifecycle_action, lifecycle_state_path_for_dir,
        load_multi_channel_lifecycle_state, probe_channel_readiness,
        save_multi_channel_lifecycle_state, MultiChannelLifecycleAction,
        MultiChannelLifecycleChannelState, MultiChannelLifecycleCommandConfig,
        MultiChannelLifecycleStateFile,
    };
    use crate::multi_channel_contract::MultiChannelTransport;
    use crate::multi_channel_credentials::{
        MultiChannelCredentialRecord, MultiChannelCredentialStoreSnapshot,
    };
    use std::collections::BTreeMap;
    use std::path::Path;
    use tempfile::tempdir;

    fn test_config(root: &Path) -> MultiChannelLifecycleCommandConfig {
        MultiChannelLifecycleCommandConfig {
            state_dir: root.join(".tau/multi-channel"),
            ingress_dir: root.join(".tau/multi-channel/live-ingress"),
            telegram_api_base: "https://api.telegram.org".to_string(),
            discord_api_base: "https://discord.com/api/v10".to_string(),
            whatsapp_api_base: "https://graph.facebook.com/v20.0".to_string(),
            credential_store: None,
            credential_store_unreadable: false,
            telegram_bot_token: None,
            discord_bot_token: None,
            whatsapp_access_token: None,
            whatsapp_phone_number_id: None,
            probe_online: false,
            probe_online_timeout_ms: 500,
            probe_online_max_attempts: 2,
            probe_online_retry_delay_ms: 5,
        }
    }

    #[test]
    fn unit_probe_channel_readiness_reports_missing_prerequisites() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());
        let report = probe_channel_readiness(&config, MultiChannelTransport::Telegram, true, false);
        assert_eq!(report.readiness_status, "fail");
        assert_eq!(report.probe_mode, "offline");
        assert_eq!(report.online_probe_status, "disabled");
        assert!(report
            .reason_codes
            .contains(&"missing_telegram_bot_token".to_string()));
        assert!(report.reason_codes.contains(&"ingress_missing".to_string()));
    }

    #[test]
    fn unit_probe_channel_readiness_online_mode_requires_credentials() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        config.probe_online = true;
        std::fs::create_dir_all(&config.ingress_dir).expect("mkdir ingress");
        std::fs::write(config.ingress_dir.join("telegram.ndjson"), "").expect("write ingress");

        let report = probe_channel_readiness(&config, MultiChannelTransport::Telegram, true, true);
        assert_eq!(report.probe_mode, "online");
        assert_eq!(report.readiness_status, "fail");
        assert_eq!(report.online_probe_status, "fail");
        assert!(report
            .online_probe_reason_codes
            .contains(&"probe_online_missing_prerequisites".to_string()));
    }

    #[test]
    fn functional_probe_action_online_telegram_reports_ready_with_provider_probe() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let get_me = server.mock(|when, then| {
            when.method(GET).path("/bottelegram-secret/getMe");
            then.status(200)
                .json_body(json!({"ok": true, "result": {"id": 42, "is_bot": true}}));
        });

        let mut config = test_config(temp.path());
        config.probe_online = true;
        config.telegram_bot_token = Some("telegram-secret".to_string());
        config.telegram_api_base = server.base_url();
        std::fs::create_dir_all(&config.ingress_dir).expect("mkdir ingress");
        std::fs::write(config.ingress_dir.join("telegram.ndjson"), "").expect("write ingress");

        let report = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Probe,
            MultiChannelTransport::Telegram,
        )
        .expect("online probe");
        get_me.assert_calls(1);
        assert_eq!(report.probe_mode, "online");
        assert_eq!(report.readiness_status, "pass");
        assert_eq!(report.online_probe_status, "pass");
        assert!(report
            .online_probe_reason_codes
            .contains(&"probe_online_ready".to_string()));
    }

    #[test]
    fn integration_probe_action_online_persists_state_with_online_diagnostics() {
        let temp = tempdir().expect("tempdir");
        let server = MockServer::start();
        let get_me = server.mock(|when, then| {
            when.method(GET).path("/bottelegram-secret/getMe");
            then.status(401).json_body(json!({"ok": false}));
        });

        let mut config = test_config(temp.path());
        config.probe_online = true;
        config.telegram_bot_token = Some("telegram-secret".to_string());
        config.telegram_api_base = server.base_url();
        std::fs::create_dir_all(&config.ingress_dir).expect("mkdir ingress");
        std::fs::write(config.ingress_dir.join("telegram.ndjson"), "").expect("write ingress");

        let report = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Probe,
            MultiChannelTransport::Telegram,
        )
        .expect("online probe");
        get_me.assert_calls(1);
        assert_eq!(report.lifecycle_status, "probe_failed");
        assert_eq!(report.online_probe_status, "fail");
        assert!(report
            .online_probe_reason_codes
            .contains(&"probe_online_telegram_auth_failed".to_string()));
        assert!(!report.remediation_hints.is_empty());

        let state =
            load_multi_channel_lifecycle_state(&lifecycle_state_path_for_dir(&config.state_dir))
                .expect("state");
        let entry = state.channels.get("telegram").expect("telegram state");
        assert_eq!(entry.last_probe_mode, "online");
        assert_eq!(entry.last_online_probe_status, "fail");
        assert!(entry
            .last_online_probe_reason_codes
            .contains(&"probe_online_telegram_auth_failed".to_string()));
        assert!(!entry.last_remediation_hints.is_empty());
    }

    #[test]
    fn regression_probe_action_online_transport_error_fails_closed_with_persisted_state() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        config.probe_online = true;
        config.telegram_bot_token = Some("telegram-secret".to_string());
        config.telegram_api_base = "http://127.0.0.1:9".to_string();
        std::fs::create_dir_all(&config.ingress_dir).expect("mkdir ingress");
        std::fs::write(config.ingress_dir.join("telegram.ndjson"), "").expect("write ingress");

        let report = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Probe,
            MultiChannelTransport::Telegram,
        )
        .expect("probe should fail closed without crashing");
        assert_eq!(report.lifecycle_status, "probe_failed");
        assert_eq!(report.online_probe_status, "fail");
        assert!(
            report
                .online_probe_reason_codes
                .iter()
                .any(|code| code.starts_with("probe_online_telegram_")),
            "expected provider-specific online probe reason code"
        );

        let state_path = lifecycle_state_path_for_dir(&config.state_dir);
        let state_raw = std::fs::read_to_string(&state_path).expect("state file should persist");
        let parsed: serde_json::Value =
            serde_json::from_str(&state_raw).expect("state should remain valid json");
        assert_eq!(parsed["channels"]["telegram"]["last_action"], "probe");
    }

    #[test]
    fn functional_login_action_creates_ingress_and_persists_initialized_state() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        config.telegram_bot_token = Some("telegram-secret".to_string());

        let report = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Login,
            MultiChannelTransport::Telegram,
        )
        .expect("login should succeed");
        assert_eq!(report.lifecycle_status, "initialized");
        assert_eq!(report.readiness_status, "pass");
        assert!(report.ingress_exists);
        assert!(report.ingress_is_file);

        let state_path = lifecycle_state_path_for_dir(&config.state_dir);
        let state = load_multi_channel_lifecycle_state(&state_path).expect("state");
        let entry = state.channels.get("telegram").expect("telegram entry");
        assert_eq!(entry.lifecycle_status, "initialized");
        assert_eq!(entry.last_action, "login");
    }

    #[test]
    fn integration_login_status_logout_probe_flow_roundtrips_channel_state() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        config.discord_bot_token = Some("discord-secret".to_string());

        let login = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Login,
            MultiChannelTransport::Discord,
        )
        .expect("login");
        assert_eq!(login.lifecycle_status, "initialized");

        let status = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Status,
            MultiChannelTransport::Discord,
        )
        .expect("status");
        assert_eq!(status.lifecycle_status, "initialized");
        assert_eq!(status.readiness_status, "pass");

        let logout = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Logout,
            MultiChannelTransport::Discord,
        )
        .expect("logout");
        assert_eq!(logout.lifecycle_status, "logged_out");
        assert_eq!(logout.reason_codes, vec!["logout_requested".to_string()]);

        let probe = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Probe,
            MultiChannelTransport::Discord,
        )
        .expect("probe");
        assert_eq!(probe.lifecycle_status, "ready");
        assert_eq!(probe.readiness_status, "pass");
    }

    #[test]
    fn integration_login_action_resolves_store_backed_secret_when_cli_secret_missing() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        let mut integrations = BTreeMap::new();
        integrations.insert(
            "telegram-bot-token".to_string(),
            MultiChannelCredentialRecord {
                secret: Some("store-telegram-secret".to_string()),
                revoked: false,
            },
        );
        config.credential_store = Some(MultiChannelCredentialStoreSnapshot { integrations });

        let report = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Login,
            MultiChannelTransport::Telegram,
        )
        .expect("login");
        assert_eq!(report.lifecycle_status, "initialized");
        assert_eq!(report.token_source, "credential_store");
    }

    #[test]
    fn regression_action_fails_on_corrupted_lifecycle_state_file() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());
        let state_path = lifecycle_state_path_for_dir(&config.state_dir);
        std::fs::create_dir_all(state_path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&state_path, "{not-json").expect("write corrupted state");

        let error = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Status,
            MultiChannelTransport::Telegram,
        )
        .expect_err("corrupted state should fail");
        assert!(error
            .to_string()
            .contains("failed to parse multi-channel lifecycle state"));
    }

    #[test]
    fn regression_probe_whatsapp_reports_missing_phone_id_when_token_present() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_config(temp.path());
        config.whatsapp_access_token = Some("wa-token".to_string());

        let report = execute_multi_channel_lifecycle_action(
            &config,
            MultiChannelLifecycleAction::Probe,
            MultiChannelTransport::Whatsapp,
        )
        .expect("probe");
        assert_eq!(report.lifecycle_status, "probe_failed");
        assert!(report
            .reason_codes
            .contains(&"missing_whatsapp_phone_number_id".to_string()));
    }

    #[test]
    fn regression_save_and_reload_state_roundtrips_schema_and_channel_rows() {
        let temp = tempdir().expect("tempdir");
        let config = test_config(temp.path());
        let state_path = lifecycle_state_path_for_dir(&config.state_dir);
        let mut channels = BTreeMap::new();
        channels.insert(
            "telegram".to_string(),
            MultiChannelLifecycleChannelState {
                lifecycle_status: "initialized".to_string(),
                reason_codes: vec!["ready".to_string()],
                last_probe_mode: "offline".to_string(),
                last_online_probe_status: "disabled".to_string(),
                last_online_probe_reason_codes: vec!["probe_online_disabled".to_string()],
                last_remediation_hints: Vec::new(),
                last_action: "login".to_string(),
                last_updated_unix_ms: 10,
                last_login_unix_ms: 10,
                last_logout_unix_ms: 0,
                last_probe_unix_ms: 0,
            },
        );
        save_multi_channel_lifecycle_state(
            &state_path,
            &MultiChannelLifecycleStateFile {
                schema_version: 1,
                channels,
            },
        )
        .expect("save");
        let reloaded = load_multi_channel_lifecycle_state(&state_path).expect("reload");
        assert_eq!(reloaded.schema_version, 1);
        assert_eq!(
            reloaded
                .channels
                .get("telegram")
                .expect("channel")
                .lifecycle_status,
            "initialized"
        );
    }
}
