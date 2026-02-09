use std::{
    collections::HashMap,
    future::pending,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::TimeZone;
use chrono_tz::Tz;
use cron::Schedule;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::{LlmClient, Message, MessageRole};

use crate::{
    channel_store::{ChannelLogEntry, ChannelStore},
    current_unix_timestamp_ms, run_prompt_with_cancellation, write_text_atomic, Cli,
    CliEventTemplateSchedule, PromptRunStatus, RenderOptions, SessionRuntime,
};
use crate::{session::SessionStore, tools::ToolPolicy};

const EVENT_RUNNER_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone)]
pub(crate) struct EventSchedulerConfig {
    pub client: std::sync::Arc<dyn LlmClient>,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub tool_policy: ToolPolicy,
    pub turn_timeout_ms: u64,
    pub render_options: RenderOptions,
    pub session_lock_wait_ms: u64,
    pub session_lock_stale_ms: u64,
    pub channel_store_root: PathBuf,
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub poll_interval: Duration,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct EventWebhookIngestConfig {
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub channel_ref: String,
    pub payload_file: PathBuf,
    pub prompt_prefix: String,
    pub debounce_key: Option<String>,
    pub debounce_window_seconds: u64,
    pub signature: Option<String>,
    pub timestamp: Option<String>,
    pub secret: Option<String>,
    pub signature_algorithm: Option<WebhookSignatureAlgorithm>,
    pub signature_max_skew_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WebhookSignatureAlgorithm {
    GithubSha256,
    SlackV0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EventSchedule {
    Immediate,
    At { at_unix_ms: u64 },
    Periodic { cron: String, timezone: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventDefinition {
    id: String,
    channel: String,
    prompt: String,
    schedule: EventSchedule,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    created_unix_ms: Option<u64>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone)]
struct EventRecord {
    path: PathBuf,
    definition: EventDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventRunnerState {
    schema_version: u32,
    #[serde(default)]
    periodic_last_run_unix_ms: HashMap<String, u64>,
    #[serde(default)]
    debounce_last_seen_unix_ms: HashMap<String, u64>,
    #[serde(default)]
    signature_replay_last_seen_unix_ms: HashMap<String, u64>,
}

impl Default for EventRunnerState {
    fn default() -> Self {
        Self {
            schema_version: EVENT_RUNNER_STATE_SCHEMA_VERSION,
            periodic_last_run_unix_ms: HashMap::new(),
            debounce_last_seen_unix_ms: HashMap::new(),
            signature_replay_last_seen_unix_ms: HashMap::new(),
        }
    }
}

#[derive(Debug, Default)]
struct EventPollReport {
    discovered: usize,
    queued: usize,
    executed: usize,
    stale_skipped: usize,
    malformed_skipped: usize,
    failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DueDecision {
    Run,
    NotDue,
    SkipStaleRemove,
}

#[derive(Debug, Clone)]
struct EventsInspectConfig {
    events_dir: PathBuf,
    state_path: PathBuf,
    queue_limit: usize,
    stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsInspectReport {
    pub events_dir: String,
    pub state_path: String,
    pub now_unix_ms: u64,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
    pub discovered_events: usize,
    pub malformed_events: usize,
    pub enabled_events: usize,
    pub disabled_events: usize,
    pub schedule_immediate_events: usize,
    pub schedule_at_events: usize,
    pub schedule_periodic_events: usize,
    pub due_now_events: usize,
    pub queued_now_events: usize,
    pub not_due_events: usize,
    pub stale_immediate_events: usize,
    pub due_eval_failed_events: usize,
    pub periodic_with_last_run_state: usize,
    pub periodic_missing_last_run_state: usize,
}

#[derive(Debug, Clone)]
struct EventsValidateConfig {
    events_dir: PathBuf,
    state_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsValidateDiagnostic {
    pub path: String,
    pub event_id: Option<String>,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsValidateReport {
    pub events_dir: String,
    pub state_path: String,
    pub now_unix_ms: u64,
    pub total_files: usize,
    pub valid_files: usize,
    pub invalid_files: usize,
    pub malformed_files: usize,
    pub failed_files: usize,
    pub disabled_files: usize,
    pub diagnostics: Vec<EventsValidateDiagnostic>,
}

#[derive(Debug, Clone)]
struct EventsSimulateConfig {
    events_dir: PathBuf,
    state_path: PathBuf,
    horizon_seconds: u64,
    stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone)]
struct EventsDryRunConfig {
    events_dir: PathBuf,
    state_path: PathBuf,
    queue_limit: usize,
    stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsSimulateRow {
    pub path: String,
    pub event_id: String,
    pub channel: String,
    pub schedule: String,
    pub enabled: bool,
    pub next_due_unix_ms: Option<u64>,
    pub due_now: bool,
    pub within_horizon: bool,
    pub last_run_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsSimulateReport {
    pub events_dir: String,
    pub state_path: String,
    pub now_unix_ms: u64,
    pub horizon_seconds: u64,
    pub total_files: usize,
    pub simulated_rows: usize,
    pub malformed_files: usize,
    pub invalid_rows: usize,
    pub due_now_rows: usize,
    pub within_horizon_rows: usize,
    pub rows: Vec<EventsSimulateRow>,
    pub diagnostics: Vec<EventsValidateDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsDryRunRow {
    pub path: String,
    pub event_id: Option<String>,
    pub channel: Option<String>,
    pub schedule: Option<String>,
    pub enabled: Option<bool>,
    pub decision: String,
    pub reason_code: String,
    pub queue_position: Option<usize>,
    pub last_run_unix_ms: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct EventsDryRunReport {
    pub events_dir: String,
    pub state_path: String,
    pub now_unix_ms: u64,
    pub queue_limit: usize,
    pub total_files: usize,
    pub evaluated_rows: usize,
    pub execute_rows: usize,
    pub skipped_rows: usize,
    pub error_rows: usize,
    pub malformed_files: usize,
    pub rows: Vec<EventsDryRunRow>,
}

pub(crate) fn execute_events_inspect_command(cli: &Cli) -> Result<()> {
    let report = inspect_events(
        &EventsInspectConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_inspect_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events inspect json")?
        );
    } else {
        println!("{}", render_events_inspect_report(&report));
    }
    Ok(())
}

pub(crate) fn execute_events_validate_command(cli: &Cli) -> Result<()> {
    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_validate_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events validate json")?
        );
    } else {
        println!("{}", render_events_validate_report(&report));
    }

    if report.failed_files > 0 {
        bail!(
            "events validate failed: failed_files={} invalid_files={} malformed_files={}",
            report.failed_files,
            report.invalid_files,
            report.malformed_files
        );
    }
    Ok(())
}

pub(crate) fn execute_events_template_write_command(cli: &Cli) -> Result<()> {
    let target_path = cli
        .events_template_write
        .as_ref()
        .ok_or_else(|| anyhow!("--events-template-write is required"))?;
    if target_path.exists() && !cli.events_template_overwrite {
        bail!(
            "template path already exists (use --events-template-overwrite=true): {}",
            target_path.display()
        );
    }

    let now_unix_ms = current_unix_timestamp_ms();
    let channel = cli
        .events_template_channel
        .clone()
        .unwrap_or_else(|| "slack/C123".to_string());
    ChannelStore::parse_channel_ref(&channel)?;

    let schedule = match cli.events_template_schedule {
        CliEventTemplateSchedule::Immediate => EventSchedule::Immediate,
        CliEventTemplateSchedule::At => EventSchedule::At {
            at_unix_ms: cli
                .events_template_at_unix_ms
                .unwrap_or_else(|| now_unix_ms.saturating_add(300_000)),
        },
        CliEventTemplateSchedule::Periodic => {
            let cron = cli
                .events_template_cron
                .clone()
                .unwrap_or_else(|| "0 0/15 * * * * *".to_string());
            let timezone = cli.events_template_timezone.trim().to_string();
            if timezone.is_empty() {
                bail!("--events-template-timezone must be non-empty");
            }
            let _ =
                next_periodic_due_unix_ms(&cron, &timezone, now_unix_ms.saturating_sub(60_000))?;
            EventSchedule::Periodic { cron, timezone }
        }
    };

    let event_id = cli
        .events_template_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| match &schedule {
            EventSchedule::Immediate => "template-immediate".to_string(),
            EventSchedule::At { .. } => "template-at".to_string(),
            EventSchedule::Periodic { .. } => "template-periodic".to_string(),
        });

    let prompt = cli
        .events_template_prompt
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            "Summarize current context and propose the next best action.".to_string()
        });

    let template = EventDefinition {
        id: event_id.clone(),
        channel: channel.clone(),
        prompt,
        schedule,
        enabled: true,
        created_unix_ms: Some(now_unix_ms),
    };

    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut payload =
        serde_json::to_string_pretty(&template).context("failed to serialize event template")?;
    payload.push('\n');
    write_text_atomic(target_path, &payload)
        .with_context(|| format!("failed to write {}", target_path.display()))?;

    println!(
        "events template write: path={} schedule={} event_id={} channel={} overwrite={}",
        target_path.display(),
        schedule_name(&template.schedule),
        template.id,
        template.channel,
        cli.events_template_overwrite,
    );
    Ok(())
}

pub(crate) fn execute_events_simulate_command(cli: &Cli) -> Result<()> {
    let report = simulate_events(
        &EventsSimulateConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            horizon_seconds: cli.events_simulate_horizon_seconds,
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_simulate_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events simulate json")?
        );
    } else {
        println!("{}", render_events_simulate_report(&report));
    }
    Ok(())
}

pub(crate) fn execute_events_dry_run_command(cli: &Cli) -> Result<()> {
    let report = dry_run_events(
        &EventsDryRunConfig {
            events_dir: cli.events_dir.clone(),
            state_path: cli.events_state_path.clone(),
            queue_limit: cli.events_queue_limit.max(1),
            stale_immediate_max_age_seconds: cli.events_stale_immediate_max_age_seconds,
        },
        current_unix_timestamp_ms(),
    )?;

    if cli.events_dry_run_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to render events dry run json")?
        );
    } else {
        println!("{}", render_events_dry_run_report(&report));
    }
    Ok(())
}

pub(crate) async fn run_event_scheduler(config: EventSchedulerConfig) -> Result<()> {
    let mut runtime = EventSchedulerRuntime::new(config)?;
    runtime.run().await
}

pub(crate) fn ingest_webhook_immediate_event(config: &EventWebhookIngestConfig) -> Result<()> {
    std::fs::create_dir_all(&config.events_dir)
        .with_context(|| format!("failed to create {}", config.events_dir.display()))?;

    let now_unix_ms = current_unix_timestamp_ms();
    let payload_raw = std::fs::read_to_string(&config.payload_file)
        .with_context(|| format!("failed to read {}", config.payload_file.display()))?;
    let payload = payload_raw.trim();
    if payload.is_empty() {
        bail!(
            "webhook payload file is empty: {}",
            config.payload_file.display()
        );
    }

    let mut state = load_runner_state(&config.state_path)?;
    if let Some(replay_key) = verify_webhook_signature(
        &payload_raw,
        config.signature.as_deref(),
        config.timestamp.as_deref(),
        config.secret.as_deref(),
        config.signature_algorithm,
        now_unix_ms,
        config.signature_max_skew_seconds,
    )? {
        enforce_signature_replay_guard(
            &mut state,
            &replay_key,
            now_unix_ms,
            config.signature_max_skew_seconds,
        )?;
    }

    if let Some(key) = config.debounce_key.as_deref() {
        let debounce_window_ms = config.debounce_window_seconds.saturating_mul(1000);
        if let Some(last_seen) = state.debounce_last_seen_unix_ms.get(key) {
            if now_unix_ms.saturating_sub(*last_seen) < debounce_window_ms {
                println!(
                    "webhook ingest skipped by debounce: key={} window_seconds={}",
                    key, config.debounce_window_seconds
                );
                return Ok(());
            }
        }
        state
            .debounce_last_seen_unix_ms
            .insert(key.to_string(), now_unix_ms);
    }

    let event_id = format!("webhook-{}-{}", now_unix_ms, short_hash(payload.as_bytes()));
    let event = EventDefinition {
        id: event_id.clone(),
        channel: config.channel_ref.clone(),
        prompt: format!("{}\n\nWebhook payload:\n{}", config.prompt_prefix, payload),
        schedule: EventSchedule::Immediate,
        enabled: true,
        created_unix_ms: Some(now_unix_ms),
    };

    let event_path = config
        .events_dir
        .join(format!("{}.json", sanitize_for_path(&event_id)));
    let mut payload =
        serde_json::to_string_pretty(&event).context("failed to serialize webhook event")?;
    payload.push('\n');
    write_text_atomic(&event_path, &payload)
        .with_context(|| format!("failed to write {}", event_path.display()))?;

    save_runner_state(&config.state_path, &state)?;
    println!(
        "webhook ingest queued immediate event: id={} path={}",
        event_id,
        event_path.display()
    );
    Ok(())
}

fn verify_webhook_signature(
    payload_raw: &str,
    signature: Option<&str>,
    timestamp: Option<&str>,
    secret: Option<&str>,
    algorithm: Option<WebhookSignatureAlgorithm>,
    now_unix_ms: u64,
    max_skew_seconds: u64,
) -> Result<Option<String>> {
    let has_signature_inputs =
        signature.is_some() || timestamp.is_some() || secret.is_some() || algorithm.is_some();
    if !has_signature_inputs {
        return Ok(None);
    }

    let signature = signature
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("--event-webhook-signature is required when webhook signing is configured")
        })?;
    let secret = secret
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow!("--event-webhook-secret is required when webhook signing is configured")
        })?;
    let algorithm = algorithm.ok_or_else(|| {
        anyhow!(
            "--event-webhook-signature-algorithm is required when webhook signing is configured"
        )
    })?;

    let replay_key = match algorithm {
        WebhookSignatureAlgorithm::GithubSha256 => {
            verify_github_sha256_signature(payload_raw.as_bytes(), signature, secret)?;
            if let Some(value) = timestamp {
                validate_timestamp_skew(value, now_unix_ms, max_skew_seconds)?;
            }
            format!(
                "github_sha256:{}:{}",
                timestamp.unwrap_or_default(),
                signature
            )
        }
        WebhookSignatureAlgorithm::SlackV0 => {
            let timestamp = timestamp
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow!("--event-webhook-timestamp is required for slack-v0 signatures")
                })?;
            verify_slack_v0_signature(payload_raw, signature, timestamp, secret)?;
            validate_timestamp_skew(timestamp, now_unix_ms, max_skew_seconds)?;
            format!("slack_v0:{timestamp}:{signature}")
        }
    };

    Ok(Some(replay_key.to_ascii_lowercase()))
}

fn verify_github_sha256_signature(payload: &[u8], signature: &str, secret: &str) -> Result<()> {
    let Some(digest_hex) = signature.strip_prefix("sha256=") else {
        bail!("github webhook signature must use sha256=<hex> format");
    };
    let signature_bytes = decode_hex(digest_hex)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .context("failed to initialize webhook HMAC verifier")?;
    mac.update(payload);
    mac.verify_slice(&signature_bytes)
        .map_err(|_| anyhow!("webhook signature verification failed"))
}

fn verify_slack_v0_signature(
    payload: &str,
    signature: &str,
    timestamp: &str,
    secret: &str,
) -> Result<()> {
    let Some(digest_hex) = signature.strip_prefix("v0=") else {
        bail!("slack webhook signature must use v0=<hex> format");
    };
    let signature_bytes = decode_hex(digest_hex)?;
    let signed_payload = format!("v0:{timestamp}:{payload}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .context("failed to initialize webhook HMAC verifier")?;
    mac.update(signed_payload.as_bytes());
    mac.verify_slice(&signature_bytes)
        .map_err(|_| anyhow!("webhook signature verification failed"))
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("signature digest cannot be empty");
    }
    if !trimmed.len().is_multiple_of(2) {
        bail!("signature digest must have an even number of hex characters");
    }

    let mut bytes = Vec::with_capacity(trimmed.len() / 2);
    let raw = trimmed.as_bytes();
    let mut index = 0usize;
    while index < raw.len() {
        let hex = std::str::from_utf8(&raw[index..index + 2]).context("invalid utf-8 in digest")?;
        let byte = u8::from_str_radix(hex, 16)
            .with_context(|| format!("invalid hex byte '{}' in signature digest", hex))?;
        bytes.push(byte);
        index = index.saturating_add(2);
    }
    Ok(bytes)
}

fn validate_timestamp_skew(timestamp: &str, now_unix_ms: u64, max_skew_seconds: u64) -> Result<()> {
    if max_skew_seconds == 0 {
        return Ok(());
    }
    let timestamp_seconds = timestamp
        .trim()
        .parse::<u64>()
        .with_context(|| format!("invalid webhook timestamp '{}'", timestamp))?;
    let now_seconds = now_unix_ms / 1_000;
    let skew = now_seconds.abs_diff(timestamp_seconds);
    if skew > max_skew_seconds {
        bail!(
            "webhook timestamp skew {}s exceeds max {}s",
            skew,
            max_skew_seconds
        );
    }
    Ok(())
}

fn enforce_signature_replay_guard(
    state: &mut EventRunnerState,
    replay_key: &str,
    now_unix_ms: u64,
    max_skew_seconds: u64,
) -> Result<()> {
    let window_ms = max_skew_seconds.max(1).saturating_mul(1_000);
    let retain_window_ms = window_ms.saturating_mul(3);
    state
        .signature_replay_last_seen_unix_ms
        .retain(|_key, seen| now_unix_ms.saturating_sub(*seen) <= retain_window_ms);

    if let Some(last_seen) = state.signature_replay_last_seen_unix_ms.get(replay_key) {
        if now_unix_ms.saturating_sub(*last_seen) <= window_ms {
            bail!("webhook signature replay detected for key '{}'", replay_key);
        }
    }

    state
        .signature_replay_last_seen_unix_ms
        .insert(replay_key.to_string(), now_unix_ms);
    Ok(())
}

struct EventSchedulerRuntime {
    config: EventSchedulerConfig,
    state: EventRunnerState,
}

impl EventSchedulerRuntime {
    fn new(config: EventSchedulerConfig) -> Result<Self> {
        std::fs::create_dir_all(&config.events_dir)
            .with_context(|| format!("failed to create {}", config.events_dir.display()))?;
        let state = load_runner_state(&config.state_path)?;
        Ok(Self { config, state })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            match self.poll_once(current_unix_timestamp_ms()).await {
                Ok(report) => {
                    if report.discovered > 0
                        || report.executed > 0
                        || report.stale_skipped > 0
                        || report.malformed_skipped > 0
                        || report.failed > 0
                    {
                        println!(
                            "events poll: discovered={} queued={} executed={} stale_skipped={} malformed_skipped={} failed={}",
                            report.discovered,
                            report.queued,
                            report.executed,
                            report.stale_skipped,
                            report.malformed_skipped,
                            report.failed
                        );
                    }
                }
                Err(error) => {
                    eprintln!("events poll error: {error}");
                }
            }

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("events scheduler shutdown requested");
                    return Ok(());
                }
                _ = tokio::time::sleep(self.config.poll_interval) => {}
            }
        }
    }

    async fn poll_once(&mut self, now_unix_ms: u64) -> Result<EventPollReport> {
        let mut report = EventPollReport::default();

        let (records, malformed) = load_event_records(&self.config.events_dir)?;
        report.discovered = records.len();
        report.malformed_skipped = malformed;

        let mut queued = Vec::new();
        for record in records {
            let decision = due_decision(
                &record.definition,
                &self.state,
                now_unix_ms,
                self.config.stale_immediate_max_age_seconds,
            )?;
            match decision {
                DueDecision::Run => {
                    queued.push(record);
                    if queued.len() >= self.config.queue_limit.max(1) {
                        break;
                    }
                }
                DueDecision::SkipStaleRemove => {
                    report.stale_skipped = report.stale_skipped.saturating_add(1);
                    let _ = std::fs::remove_file(&record.path);
                }
                DueDecision::NotDue => {}
            }
        }
        report.queued = queued.len();

        for record in queued {
            match execute_event(&self.config, &record.definition, now_unix_ms).await {
                Ok(()) => {
                    report.executed = report.executed.saturating_add(1);
                    match &record.definition.schedule {
                        EventSchedule::Immediate | EventSchedule::At { .. } => {
                            let _ = std::fs::remove_file(&record.path);
                        }
                        EventSchedule::Periodic { .. } => {
                            self.state
                                .periodic_last_run_unix_ms
                                .insert(record.definition.id.clone(), now_unix_ms);
                        }
                    }
                }
                Err(error) => {
                    report.failed = report.failed.saturating_add(1);
                    eprintln!(
                        "event execution failed: id={} channel={} error={error}",
                        record.definition.id, record.definition.channel
                    );
                }
            }
        }

        save_runner_state(&self.config.state_path, &self.state)?;
        Ok(report)
    }
}

async fn execute_event(
    config: &EventSchedulerConfig,
    event: &EventDefinition,
    now_unix_ms: u64,
) -> Result<()> {
    let channel_ref = ChannelStore::parse_channel_ref(&event.channel)?;
    let channel_store = ChannelStore::open(
        &config.channel_store_root,
        &channel_ref.transport,
        &channel_ref.channel_id,
    )?;
    channel_store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: now_unix_ms,
        direction: "inbound".to_string(),
        event_key: Some(event.id.clone()),
        source: "events".to_string(),
        payload: serde_json::json!({
            "event_id": event.id,
            "channel": event.channel,
            "prompt": event.prompt,
            "schedule": event.schedule,
        }),
    })?;

    let mut agent = Agent::new(
        config.client.clone(),
        AgentConfig {
            model: config.model.clone(),
            system_prompt: config.system_prompt.clone(),
            max_turns: config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );
    crate::tools::register_builtin_tools(&mut agent, config.tool_policy.clone());

    let usage = std::sync::Arc::new(std::sync::Mutex::new((0_u64, 0_u64, 0_u64)));
    agent.subscribe({
        let usage = usage.clone();
        move |event| {
            if let AgentEvent::TurnEnd {
                usage: turn_usage, ..
            } = event
            {
                if let Ok(mut guard) = usage.lock() {
                    guard.0 = guard.0.saturating_add(turn_usage.input_tokens);
                    guard.1 = guard.1.saturating_add(turn_usage.output_tokens);
                    guard.2 = guard.2.saturating_add(turn_usage.total_tokens);
                }
            }
        }
    });

    let mut session_runtime = Some(initialize_channel_session_runtime(
        &channel_store.session_path(),
        &config.system_prompt,
        config.session_lock_wait_ms,
        config.session_lock_stale_ms,
        &mut agent,
    )?);

    let prompt = format!(
        "You are executing an autonomous scheduled task for channel {}.\n\nTask prompt:\n{}",
        event.channel, event.prompt
    );
    let start_index = agent.messages().len();
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut session_runtime,
        &prompt,
        config.turn_timeout_ms,
        pending::<()>(),
        config.render_options,
    )
    .await?;

    let assistant_reply = if status == PromptRunStatus::Cancelled {
        "Scheduled run cancelled before completion.".to_string()
    } else if status == PromptRunStatus::TimedOut {
        "Scheduled run timed out before completion.".to_string()
    } else {
        collect_assistant_reply(&agent.messages()[start_index..])
    };

    let (input_tokens, output_tokens, total_tokens) = usage
        .lock()
        .map_err(|_| anyhow!("usage lock poisoned"))?
        .to_owned();

    channel_store.sync_context_from_messages(agent.messages())?;
    channel_store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        direction: "outbound".to_string(),
        event_key: Some(event.id.clone()),
        source: "events".to_string(),
        payload: serde_json::json!({
            "event_id": event.id,
            "status": format!("{:?}", status).to_lowercase(),
            "assistant_reply": assistant_reply,
            "tokens": {
                "input": input_tokens,
                "output": output_tokens,
                "total": total_tokens,
            }
        }),
    })?;

    Ok(())
}

fn inspect_events(config: &EventsInspectConfig, now_unix_ms: u64) -> Result<EventsInspectReport> {
    let queue_limit = config.queue_limit.max(1);
    let (records, malformed_events) = load_event_records(&config.events_dir)?;
    let state = load_runner_state(&config.state_path)?;

    let mut report = EventsInspectReport {
        events_dir: config.events_dir.display().to_string(),
        state_path: config.state_path.display().to_string(),
        now_unix_ms,
        queue_limit,
        stale_immediate_max_age_seconds: config.stale_immediate_max_age_seconds,
        discovered_events: records.len(),
        malformed_events,
        enabled_events: 0,
        disabled_events: 0,
        schedule_immediate_events: 0,
        schedule_at_events: 0,
        schedule_periodic_events: 0,
        due_now_events: 0,
        queued_now_events: 0,
        not_due_events: 0,
        stale_immediate_events: 0,
        due_eval_failed_events: 0,
        periodic_with_last_run_state: 0,
        periodic_missing_last_run_state: 0,
    };

    for record in records {
        let event = &record.definition;
        if event.enabled {
            report.enabled_events = report.enabled_events.saturating_add(1);
        } else {
            report.disabled_events = report.disabled_events.saturating_add(1);
        }

        match &event.schedule {
            EventSchedule::Immediate => {
                report.schedule_immediate_events =
                    report.schedule_immediate_events.saturating_add(1);
            }
            EventSchedule::At { .. } => {
                report.schedule_at_events = report.schedule_at_events.saturating_add(1);
            }
            EventSchedule::Periodic { .. } => {
                report.schedule_periodic_events = report.schedule_periodic_events.saturating_add(1);
                if state.periodic_last_run_unix_ms.contains_key(&event.id) {
                    report.periodic_with_last_run_state =
                        report.periodic_with_last_run_state.saturating_add(1);
                } else {
                    report.periodic_missing_last_run_state =
                        report.periodic_missing_last_run_state.saturating_add(1);
                }
            }
        }

        match due_decision(
            event,
            &state,
            now_unix_ms,
            config.stale_immediate_max_age_seconds,
        ) {
            Ok(DueDecision::Run) => {
                report.due_now_events = report.due_now_events.saturating_add(1);
            }
            Ok(DueDecision::NotDue) => {
                report.not_due_events = report.not_due_events.saturating_add(1);
            }
            Ok(DueDecision::SkipStaleRemove) => {
                report.stale_immediate_events = report.stale_immediate_events.saturating_add(1);
            }
            Err(_) => {
                report.due_eval_failed_events = report.due_eval_failed_events.saturating_add(1);
            }
        }
    }

    report.queued_now_events = report.due_now_events.min(queue_limit);
    Ok(report)
}

fn render_events_inspect_report(report: &EventsInspectReport) -> String {
    format!(
        "events inspect: events_dir={} state_path={} now_unix_ms={} discovered_events={} malformed_events={} enabled_events={} disabled_events={} due_now_events={} queued_now_events={} not_due_events={} stale_immediate_events={} due_eval_failed_events={} schedule_immediate_events={} schedule_at_events={} schedule_periodic_events={} periodic_with_last_run_state={} periodic_missing_last_run_state={} queue_limit={} stale_immediate_max_age_seconds={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.discovered_events,
        report.malformed_events,
        report.enabled_events,
        report.disabled_events,
        report.due_now_events,
        report.queued_now_events,
        report.not_due_events,
        report.stale_immediate_events,
        report.due_eval_failed_events,
        report.schedule_immediate_events,
        report.schedule_at_events,
        report.schedule_periodic_events,
        report.periodic_with_last_run_state,
        report.periodic_missing_last_run_state,
        report.queue_limit,
        report.stale_immediate_max_age_seconds,
    )
}

fn validate_events_definitions(
    config: &EventsValidateConfig,
    now_unix_ms: u64,
) -> Result<EventsValidateReport> {
    let state = load_runner_state(&config.state_path)?;
    let event_paths = collect_event_definition_paths(&config.events_dir)?;

    let mut report = EventsValidateReport {
        events_dir: config.events_dir.display().to_string(),
        state_path: config.state_path.display().to_string(),
        now_unix_ms,
        total_files: event_paths.len(),
        valid_files: 0,
        invalid_files: 0,
        malformed_files: 0,
        failed_files: 0,
        disabled_files: 0,
        diagnostics: Vec::new(),
    };

    for path in event_paths {
        let path_text = path.display().to_string();
        let raw = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) => {
                report.malformed_files = report.malformed_files.saturating_add(1);
                report.diagnostics.push(EventsValidateDiagnostic {
                    path: path_text,
                    event_id: None,
                    reason_code: "read_error".to_string(),
                    message: sanitize_error_message(&error.to_string()),
                });
                continue;
            }
        };

        let definition = match serde_json::from_str::<EventDefinition>(&raw) {
            Ok(value) => value,
            Err(error) => {
                report.malformed_files = report.malformed_files.saturating_add(1);
                report.diagnostics.push(EventsValidateDiagnostic {
                    path: path_text,
                    event_id: None,
                    reason_code: "json_parse".to_string(),
                    message: sanitize_error_message(&error.to_string()),
                });
                continue;
            }
        };

        if !definition.enabled {
            report.disabled_files = report.disabled_files.saturating_add(1);
        }

        let mut has_failure = false;
        if let Err(error) = ChannelStore::parse_channel_ref(&definition.channel) {
            has_failure = true;
            report.diagnostics.push(EventsValidateDiagnostic {
                path: path_text.clone(),
                event_id: Some(definition.id.clone()),
                reason_code: "channel_ref_invalid".to_string(),
                message: sanitize_error_message(&error.to_string()),
            });
        }

        if let Err(error) = validate_event_schedule(&definition, &state, now_unix_ms) {
            has_failure = true;
            report.diagnostics.push(EventsValidateDiagnostic {
                path: path_text,
                event_id: Some(definition.id.clone()),
                reason_code: "schedule_invalid".to_string(),
                message: sanitize_error_message(&error.to_string()),
            });
        }

        if has_failure {
            report.invalid_files = report.invalid_files.saturating_add(1);
        } else {
            report.valid_files = report.valid_files.saturating_add(1);
        }
    }

    report.failed_files = report.invalid_files.saturating_add(report.malformed_files);
    Ok(report)
}

fn render_events_validate_report(report: &EventsValidateReport) -> String {
    let mut lines = vec![format!(
        "events validate: events_dir={} state_path={} now_unix_ms={} total_files={} valid_files={} invalid_files={} malformed_files={} failed_files={} disabled_files={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.total_files,
        report.valid_files,
        report.invalid_files,
        report.malformed_files,
        report.failed_files,
        report.disabled_files,
    )];

    for diagnostic in &report.diagnostics {
        lines.push(format!(
            "events validate error: path={} event_id={} reason_code={} message={}",
            diagnostic.path,
            diagnostic
                .event_id
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("none"),
            diagnostic.reason_code,
            diagnostic.message
        ));
    }

    lines.join("\n")
}

fn validate_event_schedule(
    event: &EventDefinition,
    state: &EventRunnerState,
    now_unix_ms: u64,
) -> Result<()> {
    match &event.schedule {
        EventSchedule::Immediate => Ok(()),
        EventSchedule::At { .. } => Ok(()),
        EventSchedule::Periodic { cron, timezone } => {
            let from_unix_ms = state
                .periodic_last_run_unix_ms
                .get(&event.id)
                .copied()
                .unwrap_or_else(|| now_unix_ms.saturating_sub(60_000));
            let _ = next_periodic_due_unix_ms(cron, timezone, from_unix_ms)?;
            Ok(())
        }
    }
}

fn collect_event_definition_paths(events_dir: &Path) -> Result<Vec<PathBuf>> {
    if !events_dir.exists() {
        return Ok(Vec::new());
    }
    if !events_dir.is_dir() {
        bail!("events dir is not a directory: {}", events_dir.display());
    }

    let mut paths = Vec::new();
    for entry in std::fs::read_dir(events_dir)
        .with_context(|| format!("failed to read {}", events_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", events_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort_by_key(|path| path.display().to_string());
    Ok(paths)
}

fn sanitize_error_message(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn schedule_name(schedule: &EventSchedule) -> &'static str {
    match schedule {
        EventSchedule::Immediate => "immediate",
        EventSchedule::At { .. } => "at",
        EventSchedule::Periodic { .. } => "periodic",
    }
}

fn simulate_events(
    config: &EventsSimulateConfig,
    now_unix_ms: u64,
) -> Result<EventsSimulateReport> {
    let state = load_runner_state(&config.state_path)?;
    let event_paths = collect_event_definition_paths(&config.events_dir)?;
    let total_files = event_paths.len();
    let horizon_unix_ms = now_unix_ms.saturating_add(config.horizon_seconds.saturating_mul(1_000));

    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    let mut malformed_files = 0usize;

    for path in event_paths {
        let path_text = path.display().to_string();
        let raw = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) => {
                malformed_files = malformed_files.saturating_add(1);
                diagnostics.push(EventsValidateDiagnostic {
                    path: path_text,
                    event_id: None,
                    reason_code: "read_error".to_string(),
                    message: sanitize_error_message(&error.to_string()),
                });
                continue;
            }
        };
        let definition = match serde_json::from_str::<EventDefinition>(&raw) {
            Ok(value) => value,
            Err(error) => {
                malformed_files = malformed_files.saturating_add(1);
                diagnostics.push(EventsValidateDiagnostic {
                    path: path_text,
                    event_id: None,
                    reason_code: "json_parse".to_string(),
                    message: sanitize_error_message(&error.to_string()),
                });
                continue;
            }
        };

        if let Err(error) = ChannelStore::parse_channel_ref(&definition.channel) {
            diagnostics.push(EventsValidateDiagnostic {
                path: path_text,
                event_id: Some(definition.id),
                reason_code: "channel_ref_invalid".to_string(),
                message: sanitize_error_message(&error.to_string()),
            });
            continue;
        }

        let next_due_unix_ms = match &definition.schedule {
            EventSchedule::Immediate => {
                if definition.enabled {
                    let decision = due_decision(
                        &definition,
                        &state,
                        now_unix_ms,
                        config.stale_immediate_max_age_seconds,
                    )?;
                    match decision {
                        DueDecision::Run => Some(now_unix_ms),
                        DueDecision::SkipStaleRemove => None,
                        DueDecision::NotDue => None,
                    }
                } else {
                    None
                }
            }
            EventSchedule::At { at_unix_ms } => Some(*at_unix_ms),
            EventSchedule::Periodic { cron, timezone } => {
                let last_run = state
                    .periodic_last_run_unix_ms
                    .get(&definition.id)
                    .copied()
                    .unwrap_or_else(|| now_unix_ms.saturating_sub(60_000));
                match next_periodic_due_unix_ms(cron, timezone, last_run) {
                    Ok(next_due) => Some(next_due),
                    Err(error) => {
                        diagnostics.push(EventsValidateDiagnostic {
                            path: path_text,
                            event_id: Some(definition.id),
                            reason_code: "schedule_invalid".to_string(),
                            message: sanitize_error_message(&error.to_string()),
                        });
                        continue;
                    }
                }
            }
        };

        let due_now = definition.enabled
            && next_due_unix_ms
                .map(|value| value <= now_unix_ms)
                .unwrap_or(false);
        let within_horizon = definition.enabled
            && next_due_unix_ms
                .map(|value| value <= horizon_unix_ms)
                .unwrap_or(false);
        let last_run_unix_ms = state.periodic_last_run_unix_ms.get(&definition.id).copied();
        rows.push(EventsSimulateRow {
            path: path.display().to_string(),
            event_id: definition.id,
            channel: definition.channel,
            schedule: schedule_name(&definition.schedule).to_string(),
            enabled: definition.enabled,
            next_due_unix_ms,
            due_now,
            within_horizon,
            last_run_unix_ms,
        });
    }

    rows.sort_by(|left, right| {
        left.event_id
            .cmp(&right.event_id)
            .then_with(|| left.path.cmp(&right.path))
    });
    let due_now_rows = rows.iter().filter(|row| row.due_now).count();
    let within_horizon_rows = rows.iter().filter(|row| row.within_horizon).count();
    let invalid_rows = diagnostics.len().saturating_sub(malformed_files);

    Ok(EventsSimulateReport {
        events_dir: config.events_dir.display().to_string(),
        state_path: config.state_path.display().to_string(),
        now_unix_ms,
        horizon_seconds: config.horizon_seconds,
        total_files,
        simulated_rows: rows.len(),
        malformed_files,
        invalid_rows,
        due_now_rows,
        within_horizon_rows,
        rows,
        diagnostics,
    })
}

fn render_events_simulate_report(report: &EventsSimulateReport) -> String {
    let mut lines = vec![format!(
        "events simulate: events_dir={} state_path={} now_unix_ms={} horizon_seconds={} total_files={} simulated_rows={} malformed_files={} invalid_rows={} due_now_rows={} within_horizon_rows={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.horizon_seconds,
        report.total_files,
        report.simulated_rows,
        report.malformed_files,
        report.invalid_rows,
        report.due_now_rows,
        report.within_horizon_rows,
    )];

    for row in &report.rows {
        lines.push(format!(
            "events simulate row: path={} event_id={} schedule={} enabled={} next_due_unix_ms={} due_now={} within_horizon={} last_run_unix_ms={} channel={}",
            row.path,
            row.event_id,
            row.schedule,
            row.enabled,
            row.next_due_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.due_now,
            row.within_horizon,
            row.last_run_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.channel,
        ));
    }

    for diagnostic in &report.diagnostics {
        lines.push(format!(
            "events simulate error: path={} event_id={} reason_code={} message={}",
            diagnostic.path,
            diagnostic
                .event_id
                .as_deref()
                .filter(|value| !value.is_empty())
                .unwrap_or("none"),
            diagnostic.reason_code,
            diagnostic.message
        ));
    }

    lines.join("\n")
}

fn dry_run_events(config: &EventsDryRunConfig, now_unix_ms: u64) -> Result<EventsDryRunReport> {
    let state = load_runner_state(&config.state_path)?;
    let event_paths = collect_event_definition_paths(&config.events_dir)?;
    let total_files = event_paths.len();
    let queue_limit = config.queue_limit.max(1);
    let mut rows = Vec::new();
    let mut malformed_files = 0usize;
    let mut candidates = Vec::new();

    for path in event_paths {
        let path_text = path.display().to_string();
        let raw = match std::fs::read_to_string(&path) {
            Ok(value) => value,
            Err(error) => {
                malformed_files = malformed_files.saturating_add(1);
                rows.push(EventsDryRunRow {
                    path: path_text,
                    event_id: None,
                    channel: None,
                    schedule: None,
                    enabled: None,
                    decision: "error".to_string(),
                    reason_code: "read_error".to_string(),
                    queue_position: None,
                    last_run_unix_ms: None,
                    message: Some(sanitize_error_message(&error.to_string())),
                });
                continue;
            }
        };
        let definition = match serde_json::from_str::<EventDefinition>(&raw) {
            Ok(value) => value,
            Err(error) => {
                malformed_files = malformed_files.saturating_add(1);
                rows.push(EventsDryRunRow {
                    path: path_text,
                    event_id: None,
                    channel: None,
                    schedule: None,
                    enabled: None,
                    decision: "error".to_string(),
                    reason_code: "json_parse".to_string(),
                    queue_position: None,
                    last_run_unix_ms: None,
                    message: Some(sanitize_error_message(&error.to_string())),
                });
                continue;
            }
        };
        let schedule = schedule_name(&definition.schedule).to_string();
        if let Err(error) = ChannelStore::parse_channel_ref(&definition.channel) {
            rows.push(EventsDryRunRow {
                path: path_text,
                event_id: Some(definition.id),
                channel: Some(definition.channel),
                schedule: Some(schedule),
                enabled: Some(definition.enabled),
                decision: "error".to_string(),
                reason_code: "channel_ref_invalid".to_string(),
                queue_position: None,
                last_run_unix_ms: None,
                message: Some(sanitize_error_message(&error.to_string())),
            });
            continue;
        }
        candidates.push((path, definition));
    }

    candidates.sort_by(|left, right| {
        left.1
            .id
            .cmp(&right.1.id)
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut queued = 0usize;
    for (path, definition) in candidates {
        let schedule = schedule_name(&definition.schedule).to_string();
        let path_text = path.display().to_string();
        let last_run_unix_ms = state.periodic_last_run_unix_ms.get(&definition.id).copied();

        if queued >= queue_limit {
            rows.push(EventsDryRunRow {
                path: path_text,
                event_id: Some(definition.id),
                channel: Some(definition.channel),
                schedule: Some(schedule),
                enabled: Some(definition.enabled),
                decision: "skip".to_string(),
                reason_code: "queue_limit_reached".to_string(),
                queue_position: None,
                last_run_unix_ms,
                message: None,
            });
            continue;
        }

        match due_decision(
            &definition,
            &state,
            now_unix_ms,
            config.stale_immediate_max_age_seconds,
        ) {
            Ok(DueDecision::Run) => {
                queued = queued.saturating_add(1);
                rows.push(EventsDryRunRow {
                    path: path_text,
                    event_id: Some(definition.id),
                    channel: Some(definition.channel),
                    schedule: Some(schedule),
                    enabled: Some(definition.enabled),
                    decision: "execute".to_string(),
                    reason_code: "due_now".to_string(),
                    queue_position: Some(queued),
                    last_run_unix_ms,
                    message: None,
                });
            }
            Ok(DueDecision::NotDue) => {
                rows.push(EventsDryRunRow {
                    path: path_text,
                    event_id: Some(definition.id),
                    channel: Some(definition.channel),
                    schedule: Some(schedule),
                    enabled: Some(definition.enabled),
                    decision: "skip".to_string(),
                    reason_code: "not_due".to_string(),
                    queue_position: None,
                    last_run_unix_ms,
                    message: None,
                });
            }
            Ok(DueDecision::SkipStaleRemove) => {
                rows.push(EventsDryRunRow {
                    path: path_text,
                    event_id: Some(definition.id),
                    channel: Some(definition.channel),
                    schedule: Some(schedule),
                    enabled: Some(definition.enabled),
                    decision: "skip".to_string(),
                    reason_code: "stale_immediate".to_string(),
                    queue_position: None,
                    last_run_unix_ms,
                    message: None,
                });
            }
            Err(error) => {
                rows.push(EventsDryRunRow {
                    path: path_text,
                    event_id: Some(definition.id),
                    channel: Some(definition.channel),
                    schedule: Some(schedule),
                    enabled: Some(definition.enabled),
                    decision: "error".to_string(),
                    reason_code: "schedule_invalid".to_string(),
                    queue_position: None,
                    last_run_unix_ms,
                    message: Some(sanitize_error_message(&error.to_string())),
                });
            }
        }
    }

    rows.sort_by(|left, right| {
        left.event_id
            .as_deref()
            .unwrap_or("")
            .cmp(right.event_id.as_deref().unwrap_or(""))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.reason_code.cmp(&right.reason_code))
    });
    let execute_rows = rows.iter().filter(|row| row.decision == "execute").count();
    let skipped_rows = rows.iter().filter(|row| row.decision == "skip").count();
    let error_rows = rows.iter().filter(|row| row.decision == "error").count();

    Ok(EventsDryRunReport {
        events_dir: config.events_dir.display().to_string(),
        state_path: config.state_path.display().to_string(),
        now_unix_ms,
        queue_limit,
        total_files,
        evaluated_rows: rows.len(),
        execute_rows,
        skipped_rows,
        error_rows,
        malformed_files,
        rows,
    })
}

fn render_events_dry_run_report(report: &EventsDryRunReport) -> String {
    let mut lines = vec![format!(
        "events dry run: events_dir={} state_path={} now_unix_ms={} queue_limit={} total_files={} evaluated_rows={} execute_rows={} skipped_rows={} error_rows={} malformed_files={}",
        report.events_dir,
        report.state_path,
        report.now_unix_ms,
        report.queue_limit,
        report.total_files,
        report.evaluated_rows,
        report.execute_rows,
        report.skipped_rows,
        report.error_rows,
        report.malformed_files,
    )];

    for row in &report.rows {
        lines.push(format!(
            "events dry run row: path={} event_id={} schedule={} enabled={} decision={} reason_code={} queue_position={} last_run_unix_ms={} channel={} message={}",
            row.path,
            row.event_id.as_deref().unwrap_or("none"),
            row.schedule.as_deref().unwrap_or("none"),
            row.enabled
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.decision,
            row.reason_code,
            row.queue_position
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.last_run_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            row.channel.as_deref().unwrap_or("none"),
            row.message.as_deref().unwrap_or("none"),
        ));
    }

    lines.join("\n")
}

fn due_decision(
    event: &EventDefinition,
    state: &EventRunnerState,
    now_unix_ms: u64,
    stale_immediate_max_age_seconds: u64,
) -> Result<DueDecision> {
    if !event.enabled {
        return Ok(DueDecision::NotDue);
    }

    match &event.schedule {
        EventSchedule::Immediate => {
            if stale_immediate_max_age_seconds == 0 {
                return Ok(DueDecision::Run);
            }
            let created = event.created_unix_ms.unwrap_or(now_unix_ms);
            let max_age_ms = stale_immediate_max_age_seconds.saturating_mul(1000);
            if now_unix_ms.saturating_sub(created) > max_age_ms {
                return Ok(DueDecision::SkipStaleRemove);
            }
            Ok(DueDecision::Run)
        }
        EventSchedule::At { at_unix_ms } => {
            if now_unix_ms >= *at_unix_ms {
                Ok(DueDecision::Run)
            } else {
                Ok(DueDecision::NotDue)
            }
        }
        EventSchedule::Periodic { cron, timezone } => {
            let last_run = state
                .periodic_last_run_unix_ms
                .get(&event.id)
                .copied()
                .unwrap_or_else(|| now_unix_ms.saturating_sub(60_000));
            let next_due = next_periodic_due_unix_ms(cron, timezone, last_run)?;
            if next_due <= now_unix_ms {
                Ok(DueDecision::Run)
            } else {
                Ok(DueDecision::NotDue)
            }
        }
    }
}

fn next_periodic_due_unix_ms(cron: &str, timezone: &str, from_unix_ms: u64) -> Result<u64> {
    let schedule =
        Schedule::from_str(cron).with_context(|| format!("invalid cron expression '{}'", cron))?;
    let tz: Tz = timezone
        .parse()
        .with_context(|| format!("invalid timezone '{}'", timezone))?;
    let from = tz
        .timestamp_millis_opt(i64::try_from(from_unix_ms).unwrap_or(i64::MAX))
        .single()
        .ok_or_else(|| anyhow!("invalid from timestamp for periodic schedule"))?;
    let next = schedule
        .after(&from)
        .next()
        .ok_or_else(|| anyhow!("cron expression '{}' has no future occurrence", cron))?;
    Ok(u64::try_from(next.timestamp_millis()).unwrap_or(u64::MAX))
}

fn load_event_records(events_dir: &Path) -> Result<(Vec<EventRecord>, usize)> {
    if !events_dir.exists() {
        return Ok((Vec::new(), 0));
    }

    let mut records = Vec::new();
    let mut malformed = 0_usize;
    for entry in std::fs::read_dir(events_dir)
        .with_context(|| format!("failed to read {}", events_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", events_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => {
                malformed = malformed.saturating_add(1);
                continue;
            }
        };
        let mut definition = match serde_json::from_str::<EventDefinition>(&raw) {
            Ok(def) => def,
            Err(_) => {
                malformed = malformed.saturating_add(1);
                continue;
            }
        };

        if definition.created_unix_ms.is_none() {
            let created = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())
                .and_then(|mtime| mtime.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|dur| dur.as_millis() as u64)
                .unwrap_or_else(current_unix_timestamp_ms);
            definition.created_unix_ms = Some(created);
        }

        records.push(EventRecord { path, definition });
    }

    records.sort_by(|left, right| left.definition.id.cmp(&right.definition.id));
    Ok((records, malformed))
}

fn load_runner_state(path: &Path) -> Result<EventRunnerState> {
    if !path.exists() {
        return Ok(EventRunnerState::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let state = serde_json::from_str::<EventRunnerState>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if state.schema_version != EVENT_RUNNER_STATE_SCHEMA_VERSION {
        bail!(
            "unsupported event runner state schema: expected {}, found {}",
            EVENT_RUNNER_STATE_SCHEMA_VERSION,
            state.schema_version
        );
    }
    Ok(state)
}

fn save_runner_state(path: &Path, state: &EventRunnerState) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut payload = serde_json::to_string_pretty(state).context("failed to serialize state")?;
    payload.push('\n');
    write_text_atomic(path, &payload).with_context(|| format!("failed to write {}", path.display()))
}

fn initialize_channel_session_runtime(
    session_path: &Path,
    system_prompt: &str,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
    agent: &mut Agent,
) -> Result<SessionRuntime> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    let active_head = store.ensure_initialized(system_prompt)?;
    let lineage = store.lineage_messages(active_head)?;
    if !lineage.is_empty() {
        agent.replace_messages(lineage);
    }
    Ok(SessionRuntime { store, active_head })
}

fn collect_assistant_reply(messages: &[Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this event.".to_string()
    } else {
        content
    }
}

fn short_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest[..6]
        .iter()
        .map(|value| format!("{:02x}", value))
        .collect::<String>()
}

fn sanitize_for_path(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "event".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use clap::Parser;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tempfile::tempdir;

    use super::{
        dry_run_events, due_decision, execute_events_template_write_command,
        ingest_webhook_immediate_event, inspect_events, load_event_records,
        next_periodic_due_unix_ms, render_events_dry_run_report, render_events_inspect_report,
        render_events_simulate_report, render_events_validate_report, simulate_events,
        validate_events_definitions, DueDecision, EventDefinition, EventRunnerState, EventSchedule,
        EventSchedulerConfig, EventSchedulerRuntime, EventWebhookIngestConfig, EventsDryRunConfig,
        EventsInspectConfig, EventsSimulateConfig, EventsValidateConfig, EventsValidateReport,
        WebhookSignatureAlgorithm,
    };
    use crate::{tools::ToolPolicy, Cli, CliEventTemplateSchedule, RenderOptions};

    struct StaticReplyClient;

    #[async_trait]
    impl LlmClient for StaticReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("scheduled reply"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 4,
                    output_tokens: 5,
                    total_tokens: 9,
                },
            })
        }
    }

    fn scheduler_config(root: &Path) -> EventSchedulerConfig {
        EventSchedulerConfig {
            client: Arc::new(StaticReplyClient),
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "You are rsBot.".to_string(),
            max_turns: 4,
            tool_policy: ToolPolicy::new(vec![root.to_path_buf()]),
            turn_timeout_ms: 0,
            render_options: RenderOptions {
                stream_output: false,
                stream_delay_ms: 0,
            },
            session_lock_wait_ms: 2_000,
            session_lock_stale_ms: 30_000,
            channel_store_root: root.join("channel-store"),
            events_dir: root.join("events"),
            state_path: root.join("events/state.json"),
            poll_interval: Duration::from_millis(1),
            queue_limit: 16,
            stale_immediate_max_age_seconds: 3_600,
        }
    }

    fn write_event(path: &Path, event: &EventDefinition) {
        let mut payload = serde_json::to_string_pretty(event).expect("serialize event");
        payload.push('\n');
        std::fs::write(path, payload).expect("write event file");
    }

    fn template_cli(path: &Path) -> Cli {
        let mut cli = Cli::parse_from(["tau-rs"]);
        cli.events_template_write = Some(path.to_path_buf());
        cli.events_template_schedule = CliEventTemplateSchedule::Immediate;
        cli.events_template_overwrite = false;
        cli.events_template_id = None;
        cli.events_template_channel = None;
        cli.events_template_prompt = None;
        cli.events_template_at_unix_ms = None;
        cli.events_template_cron = None;
        cli.events_template_timezone = "UTC".to_string();
        cli
    }

    fn github_signature(secret: &str, payload: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("mac");
        mac.update(payload.as_bytes());
        let digest = mac.finalize().into_bytes();
        format!(
            "sha256={}",
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    fn slack_v0_signature(secret: &str, timestamp: &str, payload: &str) -> String {
        let signed = format!("v0:{timestamp}:{payload}");
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("mac");
        mac.update(signed.as_bytes());
        let digest = mac.finalize().into_bytes();
        format!(
            "v0={}",
            digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    #[test]
    fn unit_due_decision_and_cron_timezone_computation_are_stable() {
        let now = 1_700_000_000_000_u64;
        let mut state = EventRunnerState::default();
        state
            .periodic_last_run_unix_ms
            .insert("periodic-1".to_string(), now.saturating_sub(120_000));

        let periodic = EventDefinition {
            id: "periodic-1".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "check".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "0/1 * * * * * *".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: true,
            created_unix_ms: Some(now),
        };
        let decision = due_decision(&periodic, &state, now, 3_600).expect("due decision");
        assert!(matches!(decision, DueDecision::Run | DueDecision::NotDue));

        let next = next_periodic_due_unix_ms("0/5 * * * * * *", "UTC", now).expect("next due");
        assert!(next >= now);
    }

    #[test]
    fn functional_event_file_lifecycle_and_malformed_skip_behavior() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");

        let event = EventDefinition {
            id: "event-1".to_string(),
            channel: "github/issue-1".to_string(),
            prompt: "summarize issue".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(100),
        };
        write_event(&events_dir.join("event-1.json"), &event);
        std::fs::write(events_dir.join("broken.json"), "{not-json").expect("write malformed");

        let (records, malformed) = load_event_records(&events_dir).expect("load records");
        assert_eq!(records.len(), 1);
        assert_eq!(malformed, 1);
        assert_eq!(records[0].definition.id, "event-1");
    }

    #[test]
    fn unit_events_inspect_report_counts_due_and_schedule_buckets() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");

        let now = 1_700_000_000_000_u64;
        write_event(
            &events_dir.join("immediate.json"),
            &EventDefinition {
                id: "immediate".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "run now".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );
        write_event(
            &events_dir.join("at.json"),
            &EventDefinition {
                id: "at-later".to_string(),
                channel: "github/owner/repo#1".to_string(),
                prompt: "wait".to_string(),
                schedule: EventSchedule::At {
                    at_unix_ms: now.saturating_add(5_000),
                },
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );
        write_event(
            &events_dir.join("periodic-disabled.json"),
            &EventDefinition {
                id: "periodic-disabled".to_string(),
                channel: "github/owner/repo#2".to_string(),
                prompt: "periodic".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "0/1 * * * * * *".to_string(),
                    timezone: "UTC".to_string(),
                },
                enabled: false,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );

        let report = inspect_events(
            &EventsInspectConfig {
                events_dir,
                state_path,
                queue_limit: 1,
                stale_immediate_max_age_seconds: 3_600,
            },
            now,
        )
        .expect("inspect report");

        assert_eq!(report.discovered_events, 3);
        assert_eq!(report.malformed_events, 0);
        assert_eq!(report.enabled_events, 2);
        assert_eq!(report.disabled_events, 1);
        assert_eq!(report.schedule_immediate_events, 1);
        assert_eq!(report.schedule_at_events, 1);
        assert_eq!(report.schedule_periodic_events, 1);
        assert_eq!(report.due_now_events, 1);
        assert_eq!(report.queued_now_events, 1);
        assert_eq!(report.not_due_events, 2);
        assert_eq!(report.stale_immediate_events, 0);
        assert_eq!(report.due_eval_failed_events, 0);
        assert_eq!(report.periodic_with_last_run_state, 0);
        assert_eq!(report.periodic_missing_last_run_state, 1);
    }

    #[test]
    fn functional_events_inspect_render_includes_operator_fields() {
        let rendered = render_events_inspect_report(&super::EventsInspectReport {
            events_dir: "/tmp/events".to_string(),
            state_path: "/tmp/events/state.json".to_string(),
            now_unix_ms: 1_234,
            queue_limit: 8,
            stale_immediate_max_age_seconds: 600,
            discovered_events: 4,
            malformed_events: 1,
            enabled_events: 3,
            disabled_events: 1,
            schedule_immediate_events: 2,
            schedule_at_events: 1,
            schedule_periodic_events: 1,
            due_now_events: 2,
            queued_now_events: 2,
            not_due_events: 1,
            stale_immediate_events: 1,
            due_eval_failed_events: 0,
            periodic_with_last_run_state: 1,
            periodic_missing_last_run_state: 0,
        });

        assert!(rendered.contains("events inspect:"));
        assert!(rendered.contains("events_dir=/tmp/events"));
        assert!(rendered.contains("due_now_events=2"));
        assert!(rendered.contains("queued_now_events=2"));
        assert!(rendered.contains("schedule_periodic_events=1"));
        assert!(rendered.contains("queue_limit=8"));
    }

    #[test]
    fn integration_events_inspect_reads_state_and_applies_queue_limit() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");

        let now = 1_700_000_100_000_u64;
        write_event(
            &events_dir.join("due-a.json"),
            &EventDefinition {
                id: "due-a".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "a".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );
        write_event(
            &events_dir.join("due-b.json"),
            &EventDefinition {
                id: "due-b".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "b".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );
        write_event(
            &events_dir.join("periodic.json"),
            &EventDefinition {
                id: "periodic".to_string(),
                channel: "github/owner/repo#9".to_string(),
                prompt: "periodic".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "0/1 * * * * * *".to_string(),
                    timezone: "UTC".to_string(),
                },
                enabled: false,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );
        std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");

        std::fs::write(
            &state_path,
            r#"{
  "schema_version": 1,
  "periodic_last_run_unix_ms": {
    "periodic": 1700000000000
  },
  "debounce_last_seen_unix_ms": {},
  "signature_replay_last_seen_unix_ms": {}
}
"#,
        )
        .expect("write state");

        let report = inspect_events(
            &EventsInspectConfig {
                events_dir,
                state_path,
                queue_limit: 1,
                stale_immediate_max_age_seconds: 3_600,
            },
            now,
        )
        .expect("inspect report");

        assert_eq!(report.discovered_events, 3);
        assert_eq!(report.malformed_events, 1);
        assert_eq!(report.due_now_events, 2);
        assert_eq!(report.queued_now_events, 1);
        assert_eq!(report.periodic_with_last_run_state, 1);
        assert_eq!(report.periodic_missing_last_run_state, 0);
    }

    #[test]
    fn regression_events_inspect_handles_invalid_periodic_and_missing_state_file() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("missing/state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");

        let now = 1_700_000_200_000_u64;
        write_event(
            &events_dir.join("invalid-periodic.json"),
            &EventDefinition {
                id: "invalid-periodic".to_string(),
                channel: "github/owner/repo#3".to_string(),
                prompt: "periodic".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "invalid-cron".to_string(),
                    timezone: "UTC".to_string(),
                },
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(200)),
            },
        );
        write_event(
            &events_dir.join("stale-immediate.json"),
            &EventDefinition {
                id: "stale-immediate".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "stale".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(120_000)),
            },
        );

        let report = inspect_events(
            &EventsInspectConfig {
                events_dir,
                state_path,
                queue_limit: 8,
                stale_immediate_max_age_seconds: 60,
            },
            now,
        )
        .expect("inspect report");

        assert_eq!(report.discovered_events, 2);
        assert_eq!(report.due_eval_failed_events, 1);
        assert_eq!(report.stale_immediate_events, 1);
        assert_eq!(report.queued_now_events, 0);
    }

    #[test]
    fn unit_validate_events_definitions_classifies_channel_and_schedule_failures() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");

        write_event(
            &events_dir.join("invalid.json"),
            &EventDefinition {
                id: "invalid".to_string(),
                channel: "slack".to_string(),
                prompt: "bad".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "not-a-cron".to_string(),
                    timezone: "UTC".to_string(),
                },
                enabled: true,
                created_unix_ms: Some(1_700_000_000_000),
            },
        );

        let report = validate_events_definitions(
            &EventsValidateConfig {
                events_dir,
                state_path: temp.path().join("state.json"),
            },
            1_700_000_100_000,
        )
        .expect("validate report");

        assert_eq!(report.total_files, 1);
        assert_eq!(report.valid_files, 0);
        assert_eq!(report.invalid_files, 1);
        assert_eq!(report.malformed_files, 0);
        assert_eq!(report.failed_files, 1);
        assert_eq!(report.diagnostics.len(), 2);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.reason_code == "channel_ref_invalid"));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.reason_code == "schedule_invalid"));
    }

    #[test]
    fn functional_render_events_validate_report_includes_summary_and_diagnostics() {
        let rendered = render_events_validate_report(&EventsValidateReport {
            events_dir: "/tmp/events".to_string(),
            state_path: "/tmp/events/state.json".to_string(),
            now_unix_ms: 1_234,
            total_files: 3,
            valid_files: 1,
            invalid_files: 1,
            malformed_files: 1,
            failed_files: 2,
            disabled_files: 1,
            diagnostics: vec![super::EventsValidateDiagnostic {
                path: "/tmp/events/bad.json".to_string(),
                event_id: Some("bad".to_string()),
                reason_code: "schedule_invalid".to_string(),
                message: "invalid cron expression".to_string(),
            }],
        });

        assert!(rendered.contains("events validate:"));
        assert!(rendered.contains("failed_files=2"));
        assert!(rendered.contains("events validate error:"));
        assert!(rendered.contains("reason_code=schedule_invalid"));
    }

    #[test]
    fn integration_validate_events_definitions_reports_mixed_file_health() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let state_path = temp.path().join("state.json");

        write_event(
            &events_dir.join("valid.json"),
            &EventDefinition {
                id: "valid".to_string(),
                channel: "slack/C123".to_string(),
                prompt: "ok".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(1_700_000_000_000),
            },
        );
        write_event(
            &events_dir.join("invalid-periodic.json"),
            &EventDefinition {
                id: "invalid-periodic".to_string(),
                channel: "github/owner/repo#10".to_string(),
                prompt: "bad schedule".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "0/1 * * * * * *".to_string(),
                    timezone: "Not/AZone".to_string(),
                },
                enabled: false,
                created_unix_ms: Some(1_700_000_000_000),
            },
        );
        std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");
        std::fs::write(
            &state_path,
            r#"{
  "schema_version": 1,
  "periodic_last_run_unix_ms": {
    "invalid-periodic": 1700000000000
  },
  "debounce_last_seen_unix_ms": {},
  "signature_replay_last_seen_unix_ms": {}
}
"#,
        )
        .expect("write state");

        let report = validate_events_definitions(
            &EventsValidateConfig {
                events_dir,
                state_path,
            },
            1_700_000_100_000,
        )
        .expect("validate report");

        assert_eq!(report.total_files, 3);
        assert_eq!(report.valid_files, 1);
        assert_eq!(report.invalid_files, 1);
        assert_eq!(report.malformed_files, 1);
        assert_eq!(report.failed_files, 2);
        assert_eq!(report.disabled_files, 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.reason_code == "json_parse"));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.reason_code == "schedule_invalid"));
    }

    #[test]
    fn regression_validate_events_definitions_handles_missing_events_dir() {
        let temp = tempdir().expect("tempdir");
        let report = validate_events_definitions(
            &EventsValidateConfig {
                events_dir: temp.path().join("missing-events"),
                state_path: temp.path().join("missing-state.json"),
            },
            1_700_000_200_000,
        )
        .expect("validate report");

        assert_eq!(report.total_files, 0);
        assert_eq!(report.valid_files, 0);
        assert_eq!(report.invalid_files, 0);
        assert_eq!(report.malformed_files, 0);
        assert_eq!(report.failed_files, 0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn unit_events_template_writer_rejects_invalid_periodic_timezone() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("periodic.json");
        let mut cli = template_cli(&path);
        cli.events_template_schedule = CliEventTemplateSchedule::Periodic;
        cli.events_template_timezone = "".to_string();

        let error = execute_events_template_write_command(&cli)
            .expect_err("empty periodic timezone should fail");
        assert!(error
            .to_string()
            .contains("--events-template-timezone must be non-empty"));
    }

    #[test]
    fn functional_events_template_writer_writes_immediate_defaults() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("immediate.json");
        let cli = template_cli(&path);

        execute_events_template_write_command(&cli).expect("write template");
        let raw = std::fs::read_to_string(&path).expect("read template");
        let parsed: EventDefinition = serde_json::from_str(&raw).expect("parse template");

        assert_eq!(parsed.id, "template-immediate");
        assert_eq!(parsed.channel, "slack/C123");
        assert!(matches!(parsed.schedule, EventSchedule::Immediate));
        assert!(parsed.enabled);
        assert!(parsed.created_unix_ms.is_some());
    }

    #[test]
    fn integration_events_template_periodic_output_passes_validation_pipeline() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let path = events_dir.join("periodic.json");

        let mut cli = template_cli(&path);
        cli.events_template_schedule = CliEventTemplateSchedule::Periodic;
        cli.events_template_cron = Some("0 0/10 * * * * *".to_string());
        cli.events_template_timezone = "UTC".to_string();
        cli.events_template_channel = Some("github/owner/repo#44".to_string());
        cli.events_template_id = Some("deploy-check".to_string());

        execute_events_template_write_command(&cli).expect("write periodic template");
        let report = validate_events_definitions(
            &EventsValidateConfig {
                events_dir,
                state_path: temp.path().join("state.json"),
            },
            super::current_unix_timestamp_ms(),
        )
        .expect("validate");
        assert_eq!(report.total_files, 1);
        assert_eq!(report.valid_files, 1);
        assert_eq!(report.failed_files, 0);
    }

    #[test]
    fn regression_events_template_writer_respects_overwrite_guard() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("template.json");
        std::fs::write(&path, "{\"existing\":true}\n").expect("seed file");

        let cli = template_cli(&path);
        let error = execute_events_template_write_command(&cli)
            .expect_err("existing file should fail without overwrite");
        assert!(error.to_string().contains("template path already exists"));

        let mut overwrite_cli = template_cli(&path);
        overwrite_cli.events_template_overwrite = true;
        execute_events_template_write_command(&overwrite_cli)
            .expect("overwrite should succeed when enabled");
        let raw = std::fs::read_to_string(path).expect("read overwritten");
        assert!(raw.contains("\"id\": \"template-immediate\""));
    }

    #[test]
    fn unit_simulate_events_classifies_due_and_horizon_rows() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let now = 1_700_000_300_000_u64;

        write_event(
            &events_dir.join("immediate.json"),
            &EventDefinition {
                id: "immediate".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "run".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        write_event(
            &events_dir.join("at-future.json"),
            &EventDefinition {
                id: "at-future".to_string(),
                channel: "slack/C2".to_string(),
                prompt: "later".to_string(),
                schedule: EventSchedule::At {
                    at_unix_ms: now.saturating_add(20_000),
                },
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );

        let report = simulate_events(
            &EventsSimulateConfig {
                events_dir,
                state_path: temp.path().join("state.json"),
                horizon_seconds: 30,
                stale_immediate_max_age_seconds: 86_400,
            },
            now,
        )
        .expect("simulate report");
        assert_eq!(report.total_files, 2);
        assert_eq!(report.simulated_rows, 2);
        assert_eq!(report.due_now_rows, 1);
        assert_eq!(report.within_horizon_rows, 2);
        assert_eq!(report.invalid_rows, 0);
        assert_eq!(report.malformed_files, 0);
    }

    #[test]
    fn functional_render_events_simulate_report_contains_summary_and_rows() {
        let report = super::EventsSimulateReport {
            events_dir: "/tmp/events".to_string(),
            state_path: "/tmp/state.json".to_string(),
            now_unix_ms: 123,
            horizon_seconds: 60,
            total_files: 1,
            simulated_rows: 1,
            malformed_files: 0,
            invalid_rows: 0,
            due_now_rows: 1,
            within_horizon_rows: 1,
            rows: vec![super::EventsSimulateRow {
                path: "/tmp/events/a.json".to_string(),
                event_id: "evt-1".to_string(),
                channel: "slack/C1".to_string(),
                schedule: "immediate".to_string(),
                enabled: true,
                next_due_unix_ms: Some(123),
                due_now: true,
                within_horizon: true,
                last_run_unix_ms: None,
            }],
            diagnostics: Vec::new(),
        };

        let rendered = render_events_simulate_report(&report);
        assert!(rendered.contains("events simulate:"));
        assert!(rendered.contains("horizon_seconds=60"));
        assert!(rendered.contains("events simulate row:"));
        assert!(rendered.contains("event_id=evt-1"));
    }

    #[test]
    fn integration_simulate_events_mixed_schedules_with_state_replay() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let now = 1_700_000_400_000_u64;

        write_event(
            &events_dir.join("periodic.json"),
            &EventDefinition {
                id: "periodic".to_string(),
                channel: "github/owner/repo#1".to_string(),
                prompt: "periodic".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "0/1 * * * * * *".to_string(),
                    timezone: "UTC".to_string(),
                },
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        write_event(
            &events_dir.join("disabled-at.json"),
            &EventDefinition {
                id: "disabled-at".to_string(),
                channel: "slack/C3".to_string(),
                prompt: "disabled".to_string(),
                schedule: EventSchedule::At {
                    at_unix_ms: now.saturating_add(120_000),
                },
                enabled: false,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        std::fs::write(
            &state_path,
            r#"{
  "schema_version": 1,
  "periodic_last_run_unix_ms": {
    "periodic": 1700000390000
  },
  "debounce_last_seen_unix_ms": {},
  "signature_replay_last_seen_unix_ms": {}
}
"#,
        )
        .expect("write state");

        let report = simulate_events(
            &EventsSimulateConfig {
                events_dir,
                state_path,
                horizon_seconds: 300,
                stale_immediate_max_age_seconds: 86_400,
            },
            now,
        )
        .expect("simulate report");
        assert_eq!(report.total_files, 2);
        assert_eq!(report.simulated_rows, 2);
        assert_eq!(report.malformed_files, 0);
        assert_eq!(report.invalid_rows, 0);
        assert_eq!(report.rows.iter().filter(|row| row.enabled).count(), 1);
    }

    #[test]
    fn regression_simulate_events_reports_malformed_and_invalid_entries() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let now = 1_700_000_500_000_u64;

        write_event(
            &events_dir.join("invalid-channel.json"),
            &EventDefinition {
                id: "invalid-channel".to_string(),
                channel: "slack".to_string(),
                prompt: "bad".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");

        let report = simulate_events(
            &EventsSimulateConfig {
                events_dir,
                state_path: temp.path().join("state.json"),
                horizon_seconds: 60,
                stale_immediate_max_age_seconds: 86_400,
            },
            now,
        )
        .expect("simulate report");

        assert_eq!(report.total_files, 2);
        assert_eq!(report.simulated_rows, 0);
        assert_eq!(report.malformed_files, 1);
        assert_eq!(report.invalid_rows, 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.reason_code == "channel_ref_invalid"));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.reason_code == "json_parse"));
    }

    #[test]
    fn unit_dry_run_events_applies_queue_limit_and_decisions() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let now = 1_700_000_600_000_u64;

        write_event(
            &events_dir.join("disabled.json"),
            &EventDefinition {
                id: "a-disabled".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "disabled".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: false,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        write_event(
            &events_dir.join("due-a.json"),
            &EventDefinition {
                id: "b-due".to_string(),
                channel: "slack/C2".to_string(),
                prompt: "due".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        write_event(
            &events_dir.join("due-b.json"),
            &EventDefinition {
                id: "c-due".to_string(),
                channel: "slack/C3".to_string(),
                prompt: "due-too".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );

        let report = dry_run_events(
            &EventsDryRunConfig {
                events_dir,
                state_path: temp.path().join("state.json"),
                queue_limit: 1,
                stale_immediate_max_age_seconds: 86_400,
            },
            now,
        )
        .expect("dry run report");

        assert_eq!(report.total_files, 3);
        assert_eq!(report.evaluated_rows, 3);
        assert_eq!(report.execute_rows, 1);
        assert_eq!(report.skipped_rows, 2);
        assert_eq!(report.error_rows, 0);
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "not_due" && row.decision == "skip"));
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "due_now" && row.decision == "execute"));
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "queue_limit_reached" && row.decision == "skip"));
        assert_eq!(
            report
                .rows
                .iter()
                .find(|row| row.reason_code == "due_now")
                .and_then(|row| row.queue_position),
            Some(1)
        );
    }

    #[test]
    fn functional_render_events_dry_run_report_contains_summary_and_rows() {
        let report = super::EventsDryRunReport {
            events_dir: "/tmp/events".to_string(),
            state_path: "/tmp/state.json".to_string(),
            now_unix_ms: 123,
            queue_limit: 2,
            total_files: 1,
            evaluated_rows: 1,
            execute_rows: 1,
            skipped_rows: 0,
            error_rows: 0,
            malformed_files: 0,
            rows: vec![super::EventsDryRunRow {
                path: "/tmp/events/a.json".to_string(),
                event_id: Some("evt-1".to_string()),
                channel: Some("slack/C1".to_string()),
                schedule: Some("immediate".to_string()),
                enabled: Some(true),
                decision: "execute".to_string(),
                reason_code: "due_now".to_string(),
                queue_position: Some(1),
                last_run_unix_ms: None,
                message: None,
            }],
        };

        let rendered = render_events_dry_run_report(&report);
        assert!(rendered.contains("events dry run:"));
        assert!(rendered.contains("queue_limit=2"));
        assert!(rendered.contains("events dry run row:"));
        assert!(rendered.contains("decision=execute"));
    }

    #[test]
    fn integration_dry_run_events_is_read_only_for_event_files_and_state() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let now = 1_700_000_700_000_u64;

        let stale_path = events_dir.join("stale.json");
        write_event(
            &stale_path,
            &EventDefinition {
                id: "stale-immediate".to_string(),
                channel: "slack/C1".to_string(),
                prompt: "stale".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(3_600_000)),
            },
        );
        let stale_before = std::fs::read_to_string(&stale_path).expect("read stale before");

        let report = dry_run_events(
            &EventsDryRunConfig {
                events_dir,
                state_path: state_path.clone(),
                queue_limit: 8,
                stale_immediate_max_age_seconds: 60,
            },
            now,
        )
        .expect("dry run report");

        assert!(stale_path.exists());
        let stale_after = std::fs::read_to_string(&stale_path).expect("read stale after");
        assert_eq!(stale_before, stale_after);
        assert!(!state_path.exists());
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "stale_immediate"));
    }

    #[test]
    fn regression_dry_run_events_reports_malformed_and_invalid_entries() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let now = 1_700_000_800_000_u64;

        write_event(
            &events_dir.join("invalid-channel.json"),
            &EventDefinition {
                id: "invalid-channel".to_string(),
                channel: "slack".to_string(),
                prompt: "bad channel".to_string(),
                schedule: EventSchedule::Immediate,
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        write_event(
            &events_dir.join("invalid-schedule.json"),
            &EventDefinition {
                id: "invalid-schedule".to_string(),
                channel: "slack/C2".to_string(),
                prompt: "bad schedule".to_string(),
                schedule: EventSchedule::Periodic {
                    cron: "not-a-cron".to_string(),
                    timezone: "UTC".to_string(),
                },
                enabled: true,
                created_unix_ms: Some(now.saturating_sub(100)),
            },
        );
        std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");

        let report = dry_run_events(
            &EventsDryRunConfig {
                events_dir,
                state_path: temp.path().join("state.json"),
                queue_limit: 8,
                stale_immediate_max_age_seconds: 86_400,
            },
            now,
        )
        .expect("dry run report");

        assert_eq!(report.total_files, 3);
        assert_eq!(report.evaluated_rows, 3);
        assert_eq!(report.error_rows, 3);
        assert_eq!(report.malformed_files, 1);
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "json_parse" && row.decision == "error"));
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "channel_ref_invalid" && row.decision == "error"));
        assert!(report
            .rows
            .iter()
            .any(|row| row.reason_code == "schedule_invalid" && row.decision == "error"));
    }

    #[tokio::test]
    async fn integration_scheduled_event_executes_into_channel_store() {
        let temp = tempdir().expect("tempdir");
        let config = scheduler_config(temp.path());
        std::fs::create_dir_all(&config.events_dir).expect("create events dir");

        let event = EventDefinition {
            id: "run-now".to_string(),
            channel: "slack/C123".to_string(),
            prompt: "say hello".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(super::current_unix_timestamp_ms()),
        };
        write_event(&config.events_dir.join("run-now.json"), &event);

        let now = super::current_unix_timestamp_ms();
        let mut runtime = EventSchedulerRuntime::new(config.clone()).expect("runtime");
        let report = runtime.poll_once(now).await.expect("poll once");
        assert_eq!(report.executed, 1);

        let channel_log = std::fs::read_to_string(
            config
                .channel_store_root
                .join("channels/slack/C123/log.jsonl"),
        )
        .expect("channel log");
        let channel_context = std::fs::read_to_string(
            config
                .channel_store_root
                .join("channels/slack/C123/context.jsonl"),
        )
        .expect("channel context");
        assert!(channel_log.contains("\"source\":\"events\""));
        assert!(channel_context.contains("scheduled reply"));
        assert!(!config.events_dir.join("run-now.json").exists());
    }

    #[tokio::test]
    async fn integration_restart_recovery_runs_due_oneshot_and_keeps_periodic() {
        let temp = tempdir().expect("tempdir");
        let config = scheduler_config(temp.path());
        std::fs::create_dir_all(&config.events_dir).expect("create events dir");

        let now = super::current_unix_timestamp_ms();
        let at_event = EventDefinition {
            id: "oneshot".to_string(),
            channel: "github/issue-7".to_string(),
            prompt: "at event".to_string(),
            schedule: EventSchedule::At {
                at_unix_ms: now.saturating_sub(1_000),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(2_000)),
        };
        let periodic_event = EventDefinition {
            id: "periodic".to_string(),
            channel: "github/issue-7".to_string(),
            prompt: "periodic event".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "0/1 * * * * * *".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(2_000)),
        };
        write_event(&config.events_dir.join("oneshot.json"), &at_event);
        write_event(&config.events_dir.join("periodic.json"), &periodic_event);

        let mut runtime = EventSchedulerRuntime::new(config.clone()).expect("runtime");
        let first = runtime.poll_once(now).await.expect("first poll");
        assert!(first.executed >= 1);

        let mut runtime_after_restart =
            EventSchedulerRuntime::new(config.clone()).expect("restart runtime");
        let second = runtime_after_restart
            .poll_once(now.saturating_add(2_000))
            .await
            .expect("second poll");
        assert!(second.executed >= 1);
        assert!(!config.events_dir.join("oneshot.json").exists());
        assert!(config.events_dir.join("periodic.json").exists());
    }

    #[tokio::test]
    async fn regression_stale_immediate_events_are_ignored_and_removed() {
        let temp = tempdir().expect("tempdir");
        let mut config = scheduler_config(temp.path());
        config.stale_immediate_max_age_seconds = 1;
        std::fs::create_dir_all(&config.events_dir).expect("create events dir");

        let now = super::current_unix_timestamp_ms();
        let stale = EventDefinition {
            id: "stale-immediate".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "stale".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(10_000)),
        };
        write_event(&config.events_dir.join("stale.json"), &stale);

        let mut runtime = EventSchedulerRuntime::new(config.clone()).expect("runtime");
        let report = runtime.poll_once(now).await.expect("poll");
        assert_eq!(report.executed, 0);
        assert_eq!(report.stale_skipped, 1);
        assert!(!config.events_dir.join("stale.json").exists());
    }

    #[test]
    fn regression_webhook_ingest_debounces_and_writes_immediate_event() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("events/state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let payload_path = temp.path().join("payload.json");
        std::fs::write(&payload_path, "{\"signal\":\"high\"}").expect("write payload");

        let config = EventWebhookIngestConfig {
            events_dir: events_dir.clone(),
            state_path: state_path.clone(),
            channel_ref: "slack/C123".to_string(),
            payload_file: payload_path.clone(),
            prompt_prefix: "Handle incoming webhook".to_string(),
            debounce_key: Some("hook-A".to_string()),
            debounce_window_seconds: 60,
            signature: None,
            timestamp: None,
            secret: None,
            signature_algorithm: None,
            signature_max_skew_seconds: 300,
        };

        ingest_webhook_immediate_event(&config).expect("first ingest");
        let first_count = std::fs::read_dir(&events_dir)
            .expect("read dir first")
            .count();
        ingest_webhook_immediate_event(&config).expect("second ingest debounced");
        let second_count = std::fs::read_dir(&events_dir)
            .expect("read dir second")
            .count();

        assert_eq!(first_count, second_count);
        assert!(state_path.exists());
    }

    #[test]
    fn unit_webhook_signature_github_sha256_and_slack_v0_are_verified() {
        let payload = "{\"signal\":\"ok\"}";
        let github_secret = "github-secret";
        let github_sig = github_signature(github_secret, payload);
        let github_result = super::verify_webhook_signature(
            payload,
            Some(&github_sig),
            None,
            Some(github_secret),
            Some(WebhookSignatureAlgorithm::GithubSha256),
            1_700_000_000_000,
            300,
        )
        .expect("github signature");
        assert!(github_result.is_some());

        let slack_secret = "slack-secret";
        let slack_ts = "1700000000";
        let slack_sig = slack_v0_signature(slack_secret, slack_ts, payload);
        let slack_result = super::verify_webhook_signature(
            payload,
            Some(&slack_sig),
            Some(slack_ts),
            Some(slack_secret),
            Some(WebhookSignatureAlgorithm::SlackV0),
            1_700_000_050_000,
            600,
        )
        .expect("slack signature");
        assert!(slack_result.is_some());
    }

    #[test]
    fn functional_webhook_ingest_accepts_valid_signed_payload() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let payload_path = temp.path().join("payload.json");
        let payload = "{\"event\":\"deploy\"}";
        std::fs::write(&payload_path, payload).expect("write payload");

        let secret = "github-secret";
        let signature = github_signature(secret, payload);
        let config = EventWebhookIngestConfig {
            events_dir: events_dir.clone(),
            state_path: state_path.clone(),
            channel_ref: "github/owner/repo#10".to_string(),
            payload_file: payload_path,
            prompt_prefix: "Handle incoming webhook".to_string(),
            debounce_key: None,
            debounce_window_seconds: 60,
            signature: Some(signature),
            timestamp: None,
            secret: Some(secret.to_string()),
            signature_algorithm: Some(WebhookSignatureAlgorithm::GithubSha256),
            signature_max_skew_seconds: 300,
        };

        ingest_webhook_immediate_event(&config).expect("signed ingest");
        let count = std::fs::read_dir(&events_dir)
            .expect("read events dir")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn integration_webhook_ingest_rejects_replay_signature_within_window() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let payload_path = temp.path().join("payload.json");
        let payload = "{\"event\":\"sync\"}";
        std::fs::write(&payload_path, payload).expect("write payload");

        let secret = "slack-secret";
        let timestamp = format!("{}", super::current_unix_timestamp_ms() / 1_000);
        let signature = slack_v0_signature(secret, &timestamp, payload);
        let config = EventWebhookIngestConfig {
            events_dir: events_dir.clone(),
            state_path: state_path.clone(),
            channel_ref: "slack/C123".to_string(),
            payload_file: payload_path,
            prompt_prefix: "Handle incoming webhook".to_string(),
            debounce_key: None,
            debounce_window_seconds: 60,
            signature: Some(signature),
            timestamp: Some(timestamp),
            secret: Some(secret.to_string()),
            signature_algorithm: Some(WebhookSignatureAlgorithm::SlackV0),
            signature_max_skew_seconds: 300,
        };

        ingest_webhook_immediate_event(&config).expect("first ingest");
        let error = ingest_webhook_immediate_event(&config).expect_err("replay should fail");
        assert!(error.to_string().contains("replay"));
    }

    #[test]
    fn regression_webhook_ingest_invalid_signature_is_rejected_without_event_write() {
        let temp = tempdir().expect("tempdir");
        let events_dir = temp.path().join("events");
        let state_path = temp.path().join("state.json");
        std::fs::create_dir_all(&events_dir).expect("create events dir");
        let payload_path = temp.path().join("payload.json");
        std::fs::write(&payload_path, "{\"signal\":\"bad\"}").expect("write payload");

        let config = EventWebhookIngestConfig {
            events_dir: events_dir.clone(),
            state_path,
            channel_ref: "slack/C123".to_string(),
            payload_file: payload_path,
            prompt_prefix: "Handle incoming webhook".to_string(),
            debounce_key: None,
            debounce_window_seconds: 60,
            signature: Some("sha256=deadbeef".to_string()),
            timestamp: None,
            secret: Some("secret".to_string()),
            signature_algorithm: Some(WebhookSignatureAlgorithm::GithubSha256),
            signature_max_skew_seconds: 300,
        };

        let error = ingest_webhook_immediate_event(&config).expect_err("signature should fail");
        assert!(error.to_string().contains("verification failed"));
        let count = std::fs::read_dir(&events_dir)
            .expect("read events dir")
            .count();
        assert_eq!(count, 0);
    }
}
