//! Gateway deploy/stop endpoint handlers.
//!
//! This module provides bounded runtime contracts for deploy requests and
//! stop actions backed by deterministic state persisted under gateway state dir.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tau_core::{current_unix_timestamp_ms, write_text_atomic};

use super::{
    authorize_dashboard_request, parse_gateway_json_body, GatewayOpenResponsesServerState,
    OpenResponsesApiError,
};

const DEPLOY_STATE_FILE: &str = "deploy-agent-state.json";
const DEPLOY_STATE_SCHEMA_VERSION: u32 = 1;
const DEPLOY_STATUS_DEPLOYING: &str = "deploying";
const DEPLOY_STATUS_STOPPED: &str = "stopped";
const STOP_REASON_OPERATOR_REQUEST: &str = "operator_stop_request";

#[derive(Debug, Deserialize, Default)]
struct GatewayDeployRequest {
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GatewayDeployAgentRecord {
    agent_id: String,
    status: String,
    profile: String,
    model: String,
    created_unix_ms: u64,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct GatewayDeployStateFile {
    #[serde(default = "deploy_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    agents: BTreeMap<String, GatewayDeployAgentRecord>,
}

impl Default for GatewayDeployStateFile {
    fn default() -> Self {
        Self {
            schema_version: DEPLOY_STATE_SCHEMA_VERSION,
            agents: BTreeMap::new(),
        }
    }
}

fn deploy_state_schema_version() -> u32 {
    DEPLOY_STATE_SCHEMA_VERSION
}

pub(super) async fn handle_gateway_deploy(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayDeployRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let normalized_agent_id = request.agent_id.trim();
    if normalized_agent_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_agent_id",
            "agent_id must be non-empty",
        )
        .into_response();
    }

    let now_unix_ms = current_unix_timestamp_ms();
    let profile = normalize_non_empty(request.profile.as_str(), "default");
    let model = normalize_non_empty(request.model.as_str(), state.config.model.as_str());
    let state_path = gateway_deploy_state_path(&state.config.state_dir);
    let mut deploy_state = match load_gateway_deploy_state(&state_path) {
        Ok(state) => state,
        Err(error) => return error.into_response(),
    };

    let created_unix_ms = deploy_state
        .agents
        .get(normalized_agent_id)
        .map(|existing| existing.created_unix_ms)
        .unwrap_or(now_unix_ms);
    deploy_state.agents.insert(
        normalized_agent_id.to_string(),
        GatewayDeployAgentRecord {
            agent_id: normalized_agent_id.to_string(),
            status: DEPLOY_STATUS_DEPLOYING.to_string(),
            profile: profile.clone(),
            model: model.clone(),
            created_unix_ms,
            updated_unix_ms: now_unix_ms,
        },
    );
    if let Err(error) = save_gateway_deploy_state(&state_path, &deploy_state) {
        return error.into_response();
    }

    if let Err(error) = crate::gateway_runtime::start_gateway_service_mode(&state.config.state_dir)
    {
        return OpenResponsesApiError::internal(format!(
            "failed to transition gateway service state for deploy request: {error}"
        ))
        .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": DEPLOY_STATE_SCHEMA_VERSION,
            "agent_id": normalized_agent_id,
            "status": DEPLOY_STATUS_DEPLOYING,
            "profile": profile,
            "model": model,
            "accepted_unix_ms": now_unix_ms,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_agent_stop(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    AxumPath(agent_id): AxumPath<String>,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let normalized_agent_id = agent_id.trim();
    if normalized_agent_id.is_empty() {
        return OpenResponsesApiError::bad_request(
            "invalid_agent_id",
            "agent_id must be non-empty",
        )
        .into_response();
    }

    let state_path = gateway_deploy_state_path(&state.config.state_dir);
    let mut deploy_state = match load_gateway_deploy_state(&state_path) {
        Ok(state) => state,
        Err(error) => return error.into_response(),
    };
    let now_unix_ms = current_unix_timestamp_ms();
    let Some(record) = deploy_state.agents.get_mut(normalized_agent_id) else {
        return OpenResponsesApiError::not_found(
            "agent_not_found",
            format!("agent '{normalized_agent_id}' was not found"),
        )
        .into_response();
    };
    record.status = DEPLOY_STATUS_STOPPED.to_string();
    record.updated_unix_ms = now_unix_ms;
    if let Err(error) = save_gateway_deploy_state(&state_path, &deploy_state) {
        return error.into_response();
    }

    if let Err(error) = crate::gateway_runtime::stop_gateway_service_mode(
        &state.config.state_dir,
        Some(STOP_REASON_OPERATOR_REQUEST),
    ) {
        return OpenResponsesApiError::internal(format!(
            "failed to transition gateway service state for stop request: {error}"
        ))
        .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": DEPLOY_STATE_SCHEMA_VERSION,
            "agent_id": normalized_agent_id,
            "status": DEPLOY_STATUS_STOPPED,
            "stopped_unix_ms": now_unix_ms,
        })),
    )
        .into_response()
}

fn load_gateway_deploy_state(path: &Path) -> Result<GatewayDeployStateFile, OpenResponsesApiError> {
    if !path.exists() {
        return Ok(GatewayDeployStateFile::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read deploy runtime state '{}': {error}",
            path.display()
        ))
    })?;
    serde_json::from_str::<GatewayDeployStateFile>(&raw).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to parse deploy runtime state '{}': {error}",
            path.display()
        ))
    })
}

fn save_gateway_deploy_state(
    path: &Path,
    state: &GatewayDeployStateFile,
) -> Result<(), OpenResponsesApiError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to create deploy runtime state directory '{}': {error}",
                parent.display()
            ))
        })?;
    }
    let serialized = serde_json::to_string_pretty(state).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to serialize deploy runtime state: {error}"
        ))
    })?;
    write_text_atomic(path, serialized.as_str()).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to persist deploy runtime state '{}': {error}",
            path.display()
        ))
    })
}

fn gateway_deploy_state_path(state_dir: &Path) -> PathBuf {
    state_dir.join(DEPLOY_STATE_FILE)
}

fn normalize_non_empty(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_normalize_non_empty_uses_trimmed_or_fallback() {
        assert_eq!(normalize_non_empty("  value  ", "fallback"), "value");
        assert_eq!(normalize_non_empty("   ", "fallback"), "fallback");
    }

    #[test]
    fn unit_gateway_deploy_state_path_uses_expected_filename() {
        let state_dir = PathBuf::from("/tmp/tau-gateway-tests");
        let path = gateway_deploy_state_path(&state_dir);
        assert!(path.ends_with(DEPLOY_STATE_FILE));
    }
}
