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
    PromptRunStatus, RenderOptions, SessionRuntime,
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
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tempfile::tempdir;

    use super::{
        due_decision, ingest_webhook_immediate_event, inspect_events, load_event_records,
        next_periodic_due_unix_ms, render_events_inspect_report, DueDecision, EventDefinition,
        EventRunnerState, EventSchedule, EventSchedulerConfig, EventSchedulerRuntime,
        EventWebhookIngestConfig, EventsInspectConfig, WebhookSignatureAlgorithm,
    };
    use crate::{tools::ToolPolicy, RenderOptions};

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
