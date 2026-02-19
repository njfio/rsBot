//! Gateway training rollouts/config endpoint handlers.
//!
//! This module provides dashboard-facing training runtime contracts for
//! paginated rollout history and persisted config override updates.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_core::current_unix_timestamp_ms;

use super::{
    authorize_dashboard_request, parse_gateway_json_body, read_gateway_config_pending_overrides,
    GatewayOpenResponsesServerState, OpenResponsesApiError,
};

const TRAINING_ROLLOUTS_FILE: &str = "rollouts.jsonl";
const TRAINING_ROLLOUTS_DEFAULT_PER_PAGE: usize = 50;
const TRAINING_ROLLOUTS_MAX_PER_PAGE: usize = 200;
const TRAINING_CONFIG_OVERRIDES_FILE: &str = "training-config-overrides.json";

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayTrainingRolloutsQuery {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    per_page: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GatewayTrainingConfigPatchRequest {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    update_interval_rollouts: Option<u64>,
    #[serde(default)]
    max_rollouts_per_update: Option<u64>,
    #[serde(default)]
    max_failure_streak: Option<u64>,
    #[serde(default)]
    store_path: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GatewayTrainingRolloutLine {
    #[serde(default)]
    rollout_id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    steps: u64,
    #[serde(default)]
    reward: f64,
    #[serde(default)]
    duration_ms: u64,
    #[serde(default)]
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayTrainingRolloutView {
    rollout_id: String,
    status: String,
    mode: String,
    steps: u64,
    reward: f64,
    duration_ms: u64,
    updated_unix_ms: u64,
}

#[derive(Debug, Default)]
struct GatewayTrainingRolloutLoad {
    records: Vec<GatewayTrainingRolloutView>,
    invalid_records: u64,
    diagnostics: Vec<String>,
}

#[derive(Debug)]
struct GatewayTrainingRolloutRequest {
    page: usize,
    per_page: usize,
}

pub(super) async fn handle_gateway_training_rollouts(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    Query(query): Query<GatewayTrainingRolloutsQuery>,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }

    let request = match parse_rollout_request(&query) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };

    let load = match load_training_rollouts(&state.config.state_dir) {
        Ok(load) => load,
        Err(error) => return error.into_response(),
    };

    let total_records = load.records.len();
    let total_pages = if total_records == 0 {
        0
    } else {
        (total_records
            .saturating_add(request.per_page)
            .saturating_sub(1))
            / request.per_page
    };
    let start = request
        .page
        .saturating_sub(1)
        .saturating_mul(request.per_page);
    let records: Vec<GatewayTrainingRolloutView> = if start >= load.records.len() {
        Vec::new()
    } else {
        load.records
            .iter()
            .skip(start)
            .take(request.per_page)
            .cloned()
            .collect()
    };

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "generated_unix_ms": current_unix_timestamp_ms(),
            "page": request.page,
            "per_page": request.per_page,
            "total_records": total_records,
            "total_pages": total_pages,
            "invalid_records": load.invalid_records,
            "records": records,
            "diagnostics": load.diagnostics,
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_training_config_patch(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(error) = authorize_dashboard_request(&state, &headers) {
        return error.into_response();
    }
    let request = match parse_gateway_json_body::<GatewayTrainingConfigPatchRequest>(&body) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };

    let overrides_path = gateway_training_config_overrides_path(&state.config.state_dir);
    let mut pending_overrides = match read_gateway_config_pending_overrides(&overrides_path) {
        Ok(overrides) => overrides,
        Err(error) => return error.into_response(),
    };
    let mut accepted = serde_json::Map::<String, Value>::new();
    let mut applied = serde_json::Map::<String, Value>::new();

    if let Some(enabled) = request.enabled {
        accepted.insert("enabled".to_string(), json!(enabled));
        pending_overrides.insert("enabled".to_string(), json!(enabled));
        applied.insert(
            "enabled".to_string(),
            json!({"mode": "persisted_pending", "value": enabled}),
        );
    }

    if let Some(update_interval_rollouts) = request.update_interval_rollouts {
        if update_interval_rollouts == 0 {
            return OpenResponsesApiError::bad_request(
                "invalid_training_update_interval_rollouts",
                "update_interval_rollouts must be greater than zero",
            )
            .into_response();
        }
        accepted.insert(
            "update_interval_rollouts".to_string(),
            json!(update_interval_rollouts),
        );
        pending_overrides.insert(
            "update_interval_rollouts".to_string(),
            json!(update_interval_rollouts),
        );
        applied.insert(
            "update_interval_rollouts".to_string(),
            json!({"mode": "persisted_pending", "value": update_interval_rollouts}),
        );
    }

    if let Some(max_rollouts_per_update) = request.max_rollouts_per_update {
        if max_rollouts_per_update == 0 {
            return OpenResponsesApiError::bad_request(
                "invalid_training_max_rollouts_per_update",
                "max_rollouts_per_update must be greater than zero",
            )
            .into_response();
        }
        accepted.insert(
            "max_rollouts_per_update".to_string(),
            json!(max_rollouts_per_update),
        );
        pending_overrides.insert(
            "max_rollouts_per_update".to_string(),
            json!(max_rollouts_per_update),
        );
        applied.insert(
            "max_rollouts_per_update".to_string(),
            json!({"mode": "persisted_pending", "value": max_rollouts_per_update}),
        );
    }

    if let Some(max_failure_streak) = request.max_failure_streak {
        if max_failure_streak == 0 {
            return OpenResponsesApiError::bad_request(
                "invalid_training_max_failure_streak",
                "max_failure_streak must be greater than zero",
            )
            .into_response();
        }
        accepted.insert("max_failure_streak".to_string(), json!(max_failure_streak));
        pending_overrides.insert("max_failure_streak".to_string(), json!(max_failure_streak));
        applied.insert(
            "max_failure_streak".to_string(),
            json!({"mode": "persisted_pending", "value": max_failure_streak}),
        );
    }

    if let Some(store_path) = request.store_path {
        let trimmed = store_path.trim().to_string();
        if trimmed.is_empty() {
            return OpenResponsesApiError::bad_request(
                "invalid_training_store_path",
                "store_path must be non-empty",
            )
            .into_response();
        }
        accepted.insert("store_path".to_string(), json!(trimmed.clone()));
        pending_overrides.insert("store_path".to_string(), json!(trimmed.clone()));
        applied.insert(
            "store_path".to_string(),
            json!({"mode": "persisted_pending", "value": trimmed}),
        );
    }

    if accepted.is_empty() {
        return OpenResponsesApiError::bad_request(
            "no_training_config_changes",
            "patch payload did not include any supported training config fields",
        )
        .into_response();
    }

    let updated_unix_ms = current_unix_timestamp_ms();
    let payload = json!({
        "schema_version": 1,
        "updated_unix_ms": updated_unix_ms,
        "pending_overrides": pending_overrides,
    });
    if let Some(parent) = overrides_path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                return OpenResponsesApiError::internal(format!(
                    "failed to create training config override directory '{}': {error}",
                    parent.display()
                ))
                .into_response();
            }
        }
    }
    if let Err(error) = std::fs::write(&overrides_path, format!("{payload}\n").as_bytes()) {
        return OpenResponsesApiError::internal(format!(
            "failed to write training config overrides '{}': {error}",
            overrides_path.display()
        ))
        .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "accepted": accepted,
            "applied": applied,
            "pending_overrides": payload["pending_overrides"],
            "overrides_path": overrides_path.display().to_string(),
            "updated_unix_ms": updated_unix_ms,
        })),
    )
        .into_response()
}

fn parse_rollout_request(
    query: &GatewayTrainingRolloutsQuery,
) -> Result<GatewayTrainingRolloutRequest, OpenResponsesApiError> {
    let page =
        parse_optional_usize(query.page.as_deref(), "invalid_training_rollouts_page")?.unwrap_or(1);
    if page == 0 {
        return Err(OpenResponsesApiError::bad_request(
            "invalid_training_rollouts_page",
            "page must be >= 1",
        ));
    }

    let per_page = parse_optional_usize(
        query.per_page.as_deref(),
        "invalid_training_rollouts_per_page",
    )?
    .unwrap_or(TRAINING_ROLLOUTS_DEFAULT_PER_PAGE);
    if per_page == 0 || per_page > TRAINING_ROLLOUTS_MAX_PER_PAGE {
        return Err(OpenResponsesApiError::bad_request(
            "invalid_training_rollouts_per_page",
            format!(
                "per_page must be between 1 and {}",
                TRAINING_ROLLOUTS_MAX_PER_PAGE
            ),
        ));
    }

    Ok(GatewayTrainingRolloutRequest { page, per_page })
}

fn parse_optional_usize(
    raw: Option<&str>,
    code: &'static str,
) -> Result<Option<usize>, OpenResponsesApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    raw.parse::<usize>().map(Some).map_err(|_| {
        OpenResponsesApiError::bad_request(code, format!("invalid integer value '{}'", raw))
    })
}

fn load_training_rollouts(
    gateway_state_dir: &Path,
) -> Result<GatewayTrainingRolloutLoad, OpenResponsesApiError> {
    let path = gateway_training_rollouts_path(gateway_state_dir);
    if !path.exists() {
        return Ok(GatewayTrainingRolloutLoad {
            diagnostics: vec![format!("training_rollouts_missing:{}", path.display())],
            ..GatewayTrainingRolloutLoad::default()
        });
    }

    let raw = std::fs::read_to_string(&path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read training rollouts '{}': {error}",
            path.display()
        ))
    })?;

    let mut load = GatewayTrainingRolloutLoad::default();
    for (line_number, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayTrainingRolloutLine>(trimmed) {
            Ok(parsed) => load.records.push(GatewayTrainingRolloutView {
                rollout_id: normalize_non_empty(parsed.rollout_id.as_str(), "unknown"),
                status: normalize_non_empty(parsed.status.as_str(), "unknown"),
                mode: normalize_non_empty(parsed.mode.as_str(), "unknown"),
                steps: parsed.steps,
                reward: parsed.reward,
                duration_ms: parsed.duration_ms,
                updated_unix_ms: parsed.updated_unix_ms,
            }),
            Err(_) => {
                load.invalid_records = load.invalid_records.saturating_add(1);
                load.diagnostics.push(format!(
                    "training_rollouts_malformed_line:{}",
                    line_number.saturating_add(1)
                ));
            }
        }
    }

    load.records.sort_by(|left, right| {
        right
            .updated_unix_ms
            .cmp(&left.updated_unix_ms)
            .then_with(|| right.rollout_id.cmp(&left.rollout_id))
    });

    Ok(load)
}

fn normalize_non_empty(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn gateway_training_rollouts_path(state_dir: &Path) -> PathBuf {
    let tau_root = state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| state_dir.to_path_buf());
    tau_root.join("training").join(TRAINING_ROLLOUTS_FILE)
}

fn gateway_training_config_overrides_path(state_dir: &Path) -> PathBuf {
    state_dir
        .join("openresponses")
        .join(TRAINING_CONFIG_OVERRIDES_FILE)
}
