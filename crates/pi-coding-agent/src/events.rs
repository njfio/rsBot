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
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::{LlmClient, Message, MessageRole};
use serde::{Deserialize, Serialize};

use crate::{
    channel_store::{ChannelLogEntry, ChannelStore},
    current_unix_timestamp_ms, run_prompt_with_cancellation, write_text_atomic, PromptRunStatus,
    RenderOptions, SessionRuntime,
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
}

impl Default for EventRunnerState {
    fn default() -> Self {
        Self {
            schema_version: EVENT_RUNNER_STATE_SCHEMA_VERSION,
            periodic_last_run_unix_ms: HashMap::new(),
            debounce_last_seen_unix_ms: HashMap::new(),
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

pub(crate) async fn run_event_scheduler(config: EventSchedulerConfig) -> Result<()> {
    let mut runtime = EventSchedulerRuntime::new(config)?;
    runtime.run().await
}

pub(crate) fn ingest_webhook_immediate_event(config: &EventWebhookIngestConfig) -> Result<()> {
    std::fs::create_dir_all(&config.events_dir)
        .with_context(|| format!("failed to create {}", config.events_dir.display()))?;
    let mut state = load_runner_state(&config.state_path)?;

    let now_unix_ms = current_unix_timestamp_ms();
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

    let payload = std::fs::read_to_string(&config.payload_file)
        .with_context(|| format!("failed to read {}", config.payload_file.display()))?;
    let payload = payload.trim();
    if payload.is_empty() {
        bail!(
            "webhook payload file is empty: {}",
            config.payload_file.display()
        );
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
    use pi_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, PiAiError};
    use tempfile::tempdir;

    use super::{
        due_decision, ingest_webhook_immediate_event, load_event_records,
        next_periodic_due_unix_ms, DueDecision, EventDefinition, EventRunnerState, EventSchedule,
        EventSchedulerConfig, EventSchedulerRuntime, EventWebhookIngestConfig,
    };
    use crate::{tools::ToolPolicy, RenderOptions};

    struct StaticReplyClient;

    #[async_trait]
    impl LlmClient for StaticReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
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
}
