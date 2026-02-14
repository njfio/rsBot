//! Event scheduling, templates, and runtime execution primitives for Tau.
//!
//! Defines event manifests plus scheduler/runner plumbing used by autonomous
//! event-driven prompt execution in channel-store runtimes.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use chrono::TimeZone;
use chrono_tz::Tz;
use cron::Schedule;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tau_core::{current_unix_timestamp_ms, write_text_atomic};
use tau_runtime::channel_store::{ChannelLogEntry, ChannelStore};

mod events_cli_commands;
pub use events_cli_commands::*;

const EVENT_RUNNER_STATE_SCHEMA_VERSION: u32 = 1;

#[async_trait]
/// Trait contract for `EventRunner` behavior.
pub trait EventRunner: Send + Sync {
    async fn run_event(
        &self,
        event: &EventDefinition,
        now_unix_ms: u64,
        channel_store: &ChannelStore,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `EventTemplateSchedule` values.
pub enum EventTemplateSchedule {
    Immediate,
    At,
    Periodic,
}

#[derive(Clone)]
/// Public struct `EventSchedulerConfig` used across Tau components.
pub struct EventSchedulerConfig {
    pub runner: std::sync::Arc<dyn EventRunner>,
    pub channel_store_root: PathBuf,
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub poll_interval: Duration,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone)]
/// Public struct `EventWebhookIngestConfig` used across Tau components.
pub struct EventWebhookIngestConfig {
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
/// Enumerates supported `WebhookSignatureAlgorithm` values.
pub enum WebhookSignatureAlgorithm {
    GithubSha256,
    SlackV0,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Enumerates supported `EventSchedule` values.
pub enum EventSchedule {
    Immediate,
    At { at_unix_ms: u64 },
    Periodic { cron: String, timezone: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Public struct `EventDefinition` used across Tau components.
pub struct EventDefinition {
    pub id: String,
    pub channel: String,
    pub prompt: String,
    pub schedule: EventSchedule,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub created_unix_ms: Option<u64>,
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
/// Public struct `EventsInspectConfig` used across Tau components.
pub struct EventsInspectConfig {
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `EventsInspectReport` used across Tau components.
pub struct EventsInspectReport {
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
/// Public struct `EventsValidateConfig` used across Tau components.
pub struct EventsValidateConfig {
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `EventsValidateDiagnostic` used across Tau components.
pub struct EventsValidateDiagnostic {
    pub path: String,
    pub event_id: Option<String>,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `EventsValidateReport` used across Tau components.
pub struct EventsValidateReport {
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
/// Public struct `EventsSimulateConfig` used across Tau components.
pub struct EventsSimulateConfig {
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub horizon_seconds: u64,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone)]
/// Public struct `EventsDryRunConfig` used across Tau components.
pub struct EventsDryRunConfig {
    pub events_dir: PathBuf,
    pub state_path: PathBuf,
    pub queue_limit: usize,
    pub stale_immediate_max_age_seconds: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Public struct `EventsDryRunGateConfig` used across Tau components.
pub struct EventsDryRunGateConfig {
    pub max_error_rows: Option<usize>,
    pub max_execute_rows: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `EventsDryRunGateOutcome` used across Tau components.
pub struct EventsDryRunGateOutcome {
    status: &'static str,
    reason_codes: Vec<String>,
    execute_rows: usize,
    skipped_rows: usize,
    error_rows: usize,
    max_error_rows: Option<usize>,
    max_execute_rows: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
/// Public struct `EventsSimulateRow` used across Tau components.
pub struct EventsSimulateRow {
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
/// Public struct `EventsSimulateReport` used across Tau components.
pub struct EventsSimulateReport {
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
/// Public struct `EventsDryRunRow` used across Tau components.
pub struct EventsDryRunRow {
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
/// Public struct `EventsDryRunReport` used across Tau components.
pub struct EventsDryRunReport {
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

#[derive(Debug, Clone)]
/// Public struct `EventsTemplateConfig` used across Tau components.
pub struct EventsTemplateConfig {
    pub target_path: PathBuf,
    pub overwrite: bool,
    pub schedule: EventTemplateSchedule,
    pub channel: String,
    pub prompt: String,
    pub event_id: Option<String>,
    pub at_unix_ms: Option<u64>,
    pub cron: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Public struct `EventsTemplateWriteReport` used across Tau components.
pub struct EventsTemplateWriteReport {
    pub path: PathBuf,
    pub schedule: String,
    pub event_id: String,
    pub channel: String,
    pub overwrite: bool,
}

pub fn write_event_template(
    config: &EventsTemplateConfig,
    now_unix_ms: u64,
) -> Result<EventsTemplateWriteReport> {
    if config.target_path.exists() && !config.overwrite {
        bail!(
            "template path already exists (use --events-template-overwrite=true): {}",
            config.target_path.display()
        );
    }

    let channel = config.channel.trim();
    if channel.is_empty() {
        bail!("events template channel must be non-empty");
    }
    ChannelStore::parse_channel_ref(channel)?;

    let schedule = match config.schedule {
        EventTemplateSchedule::Immediate => EventSchedule::Immediate,
        EventTemplateSchedule::At => {
            let at_unix_ms = config
                .at_unix_ms
                .ok_or_else(|| anyhow!("events template requires at_unix_ms for schedule=at"))?;
            EventSchedule::At { at_unix_ms }
        }
        EventTemplateSchedule::Periodic => {
            let cron = config
                .cron
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("events template requires cron for schedule=periodic"))?;
            let timezone = config
                .timezone
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    anyhow!("events template requires timezone for schedule=periodic")
                })?;
            let _ =
                next_periodic_due_unix_ms(&cron, &timezone, now_unix_ms.saturating_sub(60_000))?;
            EventSchedule::Periodic { cron, timezone }
        }
    };

    let event_id = config
        .event_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| match &schedule {
            EventSchedule::Immediate => "template-immediate".to_string(),
            EventSchedule::At { .. } => "template-at".to_string(),
            EventSchedule::Periodic { .. } => "template-periodic".to_string(),
        });

    let prompt = {
        let trimmed = config.prompt.trim();
        if trimmed.is_empty() {
            "Summarize current context and propose the next best action.".to_string()
        } else {
            trimmed.to_string()
        }
    };

    let template = EventDefinition {
        id: event_id.clone(),
        channel: channel.to_string(),
        prompt,
        schedule,
        enabled: true,
        created_unix_ms: Some(now_unix_ms),
    };

    if let Some(parent) = config.target_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut payload =
        serde_json::to_string_pretty(&template).context("failed to serialize event template")?;
    payload.push('\n');
    write_text_atomic(&config.target_path, &payload)
        .with_context(|| format!("failed to write {}", config.target_path.display()))?;

    Ok(EventsTemplateWriteReport {
        path: config.target_path.clone(),
        schedule: schedule_name(&template.schedule).to_string(),
        event_id,
        channel: template.channel,
        overwrite: config.overwrite,
    })
}

pub async fn run_event_scheduler(config: EventSchedulerConfig) -> Result<()> {
    let mut runtime = EventSchedulerRuntime::new(config)?;
    runtime.run().await
}

pub fn ingest_webhook_immediate_event(config: &EventWebhookIngestConfig) -> Result<()> {
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

    config
        .runner
        .run_event(event, now_unix_ms, &channel_store)
        .await
}

pub fn inspect_events(
    config: &EventsInspectConfig,
    now_unix_ms: u64,
) -> Result<EventsInspectReport> {
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

#[cfg(test)]
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

pub fn validate_events_definitions(
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

#[cfg(test)]
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

pub fn simulate_events(
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

#[cfg(test)]
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

pub fn dry_run_events(config: &EventsDryRunConfig, now_unix_ms: u64) -> Result<EventsDryRunReport> {
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

#[cfg(test)]
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

fn evaluate_events_dry_run_gate(
    report: &EventsDryRunReport,
    config: &EventsDryRunGateConfig,
) -> EventsDryRunGateOutcome {
    let mut reason_codes = Vec::new();

    if let Some(max_error_rows) = config.max_error_rows {
        if report.error_rows > max_error_rows {
            reason_codes.push("max_error_rows_exceeded".to_string());
        }
    }
    if let Some(max_execute_rows) = config.max_execute_rows {
        if report.execute_rows > max_execute_rows {
            reason_codes.push("max_execute_rows_exceeded".to_string());
        }
    }

    let status = if reason_codes.is_empty() {
        "pass"
    } else {
        "fail"
    };
    EventsDryRunGateOutcome {
        status,
        reason_codes,
        execute_rows: report.execute_rows,
        skipped_rows: report.skipped_rows,
        error_rows: report.error_rows,
        max_error_rows: config.max_error_rows,
        max_execute_rows: config.max_execute_rows,
    }
}

fn render_events_dry_run_gate_summary(outcome: &EventsDryRunGateOutcome) -> String {
    format!(
        "events dry run gate: status={} reason_codes={} execute_rows={} skipped_rows={} error_rows={} max_error_rows={} max_execute_rows={}",
        outcome.status,
        if outcome.reason_codes.is_empty() {
            "none".to_string()
        } else {
            outcome.reason_codes.join(",")
        },
        outcome.execute_rows,
        outcome.skipped_rows,
        outcome.error_rows,
        outcome
            .max_error_rows
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        outcome
            .max_execute_rows
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
    )
}

pub fn enforce_events_dry_run_gate(
    report: &EventsDryRunReport,
    config: &EventsDryRunGateConfig,
) -> Result<()> {
    let outcome = evaluate_events_dry_run_gate(report, config);
    let summary = render_events_dry_run_gate_summary(&outcome);
    eprintln!("{summary}");
    if outcome.status == "fail" {
        bail!("{summary}");
    }
    Ok(())
}

#[cfg(test)]
fn enforce_events_dry_run_strict_mode(report: &EventsDryRunReport, strict: bool) -> Result<()> {
    let config = EventsDryRunGateConfig {
        max_error_rows: if strict { Some(0) } else { None },
        max_execute_rows: None,
    };
    enforce_events_dry_run_gate(report, &config)
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
mod tests;
