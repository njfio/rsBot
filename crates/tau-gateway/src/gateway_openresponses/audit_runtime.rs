//! Gateway audit summary/log endpoint handlers.
//!
//! This module merges dashboard action audit records and UI telemetry records
//! into a shared query surface for operator diagnostics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::dashboard_status::GatewayDashboardActionAuditRecord;
use super::{authorize_and_enforce_gateway_limits, gateway_ui_telemetry_path};
use super::{GatewayOpenResponsesServerState, OpenResponsesApiError};

const AUDIT_SOURCE_DASHBOARD_ACTION: &str = "dashboard_action";
const AUDIT_SOURCE_UI_TELEMETRY: &str = "ui_telemetry";
const AUDIT_LOG_DEFAULT_PAGE_SIZE: usize = 50;
const AUDIT_LOG_MAX_PAGE_SIZE: usize = 200;

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayAuditSummaryQuery {
    #[serde(default)]
    since_unix_ms: Option<String>,
    #[serde(default)]
    until_unix_ms: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct GatewayAuditLogQuery {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    page_size: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    reason_code: Option<String>,
    #[serde(default)]
    since_unix_ms: Option<String>,
    #[serde(default)]
    until_unix_ms: Option<String>,
}

#[derive(Debug, Clone)]
struct GatewayAuditFilters {
    source: Option<String>,
    action: Option<String>,
    view: Option<String>,
    reason_code: Option<String>,
    since_unix_ms: Option<u64>,
    until_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayAuditRecordView {
    source: String,
    timestamp_unix_ms: u64,
    view: String,
    action: String,
    reason_code: String,
    principal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Clone)]
struct GatewayAuditRecord {
    view: GatewayAuditRecordView,
}

#[derive(Debug, Default)]
struct GatewayAuditSourceLoad {
    records: Vec<GatewayAuditRecord>,
    invalid_records: u64,
}

#[derive(Debug, Default)]
struct GatewayMergedAuditLoad {
    records: Vec<GatewayAuditRecord>,
    invalid_records_total: u64,
    source_invalid_counts: BTreeMap<String, u64>,
}

#[derive(Debug)]
struct GatewayAuditLogRequest {
    filters: GatewayAuditFilters,
    page: usize,
    page_size: usize,
}

#[derive(Debug, Deserialize, Default)]
struct GatewayUiTelemetryAuditRecord {
    #[serde(default)]
    timestamp_unix_ms: u64,
    #[serde(default)]
    view: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    reason_code: String,
    #[serde(default)]
    session_key: String,
    #[serde(default)]
    principal: String,
    #[serde(default)]
    metadata: Value,
}

pub(super) async fn handle_gateway_audit_summary(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    Query(query): Query<GatewayAuditSummaryQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let filters = match parse_summary_filters(&query) {
        Ok(filters) => filters,
        Err(error) => return error.into_response(),
    };
    let load = match load_merged_audit_records(&state.config.state_dir) {
        Ok(load) => load,
        Err(error) => return error.into_response(),
    };

    let filtered_records: Vec<&GatewayAuditRecord> = load
        .records
        .iter()
        .filter(|record| record_matches_filters(record, &filters))
        .collect();

    let mut source_counts = BTreeMap::new();
    let mut action_counts = BTreeMap::new();
    let mut view_counts = BTreeMap::new();
    let mut reason_code_counts = BTreeMap::new();
    let mut actor_counts = BTreeMap::new();
    let mut latest_timestamp_unix_ms = None;
    for record in &filtered_records {
        increment_u64_counter(&mut source_counts, record.view.source.as_str());
        increment_u64_counter(&mut action_counts, record.view.action.as_str());
        increment_u64_counter(&mut view_counts, record.view.view.as_str());
        increment_u64_counter(&mut reason_code_counts, record.view.reason_code.as_str());
        increment_u64_counter(&mut actor_counts, record.view.principal.as_str());
        latest_timestamp_unix_ms = Some(
            latest_timestamp_unix_ms
                .unwrap_or(0)
                .max(record.view.timestamp_unix_ms),
        );
    }

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "records_total": filtered_records.len(),
            "invalid_records_total": load.invalid_records_total,
            "source_counts": source_counts,
            "source_invalid_counts": load.source_invalid_counts,
            "action_counts": action_counts,
            "view_counts": view_counts,
            "reason_code_counts": reason_code_counts,
            "actor_counts": actor_counts,
            "latest_timestamp_unix_ms": latest_timestamp_unix_ms,
            "window": {
                "since_unix_ms": filters.since_unix_ms,
                "until_unix_ms": filters.until_unix_ms,
            }
        })),
    )
        .into_response()
}

pub(super) async fn handle_gateway_audit_log(
    State(state): State<Arc<GatewayOpenResponsesServerState>>,
    headers: HeaderMap,
    Query(query): Query<GatewayAuditLogQuery>,
) -> Response {
    if let Err(error) = authorize_and_enforce_gateway_limits(&state, &headers) {
        return error.into_response();
    }

    let request = match parse_log_request(&query) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let load = match load_merged_audit_records(&state.config.state_dir) {
        Ok(load) => load,
        Err(error) => return error.into_response(),
    };

    let total_records = load.records.len();
    let filtered: Vec<GatewayAuditRecordView> = load
        .records
        .iter()
        .filter(|record| record_matches_filters(record, &request.filters))
        .map(|record| record.view.clone())
        .collect();
    let filtered_records = filtered.len();

    let start = request
        .page
        .saturating_sub(1)
        .saturating_mul(request.page_size);
    let items: Vec<GatewayAuditRecordView> = if start >= filtered.len() {
        Vec::new()
    } else {
        filtered
            .into_iter()
            .skip(start)
            .take(request.page_size)
            .collect()
    };

    (
        StatusCode::OK,
        Json(json!({
            "schema_version": 1,
            "page": request.page,
            "page_size": request.page_size,
            "total_records": total_records,
            "filtered_records": filtered_records,
            "invalid_records_total": load.invalid_records_total,
            "source_invalid_counts": load.source_invalid_counts,
            "items": items,
        })),
    )
        .into_response()
}

fn parse_summary_filters(
    query: &GatewayAuditSummaryQuery,
) -> Result<GatewayAuditFilters, OpenResponsesApiError> {
    let since_unix_ms = parse_optional_u64(query.since_unix_ms.as_deref(), "invalid_audit_window")?;
    let until_unix_ms = parse_optional_u64(query.until_unix_ms.as_deref(), "invalid_audit_window")?;
    validate_window(since_unix_ms, until_unix_ms)?;
    Ok(GatewayAuditFilters {
        source: None,
        action: None,
        view: None,
        reason_code: None,
        since_unix_ms,
        until_unix_ms,
    })
}

fn parse_log_request(
    query: &GatewayAuditLogQuery,
) -> Result<GatewayAuditLogRequest, OpenResponsesApiError> {
    let page = parse_optional_usize(query.page.as_deref(), "invalid_audit_page")?.unwrap_or(1);
    if page == 0 {
        return Err(OpenResponsesApiError::bad_request(
            "invalid_audit_page",
            "page must be >= 1",
        ));
    }

    let page_size = parse_optional_usize(query.page_size.as_deref(), "invalid_audit_page_size")?
        .unwrap_or(AUDIT_LOG_DEFAULT_PAGE_SIZE);
    if page_size == 0 || page_size > AUDIT_LOG_MAX_PAGE_SIZE {
        return Err(OpenResponsesApiError::bad_request(
            "invalid_audit_page_size",
            format!(
                "page_size must be between 1 and {}",
                AUDIT_LOG_MAX_PAGE_SIZE
            ),
        ));
    }

    let source = normalize_filter_field(query.source.as_deref());
    if let Some(value) = source.as_deref() {
        if value != AUDIT_SOURCE_DASHBOARD_ACTION && value != AUDIT_SOURCE_UI_TELEMETRY {
            return Err(OpenResponsesApiError::bad_request(
                "invalid_audit_source",
                "source must be one of: dashboard_action, ui_telemetry",
            ));
        }
    }

    let action = normalize_filter_field(query.action.as_deref());
    let view = normalize_filter_field(query.view.as_deref());
    let reason_code = normalize_filter_field(query.reason_code.as_deref());
    let since_unix_ms = parse_optional_u64(query.since_unix_ms.as_deref(), "invalid_audit_window")?;
    let until_unix_ms = parse_optional_u64(query.until_unix_ms.as_deref(), "invalid_audit_window")?;
    validate_window(since_unix_ms, until_unix_ms)?;

    Ok(GatewayAuditLogRequest {
        filters: GatewayAuditFilters {
            source,
            action,
            view,
            reason_code,
            since_unix_ms,
            until_unix_ms,
        },
        page,
        page_size,
    })
}

fn validate_window(since: Option<u64>, until: Option<u64>) -> Result<(), OpenResponsesApiError> {
    if let (Some(since_unix_ms), Some(until_unix_ms)) = (since, until) {
        if since_unix_ms > until_unix_ms {
            return Err(OpenResponsesApiError::bad_request(
                "invalid_audit_window",
                "since_unix_ms must be <= until_unix_ms",
            ));
        }
    }
    Ok(())
}

fn parse_optional_u64(
    raw: Option<&str>,
    code: &'static str,
) -> Result<Option<u64>, OpenResponsesApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    raw.parse::<u64>().map(Some).map_err(|_| {
        OpenResponsesApiError::bad_request(code, format!("invalid integer value '{}'", raw))
    })
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

fn normalize_filter_field(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn record_matches_filters(record: &GatewayAuditRecord, filters: &GatewayAuditFilters) -> bool {
    if let Some(source) = filters.source.as_deref() {
        if record.view.source.as_str() != source {
            return false;
        }
    }
    if let Some(action) = filters.action.as_deref() {
        if !record.view.action.eq_ignore_ascii_case(action) {
            return false;
        }
    }
    if let Some(view) = filters.view.as_deref() {
        if !record.view.view.eq_ignore_ascii_case(view) {
            return false;
        }
    }
    if let Some(reason_code) = filters.reason_code.as_deref() {
        if !record.view.reason_code.eq_ignore_ascii_case(reason_code) {
            return false;
        }
    }
    if let Some(since_unix_ms) = filters.since_unix_ms {
        if record.view.timestamp_unix_ms < since_unix_ms {
            return false;
        }
    }
    if let Some(until_unix_ms) = filters.until_unix_ms {
        if record.view.timestamp_unix_ms > until_unix_ms {
            return false;
        }
    }
    true
}

fn load_merged_audit_records(
    gateway_state_dir: &Path,
) -> Result<GatewayMergedAuditLoad, OpenResponsesApiError> {
    let dashboard = load_dashboard_action_records(gateway_state_dir)?;
    let telemetry = load_ui_telemetry_records(gateway_state_dir)?;

    let mut records = dashboard.records;
    records.extend(telemetry.records);
    records.sort_by(|left, right| {
        right
            .view
            .timestamp_unix_ms
            .cmp(&left.view.timestamp_unix_ms)
            .then_with(|| left.view.source.cmp(&right.view.source))
            .then_with(|| left.view.action.cmp(&right.view.action))
            .then_with(|| left.view.reason_code.cmp(&right.view.reason_code))
            .then_with(|| left.view.principal.cmp(&right.view.principal))
    });

    let mut source_invalid_counts = BTreeMap::new();
    source_invalid_counts.insert(
        AUDIT_SOURCE_DASHBOARD_ACTION.to_string(),
        dashboard.invalid_records,
    );
    source_invalid_counts.insert(
        AUDIT_SOURCE_UI_TELEMETRY.to_string(),
        telemetry.invalid_records,
    );

    Ok(GatewayMergedAuditLoad {
        records,
        invalid_records_total: dashboard.invalid_records + telemetry.invalid_records,
        source_invalid_counts,
    })
}

fn load_dashboard_action_records(
    gateway_state_dir: &Path,
) -> Result<GatewayAuditSourceLoad, OpenResponsesApiError> {
    let path = gateway_dashboard_actions_log_path(gateway_state_dir);
    if !path.exists() {
        return Ok(GatewayAuditSourceLoad::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read dashboard action audit '{}': {error}",
            path.display()
        ))
    })?;

    let mut load = GatewayAuditSourceLoad::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayDashboardActionAuditRecord>(trimmed) {
            Ok(parsed) => {
                let action = normalize_non_empty(parsed.action.as_str(), "unknown");
                let principal = normalize_non_empty(parsed.actor.as_str(), "unknown");
                let reason_code = format!(
                    "dashboard_action.{}",
                    sanitize_reason_code_segment(action.as_str())
                );
                let metadata = json!({
                    "reason": parsed.reason,
                    "control_mode": parsed.control_mode,
                });
                load.records.push(GatewayAuditRecord {
                    view: GatewayAuditRecordView {
                        source: AUDIT_SOURCE_DASHBOARD_ACTION.to_string(),
                        timestamp_unix_ms: parsed.timestamp_unix_ms,
                        view: "dashboard".to_string(),
                        action,
                        reason_code,
                        principal,
                        request_id: optional_non_empty(parsed.request_id.as_str()),
                        session_key: None,
                        status: optional_non_empty(parsed.status.as_str()),
                        metadata,
                    },
                });
            }
            Err(_) => {
                load.invalid_records = load.invalid_records.saturating_add(1);
            }
        }
    }
    Ok(load)
}

fn load_ui_telemetry_records(
    gateway_state_dir: &Path,
) -> Result<GatewayAuditSourceLoad, OpenResponsesApiError> {
    let path = gateway_ui_telemetry_path(gateway_state_dir);
    if !path.exists() {
        return Ok(GatewayAuditSourceLoad::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to read ui telemetry log '{}': {error}",
            path.display()
        ))
    })?;

    let mut load = GatewayAuditSourceLoad::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayUiTelemetryAuditRecord>(trimmed) {
            Ok(parsed) => {
                let metadata = if parsed.metadata.is_null() {
                    json!({})
                } else {
                    parsed.metadata
                };
                load.records.push(GatewayAuditRecord {
                    view: GatewayAuditRecordView {
                        source: AUDIT_SOURCE_UI_TELEMETRY.to_string(),
                        timestamp_unix_ms: parsed.timestamp_unix_ms,
                        view: normalize_non_empty(parsed.view.as_str(), "unknown"),
                        action: normalize_non_empty(parsed.action.as_str(), "unknown"),
                        reason_code: normalize_non_empty(parsed.reason_code.as_str(), "ui_event"),
                        principal: normalize_non_empty(parsed.principal.as_str(), "unknown"),
                        request_id: None,
                        session_key: optional_non_empty(parsed.session_key.as_str()),
                        status: None,
                        metadata,
                    },
                });
            }
            Err(_) => {
                load.invalid_records = load.invalid_records.saturating_add(1);
            }
        }
    }
    Ok(load)
}

fn gateway_dashboard_actions_log_path(gateway_state_dir: &Path) -> PathBuf {
    let tau_root = gateway_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| gateway_state_dir.to_path_buf());
    tau_root.join("dashboard").join("actions-audit.jsonl")
}

fn sanitize_reason_code_segment(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    let mut normalized = String::with_capacity(lower.len());
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
        } else {
            normalized.push('_');
        }
    }
    let trimmed = normalized.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_non_empty(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn optional_non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn increment_u64_counter(counters: &mut BTreeMap<String, u64>, key: &str) {
    *counters.entry(key.to_string()).or_insert(0) += 1;
}
