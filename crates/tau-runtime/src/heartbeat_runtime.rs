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
const RUNTIME_HEARTBEAT_REASON_STOPPED: &str = "heartbeat_stopped";
const RUNTIME_HEARTBEAT_REASON_DISABLED: &str = "heartbeat_disabled";
const RUNTIME_HEARTBEAT_REASON_STATE_MISSING: &str = "heartbeat_state_missing";
const DEFAULT_MAINTENANCE_TEMP_MAX_AGE_SECONDS: u64 = 3_600;

fn runtime_heartbeat_schema_version() -> u32 {
    RUNTIME_HEARTBEAT_SCHEMA_VERSION
}

fn default_runtime_heartbeat_reason_code() -> String {
    RUNTIME_HEARTBEAT_REASON_STATE_MISSING.to_string()
}

fn default_runtime_heartbeat_state() -> String {
    RUNTIME_HEARTBEAT_STATE_UNKNOWN.to_string()
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
    if reason_codes.is_empty() {
        reason_codes.push("heartbeat_cycle_clean".to_string());
    }

    let reason_code = if queue_depth > 0 || pending_events > 0 || pending_jobs > 0 {
        RUNTIME_HEARTBEAT_REASON_BACKLOG.to_string()
    } else {
        RUNTIME_HEARTBEAT_REASON_CYCLE_OK.to_string()
    };
    let now = current_unix_timestamp_ms();
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
        diagnostics,
    };
    RuntimeHeartbeatCycleResult { snapshot, report }
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
