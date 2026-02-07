use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::{LlmClient, Message, MessageRole};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use crate::channel_store::{ChannelLogEntry, ChannelStore};
use crate::{
    current_unix_timestamp_ms, run_prompt_with_cancellation, write_text_atomic, PromptRunStatus,
    RenderOptions, SessionRuntime,
};
use crate::{session::SessionStore, tools::ToolPolicy};

const SLACK_STATE_SCHEMA_VERSION: u32 = 1;
const SLACK_METADATA_MARKER_PREFIX: &str = "<!-- rsbot-slack-event:";
const SLACK_METADATA_MARKER_SUFFIX: &str = " -->";

#[derive(Clone)]
pub(crate) struct SlackBridgeRuntimeConfig {
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
    pub reconnect_delay: Duration,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SlackBridgeState {
    schema_version: u32,
    #[serde(default)]
    processed_event_keys: Vec<String>,
}

impl Default for SlackBridgeState {
    fn default() -> Self {
        Self {
            schema_version: SLACK_STATE_SCHEMA_VERSION,
            processed_event_keys: Vec::new(),
        }
    }
}

struct SlackBridgeStateStore {
    path: PathBuf,
    cap: usize,
    state: SlackBridgeState,
    processed_index: HashSet<String>,
}

impl SlackBridgeStateStore {
    fn load(path: PathBuf, cap: usize) -> Result<Self> {
        let mut state = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read state file {}", path.display()))?;
            serde_json::from_str::<SlackBridgeState>(&raw).with_context(|| {
                format!("failed to parse slack bridge state file {}", path.display())
            })?
        } else {
            SlackBridgeState::default()
        };

        if state.schema_version != SLACK_STATE_SCHEMA_VERSION {
            bail!(
                "unsupported slack bridge state schema: expected {}, found {}",
                SLACK_STATE_SCHEMA_VERSION,
                state.schema_version
            );
        }

        let cap = cap.max(1);
        if state.processed_event_keys.len() > cap {
            let keep_from = state.processed_event_keys.len() - cap;
            state.processed_event_keys = state.processed_event_keys[keep_from..].to_vec();
        }

        let processed_index = state
            .processed_event_keys
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        Ok(Self {
            path,
            cap,
            state,
            processed_index,
        })
    }

    fn contains(&self, key: &str) -> bool {
        self.processed_index.contains(key)
    }

    fn mark_processed(&mut self, key: &str) -> bool {
        if self.processed_index.contains(key) {
            return false;
        }
        self.state.processed_event_keys.push(key.to_string());
        self.processed_index.insert(key.to_string());
        while self.state.processed_event_keys.len() > self.cap {
            let removed = self.state.processed_event_keys.remove(0);
            self.processed_index.remove(&removed);
        }
        true
    }

    fn save(&self) -> Result<()> {
        let mut payload =
            serde_json::to_string_pretty(&self.state).context("failed to serialize state")?;
        payload.push('\n');
        write_text_atomic(&self.path, &payload)
            .with_context(|| format!("failed to write state file {}", self.path.display()))?;
        Ok(())
    }
}

#[derive(Clone)]
struct JsonlEventLog {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
}

impl JsonlEventLog {
    fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    fn append(&self, value: &Value) -> Result<()> {
        let line = serde_json::to_string(value).context("failed to encode log event")?;
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("event log mutex is poisoned"))?;
        writeln!(file, "{line}")
            .with_context(|| format!("failed to append to {}", self.path.display()))?;
        file.flush()
            .with_context(|| format!("failed to flush {}", self.path.display()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SlackAuthTestResponse {
    ok: bool,
    user_id: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackOpenSocketResponse {
    ok: bool,
    url: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SlackChatMessageResponse {
    ok: bool,
    ts: Option<String>,
    channel: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct SlackPostedMessage {
    channel: String,
    ts: String,
}

#[derive(Clone)]
struct SlackApiClient {
    http: reqwest::Client,
    api_base: String,
    app_token: String,
    bot_token: String,
    retry_max_attempts: usize,
    retry_base_delay_ms: u64,
}

impl SlackApiClient {
    fn new(
        api_base: String,
        app_token: String,
        bot_token: String,
        request_timeout_ms: u64,
        retry_max_attempts: usize,
        retry_base_delay_ms: u64,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("rsBot-slack-bridge"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(request_timeout_ms.max(1)))
            .build()
            .context("failed to create slack api client")?;

        Ok(Self {
            http,
            api_base: api_base.trim_end_matches('/').to_string(),
            app_token: app_token.trim().to_string(),
            bot_token: bot_token.trim().to_string(),
            retry_max_attempts: retry_max_attempts.max(1),
            retry_base_delay_ms: retry_base_delay_ms.max(1),
        })
    }

    async fn resolve_bot_user_id(&self) -> Result<String> {
        let response: SlackAuthTestResponse = self
            .request_json(
                "auth.test",
                || {
                    self.http
                        .post(format!("{}/auth.test", self.api_base))
                        .bearer_auth(&self.bot_token)
                },
                true,
            )
            .await?;

        if !response.ok {
            bail!(
                "slack auth.test failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        response
            .user_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("slack auth.test did not return user_id"))
    }

    async fn open_socket_connection(&self) -> Result<String> {
        let response: SlackOpenSocketResponse = self
            .request_json(
                "apps.connections.open",
                || {
                    self.http
                        .post(format!("{}/apps.connections.open", self.api_base))
                        .bearer_auth(&self.app_token)
                },
                true,
            )
            .await?;
        if !response.ok {
            bail!(
                "slack apps.connections.open failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }
        response
            .url
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow!("slack apps.connections.open did not return url"))
    }

    async fn post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<SlackPostedMessage> {
        let mut payload = json!({
            "channel": channel,
            "text": text,
            "mrkdwn": false,
            "unfurl_links": false,
            "unfurl_media": false,
        });
        if let Some(thread_ts) = thread_ts {
            payload["thread_ts"] = Value::String(thread_ts.to_string());
        }

        let response: SlackChatMessageResponse = self
            .request_json(
                "chat.postMessage",
                || {
                    self.http
                        .post(format!("{}/chat.postMessage", self.api_base))
                        .bearer_auth(&self.bot_token)
                        .json(&payload)
                },
                true,
            )
            .await?;

        if !response.ok {
            bail!(
                "slack chat.postMessage failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }

        Ok(SlackPostedMessage {
            channel: response.channel.unwrap_or_else(|| channel.to_string()),
            ts: response
                .ts
                .ok_or_else(|| anyhow!("slack chat.postMessage response missing ts"))?,
        })
    }

    async fn update_message(
        &self,
        channel: &str,
        ts: &str,
        text: &str,
    ) -> Result<SlackPostedMessage> {
        let payload = json!({
            "channel": channel,
            "ts": ts,
            "text": text,
            "mrkdwn": false,
        });
        let response: SlackChatMessageResponse = self
            .request_json(
                "chat.update",
                || {
                    self.http
                        .post(format!("{}/chat.update", self.api_base))
                        .bearer_auth(&self.bot_token)
                        .json(&payload)
                },
                true,
            )
            .await?;
        if !response.ok {
            bail!(
                "slack chat.update failed: {}",
                response
                    .error
                    .unwrap_or_else(|| "unknown error".to_string())
            );
        }
        Ok(SlackPostedMessage {
            channel: response.channel.unwrap_or_else(|| channel.to_string()),
            ts: response.ts.unwrap_or_else(|| ts.to_string()),
        })
    }

    async fn download_file(&self, url: &str) -> Result<Vec<u8>> {
        let request = || self.http.get(url).bearer_auth(&self.bot_token);
        self.request_bytes("file download", request, false).await
    }

    async fn request_json<T, F>(
        &self,
        operation: &str,
        mut builder: F,
        decode_error_body: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = builder()
                .header(
                    "x-rsbot-retry-attempt",
                    attempt.saturating_sub(1).to_string(),
                )
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let parsed = response
                            .json::<T>()
                            .await
                            .with_context(|| format!("failed to decode slack {operation}"))?;
                        return Ok(parsed);
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let body = if decode_error_body {
                        response.text().await.unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if attempt < self.retry_max_attempts
                        && is_retryable_slack_status(status.as_u16())
                    {
                        tokio::time::sleep(retry_delay(
                            self.retry_base_delay_ms,
                            attempt,
                            retry_after,
                        ))
                        .await;
                        continue;
                    }

                    bail!(
                        "slack api {operation} failed with status {}: {}",
                        status.as_u16(),
                        truncate_for_error(&body, 800)
                    );
                }
                Err(error) => {
                    if attempt < self.retry_max_attempts && is_retryable_transport_error(&error) {
                        tokio::time::sleep(retry_delay(self.retry_base_delay_ms, attempt, None))
                            .await;
                        continue;
                    }
                    return Err(error)
                        .with_context(|| format!("slack api {operation} request failed"));
                }
            }
        }
    }

    async fn request_bytes<F>(
        &self,
        operation: &str,
        mut builder: F,
        decode_error_body: bool,
    ) -> Result<Vec<u8>>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = builder()
                .header(
                    "x-rsbot-retry-attempt",
                    attempt.saturating_sub(1).to_string(),
                )
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response.bytes().await?.to_vec());
                    }
                    let retry_after = parse_retry_after(response.headers());
                    let body = if decode_error_body {
                        response.text().await.unwrap_or_default()
                    } else {
                        String::new()
                    };
                    if attempt < self.retry_max_attempts
                        && is_retryable_slack_status(status.as_u16())
                    {
                        tokio::time::sleep(retry_delay(
                            self.retry_base_delay_ms,
                            attempt,
                            retry_after,
                        ))
                        .await;
                        continue;
                    }

                    bail!(
                        "slack api {operation} failed with status {}: {}",
                        status.as_u16(),
                        truncate_for_error(&body, 800)
                    );
                }
                Err(error) => {
                    if attempt < self.retry_max_attempts && is_retryable_transport_error(&error) {
                        tokio::time::sleep(retry_delay(self.retry_base_delay_ms, attempt, None))
                            .await;
                        continue;
                    }
                    return Err(error)
                        .with_context(|| format!("slack api {operation} request failed"));
                }
            }
        }
    }
}

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
    handle: tokio::task::JoinHandle<RunTaskResult>,
    _cancel_tx: watch::Sender<bool>,
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

pub(crate) async fn run_slack_bridge(config: SlackBridgeRuntimeConfig) -> Result<()> {
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
            channel_queues: HashMap::new(),
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            let socket_url = match self.slack_client.open_socket_connection().await {
                Ok(url) => url,
                Err(error) => {
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
                eprintln!("slack bridge socket session error: {error}");
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

    async fn ack_envelope<S>(&self, sink: &mut S, envelope_id: &str) -> Result<()>
    where
        S: futures_util::Sink<WsMessage> + Unpin,
        S::Error: std::error::Error + Send + Sync + 'static,
    {
        let ack = json!({ "envelope_id": envelope_id }).to_string();
        sink.send(WsMessage::Text(ack))
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

        self.inbound_log.append(&json!({
            "timestamp_unix_ms": now_unix_ms,
            "event_key": event.key,
            "kind": event.kind.as_str(),
            "channel": event.channel_id,
            "event_id": event.event_id,
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
            }),
        })?;

        if self.state_store.mark_processed(&event.key) {
            self.state_store.save()?;
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
            let Some(queue) = self.channel_queues.get_mut(&channel) else {
                continue;
            };
            let Some(event) = queue.pop_front() else {
                continue;
            };

            let run_id = format!("slack-{}-{}", event.channel_id, current_unix_timestamp_ms());
            let working_message = self
                .slack_client
                .post_message(
                    &event.channel_id,
                    &format!("rsBot is working on run {run_id}..."),
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
                    handle,
                    _cancel_tx: cancel_tx,
                },
            );
            report.queued_events = report.queued_events.saturating_add(1);
        }

        Ok(())
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
        },
    );
    crate::tools::register_builtin_tools(&mut agent, config.tool_policy.clone());

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
            }
        }),
    })?;

    Ok(PromptRunReport {
        run_id: run_id.to_string(),
        model: config.model.clone(),
        status,
        assistant_reply,
        usage,
        downloaded_files,
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
            let text = String::from_utf8(bytes).context("invalid utf-8 slack socket payload")?;
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

fn render_event_prompt(
    event: &SlackBridgeEvent,
    bot_user_id: &str,
    downloaded_files: &[DownloadedSlackFile],
) -> String {
    let mut message_text = event.text.trim().to_string();
    if event.kind == SlackBridgeEventKind::AppMention {
        let mention = format!("<@{bot_user_id}>");
        message_text = message_text.replace(&mention, "");
        message_text = message_text.trim().to_string();
    }

    let mut prompt = format!(
        "You are responding as rsBot inside Slack.\nChannel: {}\nUser: <@{}>\nEvent kind: {}\nMessage ts: {}\n\nUser message:\n{}",
        event.channel_id,
        event.user_id,
        event.kind.as_str(),
        event.ts,
        if message_text.is_empty() {
            "(empty message)"
        } else {
            &message_text
        }
    );

    if !downloaded_files.is_empty() {
        prompt.push_str("\n\nDownloaded attachments:\n");
        for file in downloaded_files {
            prompt.push_str(&format!(
                "- id={} name={} path={} mimetype={} size={}\n",
                file.id,
                file.original_name,
                file.path.display(),
                file.mimetype
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                file.size.unwrap_or(0)
            ));
        }
    }

    prompt.push_str("\nProvide a direct, concise Slack-ready response.");
    prompt
}

fn render_slack_response(
    event: &SlackBridgeEvent,
    run: &PromptRunReport,
    detail_thread_output: bool,
    detail_threshold_chars: usize,
) -> (String, Option<String>) {
    let reply = run.assistant_reply.trim();
    let base_reply = if reply.is_empty() {
        "I couldn't generate a textual response for this Slack event."
    } else {
        reply
    };
    let usage = &run.usage;
    let status = format!("{:?}", run.status).to_lowercase();

    let mut summary_body = base_reply.to_string();
    let mut detail_body = None;

    if detail_thread_output && base_reply.chars().count() > detail_threshold_chars.max(1) {
        let summary = truncate_for_slack(base_reply, detail_threshold_chars.max(1));
        summary_body = format!("{}\n\n(full response posted in this thread)", summary);
        detail_body = Some(base_reply.to_string());
    }

    summary_body.push_str("\n\n---\n");
    summary_body.push_str(&format!(
        "{SLACK_METADATA_MARKER_PREFIX}{}{SLACK_METADATA_MARKER_SUFFIX}\nrsBot run {} | status {} | model {} | tokens {}/{}/{}",
        event.key,
        run.run_id,
        status,
        run.model,
        usage.input_tokens,
        usage.output_tokens,
        usage.total_tokens
    ));

    if !run.downloaded_files.is_empty() {
        summary_body.push_str("\nattachments downloaded:");
        for file in &run.downloaded_files {
            summary_body.push_str(&format!(
                "\n- {} ({})",
                file.original_name,
                file.path.display()
            ));
        }
    }

    (truncate_for_slack(&summary_body, 38_000), detail_body)
}

fn render_slack_run_error_message(
    event: &SlackBridgeEvent,
    run_id: &str,
    error: &anyhow::Error,
) -> String {
    truncate_for_slack(
        &format!(
            "rsBot run {} failed for event {}.\n\nError: {}\n\n---\n{SLACK_METADATA_MARKER_PREFIX}{}{SLACK_METADATA_MARKER_SUFFIX}",
            run_id,
            event.key,
            truncate_for_error(&error.to_string(), 600),
            event.key,
        ),
        38_000,
    )
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

fn prompt_status_label(status: PromptRunStatus) -> &'static str {
    match status {
        PromptRunStatus::Completed => "completed",
        PromptRunStatus::Cancelled => "cancelled",
        PromptRunStatus::TimedOut => "timed_out",
    }
}

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn retry_delay(base_delay_ms: u64, attempt: usize, retry_after_seconds: Option<u64>) -> Duration {
    if let Some(retry_after_seconds) = retry_after_seconds {
        return Duration::from_secs(retry_after_seconds);
    }
    let exponent = attempt.saturating_sub(1).min(6) as u32;
    let scale = 2_u64.pow(exponent);
    Duration::from_millis(base_delay_ms.max(1).saturating_mul(scale))
}

fn is_retryable_slack_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

fn truncate_for_error(value: &str, max_chars: usize) -> String {
    truncate_for_slack(value, max_chars)
}

fn truncate_for_slack(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = String::new();
    for ch in value.chars().take(max_chars) {
        truncated.push(ch);
    }
    truncated.push_str("...");
    truncated
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
        "channel".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use httpmock::prelude::*;
    use pi_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, PiAiError};
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::time::sleep;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use super::{
        event_is_stale, normalize_socket_envelope, parse_socket_envelope, render_event_prompt,
        render_slack_response, DownloadedSlackFile, PollCycleReport, SlackApiClient,
        SlackBridgeEvent, SlackBridgeEventKind, SlackBridgeRuntime, SlackBridgeRuntimeConfig,
        SlackBridgeStateStore, SlackSocketEnvelope,
    };
    use crate::{current_unix_timestamp_ms, tools::ToolPolicy, RenderOptions};

    struct StaticReplyClient;

    #[async_trait]
    impl LlmClient for StaticReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("slack bridge reply"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 13,
                    output_tokens: 8,
                    total_tokens: 21,
                },
            })
        }
    }

    struct SlowReplyClient;

    #[async_trait]
    impl LlmClient for SlowReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
            sleep(Duration::from_millis(300)).await;
            Ok(ChatResponse {
                message: Message::assistant_text("slow slack bridge reply"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 5,
                    output_tokens: 3,
                    total_tokens: 8,
                },
            })
        }
    }

    fn test_config(base_url: &str, state_dir: &Path) -> SlackBridgeRuntimeConfig {
        test_config_with_client(base_url, state_dir, Arc::new(StaticReplyClient))
    }

    fn test_config_with_client(
        base_url: &str,
        state_dir: &Path,
        client: Arc<dyn LlmClient>,
    ) -> SlackBridgeRuntimeConfig {
        SlackBridgeRuntimeConfig {
            client,
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "You are rsBot.".to_string(),
            max_turns: 4,
            tool_policy: ToolPolicy::new(vec![state_dir.to_path_buf()]),
            turn_timeout_ms: 0,
            request_timeout_ms: 3_000,
            render_options: RenderOptions {
                stream_output: false,
                stream_delay_ms: 0,
            },
            session_lock_wait_ms: 2_000,
            session_lock_stale_ms: 30_000,
            state_dir: state_dir.to_path_buf(),
            api_base: base_url.to_string(),
            app_token: "xapp-test".to_string(),
            bot_token: "xoxb-test".to_string(),
            bot_user_id: Some("UBOT".to_string()),
            detail_thread_output: true,
            detail_thread_threshold_chars: 20,
            processed_event_cap: 32,
            max_event_age_seconds: 3_600,
            reconnect_delay: Duration::from_millis(10),
            retry_max_attempts: 3,
            retry_base_delay_ms: 5,
        }
    }

    #[test]
    fn unit_parse_socket_envelope_handles_text_binary_and_ping() {
        let text = WsMessage::Text(
            json!({
                "envelope_id": "1",
                "type": "events_api",
                "payload": {
                    "type": "event_callback",
                    "event_id": "Ev1",
                    "event_time": 10,
                    "event": {
                        "type": "app_mention",
                        "user": "U1",
                        "channel": "C1",
                        "text": "hi",
                        "ts": "10.0"
                    }
                }
            })
            .to_string(),
        );
        let parsed = parse_socket_envelope(text).expect("parse text");
        assert!(parsed.is_some());

        let binary = WsMessage::Binary(
            json!({
                "envelope_id": "2",
                "type": "events_api",
                "payload": {
                    "type": "event_callback",
                    "event_id": "Ev2",
                    "event_time": 10,
                    "event": {
                        "type": "message",
                        "channel_type": "im",
                        "user": "U2",
                        "channel": "D1",
                        "text": "dm",
                        "ts": "10.1"
                    }
                }
            })
            .to_string()
            .into_bytes(),
        );
        assert!(parse_socket_envelope(binary)
            .expect("parse binary")
            .is_some());
        assert!(parse_socket_envelope(WsMessage::Ping(vec![]))
            .expect("ping")
            .is_none());
    }

    #[test]
    fn unit_normalize_socket_envelope_maps_mentions_and_dms() {
        let mention = SlackSocketEnvelope {
            envelope_id: "env1".to_string(),
            envelope_type: "events_api".to_string(),
            payload: json!({
                "type": "event_callback",
                "event_id": "Ev1",
                "event_time": 199,
                "event": {
                    "type": "app_mention",
                    "user": "U1",
                    "channel": "C1",
                    "text": "<@UBOT> hi",
                    "ts": "199.1"
                }
            }),
        };
        let mention_event = normalize_socket_envelope(&mention, "UBOT")
            .expect("normalize mention")
            .expect("mention event");
        assert_eq!(mention_event.kind, SlackBridgeEventKind::AppMention);

        let dm = SlackSocketEnvelope {
            envelope_id: "env2".to_string(),
            envelope_type: "events_api".to_string(),
            payload: json!({
                "type": "event_callback",
                "event_id": "Ev2",
                "event_time": 199,
                "event": {
                    "type": "message",
                    "channel_type": "im",
                    "user": "U2",
                    "channel": "D123",
                    "text": "hello",
                    "ts": "199.2"
                }
            }),
        };
        let dm_event = normalize_socket_envelope(&dm, "UBOT")
            .expect("normalize dm")
            .expect("dm event");
        assert_eq!(dm_event.kind, SlackBridgeEventKind::DirectMessage);
    }

    #[test]
    fn functional_render_event_prompt_includes_downloaded_files() {
        let event = SlackBridgeEvent {
            key: "k1".to_string(),
            kind: SlackBridgeEventKind::AppMention,
            event_id: "Ev1".to_string(),
            occurred_unix_ms: 1,
            channel_id: "C1".to_string(),
            user_id: "U1".to_string(),
            text: "<@UBOT> analyze this".to_string(),
            ts: "1.1".to_string(),
            thread_ts: None,
            files: vec![],
            raw_payload: json!({}),
        };
        let files = vec![DownloadedSlackFile {
            id: "F1".to_string(),
            original_name: "report.txt".to_string(),
            path: Path::new("/tmp/report.txt").to_path_buf(),
            mimetype: Some("text/plain".to_string()),
            size: Some(120),
        }];
        let prompt = render_event_prompt(&event, "UBOT", &files);
        assert!(prompt.contains("Downloaded attachments"));
        assert!(prompt.contains("report.txt"));
        assert!(!prompt.contains("<@UBOT>"));
    }

    #[test]
    fn functional_render_slack_response_thread_splits_long_output() {
        let event = SlackBridgeEvent {
            key: "k1".to_string(),
            kind: SlackBridgeEventKind::DirectMessage,
            event_id: "Ev1".to_string(),
            occurred_unix_ms: 1,
            channel_id: "D1".to_string(),
            user_id: "U1".to_string(),
            text: "hello".to_string(),
            ts: "1.1".to_string(),
            thread_ts: None,
            files: vec![],
            raw_payload: json!({}),
        };
        let run = super::PromptRunReport {
            run_id: "run1".to_string(),
            model: "openai/gpt-4o-mini".to_string(),
            status: crate::PromptRunStatus::Completed,
            assistant_reply: "abcdefghijklmnopqrstuvwxyz".to_string(),
            usage: super::PromptUsageSummary {
                input_tokens: 1,
                output_tokens: 2,
                total_tokens: 3,
                request_duration_ms: 10,
                finish_reason: Some("stop".to_string()),
            },
            downloaded_files: vec![],
        };
        let (summary, detail) = render_slack_response(&event, &run, true, 10);
        assert!(summary.contains("full response posted in this thread"));
        assert_eq!(detail.as_deref(), Some("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn regression_event_is_stale_respects_threshold() {
        let event = SlackBridgeEvent {
            key: "k1".to_string(),
            kind: SlackBridgeEventKind::DirectMessage,
            event_id: "Ev1".to_string(),
            occurred_unix_ms: 1_000,
            channel_id: "D1".to_string(),
            user_id: "U1".to_string(),
            text: "hello".to_string(),
            ts: "1.1".to_string(),
            thread_ts: None,
            files: vec![],
            raw_payload: json!({}),
        };
        assert!(event_is_stale(&event, 1, 4_000));
        assert!(!event_is_stale(&event, 10, 4_000));
    }

    #[test]
    fn regression_state_store_caps_processed_history() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("state.json");
        let mut store = SlackBridgeStateStore::load(state_path, 2).expect("load store");
        assert!(store.mark_processed("a"));
        assert!(store.mark_processed("b"));
        assert!(store.mark_processed("c"));
        assert!(!store.contains("a"));
        assert!(store.contains("b"));
        assert!(store.contains("c"));
    }

    #[tokio::test]
    async fn integration_slack_api_client_retries_rate_limits() {
        let server = MockServer::start();
        let first = server.mock(|when, then| {
            when.method(POST)
                .path("/chat.postMessage")
                .header("x-rsbot-retry-attempt", "0");
            then.status(429)
                .header("retry-after", "0")
                .body("rate limit");
        });
        let second = server.mock(|when, then| {
            when.method(POST)
                .path("/chat.postMessage")
                .header("x-rsbot-retry-attempt", "1");
            then.status(200).json_body(json!({
                "ok": true,
                "channel": "C1",
                "ts": "1.2"
            }));
        });

        let client = SlackApiClient::new(
            server.base_url(),
            "xapp-test".to_string(),
            "xoxb-test".to_string(),
            2_000,
            3,
            1,
        )
        .expect("client");

        let posted = client
            .post_message("C1", "hello", None)
            .await
            .expect("post message eventually succeeds");
        assert_eq!(posted.channel, "C1");
        assert_eq!(posted.ts, "1.2");
        assert_eq!(first.calls(), 1);
        assert_eq!(second.calls(), 1);
    }

    #[tokio::test]
    async fn integration_runtime_queues_per_channel_and_processes_runs() {
        let server = MockServer::start();
        let auth = server.mock(|when, then| {
            when.method(POST).path("/auth.test");
            then.status(200)
                .json_body(json!({"ok": true, "user_id": "UBOT"}));
        });
        let post_working = server.mock(|when, then| {
            when.method(POST)
                .path("/chat.postMessage")
                .body_includes("\"channel\":\"C1\"")
                .body_includes("rsBot is working on run");
            then.status(200)
                .json_body(json!({"ok": true, "channel": "C1", "ts": "2.0"}));
        });
        let post_working_dm = server.mock(|when, then| {
            when.method(POST)
                .path("/chat.postMessage")
                .body_includes("\"channel\":\"D1\"")
                .body_includes("rsBot is working on run");
            then.status(200)
                .json_body(json!({"ok": true, "channel": "D1", "ts": "3.0"}));
        });
        let update = server.mock(|when, then| {
            when.method(POST).path("/chat.update");
            then.status(200)
                .json_body(json!({"ok": true, "channel": "C1", "ts": "2.0"}));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_config(&server.base_url(), temp.path());
        let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");
        auth.assert_calls(0);

        let envelope1 = SlackSocketEnvelope {
            envelope_id: "env1".to_string(),
            envelope_type: "events_api".to_string(),
            payload: json!({
                "type": "event_callback",
                "event_id": "Ev1",
                "event_time": (current_unix_timestamp_ms() / 1000),
                "event": {
                    "type": "app_mention",
                    "user": "U1",
                    "channel": "C1",
                    "text": "<@UBOT> status",
                    "ts": "10.1"
                }
            }),
        };
        let envelope2 = SlackSocketEnvelope {
            envelope_id: "env2".to_string(),
            envelope_type: "events_api".to_string(),
            payload: json!({
                "type": "event_callback",
                "event_id": "Ev2",
                "event_time": (current_unix_timestamp_ms() / 1000),
                "event": {
                    "type": "message",
                    "channel_type": "im",
                    "user": "U2",
                    "channel": "D1",
                    "text": "help",
                    "ts": "10.2"
                }
            }),
        };

        let mut report = PollCycleReport::default();
        runtime
            .handle_envelope(envelope1, &mut report)
            .await
            .expect("handle envelope1");
        runtime
            .handle_envelope(envelope2, &mut report)
            .await
            .expect("handle envelope2");

        runtime
            .try_start_queued_runs(&mut report)
            .await
            .expect("start runs");
        sleep(Duration::from_millis(300)).await;
        runtime
            .drain_finished_runs(&mut report)
            .await
            .expect("drain runs");

        assert!(report.queued_events >= 2);
        assert!(report.completed_runs >= 2);
        assert!(post_working.calls() >= 1);
        assert!(post_working_dm.calls() >= 1);
        assert!(update.calls() >= 1);

        let channel_dir = temp.path().join("channel-store/channels/slack/C1");
        let channel_log =
            std::fs::read_to_string(channel_dir.join("log.jsonl")).expect("channel log exists");
        let channel_context = std::fs::read_to_string(channel_dir.join("context.jsonl"))
            .expect("channel context exists");
        assert!(channel_log.contains("\"direction\":\"inbound\""));
        assert!(channel_log.contains("\"direction\":\"outbound\""));
        assert!(channel_context.contains("slack bridge reply"));
    }

    #[tokio::test]
    async fn regression_duplicate_and_stale_events_do_not_trigger_runs() {
        let server = MockServer::start();
        let post = server.mock(|when, then| {
            when.method(POST).path("/chat.postMessage");
            then.status(200)
                .json_body(json!({"ok": true, "channel": "C1", "ts": "1.1"}));
        });

        let temp = tempdir().expect("tempdir");
        let mut config =
            test_config_with_client(&server.base_url(), temp.path(), Arc::new(SlowReplyClient));
        config.max_event_age_seconds = 5;
        let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

        let now_seconds = current_unix_timestamp_ms() / 1000;
        let fresh = SlackSocketEnvelope {
            envelope_id: "env1".to_string(),
            envelope_type: "events_api".to_string(),
            payload: json!({
                "type": "event_callback",
                "event_id": "EvSame",
                "event_time": now_seconds,
                "event": {
                    "type": "app_mention",
                    "user": "U1",
                    "channel": "C1",
                    "text": "<@UBOT> hello",
                    "ts": "11.1"
                }
            }),
        };
        let stale = SlackSocketEnvelope {
            envelope_id: "env2".to_string(),
            envelope_type: "events_api".to_string(),
            payload: json!({
                "type": "event_callback",
                "event_id": "EvOld",
                "event_time": now_seconds.saturating_sub(15),
                "event": {
                    "type": "app_mention",
                    "user": "U1",
                    "channel": "C1",
                    "text": "<@UBOT> old",
                    "ts": "11.2"
                }
            }),
        };

        let mut report = PollCycleReport::default();
        runtime
            .handle_envelope(fresh.clone(), &mut report)
            .await
            .expect("fresh first");
        runtime
            .handle_envelope(fresh, &mut report)
            .await
            .expect("fresh duplicate");
        runtime
            .handle_envelope(stale, &mut report)
            .await
            .expect("stale event");

        assert_eq!(report.skipped_duplicate_events, 1);
        assert_eq!(report.skipped_stale_events, 1);

        runtime
            .try_start_queued_runs(&mut report)
            .await
            .expect("start queued");
        assert_eq!(post.calls(), 1);
    }
}
