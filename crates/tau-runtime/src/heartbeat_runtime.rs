use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const RUNTIME_HEARTBEAT_SCHEMA_VERSION: u32 = 1;
const RUNTIME_HEARTBEAT_EVENTS_LOG_FILE: &str = "runtime-heartbeat-events.jsonl";
const RUNTIME_HEARTBEAT_STATE_RUNNING: &str = "running";
const RUNTIME_HEARTBEAT_STATE_STOPPED: &str = "stopped";
const RUNTIME_HEARTBEAT_STATE_DISABLED: &str = "disabled";
const RUNTIME_HEARTBEAT_STATE_UNKNOWN: &str = "unknown";
const RUNTIME_HEARTBEAT_REASON_CYCLE_OK: &str = "heartbeat_cycle_ok";
const RUNTIME_HEARTBEAT_REASON_BACKLOG: &str = "heartbeat_maintenance_backlog";
const RUNTIME_HEARTBEAT_REASON_SELF_REPAIR: &str = "heartbeat_self_repair_applied";
const RUNTIME_HEARTBEAT_REASON_STOPPED: &str = "heartbeat_stopped";
const RUNTIME_HEARTBEAT_REASON_DISABLED: &str = "heartbeat_disabled";
const RUNTIME_HEARTBEAT_REASON_STATE_MISSING: &str = "heartbeat_state_missing";
const DEFAULT_MAINTENANCE_TEMP_MAX_AGE_SECONDS: u64 = 3_600;
const DEFAULT_SELF_REPAIR_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_SELF_REPAIR_MAX_RETRIES: usize = 2;
const DEFAULT_SELF_REPAIR_ORPHAN_MAX_AGE_SECONDS: u64 = 3_600;

const REPAIR_WORK_ITEM_STATUS_RUNNING: &str = "running";
const REPAIR_WORK_ITEM_STATUS_BUILDING: &str = "building";
const REPAIR_WORK_ITEM_STATUS_TIMED_OUT: &str = "timed_out";
const REPAIR_WORK_ITEM_STATUS_QUEUED: &str = "queued";
const REPAIR_WORK_ITEM_STATUS_REBUILD_QUEUED: &str = "rebuild_queued";
const REPAIR_WORK_ITEM_STATUS_FAILED: &str = "failed";
const REPAIR_WORK_ITEM_STATUS_SUCCEEDED: &str = "succeeded";
const REPAIR_WORK_ITEM_STATUS_CANCELLED: &str = "cancelled";

fn runtime_heartbeat_schema_version() -> u32 {
    RUNTIME_HEARTBEAT_SCHEMA_VERSION
}

fn default_runtime_heartbeat_reason_code() -> String {
    RUNTIME_HEARTBEAT_REASON_STATE_MISSING.to_string()
}

fn default_runtime_heartbeat_state() -> String {
    RUNTIME_HEARTBEAT_STATE_UNKNOWN.to_string()
}

fn runtime_repair_work_item_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `RuntimeHeartbeatSchedulerConfig` used across Tau components.
pub struct RuntimeHeartbeatSchedulerConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub state_path: PathBuf,
    pub queue_state_paths: Vec<PathBuf>,
    pub events_dir: Option<PathBuf>,
    pub jobs_dir: Option<PathBuf>,
    pub self_repair_enabled: bool,
    pub self_repair_timeout: Duration,
    pub self_repair_max_retries: usize,
    pub self_repair_tool_builds_dir: Option<PathBuf>,
    pub self_repair_orphan_artifact_max_age: Duration,
    pub maintenance_temp_dirs: Vec<PathBuf>,
    pub maintenance_temp_max_age: Duration,
}

impl RuntimeHeartbeatSchedulerConfig {
    fn interval_ms(&self) -> u64 {
        u64::try_from(self.interval.as_millis()).unwrap_or(u64::MAX)
    }

    fn events_log_path(&self) -> PathBuf {
        heartbeat_events_log_path(&self.state_path)
    }
}

impl Default for RuntimeHeartbeatSchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: Duration::from_secs(5),
            state_path: PathBuf::from(".tau/runtime-heartbeat/state.json"),
            queue_state_paths: Vec::new(),
            events_dir: None,
            jobs_dir: None,
            self_repair_enabled: true,
            self_repair_timeout: Duration::from_secs(DEFAULT_SELF_REPAIR_TIMEOUT_SECONDS),
            self_repair_max_retries: DEFAULT_SELF_REPAIR_MAX_RETRIES,
            self_repair_tool_builds_dir: None,
            self_repair_orphan_artifact_max_age: Duration::from_secs(
                DEFAULT_SELF_REPAIR_ORPHAN_MAX_AGE_SECONDS,
            ),
            maintenance_temp_dirs: Vec::new(),
            maintenance_temp_max_age: Duration::from_secs(DEFAULT_MAINTENANCE_TEMP_MAX_AGE_SECONDS),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Public struct `RuntimeHeartbeatSnapshot` used across Tau components.
pub struct RuntimeHeartbeatSnapshot {
    #[serde(default = "runtime_heartbeat_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub updated_unix_ms: u64,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_runtime_heartbeat_state")]
    pub run_state: String,
    #[serde(default = "default_runtime_heartbeat_reason_code")]
    pub reason_code: String,
    #[serde(default)]
    pub interval_ms: u64,
    #[serde(default)]
    pub tick_count: u64,
    #[serde(default)]
    pub last_tick_unix_ms: u64,
    #[serde(default)]
    pub queue_depth: usize,
    #[serde(default)]
    pub pending_events: usize,
    #[serde(default)]
    pub pending_jobs: usize,
    #[serde(default)]
    pub temp_files_cleaned: usize,
    #[serde(default)]
    pub stuck_jobs: usize,
    #[serde(default)]
    pub stuck_tool_builds: usize,
    #[serde(default)]
    pub repair_actions: usize,
    #[serde(default)]
    pub retries_queued: usize,
    #[serde(default)]
    pub retries_exhausted: usize,
    #[serde(default)]
    pub orphan_artifacts_cleaned: usize,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub state_path: String,
}

impl Default for RuntimeHeartbeatSnapshot {
    fn default() -> Self {
        Self {
            schema_version: RUNTIME_HEARTBEAT_SCHEMA_VERSION,
            updated_unix_ms: current_unix_timestamp_ms(),
            enabled: false,
            run_state: RUNTIME_HEARTBEAT_STATE_UNKNOWN.to_string(),
            reason_code: RUNTIME_HEARTBEAT_REASON_STATE_MISSING.to_string(),
            interval_ms: 0,
            tick_count: 0,
            last_tick_unix_ms: 0,
            queue_depth: 0,
            pending_events: 0,
            pending_jobs: 0,
            temp_files_cleaned: 0,
            stuck_jobs: 0,
            stuck_tool_builds: 0,
            repair_actions: 0,
            retries_queued: 0,
            retries_exhausted: 0,
            orphan_artifacts_cleaned: 0,
            reason_codes: Vec::new(),
            diagnostics: Vec::new(),
            state_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeHeartbeatCycleReport {
    timestamp_unix_ms: u64,
    run_state: String,
    reason_code: String,
    reason_codes: Vec<String>,
    queue_depth: usize,
    pending_events: usize,
    pending_jobs: usize,
    temp_files_cleaned: usize,
    stuck_jobs: usize,
    stuck_tool_builds: usize,
    repair_actions: usize,
    retries_queued: usize,
    retries_exhausted: usize,
    orphan_artifacts_cleaned: usize,
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct RuntimeSelfRepairSummary {
    stuck_jobs: usize,
    stuck_tool_builds: usize,
    repair_actions: usize,
    retries_queued: usize,
    retries_exhausted: usize,
    orphan_artifacts_cleaned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeRepairWorkItem {
    #[serde(default = "runtime_repair_work_item_schema_version")]
    schema_version: u32,
    #[serde(default)]
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    retryable: bool,
    #[serde(default)]
    retry_count: usize,
    #[serde(default)]
    max_retries: usize,
    #[serde(default)]
    started_unix_ms: u64,
    #[serde(default)]
    updated_unix_ms: u64,
    #[serde(default)]
    reason_code: String,
    #[serde(default)]
    artifact_paths: Vec<String>,
    #[serde(default)]
    temp_paths: Vec<String>,
    #[serde(default)]
    diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
struct RuntimeHeartbeatCycleResult {
    snapshot: RuntimeHeartbeatSnapshot,
    report: RuntimeHeartbeatCycleReport,
}

#[derive(Debug)]
/// Public struct `RuntimeHeartbeatHandle` used across Tau components.
pub struct RuntimeHeartbeatHandle {
    state_path: PathBuf,
    enabled: bool,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl RuntimeHeartbeatHandle {
    fn disabled(state_path: PathBuf) -> Self {
        Self {
            state_path,
            enabled: false,
            shutdown_tx: None,
            task: None,
        }
    }

    fn running(
        state_path: PathBuf,
        shutdown_tx: oneshot::Sender<()>,
        task: JoinHandle<()>,
    ) -> Self {
        Self {
            state_path,
            enabled: true,
            shutdown_tx: Some(shutdown_tx),
            task: Some(task),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn state_path(&self) -> &Path {
        self.state_path.as_path()
    }

    pub fn is_running(&self) -> bool {
        self.task.is_some()
    }

    pub async fn shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

pub fn start_runtime_heartbeat_scheduler(
    config: RuntimeHeartbeatSchedulerConfig,
) -> Result<RuntimeHeartbeatHandle> {
    if config.interval.is_zero() {
        anyhow::bail!("runtime heartbeat interval must be greater than zero");
    }

    if !config.enabled {
        let mut snapshot = RuntimeHeartbeatSnapshot {
            schema_version: RUNTIME_HEARTBEAT_SCHEMA_VERSION,
            updated_unix_ms: current_unix_timestamp_ms(),
            enabled: false,
            run_state: RUNTIME_HEARTBEAT_STATE_DISABLED.to_string(),
            reason_code: RUNTIME_HEARTBEAT_REASON_DISABLED.to_string(),
            interval_ms: config.interval_ms(),
            tick_count: 0,
            last_tick_unix_ms: 0,
            queue_depth: 0,
            pending_events: 0,
            pending_jobs: 0,
            temp_files_cleaned: 0,
            stuck_jobs: 0,
            stuck_tool_builds: 0,
            repair_actions: 0,
            retries_queued: 0,
            retries_exhausted: 0,
            orphan_artifacts_cleaned: 0,
            reason_codes: vec![RUNTIME_HEARTBEAT_REASON_DISABLED.to_string()],
            diagnostics: Vec::new(),
            state_path: config.state_path.display().to_string(),
        };
        snapshot.diagnostics.push(format!(
            "heartbeat_disabled: state_path={}",
            config.state_path.display()
        ));
        persist_runtime_heartbeat_snapshot(&config.state_path, &snapshot)?;
        return Ok(RuntimeHeartbeatHandle::disabled(config.state_path));
    }

    let handle = tokio::runtime::Handle::try_current()
        .context("runtime heartbeat scheduler requires an active Tokio runtime")?;

    let bootstrap_snapshot = RuntimeHeartbeatSnapshot {
        schema_version: RUNTIME_HEARTBEAT_SCHEMA_VERSION,
        updated_unix_ms: current_unix_timestamp_ms(),
        enabled: true,
        run_state: RUNTIME_HEARTBEAT_STATE_RUNNING.to_string(),
        reason_code: RUNTIME_HEARTBEAT_REASON_CYCLE_OK.to_string(),
        interval_ms: config.interval_ms(),
        tick_count: 0,
        last_tick_unix_ms: 0,
        queue_depth: 0,
        pending_events: 0,
        pending_jobs: 0,
        temp_files_cleaned: 0,
        stuck_jobs: 0,
        stuck_tool_builds: 0,
        repair_actions: 0,
        retries_queued: 0,
        retries_exhausted: 0,
        orphan_artifacts_cleaned: 0,
        reason_codes: vec!["heartbeat_started".to_string()],
        diagnostics: vec![format!(
            "heartbeat_started: interval_ms={} state_path={}",
            config.interval_ms(),
            config.state_path.display()
        )],
        state_path: config.state_path.display().to_string(),
    };
    persist_runtime_heartbeat_snapshot(&config.state_path, &bootstrap_snapshot)?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let state_path = config.state_path.clone();
    let task = handle.spawn(async move {
        run_runtime_heartbeat_loop(config, shutdown_rx).await;
    });
    Ok(RuntimeHeartbeatHandle::running(
        state_path,
        shutdown_tx,
        task,
    ))
}

pub fn inspect_runtime_heartbeat(state_path: &Path) -> RuntimeHeartbeatSnapshot {
    let mut snapshot = RuntimeHeartbeatSnapshot {
        state_path: state_path.display().to_string(),
        ..RuntimeHeartbeatSnapshot::default()
    };
    if !state_path.exists() {
        snapshot.reason_code = RUNTIME_HEARTBEAT_REASON_STATE_MISSING.to_string();
        snapshot
            .reason_codes
            .push(RUNTIME_HEARTBEAT_REASON_STATE_MISSING.to_string());
        snapshot
            .diagnostics
            .push(format!("state_missing: path={}", state_path.display()));
        return snapshot;
    }

    let raw = match std::fs::read_to_string(state_path) {
        Ok(raw) => raw,
        Err(error) => {
            snapshot.reason_code = "heartbeat_state_read_failed".to_string();
            snapshot
                .reason_codes
                .push("heartbeat_state_read_failed".to_string());
            snapshot.diagnostics.push(format!(
                "state_read_failed: path={} error={error}",
                state_path.display()
            ));
            return snapshot;
        }
    };

    match serde_json::from_str::<RuntimeHeartbeatSnapshot>(&raw) {
        Ok(mut parsed) => {
            parsed.state_path = state_path.display().to_string();
            parsed
        }
        Err(error) => {
            snapshot.reason_code = "heartbeat_state_parse_failed".to_string();
            snapshot
                .reason_codes
                .push("heartbeat_state_parse_failed".to_string());
            snapshot.diagnostics.push(format!(
                "state_parse_failed: path={} error={error}",
                state_path.display()
            ));
            snapshot
        }
    }
}

async fn run_runtime_heartbeat_loop(
    config: RuntimeHeartbeatSchedulerConfig,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut tick_count = 0_u64;
    let mut interval = tokio::time::interval(config.interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tick_count = tick_count.saturating_add(1);
                let cycle = execute_runtime_heartbeat_cycle(&config, tick_count);
                if let Err(error) = persist_runtime_heartbeat_snapshot(&config.state_path, &cycle.snapshot) {
                    eprintln!("runtime heartbeat snapshot persist failed: path={} error={error}", config.state_path.display());
                }
                if let Err(error) = append_runtime_heartbeat_cycle_report(&config.events_log_path(), &cycle.report) {
                    eprintln!("runtime heartbeat cycle log append failed: path={} error={error}", config.events_log_path().display());
                }
            }
            _ = &mut shutdown_rx => {
                let snapshot = RuntimeHeartbeatSnapshot {
                    schema_version: RUNTIME_HEARTBEAT_SCHEMA_VERSION,
                    updated_unix_ms: current_unix_timestamp_ms(),
                    enabled: true,
                    run_state: RUNTIME_HEARTBEAT_STATE_STOPPED.to_string(),
                    reason_code: RUNTIME_HEARTBEAT_REASON_STOPPED.to_string(),
                    interval_ms: config.interval_ms(),
                    tick_count,
                    last_tick_unix_ms: current_unix_timestamp_ms(),
                    queue_depth: 0,
                    pending_events: 0,
                    pending_jobs: 0,
                    temp_files_cleaned: 0,
                    stuck_jobs: 0,
                    stuck_tool_builds: 0,
                    repair_actions: 0,
                    retries_queued: 0,
                    retries_exhausted: 0,
                    orphan_artifacts_cleaned: 0,
                    reason_codes: vec![RUNTIME_HEARTBEAT_REASON_STOPPED.to_string()],
                    diagnostics: vec![format!(
                        "heartbeat_stopped: state_path={} ticks={tick_count}",
                        config.state_path.display()
                    )],
                    state_path: config.state_path.display().to_string(),
                };
                if let Err(error) = persist_runtime_heartbeat_snapshot(&config.state_path, &snapshot) {
                    eprintln!("runtime heartbeat stop snapshot persist failed: path={} error={error}", config.state_path.display());
                }
                break;
            }
        }
    }
}

fn execute_runtime_heartbeat_cycle(
    config: &RuntimeHeartbeatSchedulerConfig,
    tick_count: u64,
) -> RuntimeHeartbeatCycleResult {
    let now = current_unix_timestamp_ms();
    let mut diagnostics = Vec::new();
    let mut reason_codes = Vec::new();

    let queue_depth = collect_queue_depth(
        &config.queue_state_paths,
        &mut diagnostics,
        &mut reason_codes,
    );
    if queue_depth > 0 {
        push_unique_reason_code(&mut reason_codes, "queue_backlog_detected");
    }

    let pending_events = collect_pending_count(
        config.events_dir.as_deref(),
        "events",
        &mut diagnostics,
        &mut reason_codes,
    );
    if pending_events > 0 {
        push_unique_reason_code(&mut reason_codes, "events_pending");
    }

    let pending_jobs = collect_pending_count(
        config.jobs_dir.as_deref(),
        "jobs",
        &mut diagnostics,
        &mut reason_codes,
    );
    if pending_jobs > 0 {
        push_unique_reason_code(&mut reason_codes, "jobs_pending");
    }

    let temp_files_cleaned = cleanup_temp_files(
        &config.maintenance_temp_dirs,
        config.maintenance_temp_max_age,
        &mut diagnostics,
        &mut reason_codes,
    );
    if temp_files_cleaned > 0 {
        push_unique_reason_code(&mut reason_codes, "stale_temp_files_cleaned");
    }

    let self_repair = execute_runtime_self_repair(config, now, &mut diagnostics, &mut reason_codes);

    if reason_codes.is_empty() {
        reason_codes.push("heartbeat_cycle_clean".to_string());
    }

    let reason_code = if self_repair.repair_actions > 0 {
        RUNTIME_HEARTBEAT_REASON_SELF_REPAIR.to_string()
    } else if queue_depth > 0 || pending_events > 0 || pending_jobs > 0 {
        RUNTIME_HEARTBEAT_REASON_BACKLOG.to_string()
    } else {
        RUNTIME_HEARTBEAT_REASON_CYCLE_OK.to_string()
    };

    let snapshot = RuntimeHeartbeatSnapshot {
        schema_version: RUNTIME_HEARTBEAT_SCHEMA_VERSION,
        updated_unix_ms: now,
        enabled: true,
        run_state: RUNTIME_HEARTBEAT_STATE_RUNNING.to_string(),
        reason_code: reason_code.clone(),
        interval_ms: config.interval_ms(),
        tick_count,
        last_tick_unix_ms: now,
        queue_depth,
        pending_events,
        pending_jobs,
        temp_files_cleaned,
        stuck_jobs: self_repair.stuck_jobs,
        stuck_tool_builds: self_repair.stuck_tool_builds,
        repair_actions: self_repair.repair_actions,
        retries_queued: self_repair.retries_queued,
        retries_exhausted: self_repair.retries_exhausted,
        orphan_artifacts_cleaned: self_repair.orphan_artifacts_cleaned,
        reason_codes: reason_codes.clone(),
        diagnostics: diagnostics.clone(),
        state_path: config.state_path.display().to_string(),
    };
    let report = RuntimeHeartbeatCycleReport {
        timestamp_unix_ms: now,
        run_state: RUNTIME_HEARTBEAT_STATE_RUNNING.to_string(),
        reason_code,
        reason_codes,
        queue_depth,
        pending_events,
        pending_jobs,
        temp_files_cleaned,
        stuck_jobs: self_repair.stuck_jobs,
        stuck_tool_builds: self_repair.stuck_tool_builds,
        repair_actions: self_repair.repair_actions,
        retries_queued: self_repair.retries_queued,
        retries_exhausted: self_repair.retries_exhausted,
        orphan_artifacts_cleaned: self_repair.orphan_artifacts_cleaned,
        diagnostics,
    };
    RuntimeHeartbeatCycleResult { snapshot, report }
}

fn execute_runtime_self_repair(
    config: &RuntimeHeartbeatSchedulerConfig,
    now_unix_ms: u64,
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
) -> RuntimeSelfRepairSummary {
    if !config.self_repair_enabled {
        diagnostics.push("self_repair_disabled".to_string());
        return RuntimeSelfRepairSummary::default();
    }

    let timeout_ms = u64::try_from(config.self_repair_timeout.as_millis()).unwrap_or(u64::MAX);
    let max_retries = config.self_repair_max_retries.max(1);
    let orphan_max_age_ms =
        u64::try_from(config.self_repair_orphan_artifact_max_age.as_millis()).unwrap_or(u64::MAX);

    let mut summary = RuntimeSelfRepairSummary::default();
    process_repair_work_item_dir(
        config.jobs_dir.as_deref(),
        "job",
        now_unix_ms,
        timeout_ms,
        max_retries,
        orphan_max_age_ms,
        diagnostics,
        reason_codes,
        &mut summary,
    );
    process_repair_work_item_dir(
        config.self_repair_tool_builds_dir.as_deref(),
        "tool_build",
        now_unix_ms,
        timeout_ms,
        max_retries,
        orphan_max_age_ms,
        diagnostics,
        reason_codes,
        &mut summary,
    );

    summary
}

#[allow(clippy::too_many_arguments)]
fn process_repair_work_item_dir(
    dir: Option<&Path>,
    kind: &str,
    now_unix_ms: u64,
    timeout_ms: u64,
    max_retries: usize,
    orphan_max_age_ms: u64,
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
    summary: &mut RuntimeSelfRepairSummary,
) {
    let Some(dir) = dir else {
        diagnostics.push(format!("self_repair_{kind}_dir_not_configured"));
        return;
    };

    if !dir.exists() {
        diagnostics.push(format!(
            "self_repair_{kind}_dir_missing: path={}",
            dir.display()
        ));
        return;
    }
    if !dir.is_dir() {
        diagnostics.push(format!(
            "self_repair_{kind}_dir_not_directory: path={}",
            dir.display()
        ));
        push_unique_reason_code(reason_codes, "self_repair_dir_error");
        return;
    }

    let manifest_paths = match collect_repair_manifest_paths(dir) {
        Ok(paths) => paths,
        Err(error) => {
            diagnostics.push(format!(
                "self_repair_{kind}_dir_read_error: path={} error={error}",
                dir.display()
            ));
            push_unique_reason_code(reason_codes, "self_repair_dir_error");
            return;
        }
    };

    for manifest_path in manifest_paths {
        if let Err(error) = process_repair_work_item_manifest(
            &manifest_path,
            kind,
            now_unix_ms,
            timeout_ms,
            max_retries,
            orphan_max_age_ms,
            diagnostics,
            reason_codes,
            summary,
        ) {
            diagnostics.push(format!(
                "self_repair_manifest_error: kind={} path={} error={error}",
                kind,
                manifest_path.display()
            ));
            push_unique_reason_code(reason_codes, "self_repair_manifest_error");
        }
    }
}

fn collect_repair_manifest_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", entry.path().display()))?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let is_json = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("json"))
            .unwrap_or(false);
        if is_json {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[allow(clippy::too_many_arguments)]
fn process_repair_work_item_manifest(
    manifest_path: &Path,
    kind: &str,
    now_unix_ms: u64,
    timeout_ms: u64,
    max_retries: usize,
    orphan_max_age_ms: u64,
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
    summary: &mut RuntimeSelfRepairSummary,
) -> Result<()> {
    let raw = std::fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut item = serde_json::from_str::<RuntimeRepairWorkItem>(&raw)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    if item.id.trim().is_empty() {
        item.id = manifest_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();
    }

    let mut changed = false;
    let mut stuck_detected = false;
    let status = normalize_repair_work_item_status(&item.status);
    let last_activity_unix_ms = item.updated_unix_ms.max(item.started_unix_ms);
    let elapsed_ms = if last_activity_unix_ms == 0 {
        0
    } else {
        now_unix_ms.saturating_sub(last_activity_unix_ms)
    };

    if is_running_repair_work_item_status(kind, status.as_str()) && elapsed_ms >= timeout_ms {
        changed = true;
        stuck_detected = true;
        summary.repair_actions = summary.repair_actions.saturating_add(1);
        match kind {
            "job" => {
                summary.stuck_jobs = summary.stuck_jobs.saturating_add(1);
                push_unique_reason_code(reason_codes, "self_repair_stuck_job_detected");
            }
            "tool_build" => {
                summary.stuck_tool_builds = summary.stuck_tool_builds.saturating_add(1);
                push_unique_reason_code(reason_codes, "self_repair_stuck_tool_build_detected");
            }
            _ => {}
        }
        diagnostics.push(format!(
            "self_repair_stuck_detected: kind={} id={} path={} status={} elapsed_ms={} timeout_ms={}",
            kind,
            item.id,
            manifest_path.display(),
            status,
            elapsed_ms,
            timeout_ms,
        ));
        item.status = REPAIR_WORK_ITEM_STATUS_TIMED_OUT.to_string();
        item.reason_code = "self_repair_timed_out".to_string();
        item.updated_unix_ms = now_unix_ms;
        item.diagnostics.push(format!(
            "self_repair_timed_out: elapsed_ms={} timeout_ms={}",
            elapsed_ms, timeout_ms
        ));

        if item.retryable {
            let retry_budget = if item.max_retries == 0 {
                max_retries
            } else {
                item.max_retries
            };
            if item.retry_count < retry_budget {
                item.retry_count = item.retry_count.saturating_add(1);
                item.updated_unix_ms = now_unix_ms;
                item.status = match kind {
                    "tool_build" => REPAIR_WORK_ITEM_STATUS_REBUILD_QUEUED.to_string(),
                    _ => REPAIR_WORK_ITEM_STATUS_QUEUED.to_string(),
                };
                item.reason_code = match kind {
                    "tool_build" => "self_repair_rebuild_queued".to_string(),
                    _ => "self_repair_retry_queued".to_string(),
                };
                item.diagnostics.push(format!(
                    "self_repair_retry_queued: retry_count={} max_retries={retry_budget}",
                    item.retry_count
                ));
                summary.retries_queued = summary.retries_queued.saturating_add(1);
                summary.repair_actions = summary.repair_actions.saturating_add(1);
                push_unique_reason_code(reason_codes, item.reason_code.as_str());
                diagnostics.push(format!(
                    "self_repair_retry_queued: kind={} id={} path={} retry_count={} max_retries={retry_budget}",
                    kind,
                    item.id,
                    manifest_path.display(),
                    item.retry_count
                ));
            } else {
                item.updated_unix_ms = now_unix_ms;
                item.status = REPAIR_WORK_ITEM_STATUS_FAILED.to_string();
                item.reason_code = "self_repair_retry_exhausted".to_string();
                item.diagnostics.push(format!(
                    "self_repair_retry_exhausted: retry_count={} max_retries={retry_budget}",
                    item.retry_count
                ));
                summary.retries_exhausted = summary.retries_exhausted.saturating_add(1);
                summary.repair_actions = summary.repair_actions.saturating_add(1);
                push_unique_reason_code(reason_codes, "self_repair_retry_exhausted");
                diagnostics.push(format!(
                    "self_repair_retry_exhausted: kind={} id={} path={} retry_count={} max_retries={retry_budget}",
                    kind,
                    item.id,
                    manifest_path.display(),
                    item.retry_count
                ));
            }
        }
    }

    let status_after_repair = normalize_repair_work_item_status(&item.status);
    let last_activity_after_repair = item.updated_unix_ms.max(item.started_unix_ms);
    let elapsed_after_repair_ms = if last_activity_after_repair == 0 {
        0
    } else {
        now_unix_ms.saturating_sub(last_activity_after_repair)
    };
    let stale_for_orphan_cleanup = if stuck_detected {
        elapsed_ms >= orphan_max_age_ms
    } else {
        elapsed_after_repair_ms >= orphan_max_age_ms
    };
    if (is_terminal_repair_work_item_status(status_after_repair.as_str()) || stuck_detected)
        && stale_for_orphan_cleanup
    {
        let (artifact_removed, artifact_changed) = cleanup_orphan_path_entries(
            manifest_path,
            &mut item.artifact_paths,
            "artifact",
            diagnostics,
            reason_codes,
        );
        let (temp_removed, temp_changed) = cleanup_orphan_path_entries(
            manifest_path,
            &mut item.temp_paths,
            "temp",
            diagnostics,
            reason_codes,
        );
        let total_removed = artifact_removed.saturating_add(temp_removed);
        if total_removed > 0 {
            changed = true;
            summary.orphan_artifacts_cleaned = summary
                .orphan_artifacts_cleaned
                .saturating_add(total_removed);
            summary.repair_actions = summary.repair_actions.saturating_add(1);
            item.reason_code = "self_repair_orphan_cleanup".to_string();
            item.updated_unix_ms = now_unix_ms;
            item.diagnostics.push(format!(
                "self_repair_orphan_cleanup: removed={total_removed} age_ms={elapsed_after_repair_ms} orphan_max_age_ms={orphan_max_age_ms}"
            ));
            push_unique_reason_code(reason_codes, "orphan_resources_cleaned");
            diagnostics.push(format!(
                "self_repair_orphan_cleanup: kind={} id={} path={} removed={total_removed}",
                kind,
                item.id,
                manifest_path.display()
            ));
        } else if artifact_changed || temp_changed {
            changed = true;
        }
    }

    if changed {
        persist_repair_work_item_manifest(manifest_path, &item)?;
    }
    Ok(())
}

fn cleanup_orphan_path_entries(
    manifest_path: &Path,
    entries: &mut Vec<String>,
    label: &str,
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
) -> (usize, bool) {
    if entries.is_empty() {
        return (0, false);
    }

    let original = entries.clone();
    let mut removed = 0_usize;
    let mut retained = Vec::new();
    for raw_entry in &original {
        let entry = raw_entry.trim();
        if entry.is_empty() {
            continue;
        }
        let resolved = match resolve_repair_path(manifest_path, entry) {
            Ok(path) => path,
            Err(error) => {
                retained.push(entry.to_string());
                diagnostics.push(format!(
                    "self_repair_orphan_path_invalid: path={} entry={} error={error}",
                    manifest_path.display(),
                    entry
                ));
                push_unique_reason_code(reason_codes, "self_repair_orphan_cleanup_error");
                continue;
            }
        };
        if !resolved.exists() {
            continue;
        }

        let remove_result = if resolved.is_file() {
            std::fs::remove_file(&resolved)
        } else if resolved.is_dir() {
            std::fs::remove_dir_all(&resolved)
        } else {
            retained.push(entry.to_string());
            continue;
        };
        match remove_result {
            Ok(()) => {
                removed = removed.saturating_add(1);
                diagnostics.push(format!(
                    "self_repair_orphan_path_removed: kind={label} manifest={} path={}",
                    manifest_path.display(),
                    resolved.display()
                ));
            }
            Err(error) => {
                retained.push(entry.to_string());
                diagnostics.push(format!(
                    "self_repair_orphan_path_remove_error: kind={label} manifest={} path={} error={error}",
                    manifest_path.display(),
                    resolved.display()
                ));
                push_unique_reason_code(reason_codes, "self_repair_orphan_cleanup_error");
            }
        }
    }
    let changed = retained != original;
    if changed {
        *entries = retained;
    }
    (removed, changed)
}

fn resolve_repair_path(manifest_path: &Path, entry: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(entry);
    if candidate.is_absolute() {
        return Ok(candidate);
    }
    if candidate
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        anyhow::bail!("relative orphan path must not traverse parent directories");
    }
    let parent = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(parent.join(candidate))
}

fn normalize_repair_work_item_status(status: &str) -> String {
    status.trim().to_ascii_lowercase()
}

fn is_running_repair_work_item_status(kind: &str, status: &str) -> bool {
    match kind {
        "tool_build" => {
            status == REPAIR_WORK_ITEM_STATUS_RUNNING || status == REPAIR_WORK_ITEM_STATUS_BUILDING
        }
        _ => status == REPAIR_WORK_ITEM_STATUS_RUNNING,
    }
}

fn is_terminal_repair_work_item_status(status: &str) -> bool {
    matches!(
        status,
        REPAIR_WORK_ITEM_STATUS_FAILED
            | REPAIR_WORK_ITEM_STATUS_TIMED_OUT
            | REPAIR_WORK_ITEM_STATUS_SUCCEEDED
            | REPAIR_WORK_ITEM_STATUS_CANCELLED
    )
}

fn persist_repair_work_item_manifest(path: &Path, item: &RuntimeRepairWorkItem) -> Result<()> {
    let payload =
        serde_json::to_string_pretty(item).context("failed to serialize self-repair work item")?;
    write_text_atomic(path, &payload)
}

fn collect_queue_depth(
    queue_state_paths: &[PathBuf],
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
) -> usize {
    if queue_state_paths.is_empty() {
        diagnostics.push("queue_probe_not_configured".to_string());
        push_unique_reason_code(reason_codes, "queue_probe_not_configured");
        return 0;
    }

    let mut queue_depth = 0_usize;
    for path in queue_state_paths {
        match queue_depth_from_state(path) {
            Ok(Some(depth)) => {
                queue_depth = queue_depth.saturating_add(depth);
                diagnostics.push(format!(
                    "queue_state_checked: path={} queue_depth={depth}",
                    path.display()
                ));
            }
            Ok(None) => {
                diagnostics.push(format!("queue_state_missing: path={}", path.display()));
            }
            Err(error) => {
                diagnostics.push(format!(
                    "queue_state_error: path={} error={error}",
                    path.display()
                ));
                push_unique_reason_code(reason_codes, "queue_probe_error");
            }
        }
    }
    queue_depth
}

fn queue_depth_from_state(path: &Path) -> Result<Option<usize>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read queue state {}", path.display()))?;
    let parsed = serde_json::from_str::<Value>(&raw)
        .with_context(|| format!("failed to parse queue state {}", path.display()))?;
    let depth = parsed
        .get("health")
        .and_then(|health| health.get("queue_depth"))
        .and_then(Value::as_u64)
        .or_else(|| parsed.get("queue_depth").and_then(Value::as_u64))
        .map(u64_to_usize)
        .unwrap_or(0);
    Ok(Some(depth))
}

fn collect_pending_count(
    path: Option<&Path>,
    label: &str,
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
) -> usize {
    let Some(path) = path else {
        diagnostics.push(format!("{label}_probe_not_configured"));
        push_unique_reason_code(reason_codes, &format!("{label}_probe_not_configured"));
        return 0;
    };
    match count_regular_files(path) {
        Ok(count) => {
            diagnostics.push(format!(
                "{label}_checked: path={} count={count}",
                path.display()
            ));
            count
        }
        Err(error) => {
            diagnostics.push(format!(
                "{label}_probe_error: path={} error={error}",
                path.display()
            ));
            push_unique_reason_code(reason_codes, &format!("{label}_probe_error"));
            0
        }
    }
}

fn cleanup_temp_files(
    temp_dirs: &[PathBuf],
    max_age: Duration,
    diagnostics: &mut Vec<String>,
    reason_codes: &mut Vec<String>,
) -> usize {
    if temp_dirs.is_empty() {
        diagnostics.push("temp_cleanup_not_configured".to_string());
        return 0;
    }

    let mut total_removed = 0_usize;
    for temp_dir in temp_dirs {
        match cleanup_stale_files_in_dir(temp_dir, max_age) {
            Ok(removed) => {
                total_removed = total_removed.saturating_add(removed);
                diagnostics.push(format!(
                    "temp_cleanup_checked: path={} removed={removed}",
                    temp_dir.display()
                ));
            }
            Err(error) => {
                diagnostics.push(format!(
                    "temp_cleanup_error: path={} error={error}",
                    temp_dir.display()
                ));
                push_unique_reason_code(reason_codes, "temp_cleanup_error");
            }
        }
    }
    total_removed
}

fn cleanup_stale_files_in_dir(path: &Path, max_age: Duration) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    if !path.is_dir() {
        return Ok(0);
    }

    let now = SystemTime::now();
    let mut removed = 0_usize;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .with_context(|| format!("failed to read {}", current.display()))?;
        for entry in entries {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", current.display()))?;
            let entry_path = entry.path();
            let file_type = entry.file_type().with_context(|| {
                format!("failed to read file type for {}", entry_path.display())
            })?;
            if file_type.is_dir() {
                stack.push(entry_path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let metadata = entry
                .metadata()
                .with_context(|| format!("failed to read metadata for {}", entry_path.display()))?;
            let modified = metadata.modified().unwrap_or(now);
            let age = now.duration_since(modified).unwrap_or_default();
            if age >= max_age {
                std::fs::remove_file(&entry_path)
                    .with_context(|| format!("failed to remove {}", entry_path.display()))?;
                removed = removed.saturating_add(1);
            }
        }
    }
    Ok(removed)
}

fn count_regular_files(path: &Path) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    if path.is_file() {
        return Ok(1);
    }
    if !path.is_dir() {
        return Ok(0);
    }

    let mut count = 0_usize;
    let mut stack = vec![path.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .with_context(|| format!("failed to read {}", current.display()))?;
        for entry in entries {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", current.display()))?;
            let file_type = entry.file_type().with_context(|| {
                format!("failed to read file type for {}", entry.path().display())
            })?;
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                count = count.saturating_add(1);
            }
        }
    }
    Ok(count)
}

fn persist_runtime_heartbeat_snapshot(
    state_path: &Path,
    snapshot: &RuntimeHeartbeatSnapshot,
) -> Result<()> {
    if let Some(parent) = state_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let payload = serde_json::to_string_pretty(snapshot)
        .context("failed to serialize runtime heartbeat snapshot")?;
    write_text_atomic(state_path, &payload)
}

fn append_runtime_heartbeat_cycle_report(
    events_log_path: &Path,
    report: &RuntimeHeartbeatCycleReport,
) -> Result<()> {
    if let Some(parent) = events_log_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let line =
        serde_json::to_string(report).context("failed to serialize runtime heartbeat cycle")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(events_log_path)
        .with_context(|| format!("failed to open {}", events_log_path.display()))?;
    writeln!(file, "{line}")
        .with_context(|| format!("failed to append {}", events_log_path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", events_log_path.display()))?;
    Ok(())
}

fn heartbeat_events_log_path(state_path: &Path) -> PathBuf {
    state_path
        .parent()
        .map(|parent| parent.join(RUNTIME_HEARTBEAT_EVENTS_LOG_FILE))
        .unwrap_or_else(|| PathBuf::from(RUNTIME_HEARTBEAT_EVENTS_LOG_FILE))
}

fn u64_to_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn push_unique_reason_code(reason_codes: &mut Vec<String>, reason_code: &str) {
    if reason_codes.iter().any(|existing| existing == reason_code) {
        return;
    }
    reason_codes.push(reason_code.to_string());
}

#[cfg(test)]
mod tests {
    use super::{
        inspect_runtime_heartbeat, start_runtime_heartbeat_scheduler,
        RuntimeHeartbeatSchedulerConfig,
    };
    use serde_json::Value;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::tempdir;

    fn scheduler_config(root: &Path, enabled: bool) -> RuntimeHeartbeatSchedulerConfig {
        RuntimeHeartbeatSchedulerConfig {
            enabled,
            interval: Duration::from_millis(10),
            state_path: root.join("heartbeat/state.json"),
            queue_state_paths: vec![root.join("queue/state.json")],
            events_dir: Some(root.join("events")),
            jobs_dir: Some(root.join("jobs")),
            self_repair_enabled: true,
            self_repair_timeout: Duration::from_secs(300),
            self_repair_max_retries: 2,
            self_repair_tool_builds_dir: Some(root.join("tool-builds")),
            self_repair_orphan_artifact_max_age: Duration::from_secs(3_600),
            maintenance_temp_dirs: vec![root.join("tmp")],
            maintenance_temp_max_age: Duration::from_secs(60),
        }
    }

    async fn wait_for_tick(state_path: &Path, timeout: Duration) {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            let snapshot = inspect_runtime_heartbeat(state_path);
            if snapshot.tick_count > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "runtime heartbeat did not report a tick before timeout for {}",
            state_path.display()
        );
    }

    #[test]
    fn unit_inspect_runtime_heartbeat_returns_missing_state_snapshot() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("missing/state.json");
        let snapshot = inspect_runtime_heartbeat(&state_path);
        assert_eq!(snapshot.run_state, "unknown");
        assert_eq!(snapshot.reason_code, "heartbeat_state_missing");
        assert!(snapshot
            .diagnostics
            .iter()
            .any(|line| line.contains("state_missing")));
    }

    #[tokio::test]
    async fn functional_runtime_heartbeat_scheduler_persists_tick_state_and_reason_codes() {
        let temp = tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("queue")).expect("create queue dir");
        std::fs::create_dir_all(temp.path().join("events")).expect("create events dir");
        std::fs::create_dir_all(temp.path().join("jobs")).expect("create jobs dir");
        std::fs::write(
            temp.path().join("queue/state.json"),
            r#"{"health":{"queue_depth":2}}"#,
        )
        .expect("write queue state");
        std::fs::write(temp.path().join("events/a.json"), "{}").expect("write event file");
        std::fs::write(temp.path().join("jobs/a.json"), "{}").expect("write job file");

        let config = scheduler_config(temp.path(), true);
        let mut handle =
            start_runtime_heartbeat_scheduler(config.clone()).expect("start runtime heartbeat");
        wait_for_tick(handle.state_path(), Duration::from_secs(2)).await;

        let snapshot = inspect_runtime_heartbeat(handle.state_path());
        assert_eq!(snapshot.run_state, "running");
        assert_eq!(snapshot.queue_depth, 2);
        assert_eq!(snapshot.pending_events, 1);
        assert_eq!(snapshot.pending_jobs, 1);
        assert!(snapshot.tick_count > 0);
        assert!(snapshot
            .reason_codes
            .iter()
            .any(|code| code == "queue_backlog_detected"));

        handle.shutdown().await;
        let stopped = inspect_runtime_heartbeat(handle.state_path());
        assert_eq!(stopped.run_state, "stopped");
        assert_eq!(stopped.reason_code, "heartbeat_stopped");
    }

    #[tokio::test]
    async fn integration_runtime_heartbeat_scheduler_removes_temp_files_when_stale() {
        let temp = tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("queue")).expect("create queue dir");
        std::fs::create_dir_all(temp.path().join("tmp")).expect("create temp dir");
        std::fs::write(temp.path().join("tmp/stale.tmp"), "temp").expect("write temp file");
        std::fs::write(temp.path().join("queue/state.json"), "{}").expect("write queue state");

        let mut config = scheduler_config(temp.path(), true);
        config.maintenance_temp_max_age = Duration::ZERO;

        let mut handle =
            start_runtime_heartbeat_scheduler(config.clone()).expect("start runtime heartbeat");
        wait_for_tick(handle.state_path(), Duration::from_secs(2)).await;

        assert!(!temp.path().join("tmp/stale.tmp").exists());
        let snapshot = inspect_runtime_heartbeat(handle.state_path());
        assert!(snapshot.temp_files_cleaned >= 1);
        assert!(snapshot
            .reason_codes
            .iter()
            .any(|code| code == "stale_temp_files_cleaned"));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn integration_runtime_heartbeat_scheduler_marks_stuck_jobs_and_queues_retry() {
        let temp = tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("queue")).expect("create queue dir");
        std::fs::create_dir_all(temp.path().join("jobs")).expect("create jobs dir");
        std::fs::write(temp.path().join("queue/state.json"), "{}").expect("write queue state");
        std::fs::write(
            temp.path().join("jobs/job-1.json"),
            r#"{
  "id": "job-1",
  "status": "running",
  "retryable": true,
  "retry_count": 0,
  "max_retries": 2,
  "started_unix_ms": 1,
  "updated_unix_ms": 1
}"#,
        )
        .expect("write job manifest");

        let mut config = scheduler_config(temp.path(), true);
        config.self_repair_timeout = Duration::ZERO;
        config.self_repair_max_retries = 2;

        let mut handle =
            start_runtime_heartbeat_scheduler(config).expect("start runtime heartbeat");
        wait_for_tick(handle.state_path(), Duration::from_secs(2)).await;

        let repaired_raw = std::fs::read_to_string(temp.path().join("jobs/job-1.json"))
            .expect("read repaired job");
        let repaired: Value = serde_json::from_str(&repaired_raw).expect("parse repaired job");
        assert_eq!(repaired["status"], "queued");
        assert_eq!(repaired["retry_count"], 1);
        assert_eq!(repaired["reason_code"], "self_repair_retry_queued");

        let snapshot = inspect_runtime_heartbeat(handle.state_path());
        assert!(snapshot.stuck_jobs >= 1);
        assert!(snapshot.retries_queued >= 1);
        assert!(snapshot.repair_actions >= 2);
        assert!(snapshot
            .reason_codes
            .iter()
            .any(|code| code == "self_repair_retry_queued"));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn functional_runtime_heartbeat_scheduler_rebuilds_tool_build_and_cleans_orphans() {
        let temp = tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("queue")).expect("create queue dir");
        std::fs::create_dir_all(temp.path().join("tool-builds")).expect("create tool-builds dir");
        std::fs::write(temp.path().join("queue/state.json"), "{}").expect("write queue state");

        std::fs::write(temp.path().join("tool-builds/stale-output.bin"), "artifact")
            .expect("write orphan artifact");
        std::fs::write(
            temp.path().join("tool-builds/build-1.json"),
            r#"{
  "id": "build-1",
  "status": "building",
  "retryable": true,
  "retry_count": 0,
  "max_retries": 1,
  "started_unix_ms": 1,
  "updated_unix_ms": 1,
  "artifact_paths": ["stale-output.bin"]
}"#,
        )
        .expect("write tool build manifest");

        let mut config = scheduler_config(temp.path(), true);
        config.self_repair_timeout = Duration::ZERO;
        config.self_repair_orphan_artifact_max_age = Duration::ZERO;

        let mut handle =
            start_runtime_heartbeat_scheduler(config).expect("start runtime heartbeat");
        wait_for_tick(handle.state_path(), Duration::from_secs(2)).await;
        let snapshot = inspect_runtime_heartbeat(handle.state_path());
        handle.shutdown().await;

        assert!(!temp.path().join("tool-builds/stale-output.bin").exists());
        let repaired_raw = std::fs::read_to_string(temp.path().join("tool-builds/build-1.json"))
            .expect("read repaired tool build");
        let repaired: Value = serde_json::from_str(&repaired_raw).expect("parse repaired build");
        assert_eq!(repaired["status"], "rebuild_queued");
        assert_eq!(repaired["retry_count"], 1);

        assert!(snapshot.stuck_tool_builds >= 1);
        assert!(snapshot.orphan_artifacts_cleaned >= 1);
        assert!(snapshot
            .reason_codes
            .iter()
            .any(|code| code == "orphan_resources_cleaned"));
    }

    #[tokio::test]
    async fn regression_runtime_heartbeat_scheduler_marks_failed_when_retry_budget_exhausted() {
        let temp = tempdir().expect("tempdir");
        std::fs::create_dir_all(temp.path().join("queue")).expect("create queue dir");
        std::fs::create_dir_all(temp.path().join("jobs")).expect("create jobs dir");
        std::fs::write(temp.path().join("queue/state.json"), "{}").expect("write queue state");
        std::fs::write(
            temp.path().join("jobs/job-exhausted.json"),
            r#"{
  "id": "job-exhausted",
  "status": "running",
  "retryable": true,
  "retry_count": 1,
  "max_retries": 1,
  "started_unix_ms": 1,
  "updated_unix_ms": 1
}"#,
        )
        .expect("write exhausted job manifest");

        let mut config = scheduler_config(temp.path(), true);
        config.self_repair_timeout = Duration::ZERO;

        let mut handle =
            start_runtime_heartbeat_scheduler(config).expect("start runtime heartbeat");
        wait_for_tick(handle.state_path(), Duration::from_secs(2)).await;

        let repaired_raw = std::fs::read_to_string(temp.path().join("jobs/job-exhausted.json"))
            .expect("read repaired exhausted job");
        let repaired: Value = serde_json::from_str(&repaired_raw).expect("parse repaired job");
        assert_eq!(repaired["status"], "failed");
        assert_eq!(repaired["reason_code"], "self_repair_retry_exhausted");

        let snapshot = inspect_runtime_heartbeat(handle.state_path());
        assert!(snapshot.retries_exhausted >= 1);
        assert!(snapshot
            .reason_codes
            .iter()
            .any(|code| code == "self_repair_retry_exhausted"));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn regression_runtime_heartbeat_scheduler_disabled_mode_persists_disabled_snapshot() {
        let temp = tempdir().expect("tempdir");
        let config = scheduler_config(temp.path(), false);
        let mut handle =
            start_runtime_heartbeat_scheduler(config.clone()).expect("start runtime heartbeat");
        let snapshot = inspect_runtime_heartbeat(handle.state_path());
        assert_eq!(snapshot.run_state, "disabled");
        assert_eq!(snapshot.reason_code, "heartbeat_disabled");
        assert_eq!(snapshot.tick_count, 0);
        assert!(!handle.enabled());
        assert!(!handle.is_running());
        handle.shutdown().await;
    }
}
