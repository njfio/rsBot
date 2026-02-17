use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{process::Command, time::Duration};

use tau_ai::Message;
use tau_core::{
    append_line_with_rotation, current_unix_timestamp_ms, write_text_atomic, LogRotationPolicy,
};
use tau_session::SessionStore;

use crate::channel_store::{ChannelLogEntry, ChannelStore};

const BACKGROUND_JOB_SCHEMA_VERSION: u32 = 1;
const BACKGROUND_JOB_STATE_SCHEMA_VERSION: u32 = 1;
const BACKGROUND_JOB_EVENT_LOG_FILE: &str = "events.jsonl";
const BACKGROUND_JOB_STATE_FILE: &str = "state.json";
const BACKGROUND_JOB_MANIFEST_DIR: &str = "jobs";
const BACKGROUND_JOB_REASON_QUEUED: &str = "job_queued";
const BACKGROUND_JOB_REASON_STARTED: &str = "job_started";
const BACKGROUND_JOB_REASON_SUCCEEDED: &str = "job_succeeded";
const BACKGROUND_JOB_REASON_NON_ZERO_EXIT: &str = "job_non_zero_exit";
const BACKGROUND_JOB_REASON_SPAWN_FAILED: &str = "job_spawn_failed";
const BACKGROUND_JOB_REASON_TIMEOUT: &str = "job_timeout";
const BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START: &str = "job_cancelled_before_start";
const BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN: &str = "job_cancelled_during_run";
const BACKGROUND_JOB_REASON_RECOVERED_RUNNING: &str = "job_recovered_after_restart";
const BACKGROUND_JOB_REASON_RUNTIME_ERROR: &str = "job_runtime_error";
const BACKGROUND_JOB_REASON_TRACE_WRITE_FAILED: &str = "job_trace_write_failed";
const BACKGROUND_JOB_RECENT_REASON_CODE_CAP: usize = 16;
const BACKGROUND_JOB_RECENT_DIAGNOSTICS_CAP: usize = 24;
const BACKGROUND_JOB_WORKER_POLL_MS: u64 = 100;
const BACKGROUND_JOB_ID_PREFIX: &str = "job";

static BACKGROUND_JOB_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn background_job_schema_version() -> u32 {
    BACKGROUND_JOB_SCHEMA_VERSION
}

fn background_job_state_schema_version() -> u32 {
    BACKGROUND_JOB_STATE_SCHEMA_VERSION
}

/// Enumerates the lifecycle states for background job manifests.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundJobStatus {
    /// Job is persisted and waiting for worker execution.
    Queued,
    /// Job command has started and is currently active.
    Running,
    /// Job finished successfully with zero exit status.
    Succeeded,
    /// Job finished with a runtime/spawn/exit failure.
    Failed,
    /// Job was cancelled before completion.
    Cancelled,
}

impl BackgroundJobStatus {
    /// Returns the stable snake_case wire representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Returns true when the job cannot transition any further.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Enumerates list filters used by the background jobs query APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundJobStatusFilter {
    /// Matches queued jobs only.
    Queued,
    /// Matches running jobs only.
    Running,
    /// Matches succeeded jobs only.
    Succeeded,
    /// Matches failed jobs only.
    Failed,
    /// Matches cancelled jobs only.
    Cancelled,
    /// Matches any terminal job (`succeeded`, `failed`, `cancelled`).
    Terminal,
}

impl BackgroundJobStatusFilter {
    /// Parses a filter token used by runtime/tool list APIs.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" | "canceled" => Some(Self::Cancelled),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    /// Evaluates whether a status satisfies this filter.
    pub fn matches(self, status: BackgroundJobStatus) -> bool {
        match self {
            Self::Queued => status == BackgroundJobStatus::Queued,
            Self::Running => status == BackgroundJobStatus::Running,
            Self::Succeeded => status == BackgroundJobStatus::Succeeded,
            Self::Failed => status == BackgroundJobStatus::Failed,
            Self::Cancelled => status == BackgroundJobStatus::Cancelled,
            Self::Terminal => status.is_terminal(),
        }
    }
}

/// Captures optional trace sinks used when a background job changes state.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BackgroundJobTraceContext {
    #[serde(default)]
    pub channel_store_root: Option<PathBuf>,
    #[serde(default)]
    pub channel_transport: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub session_path: Option<PathBuf>,
}

/// Durable manifest persisted for each background job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobRecord {
    #[serde(default = "background_job_schema_version")]
    pub schema_version: u32,
    pub job_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    pub requested_timeout_ms: u64,
    pub effective_timeout_ms: u64,
    pub status: BackgroundJobStatus,
    pub reason_code: String,
    pub created_unix_ms: u64,
    pub updated_unix_ms: u64,
    #[serde(default)]
    pub started_unix_ms: Option<u64>,
    #[serde(default)]
    pub finished_unix_ms: Option<u64>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub cancellation_requested: bool,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    #[serde(default)]
    pub trace: BackgroundJobTraceContext,
}

/// Runtime counters and diagnostics persisted for operator inspection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobHealthSnapshot {
    #[serde(default = "background_job_state_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub updated_unix_ms: u64,
    #[serde(default)]
    pub queue_depth: usize,
    #[serde(default)]
    pub running_jobs: usize,
    #[serde(default)]
    pub created_total: u64,
    #[serde(default)]
    pub started_total: u64,
    #[serde(default)]
    pub succeeded_total: u64,
    #[serde(default)]
    pub failed_total: u64,
    #[serde(default)]
    pub cancelled_total: u64,
    #[serde(default)]
    pub last_job_id: String,
    #[serde(default)]
    pub last_reason_code: String,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

impl Default for BackgroundJobHealthSnapshot {
    fn default() -> Self {
        Self {
            schema_version: BACKGROUND_JOB_STATE_SCHEMA_VERSION,
            updated_unix_ms: current_unix_timestamp_ms(),
            queue_depth: 0,
            running_jobs: 0,
            created_total: 0,
            started_total: 0,
            succeeded_total: 0,
            failed_total: 0,
            cancelled_total: 0,
            last_job_id: String::new(),
            last_reason_code: String::new(),
            reason_codes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// Input payload used to enqueue a new background job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackgroundJobCreateRequest {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub timeout_ms: Option<u64>,
    pub trace: BackgroundJobTraceContext,
}

/// Runtime configuration for the persisted background jobs engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobRuntimeConfig {
    pub state_dir: PathBuf,
    pub default_timeout_ms: u64,
    pub max_timeout_ms: u64,
    pub worker_poll_ms: u64,
}

impl Default for BackgroundJobRuntimeConfig {
    fn default() -> Self {
        Self {
            state_dir: PathBuf::from(".tau/jobs"),
            default_timeout_ms: 30_000,
            max_timeout_ms: 900_000,
            worker_poll_ms: BACKGROUND_JOB_WORKER_POLL_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct BackgroundJobEventRecord {
    timestamp_unix_ms: u64,
    job_id: String,
    event: String,
    status: String,
    reason_code: String,
    detail: String,
}

#[derive(Debug)]
struct BackgroundJobRuntimeInner {
    config: BackgroundJobRuntimeConfig,
    queue: Mutex<VecDeque<String>>,
    cancellation_requests: Mutex<BTreeSet<String>>,
    health: Mutex<BackgroundJobHealthSnapshot>,
    worker_running: AtomicBool,
}

/// Persistent queue/runtime abstraction for background job execution.
#[derive(Debug, Clone)]
pub struct BackgroundJobRuntime {
    inner: Arc<BackgroundJobRuntimeInner>,
}

impl BackgroundJobRuntime {
    /// Creates a runtime bound to one persisted state directory.
    pub fn new(config: BackgroundJobRuntimeConfig) -> Result<Self> {
        ensure_background_job_layout(&config.state_dir)?;
        let state_path = background_job_state_path(&config.state_dir);
        let health = load_background_job_health_snapshot(&state_path)?;
        let runtime = Self {
            inner: Arc::new(BackgroundJobRuntimeInner {
                config,
                queue: Mutex::new(VecDeque::new()),
                cancellation_requests: Mutex::new(BTreeSet::new()),
                health: Mutex::new(health),
                worker_running: AtomicBool::new(false),
            }),
        };
        runtime.recover_queue_from_disk()?;
        Ok(runtime)
    }

    /// Returns the configured runtime state directory.
    pub fn state_dir(&self) -> &Path {
        self.inner.config.state_dir.as_path()
    }

    /// Returns the persisted health snapshot path.
    pub fn state_path(&self) -> PathBuf {
        background_job_state_path(&self.inner.config.state_dir)
    }

    /// Returns the append-only event log path.
    pub fn events_path(&self) -> PathBuf {
        background_job_events_path(&self.inner.config.state_dir)
    }

    /// Returns the directory containing per-job manifests and outputs.
    pub fn jobs_dir(&self) -> PathBuf {
        background_job_manifests_dir(&self.inner.config.state_dir)
    }

    /// Persists and queues a new background job for asynchronous execution.
    pub async fn create_job(
        &self,
        request: BackgroundJobCreateRequest,
    ) -> Result<BackgroundJobRecord> {
        let command = request.command.trim();
        if command.is_empty() {
            return Err(anyhow!("background job command must be non-empty"));
        }
        let now = current_unix_timestamp_ms();
        let default_timeout_ms = self.inner.config.default_timeout_ms.max(1);
        let max_timeout_ms = self.inner.config.max_timeout_ms.max(default_timeout_ms);
        let requested_timeout_ms = request.timeout_ms.unwrap_or(default_timeout_ms);
        let effective_timeout_ms = requested_timeout_ms.clamp(1, max_timeout_ms);
        let job_id = next_background_job_id();
        let job_dir = self.jobs_dir();
        let stdout_path = job_dir.join(format!("{job_id}.stdout.log"));
        let stderr_path = job_dir.join(format!("{job_id}.stderr.log"));
        let record = BackgroundJobRecord {
            schema_version: BACKGROUND_JOB_SCHEMA_VERSION,
            job_id: job_id.clone(),
            command: command.to_string(),
            args: request.args,
            env: request.env,
            cwd: request.cwd,
            requested_timeout_ms,
            effective_timeout_ms,
            status: BackgroundJobStatus::Queued,
            reason_code: BACKGROUND_JOB_REASON_QUEUED.to_string(),
            created_unix_ms: now,
            updated_unix_ms: now,
            started_unix_ms: None,
            finished_unix_ms: None,
            exit_code: None,
            error: None,
            cancellation_requested: false,
            stdout_path,
            stderr_path,
            trace: request.trace,
        };

        persist_background_job_record(self.state_dir(), &record)?;
        let queue_depth = {
            let mut queue = lock_unpoisoned(&self.inner.queue);
            queue.push_back(job_id);
            queue.len()
        };
        self.update_health_mutation(
            Some(record.job_id.as_str()),
            BACKGROUND_JOB_REASON_QUEUED,
            Some(format!(
                "background_job_created: id={} queue_depth={queue_depth}",
                record.job_id
            )),
            |snapshot| {
                snapshot.created_total = snapshot.created_total.saturating_add(1);
                snapshot.queue_depth = queue_depth;
            },
        )
        .await?;

        self.append_event(
            &record,
            "created",
            BACKGROUND_JOB_REASON_QUEUED,
            "background job queued",
        )?;
        let _ = self.emit_traces(
            &record,
            "created",
            BACKGROUND_JOB_REASON_QUEUED,
            "background job queued",
        );

        self.schedule_worker();
        Ok(record)
    }

    /// Lists persisted jobs with optional status filter and result cap.
    pub async fn list_jobs(
        &self,
        limit: usize,
        status_filter: Option<BackgroundJobStatusFilter>,
    ) -> Result<Vec<BackgroundJobRecord>> {
        let limit = limit.max(1);
        let mut jobs = load_background_job_records(self.state_dir())?;
        jobs.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.job_id.cmp(&right.job_id))
        });
        if let Some(filter) = status_filter {
            jobs.retain(|job| filter.matches(job.status));
        }
        jobs.truncate(limit);
        Ok(jobs)
    }

    /// Loads a single persisted job record by id.
    pub async fn get_job(&self, job_id: &str) -> Result<Option<BackgroundJobRecord>> {
        load_background_job_record(self.state_dir(), job_id)
    }

    /// Returns the latest runtime health counters snapshot.
    pub async fn inspect_health(&self) -> BackgroundJobHealthSnapshot {
        lock_unpoisoned(&self.inner.health).clone()
    }

    /// Requests cancellation for a queued or running job.
    pub async fn cancel_job(&self, job_id: &str) -> Result<Option<BackgroundJobRecord>> {
        let mut record = match load_background_job_record(self.state_dir(), job_id)? {
            Some(record) => record,
            None => return Ok(None),
        };
        if record.status.is_terminal() {
            return Ok(Some(record));
        }

        let now = current_unix_timestamp_ms();
        record.cancellation_requested = true;
        record.updated_unix_ms = now;

        if record.status == BackgroundJobStatus::Queued {
            record.status = BackgroundJobStatus::Cancelled;
            record.reason_code = BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START.to_string();
            record.finished_unix_ms = Some(now);
            record.error = None;
            record.exit_code = None;
            persist_background_job_record(self.state_dir(), &record)?;

            let queue_depth = {
                let mut queue = lock_unpoisoned(&self.inner.queue);
                queue.retain(|entry| entry != job_id);
                queue.len()
            };
            self.update_health_mutation(
                Some(record.job_id.as_str()),
                BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START,
                Some(format!(
                    "background_job_cancelled_before_start: id={} queue_depth={queue_depth}",
                    record.job_id
                )),
                |snapshot| {
                    snapshot.cancelled_total = snapshot.cancelled_total.saturating_add(1);
                    snapshot.queue_depth = queue_depth;
                },
            )
            .await?;
            self.append_event(
                &record,
                "cancelled",
                BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START,
                "background job cancelled before start",
            )?;
            let _ = self.emit_traces(
                &record,
                "cancelled",
                BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START,
                "background job cancelled before start",
            );
            return Ok(Some(record));
        }

        {
            let mut cancellation_requests = lock_unpoisoned(&self.inner.cancellation_requests);
            cancellation_requests.insert(job_id.to_string());
        }
        persist_background_job_record(self.state_dir(), &record)?;
        self.append_event(
            &record,
            "cancel_requested",
            BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN,
            "background job cancellation requested",
        )?;
        self.update_health_mutation(
            Some(record.job_id.as_str()),
            BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN,
            Some(format!(
                "background_job_cancel_requested: id={} status={}",
                record.job_id,
                record.status.as_str()
            )),
            |_snapshot| {},
        )
        .await?;
        Ok(Some(record))
    }

    fn recover_queue_from_disk(&self) -> Result<()> {
        let now = current_unix_timestamp_ms();
        let mut queue_seed = VecDeque::new();
        let mut diagnostics = Vec::new();

        for mut record in load_background_job_records(self.state_dir())? {
            if record.status == BackgroundJobStatus::Queued {
                queue_seed.push_back(record.job_id.clone());
                continue;
            }

            if record.status == BackgroundJobStatus::Running {
                record.status = BackgroundJobStatus::Queued;
                record.reason_code = BACKGROUND_JOB_REASON_RECOVERED_RUNNING.to_string();
                record.updated_unix_ms = now;
                record.started_unix_ms = None;
                record.finished_unix_ms = None;
                record.exit_code = None;
                record.error = None;
                persist_background_job_record(self.state_dir(), &record)?;
                self.append_event(
                    &record,
                    "recovered",
                    BACKGROUND_JOB_REASON_RECOVERED_RUNNING,
                    "requeued running job during runtime recovery",
                )?;
                queue_seed.push_back(record.job_id.clone());
                diagnostics.push(format!(
                    "background_job_recovered_running_job: id={} path={}",
                    record.job_id,
                    background_job_manifest_path(self.state_dir(), record.job_id.as_str())
                        .display()
                ));
            }
        }

        if !queue_seed.is_empty() {
            let queue_depth = queue_seed.len();
            {
                let mut queue = lock_unpoisoned(&self.inner.queue);
                *queue = queue_seed;
            }
            {
                let mut health = lock_unpoisoned(&self.inner.health);
                for line in diagnostics {
                    push_recent_line(
                        &mut health.diagnostics,
                        line,
                        BACKGROUND_JOB_RECENT_DIAGNOSTICS_CAP,
                    );
                }
                apply_health_base(
                    &mut health,
                    None,
                    BACKGROUND_JOB_REASON_RECOVERED_RUNNING,
                    Some(format!(
                        "background_job_runtime_recovered_queue: queue_depth={queue_depth}"
                    )),
                );
                health.queue_depth = queue_depth;
                health.running_jobs = 0;
                persist_background_job_health_snapshot(self.state_dir(), &health)?;
            }
            self.schedule_worker();
        } else {
            let mut health = lock_unpoisoned(&self.inner.health);
            health.queue_depth = 0;
            health.running_jobs = 0;
            persist_background_job_health_snapshot(self.state_dir(), &health)?;
        }

        Ok(())
    }

    fn schedule_worker(&self) {
        if self
            .inner
            .worker_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        let runtime = self.clone();
        spawn_background_future(async move {
            runtime.worker_loop().await;
        });
    }

    async fn worker_loop(self) {
        loop {
            let next_job_id = {
                let mut queue = lock_unpoisoned(&self.inner.queue);
                queue.pop_front()
            };
            let Some(job_id) = next_job_id else {
                break;
            };
            if let Err(error) = self.execute_job(job_id.clone()).await {
                let _ = self
                    .update_health_mutation(
                        Some(job_id.as_str()),
                        BACKGROUND_JOB_REASON_RUNTIME_ERROR,
                        Some(format!(
                            "background_job_runtime_error: id={job_id} error={error}"
                        )),
                        |snapshot| {
                            snapshot.failed_total = snapshot.failed_total.saturating_add(1);
                        },
                    )
                    .await;
            }
        }

        self.inner.worker_running.store(false, Ordering::SeqCst);
        let has_remaining = { !lock_unpoisoned(&self.inner.queue).is_empty() };
        if has_remaining {
            self.schedule_worker();
        }
    }

    async fn execute_job(&self, job_id: String) -> Result<()> {
        let mut record = match load_background_job_record(self.state_dir(), job_id.as_str())? {
            Some(record) => record,
            None => return Ok(()),
        };
        if record.status != BackgroundJobStatus::Queued {
            return Ok(());
        }
        if record.cancellation_requested {
            record.status = BackgroundJobStatus::Cancelled;
            record.reason_code = BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START.to_string();
            record.finished_unix_ms = Some(current_unix_timestamp_ms());
            record.updated_unix_ms = record.finished_unix_ms.unwrap_or(record.updated_unix_ms);
            persist_background_job_record(self.state_dir(), &record)?;
            self.update_health_mutation(
                Some(record.job_id.as_str()),
                BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START,
                Some(format!(
                    "background_job_cancelled_before_start: id={}",
                    record.job_id
                )),
                |snapshot| {
                    snapshot.cancelled_total = snapshot.cancelled_total.saturating_add(1);
                },
            )
            .await?;
            self.append_event(
                &record,
                "cancelled",
                BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START,
                "background job cancelled before worker start",
            )?;
            let _ = self.emit_traces(
                &record,
                "cancelled",
                BACKGROUND_JOB_REASON_CANCELLED_BEFORE_START,
                "background job cancelled before worker start",
            );
            return Ok(());
        }

        let started = current_unix_timestamp_ms();
        record.status = BackgroundJobStatus::Running;
        record.reason_code = BACKGROUND_JOB_REASON_STARTED.to_string();
        record.updated_unix_ms = started;
        record.started_unix_ms = Some(started);
        record.finished_unix_ms = None;
        record.exit_code = None;
        record.error = None;
        persist_background_job_record(self.state_dir(), &record)?;

        let queue_depth = lock_unpoisoned(&self.inner.queue).len();
        self.update_health_mutation(
            Some(record.job_id.as_str()),
            BACKGROUND_JOB_REASON_STARTED,
            Some(format!(
                "background_job_started: id={} queue_depth={queue_depth}",
                record.job_id
            )),
            |snapshot| {
                snapshot.started_total = snapshot.started_total.saturating_add(1);
                snapshot.running_jobs = 1;
                snapshot.queue_depth = queue_depth;
            },
        )
        .await?;
        self.append_event(
            &record,
            "started",
            BACKGROUND_JOB_REASON_STARTED,
            "background job started execution",
        )?;
        let _ = self.emit_traces(
            &record,
            "started",
            BACKGROUND_JOB_REASON_STARTED,
            "background job started execution",
        );

        let stdout_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&record.stdout_path)
            .with_context(|| format!("failed to open {}", record.stdout_path.display()))?;
        let stderr_file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&record.stderr_path)
            .with_context(|| format!("failed to open {}", record.stderr_path.display()))?;

        let mut command = Command::new(record.command.as_str());
        command.args(&record.args);
        if let Some(cwd) = record.cwd.as_ref() {
            command.current_dir(cwd);
        }
        command.kill_on_drop(true);
        command.stdout(Stdio::from(stdout_file));
        command.stderr(Stdio::from(stderr_file));
        for (key, value) in &record.env {
            command.env(key, value);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let finished = current_unix_timestamp_ms();
                record.status = BackgroundJobStatus::Failed;
                record.reason_code = BACKGROUND_JOB_REASON_SPAWN_FAILED.to_string();
                record.updated_unix_ms = finished;
                record.finished_unix_ms = Some(finished);
                record.exit_code = None;
                record.error = Some(error.to_string());
                persist_background_job_record(self.state_dir(), &record)?;
                self.update_health_mutation(
                    Some(record.job_id.as_str()),
                    BACKGROUND_JOB_REASON_SPAWN_FAILED,
                    Some(format!(
                        "background_job_spawn_failed: id={} error={error}",
                        record.job_id
                    )),
                    |snapshot| {
                        snapshot.failed_total = snapshot.failed_total.saturating_add(1);
                        snapshot.running_jobs = 0;
                    },
                )
                .await?;
                self.append_event(
                    &record,
                    "failed",
                    BACKGROUND_JOB_REASON_SPAWN_FAILED,
                    "background job failed to spawn",
                )?;
                let _ = self.emit_traces(
                    &record,
                    "failed",
                    BACKGROUND_JOB_REASON_SPAWN_FAILED,
                    "background job failed to spawn",
                );
                return Ok(());
            }
        };

        let timeout = Duration::from_millis(record.effective_timeout_ms.max(1));
        let started_at = Instant::now();
        let poll_interval = Duration::from_millis(self.inner.config.worker_poll_ms.max(10));

        loop {
            if started_at.elapsed() >= timeout {
                let _ = child.kill().await;
                let finished = current_unix_timestamp_ms();
                record.status = BackgroundJobStatus::Failed;
                record.reason_code = BACKGROUND_JOB_REASON_TIMEOUT.to_string();
                record.updated_unix_ms = finished;
                record.finished_unix_ms = Some(finished);
                record.exit_code = None;
                record.error = Some(format!(
                    "job exceeded timeout of {}ms",
                    record.effective_timeout_ms
                ));
                persist_background_job_record(self.state_dir(), &record)?;
                self.update_health_mutation(
                    Some(record.job_id.as_str()),
                    BACKGROUND_JOB_REASON_TIMEOUT,
                    Some(format!(
                        "background_job_timeout: id={} timeout_ms={}",
                        record.job_id, record.effective_timeout_ms
                    )),
                    |snapshot| {
                        snapshot.failed_total = snapshot.failed_total.saturating_add(1);
                        snapshot.running_jobs = 0;
                    },
                )
                .await?;
                self.append_event(
                    &record,
                    "failed",
                    BACKGROUND_JOB_REASON_TIMEOUT,
                    "background job timed out",
                )?;
                let _ = self.emit_traces(
                    &record,
                    "failed",
                    BACKGROUND_JOB_REASON_TIMEOUT,
                    "background job timed out",
                );
                return Ok(());
            }

            let cancel_requested = {
                let mut cancellation_requests = lock_unpoisoned(&self.inner.cancellation_requests);
                cancellation_requests.remove(record.job_id.as_str())
            };
            if cancel_requested || record.cancellation_requested {
                let _ = child.kill().await;
                let finished = current_unix_timestamp_ms();
                record.status = BackgroundJobStatus::Cancelled;
                record.reason_code = BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN.to_string();
                record.updated_unix_ms = finished;
                record.finished_unix_ms = Some(finished);
                record.exit_code = None;
                record.error = None;
                persist_background_job_record(self.state_dir(), &record)?;
                self.update_health_mutation(
                    Some(record.job_id.as_str()),
                    BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN,
                    Some(format!(
                        "background_job_cancelled_during_run: id={}",
                        record.job_id
                    )),
                    |snapshot| {
                        snapshot.cancelled_total = snapshot.cancelled_total.saturating_add(1);
                        snapshot.running_jobs = 0;
                    },
                )
                .await?;
                self.append_event(
                    &record,
                    "cancelled",
                    BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN,
                    "background job cancelled during execution",
                )?;
                let _ = self.emit_traces(
                    &record,
                    "cancelled",
                    BACKGROUND_JOB_REASON_CANCELLED_DURING_RUN,
                    "background job cancelled during execution",
                );
                return Ok(());
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    let finished = current_unix_timestamp_ms();
                    record.updated_unix_ms = finished;
                    record.finished_unix_ms = Some(finished);
                    record.exit_code = status.code();
                    if status.success() {
                        record.status = BackgroundJobStatus::Succeeded;
                        record.reason_code = BACKGROUND_JOB_REASON_SUCCEEDED.to_string();
                        record.error = None;
                        persist_background_job_record(self.state_dir(), &record)?;
                        self.update_health_mutation(
                            Some(record.job_id.as_str()),
                            BACKGROUND_JOB_REASON_SUCCEEDED,
                            Some(format!("background_job_succeeded: id={}", record.job_id)),
                            |snapshot| {
                                snapshot.succeeded_total =
                                    snapshot.succeeded_total.saturating_add(1);
                                snapshot.running_jobs = 0;
                            },
                        )
                        .await?;
                        self.append_event(
                            &record,
                            "succeeded",
                            BACKGROUND_JOB_REASON_SUCCEEDED,
                            "background job succeeded",
                        )?;
                        let _ = self.emit_traces(
                            &record,
                            "succeeded",
                            BACKGROUND_JOB_REASON_SUCCEEDED,
                            "background job succeeded",
                        );
                        return Ok(());
                    }

                    record.status = BackgroundJobStatus::Failed;
                    record.reason_code = BACKGROUND_JOB_REASON_NON_ZERO_EXIT.to_string();
                    record.error = Some(format!(
                        "background job exited with status {}",
                        record.exit_code.unwrap_or(-1)
                    ));
                    persist_background_job_record(self.state_dir(), &record)?;
                    self.update_health_mutation(
                        Some(record.job_id.as_str()),
                        BACKGROUND_JOB_REASON_NON_ZERO_EXIT,
                        Some(format!(
                            "background_job_non_zero_exit: id={} exit_code={}",
                            record.job_id,
                            record.exit_code.unwrap_or(-1)
                        )),
                        |snapshot| {
                            snapshot.failed_total = snapshot.failed_total.saturating_add(1);
                            snapshot.running_jobs = 0;
                        },
                    )
                    .await?;
                    self.append_event(
                        &record,
                        "failed",
                        BACKGROUND_JOB_REASON_NON_ZERO_EXIT,
                        "background job exited non-zero",
                    )?;
                    let _ = self.emit_traces(
                        &record,
                        "failed",
                        BACKGROUND_JOB_REASON_NON_ZERO_EXIT,
                        "background job exited non-zero",
                    );
                    return Ok(());
                }
                Ok(None) => {
                    tokio::time::sleep(poll_interval).await;
                }
                Err(error) => {
                    let finished = current_unix_timestamp_ms();
                    record.status = BackgroundJobStatus::Failed;
                    record.reason_code = BACKGROUND_JOB_REASON_RUNTIME_ERROR.to_string();
                    record.updated_unix_ms = finished;
                    record.finished_unix_ms = Some(finished);
                    record.exit_code = None;
                    record.error = Some(error.to_string());
                    persist_background_job_record(self.state_dir(), &record)?;
                    self.update_health_mutation(
                        Some(record.job_id.as_str()),
                        BACKGROUND_JOB_REASON_RUNTIME_ERROR,
                        Some(format!(
                            "background_job_runtime_error: id={} error={error}",
                            record.job_id
                        )),
                        |snapshot| {
                            snapshot.failed_total = snapshot.failed_total.saturating_add(1);
                            snapshot.running_jobs = 0;
                        },
                    )
                    .await?;
                    self.append_event(
                        &record,
                        "failed",
                        BACKGROUND_JOB_REASON_RUNTIME_ERROR,
                        "background job runtime poll failed",
                    )?;
                    let _ = self.emit_traces(
                        &record,
                        "failed",
                        BACKGROUND_JOB_REASON_RUNTIME_ERROR,
                        "background job runtime poll failed",
                    );
                    return Ok(());
                }
            }
        }
    }

    async fn update_health_mutation<F>(
        &self,
        job_id: Option<&str>,
        reason_code: &str,
        diagnostic: Option<String>,
        mutate: F,
    ) -> Result<()>
    where
        F: FnOnce(&mut BackgroundJobHealthSnapshot),
    {
        let mut health = lock_unpoisoned(&self.inner.health);
        apply_health_base(&mut health, job_id, reason_code, diagnostic);
        mutate(&mut health);
        persist_background_job_health_snapshot(self.state_dir(), &health)?;
        Ok(())
    }

    fn append_event(
        &self,
        record: &BackgroundJobRecord,
        event: &str,
        reason_code: &str,
        detail: &str,
    ) -> Result<()> {
        let event_record = BackgroundJobEventRecord {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            job_id: record.job_id.clone(),
            event: event.to_string(),
            status: record.status.as_str().to_string(),
            reason_code: reason_code.to_string(),
            detail: detail.to_string(),
        };
        append_jsonl_record(self.events_path().as_path(), &event_record)
    }

    fn emit_traces(
        &self,
        record: &BackgroundJobRecord,
        event: &str,
        reason_code: &str,
        detail: &str,
    ) -> Result<()> {
        let payload = json!({
            "job_id": record.job_id,
            "event": event,
            "status": record.status.as_str(),
            "reason_code": reason_code,
            "detail": detail,
            "command": record.command,
            "args": record.args,
            "exit_code": record.exit_code,
            "stdout_path": record.stdout_path.display().to_string(),
            "stderr_path": record.stderr_path.display().to_string(),
        });

        let mut trace_errors = Vec::new();
        if let (Some(root), Some(transport), Some(channel_id)) = (
            record.trace.channel_store_root.as_ref(),
            record.trace.channel_transport.as_deref(),
            record.trace.channel_id.as_deref(),
        ) {
            match ChannelStore::open(root, transport, channel_id) {
                Ok(store) => {
                    if let Err(error) = store.append_log_entry(&ChannelLogEntry {
                        timestamp_unix_ms: current_unix_timestamp_ms(),
                        direction: "runtime".to_string(),
                        event_key: Some(format!("background-job:{}", record.job_id)),
                        source: "background_job_runtime".to_string(),
                        payload: payload.clone(),
                    }) {
                        trace_errors.push(format!(
                            "channel_store_trace_failed: root={} transport={} channel={} error={error}",
                            root.display(),
                            transport,
                            channel_id
                        ));
                    }
                }
                Err(error) => trace_errors.push(format!(
                    "channel_store_open_failed: root={} transport={} channel={} error={error}",
                    root.display(),
                    transport,
                    channel_id
                )),
            }
        }

        if let Some(session_path) = record.trace.session_path.as_ref() {
            match SessionStore::load(session_path) {
                Ok(mut store) => {
                    let line = format!(
                        "background job {} {} ({})",
                        record.job_id,
                        record.status.as_str(),
                        reason_code
                    );
                    if let Err(error) =
                        store.append_messages(None, &[Message::assistant_text(line)])
                    {
                        trace_errors.push(format!(
                            "session_trace_append_failed: path={} error={error}",
                            session_path.display()
                        ));
                    }
                }
                Err(error) => trace_errors.push(format!(
                    "session_trace_open_failed: path={} error={error}",
                    session_path.display()
                )),
            }
        }

        if trace_errors.is_empty() {
            return Ok(());
        }

        let detail = trace_errors.join(" | ");
        self.append_event(
            record,
            "trace_error",
            BACKGROUND_JOB_REASON_TRACE_WRITE_FAILED,
            detail.as_str(),
        )?;
        Ok(())
    }
}

fn apply_health_base(
    snapshot: &mut BackgroundJobHealthSnapshot,
    job_id: Option<&str>,
    reason_code: &str,
    diagnostic: Option<String>,
) {
    snapshot.updated_unix_ms = current_unix_timestamp_ms();
    if let Some(job_id) = job_id {
        snapshot.last_job_id = job_id.to_string();
    }
    snapshot.last_reason_code = reason_code.to_string();
    push_recent_reason_code(
        &mut snapshot.reason_codes,
        reason_code,
        BACKGROUND_JOB_RECENT_REASON_CODE_CAP,
    );
    if let Some(line) = diagnostic {
        push_recent_line(
            &mut snapshot.diagnostics,
            line,
            BACKGROUND_JOB_RECENT_DIAGNOSTICS_CAP,
        );
    }
}

fn push_recent_reason_code(reason_codes: &mut Vec<String>, reason_code: &str, cap: usize) {
    if reason_codes.iter().any(|existing| existing == reason_code) {
        reason_codes.retain(|existing| existing != reason_code);
    }
    reason_codes.push(reason_code.to_string());
    while reason_codes.len() > cap {
        reason_codes.remove(0);
    }
}

fn push_recent_line(lines: &mut Vec<String>, line: String, cap: usize) {
    lines.push(line);
    while lines.len() > cap {
        lines.remove(0);
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn spawn_background_future<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
        return;
    }

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        match runtime {
            Ok(runtime) => runtime.block_on(future),
            Err(error) => eprintln!("background job runtime worker bootstrap failed: {error}"),
        }
    });
}

fn ensure_background_job_layout(state_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    std::fs::create_dir_all(background_job_manifests_dir(state_dir)).with_context(|| {
        format!(
            "failed to create {}",
            background_job_manifests_dir(state_dir).display()
        )
    })?;
    let events_path = background_job_events_path(state_dir);
    if !events_path.exists() {
        std::fs::write(&events_path, "")
            .with_context(|| format!("failed to initialize {}", events_path.display()))?;
    }
    Ok(())
}

fn next_background_job_id() -> String {
    let now = current_unix_timestamp_ms();
    let suffix = BACKGROUND_JOB_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{BACKGROUND_JOB_ID_PREFIX}-{now}-{suffix:04}")
}

fn background_job_state_path(state_dir: &Path) -> PathBuf {
    state_dir.join(BACKGROUND_JOB_STATE_FILE)
}

fn background_job_events_path(state_dir: &Path) -> PathBuf {
    state_dir.join(BACKGROUND_JOB_EVENT_LOG_FILE)
}

fn background_job_manifests_dir(state_dir: &Path) -> PathBuf {
    state_dir.join(BACKGROUND_JOB_MANIFEST_DIR)
}

fn background_job_manifest_path(state_dir: &Path, job_id: &str) -> PathBuf {
    background_job_manifests_dir(state_dir).join(format!("{job_id}.json"))
}

fn persist_background_job_record(state_dir: &Path, record: &BackgroundJobRecord) -> Result<()> {
    let path = background_job_manifest_path(state_dir, record.job_id.as_str());
    let mut payload =
        serde_json::to_string_pretty(record).context("failed to encode background job record")?;
    payload.push('\n');
    write_text_atomic(path.as_path(), payload.as_str())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn load_background_job_record(
    state_dir: &Path,
    job_id: &str,
) -> Result<Option<BackgroundJobRecord>> {
    let path = background_job_manifest_path(state_dir, job_id);
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let record = serde_json::from_str::<BackgroundJobRecord>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(record))
}

fn collect_background_job_manifest_paths(state_dir: &Path) -> Result<Vec<PathBuf>> {
    let dir = background_job_manifests_dir(state_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in
        std::fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
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

fn load_background_job_records(state_dir: &Path) -> Result<Vec<BackgroundJobRecord>> {
    let mut records = Vec::new();
    for path in collect_background_job_manifest_paths(state_dir)? {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let record = serde_json::from_str::<BackgroundJobRecord>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        records.push(record);
    }
    Ok(records)
}

fn load_background_job_health_snapshot(path: &Path) -> Result<BackgroundJobHealthSnapshot> {
    if !path.exists() {
        return Ok(BackgroundJobHealthSnapshot::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(BackgroundJobHealthSnapshot::default());
    }
    serde_json::from_str::<BackgroundJobHealthSnapshot>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))
}

fn persist_background_job_health_snapshot(
    state_dir: &Path,
    snapshot: &BackgroundJobHealthSnapshot,
) -> Result<()> {
    let path = background_job_state_path(state_dir);
    let mut payload = serde_json::to_string_pretty(snapshot)
        .context("failed to encode background job health snapshot")?;
    payload.push('\n');
    write_text_atomic(path.as_path(), payload.as_str())
        .with_context(|| format!("failed to write {}", path.display()))
}

fn append_jsonl_record<T>(path: &Path, value: &T) -> Result<()>
where
    T: Serialize,
{
    let line = serde_json::to_string(value).context("failed to encode JSONL record")?;
    append_line_with_rotation(path, &line, LogRotationPolicy::from_env())
        .with_context(|| format!("failed to append {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        background_job_manifest_path, BackgroundJobCreateRequest, BackgroundJobRecord,
        BackgroundJobRuntime, BackgroundJobRuntimeConfig, BackgroundJobStatus,
        BackgroundJobStatusFilter, BackgroundJobTraceContext,
    };
    use std::collections::BTreeMap;
    use std::time::Duration;
    use tempfile::tempdir;

    fn shell_command(script: &str) -> (String, Vec<String>) {
        if cfg!(windows) {
            (
                "cmd".to_string(),
                vec!["/C".to_string(), script.to_string()],
            )
        } else {
            (
                "sh".to_string(),
                vec!["-lc".to_string(), script.to_string()],
            )
        }
    }

    fn sleep_script() -> &'static str {
        if cfg!(windows) {
            "ping -n 3 127.0.0.1 >NUL"
        } else {
            "sleep 1"
        }
    }

    async fn wait_for_terminal_status(
        runtime: &BackgroundJobRuntime,
        job_id: &str,
        timeout: Duration,
    ) -> BackgroundJobStatus {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let record = runtime
                .get_job(job_id)
                .await
                .expect("get job")
                .expect("job should exist");
            if record.status.is_terminal() {
                return record.status;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "job {job_id} did not reach terminal status"
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }

    #[test]
    fn unit_background_job_status_filter_parsing_supports_terminal_alias() {
        assert_eq!(
            BackgroundJobStatusFilter::parse("terminal"),
            Some(BackgroundJobStatusFilter::Terminal)
        );
        assert!(BackgroundJobStatusFilter::Terminal.matches(BackgroundJobStatus::Succeeded));
        assert!(BackgroundJobStatusFilter::Terminal.matches(BackgroundJobStatus::Failed));
        assert!(!BackgroundJobStatusFilter::Terminal.matches(BackgroundJobStatus::Queued));
    }

    #[tokio::test]
    async fn functional_background_job_runtime_executes_and_persists_outputs() {
        let temp = tempdir().expect("tempdir");
        let runtime = BackgroundJobRuntime::new(BackgroundJobRuntimeConfig {
            state_dir: temp.path().join("jobs"),
            default_timeout_ms: 5_000,
            max_timeout_ms: 10_000,
            worker_poll_ms: 20,
        })
        .expect("runtime");

        let (command, args) = shell_command("echo tau-background-job");
        let record = runtime
            .create_job(BackgroundJobCreateRequest {
                command,
                args,
                ..BackgroundJobCreateRequest::default()
            })
            .await
            .expect("create job");

        let status =
            wait_for_terminal_status(&runtime, &record.job_id, Duration::from_secs(5)).await;
        assert_eq!(status, BackgroundJobStatus::Succeeded);

        let refreshed = runtime
            .get_job(record.job_id.as_str())
            .await
            .expect("get job")
            .expect("job exists");
        let stdout = std::fs::read_to_string(&refreshed.stdout_path).expect("read stdout");
        assert!(stdout.contains("tau-background-job"));
        let health = runtime.inspect_health().await;
        assert!(health.succeeded_total >= 1);
    }

    #[tokio::test]
    async fn integration_background_job_runtime_emits_channel_store_and_session_traces() {
        let temp = tempdir().expect("tempdir");
        let channel_store_root = temp.path().join("channel-store");
        let session_path = temp.path().join("sessions/default.sqlite");
        let runtime = BackgroundJobRuntime::new(BackgroundJobRuntimeConfig {
            state_dir: temp.path().join("jobs"),
            default_timeout_ms: 5_000,
            max_timeout_ms: 10_000,
            worker_poll_ms: 20,
        })
        .expect("runtime");

        let (command, args) = shell_command("echo trace-job");
        let record = runtime
            .create_job(BackgroundJobCreateRequest {
                command,
                args,
                trace: BackgroundJobTraceContext {
                    channel_store_root: Some(channel_store_root.clone()),
                    channel_transport: Some("local".to_string()),
                    channel_id: Some("integration".to_string()),
                    session_path: Some(session_path.clone()),
                },
                ..BackgroundJobCreateRequest::default()
            })
            .await
            .expect("create");
        wait_for_terminal_status(&runtime, &record.job_id, Duration::from_secs(5)).await;

        let store =
            crate::channel_store::ChannelStore::open(&channel_store_root, "local", "integration")
                .expect("channel store");
        let logs = store.load_log_entries().expect("channel logs");
        assert!(
            logs.iter()
                .any(|entry| entry.source == "background_job_runtime"),
            "expected channel logs to include background job events"
        );

        let session = tau_session::SessionStore::load(&session_path).expect("session");
        assert!(
            session
                .entries()
                .iter()
                .any(|entry| entry.message.text_content().contains("background job")),
            "expected session entries to include background job trace"
        );
    }

    #[tokio::test]
    async fn regression_background_job_runtime_cancelled_queued_job_does_not_execute() {
        let temp = tempdir().expect("tempdir");
        let runtime = BackgroundJobRuntime::new(BackgroundJobRuntimeConfig {
            state_dir: temp.path().join("jobs"),
            default_timeout_ms: 15_000,
            max_timeout_ms: 15_000,
            worker_poll_ms: 20,
        })
        .expect("runtime");

        let (command_a, args_a) = shell_command(sleep_script());
        let first = runtime
            .create_job(BackgroundJobCreateRequest {
                command: command_a,
                args: args_a,
                ..BackgroundJobCreateRequest::default()
            })
            .await
            .expect("create first");

        let (command_b, args_b) = shell_command("echo never-runs");
        let second = runtime
            .create_job(BackgroundJobCreateRequest {
                command: command_b,
                args: args_b,
                ..BackgroundJobCreateRequest::default()
            })
            .await
            .expect("create second");

        let cancelled = runtime
            .cancel_job(second.job_id.as_str())
            .await
            .expect("cancel second")
            .expect("second should exist");
        assert_eq!(cancelled.status, BackgroundJobStatus::Cancelled);
        assert_eq!(
            cancelled.reason_code,
            "job_cancelled_before_start".to_string()
        );

        wait_for_terminal_status(&runtime, &first.job_id, Duration::from_secs(5)).await;
        let refreshed_second = runtime
            .get_job(second.job_id.as_str())
            .await
            .expect("get second")
            .expect("second exists");
        assert_eq!(refreshed_second.status, BackgroundJobStatus::Cancelled);

        let manifest_path =
            background_job_manifest_path(runtime.state_dir(), refreshed_second.job_id.as_str());
        assert!(manifest_path.exists());
    }

    #[tokio::test]
    async fn integration_background_job_runtime_recovers_running_manifest_after_restart() {
        let temp = tempdir().expect("tempdir");
        let state_dir = temp.path().join("jobs");
        let config = BackgroundJobRuntimeConfig {
            state_dir: state_dir.clone(),
            default_timeout_ms: 5_000,
            max_timeout_ms: 10_000,
            worker_poll_ms: 20,
        };
        let bootstrap = BackgroundJobRuntime::new(config.clone()).expect("bootstrap runtime");
        drop(bootstrap);

        let job_id = "job-recover-after-crash-1".to_string();
        let (command, args) = shell_command("echo recovered-after-crash");
        let stdout_path = state_dir.join("stdout").join(format!("{job_id}.log"));
        let stderr_path = state_dir.join("stderr").join(format!("{job_id}.log"));
        if let Some(parent) = stdout_path.parent() {
            std::fs::create_dir_all(parent).expect("create stdout parent");
        }
        if let Some(parent) = stderr_path.parent() {
            std::fs::create_dir_all(parent).expect("create stderr parent");
        }

        let record = BackgroundJobRecord {
            schema_version: 1,
            job_id: job_id.clone(),
            command,
            args,
            env: BTreeMap::new(),
            cwd: None,
            requested_timeout_ms: 5_000,
            effective_timeout_ms: 5_000,
            status: BackgroundJobStatus::Running,
            reason_code: "job_started".to_string(),
            created_unix_ms: 1_700_000_000_000,
            updated_unix_ms: 1_700_000_000_100,
            started_unix_ms: Some(1_700_000_000_100),
            finished_unix_ms: None,
            exit_code: None,
            error: None,
            cancellation_requested: false,
            stdout_path,
            stderr_path,
            trace: BackgroundJobTraceContext::default(),
        };
        let manifest_path = background_job_manifest_path(&state_dir, &job_id);
        let payload = serde_json::to_string_pretty(&record).expect("serialize running manifest");
        std::fs::write(&manifest_path, payload).expect("write running manifest");

        let restarted = BackgroundJobRuntime::new(config).expect("restart runtime");
        let status = wait_for_terminal_status(&restarted, &job_id, Duration::from_secs(5)).await;
        assert_eq!(status, BackgroundJobStatus::Succeeded);

        let refreshed = restarted
            .get_job(&job_id)
            .await
            .expect("get recovered job")
            .expect("recovered job should exist");
        assert_eq!(refreshed.status, BackgroundJobStatus::Succeeded);
        assert_eq!(refreshed.reason_code, "job_succeeded".to_string());

        let events_raw = std::fs::read_to_string(restarted.events_path())
            .expect("read events for recovered job");
        assert!(events_raw.contains("\"event\":\"recovered\""));
        assert!(events_raw.contains("\"reason_code\":\"job_recovered_after_restart\""));
    }
}
