//! Dashboard backend status and control helpers for gateway API surfaces.
use super::*;

const DASHBOARD_SCHEMA_VERSION: u32 = 1;
const DASHBOARD_STATE_FILE: &str = "state.json";
const DASHBOARD_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const DASHBOARD_ACTIONS_LOG_FILE: &str = "actions-audit.jsonl";
const DASHBOARD_CONTROL_STATE_FILE: &str = "control-state.json";
const TRAINING_STATUS_FILE: &str = "status.json";
const DASHBOARD_TIMELINE_CAP: usize = 64;
const DASHBOARD_ACTIONS_TAIL_CAP: usize = 5;
const DASHBOARD_CONTROL_MODE_RUNNING: &str = "running";
const DASHBOARD_CONTROL_MODE_PAUSED: &str = "paused";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardSnapshot {
    pub(super) schema_version: u32,
    pub(super) generated_unix_ms: u64,
    pub(super) state: GatewayDashboardStateMeta,
    pub(super) health: GatewayDashboardHealthReport,
    pub(super) training: GatewayDashboardTrainingReport,
    pub(super) widgets: Vec<GatewayDashboardWidgetView>,
    pub(super) queue_timeline: GatewayDashboardQueueTimelineReport,
    pub(super) alerts: Vec<GatewayDashboardAlert>,
    pub(super) control: GatewayDashboardControlReport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardStateMeta {
    pub(super) dashboard_root: String,
    pub(super) state_path: String,
    pub(super) events_log_path: String,
    pub(super) actions_log_path: String,
    pub(super) control_state_path: String,
    pub(super) state_present: bool,
    pub(super) events_log_present: bool,
    pub(super) actions_log_present: bool,
    pub(super) control_state_present: bool,
    pub(super) cycle_reports: usize,
    pub(super) invalid_cycle_reports: usize,
    pub(super) action_audit_records: usize,
    pub(super) invalid_action_audit_records: usize,
    pub(super) last_reason_codes: Vec<String>,
    pub(super) diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardHealthReport {
    pub(super) health_state: String,
    pub(super) health_reason: String,
    pub(super) rollout_gate: String,
    pub(super) queue_depth: usize,
    pub(super) failure_streak: usize,
    pub(super) last_cycle_failed: usize,
    pub(super) last_cycle_completed: usize,
    pub(super) processed_case_count: usize,
    pub(super) control_audit_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardTrainingReport {
    pub(super) status_path: String,
    pub(super) status_present: bool,
    pub(super) updated_unix_ms: u64,
    pub(super) run_state: String,
    pub(super) model_ref: String,
    pub(super) store_path: String,
    pub(super) total_rollouts: usize,
    pub(super) succeeded: usize,
    pub(super) failed: usize,
    pub(super) cancelled: usize,
    pub(super) diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub(super) struct GatewayDashboardWidgetView {
    #[serde(default)]
    pub(super) widget_id: String,
    #[serde(default)]
    pub(super) kind: String,
    #[serde(default)]
    pub(super) title: String,
    #[serde(default)]
    pub(super) query_key: String,
    #[serde(default)]
    pub(super) refresh_interval_ms: u64,
    #[serde(default)]
    pub(super) last_case_key: String,
    #[serde(default)]
    pub(super) updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardQueueTimelineReport {
    pub(super) cycle_reports: usize,
    pub(super) invalid_cycle_reports: usize,
    pub(super) last_reason_codes: Vec<String>,
    pub(super) reason_code_counts: BTreeMap<String, usize>,
    pub(super) recent_cycles: Vec<GatewayDashboardCycleSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardCycleSummary {
    pub(super) timestamp_unix_ms: u64,
    pub(super) health_state: String,
    pub(super) health_reason: String,
    pub(super) reason_codes: Vec<String>,
    pub(super) discovered_cases: usize,
    pub(super) queued_cases: usize,
    pub(super) backlog_cases: usize,
    pub(super) applied_cases: usize,
    pub(super) malformed_cases: usize,
    pub(super) retryable_failures: usize,
    pub(super) failed_cases: usize,
    pub(super) control_actions_applied: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardAlert {
    pub(super) code: String,
    pub(super) severity: String,
    pub(super) message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardControlReport {
    pub(super) mode: String,
    pub(super) paused: bool,
    pub(super) allowed_actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) last_action: Option<GatewayDashboardActionAuditRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct GatewayDashboardActionRequest {
    pub(super) action: String,
    #[serde(default)]
    pub(super) reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardActionResult {
    pub(super) schema_version: u32,
    pub(super) request_id: String,
    pub(super) action: String,
    pub(super) actor: String,
    pub(super) reason: String,
    pub(super) status: String,
    pub(super) timestamp_unix_ms: u64,
    pub(super) control_mode: String,
    pub(super) health_state: String,
    pub(super) health_reason: String,
    pub(super) rollout_gate: String,
    pub(super) actions_log_path: String,
    pub(super) control_state_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct GatewayDashboardActionAuditRecord {
    #[serde(default = "dashboard_schema_version")]
    pub(super) schema_version: u32,
    #[serde(default)]
    pub(super) request_id: String,
    #[serde(default)]
    pub(super) action: String,
    #[serde(default)]
    pub(super) actor: String,
    #[serde(default)]
    pub(super) reason: String,
    #[serde(default)]
    pub(super) status: String,
    #[serde(default)]
    pub(super) timestamp_unix_ms: u64,
    #[serde(default = "default_dashboard_control_mode")]
    pub(super) control_mode: String,
}

impl Default for GatewayDashboardActionAuditRecord {
    fn default() -> Self {
        Self {
            schema_version: DASHBOARD_SCHEMA_VERSION,
            request_id: String::new(),
            action: String::new(),
            actor: String::new(),
            reason: String::new(),
            status: String::new(),
            timestamp_unix_ms: 0,
            control_mode: default_dashboard_control_mode(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayDashboardControlStateFile {
    #[serde(default = "dashboard_schema_version")]
    schema_version: u32,
    #[serde(default = "default_dashboard_control_mode")]
    mode: String,
    #[serde(default)]
    updated_unix_ms: u64,
    #[serde(default)]
    last_action: Option<GatewayDashboardActionAuditRecord>,
}

impl Default for GatewayDashboardControlStateFile {
    fn default() -> Self {
        Self {
            schema_version: DASHBOARD_SCHEMA_VERSION,
            mode: default_dashboard_control_mode(),
            updated_unix_ms: 0,
            last_action: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayDashboardRuntimeStateFile {
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    widget_views: Vec<GatewayDashboardWidgetView>,
    #[serde(default)]
    control_audit: Vec<serde_json::Value>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayDashboardTrainingStatusFile {
    #[serde(default)]
    updated_unix_ms: u64,
    #[serde(default)]
    run_state: String,
    #[serde(default)]
    model_ref: String,
    #[serde(default)]
    store_path: String,
    #[serde(default)]
    total_rollouts: usize,
    #[serde(default)]
    succeeded: usize,
    #[serde(default)]
    failed: usize,
    #[serde(default)]
    cancelled: usize,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct GatewayDashboardCycleReportLine {
    #[serde(default)]
    timestamp_unix_ms: u64,
    #[serde(default)]
    health_state: String,
    #[serde(default)]
    health_reason: String,
    #[serde(default)]
    reason_codes: Vec<String>,
    #[serde(default)]
    discovered_cases: usize,
    #[serde(default)]
    queued_cases: usize,
    #[serde(default)]
    backlog_cases: usize,
    #[serde(default)]
    applied_cases: usize,
    #[serde(default)]
    malformed_cases: usize,
    #[serde(default)]
    retryable_failures: usize,
    #[serde(default)]
    failed_cases: usize,
    #[serde(default)]
    control_actions_applied: usize,
}

#[derive(Debug, Clone, Default)]
struct GatewayDashboardEventsSummary {
    log_present: bool,
    cycle_reports: usize,
    invalid_cycle_reports: usize,
    last_reason_codes: Vec<String>,
    reason_code_counts: BTreeMap<String, usize>,
    last_health_reason: String,
    recent_cycles: Vec<GatewayDashboardCycleSummary>,
}

#[derive(Debug, Clone, Default)]
struct GatewayDashboardActionLogSummary {
    log_present: bool,
    records: usize,
    invalid_records: usize,
    recent_actions: Vec<GatewayDashboardActionAuditRecord>,
    last_action: Option<GatewayDashboardActionAuditRecord>,
}

fn dashboard_schema_version() -> u32 {
    DASHBOARD_SCHEMA_VERSION
}

fn default_dashboard_control_mode() -> String {
    DASHBOARD_CONTROL_MODE_RUNNING.to_string()
}

fn dashboard_allowed_actions() -> Vec<String> {
    vec![
        "pause".to_string(),
        "resume".to_string(),
        "refresh".to_string(),
    ]
}

fn resolve_dashboard_root(gateway_state_dir: &Path) -> PathBuf {
    let tau_root = gateway_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| gateway_state_dir.to_path_buf());
    tau_root.join("dashboard")
}

fn resolve_training_root(gateway_state_dir: &Path) -> PathBuf {
    let tau_root = gateway_state_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| gateway_state_dir.to_path_buf());
    tau_root.join("training")
}

pub(super) fn collect_gateway_dashboard_snapshot(
    gateway_state_dir: &Path,
) -> GatewayDashboardSnapshot {
    let dashboard_root = resolve_dashboard_root(gateway_state_dir);
    let training_root = resolve_training_root(gateway_state_dir);
    let state_path = dashboard_root.join(DASHBOARD_STATE_FILE);
    let events_log_path = dashboard_root.join(DASHBOARD_EVENTS_LOG_FILE);
    let actions_log_path = dashboard_root.join(DASHBOARD_ACTIONS_LOG_FILE);
    let control_state_path = dashboard_root.join(DASHBOARD_CONTROL_STATE_FILE);
    let training_status_path = training_root.join(TRAINING_STATUS_FILE);

    let mut diagnostics = Vec::new();
    let runtime_state = load_dashboard_runtime_state(&state_path, &mut diagnostics);
    let events_summary = load_dashboard_events_summary(&events_log_path, &mut diagnostics);
    let action_log_summary = load_dashboard_action_log_summary(&actions_log_path, &mut diagnostics);
    let control_state = load_dashboard_control_state(&control_state_path, &mut diagnostics);
    let training = load_dashboard_training_report(&training_status_path, &mut diagnostics);

    let state_present = runtime_state.is_some();
    let parsed_runtime = runtime_state.unwrap_or_default();
    let classification = parsed_runtime.health.classify();

    let mut health_state = if state_present {
        classification.state.as_str().to_string()
    } else {
        "unknown".to_string()
    };
    let mut health_reason = if state_present {
        classification.reason
    } else {
        "dashboard runtime state is unavailable".to_string()
    };
    if !events_summary.last_health_reason.trim().is_empty() {
        health_reason = events_summary.last_health_reason.clone();
    }

    let mut control_mode = control_state
        .as_ref()
        .map(|state| normalize_dashboard_control_mode(state.mode.as_str()))
        .unwrap_or_else(default_dashboard_control_mode);
    if control_mode != DASHBOARD_CONTROL_MODE_PAUSED {
        control_mode = DASHBOARD_CONTROL_MODE_RUNNING.to_string();
    }
    let paused = control_mode == DASHBOARD_CONTROL_MODE_PAUSED;
    if paused && health_state == "healthy" {
        health_state = "degraded".to_string();
    }
    if paused {
        health_reason = "operator pause action is active".to_string();
    }

    let rollout_gate = if health_state == "healthy" && !paused {
        "pass".to_string()
    } else {
        "hold".to_string()
    };

    let mut alerts = Vec::new();
    if !state_present {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_state_missing".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "dashboard runtime state file is missing: {}",
                state_path.display()
            ),
        });
    }
    if parsed_runtime.health.failure_streak > 0 || parsed_runtime.health.last_cycle_failed > 0 {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_transport_failures".to_string(),
            severity: if parsed_runtime.health.failure_streak >= 3 {
                "critical".to_string()
            } else {
                "warning".to_string()
            },
            message: format!(
                "runtime failure signals detected (failure_streak={}, last_cycle_failed={})",
                parsed_runtime.health.failure_streak, parsed_runtime.health.last_cycle_failed
            ),
        });
    }
    if parsed_runtime.health.queue_depth > 0 {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_queue_backlog".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "runtime backlog detected (queue_depth={})",
                parsed_runtime.health.queue_depth
            ),
        });
    }
    if events_summary.invalid_cycle_reports > 0 {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_cycle_log_invalid_lines".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "runtime events log contains {} malformed line(s)",
                events_summary.invalid_cycle_reports
            ),
        });
    }
    if action_log_summary.invalid_records > 0 {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_action_log_invalid_lines".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "action audit log contains {} malformed line(s)",
                action_log_summary.invalid_records
            ),
        });
    }
    if paused {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_operator_pause_active".to_string(),
            severity: "info".to_string(),
            message: "operator pause is active; rollout gate is held".to_string(),
        });
    }
    if training.status_present && (training.failed > 0 || training.run_state == "failed") {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_training_failures".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "latest training run recorded failures (failed={}, cancelled={}, state={})",
                training.failed, training.cancelled, training.run_state
            ),
        });
    }
    if alerts.is_empty() {
        alerts.push(GatewayDashboardAlert {
            code: "dashboard_healthy".to_string(),
            severity: "info".to_string(),
            message: "dashboard runtime health is nominal".to_string(),
        });
    }

    GatewayDashboardSnapshot {
        schema_version: DASHBOARD_SCHEMA_VERSION,
        generated_unix_ms: current_unix_timestamp_ms(),
        state: GatewayDashboardStateMeta {
            dashboard_root: dashboard_root.display().to_string(),
            state_path: state_path.display().to_string(),
            events_log_path: events_log_path.display().to_string(),
            actions_log_path: actions_log_path.display().to_string(),
            control_state_path: control_state_path.display().to_string(),
            state_present,
            events_log_present: events_summary.log_present,
            actions_log_present: action_log_summary.log_present,
            control_state_present: control_state.is_some(),
            cycle_reports: events_summary.cycle_reports,
            invalid_cycle_reports: events_summary.invalid_cycle_reports,
            action_audit_records: action_log_summary.records,
            invalid_action_audit_records: action_log_summary.invalid_records,
            last_reason_codes: events_summary.last_reason_codes.clone(),
            diagnostics,
        },
        health: GatewayDashboardHealthReport {
            health_state,
            health_reason,
            rollout_gate,
            queue_depth: parsed_runtime.health.queue_depth,
            failure_streak: parsed_runtime.health.failure_streak,
            last_cycle_failed: parsed_runtime.health.last_cycle_failed,
            last_cycle_completed: parsed_runtime.health.last_cycle_completed,
            processed_case_count: parsed_runtime.processed_case_keys.len(),
            control_audit_count: parsed_runtime
                .control_audit
                .len()
                .saturating_add(action_log_summary.records),
        },
        training,
        widgets: parsed_runtime.widget_views,
        queue_timeline: GatewayDashboardQueueTimelineReport {
            cycle_reports: events_summary.cycle_reports,
            invalid_cycle_reports: events_summary.invalid_cycle_reports,
            last_reason_codes: events_summary.last_reason_codes,
            reason_code_counts: events_summary.reason_code_counts,
            recent_cycles: events_summary.recent_cycles,
        },
        alerts,
        control: GatewayDashboardControlReport {
            mode: control_mode,
            paused,
            allowed_actions: dashboard_allowed_actions(),
            last_action: control_state
                .and_then(|state| state.last_action)
                .or(action_log_summary.last_action),
        },
    }
}

pub(super) fn apply_gateway_dashboard_action(
    gateway_state_dir: &Path,
    principal: &str,
    request: GatewayDashboardActionRequest,
) -> Result<GatewayDashboardActionResult, OpenResponsesApiError> {
    let action = normalize_dashboard_action(request.action.as_str()).ok_or_else(|| {
        OpenResponsesApiError::bad_request(
            "invalid_dashboard_action",
            "supported actions are pause, resume, refresh",
        )
    })?;

    let actor = principal.trim();
    if actor.is_empty() {
        return Err(OpenResponsesApiError::unauthorized());
    }

    let dashboard_root = resolve_dashboard_root(gateway_state_dir);
    std::fs::create_dir_all(&dashboard_root).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to create dashboard state directory {}: {error}",
            dashboard_root.display()
        ))
    })?;

    let actions_log_path = dashboard_root.join(DASHBOARD_ACTIONS_LOG_FILE);
    let control_state_path = dashboard_root.join(DASHBOARD_CONTROL_STATE_FILE);
    let mut control_state =
        load_dashboard_control_state_for_write(&control_state_path).map_err(|error| {
            OpenResponsesApiError::internal(format!(
                "failed to read dashboard control state {}: {error}",
                control_state_path.display()
            ))
        })?;

    let timestamp_unix_ms = current_unix_timestamp_ms();
    let request_id = format!("dashboard-action-{timestamp_unix_ms}");
    let reason = request.reason.trim().to_string();

    control_state.mode = match action {
        "pause" => DASHBOARD_CONTROL_MODE_PAUSED.to_string(),
        "resume" => DASHBOARD_CONTROL_MODE_RUNNING.to_string(),
        "refresh" => normalize_dashboard_control_mode(control_state.mode.as_str()),
        _ => DASHBOARD_CONTROL_MODE_RUNNING.to_string(),
    };
    control_state.updated_unix_ms = timestamp_unix_ms;

    let record = GatewayDashboardActionAuditRecord {
        schema_version: DASHBOARD_SCHEMA_VERSION,
        request_id: request_id.clone(),
        action: action.to_string(),
        actor: actor.to_string(),
        reason: reason.clone(),
        status: "accepted".to_string(),
        timestamp_unix_ms,
        control_mode: control_state.mode.clone(),
    };

    append_dashboard_action_audit_record(&actions_log_path, &record).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to append dashboard action audit {}: {error}",
            actions_log_path.display()
        ))
    })?;

    control_state.last_action = Some(record.clone());
    save_dashboard_control_state(&control_state_path, &control_state).map_err(|error| {
        OpenResponsesApiError::internal(format!(
            "failed to persist dashboard control state {}: {error}",
            control_state_path.display()
        ))
    })?;

    let snapshot = collect_gateway_dashboard_snapshot(gateway_state_dir);
    Ok(GatewayDashboardActionResult {
        schema_version: DASHBOARD_SCHEMA_VERSION,
        request_id,
        action: action.to_string(),
        actor: actor.to_string(),
        reason,
        status: "accepted".to_string(),
        timestamp_unix_ms,
        control_mode: control_state.mode,
        health_state: snapshot.health.health_state,
        health_reason: snapshot.health.health_reason,
        rollout_gate: snapshot.health.rollout_gate,
        actions_log_path: actions_log_path.display().to_string(),
        control_state_path: control_state_path.display().to_string(),
    })
}

fn load_dashboard_runtime_state(
    path: &Path,
    diagnostics: &mut Vec<String>,
) -> Option<GatewayDashboardRuntimeStateFile> {
    if !path.exists() {
        diagnostics.push(format!("state_missing:{}", path.display()));
        return None;
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(format!("state_read_failed:{}:{error}", path.display()));
            return None;
        }
    };
    match serde_json::from_str::<GatewayDashboardRuntimeStateFile>(&raw) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            diagnostics.push(format!("state_parse_failed:{}:{error}", path.display()));
            None
        }
    }
}

fn load_dashboard_training_report(
    path: &Path,
    diagnostics: &mut Vec<String>,
) -> GatewayDashboardTrainingReport {
    let mut training_diagnostics = Vec::new();
    if !path.exists() {
        let message = format!("training_status_missing:{}", path.display());
        diagnostics.push(message.clone());
        training_diagnostics.push(message);
        return GatewayDashboardTrainingReport {
            status_path: path.display().to_string(),
            status_present: false,
            updated_unix_ms: 0,
            run_state: "unknown".to_string(),
            model_ref: String::new(),
            store_path: String::new(),
            total_rollouts: 0,
            succeeded: 0,
            failed: 0,
            cancelled: 0,
            diagnostics: training_diagnostics,
        };
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            let message = format!("training_status_read_failed:{}:{error}", path.display());
            diagnostics.push(message.clone());
            training_diagnostics.push(message);
            return GatewayDashboardTrainingReport {
                status_path: path.display().to_string(),
                status_present: false,
                updated_unix_ms: 0,
                run_state: "unknown".to_string(),
                model_ref: String::new(),
                store_path: String::new(),
                total_rollouts: 0,
                succeeded: 0,
                failed: 0,
                cancelled: 0,
                diagnostics: training_diagnostics,
            };
        }
    };

    match serde_json::from_str::<GatewayDashboardTrainingStatusFile>(&raw) {
        Ok(parsed) => GatewayDashboardTrainingReport {
            status_path: path.display().to_string(),
            status_present: true,
            updated_unix_ms: parsed.updated_unix_ms,
            run_state: normalize_non_empty_string(parsed.run_state.as_str(), "completed"),
            model_ref: parsed.model_ref,
            store_path: parsed.store_path,
            total_rollouts: parsed.total_rollouts,
            succeeded: parsed.succeeded,
            failed: parsed.failed,
            cancelled: parsed.cancelled,
            diagnostics: training_diagnostics,
        },
        Err(error) => {
            let message = format!("training_status_parse_failed:{}:{error}", path.display());
            diagnostics.push(message.clone());
            training_diagnostics.push(message);
            GatewayDashboardTrainingReport {
                status_path: path.display().to_string(),
                status_present: false,
                updated_unix_ms: 0,
                run_state: "unknown".to_string(),
                model_ref: String::new(),
                store_path: String::new(),
                total_rollouts: 0,
                succeeded: 0,
                failed: 0,
                cancelled: 0,
                diagnostics: training_diagnostics,
            }
        }
    }
}

fn load_dashboard_events_summary(
    path: &Path,
    diagnostics: &mut Vec<String>,
) -> GatewayDashboardEventsSummary {
    if !path.exists() {
        diagnostics.push(format!("events_log_missing:{}", path.display()));
        return GatewayDashboardEventsSummary::default();
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(format!("events_log_read_failed:{}:{error}", path.display()));
            return GatewayDashboardEventsSummary::default();
        }
    };

    let mut summary = GatewayDashboardEventsSummary {
        log_present: true,
        ..GatewayDashboardEventsSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayDashboardCycleReportLine>(trimmed) {
            Ok(parsed) => {
                summary.cycle_reports = summary.cycle_reports.saturating_add(1);
                summary.last_reason_codes = parsed.reason_codes.clone();
                if !parsed.health_reason.trim().is_empty() {
                    summary.last_health_reason = parsed.health_reason.clone();
                }
                for reason_code in &parsed.reason_codes {
                    increment_counter(&mut summary.reason_code_counts, reason_code);
                }
                summary.recent_cycles.push(GatewayDashboardCycleSummary {
                    timestamp_unix_ms: parsed.timestamp_unix_ms,
                    health_state: normalize_non_empty_string(
                        parsed.health_state.as_str(),
                        "unknown",
                    ),
                    health_reason: parsed.health_reason,
                    reason_codes: parsed.reason_codes,
                    discovered_cases: parsed.discovered_cases,
                    queued_cases: parsed.queued_cases,
                    backlog_cases: parsed.backlog_cases,
                    applied_cases: parsed.applied_cases,
                    malformed_cases: parsed.malformed_cases,
                    retryable_failures: parsed.retryable_failures,
                    failed_cases: parsed.failed_cases,
                    control_actions_applied: parsed.control_actions_applied,
                });
                if summary.recent_cycles.len() > DASHBOARD_TIMELINE_CAP {
                    summary.recent_cycles.remove(0);
                }
            }
            Err(_) => {
                summary.invalid_cycle_reports = summary.invalid_cycle_reports.saturating_add(1);
            }
        }
    }
    summary
}

fn load_dashboard_action_log_summary(
    path: &Path,
    diagnostics: &mut Vec<String>,
) -> GatewayDashboardActionLogSummary {
    if !path.exists() {
        diagnostics.push(format!("actions_log_missing:{}", path.display()));
        return GatewayDashboardActionLogSummary::default();
    }

    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(format!(
                "actions_log_read_failed:{}:{error}",
                path.display()
            ));
            return GatewayDashboardActionLogSummary::default();
        }
    };

    let mut summary = GatewayDashboardActionLogSummary {
        log_present: true,
        ..GatewayDashboardActionLogSummary::default()
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<GatewayDashboardActionAuditRecord>(trimmed) {
            Ok(parsed) => {
                summary.records = summary.records.saturating_add(1);
                summary.last_action = Some(parsed.clone());
                summary.recent_actions.push(parsed);
                if summary.recent_actions.len() > DASHBOARD_ACTIONS_TAIL_CAP {
                    summary.recent_actions.remove(0);
                }
            }
            Err(_) => {
                summary.invalid_records = summary.invalid_records.saturating_add(1);
            }
        }
    }
    summary
}

fn load_dashboard_control_state(
    path: &Path,
    diagnostics: &mut Vec<String>,
) -> Option<GatewayDashboardControlStateFile> {
    if !path.exists() {
        diagnostics.push(format!("control_state_missing:{}", path.display()));
        return None;
    }
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            diagnostics.push(format!(
                "control_state_read_failed:{}:{error}",
                path.display()
            ));
            return None;
        }
    };
    match serde_json::from_str::<GatewayDashboardControlStateFile>(&raw) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            diagnostics.push(format!(
                "control_state_parse_failed:{}:{error}",
                path.display()
            ));
            None
        }
    }
}

fn load_dashboard_control_state_for_write(path: &Path) -> Result<GatewayDashboardControlStateFile> {
    if !path.exists() {
        return Ok(GatewayDashboardControlStateFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = serde_json::from_str::<GatewayDashboardControlStateFile>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(parsed)
}

fn save_dashboard_control_state(
    path: &Path,
    state: &GatewayDashboardControlStateFile,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload =
        serde_json::to_string_pretty(state).context("failed to serialize control state payload")?;
    std::fs::write(path, payload).with_context(|| format!("failed to write {}", path.display()))
}

fn append_dashboard_action_audit_record(
    path: &Path,
    record: &GatewayDashboardActionAuditRecord,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = serde_json::to_string(record).context("serialize action audit record")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    use std::io::Write;
    writeln!(file, "{payload}").with_context(|| format!("failed to append {}", path.display()))
}

fn increment_counter(counters: &mut BTreeMap<String, usize>, key: &str) {
    *counters.entry(key.to_string()).or_insert(0) += 1;
}

fn normalize_non_empty_string(raw: &str, fallback: &str) -> String {
    if raw.trim().is_empty() {
        fallback.to_string()
    } else {
        raw.trim().to_string()
    }
}

fn normalize_dashboard_control_mode(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        DASHBOARD_CONTROL_MODE_PAUSED => DASHBOARD_CONTROL_MODE_PAUSED.to_string(),
        _ => DASHBOARD_CONTROL_MODE_RUNNING.to_string(),
    }
}

fn normalize_dashboard_action(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pause" => Some("pause"),
        "resume" => Some("resume"),
        "refresh" => Some("refresh"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn write_dashboard_state(root: &Path) -> PathBuf {
        let dashboard_root = root.join(".tau").join("dashboard");
        let training_root = root.join(".tau").join("training");
        std::fs::create_dir_all(&dashboard_root).expect("create dashboard root");
        std::fs::create_dir_all(&training_root).expect("create training root");
        std::fs::write(
            dashboard_root.join(DASHBOARD_STATE_FILE),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["snapshot:a", "control:b"],
  "widget_views": [
    {
      "widget_id": "health-summary",
      "kind": "health_summary",
      "title": "Health",
      "query_key": "runtime.health",
      "refresh_interval_ms": 3000,
      "last_case_key": "snapshot:a",
      "updated_unix_ms": 11
    }
  ],
  "control_audit": [{"event_key":"dashboard-control:resume:b"}],
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write dashboard state");
        std::fs::write(
            dashboard_root.join(DASHBOARD_EVENTS_LOG_FILE),
            r#"{"timestamp_unix_ms":1,"health_state":"healthy","health_reason":"no recent transport failures observed","reason_codes":["widget_views_updated"],"discovered_cases":2,"queued_cases":2,"backlog_cases":0,"applied_cases":2,"failed_cases":0}
invalid-json-line
"#,
        )
        .expect("write dashboard events");
        std::fs::write(
            training_root.join(TRAINING_STATUS_FILE),
            r#"{
  "schema_version": 1,
  "updated_unix_ms": 701,
  "run_state": "completed",
  "model_ref": "openai/gpt-4o-mini",
  "store_path": ".tau/training/store.sqlite",
  "total_rollouts": 3,
  "succeeded": 2,
  "failed": 1,
  "cancelled": 0
}
"#,
        )
        .expect("write training status");
        dashboard_root
    }

    #[test]
    fn unit_collect_gateway_dashboard_snapshot_reads_state_and_logs() {
        let temp = tempdir().expect("tempdir");
        write_dashboard_state(temp.path());
        let gateway_root = temp.path().join(".tau").join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway root");

        let snapshot = collect_gateway_dashboard_snapshot(&gateway_root);
        assert_eq!(snapshot.schema_version, DASHBOARD_SCHEMA_VERSION);
        assert!(snapshot.state.state_present);
        assert!(snapshot.state.events_log_present);
        assert_eq!(snapshot.widgets.len(), 1);
        assert_eq!(snapshot.health.rollout_gate, "pass");
        assert!(snapshot.training.status_present);
        assert_eq!(snapshot.training.total_rollouts, 3);
        assert_eq!(snapshot.training.failed, 1);
        assert_eq!(snapshot.queue_timeline.cycle_reports, 1);
        assert_eq!(snapshot.queue_timeline.invalid_cycle_reports, 1);
        assert!(snapshot
            .queue_timeline
            .reason_code_counts
            .contains_key("widget_views_updated"));
        assert!(snapshot
            .alerts
            .iter()
            .any(|alert| alert.code == "dashboard_training_failures"));
    }

    #[test]
    fn functional_apply_gateway_dashboard_action_writes_control_and_audit_records() {
        let temp = tempdir().expect("tempdir");
        write_dashboard_state(temp.path());
        let gateway_root = temp.path().join(".tau").join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway root");

        let pause = apply_gateway_dashboard_action(
            &gateway_root,
            "ops-user",
            GatewayDashboardActionRequest {
                action: "pause".to_string(),
                reason: "maintenance".to_string(),
            },
        )
        .expect("apply pause");
        assert_eq!(pause.action, "pause");
        assert_eq!(pause.status, "accepted");
        assert_eq!(pause.control_mode, DASHBOARD_CONTROL_MODE_PAUSED);

        let dashboard_root = resolve_dashboard_root(&gateway_root);
        let actions_raw = std::fs::read_to_string(dashboard_root.join(DASHBOARD_ACTIONS_LOG_FILE))
            .expect("read actions audit log");
        assert!(actions_raw.contains("\"action\":\"pause\""));

        let control_raw =
            std::fs::read_to_string(dashboard_root.join(DASHBOARD_CONTROL_STATE_FILE))
                .expect("read control state file");
        assert!(control_raw.contains("\"mode\": \"paused\""));
    }

    #[test]
    fn regression_apply_gateway_dashboard_action_rejects_unknown_action() {
        let temp = tempdir().expect("tempdir");
        let gateway_root = temp.path().join(".tau").join("gateway");
        std::fs::create_dir_all(&gateway_root).expect("create gateway root");

        let error = apply_gateway_dashboard_action(
            &gateway_root,
            "ops-user",
            GatewayDashboardActionRequest {
                action: "explode".to_string(),
                reason: String::new(),
            },
        )
        .expect_err("invalid action should fail");
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "invalid_dashboard_action");
    }
}
