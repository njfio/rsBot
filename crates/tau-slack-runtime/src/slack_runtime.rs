//! Slack bridge runtime that polls events and orchestrates agent responses.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::LlmClient;
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use crate::channel_store::{ChannelArtifactRecord, ChannelLogEntry, ChannelStore};
use crate::slack_helpers::{
    is_retryable_slack_status, is_retryable_transport_error, parse_retry_after, retry_delay,
    sanitize_for_path, truncate_for_error, truncate_for_slack,
};
use crate::tools::ToolPolicy;
use crate::{
    authorize_action_for_principal_with_policy_path, current_unix_timestamp_ms,
    evaluate_pairing_access, execute_canvas_command, pairing_policy_for_state_dir,
    rbac_policy_path_for_state_dir, run_prompt_with_cancellation, slack_principal,
    write_text_atomic, CanvasCommandConfig, CanvasEventOrigin, CanvasSessionLinkContext,
    PairingDecision, PromptRunStatus, RbacDecision, RenderOptions, SessionRuntime,
    TransportHealthSnapshot,
};
use tau_session::SessionStore;

const SLACK_STATE_SCHEMA_VERSION: u32 = 1;
const SLACK_METADATA_MARKER_PREFIX: &str = "<!-- tau-slack-event:";
const SLACK_METADATA_MARKER_SUFFIX: &str = " -->";

#[derive(Clone)]
/// Runtime configuration for the Slack bridge transport loop.
pub struct SlackBridgeRuntimeConfig {
    pub client: Arc<dyn LlmClient>,
    pub model: String,
    pub system_prompt: String,
    pub max_turns: usize,
    pub tool_policy: ToolPolicy,
    pub turn_timeout_ms: u64,
    pub request_timeout_ms: u64,
    pub render_options: RenderOptions,
    pub session_lock_wait_ms: u64,
    pub session_lock_stale_ms: u64,
    pub state_dir: PathBuf,
    pub api_base: String,
    pub app_token: String,
    pub bot_token: String,
    pub bot_user_id: Option<String>,
    pub detail_thread_output: bool,
    pub detail_thread_threshold_chars: usize,
    pub processed_event_cap: usize,
    pub max_event_age_seconds: u64,
    pub coalescing_window_ms: u64,
    pub reconnect_delay: Duration,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub artifact_retention_days: u64,
}

mod slack_api_client;
mod slack_command_helpers;
mod slack_render_helpers;
mod slack_state_store;

use slack_api_client::{SlackApiClient, SlackPostedMessage};
use slack_command_helpers::{
    parse_slack_command, rbac_action_for_slack_command, render_slack_command_response,
    slack_command_usage,
};
use slack_render_helpers::{
    collect_assistant_reply, normalize_artifact_retention_days, prompt_status_label,
    render_event_prompt, render_slack_artifact_markdown, render_slack_response,
    render_slack_run_error_message, slack_metadata_marker,
};
use slack_state_store::{JsonlEventLog, SlackBridgeStateStore};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SlackSocketEnvelope {
    envelope_id: String,
    #[serde(rename = "type")]
    envelope_type: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SlackFileAttachment {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    mimetype: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    url_private_download: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum SlackBridgeEventKind {
    AppMention,
    DirectMessage,
}

impl SlackBridgeEventKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::AppMention => "app_mention",
            Self::DirectMessage => "message.im",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackBridgeEvent {
    key: String,
    kind: SlackBridgeEventKind,
    event_id: String,
    occurred_unix_ms: u64,
    channel_id: String,
    user_id: String,
    text: String,
    ts: String,
    thread_ts: Option<String>,
    files: Vec<SlackFileAttachment>,
    raw_payload: Value,
}

impl SlackBridgeEvent {
    fn reply_thread_ts(&self) -> Option<&str> {
        self.thread_ts.as_deref().or(Some(self.ts.as_str()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlackCommand {
    Help,
    Status,
    Health,
    Stop,
    Artifacts { purge: bool, run_id: Option<String> },
    ArtifactShow { artifact_id: String },
    Canvas { args: String },
    Invalid { message: String },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct PromptUsageSummary {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    request_duration_ms: u64,
    finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct PromptRunReport {
    run_id: String,
    model: String,
    status: PromptRunStatus,
    assistant_reply: String,
    usage: PromptUsageSummary,
    downloaded_files: Vec<DownloadedSlackFile>,
    artifact: ChannelArtifactRecord,
}

#[derive(Debug, Clone)]
struct DownloadedSlackFile {
    id: String,
    original_name: String,
    path: PathBuf,
    mimetype: Option<String>,
    size: Option<u64>,
}

#[derive(Debug)]
struct ActiveChannelRun {
    run_id: String,
    event_key: String,
    started_unix_ms: u64,
    started: Instant,
    cancel_tx: watch::Sender<bool>,
    handle: tokio::task::JoinHandle<RunTaskResult>,
}

#[derive(Debug, Clone)]
struct SlackLatestRun {
    run_id: String,
    event_key: String,
    status: String,
    started_unix_ms: u64,
    completed_unix_ms: u64,
    duration_ms: u64,
}

#[derive(Debug)]
struct RunTaskResult {
    channel_id: String,
    event_key: String,
    run_id: String,
    started_unix_ms: u64,
    completed_unix_ms: u64,
    duration_ms: u64,
    status: String,
    model: String,
    usage: PromptUsageSummary,
    error: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct PollCycleReport {
    pub discovered_events: usize,
    pub queued_events: usize,
    pub completed_runs: usize,
    pub skipped_duplicate_events: usize,
    pub skipped_stale_events: usize,
    pub failed_events: usize,
}

/// Runs the Slack bridge transport loop and processes incoming events.
pub async fn run_slack_bridge(config: SlackBridgeRuntimeConfig) -> Result<()> {
    let mut runtime = SlackBridgeRuntime::new(config).await?;
    runtime.run().await
}

struct SlackBridgeRuntime {
    config: SlackBridgeRuntimeConfig,
    slack_client: SlackApiClient,
    state_store: SlackBridgeStateStore,
    inbound_log: JsonlEventLog,
    outbound_log: JsonlEventLog,
    bot_user_id: String,
    state_dir: PathBuf,
    active_runs: HashMap<String, ActiveChannelRun>,
    latest_runs: HashMap<String, SlackLatestRun>,
    channel_queues: HashMap<String, VecDeque<SlackBridgeEvent>>,
}

impl SlackBridgeRuntime {
    async fn new(config: SlackBridgeRuntimeConfig) -> Result<Self> {
        let state_dir = config.state_dir.clone();
        std::fs::create_dir_all(&state_dir)
            .with_context(|| format!("failed to create {}", state_dir.display()))?;

        let slack_client = SlackApiClient::new(
            config.api_base.clone(),
            config.app_token.clone(),
            config.bot_token.clone(),
            config.request_timeout_ms,
            config.retry_max_attempts,
            config.retry_base_delay_ms,
        )?;

        let bot_user_id = match config.bot_user_id.clone() {
            Some(user_id) if !user_id.trim().is_empty() => user_id.trim().to_string(),
            _ => slack_client.resolve_bot_user_id().await?,
        };

        let state_store =
            SlackBridgeStateStore::load(state_dir.join("state.json"), config.processed_event_cap)?;
        let inbound_log = JsonlEventLog::open(state_dir.join("inbound-events.jsonl"))?;
        let outbound_log = JsonlEventLog::open(state_dir.join("outbound-events.jsonl"))?;

        Ok(Self {
            config,
            slack_client,
            state_store,
            inbound_log,
            outbound_log,
            bot_user_id,
            state_dir,
            active_runs: HashMap::new(),
            latest_runs: HashMap::new(),
            channel_queues: HashMap::new(),
        })
    }

    async fn run(&mut self) -> Result<()> {
        let mut failure_streak = self.state_store.transport_health().failure_streak;
        loop {
            let connect_started = Instant::now();
            let socket_url = match self.slack_client.open_socket_connection().await {
                Ok(url) => url,
                Err(error) => {
                    failure_streak = failure_streak.saturating_add(1);
                    self.persist_transport_health(
                        &PollCycleReport::default(),
                        connect_started.elapsed().as_millis() as u64,
                        failure_streak,
                    )?;
                    eprintln!("slack bridge failed to open socket connection: {error}");
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            println!("slack bridge shutdown requested");
                            return Ok(());
                        }
                        _ = tokio::time::sleep(self.config.reconnect_delay) => {}
                    }
                    continue;
                }
            };

            println!("slack bridge socket connected");
            let session_result = self.run_socket_session(&socket_url).await;
            if let Err(error) = session_result {
                failure_streak = failure_streak.saturating_add(1);
                self.persist_transport_health(&PollCycleReport::default(), 0, failure_streak)?;
                eprintln!("slack bridge socket session error: {error}");
            } else {
                failure_streak = 0;
            }

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("slack bridge shutdown requested");
                    return Ok(());
                }
                _ = tokio::time::sleep(self.config.reconnect_delay) => {}
            }
        }
    }

    async fn run_socket_session(&mut self, socket_url: &str) -> Result<()> {
        let (stream, _response) = connect_async(socket_url)
            .await
            .with_context(|| "failed to connect slack socket mode websocket")?;
        let (mut sink, mut source) = stream.split();

        loop {
            let cycle_started = Instant::now();
            let mut report = PollCycleReport::default();
            self.drain_finished_runs(&mut report).await?;
            self.try_start_queued_runs(&mut report).await?;

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    return Ok(());
                }
                maybe_message = source.next() => {
                    let Some(message_result) = maybe_message else {
                        return Ok(());
                    };
                    let message = message_result.context("failed reading slack websocket message")?;
                    if let Some(envelope) = parse_socket_envelope(message)? {
                        self.ack_envelope(&mut sink, &envelope.envelope_id).await?;
                        self.handle_envelope(envelope, &mut report).await?;
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                }
            }

            let cycle_duration_ms = cycle_started.elapsed().as_millis() as u64;
            self.persist_transport_health(&report, cycle_duration_ms, 0)?;

            if report.discovered_events > 0
                || report.queued_events > 0
                || report.completed_runs > 0
                || report.skipped_duplicate_events > 0
                || report.skipped_stale_events > 0
                || report.failed_events > 0
            {
                println!(
                    "slack bridge cycle: discovered={} queued={} completed={} duplicate_skips={} stale_skips={} failed={}",
                    report.discovered_events,
                    report.queued_events,
                    report.completed_runs,
                    report.skipped_duplicate_events,
                    report.skipped_stale_events,
                    report.failed_events,
                );
            }
        }
    }

    fn persist_transport_health(
        &mut self,
        report: &PollCycleReport,
        cycle_duration_ms: u64,
        failure_streak: usize,
    ) -> Result<()> {
        let snapshot =
            self.build_transport_health_snapshot(report, cycle_duration_ms, failure_streak);
        if self.state_store.update_transport_health(snapshot) {
            self.state_store.save()?;
        }
        Ok(())
    }

    fn build_transport_health_snapshot(
        &self,
        report: &PollCycleReport,
        cycle_duration_ms: u64,
        failure_streak: usize,
    ) -> TransportHealthSnapshot {
        let queue_depth = self
            .channel_queues
            .values()
            .map(VecDeque::len)
            .sum::<usize>();
        let processed_events = report
            .discovered_events
            .saturating_sub(report.skipped_duplicate_events)
            .saturating_sub(report.skipped_stale_events);
        TransportHealthSnapshot {
            updated_unix_ms: current_unix_timestamp_ms(),
            cycle_duration_ms,
            queue_depth,
            active_runs: self.active_runs.len(),
            failure_streak,
            last_cycle_discovered: report.discovered_events,
            last_cycle_processed: processed_events,
            last_cycle_completed: report.completed_runs,
            last_cycle_failed: report.failed_events,
            last_cycle_duplicates: report.skipped_duplicate_events,
        }
    }

    async fn ack_envelope<S>(&self, sink: &mut S, envelope_id: &str) -> Result<()>
    where
        S: futures_util::Sink<WsMessage> + Unpin,
        S::Error: std::error::Error + Send + Sync + 'static,
    {
        let ack = json!({ "envelope_id": envelope_id }).to_string();
        sink.send(WsMessage::Text(ack.into()))
            .await
            .context("failed to send slack socket ack")
    }

    async fn handle_envelope(
        &mut self,
        envelope: SlackSocketEnvelope,
        report: &mut PollCycleReport,
    ) -> Result<()> {
        let now_unix_ms = current_unix_timestamp_ms();
        report.discovered_events = report.discovered_events.saturating_add(1);

        let normalized = normalize_socket_envelope(&envelope, &self.bot_user_id)?;

        let Some(event) = normalized else {
            return Ok(());
        };

        if self.state_store.contains(&event.key) {
            report.skipped_duplicate_events = report.skipped_duplicate_events.saturating_add(1);
            return Ok(());
        }

        if event_is_stale(&event, self.config.max_event_age_seconds, now_unix_ms) {
            if self.state_store.mark_processed(&event.key) {
                self.state_store.save()?;
            }
            report.skipped_stale_events = report.skipped_stale_events.saturating_add(1);
            return Ok(());
        }

        let policy_channel = format!("slack:{}", event.channel_id);
        let pairing_policy = pairing_policy_for_state_dir(&self.config.state_dir);
        let pairing_decision = evaluate_pairing_access(
            &pairing_policy,
            &policy_channel,
            &event.user_id,
            now_unix_ms,
        )?;
        let pairing_status = if matches!(pairing_decision, PairingDecision::Allow { .. }) {
            "allow"
        } else {
            "deny"
        };
        let pairing_reason_code = pairing_decision.reason_code().to_string();

        self.inbound_log.append(&json!({
            "timestamp_unix_ms": now_unix_ms,
            "event_key": event.key,
            "kind": event.kind.as_str(),
            "channel": event.channel_id,
            "event_id": event.event_id,
            "pairing": {
                "decision": pairing_status,
                "reason_code": pairing_reason_code,
                "channel": policy_channel,
                "actor_id": event.user_id,
            },
            "payload": event.raw_payload,
        }))?;
        ChannelStore::open(
            &self.state_dir.join("channel-store"),
            "slack",
            &event.channel_id,
        )?
        .append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: now_unix_ms,
            direction: "inbound".to_string(),
            event_key: Some(event.key.clone()),
            source: "slack".to_string(),
            payload: json!({
                "kind": event.kind.as_str(),
                "event_id": event.event_id,
                "user_id": event.user_id,
                "text": event.text,
                "pairing": {
                    "decision": pairing_status,
                    "reason_code": pairing_reason_code,
                    "channel": policy_channel,
                },
            }),
        })?;

        if let PairingDecision::Deny { reason_code } = pairing_decision {
            self.outbound_log.append(&json!({
                "timestamp_unix_ms": now_unix_ms,
                "event_key": event.key,
                "channel": event.channel_id,
                "event_id": event.event_id,
                "command": "authorization",
                "status": "denied",
                "reason_code": reason_code,
                "policy_channel": policy_channel,
                "actor_id": event.user_id,
            }))?;
            if self.state_store.mark_processed(&event.key) {
                self.state_store.save()?;
            }
            eprintln!(
                "slack bridge event denied: channel={} event_id={} key={} actor={} reason_code={}",
                event.channel_id, event.event_id, event.key, event.user_id, reason_code
            );
            return Ok(());
        }

        let slack_command = parse_slack_command(&event, &self.bot_user_id);
        let rbac_principal = slack_principal(&event.user_id);
        let rbac_action = rbac_action_for_slack_command(slack_command.as_ref());
        let rbac_policy_path = rbac_policy_path_for_state_dir(&self.config.state_dir);
        match authorize_action_for_principal_with_policy_path(
            &rbac_principal,
            &rbac_action,
            rbac_policy_path.as_path(),
        ) {
            Ok(RbacDecision::Allow { .. }) => {}
            Ok(RbacDecision::Deny {
                reason_code,
                matched_role,
                matched_pattern,
            }) => {
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": now_unix_ms,
                    "event_key": event.key,
                    "channel": event.channel_id,
                    "event_id": event.event_id,
                    "command": "rbac-authorization",
                    "status": "denied",
                    "reason_code": reason_code,
                    "matched_role": matched_role,
                    "matched_pattern": matched_pattern,
                    "principal": rbac_principal,
                    "action": rbac_action,
                    "actor_id": event.user_id,
                }))?;
                if self.state_store.mark_processed(&event.key) {
                    self.state_store.save()?;
                }
                return Ok(());
            }
            Err(error) => {
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": now_unix_ms,
                    "event_key": event.key,
                    "channel": event.channel_id,
                    "event_id": event.event_id,
                    "command": "rbac-authorization",
                    "status": "error",
                    "reason_code": "rbac_policy_error",
                    "principal": rbac_principal,
                    "action": rbac_action,
                    "actor_id": event.user_id,
                    "error": error.to_string(),
                }))?;
                if self.state_store.mark_processed(&event.key) {
                    self.state_store.save()?;
                }
                report.failed_events = report.failed_events.saturating_add(1);
                return Ok(());
            }
        }

        if self.state_store.mark_processed(&event.key) {
            self.state_store.save()?;
        }

        if let Some(command) = slack_command {
            self.handle_slack_command(&event, command, report).await?;
            return Ok(());
        }

        self.channel_queues
            .entry(event.channel_id.clone())
            .or_default()
            .push_back(event);
        report.queued_events = report.queued_events.saturating_add(1);

        Ok(())
    }

    async fn try_start_queued_runs(&mut self, report: &mut PollCycleReport) -> Result<()> {
        let channels = self.channel_queues.keys().cloned().collect::<Vec<_>>();

        for channel in channels {
            if self.active_runs.contains_key(&channel) {
                continue;
            }
            let now_unix_ms = current_unix_timestamp_ms();
            let Some(event) = self.channel_queues.get_mut(&channel).and_then(|queue| {
                dequeue_coalesced_event_for_run(
                    queue,
                    now_unix_ms,
                    self.config.coalescing_window_ms,
                )
            }) else {
                continue;
            };

            let run_id = format!("slack-{}-{}", event.channel_id, current_unix_timestamp_ms());
            let working_message = self
                .slack_client
                .post_message(
                    &event.channel_id,
                    &format!("Tau is working on run {run_id}..."),
                    event.reply_thread_ts(),
                )
                .await?;

            let (cancel_tx, cancel_rx) = watch::channel(false);
            let started_unix_ms = current_unix_timestamp_ms();
            let task_params = SlackRunTaskParams {
                slack_client: self.slack_client.clone(),
                config: self.config.clone(),
                state_dir: self.state_dir.clone(),
                event: event.clone(),
                run_id: run_id.clone(),
                working_message,
                cancel_rx,
                bot_user_id: self.bot_user_id.clone(),
                started_unix_ms,
            };
            let handle = tokio::spawn(async move { execute_channel_run_task(task_params).await });

            self.active_runs.insert(
                channel,
                ActiveChannelRun {
                    run_id: run_id.clone(),
                    event_key: event.key.clone(),
                    started_unix_ms,
                    started: Instant::now(),
                    cancel_tx,
                    handle,
                },
            );
            report.queued_events = report.queued_events.saturating_add(1);
        }

        Ok(())
    }

    async fn handle_slack_command(
        &mut self,
        event: &SlackBridgeEvent,
        command: SlackCommand,
        report: &mut PollCycleReport,
    ) -> Result<()> {
        let now_unix_ms = current_unix_timestamp_ms();
        let reply_thread_ts = event.reply_thread_ts();
        let (message, command_name, status, extra) = match command {
            SlackCommand::Help => (slack_command_usage(), "help", "reported", None),
            SlackCommand::Status => (
                self.render_channel_status(&event.channel_id),
                "status",
                "reported",
                None,
            ),
            SlackCommand::Health => (
                self.render_channel_health(&event.channel_id),
                "health",
                "reported",
                None,
            ),
            SlackCommand::Stop => {
                if let Some(active) = self.active_runs.get(&event.channel_id) {
                    if *active.cancel_tx.borrow() {
                        (
                            format!(
                                "Stop has already been requested for run `{}`.",
                                active.run_id
                            ),
                            "stop",
                            "acknowledged",
                            Some(json!({"run_id": active.run_id})),
                        )
                    } else {
                        let _ = active.cancel_tx.send(true);
                        (
                            format!(
                                "Cancellation requested for run `{}` (event `{}`).",
                                active.run_id, active.event_key
                            ),
                            "stop",
                            "acknowledged",
                            Some(json!({"run_id": active.run_id, "event_key": active.event_key})),
                        )
                    }
                } else {
                    (
                        "No active run for this channel. Current state is idle.".to_string(),
                        "stop",
                        "acknowledged",
                        None,
                    )
                }
            }
            SlackCommand::Artifacts { purge, run_id } => {
                if purge {
                    (
                        self.render_channel_artifact_purge(&event.channel_id)?,
                        "artifacts-purge",
                        "reported",
                        None,
                    )
                } else {
                    (
                        self.render_channel_artifacts(&event.channel_id, run_id.as_deref())?,
                        "artifacts",
                        "reported",
                        run_id.as_ref().map(|value| json!({"run_id": value})),
                    )
                }
            }
            SlackCommand::ArtifactShow { artifact_id } => (
                self.render_channel_artifact_show(&event.channel_id, &artifact_id)?,
                "artifacts-show",
                "reported",
                Some(json!({"artifact_id": artifact_id})),
            ),
            SlackCommand::Canvas { args } => {
                let channel_store = ChannelStore::open(
                    &self.state_dir.join("channel-store"),
                    "slack",
                    &event.channel_id,
                )?;
                let session_path = channel_store.session_path();
                let session_head_id = SessionStore::load(&session_path)
                    .ok()
                    .and_then(|store| store.head_id());
                (
                    execute_canvas_command(
                        &args,
                        &CanvasCommandConfig {
                            canvas_root: self.state_dir.join("canvas"),
                            channel_store_root: self.state_dir.join("channel-store"),
                            principal: slack_principal(&event.user_id),
                            origin: CanvasEventOrigin {
                                transport: "slack".to_string(),
                                channel: Some(event.channel_id.clone()),
                                source_event_key: Some(event.key.clone()),
                                source_unix_ms: Some(event.occurred_unix_ms),
                            },
                            session_link: Some(CanvasSessionLinkContext {
                                session_path,
                                session_head_id,
                            }),
                        },
                    ),
                    "canvas",
                    "reported",
                    Some(json!({"canvas_args": args})),
                )
            }
            SlackCommand::Invalid { message } => (message, "invalid", "usage_reported", None),
        };
        let source_event_key = event.key.clone();
        let response_marker = slack_metadata_marker(&source_event_key);
        let response_message = render_slack_command_response(event, command_name, status, &message);

        let posted = self
            .slack_client
            .post_message(&event.channel_id, &response_message, reply_thread_ts)
            .await?;
        let mut payload = json!({
            "timestamp_unix_ms": now_unix_ms,
            "event_key": source_event_key,
            "source_event_key": event.key,
            "channel_id": event.channel_id,
            "command": command_name,
            "status": status,
            "posted_ts": posted.ts.clone(),
            "response_marker": response_marker.clone(),
        });
        if let Some(extra) = extra {
            payload["details"] = extra;
        }
        self.outbound_log.append(&payload)?;
        ChannelStore::open(
            &self.state_dir.join("channel-store"),
            "slack",
            &event.channel_id,
        )?
        .append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: now_unix_ms,
            direction: "outbound".to_string(),
            event_key: Some(event.key.clone()),
            source: "slack".to_string(),
            payload: json!({
                "kind": "command_response",
                "command": command_name,
                "status": status,
                "posted_ts": posted.ts,
                "response_marker": response_marker,
                "source_event_key": event.key,
                "body": response_message,
            }),
        })?;
        report.completed_runs = report.completed_runs.saturating_add(1);
        Ok(())
    }

    fn render_channel_status(&self, channel_id: &str) -> String {
        let active = self.active_runs.get(channel_id);
        let latest = self.latest_runs.get(channel_id);
        let state = if active.is_some() { "running" } else { "idle" };
        let mut lines = vec![format!("Tau status for channel {channel_id}: {state}")];
        if let Some(active) = active {
            lines.push(format!("active_run_id: {}", active.run_id));
            lines.push(format!("active_event_key: {}", active.event_key));
            lines.push(format!(
                "active_elapsed_ms: {}",
                active.started.elapsed().as_millis()
            ));
            lines.push(format!(
                "active_started_unix_ms: {}",
                active.started_unix_ms
            ));
            lines.push(format!(
                "cancellation_requested: {}",
                if *active.cancel_tx.borrow() {
                    "true"
                } else {
                    "false"
                }
            ));
        } else {
            lines.push("active_run_id: none".to_string());
        }

        if let Some(latest) = latest {
            lines.push(format!("latest_run_id: {}", latest.run_id));
            lines.push(format!("latest_event_key: {}", latest.event_key));
            lines.push(format!("latest_status: {}", latest.status));
            lines.push(format!(
                "latest_started_unix_ms: {}",
                latest.started_unix_ms
            ));
            lines.push(format!(
                "latest_completed_unix_ms: {}",
                latest.completed_unix_ms
            ));
            lines.push(format!("latest_duration_ms: {}", latest.duration_ms));
        } else {
            lines.push("latest_run_id: none".to_string());
        }

        lines.extend(self.state_store.transport_health().status_lines());

        lines.join("\n")
    }

    fn render_channel_health(&self, channel_id: &str) -> String {
        let active = self.active_runs.get(channel_id);
        let runtime_state = if active.is_some() { "running" } else { "idle" };
        let health = self.state_store.transport_health();
        let classification = health.classify();
        let mut lines = vec![format!(
            "Tau health for channel {}: {}",
            channel_id,
            classification.state.as_str()
        )];
        lines.push(format!("runtime_state: {runtime_state}"));
        if let Some(active) = active {
            lines.push(format!("active_run_id: {}", active.run_id));
            lines.push(format!("active_event_key: {}", active.event_key));
            lines.push(format!(
                "active_elapsed_ms: {}",
                active.started.elapsed().as_millis()
            ));
        } else {
            lines.push("active_run_id: none".to_string());
        }
        lines.extend(health.health_detail_lines());
        lines.join("\n")
    }

    fn render_channel_artifacts(
        &self,
        channel_id: &str,
        run_id_filter: Option<&str>,
    ) -> Result<String> {
        let store = ChannelStore::open(&self.state_dir.join("channel-store"), "slack", channel_id)?;
        let loaded = store.load_artifact_records_tolerant()?;
        let mut active = store.list_active_artifacts(current_unix_timestamp_ms())?;
        if let Some(run_id_filter) = run_id_filter {
            active.retain(|artifact| artifact.run_id == run_id_filter);
        }
        active.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut lines = vec![if let Some(run_id_filter) = run_id_filter {
            format!(
                "Tau artifacts for channel {} run_id `{}`: active={}",
                channel_id,
                run_id_filter,
                active.len()
            )
        } else {
            format!(
                "Tau artifacts for channel {}: active={}",
                channel_id,
                active.len()
            )
        }];
        if active.is_empty() {
            if let Some(run_id_filter) = run_id_filter {
                lines.push(format!("none for run_id `{}`", run_id_filter));
            } else {
                lines.push("none".to_string());
            }
        } else {
            let max_rows = 10_usize;
            for artifact in active.iter().take(max_rows) {
                lines.push(format!(
                    "- id `{}` type `{}` bytes `{}` visibility `{}` created_unix_ms `{}` expires_unix_ms `{}` checksum `{}` path `{}`",
                    artifact.id,
                    artifact.artifact_type,
                    artifact.bytes,
                    artifact.visibility,
                    artifact.created_unix_ms,
                    artifact
                        .expires_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    artifact.checksum_sha256,
                    artifact.relative_path,
                ));
            }
            if active.len() > max_rows {
                lines.push(format!(
                    "... {} additional artifacts omitted",
                    active.len() - max_rows
                ));
            }
        }
        if loaded.invalid_lines > 0 {
            lines.push(format!(
                "index_invalid_lines: {} (ignored)",
                loaded.invalid_lines
            ));
        }
        Ok(lines.join("\n"))
    }

    fn render_channel_artifact_purge(&self, channel_id: &str) -> Result<String> {
        let now_unix_ms = current_unix_timestamp_ms();
        let store = ChannelStore::open(&self.state_dir.join("channel-store"), "slack", channel_id)?;
        let purge = store.purge_expired_artifacts(now_unix_ms)?;
        let active = store.list_active_artifacts(now_unix_ms)?;
        Ok(format!(
            "Tau artifact purge for channel {}: expired_removed={} invalid_removed={} active_remaining={}",
            channel_id,
            purge.expired_removed,
            purge.invalid_removed,
            active.len()
        ))
    }

    fn render_channel_artifact_show(&self, channel_id: &str, artifact_id: &str) -> Result<String> {
        let store = ChannelStore::open(&self.state_dir.join("channel-store"), "slack", channel_id)?;
        let loaded = store.load_artifact_records_tolerant()?;
        let now_unix_ms = current_unix_timestamp_ms();
        let artifact = loaded
            .records
            .iter()
            .find(|record| record.id == artifact_id);
        let mut lines = Vec::new();
        match artifact {
            Some(record) => {
                let expired = record
                    .expires_unix_ms
                    .map(|expires_unix_ms| expires_unix_ms <= now_unix_ms)
                    .unwrap_or(false);
                lines.push(format!(
                    "Tau artifact for channel {} id `{}`: state={}",
                    channel_id,
                    artifact_id,
                    if expired { "expired" } else { "active" }
                ));
                lines.push(format!("run_id: {}", record.run_id));
                lines.push(format!("artifact_type: {}", record.artifact_type));
                lines.push(format!("visibility: {}", record.visibility));
                lines.push(format!("bytes: {}", record.bytes));
                lines.push(format!("created_unix_ms: {}", record.created_unix_ms));
                lines.push(format!(
                    "expires_unix_ms: {}",
                    record
                        .expires_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!("checksum: {}", record.checksum_sha256));
                lines.push(format!("path: {}", record.relative_path));
                if expired {
                    lines.push(
                        "artifact is expired and may be removed by `/tau artifacts purge`."
                            .to_string(),
                    );
                }
            }
            None => lines.push(format!(
                "Tau artifact for channel {} id `{}`: not found",
                channel_id, artifact_id
            )),
        }
        if loaded.invalid_lines > 0 {
            lines.push(format!(
                "index_invalid_lines: {} (ignored)",
                loaded.invalid_lines
            ));
        }
        Ok(lines.join("\n"))
    }

    async fn drain_finished_runs(&mut self, report: &mut PollCycleReport) -> Result<()> {
        let finished_channels = self
            .active_runs
            .iter()
            .filter_map(|(channel, run)| run.handle.is_finished().then_some(channel.clone()))
            .collect::<Vec<_>>();

        for channel in finished_channels {
            let Some(active) = self.active_runs.remove(&channel) else {
                continue;
            };
            match active.handle.await {
                Ok(result) => {
                    self.latest_runs.insert(
                        channel.clone(),
                        SlackLatestRun {
                            run_id: result.run_id.clone(),
                            event_key: result.event_key.clone(),
                            status: result.status.clone(),
                            started_unix_ms: result.started_unix_ms,
                            completed_unix_ms: result.completed_unix_ms,
                            duration_ms: result.duration_ms,
                        },
                    );
                    self.outbound_log.append(&json!({
                        "timestamp_unix_ms": current_unix_timestamp_ms(),
                        "event_key": result.event_key,
                        "channel": result.channel_id,
                        "run_id": result.run_id,
                        "status": result.status,
                        "started_unix_ms": result.started_unix_ms,
                        "completed_unix_ms": result.completed_unix_ms,
                        "duration_ms": result.duration_ms,
                        "model": result.model,
                        "usage": {
                            "input_tokens": result.usage.input_tokens,
                            "output_tokens": result.usage.output_tokens,
                            "total_tokens": result.usage.total_tokens,
                            "request_duration_ms": result.usage.request_duration_ms,
                            "finish_reason": result.usage.finish_reason,
                        },
                        "error": result.error,
                    }))?;
                    report.completed_runs = report.completed_runs.saturating_add(1);
                }
                Err(error) => {
                    report.failed_events = report.failed_events.saturating_add(1);
                    eprintln!("slack bridge run task join error: {error}");
                }
            }
        }

        Ok(())
    }
}

fn should_coalesce_events(
    previous: &SlackBridgeEvent,
    current: &SlackBridgeEvent,
    coalescing_window_ms: u64,
) -> bool {
    if coalescing_window_ms == 0 {
        return false;
    }
    if previous.user_id.trim() != current.user_id.trim() {
        return false;
    }
    if previous.thread_ts.as_deref().unwrap_or_default()
        != current.thread_ts.as_deref().unwrap_or_default()
    {
        return false;
    }
    if current.occurred_unix_ms < previous.occurred_unix_ms {
        return false;
    }
    current
        .occurred_unix_ms
        .saturating_sub(previous.occurred_unix_ms)
        <= coalescing_window_ms
}

fn coalesced_batch_len(queue: &VecDeque<SlackBridgeEvent>, coalescing_window_ms: u64) -> usize {
    let Some(mut previous) = queue.front() else {
        return 0;
    };
    if coalescing_window_ms == 0 {
        return 1;
    }
    let mut len = 1;
    while let Some(current) = queue.get(len) {
        if !should_coalesce_events(previous, current, coalescing_window_ms) {
            break;
        }
        len = len.saturating_add(1);
        previous = current;
    }
    len
}

fn merge_coalesced_event(target: &mut SlackBridgeEvent, source: SlackBridgeEvent) {
    if !source.text.is_empty() {
        if !target.text.is_empty() {
            target.text.push('\n');
        }
        target.text.push_str(&source.text);
    }
    target.files.extend(source.files);
    target.occurred_unix_ms = source.occurred_unix_ms;
    target.ts = source.ts;
}

fn dequeue_coalesced_event_for_run(
    queue: &mut VecDeque<SlackBridgeEvent>,
    now_unix_ms: u64,
    coalescing_window_ms: u64,
) -> Option<SlackBridgeEvent> {
    queue.front()?;

    let batch_len = coalesced_batch_len(queue, coalescing_window_ms);
    if batch_len == 0 {
        return None;
    }
    let last = queue.get(batch_len.saturating_sub(1))?;
    if now_unix_ms.saturating_sub(last.occurred_unix_ms) < coalescing_window_ms {
        return None;
    }

    let mut event = queue.pop_front()?;
    for _ in 1..batch_len {
        let Some(next) = queue.pop_front() else {
            break;
        };
        merge_coalesced_event(&mut event, next);
    }
    Some(event)
}

struct SlackRunTaskParams {
    slack_client: SlackApiClient,
    config: SlackBridgeRuntimeConfig,
    state_dir: PathBuf,
    event: SlackBridgeEvent,
    run_id: String,
    working_message: SlackPostedMessage,
    cancel_rx: watch::Receiver<bool>,
    bot_user_id: String,
    started_unix_ms: u64,
}

async fn execute_channel_run_task(params: SlackRunTaskParams) -> RunTaskResult {
    let SlackRunTaskParams {
        slack_client,
        config,
        state_dir,
        event,
        run_id,
        working_message,
        cancel_rx,
        bot_user_id,
        started_unix_ms,
    } = params;

    let started = Instant::now();
    let run_result = run_prompt_for_event(
        &config,
        &state_dir,
        &event,
        &run_id,
        cancel_rx,
        &slack_client,
        &bot_user_id,
    )
    .await;

    let completed_unix_ms = current_unix_timestamp_ms();
    let duration_ms = started.elapsed().as_millis() as u64;

    let (status, usage, body, detail): (String, PromptUsageSummary, String, Option<String>) =
        match run_result {
            Ok(run) => {
                let status = prompt_status_label(run.status).to_string();
                let rendered = render_slack_response(
                    &event,
                    &run,
                    config.detail_thread_output,
                    config.detail_thread_threshold_chars,
                );
                (status, run.usage.clone(), rendered.0, rendered.1)
            }
            Err(error) => (
                "failed".to_string(),
                PromptUsageSummary::default(),
                render_slack_run_error_message(&event, &run_id, &error),
                None,
            ),
        };

    let update_result = slack_client
        .update_message(&working_message.channel, &working_message.ts, &body)
        .await;

    if update_result.is_err() {
        let fallback = format!(
            "{}\n\n(warning: failed to update placeholder message)",
            truncate_for_slack(&body, 30_000)
        );
        let _ = slack_client
            .post_message(&working_message.channel, &fallback, event.reply_thread_ts())
            .await;
    }

    if let Some(detail_text) = detail {
        let _ = slack_client
            .post_message(
                &event.channel_id,
                &truncate_for_slack(&detail_text, 38_000),
                event.reply_thread_ts(),
            )
            .await;
    }

    RunTaskResult {
        channel_id: event.channel_id,
        event_key: event.key,
        run_id,
        started_unix_ms,
        completed_unix_ms,
        duration_ms,
        status,
        model: config.model,
        usage,
        error: update_result.err().map(|error| error.to_string()),
    }
}

async fn run_prompt_for_event(
    config: &SlackBridgeRuntimeConfig,
    state_dir: &Path,
    event: &SlackBridgeEvent,
    run_id: &str,
    mut cancel_rx: watch::Receiver<bool>,
    slack_client: &SlackApiClient,
    bot_user_id: &str,
) -> Result<PromptRunReport> {
    let channel_store =
        ChannelStore::open(&state_dir.join("channel-store"), "slack", &event.channel_id)?;
    let session_path = channel_store.session_path();

    let downloaded_files = download_attachments(
        slack_client,
        &channel_store.attachments_dir(),
        &event.key,
        &event.files,
    )
    .await?;

    let mut agent = Agent::new(
        config.client.clone(),
        AgentConfig {
            model: config.model.clone(),
            system_prompt: config.system_prompt.clone(),
            max_turns: config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            ..AgentConfig::default()
        },
    );
    let mut tool_policy = config.tool_policy.clone();
    tool_policy.rbac_principal = Some(slack_principal(&event.user_id));
    tool_policy.rbac_policy_path = Some(rbac_policy_path_for_state_dir(&config.state_dir));
    crate::tools::register_builtin_tools(&mut agent, tool_policy);

    let usage = Arc::new(Mutex::new(PromptUsageSummary::default()));
    agent.subscribe({
        let usage = usage.clone();
        move |event| {
            if let AgentEvent::TurnEnd {
                usage: turn_usage,
                request_duration_ms,
                finish_reason,
                ..
            } = event
            {
                if let Ok(mut guard) = usage.lock() {
                    guard.input_tokens = guard.input_tokens.saturating_add(turn_usage.input_tokens);
                    guard.output_tokens =
                        guard.output_tokens.saturating_add(turn_usage.output_tokens);
                    guard.total_tokens = guard.total_tokens.saturating_add(turn_usage.total_tokens);
                    guard.request_duration_ms = guard
                        .request_duration_ms
                        .saturating_add(*request_duration_ms);
                    guard.finish_reason = finish_reason.clone();
                }
            }
        }
    });

    let mut session_runtime = Some(initialize_channel_session_runtime(
        &session_path,
        &config.system_prompt,
        config.session_lock_wait_ms,
        config.session_lock_stale_ms,
        &mut agent,
    )?);

    let formatted_prompt = render_event_prompt(event, bot_user_id, &downloaded_files);
    let start_index = agent.messages().len();
    let cancellation_signal = async move {
        loop {
            if *cancel_rx.borrow() {
                break;
            }
            if cancel_rx.changed().await.is_err() {
                break;
            }
        }
    };

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut session_runtime,
        &formatted_prompt,
        config.turn_timeout_ms,
        cancellation_signal,
        config.render_options,
    )
    .await?;

    let assistant_reply = if status == PromptRunStatus::Cancelled {
        "Run cancelled before completion.".to_string()
    } else if status == PromptRunStatus::TimedOut {
        "Run timed out before completion.".to_string()
    } else {
        collect_assistant_reply(&agent.messages()[start_index..])
    };

    let usage = usage
        .lock()
        .map_err(|_| anyhow!("prompt usage lock is poisoned"))?
        .clone();
    let artifact = channel_store.write_text_artifact(
        run_id,
        "slack-reply",
        "private",
        normalize_artifact_retention_days(config.artifact_retention_days),
        "md",
        &render_slack_artifact_markdown(event, run_id, status, &assistant_reply, &downloaded_files),
    )?;
    channel_store.sync_context_from_messages(agent.messages())?;
    channel_store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        direction: "outbound".to_string(),
        event_key: Some(event.key.clone()),
        source: "slack".to_string(),
        payload: json!({
            "run_id": run_id,
            "status": prompt_status_label(status),
            "assistant_reply": assistant_reply.clone(),
            "tokens": {
                "input": usage.input_tokens,
                "output": usage.output_tokens,
                "total": usage.total_tokens,
            },
            "artifact": {
                "id": artifact.id,
                "path": artifact.relative_path,
                "checksum_sha256": artifact.checksum_sha256,
                "bytes": artifact.bytes,
                "expires_unix_ms": artifact.expires_unix_ms,
            },
        }),
    })?;

    Ok(PromptRunReport {
        run_id: run_id.to_string(),
        model: config.model.clone(),
        status,
        assistant_reply,
        usage,
        downloaded_files,
        artifact,
    })
}

async fn download_attachments(
    slack_client: &SlackApiClient,
    attachments_root: &Path,
    event_key: &str,
    files: &[SlackFileAttachment],
) -> Result<Vec<DownloadedSlackFile>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }

    let file_dir = attachments_root.join(sanitize_for_path(event_key));
    std::fs::create_dir_all(&file_dir)
        .with_context(|| format!("failed to create {}", file_dir.display()))?;

    let mut downloaded = Vec::new();
    for file in files {
        let Some(url) = file.url_private_download.as_deref() else {
            continue;
        };
        let bytes = match slack_client.download_file(url).await {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!(
                    "slack attachment download failed: id={} event={} error={error}",
                    file.id, event_key
                );
                continue;
            }
        };

        let preferred_name = file
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("{}.bin", file.id));
        let safe_name = sanitize_for_path(&preferred_name);
        let path = file_dir.join(format!("{}-{}", file.id, safe_name));
        std::fs::write(&path, bytes)
            .with_context(|| format!("failed to write {}", path.display()))?;

        downloaded.push(DownloadedSlackFile {
            id: file.id.clone(),
            original_name: preferred_name,
            path,
            mimetype: file.mimetype.clone(),
            size: file.size,
        });
    }

    Ok(downloaded)
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

fn parse_socket_envelope(message: WsMessage) -> Result<Option<SlackSocketEnvelope>> {
    match message {
        WsMessage::Text(text) => {
            let envelope = serde_json::from_str::<SlackSocketEnvelope>(&text)
                .context("failed to parse slack socket envelope")?;
            Ok(Some(envelope))
        }
        WsMessage::Binary(bytes) => {
            let text =
                String::from_utf8(bytes.to_vec()).context("invalid utf-8 slack socket payload")?;
            let envelope = serde_json::from_str::<SlackSocketEnvelope>(&text)
                .context("failed to parse slack socket envelope")?;
            Ok(Some(envelope))
        }
        WsMessage::Ping(_) | WsMessage::Pong(_) => Ok(None),
        WsMessage::Close(_) => Ok(None),
        WsMessage::Frame(_) => Ok(None),
    }
}

#[derive(Debug, Deserialize)]
struct SlackEventCallbackEnvelope {
    #[serde(rename = "type")]
    callback_type: String,
    event_id: String,
    event_time: u64,
    event: SlackEventPayload,
}

#[derive(Debug, Deserialize)]
struct SlackEventPayload {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    channel_type: Option<String>,
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    thread_ts: Option<String>,
    #[serde(default)]
    files: Vec<SlackFileAttachment>,
}

fn normalize_socket_envelope(
    envelope: &SlackSocketEnvelope,
    bot_user_id: &str,
) -> Result<Option<SlackBridgeEvent>> {
    if envelope.envelope_type != "events_api" {
        return Ok(None);
    }

    let callback = serde_json::from_value::<SlackEventCallbackEnvelope>(envelope.payload.clone())
        .context("failed to decode slack event callback payload")?;
    if callback.callback_type != "event_callback" {
        return Ok(None);
    }

    let event = callback.event;
    if event.subtype.as_deref() == Some("bot_message") {
        return Ok(None);
    }
    let user_id = match event.user {
        Some(user) if !user.trim().is_empty() => user,
        _ => return Ok(None),
    };
    if user_id == bot_user_id {
        return Ok(None);
    }

    let channel_id = match event.channel {
        Some(channel) if !channel.trim().is_empty() => channel,
        _ => return Ok(None),
    };
    let message_ts = match event.ts {
        Some(ts) if !ts.trim().is_empty() => ts,
        _ => return Ok(None),
    };
    let text = event.text.unwrap_or_default();

    let kind = match event.event_type.as_str() {
        "app_mention" => SlackBridgeEventKind::AppMention,
        "message" if event.channel_type.as_deref() == Some("im") || channel_id.starts_with('D') => {
            SlackBridgeEventKind::DirectMessage
        }
        _ => return Ok(None),
    };

    let occurred_unix_ms = callback.event_time.saturating_mul(1000);

    let key = format!("{}:{}:{}", callback.event_id, channel_id, message_ts);
    Ok(Some(SlackBridgeEvent {
        key,
        kind,
        event_id: callback.event_id,
        occurred_unix_ms,
        channel_id,
        user_id,
        text,
        ts: message_ts,
        thread_ts: event.thread_ts,
        files: event.files,
        raw_payload: envelope.payload.clone(),
    }))
}

fn event_is_stale(event: &SlackBridgeEvent, max_event_age_seconds: u64, now_unix_ms: u64) -> bool {
    if max_event_age_seconds == 0 {
        return false;
    }
    let max_age_ms = max_event_age_seconds.saturating_mul(1000);
    now_unix_ms.saturating_sub(event.occurred_unix_ms) > max_age_ms
}

#[cfg(test)]
mod tests;
