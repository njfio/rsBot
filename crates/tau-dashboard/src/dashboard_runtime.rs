use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::dashboard_contract::{
    load_dashboard_contract_fixture, DashboardContractCase, DashboardContractFixture,
    DashboardControlAction, DashboardFixtureMode, DashboardOutcomeKind, DashboardReplayResult,
    DashboardReplayStep, DashboardScope, DASHBOARD_ERROR_BACKEND_UNAVAILABLE,
    DASHBOARD_ERROR_EMPTY_INPUT, DASHBOARD_ERROR_INVALID_ACTION, DASHBOARD_ERROR_INVALID_FILTER,
    DASHBOARD_ERROR_INVALID_SCOPE,
};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::channel_store::{ChannelContextEntry, ChannelLogEntry, ChannelStore};
use tau_runtime::transport_health::TransportHealthSnapshot;

const DASHBOARD_RUNTIME_STATE_SCHEMA_VERSION: u32 = 1;
const DASHBOARD_RUNTIME_EVENTS_LOG_FILE: &str = "runtime-events.jsonl";
const DASHBOARD_CONTROL_AUDIT_CAP: usize = 512;

fn dashboard_runtime_state_schema_version() -> u32 {
    DASHBOARD_RUNTIME_STATE_SCHEMA_VERSION
}

#[derive(Debug, Clone)]
pub struct DashboardRuntimeConfig {
    pub fixture_path: PathBuf,
    pub state_dir: PathBuf,
    pub queue_limit: usize,
    pub processed_case_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DashboardRuntimeSummary {
    pub discovered_cases: usize,
    pub queued_cases: usize,
    pub applied_cases: usize,
    pub duplicate_skips: usize,
    pub malformed_cases: usize,
    pub retryable_failures: usize,
    pub retry_attempts: usize,
    pub failed_cases: usize,
    pub upserted_widgets: usize,
    pub control_actions_applied: usize,
}

#[derive(Debug, Clone, Serialize)]
struct DashboardRuntimeCycleReport {
    timestamp_unix_ms: u64,
    health_state: String,
    health_reason: String,
    reason_codes: Vec<String>,
    discovered_cases: usize,
    queued_cases: usize,
    applied_cases: usize,
    duplicate_skips: usize,
    malformed_cases: usize,
    retryable_failures: usize,
    retry_attempts: usize,
    failed_cases: usize,
    upserted_widgets: usize,
    control_actions_applied: usize,
    backlog_cases: usize,
    failure_streak: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DashboardWidgetView {
    widget_id: String,
    kind: String,
    title: String,
    query_key: String,
    refresh_interval_ms: u64,
    last_case_key: String,
    updated_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DashboardControlAuditRecord {
    event_key: String,
    case_id: String,
    action: String,
    status: String,
    timestamp_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DashboardRuntimeState {
    #[serde(default = "dashboard_runtime_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    processed_case_keys: Vec<String>,
    #[serde(default)]
    widget_views: Vec<DashboardWidgetView>,
    #[serde(default)]
    control_audit: Vec<DashboardControlAuditRecord>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

impl Default for DashboardRuntimeState {
    fn default() -> Self {
        Self {
            schema_version: DASHBOARD_RUNTIME_STATE_SCHEMA_VERSION,
            processed_case_keys: Vec::new(),
            widget_views: Vec::new(),
            control_audit: Vec::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

pub async fn run_dashboard_contract_runner(config: DashboardRuntimeConfig) -> Result<()> {
    let fixture = load_dashboard_contract_fixture(&config.fixture_path)?;
    let mut runtime = DashboardRuntime::new(config)?;
    let summary = runtime.run_once(&fixture).await?;
    let health = runtime.transport_health().clone();
    let classification = health.classify();

    println!(
        "dashboard runner summary: discovered={} queued={} applied={} duplicate_skips={} malformed={} retryable_failures={} retries={} failed={} upserted_widgets={} control_actions={}",
        summary.discovered_cases,
        summary.queued_cases,
        summary.applied_cases,
        summary.duplicate_skips,
        summary.malformed_cases,
        summary.retryable_failures,
        summary.retry_attempts,
        summary.failed_cases,
        summary.upserted_widgets,
        summary.control_actions_applied
    );
    println!(
        "dashboard runner health: state={} failure_streak={} queue_depth={} reason={}",
        classification.state.as_str(),
        health.failure_streak,
        health.queue_depth,
        classification.reason
    );

    Ok(())
}

struct DashboardRuntime {
    config: DashboardRuntimeConfig,
    state: DashboardRuntimeState,
    processed_case_keys: HashSet<String>,
}

impl DashboardRuntime {
    fn new(config: DashboardRuntimeConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.state_dir)
            .with_context(|| format!("failed to create {}", config.state_dir.display()))?;
        let mut state = load_dashboard_runtime_state(&config.state_dir.join("state.json"))?;
        state.processed_case_keys =
            normalize_processed_case_keys(&state.processed_case_keys, config.processed_case_cap);
        state
            .widget_views
            .sort_by(|left, right| left.widget_id.cmp(&right.widget_id));

        let processed_case_keys = state.processed_case_keys.iter().cloned().collect();
        Ok(Self {
            config,
            state,
            processed_case_keys,
        })
    }

    fn state_path(&self) -> PathBuf {
        self.config.state_dir.join("state.json")
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    async fn run_once(
        &mut self,
        fixture: &DashboardContractFixture,
    ) -> Result<DashboardRuntimeSummary> {
        let cycle_started = Instant::now();
        let mut summary = DashboardRuntimeSummary {
            discovered_cases: fixture.cases.len(),
            ..DashboardRuntimeSummary::default()
        };

        let mut queued_cases = fixture.cases.clone();
        queued_cases.sort_by(|left, right| {
            left.case_id
                .cmp(&right.case_id)
                .then_with(|| left.mode.as_str().cmp(right.mode.as_str()))
        });
        queued_cases.truncate(self.config.queue_limit);
        summary.queued_cases = queued_cases.len();

        for case in queued_cases {
            let case_key = case_runtime_key(&case);
            if self.processed_case_keys.contains(&case_key) {
                summary.duplicate_skips = summary.duplicate_skips.saturating_add(1);
                continue;
            }

            let mut attempt = 1_usize;
            loop {
                let result = evaluate_dashboard_case(&case)?;
                validate_case_result_against_contract(&case, &result)?;
                match result.step {
                    DashboardReplayStep::Success => {
                        let (upserted_widgets, control_actions) =
                            self.persist_success_result(&case, &case_key, &result)?;
                        summary.applied_cases = summary.applied_cases.saturating_add(1);
                        summary.upserted_widgets =
                            summary.upserted_widgets.saturating_add(upserted_widgets);
                        summary.control_actions_applied = summary
                            .control_actions_applied
                            .saturating_add(control_actions);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    DashboardReplayStep::MalformedInput => {
                        summary.malformed_cases = summary.malformed_cases.saturating_add(1);
                        let control_actions =
                            self.persist_non_success_result(&case, &case_key, &result)?;
                        summary.control_actions_applied = summary
                            .control_actions_applied
                            .saturating_add(control_actions);
                        self.record_processed_case(&case_key);
                        break;
                    }
                    DashboardReplayStep::RetryableFailure => {
                        summary.retryable_failures = summary.retryable_failures.saturating_add(1);
                        if attempt >= self.config.retry_max_attempts {
                            summary.failed_cases = summary.failed_cases.saturating_add(1);
                            let control_actions =
                                self.persist_non_success_result(&case, &case_key, &result)?;
                            summary.control_actions_applied = summary
                                .control_actions_applied
                                .saturating_add(control_actions);
                            break;
                        }
                        summary.retry_attempts = summary.retry_attempts.saturating_add(1);
                        apply_retry_delay(self.config.retry_base_delay_ms, attempt).await;
                        attempt = attempt.saturating_add(1);
                    }
                }
            }
        }

        let cycle_duration_ms =
            u64::try_from(cycle_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let health = build_transport_health_snapshot(
            &summary,
            cycle_duration_ms,
            self.state.health.failure_streak,
        );
        let classification = health.classify();
        let reason_codes = cycle_reason_codes(&summary);
        self.state.health = health.clone();

        save_dashboard_runtime_state(&self.state_path(), &self.state)?;
        append_dashboard_cycle_report(
            &self
                .config
                .state_dir
                .join(DASHBOARD_RUNTIME_EVENTS_LOG_FILE),
            &summary,
            &health,
            &classification.reason,
            &reason_codes,
        )?;

        Ok(summary)
    }

    fn persist_success_result(
        &mut self,
        case: &DashboardContractCase,
        case_key: &str,
        result: &DashboardReplayResult,
    ) -> Result<(usize, usize)> {
        let mut upserted = 0_usize;
        for widget in &result.widgets {
            let view = DashboardWidgetView {
                widget_id: widget.widget_id.clone(),
                kind: format!("{:?}", widget.kind).to_ascii_lowercase(),
                title: widget.title.clone(),
                query_key: widget.query_key.clone(),
                refresh_interval_ms: widget.refresh_interval_ms,
                last_case_key: case_key.to_string(),
                updated_unix_ms: current_unix_timestamp_ms(),
            };
            if let Some(existing) = self
                .state
                .widget_views
                .iter_mut()
                .find(|existing| existing.widget_id == widget.widget_id)
            {
                *existing = view;
            } else {
                self.state.widget_views.push(view);
            }
            upserted = upserted.saturating_add(1);
        }
        self.state
            .widget_views
            .sort_by(|left, right| left.widget_id.cmp(&right.widget_id));

        let mut control_actions = 0_usize;
        if let Some(action) = case.control_action {
            self.record_control_audit(case, case_key, action, "success");
            control_actions = control_actions.saturating_add(1);
        }

        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-dashboard-runner".to_string(),
                payload: json!({
                    "outcome": "success",
                    "mode": case.mode.as_str(),
                    "case_id": case.case_id,
                    "upserted_widgets": upserted,
                    "control_action": case.control_action.map(DashboardControlAction::as_str),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "dashboard case {} applied with {} widget updates",
                    case.case_id, upserted
                ),
            })?;
            let rendered = render_dashboard_snapshot(&self.state.widget_views, &case.scope);
            store.write_memory(&rendered)?;
        }

        Ok((upserted, control_actions))
    }

    fn persist_non_success_result(
        &mut self,
        case: &DashboardContractCase,
        case_key: &str,
        result: &DashboardReplayResult,
    ) -> Result<usize> {
        let outcome = match result.step {
            DashboardReplayStep::Success => "success",
            DashboardReplayStep::MalformedInput => "malformed_input",
            DashboardReplayStep::RetryableFailure => "retryable_failure",
        };
        let mut control_actions = 0_usize;
        if let Some(action) = case.control_action {
            self.record_control_audit(case, case_key, action, outcome);
            control_actions = control_actions.saturating_add(1);
        }

        if let Some(store) = self.scope_channel_store(case)? {
            let timestamp_unix_ms = current_unix_timestamp_ms();
            store.append_log_entry(&ChannelLogEntry {
                timestamp_unix_ms,
                direction: "system".to_string(),
                event_key: Some(case_key.to_string()),
                source: "tau-dashboard-runner".to_string(),
                payload: json!({
                    "outcome": outcome,
                    "mode": case.mode.as_str(),
                    "case_id": case.case_id,
                    "error_code": result.error_code.clone().unwrap_or_default(),
                    "control_action": case.control_action.map(DashboardControlAction::as_str),
                }),
            })?;
            store.append_context_entry(&ChannelContextEntry {
                timestamp_unix_ms,
                role: "system".to_string(),
                text: format!(
                    "dashboard case {} outcome={} error_code={}",
                    case.case_id,
                    outcome,
                    result.error_code.clone().unwrap_or_default()
                ),
            })?;
        }

        Ok(control_actions)
    }

    fn record_control_audit(
        &mut self,
        case: &DashboardContractCase,
        case_key: &str,
        action: DashboardControlAction,
        status: &str,
    ) {
        self.state.control_audit.push(DashboardControlAuditRecord {
            event_key: case_key.to_string(),
            case_id: case.case_id.trim().to_string(),
            action: action.as_str().to_string(),
            status: status.to_string(),
            timestamp_unix_ms: current_unix_timestamp_ms(),
        });
        if self.state.control_audit.len() > DASHBOARD_CONTROL_AUDIT_CAP {
            let overflow = self
                .state
                .control_audit
                .len()
                .saturating_sub(DASHBOARD_CONTROL_AUDIT_CAP);
            self.state.control_audit.drain(0..overflow);
        }
    }

    fn scope_channel_store(&self, case: &DashboardContractCase) -> Result<Option<ChannelStore>> {
        let workspace_id = case.scope.workspace_id.trim();
        if workspace_id.is_empty() {
            return Ok(None);
        }
        let channel_id = if case.scope.operator_id.trim().is_empty() {
            format!("workspace:{workspace_id}")
        } else {
            format!("operator:{}", case.scope.operator_id.trim())
        };
        let store = ChannelStore::open(
            &self.config.state_dir.join("channel-store"),
            "dashboard",
            &channel_id,
        )?;
        Ok(Some(store))
    }

    fn record_processed_case(&mut self, case_key: &str) {
        if self.processed_case_keys.contains(case_key) {
            return;
        }
        self.state.processed_case_keys.push(case_key.to_string());
        self.processed_case_keys.insert(case_key.to_string());
        if self.state.processed_case_keys.len() > self.config.processed_case_cap {
            let overflow = self
                .state
                .processed_case_keys
                .len()
                .saturating_sub(self.config.processed_case_cap);
            let removed = self.state.processed_case_keys.drain(0..overflow);
            for key in removed {
                self.processed_case_keys.remove(&key);
            }
        }
    }
}

fn case_runtime_key(case: &DashboardContractCase) -> String {
    format!("{}:{}", case.mode.as_str(), case.case_id.trim())
}

fn build_transport_health_snapshot(
    summary: &DashboardRuntimeSummary,
    cycle_duration_ms: u64,
    previous_failure_streak: usize,
) -> TransportHealthSnapshot {
    let backlog_cases = summary
        .discovered_cases
        .saturating_sub(summary.queued_cases);
    let failure_streak = if summary.failed_cases > 0 {
        previous_failure_streak.saturating_add(1)
    } else {
        0
    };
    TransportHealthSnapshot {
        updated_unix_ms: current_unix_timestamp_ms(),
        cycle_duration_ms,
        queue_depth: backlog_cases,
        active_runs: 0,
        failure_streak,
        last_cycle_discovered: summary.discovered_cases,
        last_cycle_processed: summary
            .applied_cases
            .saturating_add(summary.malformed_cases)
            .saturating_add(summary.failed_cases)
            .saturating_add(summary.duplicate_skips),
        last_cycle_completed: summary
            .applied_cases
            .saturating_add(summary.malformed_cases),
        last_cycle_failed: summary.failed_cases,
        last_cycle_duplicates: summary.duplicate_skips,
    }
}

fn cycle_reason_codes(summary: &DashboardRuntimeSummary) -> Vec<String> {
    let mut codes = Vec::new();
    if summary.discovered_cases > summary.queued_cases {
        codes.push("queue_backpressure_applied".to_string());
    }
    if summary.duplicate_skips > 0 {
        codes.push("duplicate_cases_skipped".to_string());
    }
    if summary.malformed_cases > 0 {
        codes.push("malformed_inputs_observed".to_string());
    }
    if summary.retry_attempts > 0 {
        codes.push("retry_attempted".to_string());
    }
    if summary.retryable_failures > 0 {
        codes.push("retryable_failures_observed".to_string());
    }
    if summary.failed_cases > 0 {
        codes.push("case_processing_failed".to_string());
    }
    if summary.upserted_widgets > 0 {
        codes.push("widget_views_updated".to_string());
    }
    if summary.control_actions_applied > 0 {
        codes.push("control_actions_applied".to_string());
    }
    if codes.is_empty() {
        codes.push("healthy_cycle".to_string());
    }
    codes
}

fn append_dashboard_cycle_report(
    path: &Path,
    summary: &DashboardRuntimeSummary,
    health: &TransportHealthSnapshot,
    health_reason: &str,
    reason_codes: &[String],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }

    let payload = DashboardRuntimeCycleReport {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        health_state: health.classify().state.as_str().to_string(),
        health_reason: health_reason.to_string(),
        reason_codes: reason_codes.to_vec(),
        discovered_cases: summary.discovered_cases,
        queued_cases: summary.queued_cases,
        applied_cases: summary.applied_cases,
        duplicate_skips: summary.duplicate_skips,
        malformed_cases: summary.malformed_cases,
        retryable_failures: summary.retryable_failures,
        retry_attempts: summary.retry_attempts,
        failed_cases: summary.failed_cases,
        upserted_widgets: summary.upserted_widgets,
        control_actions_applied: summary.control_actions_applied,
        backlog_cases: summary
            .discovered_cases
            .saturating_sub(summary.queued_cases),
        failure_streak: health.failure_streak,
    };
    let line = serde_json::to_string(&payload).context("serialize dashboard runtime report")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}").with_context(|| format!("failed to append {}", path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

fn evaluate_dashboard_case(case: &DashboardContractCase) -> Result<DashboardReplayResult> {
    if case.scope.workspace_id.trim().is_empty() {
        return Ok(DashboardReplayResult {
            step: DashboardReplayStep::MalformedInput,
            error_code: Some(DASHBOARD_ERROR_INVALID_SCOPE.to_string()),
            widgets: Vec::new(),
            audit_event_key: String::new(),
        });
    }
    if case.requested_widgets.is_empty() {
        return Ok(DashboardReplayResult {
            step: DashboardReplayStep::MalformedInput,
            error_code: Some(DASHBOARD_ERROR_EMPTY_INPUT.to_string()),
            widgets: Vec::new(),
            audit_event_key: String::new(),
        });
    }

    if case.simulate_retryable_failure {
        return Ok(DashboardReplayResult {
            step: DashboardReplayStep::RetryableFailure,
            error_code: Some(DASHBOARD_ERROR_BACKEND_UNAVAILABLE.to_string()),
            widgets: Vec::new(),
            audit_event_key: String::new(),
        });
    }

    match case.mode {
        DashboardFixtureMode::Snapshot => {}
        DashboardFixtureMode::Filter => {
            if case.filters.is_empty() {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_EMPTY_INPUT.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }
            if case
                .filters
                .iter()
                .any(|filter| filter.value.trim().is_empty())
            {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_INVALID_FILTER.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }
        }
        DashboardFixtureMode::Control => {
            if case.control_action.is_none() {
                return Ok(DashboardReplayResult {
                    step: DashboardReplayStep::MalformedInput,
                    error_code: Some(DASHBOARD_ERROR_INVALID_ACTION.to_string()),
                    widgets: Vec::new(),
                    audit_event_key: String::new(),
                });
            }
        }
    }

    let mut widgets = case.requested_widgets.clone();
    widgets.sort_by(|left, right| left.widget_id.cmp(&right.widget_id));
    let audit_event_key = if case.mode == DashboardFixtureMode::Control {
        let action = case
            .control_action
            .ok_or_else(|| anyhow::anyhow!("control mode requires control_action"))?
            .as_str();
        format!("dashboard-control:{action}:{}", case.case_id.trim())
    } else {
        String::new()
    };
    Ok(DashboardReplayResult {
        step: DashboardReplayStep::Success,
        error_code: None,
        widgets,
        audit_event_key,
    })
}

fn validate_case_result_against_contract(
    case: &DashboardContractCase,
    result: &DashboardReplayResult,
) -> Result<()> {
    let expected_step = match case.expected.outcome {
        DashboardOutcomeKind::Success => DashboardReplayStep::Success,
        DashboardOutcomeKind::MalformedInput => DashboardReplayStep::MalformedInput,
        DashboardOutcomeKind::RetryableFailure => DashboardReplayStep::RetryableFailure,
    };
    if result.step != expected_step {
        bail!(
            "case '{}' expected step {:?} but observed {:?}",
            case.case_id,
            expected_step,
            result.step
        );
    }

    match case.expected.outcome {
        DashboardOutcomeKind::Success => {
            if result.error_code.is_some() {
                bail!(
                    "case '{}' expected empty error_code for success but observed {:?}",
                    case.case_id,
                    result.error_code
                );
            }
            if result.widgets != case.expected.widgets {
                bail!(
                    "case '{}' expected widgets {:?} but observed {:?}",
                    case.case_id,
                    case.expected.widgets,
                    result.widgets
                );
            }
            if result.audit_event_key != case.expected.audit_event_key {
                bail!(
                    "case '{}' expected audit_event_key '{}' but observed '{}'",
                    case.case_id,
                    case.expected.audit_event_key,
                    result.audit_event_key
                );
            }
        }
        DashboardOutcomeKind::MalformedInput | DashboardOutcomeKind::RetryableFailure => {
            let expected_code = case.expected.error_code.trim();
            if result.error_code.as_deref() != Some(expected_code) {
                bail!(
                    "case '{}' expected error_code '{}' but observed {:?}",
                    case.case_id,
                    expected_code,
                    result.error_code
                );
            }
            if !result.widgets.is_empty() {
                bail!(
                    "case '{}' expected no widgets for non-success outcome but observed {} widgets",
                    case.case_id,
                    result.widgets.len()
                );
            }
            if !result.audit_event_key.is_empty() {
                bail!(
                    "case '{}' expected empty audit_event_key for non-success outcome but observed '{}'",
                    case.case_id,
                    result.audit_event_key
                );
            }
        }
    }
    Ok(())
}

fn render_dashboard_snapshot(widgets: &[DashboardWidgetView], scope: &DashboardScope) -> String {
    if widgets.is_empty() {
        return format!(
            "# Tau Dashboard Snapshot ({})\n\n- No materialized widgets",
            scope.workspace_id.trim()
        );
    }
    let mut lines = vec![
        format!("# Tau Dashboard Snapshot ({})", scope.workspace_id.trim()),
        String::new(),
    ];
    for widget in widgets {
        lines.push(format!(
            "- {}: {} ({})",
            widget.widget_id, widget.title, widget.query_key
        ));
    }
    lines.join("\n")
}

fn normalize_processed_case_keys(raw: &[String], cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for key in raw {
        let trimmed = key.trim();
        if trimmed.is_empty() {
            continue;
        }
        let owned = trimmed.to_string();
        if seen.insert(owned.clone()) {
            normalized.push(owned);
        }
    }
    if cap == 0 {
        return Vec::new();
    }
    if normalized.len() > cap {
        normalized.drain(0..normalized.len().saturating_sub(cap));
    }
    normalized
}

fn retry_delay_ms(base_delay_ms: u64, attempt: usize) -> u64 {
    if base_delay_ms == 0 {
        return 0;
    }
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    base_delay_ms.saturating_mul(1_u64 << exponent)
}

async fn apply_retry_delay(base_delay_ms: u64, attempt: usize) {
    let delay_ms = retry_delay_ms(base_delay_ms, attempt);
    if delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

fn load_dashboard_runtime_state(path: &Path) -> Result<DashboardRuntimeState> {
    if !path.exists() {
        return Ok(DashboardRuntimeState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = match serde_json::from_str::<DashboardRuntimeState>(&raw) {
        Ok(state) => state,
        Err(error) => {
            eprintln!(
                "dashboard runner: failed to parse state file {} ({error}); starting fresh",
                path.display()
            );
            return Ok(DashboardRuntimeState::default());
        }
    };
    if parsed.schema_version != DASHBOARD_RUNTIME_STATE_SCHEMA_VERSION {
        eprintln!(
            "dashboard runner: unsupported state schema {} in {}; starting fresh",
            parsed.schema_version,
            path.display()
        );
        return Ok(DashboardRuntimeState::default());
    }
    Ok(parsed)
}

fn save_dashboard_runtime_state(path: &Path, state: &DashboardRuntimeState) -> Result<()> {
    let payload = serde_json::to_string_pretty(state).context("serialize dashboard state")?;
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use tempfile::tempdir;

    use super::{
        load_dashboard_runtime_state, retry_delay_ms, DashboardRuntime, DashboardRuntimeConfig,
        DASHBOARD_RUNTIME_EVENTS_LOG_FILE,
    };
    use crate::dashboard_contract::load_dashboard_contract_fixture;
    use tau_runtime::channel_store::ChannelStore;
    use tau_runtime::transport_health::TransportHealthState;

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("dashboard-contract")
            .join(name)
    }

    fn build_config(root: &Path) -> DashboardRuntimeConfig {
        DashboardRuntimeConfig {
            fixture_path: fixture_path("mixed-outcomes.json"),
            state_dir: root.join(".tau/dashboard"),
            queue_limit: 64,
            processed_case_cap: 10_000,
            retry_max_attempts: 2,
            retry_base_delay_ms: 0,
        }
    }

    #[test]
    fn unit_retry_delay_ms_scales_with_attempt_number() {
        assert_eq!(retry_delay_ms(0, 1), 0);
        assert_eq!(retry_delay_ms(10, 1), 10);
        assert_eq!(retry_delay_ms(10, 2), 20);
        assert_eq!(retry_delay_ms(10, 3), 40);
    }

    #[tokio::test]
    async fn functional_runner_processes_fixture_and_persists_dashboard_state() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_dashboard_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = DashboardRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 3);
        assert_eq!(summary.applied_cases, 1);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.retryable_failures, 2);
        assert_eq!(summary.retry_attempts, 1);
        assert_eq!(summary.failed_cases, 1);
        assert_eq!(summary.upserted_widgets, 3);
        assert_eq!(summary.control_actions_applied, 1);
        assert_eq!(summary.duplicate_skips, 0);

        let state =
            load_dashboard_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.widget_views.len(), 3);
        assert_eq!(state.control_audit.len(), 1);
        assert_eq!(state.processed_case_keys.len(), 2);
        assert_eq!(state.health.last_cycle_discovered, 3);
        assert_eq!(state.health.last_cycle_failed, 1);
        assert_eq!(state.health.failure_streak, 1);
        assert_eq!(
            state.health.classify().state,
            TransportHealthState::Degraded
        );

        let events_log =
            std::fs::read_to_string(config.state_dir.join(DASHBOARD_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        assert!(events_log.contains("retryable_failures_observed"));
        assert!(events_log.contains("control_actions_applied"));

        let store = ChannelStore::open(
            &config.state_dir.join("channel-store"),
            "dashboard",
            "operator:ops-user-1",
        )
        .expect("open channel store");
        let memory = store
            .load_memory()
            .expect("load memory")
            .expect("memory should exist");
        assert!(memory.contains("Tau Dashboard Snapshot (tau-core)"));
        assert!(memory.contains("health-summary"));
    }

    #[tokio::test]
    async fn integration_runner_respects_queue_limit_for_backpressure() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.queue_limit = 2;
        let fixture =
            load_dashboard_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = DashboardRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");

        assert_eq!(summary.discovered_cases, 3);
        assert_eq!(summary.queued_cases, 2);
        assert_eq!(summary.applied_cases, 0);
        assert_eq!(summary.malformed_cases, 1);
        assert_eq!(summary.failed_cases, 1);
        let state =
            load_dashboard_runtime_state(&config.state_dir.join("state.json")).expect("load state");
        assert_eq!(state.widget_views.len(), 0);
    }

    #[tokio::test]
    async fn integration_runner_skips_processed_cases_but_retries_unresolved_failures() {
        let temp = tempdir().expect("tempdir");
        let config = build_config(temp.path());
        let fixture =
            load_dashboard_contract_fixture(&config.fixture_path).expect("fixture should load");

        let mut first_runtime = DashboardRuntime::new(config.clone()).expect("first runtime");
        let first = first_runtime.run_once(&fixture).await.expect("first run");
        assert_eq!(first.applied_cases, 1);
        assert_eq!(first.malformed_cases, 1);

        let mut second_runtime = DashboardRuntime::new(config).expect("second runtime");
        let second = second_runtime.run_once(&fixture).await.expect("second run");
        assert_eq!(second.duplicate_skips, 2);
        assert_eq!(second.applied_cases, 0);
        assert_eq!(second.malformed_cases, 0);
        assert_eq!(second.failed_cases, 1);
    }

    #[tokio::test]
    async fn regression_runner_rejects_contract_drift_between_expected_and_runtime_result() {
        let temp = tempdir().expect("tempdir");
        let mut fixture = load_dashboard_contract_fixture(&fixture_path("snapshot-layout.json"))
            .expect("fixture");
        fixture.cases[0].expected.widgets[0].title = "invalid-title".to_string();
        let fixture_path = temp.path().join("drift-fixture.json");
        std::fs::write(
            &fixture_path,
            serde_json::to_string_pretty(&fixture).expect("serialize"),
        )
        .expect("write fixture");

        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path;
        let mut runtime = DashboardRuntime::new(config.clone()).expect("runtime");
        let drift_fixture =
            load_dashboard_contract_fixture(&config.fixture_path).expect("fixture should load");
        let error = runtime
            .run_once(&drift_fixture)
            .await
            .expect_err("drift should fail");
        assert!(error.to_string().contains("expected widgets"));
    }

    #[tokio::test]
    async fn regression_runner_failure_streak_resets_after_successful_cycle() {
        let temp = tempdir().expect("tempdir");
        let mut failing_config = build_config(temp.path());
        failing_config.retry_max_attempts = 1;
        let failing_fixture =
            load_dashboard_contract_fixture(&failing_config.fixture_path).expect("fixture");
        let mut failing_runtime = DashboardRuntime::new(failing_config.clone()).expect("runtime");
        let failed = failing_runtime
            .run_once(&failing_fixture)
            .await
            .expect("failed cycle");
        assert_eq!(failed.failed_cases, 1);
        let state_after_fail =
            load_dashboard_runtime_state(&failing_config.state_dir.join("state.json"))
                .expect("load state after fail");
        assert_eq!(state_after_fail.health.failure_streak, 1);

        let mut success_config = failing_config.clone();
        success_config.fixture_path = fixture_path("snapshot-layout.json");
        let success_fixture =
            load_dashboard_contract_fixture(&success_config.fixture_path).expect("fixture");
        let mut success_runtime = DashboardRuntime::new(success_config.clone()).expect("runtime");
        let success = success_runtime
            .run_once(&success_fixture)
            .await
            .expect("success cycle");
        assert_eq!(success.failed_cases, 0);
        assert_eq!(success.applied_cases, 2);
        let state_after_success =
            load_dashboard_runtime_state(&success_config.state_dir.join("state.json"))
                .expect("load state after success");
        assert_eq!(state_after_success.health.failure_streak, 0);
        assert_eq!(
            state_after_success.health.classify().state,
            TransportHealthState::Healthy
        );
    }

    #[tokio::test]
    async fn regression_runner_events_log_contains_widget_and_control_reason_codes() {
        let temp = tempdir().expect("tempdir");
        let mut config = build_config(temp.path());
        config.fixture_path = fixture_path("snapshot-layout.json");
        let fixture =
            load_dashboard_contract_fixture(&config.fixture_path).expect("fixture should load");
        let mut runtime = DashboardRuntime::new(config.clone()).expect("runtime");
        let summary = runtime.run_once(&fixture).await.expect("run once");
        assert_eq!(summary.failed_cases, 0);
        assert_eq!(summary.control_actions_applied, 1);

        let events_log =
            std::fs::read_to_string(config.state_dir.join(DASHBOARD_RUNTIME_EVENTS_LOG_FILE))
                .expect("read runtime events");
        let parsed = events_log
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("valid json line"))
            .collect::<Vec<_>>();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["health_state"].as_str(), Some("healthy"));
        let reason_codes = parsed[0]["reason_codes"]
            .as_array()
            .expect("reason codes array");
        assert!(reason_codes
            .iter()
            .any(|value| value.as_str() == Some("widget_views_updated")));
        assert!(reason_codes
            .iter()
            .any(|value| value.as_str() == Some("control_actions_applied")));
    }
}
