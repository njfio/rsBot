use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use tau_core::current_unix_timestamp_ms;
use tau_multi_channel::multi_channel_contract::MultiChannelTransport;
use tau_multi_channel::multi_channel_lifecycle::{
    default_probe_max_attempts, default_probe_retry_delay_ms, default_probe_timeout_ms,
    execute_multi_channel_lifecycle_action, MultiChannelLifecycleAction,
    MultiChannelLifecycleCommandConfig,
};

use super::types::{GatewayChannelLifecycleRequest, GatewayUiTelemetryRequest};
use super::{
    authorize_and_enforce_gateway_limits, parse_gateway_json_body, sanitize_session_key,
    GatewayOpenResponsesServerState, OpenResponsesApiError, DEFAULT_SESSION_KEY,
};

pub(super) async fn handle_gateway_channel_lifecycle_action(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(channel): AxumPath<String>,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayChannelLifecycleRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let channel = match parse_gateway_channel_transport(channel.as_str()) {
        Ok(channel) => channel,
        Err(error) => return error.into_response(),
    };
    let action_label = request.action.trim().to_ascii_lowercase();
    let action = match parse_gateway_channel_lifecycle_action(action_label.as_str()) {
        Ok(action) => action,
        Err(error) => return error.into_response(),
    };

    let command_config =
        build_gateway_multi_channel_lifecycle_command_config(&state.config.state_dir, &request);
    match execute_multi_channel_lifecycle_action(&command_config, action, channel) {
        Ok(report) => {
            let reason_code = format!("channel_lifecycle_action_{}_applied", action_label);
            state.record_ui_telemetry_event("channels", "lifecycle_action", reason_code.as_str());
            (
                StatusCode::OK,
                Json(json!({
                    "channel": channel.as_str(),
                    "action": action_label,
                    "report": report,
                })),
            )
                .into_response()
        }
        Err(error) => OpenResponsesApiError::internal(format!(
            "failed to execute channel lifecycle action: {error}"
        ))
        .into_response(),
    }
}

pub(super) async fn handle_gateway_ui_telemetry(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let principal = match authorize_and_enforce_gateway_limits(&state, &headers) {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };

    let request = match parse_gateway_json_body::<GatewayUiTelemetryRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let view = request.view.trim();
    let action = request.action.trim();
    if view.is_empty() || action.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_telemetry",
            "view and action must be non-empty",
        )
        .into_response();
    }
    let reason_code = request
        .reason_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("ui_event");
    let session_key = request
        .session_key
        .as_deref()
        .map(sanitize_session_key)
        .unwrap_or_else(|| DEFAULT_SESSION_KEY.to_string());

    let event = json!({
        "timestamp_unix_ms": current_unix_timestamp_ms(),
        "view": view,
        "action": action,
        "reason_code": reason_code,
        "session_key": session_key,
        "principal": principal,
        "metadata": request.metadata,
    });
    let telemetry_path = gateway_ui_telemetry_path(&state.config.state_dir);
    if let Err(error) = append_jsonl_record(&telemetry_path, &event) {
        return OpenResponsesApiError::internal(format!(
            "failed to append ui telemetry '{}': {error}",
            telemetry_path.display()
        ))
        .into_response();
    }

    state.record_ui_telemetry_event(view, action, reason_code);
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "accepted": true,
            "reason_code": reason_code,
        })),
    )
        .into_response()
}

fn parse_gateway_channel_transport(
    raw: &str,
) -> Result<MultiChannelTransport, OpenResponsesApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "telegram" => Ok(MultiChannelTransport::Telegram),
        "discord" => Ok(MultiChannelTransport::Discord),
        "whatsapp" => Ok(MultiChannelTransport::Whatsapp),
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_channel",
            "channel must be one of: telegram, discord, whatsapp",
        )),
    }
}

fn parse_gateway_channel_lifecycle_action(
    raw: &str,
) -> Result<MultiChannelLifecycleAction, OpenResponsesApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "status" => Ok(MultiChannelLifecycleAction::Status),
        "login" => Ok(MultiChannelLifecycleAction::Login),
        "logout" => Ok(MultiChannelLifecycleAction::Logout),
        "probe" => Ok(MultiChannelLifecycleAction::Probe),
        _ => Err(OpenResponsesApiError::bad_request(
            "invalid_lifecycle_action",
            "action must be one of: status, login, logout, probe",
        )),
    }
}

fn build_gateway_multi_channel_lifecycle_command_config(
    gateway_state_dir: &Path,
    request: &GatewayChannelLifecycleRequest,
) -> MultiChannelLifecycleCommandConfig {
    let tau_root = gateway_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| gateway_state_dir.to_path_buf());
    let probe_online = request.probe_online.unwrap_or(false);
    let probe_online_timeout_ms = request
        .probe_online_timeout_ms
        .unwrap_or(default_probe_timeout_ms())
        .clamp(100, 30_000);
    let probe_online_max_attempts = request
        .probe_online_max_attempts
        .unwrap_or(default_probe_max_attempts())
        .clamp(1, 5);
    let probe_online_retry_delay_ms = request
        .probe_online_retry_delay_ms
        .unwrap_or(default_probe_retry_delay_ms())
        .clamp(25, 5_000);

    MultiChannelLifecycleCommandConfig {
        state_dir: tau_root.join("multi-channel"),
        ingress_dir: tau_root.join("channel-store"),
        telegram_api_base: "https://api.telegram.org".to_string(),
        discord_api_base: "https://discord.com/api/v10".to_string(),
        whatsapp_api_base: "https://graph.facebook.com/v20.0".to_string(),
        credential_store: None,
        credential_store_unreadable: false,
        telegram_bot_token: None,
        discord_bot_token: None,
        whatsapp_access_token: None,
        whatsapp_phone_number_id: None,
        probe_online,
        probe_online_timeout_ms,
        probe_online_max_attempts,
        probe_online_retry_delay_ms,
    }
}

pub(super) fn gateway_ui_telemetry_path(state_dir: &Path) -> PathBuf {
    state_dir.join("openresponses").join("ui-telemetry.jsonl")
}

pub(super) fn append_jsonl_record(path: &Path, record: &Value) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(record.to_string().as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to write newline {}", path.display()))?;
    Ok(())
}
