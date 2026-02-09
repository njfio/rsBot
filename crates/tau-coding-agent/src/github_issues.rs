use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tau_ai::LlmClient;
use tokio::sync::watch;

use crate::channel_store::{
    ChannelArtifactRecord, ChannelAttachmentRecord, ChannelLogEntry, ChannelStore,
};
use crate::github_issues_helpers::{
    attachment_filename_from_url, chunk_text_by_chars, evaluate_attachment_content_type_policy,
    evaluate_attachment_url_policy, extract_attachment_urls, split_at_char_index,
};
use crate::github_transport_helpers::{
    is_retryable_github_status, is_retryable_transport_error, parse_retry_after, retry_delay,
    truncate_for_error,
};
use crate::session_commands::{parse_session_search_args, search_session_entries};
use crate::{
    authorize_action_for_principal_with_policy_path, current_unix_timestamp_ms,
    evaluate_pairing_access, execute_canvas_command, github_principal,
    pairing_policy_for_state_dir, rbac_policy_path_for_state_dir, run_prompt_with_cancellation,
    session_message_preview, session_message_role, write_text_atomic, CanvasCommandConfig,
    CanvasEventOrigin, CanvasSessionLinkContext, PairingDecision, PromptRunStatus, RbacDecision,
    RenderOptions, SessionRuntime, TransportHealthSnapshot,
};
use crate::{session::SessionStore, tools::ToolPolicy};

const GITHUB_STATE_SCHEMA_VERSION: u32 = 1;
const GITHUB_COMMENT_MAX_CHARS: usize = 65_000;
const EVENT_KEY_MARKER_PREFIX: &str = "<!-- tau-event-key:";
const LEGACY_EVENT_KEY_MARKER_PREFIX: &str = "<!-- rsbot-event-key:";
const EVENT_KEY_MARKER_SUFFIX: &str = " -->";
const CHAT_SHOW_DEFAULT_LIMIT: usize = 10;
const CHAT_SHOW_MAX_LIMIT: usize = 50;
const CHAT_SEARCH_MAX_LIMIT: usize = 50;
const GITHUB_ATTACHMENT_MAX_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct GithubIssuesBridgeRuntimeConfig {
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
    pub repo_slug: String,
    pub api_base: String,
    pub token: String,
    pub bot_login: Option<String>,
    pub poll_interval: Duration,
    pub include_issue_body: bool,
    pub include_edited_comments: bool,
    pub processed_event_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub artifact_retention_days: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoRef {
    owner: String,
    name: String,
}

impl RepoRef {
    fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        let (owner, name) = trimmed
            .split_once('/')
            .ok_or_else(|| anyhow!("invalid --github-repo '{raw}', expected owner/repo"))?;
        let owner = owner.trim();
        let name = name.trim();
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            bail!("invalid --github-repo '{raw}', expected owner/repo");
        }
        Ok(Self {
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    fn as_slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubIssuesBridgeState {
    schema_version: u32,
    #[serde(default)]
    last_issue_scan_at: Option<String>,
    #[serde(default)]
    processed_event_keys: Vec<String>,
    #[serde(default)]
    issue_sessions: BTreeMap<String, GithubIssueChatSessionState>,
    #[serde(default)]
    health: TransportHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubIssueChatSessionState {
    session_id: String,
    #[serde(default)]
    last_comment_id: Option<u64>,
    #[serde(default)]
    last_run_id: Option<String>,
}

impl Default for GithubIssuesBridgeState {
    fn default() -> Self {
        Self {
            schema_version: GITHUB_STATE_SCHEMA_VERSION,
            last_issue_scan_at: None,
            processed_event_keys: Vec::new(),
            issue_sessions: BTreeMap::new(),
            health: TransportHealthSnapshot::default(),
        }
    }
}

struct GithubIssuesBridgeStateStore {
    path: PathBuf,
    cap: usize,
    state: GithubIssuesBridgeState,
    processed_index: HashSet<String>,
}

impl GithubIssuesBridgeStateStore {
    fn load(path: PathBuf, cap: usize) -> Result<Self> {
        let mut state = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read state file {}", path.display()))?;
            match serde_json::from_str::<GithubIssuesBridgeState>(&raw) {
                Ok(state) => state,
                Err(error) => {
                    eprintln!(
                        "failed to parse github issues bridge state file {}: {} (starting fresh)",
                        path.display(),
                        error
                    );
                    GithubIssuesBridgeState::default()
                }
            }
        } else {
            GithubIssuesBridgeState::default()
        };

        if state.schema_version != GITHUB_STATE_SCHEMA_VERSION {
            eprintln!(
                "unsupported github issues bridge state schema: expected {}, found {} (starting fresh)",
                GITHUB_STATE_SCHEMA_VERSION,
                state.schema_version
            );
            state = GithubIssuesBridgeState::default();
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

    fn issue_session(&self, issue_number: u64) -> Option<&GithubIssueChatSessionState> {
        self.state.issue_sessions.get(&issue_number.to_string())
    }

    fn update_issue_session(
        &mut self,
        issue_number: u64,
        session_id: String,
        last_comment_id: Option<u64>,
        last_run_id: Option<String>,
    ) -> bool {
        let key = issue_number.to_string();
        let entry =
            self.state
                .issue_sessions
                .entry(key)
                .or_insert_with(|| GithubIssueChatSessionState {
                    session_id: session_id.clone(),
                    last_comment_id: None,
                    last_run_id: None,
                });
        let mut changed = false;
        if entry.session_id != session_id {
            entry.session_id = session_id;
            changed = true;
        }
        if let Some(comment_id) = last_comment_id {
            if entry.last_comment_id != Some(comment_id) {
                entry.last_comment_id = Some(comment_id);
                changed = true;
            }
        }
        if let Some(run_id) = last_run_id {
            if entry.last_run_id.as_deref() != Some(run_id.as_str()) {
                entry.last_run_id = Some(run_id);
                changed = true;
            }
        }
        changed
    }

    fn clear_issue_session(&mut self, issue_number: u64) -> bool {
        self.state
            .issue_sessions
            .remove(&issue_number.to_string())
            .is_some()
    }

    fn last_issue_scan_at(&self) -> Option<&str> {
        self.state.last_issue_scan_at.as_deref()
    }

    fn update_last_issue_scan_at(&mut self, value: Option<String>) -> bool {
        if self.state.last_issue_scan_at == value {
            return false;
        }
        self.state.last_issue_scan_at = value;
        true
    }

    fn transport_health(&self) -> &TransportHealthSnapshot {
        &self.state.health
    }

    fn update_transport_health(&mut self, value: TransportHealthSnapshot) -> bool {
        if self.state.health == value {
            return false;
        }
        self.state.health = value;
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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GithubUser {
    login: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GithubIssue {
    id: u64,
    number: u64,
    title: String,
    body: Option<String>,
    created_at: String,
    updated_at: String,
    user: GithubUser,
    #[serde(default)]
    pull_request: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GithubIssueComment {
    id: u64,
    body: Option<String>,
    created_at: String,
    updated_at: String,
    user: GithubUser,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubCommentCreateResponse {
    id: u64,
    html_url: Option<String>,
}

#[derive(Debug, Clone)]
struct GithubBytesResponse {
    bytes: Vec<u8>,
    content_type: Option<String>,
}

#[derive(Clone)]
struct GithubApiClient {
    http: reqwest::Client,
    api_base: String,
    repo: RepoRef,
    retry_max_attempts: usize,
    retry_base_delay_ms: u64,
}

impl GithubApiClient {
    fn new(
        api_base: String,
        token: String,
        repo: RepoRef,
        request_timeout_ms: u64,
        retry_max_attempts: usize,
        retry_base_delay_ms: u64,
    ) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Tau-github-issues-bridge"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            reqwest::header::HeaderValue::from_static("2022-11-28"),
        );
        let auth_header = format!("Bearer {}", token.trim());
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&auth_header)
                .context("invalid github authorization header")?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(request_timeout_ms.max(1)))
            .build()
            .context("failed to create github api client")?;
        Ok(Self {
            http: client,
            api_base: api_base.trim_end_matches('/').to_string(),
            repo,
            retry_max_attempts: retry_max_attempts.max(1),
            retry_base_delay_ms: retry_base_delay_ms.max(1),
        })
    }

    async fn resolve_bot_login(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Viewer {
            login: String,
        }

        let viewer: Viewer = self
            .request_json("resolve bot login", || {
                self.http.get(format!("{}/user", self.api_base))
            })
            .await?;
        Ok(viewer.login)
    }

    async fn list_updated_issues(&self, since: Option<&str>) -> Result<Vec<GithubIssue>> {
        let mut page = 1_u32;
        let mut rows = Vec::new();
        loop {
            let mut request = self.http.get(format!(
                "{}/repos/{}/{}/issues",
                self.api_base, self.repo.owner, self.repo.name
            ));
            request = request.query(&[
                ("state", "open"),
                ("sort", "updated"),
                ("direction", "asc"),
                ("per_page", "100"),
                ("page", &page.to_string()),
            ]);
            if let Some(since_value) = since {
                request = request.query(&[("since", since_value)]);
            }
            let chunk: Vec<GithubIssue> = self
                .request_json("list issues", || {
                    request.try_clone().expect("cloned request")
                })
                .await?;
            let chunk_len = chunk.len();
            rows.extend(
                chunk
                    .into_iter()
                    .filter(|issue| issue.pull_request.is_none()),
            );
            if chunk_len < 100 {
                break;
            }
            page = page.saturating_add(1);
        }
        Ok(rows)
    }

    async fn list_issue_comments(&self, issue_number: u64) -> Result<Vec<GithubIssueComment>> {
        let mut page = 1_u32;
        let mut rows = Vec::new();
        loop {
            let request = self
                .http
                .get(format!(
                    "{}/repos/{}/{}/issues/{}/comments",
                    self.api_base, self.repo.owner, self.repo.name, issue_number
                ))
                .query(&[
                    ("sort", "created"),
                    ("direction", "asc"),
                    ("per_page", "100"),
                    ("page", &page.to_string()),
                ]);
            let chunk: Vec<GithubIssueComment> = self
                .request_json("list issue comments", || {
                    request.try_clone().expect("cloned request")
                })
                .await?;
            let chunk_len = chunk.len();
            rows.extend(chunk);
            if chunk_len < 100 {
                break;
            }
            page = page.saturating_add(1);
        }
        Ok(rows)
    }

    async fn create_issue_comment(
        &self,
        issue_number: u64,
        body: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let payload = json!({ "body": body });
        self.request_json("create issue comment", || {
            self.http
                .post(format!(
                    "{}/repos/{}/{}/issues/{}/comments",
                    self.api_base, self.repo.owner, self.repo.name, issue_number
                ))
                .json(&payload)
        })
        .await
    }

    async fn update_issue_comment(
        &self,
        comment_id: u64,
        body: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let payload = json!({ "body": body });
        self.request_json("update issue comment", || {
            self.http
                .patch(format!(
                    "{}/repos/{}/{}/issues/comments/{}",
                    self.api_base, self.repo.owner, self.repo.name, comment_id
                ))
                .json(&payload)
        })
        .await
    }

    async fn download_url_bytes(&self, url: &str) -> Result<GithubBytesResponse> {
        let request = || self.http.get(url);
        self.request_bytes("download issue attachment", request)
            .await
    }

    async fn request_bytes<F>(
        &self,
        operation: &str,
        mut request_builder: F,
    ) -> Result<GithubBytesResponse>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = request_builder()
                .header("x-tau-retry-attempt", attempt.saturating_sub(1).to_string())
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let content_type = response
                            .headers()
                            .get(reqwest::header::CONTENT_TYPE)
                            .and_then(|value| value.to_str().ok())
                            .map(|value| value.to_string());
                        let bytes = response
                            .bytes()
                            .await
                            .with_context(|| format!("failed to read github {operation} body"))?;
                        return Ok(GithubBytesResponse {
                            bytes: bytes.to_vec(),
                            content_type,
                        });
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let body = response.text().await.unwrap_or_default();
                    if attempt < self.retry_max_attempts
                        && is_retryable_github_status(status.as_u16())
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
                        "github api {operation} failed with status {}: {}",
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
                        .with_context(|| format!("github api {operation} request failed"));
                }
            }
        }
    }

    async fn request_json<T, F>(&self, operation: &str, mut request_builder: F) -> Result<T>
    where
        T: DeserializeOwned,
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = request_builder()
                .header("x-tau-retry-attempt", attempt.saturating_sub(1).to_string())
                .send()
                .await;
            match response {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let parsed = response
                            .json::<T>()
                            .await
                            .with_context(|| format!("failed to decode github {operation}"))?;
                        return Ok(parsed);
                    }

                    let retry_after = parse_retry_after(response.headers());
                    let body = response.text().await.unwrap_or_default();
                    if attempt < self.retry_max_attempts
                        && is_retryable_github_status(status.as_u16())
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
                        "github api {operation} failed with status {}: {}",
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
                        .with_context(|| format!("github api {operation} request failed"));
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum GithubBridgeEventKind {
    Opened,
    CommentCreated,
    CommentEdited,
}

impl GithubBridgeEventKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Opened => "issue_opened",
            Self::CommentCreated => "issue_comment_created",
            Self::CommentEdited => "issue_comment_edited",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GithubBridgeEvent {
    key: String,
    kind: GithubBridgeEventKind,
    issue_number: u64,
    issue_title: String,
    author_login: String,
    occurred_at: String,
    body: String,
    raw_payload: Value,
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
    downloaded_attachments: Vec<DownloadedGithubAttachment>,
    artifact: ChannelArtifactRecord,
}

struct RunPromptForEventRequest<'a> {
    config: &'a GithubIssuesBridgeRuntimeConfig,
    github_client: &'a GithubApiClient,
    repo: &'a RepoRef,
    repository_state_dir: &'a Path,
    event: &'a GithubBridgeEvent,
    prompt: &'a str,
    run_id: &'a str,
    cancel_rx: watch::Receiver<bool>,
}

#[derive(Debug, Clone)]
struct DownloadedGithubAttachment {
    source_url: String,
    original_name: String,
    path: PathBuf,
    relative_path: String,
    content_type: Option<String>,
    bytes: u64,
    checksum_sha256: String,
    policy_reason_code: String,
    created_unix_ms: u64,
    expires_unix_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct CommentUpdateOutcome {
    posted_comment_id: Option<u64>,
    edit_attempted: bool,
    edit_success: bool,
    append_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TauIssueCommand {
    Run {
        prompt: String,
    },
    Stop,
    Status,
    Health,
    Compact,
    Help,
    ChatStart,
    ChatResume,
    ChatReset,
    ChatExport,
    ChatStatus,
    ChatShow {
        limit: usize,
    },
    ChatSearch {
        query: String,
        role: Option<String>,
        limit: usize,
    },
    Artifacts {
        purge: bool,
        run_id: Option<String>,
    },
    ArtifactShow {
        artifact_id: String,
    },
    Canvas {
        args: String,
    },
    Summarize {
        focus: Option<String>,
    },
    Invalid {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventAction {
    RunPrompt { prompt: String },
    Command(TauIssueCommand),
}

#[derive(Debug)]
struct ActiveIssueRun {
    run_id: String,
    event_key: String,
    started_unix_ms: u64,
    started: Instant,
    cancel_tx: watch::Sender<bool>,
    handle: tokio::task::JoinHandle<RunTaskResult>,
}

#[derive(Debug, Clone)]
struct IssueLatestRun {
    run_id: String,
    event_key: String,
    status: String,
    started_unix_ms: u64,
    completed_unix_ms: u64,
    duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IssueArtifactSummary {
    total_records: usize,
    active_records: usize,
    latest_artifact_id: Option<String>,
    latest_artifact_run_id: Option<String>,
    latest_artifact_created_unix_ms: Option<u64>,
    invalid_index_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IssueChatContinuitySummary {
    entries: usize,
    head_id: Option<u64>,
    oldest_entry_id: Option<u64>,
    newest_entry_id: Option<u64>,
    newest_entry_role: Option<String>,
    lineage_digest_sha256: String,
    artifacts: IssueArtifactSummary,
}

#[derive(Debug)]
struct RunTaskResult {
    issue_number: u64,
    event_key: String,
    run_id: String,
    started_unix_ms: u64,
    completed_unix_ms: u64,
    duration_ms: u64,
    status: String,
    posted_comment_id: Option<u64>,
    comment_edit_attempted: bool,
    comment_edit_success: bool,
    comment_append_count: usize,
    model: String,
    usage: PromptUsageSummary,
    error: Option<String>,
}

struct IssueRunTaskParams {
    github_client: GithubApiClient,
    config: GithubIssuesBridgeRuntimeConfig,
    repo: RepoRef,
    repository_state_dir: PathBuf,
    event: GithubBridgeEvent,
    prompt: String,
    run_id: String,
    working_comment_id: u64,
    cancel_rx: watch::Receiver<bool>,
    started_unix_ms: u64,
}

#[derive(Debug, Default)]
pub(crate) struct PollCycleReport {
    pub discovered_events: usize,
    pub processed_events: usize,
    pub skipped_duplicate_events: usize,
    pub failed_events: usize,
}

pub(crate) async fn run_github_issues_bridge(
    config: GithubIssuesBridgeRuntimeConfig,
) -> Result<()> {
    let mut runtime = GithubIssuesBridgeRuntime::new(config).await?;
    runtime.run().await
}

struct GithubIssuesBridgeRuntime {
    config: GithubIssuesBridgeRuntimeConfig,
    repo: RepoRef,
    github_client: GithubApiClient,
    state_store: GithubIssuesBridgeStateStore,
    inbound_log: JsonlEventLog,
    outbound_log: JsonlEventLog,
    bot_login: String,
    repository_state_dir: PathBuf,
    active_runs: HashMap<u64, ActiveIssueRun>,
    latest_runs: HashMap<u64, IssueLatestRun>,
}

impl GithubIssuesBridgeRuntime {
    async fn new(config: GithubIssuesBridgeRuntimeConfig) -> Result<Self> {
        let repo = RepoRef::parse(&config.repo_slug)?;
        let github_client = GithubApiClient::new(
            config.api_base.clone(),
            config.token.clone(),
            repo.clone(),
            config.request_timeout_ms,
            config.retry_max_attempts,
            config.retry_base_delay_ms,
        )?;
        let bot_login = match config.bot_login.clone() {
            Some(login) if !login.trim().is_empty() => login.trim().to_string(),
            _ => github_client.resolve_bot_login().await?,
        };
        let repository_state_dir = config
            .state_dir
            .join(sanitize_for_path(&format!("{}__{}", repo.owner, repo.name)));
        std::fs::create_dir_all(&repository_state_dir)
            .with_context(|| format!("failed to create {}", repository_state_dir.display()))?;

        let state_store = GithubIssuesBridgeStateStore::load(
            repository_state_dir.join("state.json"),
            config.processed_event_cap,
        )?;
        let inbound_log = JsonlEventLog::open(repository_state_dir.join("inbound-events.jsonl"))?;
        let outbound_log = JsonlEventLog::open(repository_state_dir.join("outbound-events.jsonl"))?;
        Ok(Self {
            config,
            repo,
            github_client,
            state_store,
            inbound_log,
            outbound_log,
            bot_login,
            repository_state_dir,
            active_runs: HashMap::new(),
            latest_runs: HashMap::new(),
        })
    }

    async fn run(&mut self) -> Result<()> {
        let mut failure_streak = self.state_store.transport_health().failure_streak;
        loop {
            let cycle_started = Instant::now();
            match self.poll_once().await {
                Ok(report) => {
                    failure_streak = 0;
                    println!(
                        "github bridge poll: repo={} discovered={} processed={} duplicate_skips={} failed={}",
                        self.repo.as_slug(),
                        report.discovered_events,
                        report.processed_events,
                        report.skipped_duplicate_events,
                        report.failed_events
                    );
                }
                Err(error) => {
                    failure_streak = failure_streak.saturating_add(1);
                    let duration_ms = cycle_started.elapsed().as_millis() as u64;
                    let snapshot = self.build_transport_health_snapshot(
                        &PollCycleReport::default(),
                        duration_ms,
                        failure_streak,
                    );
                    if self.state_store.update_transport_health(snapshot) {
                        self.state_store.save()?;
                    }
                    eprintln!("github bridge poll error: {error}");
                }
            }

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    println!("github bridge shutdown requested");
                    return Ok(());
                }
                _ = tokio::time::sleep(self.config.poll_interval) => {}
            }
        }
    }

    async fn poll_once(&mut self) -> Result<PollCycleReport> {
        let cycle_started = Instant::now();
        let mut report = PollCycleReport::default();
        let mut state_dirty = false;
        tokio::task::yield_now().await;
        self.drain_finished_runs(&mut report, &mut state_dirty)
            .await?;

        let issues = self
            .github_client
            .list_updated_issues(self.state_store.last_issue_scan_at())
            .await?;
        let mut latest_issue_scan = self.state_store.last_issue_scan_at().map(str::to_string);

        for issue in issues {
            latest_issue_scan = match latest_issue_scan {
                Some(existing) if existing >= issue.updated_at => Some(existing),
                _ => Some(issue.updated_at.clone()),
            };

            let comments = self.github_client.list_issue_comments(issue.number).await?;
            let known_event_keys = comments
                .iter()
                .filter(|comment| comment.user.login == self.bot_login)
                .flat_map(|comment| {
                    comment
                        .body
                        .as_deref()
                        .map(extract_footer_event_keys)
                        .unwrap_or_default()
                })
                .collect::<HashSet<_>>();

            for key in &known_event_keys {
                if self.state_store.mark_processed(key) {
                    state_dirty = true;
                }
            }

            let events = collect_issue_events(
                &issue,
                &comments,
                &self.bot_login,
                self.config.include_issue_body,
                self.config.include_edited_comments,
            );
            report.discovered_events = report.discovered_events.saturating_add(events.len());

            for event in events {
                if self.state_store.contains(&event.key) || known_event_keys.contains(&event.key) {
                    report.skipped_duplicate_events =
                        report.skipped_duplicate_events.saturating_add(1);
                    continue;
                }

                let action = event_action_from_body(&event.body);
                let policy_channel = format!("github:{}", self.repo.as_slug());
                let pairing_policy = pairing_policy_for_state_dir(&self.config.state_dir);
                let pairing_decision = evaluate_pairing_access(
                    &pairing_policy,
                    &policy_channel,
                    &event.author_login,
                    current_unix_timestamp_ms(),
                )?;
                let pairing_status = if matches!(pairing_decision, PairingDecision::Allow { .. }) {
                    "allow"
                } else {
                    "deny"
                };
                let pairing_reason_code = pairing_decision.reason_code().to_string();
                self.inbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key.clone(),
                    "kind": event.kind.as_str(),
                    "issue_number": event.issue_number,
                    "action": format!("{action:?}"),
                    "pairing": {
                        "decision": pairing_status,
                        "reason_code": pairing_reason_code,
                        "channel": policy_channel,
                        "actor_id": event.author_login,
                    },
                    "payload": event.raw_payload,
                }))?;

                if let PairingDecision::Deny { reason_code } = pairing_decision {
                    self.append_channel_log(
                        &event,
                        "inbound",
                        json!({
                            "kind": event.kind.as_str(),
                            "author_login": event.author_login,
                            "body": event.body,
                            "action": format!("{action:?}"),
                            "pairing": {
                                "decision": "deny",
                                "reason_code": reason_code,
                                "channel": policy_channel,
                            },
                        }),
                    )?;
                    self.outbound_log.append(&json!({
                        "timestamp_unix_ms": current_unix_timestamp_ms(),
                        "repo": self.repo.as_slug(),
                        "event_key": event.key.clone(),
                        "issue_number": event.issue_number,
                        "command": "authorization",
                        "status": "denied",
                        "reason_code": reason_code,
                        "channel": policy_channel,
                        "actor_id": event.author_login,
                    }))?;
                    if self.state_store.mark_processed(&event.key) {
                        state_dirty = true;
                    }
                    report.processed_events = report.processed_events.saturating_add(1);
                    eprintln!(
                        "github bridge event denied: repo={} issue=#{} key={} actor={} channel={} reason_code={}",
                        self.repo.as_slug(),
                        event.issue_number,
                        event.key,
                        event.author_login,
                        policy_channel,
                        reason_code
                    );
                    continue;
                }

                let rbac_principal = github_principal(&event.author_login);
                let rbac_action = rbac_action_for_event(&action);
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
                        self.append_channel_log(
                            &event,
                            "inbound",
                            json!({
                                "kind": event.kind.as_str(),
                                "author_login": event.author_login,
                                "body": event.body,
                                "action": format!("{action:?}"),
                                "rbac": {
                                    "decision": "deny",
                                    "reason_code": reason_code,
                                    "matched_role": matched_role,
                                    "matched_pattern": matched_pattern,
                                    "principal": rbac_principal,
                                    "action": rbac_action,
                                },
                            }),
                        )?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key.clone(),
                            "issue_number": event.issue_number,
                            "command": "rbac-authorization",
                            "status": "denied",
                            "reason_code": reason_code,
                            "matched_role": matched_role,
                            "matched_pattern": matched_pattern,
                            "principal": rbac_principal,
                            "action": rbac_action,
                            "actor_id": event.author_login,
                        }))?;
                        if self.state_store.mark_processed(&event.key) {
                            state_dirty = true;
                        }
                        report.processed_events = report.processed_events.saturating_add(1);
                        continue;
                    }
                    Err(error) => {
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key.clone(),
                            "issue_number": event.issue_number,
                            "command": "rbac-authorization",
                            "status": "error",
                            "reason_code": "rbac_policy_error",
                            "principal": rbac_principal,
                            "action": rbac_action,
                            "actor_id": event.author_login,
                            "error": error.to_string(),
                        }))?;
                        if self.state_store.mark_processed(&event.key) {
                            state_dirty = true;
                        }
                        report.failed_events = report.failed_events.saturating_add(1);
                        continue;
                    }
                }

                if let Err(error) = self
                    .handle_event_action(&event, action, &mut report, &mut state_dirty)
                    .await
                {
                    report.failed_events = report.failed_events.saturating_add(1);
                    eprintln!(
                        "github bridge event failed: repo={} issue=#{} key={} error={error}",
                        self.repo.as_slug(),
                        event.issue_number,
                        event.key
                    );
                }
            }
        }

        self.drain_finished_runs(&mut report, &mut state_dirty)
            .await?;

        if self
            .state_store
            .update_last_issue_scan_at(latest_issue_scan)
        {
            state_dirty = true;
        }
        let duration_ms = cycle_started.elapsed().as_millis() as u64;
        let snapshot = self.build_transport_health_snapshot(&report, duration_ms, 0);
        if self.state_store.update_transport_health(snapshot) {
            state_dirty = true;
        }
        if state_dirty {
            self.state_store.save()?;
        }
        Ok(report)
    }

    fn build_transport_health_snapshot(
        &self,
        report: &PollCycleReport,
        cycle_duration_ms: u64,
        failure_streak: usize,
    ) -> TransportHealthSnapshot {
        TransportHealthSnapshot {
            updated_unix_ms: current_unix_timestamp_ms(),
            cycle_duration_ms,
            queue_depth: 0,
            active_runs: self.active_runs.len(),
            failure_streak,
            last_cycle_discovered: report.discovered_events,
            last_cycle_processed: report.processed_events,
            last_cycle_completed: report.processed_events.saturating_sub(report.failed_events),
            last_cycle_failed: report.failed_events,
            last_cycle_duplicates: report.skipped_duplicate_events,
        }
    }

    async fn drain_finished_runs(
        &mut self,
        report: &mut PollCycleReport,
        state_dirty: &mut bool,
    ) -> Result<()> {
        let finished_issues = self
            .active_runs
            .iter()
            .filter_map(|(issue_number, run)| run.handle.is_finished().then_some(*issue_number))
            .collect::<Vec<_>>();

        for issue_number in finished_issues {
            let Some(active) = self.active_runs.remove(&issue_number) else {
                continue;
            };
            match active.handle.await {
                Ok(result) => {
                    self.latest_runs.insert(
                        issue_number,
                        IssueLatestRun {
                            run_id: result.run_id.clone(),
                            event_key: result.event_key.clone(),
                            status: result.status.clone(),
                            started_unix_ms: result.started_unix_ms,
                            completed_unix_ms: result.completed_unix_ms,
                            duration_ms: result.duration_ms,
                        },
                    );
                    if self.state_store.update_issue_session(
                        result.issue_number,
                        issue_session_id(result.issue_number),
                        result.posted_comment_id,
                        Some(result.run_id.clone()),
                    ) {
                        *state_dirty = true;
                    }
                    self.outbound_log.append(&json!({
                        "timestamp_unix_ms": current_unix_timestamp_ms(),
                        "repo": self.repo.as_slug(),
                        "event_key": result.event_key,
                        "issue_number": result.issue_number,
                        "run_id": result.run_id,
                        "status": result.status,
                        "posted_comment_id": result.posted_comment_id,
                        "comment_update": {
                            "edit_attempted": result.comment_edit_attempted,
                            "edit_success": result.comment_edit_success,
                            "append_count": result.comment_append_count,
                        },
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
                    if result.error.is_some() {
                        report.failed_events = report.failed_events.saturating_add(1);
                    }
                }
                Err(error) => {
                    report.failed_events = report.failed_events.saturating_add(1);
                    eprintln!(
                        "github bridge run join failed: repo={} issue=#{} run_id={} key={} error={error}",
                        self.repo.as_slug(),
                        issue_number,
                        active.run_id,
                        active.event_key
                    );
                }
            }
        }

        Ok(())
    }

    async fn handle_event_action(
        &mut self,
        event: &GithubBridgeEvent,
        action: EventAction,
        report: &mut PollCycleReport,
        state_dirty: &mut bool,
    ) -> Result<()> {
        self.append_channel_log(
            event,
            "inbound",
            json!({
                "kind": event.kind.as_str(),
                "author_login": event.author_login,
                "body": event.body,
                "action": format!("{action:?}"),
            }),
        )?;
        match action {
            EventAction::RunPrompt { prompt } => {
                self.enqueue_issue_run(event, prompt, report, state_dirty)
                    .await
            }
            EventAction::Command(command) => {
                self.execute_issue_command(event, command, report, state_dirty)
                    .await
            }
        }
    }

    async fn enqueue_issue_run(
        &mut self,
        event: &GithubBridgeEvent,
        prompt: String,
        report: &mut PollCycleReport,
        state_dirty: &mut bool,
    ) -> Result<()> {
        if self.active_runs.contains_key(&event.issue_number) {
            let status_text = self.render_issue_status(event.issue_number);
            let posted = self
                .github_client
                .create_issue_comment(
                    event.issue_number,
                    &format!(
                        "A run is already active for this issue.\n\n{}\n\nUse `/tau stop` to cancel it first.",
                        status_text
                    ),
                )
                .await?;
            self.outbound_log.append(&json!({
                "timestamp_unix_ms": current_unix_timestamp_ms(),
                "repo": self.repo.as_slug(),
                "event_key": event.key,
                "issue_number": event.issue_number,
                "command": "run",
                "status": "rejected_active_run",
                "posted_comment_id": posted.id,
                "posted_comment_url": posted.html_url,
            }))?;
            if self.state_store.mark_processed(&event.key) {
                *state_dirty = true;
            }
            report.processed_events = report.processed_events.saturating_add(1);
            return Ok(());
        }

        let run_id = format!(
            "gh-{}-{}-{}",
            event.issue_number,
            current_unix_timestamp_ms(),
            short_key_hash(&event.key)
        );
        let started_unix_ms = current_unix_timestamp_ms();
        let working_comment = self
            .github_client
            .create_issue_comment(
                event.issue_number,
                &format!(
                    "⏳ Tau is working on run `{}` for event `{}`.",
                    run_id, event.key
                ),
            )
            .await?;
        let working_comment_id = working_comment.id;

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let github_client = self.github_client.clone();
        let repo = self.repo.clone();
        let event_clone = event.clone();
        let repository_state_dir = self.repository_state_dir.clone();
        let config = self.config.clone();
        let run_id_for_task = run_id.clone();
        let prompt_for_task = prompt.clone();
        let handle = tokio::spawn(async move {
            execute_issue_run_task(IssueRunTaskParams {
                github_client,
                config,
                repo,
                repository_state_dir,
                event: event_clone,
                prompt: prompt_for_task,
                run_id: run_id_for_task,
                working_comment_id,
                cancel_rx,
                started_unix_ms,
            })
            .await
        });
        self.active_runs.insert(
            event.issue_number,
            ActiveIssueRun {
                run_id: run_id.clone(),
                event_key: event.key.clone(),
                started_unix_ms,
                started: Instant::now(),
                cancel_tx,
                handle,
            },
        );
        if self.state_store.update_issue_session(
            event.issue_number,
            issue_session_id(event.issue_number),
            Some(working_comment_id),
            Some(run_id.clone()),
        ) {
            *state_dirty = true;
        }
        if self.state_store.mark_processed(&event.key) {
            *state_dirty = true;
        }
        report.processed_events = report.processed_events.saturating_add(1);
        self.outbound_log.append(&json!({
            "timestamp_unix_ms": current_unix_timestamp_ms(),
            "repo": self.repo.as_slug(),
            "event_key": event.key,
            "issue_number": event.issue_number,
            "run_id": run_id,
            "status": "run_started",
            "working_comment_id": working_comment_id,
        }))?;
        Ok(())
    }

    async fn execute_issue_command(
        &mut self,
        event: &GithubBridgeEvent,
        command: TauIssueCommand,
        report: &mut PollCycleReport,
        state_dirty: &mut bool,
    ) -> Result<()> {
        match command {
            TauIssueCommand::Run { prompt } => {
                return self
                    .enqueue_issue_run(event, prompt, report, state_dirty)
                    .await;
            }
            TauIssueCommand::Summarize { focus } => {
                let prompt = build_summarize_prompt(&self.repo, event, focus.as_deref());
                return self
                    .enqueue_issue_run(event, prompt, report, state_dirty)
                    .await;
            }
            TauIssueCommand::Stop => {
                let message = if let Some(active) = self.active_runs.get(&event.issue_number) {
                    if *active.cancel_tx.borrow() {
                        format!(
                            "Stop has already been requested for run `{}`.",
                            active.run_id
                        )
                    } else {
                        let _ = active.cancel_tx.send(true);
                        format!(
                            "Cancellation requested for run `{}` (event `{}`).",
                            active.run_id, active.event_key
                        )
                    }
                } else {
                    "No active run for this issue. Current state is idle.".to_string()
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "stop",
                        "acknowledged",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "stop",
                    "status": "acknowledged",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::Status => {
                let status = self.render_issue_status(event.issue_number);
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "status",
                        "reported",
                        &status,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "status",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::Health => {
                let health = self.render_issue_health(event.issue_number);
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "health",
                        "reported",
                        &health,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "health",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::Artifacts { purge, run_id } => {
                let artifact_report = if purge {
                    self.render_issue_artifact_purge(event.issue_number)?
                } else {
                    self.render_issue_artifacts(event.issue_number, run_id.as_deref())?
                };
                let command_name = if purge {
                    "artifacts-purge"
                } else if run_id.is_some() {
                    "artifacts-run"
                } else {
                    "artifacts"
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        command_name,
                        "reported",
                        &artifact_report,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": command_name,
                    "status": "reported",
                    "artifact_run_id": run_id,
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::ArtifactShow { artifact_id } => {
                let artifact_report =
                    self.render_issue_artifact_show(event.issue_number, &artifact_id)?;
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "artifacts-show",
                        "reported",
                        &artifact_report,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "artifacts-show",
                    "status": "reported",
                    "artifact_id": artifact_id,
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::Canvas { args } => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let session_head_id = SessionStore::load(&session_path)
                    .ok()
                    .and_then(|store| store.head_id());
                let output = execute_canvas_command(
                    &args,
                    &CanvasCommandConfig {
                        canvas_root: self.repository_state_dir.join("canvas"),
                        channel_store_root: self.repository_state_dir.join("channel-store"),
                        principal: github_principal(&event.author_login),
                        origin: CanvasEventOrigin {
                            transport: "github".to_string(),
                            channel: Some(issue_session_id(event.issue_number)),
                            source_event_key: Some(event.key.clone()),
                            source_unix_ms: parse_rfc3339_to_unix_ms(&event.occurred_at),
                        },
                        session_link: Some(CanvasSessionLinkContext {
                            session_path,
                            session_head_id,
                        }),
                    },
                );
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "canvas",
                        "reported",
                        &output,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "canvas",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "canvas_args": args,
                }))?;
            }
            TauIssueCommand::Compact => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let compact_report = compact_issue_session(
                    &session_path,
                    self.config.session_lock_wait_ms,
                    self.config.session_lock_stale_ms,
                )?;
                if self.state_store.clear_issue_session(event.issue_number) {
                    *state_dirty = true;
                }
                let compact_message = format!(
                    "Session compact complete for issue #{}.\n\nremoved_entries={} retained_entries={} head={}",
                    event.issue_number,
                    compact_report.removed_entries,
                    compact_report.retained_entries,
                    compact_report
                        .head_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "none".to_string())
                );
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "compact",
                        "completed",
                        &compact_message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "compact",
                    "status": "completed",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "compact_report": {
                        "removed_entries": compact_report.removed_entries,
                        "retained_entries": compact_report.retained_entries,
                        "head_id": compact_report.head_id,
                    }
                }))?;
            }
            TauIssueCommand::Help => {
                let message = tau_command_usage();
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "help",
                        "reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "help",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::ChatStart => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let (before_entries, after_entries, head_id) = ensure_issue_session_initialized(
                    &session_path,
                    &self.config.system_prompt,
                    self.config.session_lock_wait_ms,
                    self.config.session_lock_stale_ms,
                )?;
                let message = if before_entries == 0 {
                    format!(
                        "Chat session started for issue #{}.\n\nentries={} head={}",
                        event.issue_number,
                        after_entries,
                        head_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    )
                } else {
                    format!(
                        "Chat session already initialized for issue #{}.\n\nentries={} head={}",
                        event.issue_number,
                        after_entries,
                        head_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    )
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-start",
                        "completed",
                        &message,
                    )
                    .await?;
                if self.state_store.update_issue_session(
                    event.issue_number,
                    issue_session_id(event.issue_number),
                    Some(posted.id),
                    None,
                ) {
                    *state_dirty = true;
                }
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-start",
                    "status": "completed",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "session": {
                        "entries_before": before_entries,
                        "entries_after": after_entries,
                        "head_id": head_id,
                    }
                }))?;
            }
            TauIssueCommand::ChatResume => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let (before_entries, after_entries, head_id) = ensure_issue_session_initialized(
                    &session_path,
                    &self.config.system_prompt,
                    self.config.session_lock_wait_ms,
                    self.config.session_lock_stale_ms,
                )?;
                let message = if before_entries == 0 {
                    format!(
                        "No existing chat session found for issue #{}.\nStarted a new session with entries={} head={}.",
                        event.issue_number,
                        after_entries,
                        head_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    )
                } else {
                    format!(
                        "Chat session resumed for issue #{}.\n\nentries={} head={}",
                        event.issue_number,
                        after_entries,
                        head_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    )
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-resume",
                        "completed",
                        &message,
                    )
                    .await?;
                if self.state_store.update_issue_session(
                    event.issue_number,
                    issue_session_id(event.issue_number),
                    Some(posted.id),
                    None,
                ) {
                    *state_dirty = true;
                }
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-resume",
                    "status": "completed",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "session": {
                        "entries_before": before_entries,
                        "entries_after": after_entries,
                        "head_id": head_id,
                    }
                }))?;
            }
            TauIssueCommand::ChatReset => {
                if let Some(active) = self.active_runs.get(&event.issue_number) {
                    let message = format!(
                        "Cannot reset chat while run `{}` is active. Use `/tau stop` first.",
                        active.run_id
                    );
                    let posted = self
                        .post_issue_command_comment(
                            event.issue_number,
                            &event.key,
                            "chat-reset",
                            "blocked",
                            &message,
                        )
                        .await?;
                    self.outbound_log.append(&json!({
                        "timestamp_unix_ms": current_unix_timestamp_ms(),
                        "repo": self.repo.as_slug(),
                        "event_key": event.key,
                        "issue_number": event.issue_number,
                        "command": "chat-reset",
                        "status": "blocked",
                        "posted_comment_id": posted.id,
                        "posted_comment_url": posted.html_url,
                        "active_run_id": active.run_id,
                    }))?;
                } else {
                    let session_path =
                        session_path_for_issue(&self.repository_state_dir, event.issue_number);
                    let (removed_session, removed_lock) = reset_issue_session_files(&session_path)?;
                    if self.state_store.clear_issue_session(event.issue_number) {
                        *state_dirty = true;
                    }
                    let message = format!(
                        "Chat session reset for issue #{}.\n\nremoved_session_file={} removed_lock_file={}",
                        event.issue_number, removed_session, removed_lock
                    );
                    let posted = self
                        .post_issue_command_comment(
                            event.issue_number,
                            &event.key,
                            "chat-reset",
                            "completed",
                            &message,
                        )
                        .await?;
                    self.outbound_log.append(&json!({
                        "timestamp_unix_ms": current_unix_timestamp_ms(),
                        "repo": self.repo.as_slug(),
                        "event_key": event.key,
                        "issue_number": event.issue_number,
                        "command": "chat-reset",
                        "status": "completed",
                        "posted_comment_id": posted.id,
                        "posted_comment_url": posted.html_url,
                        "removed_session_file": removed_session,
                        "removed_lock_file": removed_lock,
                    }))?;
                }
            }
            TauIssueCommand::ChatExport => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let mut store = SessionStore::load(&session_path)?;
                store.set_lock_policy(
                    self.config.session_lock_wait_ms,
                    self.config.session_lock_stale_ms,
                );
                let head_id = store.head_id();
                let lineage_entries = store.lineage_entries(head_id)?;
                let export_jsonl = store.export_lineage_jsonl(head_id)?;
                let channel_store = ChannelStore::open(
                    &self.repository_state_dir.join("channel-store"),
                    "github",
                    &format!("issue-{}", event.issue_number),
                )?;
                let run_id = format!("chat-export-{}", event.issue_number);
                let artifact = channel_store.write_text_artifact(
                    &run_id,
                    "github-issue-chat-export",
                    "private",
                    normalize_artifact_retention_days(self.config.artifact_retention_days),
                    "jsonl",
                    &export_jsonl,
                )?;
                let head_display = head_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let message = if lineage_entries.is_empty() {
                    format!(
                        "Chat session export ready for issue #{} (no entries).\n\nentries=0 head={} artifact_id={} artifact_path={}",
                        event.issue_number,
                        head_display,
                        artifact.id,
                        artifact.relative_path
                    )
                } else {
                    format!(
                        "Chat session export ready for issue #{}.\n\nentries={} head={} artifact_id={} artifact_path={}",
                        event.issue_number,
                        lineage_entries.len(),
                        head_display,
                        artifact.id,
                        artifact.relative_path
                    )
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-export",
                        "completed",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-export",
                    "status": "completed",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "session": {
                        "entries": lineage_entries.len(),
                        "head_id": head_id,
                    },
                    "artifact": {
                        "id": artifact.id,
                        "run_id": artifact.run_id,
                        "type": artifact.artifact_type,
                        "relative_path": artifact.relative_path,
                        "bytes": artifact.bytes,
                        "expires_unix_ms": artifact.expires_unix_ms,
                    }
                }))?;
            }
            TauIssueCommand::ChatStatus => {
                let continuity = self.issue_chat_continuity_summary(event.issue_number)?;
                let session_state = self.state_store.issue_session(event.issue_number);
                let (session_id, last_comment_id, last_run_id, has_session) = match session_state {
                    Some(state) => (
                        state.session_id.as_str(),
                        state.last_comment_id,
                        state.last_run_id.as_deref(),
                        true,
                    ),
                    None => ("none", None, None, false),
                };
                let head_display = continuity
                    .head_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let message = if continuity.entries == 0 && !has_session {
                    format!(
                        "No chat session found for issue #{}.\n\nentries=0 head=none session_id=none last_comment_id=none last_run_id=none lineage_digest_sha256={} artifact_active={} artifact_total={} artifact_latest_id={} artifact_latest_run_id={} artifact_latest_created_unix_ms={} artifact_index_invalid_lines={}",
                        event.issue_number,
                        continuity.lineage_digest_sha256,
                        continuity.artifacts.active_records,
                        continuity.artifacts.total_records,
                        continuity
                            .artifacts
                            .latest_artifact_id
                            .as_deref()
                            .unwrap_or("none"),
                        continuity
                            .artifacts
                            .latest_artifact_run_id
                            .as_deref()
                            .unwrap_or("none"),
                        continuity
                            .artifacts
                            .latest_artifact_created_unix_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        continuity.artifacts.invalid_index_lines
                    )
                } else {
                    format!(
                        "Chat session status for issue #{}.\n\nentries={} head={} oldest_entry_id={} newest_entry_id={} newest_entry_role={} session_id={} last_comment_id={} last_run_id={} lineage_digest_sha256={} artifact_active={} artifact_total={} artifact_latest_id={} artifact_latest_run_id={} artifact_latest_created_unix_ms={} artifact_index_invalid_lines={}",
                        event.issue_number,
                        continuity.entries,
                        head_display,
                        continuity
                            .oldest_entry_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        continuity
                            .newest_entry_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        continuity.newest_entry_role.as_deref().unwrap_or("none"),
                        session_id,
                        last_comment_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        last_run_id.unwrap_or("none"),
                        continuity.lineage_digest_sha256,
                        continuity.artifacts.active_records,
                        continuity.artifacts.total_records,
                        continuity
                            .artifacts
                            .latest_artifact_id
                            .as_deref()
                            .unwrap_or("none"),
                        continuity
                            .artifacts
                            .latest_artifact_run_id
                            .as_deref()
                            .unwrap_or("none"),
                        continuity
                            .artifacts
                            .latest_artifact_created_unix_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string()),
                        continuity.artifacts.invalid_index_lines
                    )
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-status",
                        "reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-status",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "session": {
                        "entries": continuity.entries,
                        "head_id": continuity.head_id,
                        "oldest_entry_id": continuity.oldest_entry_id,
                        "newest_entry_id": continuity.newest_entry_id,
                        "newest_entry_role": continuity.newest_entry_role,
                        "lineage_digest_sha256": continuity.lineage_digest_sha256,
                        "session_id": session_id,
                        "last_comment_id": last_comment_id,
                        "last_run_id": last_run_id,
                    },
                    "artifacts": {
                        "active": continuity.artifacts.active_records,
                        "total": continuity.artifacts.total_records,
                        "latest_id": continuity.artifacts.latest_artifact_id,
                        "latest_run_id": continuity.artifacts.latest_artifact_run_id,
                        "latest_created_unix_ms": continuity.artifacts.latest_artifact_created_unix_ms,
                        "index_invalid_lines": continuity.artifacts.invalid_index_lines,
                    }
                }))?;
            }
            TauIssueCommand::ChatShow { limit } => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let store = SessionStore::load(&session_path)?;
                let head_id = store.head_id();
                let lineage = store.lineage_entries(head_id)?;
                let session_state = self.state_store.issue_session(event.issue_number);
                let has_session = session_state.is_some();
                let head_display = head_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let message = if lineage.is_empty() && !has_session {
                    format!(
                        "No chat session found for issue #{}.\n\nentries=0 head=none",
                        event.issue_number
                    )
                } else {
                    let total = lineage.len();
                    let capped_limit = limit.clamp(1, CHAT_SHOW_MAX_LIMIT);
                    let show_count = total.min(capped_limit);
                    let start_index = total.saturating_sub(show_count);
                    let mut lines = vec![format!(
                        "Chat session show for issue #{}.",
                        event.issue_number
                    )];
                    lines.push(format!(
                        "entries={} head={} showing_last={}",
                        total, head_display, show_count
                    ));
                    if show_count == 0 {
                        lines.push("no messages".to_string());
                    } else {
                        for entry in lineage.iter().skip(start_index) {
                            let role = session_message_role(&entry.message);
                            let preview = session_message_preview(&entry.message);
                            lines.push(format!(
                                "- id={} role={} preview={}",
                                entry.id, role, preview
                            ));
                        }
                        lines.push(format!(
                            "Note: previews truncated to {} chars.",
                            crate::session_commands::SESSION_SEARCH_PREVIEW_CHARS
                        ));
                    }
                    lines.join("\n")
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-show",
                        "reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-show",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "session": {
                        "entries": lineage.len(),
                        "head_id": head_id,
                        "show_limit": limit,
                    }
                }))?;
            }
            TauIssueCommand::ChatSearch { query, role, limit } => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let store = SessionStore::load(&session_path)?;
                let entries = store.entries();
                let has_session = self.state_store.issue_session(event.issue_number).is_some();
                let (matches, total_matches) =
                    search_session_entries(entries, &query, role.as_deref(), limit);
                let message = if entries.is_empty() && !has_session {
                    format!(
                        "No chat session found for issue #{}.\n\nentries=0",
                        event.issue_number
                    )
                } else {
                    let mut lines = vec![format!(
                        "Chat session search for issue #{}.",
                        event.issue_number
                    )];
                    lines.push(format!(
                        "query={} role={} limit={} matches={}",
                        query,
                        role.as_deref().unwrap_or("any"),
                        limit,
                        total_matches
                    ));
                    if matches.is_empty() {
                        lines.push("no matches".to_string());
                    } else {
                        for entry in matches {
                            lines.push(format!(
                                "- id={} role={} preview={}",
                                entry.id, entry.role, entry.preview
                            ));
                        }
                    }
                    lines.join("\n")
                };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-search",
                        "reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-search",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "search": {
                        "query": query,
                        "role": role,
                        "limit": limit,
                        "matches": total_matches,
                    }
                }))?;
            }
            TauIssueCommand::Invalid { message } => {
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "invalid",
                        "usage-reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "invalid",
                    "status": "usage_reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
        }

        if self.state_store.mark_processed(&event.key) {
            *state_dirty = true;
        }
        report.processed_events = report.processed_events.saturating_add(1);
        Ok(())
    }

    fn issue_artifact_summary(&self, issue_number: u64) -> Result<IssueArtifactSummary> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let loaded = store.load_artifact_records_tolerant()?;
        let now_unix_ms = current_unix_timestamp_ms();
        let mut records = loaded.records;
        let active_records = records
            .iter()
            .filter(|record| !is_artifact_record_expired(record, now_unix_ms))
            .count();
        records.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        let latest = records.first();
        Ok(IssueArtifactSummary {
            total_records: records.len(),
            active_records,
            latest_artifact_id: latest.map(|record| record.id.clone()),
            latest_artifact_run_id: latest.map(|record| record.run_id.clone()),
            latest_artifact_created_unix_ms: latest.map(|record| record.created_unix_ms),
            invalid_index_lines: loaded.invalid_lines,
        })
    }

    fn issue_chat_continuity_summary(
        &self,
        issue_number: u64,
    ) -> Result<IssueChatContinuitySummary> {
        let session_path = session_path_for_issue(&self.repository_state_dir, issue_number);
        let store = SessionStore::load(&session_path)?;
        let head_id = store.head_id();
        let lineage = store.lineage_entries(head_id)?;
        let lineage_jsonl = store.export_lineage_jsonl(head_id)?;
        let digest = sha256_hex(lineage_jsonl.as_bytes());
        let oldest_entry_id = lineage.first().map(|entry| entry.id);
        let newest_entry_id = lineage.last().map(|entry| entry.id);
        let newest_entry_role = lineage
            .last()
            .map(|entry| session_message_role(&entry.message));
        Ok(IssueChatContinuitySummary {
            entries: lineage.len(),
            head_id,
            oldest_entry_id,
            newest_entry_id,
            newest_entry_role,
            lineage_digest_sha256: digest,
            artifacts: self.issue_artifact_summary(issue_number)?,
        })
    }

    fn render_issue_status(&self, issue_number: u64) -> String {
        let active = self.active_runs.get(&issue_number);
        let latest = self.latest_runs.get(&issue_number);
        let state = if active.is_some() { "running" } else { "idle" };
        let mut lines = vec![format!("Tau status for issue #{issue_number}: {state}")];
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

        if let Some(session) = self.state_store.issue_session(issue_number) {
            lines.push(format!("chat_session_id: {}", session.session_id));
            lines.push(format!(
                "chat_last_comment_id: {}",
                session
                    .last_comment_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
            lines.push(format!(
                "chat_last_run_id: {}",
                session.last_run_id.as_deref().unwrap_or("none")
            ));
        } else {
            lines.push("chat_session_id: none".to_string());
        }
        match self.issue_chat_continuity_summary(issue_number) {
            Ok(summary) => {
                lines.push(format!("chat_entries: {}", summary.entries));
                lines.push(format!(
                    "chat_head_id: {}",
                    summary
                        .head_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "chat_oldest_entry_id: {}",
                    summary
                        .oldest_entry_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "chat_newest_entry_id: {}",
                    summary
                        .newest_entry_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "chat_newest_entry_role: {}",
                    summary.newest_entry_role.as_deref().unwrap_or("none")
                ));
                lines.push(format!(
                    "chat_lineage_digest_sha256: {}",
                    summary.lineage_digest_sha256
                ));
                lines.push(format!(
                    "artifacts_active: {}",
                    summary.artifacts.active_records
                ));
                lines.push(format!(
                    "artifacts_total: {}",
                    summary.artifacts.total_records
                ));
                lines.push(format!(
                    "artifacts_latest_id: {}",
                    summary
                        .artifacts
                        .latest_artifact_id
                        .as_deref()
                        .unwrap_or("none")
                ));
                lines.push(format!(
                    "artifacts_latest_run_id: {}",
                    summary
                        .artifacts
                        .latest_artifact_run_id
                        .as_deref()
                        .unwrap_or("none")
                ));
                lines.push(format!(
                    "artifacts_latest_created_unix_ms: {}",
                    summary
                        .artifacts
                        .latest_artifact_created_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "artifacts_index_invalid_lines: {}",
                    summary.artifacts.invalid_index_lines
                ));
            }
            Err(error) => lines.push(format!(
                "chat_summary_error: {}",
                truncate_for_error(&error.to_string(), 240)
            )),
        }
        lines.join("\n")
    }

    fn render_issue_health(&self, issue_number: u64) -> String {
        let active = self.active_runs.get(&issue_number);
        let runtime_state = if active.is_some() { "running" } else { "idle" };
        let health = self.state_store.transport_health();
        let classification = health.classify();
        let mut lines = vec![format!(
            "Tau health for issue #{}: {}",
            issue_number,
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

    fn render_issue_artifacts(
        &self,
        issue_number: u64,
        run_id_filter: Option<&str>,
    ) -> Result<String> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
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
                "Tau artifacts for issue #{} run_id `{}`: active={}",
                issue_number,
                run_id_filter,
                active.len()
            )
        } else {
            format!(
                "Tau artifacts for issue #{}: active={}",
                issue_number,
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

    fn render_issue_artifact_purge(&self, issue_number: u64) -> Result<String> {
        let now_unix_ms = current_unix_timestamp_ms();
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let purge = store.purge_expired_artifacts(now_unix_ms)?;
        let active = store.list_active_artifacts(now_unix_ms)?;
        Ok(format!(
            "Tau artifact purge for issue #{}: expired_removed={} invalid_removed={} attachment_expired_removed={} attachment_invalid_removed={} active_remaining={}",
            issue_number,
            purge.expired_removed,
            purge.invalid_removed,
            purge.attachment_expired_removed,
            purge.attachment_invalid_removed,
            active.len()
        ))
    }

    fn render_issue_artifact_show(&self, issue_number: u64, artifact_id: &str) -> Result<String> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
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
                    "Tau artifact for issue #{} id `{}`: state={}",
                    issue_number,
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
                "Tau artifact for issue #{} id `{}`: not found",
                issue_number, artifact_id
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

    async fn post_issue_command_comment(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &str,
        status: &str,
        message: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let body = render_issue_command_comment(event_key, command, status, message);
        self.github_client
            .create_issue_comment(issue_number, &body)
            .await
    }

    fn append_channel_log(
        &self,
        event: &GithubBridgeEvent,
        direction: &str,
        payload: Value,
    ) -> Result<()> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{}", event.issue_number),
        )?;
        store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: direction.to_string(),
            event_key: Some(event.key.clone()),
            source: "github".to_string(),
            payload,
        })
    }
}

fn rbac_action_for_event(action: &EventAction) -> String {
    match action {
        EventAction::RunPrompt { .. } => "command:/tau-run".to_string(),
        EventAction::Command(command) => match command {
            TauIssueCommand::Run { .. } => "command:/tau-run".to_string(),
            TauIssueCommand::Stop => "command:/tau-stop".to_string(),
            TauIssueCommand::Status => "command:/tau-status".to_string(),
            TauIssueCommand::Health => "command:/tau-health".to_string(),
            TauIssueCommand::Compact => "command:/tau-compact".to_string(),
            TauIssueCommand::Help => "command:/tau-help".to_string(),
            TauIssueCommand::ChatStart => "command:/tau-chat-start".to_string(),
            TauIssueCommand::ChatResume => "command:/tau-chat-resume".to_string(),
            TauIssueCommand::ChatReset => "command:/tau-chat-reset".to_string(),
            TauIssueCommand::ChatExport => "command:/tau-chat-export".to_string(),
            TauIssueCommand::ChatStatus => "command:/tau-chat-status".to_string(),
            TauIssueCommand::ChatShow { .. } => "command:/tau-chat-show".to_string(),
            TauIssueCommand::ChatSearch { .. } => "command:/tau-chat-search".to_string(),
            TauIssueCommand::Artifacts { .. } => "command:/tau-artifacts".to_string(),
            TauIssueCommand::ArtifactShow { .. } => "command:/tau-artifacts-show".to_string(),
            TauIssueCommand::Canvas { .. } => "command:/tau-canvas".to_string(),
            TauIssueCommand::Summarize { .. } => "command:/tau-summarize".to_string(),
            TauIssueCommand::Invalid { .. } => "command:/tau-invalid".to_string(),
        },
    }
}

async fn execute_issue_run_task(params: IssueRunTaskParams) -> RunTaskResult {
    let IssueRunTaskParams {
        github_client,
        config,
        repo,
        repository_state_dir,
        event,
        prompt,
        run_id,
        working_comment_id,
        cancel_rx,
        started_unix_ms,
    } = params;
    let started = Instant::now();
    let run_result = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: &repository_state_dir,
        event: &event,
        prompt: &prompt,
        run_id: &run_id,
        cancel_rx,
    })
    .await;

    let completed_unix_ms = current_unix_timestamp_ms();
    let duration_ms = started.elapsed().as_millis() as u64;

    let (status, usage, chunks, error) = match run_result {
        Ok(run) => {
            let status = prompt_status_label(run.status).to_string();
            (
                status,
                run.usage.clone(),
                render_issue_comment_chunks(&event, &run),
                None,
            )
        }
        Err(error) => (
            "failed".to_string(),
            PromptUsageSummary::default(),
            vec![render_issue_run_error_comment(&event, &run_id, &error)],
            Some(error.to_string()),
        ),
    };

    let comment_outcome = post_issue_comment_chunks(
        &github_client,
        event.issue_number,
        working_comment_id,
        &chunks,
    )
    .await;

    RunTaskResult {
        issue_number: event.issue_number,
        event_key: event.key,
        run_id,
        started_unix_ms,
        completed_unix_ms,
        duration_ms,
        status,
        posted_comment_id: comment_outcome.posted_comment_id,
        comment_edit_attempted: comment_outcome.edit_attempted,
        comment_edit_success: comment_outcome.edit_success,
        comment_append_count: comment_outcome.append_count,
        model: config.model,
        usage,
        error,
    }
}

async fn post_issue_comment_chunks(
    github_client: &GithubApiClient,
    issue_number: u64,
    working_comment_id: u64,
    chunks: &[String],
) -> CommentUpdateOutcome {
    let mut outcome = CommentUpdateOutcome {
        posted_comment_id: None,
        edit_attempted: false,
        edit_success: false,
        append_count: 0,
    };
    let Some((first, rest)) = chunks.split_first() else {
        return outcome;
    };

    outcome.edit_attempted = true;
    match github_client
        .update_issue_comment(working_comment_id, first)
        .await
    {
        Ok(comment) => {
            outcome.edit_success = true;
            outcome.posted_comment_id = Some(comment.id);
        }
        Err(update_error) => {
            let fallback_body = format!(
                "{first}\n\n_(warning: failed to update placeholder comment: {})_",
                truncate_for_error(&update_error.to_string(), 200)
            );
            match github_client
                .create_issue_comment(issue_number, &fallback_body)
                .await
            {
                Ok(comment) => {
                    outcome.append_count = outcome.append_count.saturating_add(1);
                    outcome.posted_comment_id = Some(comment.id);
                }
                Err(_) => {
                    return outcome;
                }
            }
        }
    };

    for chunk in rest {
        match github_client
            .create_issue_comment(issue_number, chunk)
            .await
        {
            Ok(comment) => {
                outcome.append_count = outcome.append_count.saturating_add(1);
                outcome.posted_comment_id = Some(comment.id);
            }
            Err(_) => break,
        }
    }
    outcome
}

async fn run_prompt_for_event(request: RunPromptForEventRequest<'_>) -> Result<PromptRunReport> {
    let RunPromptForEventRequest {
        config,
        github_client,
        repo,
        repository_state_dir,
        event,
        prompt,
        run_id,
        mut cancel_rx,
    } = request;

    let channel_store = ChannelStore::open(
        &repository_state_dir.join("channel-store"),
        "github",
        &format!("issue-{}", event.issue_number),
    )?;
    let attachment_retention_days =
        normalize_artifact_retention_days(config.artifact_retention_days);
    let downloaded_attachments = download_issue_attachments(
        github_client,
        &channel_store,
        event,
        run_id,
        attachment_retention_days,
        &event.body,
    )
    .await?;
    let session_path = channel_store.session_path();
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
    let mut tool_policy = config.tool_policy.clone();
    tool_policy.rbac_principal = Some(github_principal(&event.author_login));
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

    let mut session_runtime = Some(initialize_issue_session_runtime(
        &session_path,
        &config.system_prompt,
        config.session_lock_wait_ms,
        config.session_lock_stale_ms,
        &mut agent,
    )?);

    let formatted_prompt = render_event_prompt(repo, event, prompt, &downloaded_attachments);
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
        "Run cancelled by /tau stop.".to_string()
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
        "github-issue-reply",
        "private",
        attachment_retention_days,
        "md",
        &render_issue_artifact_markdown(
            repo,
            event,
            run_id,
            status,
            &assistant_reply,
            &downloaded_attachments,
        ),
    )?;
    channel_store.sync_context_from_messages(agent.messages())?;
    channel_store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        direction: "outbound".to_string(),
        event_key: Some(event.key.clone()),
        source: "github".to_string(),
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
            "downloaded_attachments": downloaded_attachments.iter().map(|attachment| {
                json!({
                    "source_url": attachment.source_url,
                    "original_name": attachment.original_name,
                    "path": attachment.path.display().to_string(),
                    "relative_path": attachment.relative_path,
                    "content_type": attachment.content_type,
                    "bytes": attachment.bytes,
                    "checksum_sha256": attachment.checksum_sha256,
                    "policy_reason_code": attachment.policy_reason_code,
                    "created_unix_ms": attachment.created_unix_ms,
                    "expires_unix_ms": attachment.expires_unix_ms,
                })
            }).collect::<Vec<_>>(),
        }),
    })?;
    Ok(PromptRunReport {
        run_id: run_id.to_string(),
        model: config.model.clone(),
        status,
        assistant_reply,
        usage,
        downloaded_attachments,
        artifact,
    })
}

async fn download_issue_attachments(
    github_client: &GithubApiClient,
    channel_store: &ChannelStore,
    event: &GithubBridgeEvent,
    run_id: &str,
    retention_days: Option<u64>,
    text: &str,
) -> Result<Vec<DownloadedGithubAttachment>> {
    let urls = extract_attachment_urls(text);
    if urls.is_empty() {
        return Ok(Vec::new());
    }

    let file_dir = channel_store
        .attachments_dir()
        .join(sanitize_for_path(&event.key));
    std::fs::create_dir_all(&file_dir)
        .with_context(|| format!("failed to create {}", file_dir.display()))?;

    let mut downloaded = Vec::new();
    for (index, url) in urls.iter().enumerate() {
        let url_policy = evaluate_attachment_url_policy(url);
        if !url_policy.accepted {
            eprintln!(
                "github attachment blocked by url policy: event={} url={} reason={}",
                event.key, url, url_policy.reason_code
            );
            continue;
        }

        let payload = match github_client.download_url_bytes(url).await {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!(
                    "github attachment download failed: event={} url={} error={error}",
                    event.key, url
                );
                continue;
            }
        };
        if payload.bytes.len() > GITHUB_ATTACHMENT_MAX_BYTES {
            eprintln!(
                "github attachment skipped due size limit: event={} url={} bytes={} limit={}",
                event.key,
                url,
                payload.bytes.len(),
                GITHUB_ATTACHMENT_MAX_BYTES
            );
            continue;
        }
        let content_policy =
            evaluate_attachment_content_type_policy(payload.content_type.as_deref());
        if !content_policy.accepted {
            eprintln!(
                "github attachment blocked by content policy: event={} url={} content_type={} reason={}",
                event.key,
                url,
                payload
                    .content_type
                    .as_deref()
                    .unwrap_or("unknown"),
                content_policy.reason_code,
            );
            continue;
        }

        let original_name = attachment_filename_from_url(url, index + 1);
        let safe_name = sanitize_for_path(&original_name);
        let safe_name = if safe_name.is_empty() {
            format!("attachment-{}.bin", index + 1)
        } else {
            safe_name
        };
        let path = file_dir.join(format!("{:02}-{}", index + 1, safe_name));
        std::fs::write(&path, &payload.bytes)
            .with_context(|| format!("failed to write {}", path.display()))?;
        let relative_path = normalize_relative_channel_path(
            &path,
            &channel_store.channel_dir(),
            "attachment file",
        )?;
        let created_unix_ms = current_unix_timestamp_ms();
        let expires_unix_ms = retention_days
            .map(|days| days.saturating_mul(86_400_000))
            .map(|ttl| created_unix_ms.saturating_add(ttl));
        let checksum_sha256 = sha256_hex(&payload.bytes);
        let policy_reason_code = if content_policy.reason_code == "allow_content_type_default" {
            url_policy.reason_code
        } else {
            content_policy.reason_code
        };
        let record = ChannelAttachmentRecord {
            id: format!(
                "attachment-{}-{}",
                created_unix_ms,
                short_key_hash(&format!("{}:{}:{}:{}", run_id, event.key, index, url))
            ),
            run_id: run_id.to_string(),
            event_key: event.key.clone(),
            actor: event.author_login.clone(),
            source_url: url.clone(),
            original_name: original_name.clone(),
            relative_path: relative_path.clone(),
            content_type: payload.content_type.clone(),
            bytes: payload.bytes.len() as u64,
            checksum_sha256: checksum_sha256.clone(),
            policy_decision: "accepted".to_string(),
            policy_reason_code: policy_reason_code.to_string(),
            created_unix_ms,
            expires_unix_ms,
        };
        channel_store.append_attachment_record(&record)?;
        downloaded.push(DownloadedGithubAttachment {
            source_url: url.clone(),
            original_name,
            path,
            relative_path,
            content_type: payload.content_type,
            bytes: payload.bytes.len() as u64,
            checksum_sha256,
            policy_reason_code: policy_reason_code.to_string(),
            created_unix_ms,
            expires_unix_ms,
        });
    }

    Ok(downloaded)
}

fn normalize_relative_channel_path(
    path: &Path,
    channel_root: &Path,
    label: &str,
) -> Result<String> {
    let relative = path.strip_prefix(channel_root).with_context(|| {
        format!(
            "failed to derive relative path for {label}: {}",
            path.display()
        )
    })?;
    let normalized = relative.to_string_lossy().replace('\\', "/");
    if normalized.trim().is_empty() {
        bail!(
            "derived empty relative path for {label}: {}",
            path.display()
        );
    }
    Ok(normalized)
}

fn initialize_issue_session_runtime(
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

fn collect_assistant_reply(messages: &[tau_ai::Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == tau_ai::MessageRole::Assistant)
        .map(|message| message.text_content())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this event.".to_string()
    } else {
        content
    }
}

fn render_event_prompt(
    repo: &RepoRef,
    event: &GithubBridgeEvent,
    prompt: &str,
    downloaded_attachments: &[DownloadedGithubAttachment],
) -> String {
    let mut rendered = format!(
        "You are responding as Tau inside GitHub issues.\nRepository: {}\nIssue: #{} ({})\nAuthor: @{}\nEvent: {}\n\nUser message:\n{}\n\nProvide a direct, actionable response suitable for a GitHub issue comment.",
        repo.as_slug(),
        event.issue_number,
        event.issue_title,
        event.author_login,
        event.kind.as_str(),
        prompt
    );
    if !downloaded_attachments.is_empty() {
        rendered.push_str("\n\nDownloaded attachments:\n");
        for attachment in downloaded_attachments {
            rendered.push_str(&format!(
                "- name={} path={} content_type={} bytes={} source_url={} policy_reason={} expires_unix_ms={}\n",
                attachment.original_name,
                attachment.path.display(),
                attachment
                    .content_type
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                attachment.bytes,
                attachment.source_url,
                attachment.policy_reason_code,
                attachment
                    .expires_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ));
        }
    }
    rendered
}

fn render_issue_comment_response_parts(
    event: &GithubBridgeEvent,
    run: &PromptRunReport,
) -> (String, String) {
    let mut content = run.assistant_reply.trim().to_string();
    if content.is_empty() {
        content = "I couldn't generate a textual response for this event.".to_string();
    }
    let usage = &run.usage;
    let status = format!("{:?}", run.status).to_lowercase();
    let mut footer = format!(
        "{EVENT_KEY_MARKER_PREFIX}{}{EVENT_KEY_MARKER_SUFFIX}\n_Tau run `{}` | status `{}` | model `{}` | tokens in/out/total `{}/{}/{}` | cost `unavailable`_\n_artifact `{}` | sha256 `{}` | bytes `{}`_",
        event.key,
        run.run_id,
        status,
        run.model,
        usage.input_tokens,
        usage.output_tokens,
        usage.total_tokens,
        run.artifact.relative_path,
        run.artifact.checksum_sha256,
        run.artifact.bytes
    );
    if !run.downloaded_attachments.is_empty() {
        footer.push_str(&format!(
            "\n_attachments downloaded `{}`_",
            run.downloaded_attachments.len()
        ));
    }
    (content, footer)
}

fn render_issue_command_comment(
    event_key: &str,
    command: &str,
    status: &str,
    message: &str,
) -> String {
    let content = if message.trim().is_empty() {
        "Tau command response.".to_string()
    } else {
        message.trim().to_string()
    };
    let command = if command.trim().is_empty() {
        "unknown"
    } else {
        command.trim()
    };
    let status = if status.trim().is_empty() {
        "unknown"
    } else {
        status.trim()
    };
    format!(
        "{content}\n\n---\n{EVENT_KEY_MARKER_PREFIX}{event_key}{EVENT_KEY_MARKER_SUFFIX}\n_Tau command `{command}` | status `{status}`_"
    )
}

fn render_issue_comment_chunks(event: &GithubBridgeEvent, run: &PromptRunReport) -> Vec<String> {
    render_issue_comment_chunks_with_limit(event, run, GITHUB_COMMENT_MAX_CHARS)
}

fn render_issue_comment_chunks_with_limit(
    event: &GithubBridgeEvent,
    run: &PromptRunReport,
    max_chars: usize,
) -> Vec<String> {
    let (content, footer) = render_issue_comment_response_parts(event, run);
    let footer_block = format!("\n\n---\n{footer}");
    let footer_len = footer_block.chars().count();
    if max_chars == 0 {
        return Vec::new();
    }
    if footer_len >= max_chars {
        return vec![footer_block];
    }
    let content_len = content.chars().count();
    if content_len + footer_len <= max_chars {
        return vec![format!("{content}{footer_block}")];
    }

    let max_first_len = max_chars.saturating_sub(footer_len);
    let (first_content, remainder) = split_at_char_index(&content, max_first_len);
    let mut chunks = Vec::new();
    chunks.push(format!("{first_content}{footer_block}"));
    let mut trailing = chunk_text_by_chars(&remainder, max_chars);
    chunks.append(&mut trailing);
    chunks
}

fn render_issue_artifact_markdown(
    repo: &RepoRef,
    event: &GithubBridgeEvent,
    run_id: &str,
    status: PromptRunStatus,
    assistant_reply: &str,
    downloaded_attachments: &[DownloadedGithubAttachment],
) -> String {
    let status_label = prompt_status_label(status);
    let mut lines = vec![
        "# Tau Artifact".to_string(),
        format!("repository: {}", repo.as_slug()),
        format!("issue_number: {}", event.issue_number),
        format!("event_key: {}", event.key),
        format!("run_id: {}", run_id),
        format!("status: {}", status_label),
    ];
    if downloaded_attachments.is_empty() {
        lines.push("attachments: none".to_string());
    } else {
        lines.push(format!("attachments: {}", downloaded_attachments.len()));
        for attachment in downloaded_attachments {
            lines.push(format!(
                "- name={} path={} relative_path={} content_type={} bytes={} source_url={} checksum_sha256={} policy_reason={} created_unix_ms={} expires_unix_ms={}",
                attachment.original_name,
                attachment.path.display(),
                attachment.relative_path,
                attachment
                    .content_type
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                attachment.bytes,
                attachment.source_url,
                attachment.checksum_sha256,
                attachment.policy_reason_code,
                attachment.created_unix_ms,
                attachment
                    .expires_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ));
        }
    }
    lines.push(String::new());
    lines.push("## Assistant Reply".to_string());
    lines.push(assistant_reply.trim().to_string());
    lines.join("\n")
}

fn normalize_artifact_retention_days(days: u64) -> Option<u64> {
    if days == 0 {
        None
    } else {
        Some(days)
    }
}

fn render_issue_run_error_comment(
    event: &GithubBridgeEvent,
    run_id: &str,
    error: &anyhow::Error,
) -> String {
    format!(
        "Tau run `{}` failed for event `{}`.\n\nError: `{}`\n\n---\n{EVENT_KEY_MARKER_PREFIX}{}{EVENT_KEY_MARKER_SUFFIX}\n_Tau run `{}` | status `failed` | model `unavailable` | tokens in/out/total `0/0/0` | cost `unavailable`_",
        run_id,
        event.key,
        truncate_for_error(&error.to_string(), 600),
        event.key,
        run_id
    )
}

fn event_action_from_body(body: &str) -> EventAction {
    match parse_tau_issue_command(body) {
        Some(command) => EventAction::Command(command),
        None => EventAction::RunPrompt {
            prompt: body.trim().to_string(),
        },
    }
}

fn parse_tau_issue_command(body: &str) -> Option<TauIssueCommand> {
    let trimmed = body.trim();
    let mut pieces = trimmed.split_whitespace();
    let command_prefix = pieces.next()?;
    if command_prefix != "/tau" {
        return None;
    }

    let args = trimmed[command_prefix.len()..].trim();
    if args.is_empty() {
        return Some(TauIssueCommand::Invalid {
            message: tau_command_usage(),
        });
    }
    let mut parts = args.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();
    let parsed = match command {
        "run" => {
            if remainder.is_empty() {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau run <prompt>".to_string(),
                }
            } else {
                TauIssueCommand::Run {
                    prompt: remainder.to_string(),
                }
            }
        }
        "stop" => {
            if remainder.is_empty() {
                TauIssueCommand::Stop
            } else {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau stop".to_string(),
                }
            }
        }
        "status" => {
            if remainder.is_empty() {
                TauIssueCommand::Status
            } else {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau status".to_string(),
                }
            }
        }
        "health" => {
            if remainder.is_empty() {
                TauIssueCommand::Health
            } else {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau health".to_string(),
                }
            }
        }
        "compact" => {
            if remainder.is_empty() {
                TauIssueCommand::Compact
            } else {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau compact".to_string(),
                }
            }
        }
        "help" => {
            if remainder.is_empty() {
                TauIssueCommand::Help
            } else {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau help".to_string(),
                }
            }
        }
        "chat" => {
            let mut chat_parts = remainder.splitn(2, char::is_whitespace);
            let chat_command = chat_parts.next();
            let chat_remainder = chat_parts.next().unwrap_or_default().trim();
            match chat_command {
                Some("start") if chat_remainder.is_empty() => TauIssueCommand::ChatStart,
                Some("resume") if chat_remainder.is_empty() => TauIssueCommand::ChatResume,
                Some("reset") if chat_remainder.is_empty() => TauIssueCommand::ChatReset,
                Some("export") if chat_remainder.is_empty() => TauIssueCommand::ChatExport,
                Some("status") if chat_remainder.is_empty() => TauIssueCommand::ChatStatus,
                Some("show") => {
                    if chat_remainder.is_empty() {
                        TauIssueCommand::ChatShow {
                            limit: CHAT_SHOW_DEFAULT_LIMIT,
                        }
                    } else {
                        let mut show_parts = chat_remainder.split_whitespace();
                        match (show_parts.next(), show_parts.next()) {
                            (Some(raw), None) => match raw.parse::<usize>() {
                                Ok(limit) if limit > 0 => TauIssueCommand::ChatShow {
                                    limit: limit.min(CHAT_SHOW_MAX_LIMIT),
                                },
                                _ => TauIssueCommand::Invalid {
                                    message: "Usage: /tau chat show [limit]".to_string(),
                                },
                            },
                            _ => TauIssueCommand::Invalid {
                                message: "Usage: /tau chat show [limit]".to_string(),
                            },
                        }
                    }
                }
                Some("search") => {
                    if chat_remainder.is_empty() {
                        TauIssueCommand::Invalid {
                            message:
                                "Usage: /tau chat search <query> [--role <role>] [--limit <n>]"
                                    .to_string(),
                        }
                    } else {
                        match parse_session_search_args(chat_remainder) {
                            Ok(args) if args.limit <= CHAT_SEARCH_MAX_LIMIT => {
                                TauIssueCommand::ChatSearch {
                                    query: args.query,
                                    role: args.role,
                                    limit: args.limit,
                                }
                            }
                            _ => TauIssueCommand::Invalid {
                                message:
                                    "Usage: /tau chat search <query> [--role <role>] [--limit <n>]"
                                        .to_string(),
                            },
                        }
                    }
                }
                None => TauIssueCommand::Invalid {
                    message: "Usage: /tau chat <start|resume|reset|export|status|show [limit]|search <query>>"
                        .to_string(),
                },
                _ => TauIssueCommand::Invalid {
                    message: "Usage: /tau chat <start|resume|reset|export|status|show [limit]|search <query>>"
                        .to_string(),
                },
            }
        }
        "artifacts" => {
            if remainder.is_empty() {
                TauIssueCommand::Artifacts {
                    purge: false,
                    run_id: None,
                }
            } else if remainder == "purge" {
                TauIssueCommand::Artifacts {
                    purge: true,
                    run_id: None,
                }
            } else {
                let mut artifact_args = remainder.split_whitespace();
                match (
                    artifact_args.next(),
                    artifact_args.next(),
                    artifact_args.next(),
                ) {
                    (Some("run"), Some(run_id), None) => TauIssueCommand::Artifacts {
                        purge: false,
                        run_id: Some(run_id.to_string()),
                    },
                    (Some("show"), Some(artifact_id), None) => TauIssueCommand::ArtifactShow {
                        artifact_id: artifact_id.to_string(),
                    },
                    _ => TauIssueCommand::Invalid {
                        message: "Usage: /tau artifacts [purge|run <run_id>|show <artifact_id>]"
                            .to_string(),
                    },
                }
            }
        }
        "canvas" => {
            if remainder.is_empty() {
                TauIssueCommand::Invalid {
                    message: "Usage: /tau canvas <create|update|show|export|import> ..."
                        .to_string(),
                }
            } else {
                TauIssueCommand::Canvas {
                    args: remainder.to_string(),
                }
            }
        }
        "summarize" => {
            let focus = (!remainder.is_empty()).then(|| remainder.to_string());
            TauIssueCommand::Summarize { focus }
        }
        _ => TauIssueCommand::Invalid {
            message: format!("Unknown command `{}`.\n\n{}", command, tau_command_usage()),
        },
    };
    Some(parsed)
}

fn tau_command_usage() -> String {
    [
        "Supported `/tau` commands:",
        "- `/tau run <prompt>`",
        "- `/tau stop`",
        "- `/tau status`",
        "- `/tau health`",
        "- `/tau compact`",
        "- `/tau help`",
        "- `/tau chat <start|resume|reset|export|status|show [limit]|search <query>>`",
        "- `/tau artifacts [purge|run <run_id>|show <artifact_id>]`",
        "- `/tau canvas <create|update|show|export|import> ...`",
        "- `/tau summarize [focus]`",
    ]
    .join("\n")
}

fn build_summarize_prompt(
    repo: &RepoRef,
    event: &GithubBridgeEvent,
    focus: Option<&str>,
) -> String {
    match focus {
        Some(focus) => format!(
            "Summarize the current GitHub issue thread for {} issue #{} with focus on: {}.\nInclude decisions, open questions, blockers, and immediate next steps.",
            repo.as_slug(),
            event.issue_number,
            focus
        ),
        None => format!(
            "Summarize the current GitHub issue thread for {} issue #{}.\nInclude decisions, open questions, blockers, and immediate next steps.",
            repo.as_slug(),
            event.issue_number
        ),
    }
}

fn compact_issue_session(
    session_path: &Path,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
) -> Result<crate::session::CompactReport> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    store.compact_to_lineage(store.head_id())
}

fn ensure_issue_session_initialized(
    session_path: &Path,
    system_prompt: &str,
    lock_wait_ms: u64,
    lock_stale_ms: u64,
) -> Result<(usize, usize, Option<u64>)> {
    if let Some(parent) = session_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let mut store = SessionStore::load(session_path)?;
    store.set_lock_policy(lock_wait_ms.max(1), lock_stale_ms);
    let before_entries = store.entries().len();
    let head = store.ensure_initialized(system_prompt)?;
    let after_entries = store.entries().len();
    Ok((before_entries, after_entries, head))
}

fn reset_issue_session_files(session_path: &Path) -> Result<(bool, bool)> {
    let mut removed_session = false;
    if session_path.exists() {
        std::fs::remove_file(session_path)
            .with_context(|| format!("failed to remove {}", session_path.display()))?;
        removed_session = true;
    }
    let lock_path = session_path.with_extension("lock");
    let mut removed_lock = false;
    if lock_path.exists() {
        std::fs::remove_file(&lock_path)
            .with_context(|| format!("failed to remove {}", lock_path.display()))?;
        removed_lock = true;
    }
    Ok((removed_session, removed_lock))
}

fn prompt_status_label(status: PromptRunStatus) -> &'static str {
    match status {
        PromptRunStatus::Completed => "completed",
        PromptRunStatus::Cancelled => "cancelled",
        PromptRunStatus::TimedOut => "timed_out",
    }
}

fn collect_issue_events(
    issue: &GithubIssue,
    comments: &[GithubIssueComment],
    bot_login: &str,
    include_issue_body: bool,
    include_edited_comments: bool,
) -> Vec<GithubBridgeEvent> {
    let mut events = Vec::new();
    if include_issue_body
        && issue.user.login != bot_login
        && !issue.body.as_deref().unwrap_or_default().trim().is_empty()
    {
        let body = issue.body.clone().unwrap_or_default();
        events.push(GithubBridgeEvent {
            key: format!("issue-opened:{}", issue.id),
            kind: GithubBridgeEventKind::Opened,
            issue_number: issue.number,
            issue_title: issue.title.clone(),
            author_login: issue.user.login.clone(),
            occurred_at: issue.created_at.clone(),
            body,
            raw_payload: serde_json::to_value(issue).unwrap_or(Value::Null),
        });
    }

    for comment in comments {
        if comment.user.login == bot_login {
            continue;
        }
        let body = comment
            .body
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        if body.is_empty() {
            continue;
        }
        let is_edit = comment.updated_at != comment.created_at;
        if is_edit && !include_edited_comments {
            continue;
        }
        let (key, kind) = if is_edit {
            (
                format!("issue-comment-edited:{}:{}", comment.id, comment.updated_at),
                GithubBridgeEventKind::CommentEdited,
            )
        } else {
            (
                format!("issue-comment-created:{}", comment.id),
                GithubBridgeEventKind::CommentCreated,
            )
        };
        events.push(GithubBridgeEvent {
            key,
            kind,
            issue_number: issue.number,
            issue_title: issue.title.clone(),
            author_login: comment.user.login.clone(),
            occurred_at: comment.created_at.clone(),
            body: body.to_string(),
            raw_payload: serde_json::to_value(comment).unwrap_or(Value::Null),
        });
    }

    events.sort_by(|left, right| {
        left.occurred_at
            .cmp(&right.occurred_at)
            .then(left.key.cmp(&right.key))
    });
    events
}

fn session_path_for_issue(repo_state_dir: &Path, issue_number: u64) -> PathBuf {
    repo_state_dir
        .join("sessions")
        .join(format!("issue-{}.jsonl", issue_number))
}

fn issue_session_id(issue_number: u64) -> String {
    format!("issue-{}", issue_number)
}

fn parse_rfc3339_to_unix_ms(raw: &str) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    u64::try_from(parsed.timestamp_millis()).ok()
}

fn sanitize_for_path(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn extract_footer_event_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut cursor = text;
    loop {
        let tau = cursor.find(EVENT_KEY_MARKER_PREFIX);
        let legacy = cursor.find(LEGACY_EVENT_KEY_MARKER_PREFIX);
        let (start, marker_prefix) = match (tau, legacy) {
            (Some(tau_start), Some(legacy_start)) if tau_start <= legacy_start => {
                (tau_start, EVENT_KEY_MARKER_PREFIX)
            }
            (Some(_), Some(legacy_start)) => (legacy_start, LEGACY_EVENT_KEY_MARKER_PREFIX),
            (Some(tau_start), None) => (tau_start, EVENT_KEY_MARKER_PREFIX),
            (None, Some(legacy_start)) => (legacy_start, LEGACY_EVENT_KEY_MARKER_PREFIX),
            (None, None) => break,
        };
        let after_start = &cursor[start + marker_prefix.len()..];
        let Some(end) = after_start.find(EVENT_KEY_MARKER_SUFFIX) else {
            break;
        };
        let key = after_start[..end].trim();
        if !key.is_empty() {
            keys.push(key.to_string());
        }
        cursor = &after_start[end + EVENT_KEY_MARKER_SUFFIX.len()..];
    }
    keys
}

fn is_artifact_record_expired(record: &ChannelArtifactRecord, now_unix_ms: u64) -> bool {
    record
        .expires_unix_ms
        .map(|value| value <= now_unix_ms)
        .unwrap_or(false)
}

fn sha256_hex(payload: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let digest = hasher.finalize();
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn short_key_hash(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        digest[0], digest[1], digest[2], digest[3]
    )
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
        time::Duration,
    };

    use async_trait::async_trait;
    use httpmock::prelude::*;
    use serde_json::json;
    use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
    use tempfile::tempdir;
    use tokio::time::sleep;

    use super::{
        collect_issue_events, evaluate_attachment_content_type_policy,
        evaluate_attachment_url_policy, event_action_from_body, extract_attachment_urls,
        extract_footer_event_keys, is_retryable_github_status, issue_session_id,
        normalize_artifact_retention_days, normalize_relative_channel_path,
        parse_rfc3339_to_unix_ms, parse_tau_issue_command, post_issue_comment_chunks,
        render_event_prompt, render_issue_command_comment, render_issue_comment_chunks_with_limit,
        render_issue_comment_response_parts, retry_delay, run_prompt_for_event, sanitize_for_path,
        session_path_for_issue, DownloadedGithubAttachment, EventAction, GithubApiClient,
        GithubBridgeEvent, GithubBridgeEventKind, GithubIssue, GithubIssueComment,
        GithubIssuesBridgeRuntime, GithubIssuesBridgeRuntimeConfig, GithubIssuesBridgeStateStore,
        GithubUser, PromptRunReport, PromptUsageSummary, RepoRef, RunPromptForEventRequest,
        SessionStore, TauIssueCommand, CHAT_SHOW_DEFAULT_LIMIT, EVENT_KEY_MARKER_PREFIX,
    };
    use crate::{
        channel_store::{ChannelArtifactRecord, ChannelStore},
        tools::ToolPolicy,
        PromptRunStatus, RenderOptions,
    };

    struct StaticReplyClient;

    #[async_trait]
    impl LlmClient for StaticReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            Ok(ChatResponse {
                message: Message::assistant_text("bridge reply"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 11,
                    output_tokens: 7,
                    total_tokens: 18,
                },
            })
        }
    }

    struct SlowReplyClient;

    #[async_trait]
    impl LlmClient for SlowReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
            sleep(Duration::from_millis(500)).await;
            Ok(ChatResponse {
                message: Message::assistant_text("slow bridge reply"),
                finish_reason: Some("stop".to_string()),
                usage: ChatUsage {
                    input_tokens: 5,
                    output_tokens: 3,
                    total_tokens: 8,
                },
            })
        }
    }

    fn test_bridge_config(base_url: &str, state_dir: &Path) -> GithubIssuesBridgeRuntimeConfig {
        test_bridge_config_with_client(base_url, state_dir, Arc::new(StaticReplyClient))
    }

    fn test_bridge_config_with_client(
        base_url: &str,
        state_dir: &Path,
        client: Arc<dyn LlmClient>,
    ) -> GithubIssuesBridgeRuntimeConfig {
        GithubIssuesBridgeRuntimeConfig {
            client,
            model: "openai/gpt-4o-mini".to_string(),
            system_prompt: "You are Tau.".to_string(),
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
            repo_slug: "owner/repo".to_string(),
            api_base: base_url.to_string(),
            token: "test-token".to_string(),
            bot_login: Some("tau".to_string()),
            poll_interval: Duration::from_millis(1),
            include_issue_body: false,
            include_edited_comments: true,
            processed_event_cap: 32,
            retry_max_attempts: 3,
            retry_base_delay_ms: 5,
            artifact_retention_days: 30,
        }
    }

    fn test_issue_event() -> GithubBridgeEvent {
        GithubBridgeEvent {
            key: "issue-comment-created:200".to_string(),
            kind: GithubBridgeEventKind::CommentCreated,
            issue_number: 7,
            issue_title: "Bridge me".to_string(),
            author_login: "alice".to_string(),
            occurred_at: "2026-01-01T00:00:01Z".to_string(),
            body: "hello from issue stream".to_string(),
            raw_payload: json!({"id": 200}),
        }
    }

    fn test_prompt_run_report(reply: &str) -> PromptRunReport {
        PromptRunReport {
            run_id: "run-1".to_string(),
            model: "openai/gpt-4o-mini".to_string(),
            status: PromptRunStatus::Completed,
            assistant_reply: reply.to_string(),
            usage: PromptUsageSummary {
                input_tokens: 2,
                output_tokens: 3,
                total_tokens: 5,
                request_duration_ms: 0,
                finish_reason: None,
            },
            downloaded_attachments: Vec::new(),
            artifact: ChannelArtifactRecord {
                id: "artifact-1".to_string(),
                run_id: "run-1".to_string(),
                artifact_type: "github-issue-reply".to_string(),
                visibility: "private".to_string(),
                relative_path: "artifacts/run-1.md".to_string(),
                bytes: 123,
                checksum_sha256: "checksum".to_string(),
                created_unix_ms: 0,
                expires_unix_ms: None,
            },
        }
    }

    #[test]
    fn unit_normalize_artifact_retention_days_maps_zero_to_none() {
        assert_eq!(normalize_artifact_retention_days(0), None);
        assert_eq!(normalize_artifact_retention_days(30), Some(30));
    }

    #[test]
    fn unit_repo_ref_parse_accepts_owner_repo_shape() {
        let repo = RepoRef::parse("njfio/Tau").expect("parse repo");
        assert_eq!(repo.owner, "njfio");
        assert_eq!(repo.name, "Tau");

        let error = RepoRef::parse("missing").expect_err("invalid repo should fail");
        assert!(error.to_string().contains("expected owner/repo"));
    }

    #[test]
    fn functional_collect_issue_events_supports_created_and_edited_comments() {
        let issue = GithubIssue {
            id: 100,
            number: 42,
            title: "Issue".to_string(),
            body: Some("initial issue body".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:10Z".to_string(),
            user: GithubUser {
                login: "alice".to_string(),
            },
            pull_request: None,
        };
        let comments = vec![
            GithubIssueComment {
                id: 1,
                body: Some("first".to_string()),
                created_at: "2026-01-01T00:00:01Z".to_string(),
                updated_at: "2026-01-01T00:00:01Z".to_string(),
                user: GithubUser {
                    login: "bob".to_string(),
                },
            },
            GithubIssueComment {
                id: 2,
                body: Some("second edited".to_string()),
                created_at: "2026-01-01T00:00:02Z".to_string(),
                updated_at: "2026-01-01T00:10:02Z".to_string(),
                user: GithubUser {
                    login: "carol".to_string(),
                },
            },
        ];
        let events = collect_issue_events(&issue, &comments, "tau", true, true);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, GithubBridgeEventKind::Opened);
        assert_eq!(events[1].kind, GithubBridgeEventKind::CommentCreated);
        assert_eq!(events[2].kind, GithubBridgeEventKind::CommentEdited);
    }

    #[tokio::test]
    async fn functional_run_prompt_for_event_sets_expiry_with_default_retention() {
        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config("http://unused.local", temp.path());
        let repo = RepoRef::parse("owner/repo").expect("repo");
        let github_client = GithubApiClient::new(
            "http://unused.local".to_string(),
            "token".to_string(),
            repo.clone(),
            2_000,
            1,
            1,
        )
        .expect("github client");
        let event = test_issue_event();
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let report = run_prompt_for_event(RunPromptForEventRequest {
            config: &config,
            github_client: &github_client,
            repo: &repo,
            repository_state_dir: temp.path(),
            event: &event,
            prompt: "hello from test",
            run_id: "run-default-retention",
            cancel_rx,
        })
        .await
        .expect("run prompt");
        assert!(report.artifact.expires_unix_ms.is_some());
    }

    #[tokio::test]
    async fn regression_run_prompt_for_event_zero_retention_disables_expiry() {
        let temp = tempdir().expect("tempdir");
        let mut config = test_bridge_config("http://unused.local", temp.path());
        config.artifact_retention_days = 0;
        let repo = RepoRef::parse("owner/repo").expect("repo");
        let github_client = GithubApiClient::new(
            "http://unused.local".to_string(),
            "token".to_string(),
            repo.clone(),
            2_000,
            1,
            1,
        )
        .expect("github client");
        let event = test_issue_event();
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let report = run_prompt_for_event(RunPromptForEventRequest {
            config: &config,
            github_client: &github_client,
            repo: &repo,
            repository_state_dir: temp.path(),
            event: &event,
            prompt: "hello from test",
            run_id: "run-zero-retention",
            cancel_rx,
        })
        .await
        .expect("run prompt");
        assert_eq!(report.artifact.expires_unix_ms, None);

        let store = ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-7")
            .expect("open store");
        let active = store
            .list_active_artifacts(crate::current_unix_timestamp_ms())
            .expect("list active");
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn regression_zero_retention_keeps_attachment_manifest_entries_non_expiring() {
        let server = MockServer::start();
        let attachment_url = format!("{}/assets/trace.log", server.base_url());
        let attachment_download = server.mock(|when, then| {
            when.method(GET).path("/assets/trace.log");
            then.status(200)
                .header("content-type", "text/plain")
                .body("trace-line-1\ntrace-line-2\n");
        });
        let temp = tempdir().expect("tempdir");
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.artifact_retention_days = 0;
        let repo = RepoRef::parse("owner/repo").expect("repo");
        let github_client = GithubApiClient::new(
            server.base_url(),
            "token".to_string(),
            repo.clone(),
            2_000,
            1,
            1,
        )
        .expect("github client");
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let event = GithubBridgeEvent {
            key: "issue-comment-created:1202".to_string(),
            kind: GithubBridgeEventKind::CommentCreated,
            issue_number: 22,
            issue_title: "Attachment retention".to_string(),
            author_login: "alice".to_string(),
            occurred_at: "2026-01-01T00:00:01Z".to_string(),
            body: attachment_url.clone(),
            raw_payload: json!({"id": 1202}),
        };
        let report = run_prompt_for_event(RunPromptForEventRequest {
            config: &config,
            github_client: &github_client,
            repo: &repo,
            repository_state_dir: temp.path(),
            event: &event,
            prompt: &event.body,
            run_id: "run-zero-retention-attachment",
            cancel_rx,
        })
        .await
        .expect("run prompt");
        assert_eq!(report.downloaded_attachments.len(), 1);
        assert_eq!(report.downloaded_attachments[0].expires_unix_ms, None);
        attachment_download.assert_calls(1);

        let store = ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-22")
            .expect("channel store");
        let attachment_manifest = store
            .load_attachment_records_tolerant()
            .expect("attachment manifest");
        assert_eq!(attachment_manifest.records.len(), 1);
        assert_eq!(attachment_manifest.records[0].expires_unix_ms, None);

        let purge = store
            .purge_expired_artifacts(
                crate::current_unix_timestamp_ms().saturating_add(31 * 86_400_000),
            )
            .expect("purge");
        assert_eq!(purge.attachment_expired_removed, 0);
        assert!(store
            .channel_dir()
            .join(&attachment_manifest.records[0].relative_path)
            .exists());
    }

    #[test]
    fn regression_state_store_caps_processed_event_history() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("state.json");
        let mut state = GithubIssuesBridgeStateStore::load(state_path, 2).expect("load store");
        assert!(state.mark_processed("a"));
        assert!(state.mark_processed("b"));
        assert!(state.mark_processed("c"));
        assert!(!state.contains("a"));
        assert!(state.contains("b"));
        assert!(state.contains("c"));
    }

    #[test]
    fn unit_state_store_upserts_issue_session_state() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("state.json");
        let mut state = GithubIssuesBridgeStateStore::load(state_path, 8).expect("load store");

        assert!(state.update_issue_session(
            42,
            "issue-42".to_string(),
            Some(101),
            Some("run-1".to_string())
        ));
        let session = state.issue_session(42).expect("session state");
        assert_eq!(session.session_id, "issue-42");
        assert_eq!(session.last_comment_id, Some(101));
        assert_eq!(session.last_run_id.as_deref(), Some("run-1"));

        assert!(!state.update_issue_session(
            42,
            "issue-42".to_string(),
            Some(101),
            Some("run-1".to_string())
        ));
        assert!(state.update_issue_session(42, "issue-42".to_string(), Some(202), None));
        let session = state.issue_session(42).expect("updated session state");
        assert_eq!(session.last_comment_id, Some(202));
        assert_eq!(session.last_run_id.as_deref(), Some("run-1"));

        assert!(state.clear_issue_session(42));
        assert!(state.issue_session(42).is_none());
        assert!(!state.clear_issue_session(42));
    }

    #[test]
    fn regression_state_store_loads_legacy_state_without_issue_sessions() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("state.json");
        std::fs::write(
            &state_path,
            r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": ["a", "b"]
}"#,
        )
        .expect("write legacy state");

        let state = GithubIssuesBridgeStateStore::load(state_path, 8).expect("load store");
        assert_eq!(state.last_issue_scan_at(), Some("2026-01-01T00:00:00Z"));
        assert!(state.contains("a"));
        assert!(state.contains("b"));
        assert!(state.issue_session(9).is_none());
        assert_eq!(
            state.transport_health(),
            &crate::TransportHealthSnapshot::default()
        );
    }

    #[test]
    fn regression_state_store_loads_with_corrupt_state_file() {
        let temp = tempdir().expect("tempdir");
        let state_path = temp.path().join("state.json");
        std::fs::write(&state_path, "{not-json").expect("write corrupt state");

        let state = GithubIssuesBridgeStateStore::load(state_path, 8).expect("load store");
        assert!(state.last_issue_scan_at().is_none());
        assert!(!state.contains("a"));
        assert!(state.issue_session(1).is_none());
    }

    #[test]
    fn unit_retry_helpers_identify_retryable_status_and_delays() {
        assert!(is_retryable_github_status(429));
        assert!(is_retryable_github_status(500));
        assert!(!is_retryable_github_status(404));
        let delay = retry_delay(100, 3, None);
        assert_eq!(delay, Duration::from_millis(400));
    }

    #[test]
    fn unit_parse_rfc3339_to_unix_ms_handles_valid_and_invalid_values() {
        assert!(parse_rfc3339_to_unix_ms("2026-01-01T00:00:01Z").is_some());
        assert_eq!(parse_rfc3339_to_unix_ms("invalid"), None);
    }

    #[test]
    fn unit_footer_key_extraction_and_path_helpers_are_stable() {
        let text = "hello\n<!-- tau-event-key:abc -->\nworld\n<!-- rsbot-event-key:def -->";
        let keys = extract_footer_event_keys(text);
        assert_eq!(keys, vec!["abc".to_string(), "def".to_string()]);

        let root = Path::new("/tmp/state");
        let session = session_path_for_issue(root, 9);
        assert!(session.ends_with("sessions/issue-9.jsonl"));
        assert_eq!(sanitize_for_path("owner/repo"), "owner_repo");
    }

    #[test]
    fn unit_normalize_relative_channel_path_requires_descendant_paths() {
        let channel_root = Path::new("/tmp/tau-channel");
        let file_path = channel_root.join("attachments/issue-comment-created_1/01-trace.log");
        let relative =
            normalize_relative_channel_path(&file_path, channel_root, "attachment").expect("path");
        assert_eq!(relative, "attachments/issue-comment-created_1/01-trace.log");

        let outside = Path::new("/tmp/not-channel/trace.log");
        let error = normalize_relative_channel_path(outside, channel_root, "attachment")
            .expect_err("outside channel root should fail");
        assert!(error.to_string().contains("failed to derive relative path"));
    }

    #[test]
    fn unit_render_issue_command_comment_appends_marker_footer() {
        let rendered = render_issue_command_comment(
            "issue-comment-created:123",
            "chat-status",
            "reported",
            "Chat session status for issue #12.",
        );
        assert!(rendered.contains("Chat session status for issue #12."));
        assert!(rendered.contains("tau-event-key:issue-comment-created:123"));
        assert!(rendered.contains("Tau command `chat-status` | status `reported`"));
    }

    #[test]
    fn unit_extract_attachment_urls_supports_markdown_and_bare_links() {
        let text = "See [trace](https://example.com/files/trace.log) and https://example.com/images/graph.png plus duplicate https://example.com/files/trace.log";
        let urls = extract_attachment_urls(text);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com/files/trace.log");
        assert_eq!(urls[1], "https://example.com/images/graph.png");
    }

    #[test]
    fn unit_extract_attachment_urls_accepts_localhost_port_with_extension() {
        let url = "http://127.0.0.1:1234/assets/trace.log";
        let urls = extract_attachment_urls(url);
        assert_eq!(urls, vec![url.to_string()]);
        assert!(crate::github_issues_helpers::is_supported_attachment_url(
            url
        ));
    }

    #[test]
    fn unit_attachment_url_policy_enforces_allowlist_and_denylist() {
        let denied = evaluate_attachment_url_policy("https://example.com/files/run.exe");
        assert!(!denied.accepted);
        assert_eq!(denied.reason_code, "deny_extension_denylist");

        let unknown = evaluate_attachment_url_policy("https://example.com/files/run.unknown");
        assert!(!unknown.accepted);
        assert_eq!(unknown.reason_code, "deny_extension_not_allowlisted");

        let allowed = evaluate_attachment_url_policy("https://example.com/files/run.log");
        assert!(allowed.accepted);
        assert_eq!(allowed.reason_code, "allow_extension_allowlist");
    }

    #[test]
    fn unit_attachment_content_type_policy_blocks_dangerous_values() {
        let denied = evaluate_attachment_content_type_policy(Some("application/x-msdownload"));
        assert!(!denied.accepted);
        assert_eq!(denied.reason_code, "deny_content_type_dangerous");

        let allowed = evaluate_attachment_content_type_policy(Some("text/plain"));
        assert!(allowed.accepted);
        assert_eq!(allowed.reason_code, "allow_content_type_default");
    }

    #[test]
    fn functional_render_event_prompt_includes_downloaded_attachments() {
        let repo = RepoRef::parse("owner/repo").expect("repo");
        let event = test_issue_event();
        let attachments = vec![DownloadedGithubAttachment {
            source_url: "https://example.com/files/trace.log".to_string(),
            original_name: "trace.log".to_string(),
            path: PathBuf::from("/tmp/attachments/trace.log"),
            relative_path: "attachments/issue-comment-created_1/01-trace.log".to_string(),
            content_type: Some("text/plain".to_string()),
            bytes: 42,
            checksum_sha256: "abc123".to_string(),
            policy_reason_code: "allow_extension_allowlist".to_string(),
            created_unix_ms: 1,
            expires_unix_ms: Some(1000),
        }];
        let prompt = render_event_prompt(&repo, &event, "inspect this", &attachments);
        assert!(prompt.contains("Downloaded attachments:"));
        assert!(prompt.contains("name=trace.log"));
        assert!(prompt.contains("source_url=https://example.com/files/trace.log"));
        assert!(prompt.contains("policy_reason=allow_extension_allowlist"));
    }

    #[tokio::test]
    async fn unit_issue_chat_continuity_summary_digest_is_deterministic_and_tracks_changes() {
        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config("http://127.0.0.1", temp.path());
        let runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let issue_number = 77_u64;
        let session_path = session_path_for_issue(&runtime.repository_state_dir, issue_number);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("session store");
        store
            .append_messages(
                None,
                &[Message::user("alpha"), Message::assistant_text("beta")],
            )
            .expect("append entries");

        let first = runtime
            .issue_chat_continuity_summary(issue_number)
            .expect("first summary");
        let second = runtime
            .issue_chat_continuity_summary(issue_number)
            .expect("second summary");
        assert_eq!(first.lineage_digest_sha256, second.lineage_digest_sha256);
        assert_eq!(first.entries, 2);
        assert_eq!(first.oldest_entry_id, Some(1));
        assert_eq!(first.newest_entry_id, Some(2));
        assert_eq!(first.newest_entry_role.as_deref(), Some("assistant"));
        assert_eq!(first.artifacts.total_records, 0);
        assert_eq!(first.artifacts.active_records, 0);

        let channel_store = ChannelStore::open(
            &runtime.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )
        .expect("channel store");
        channel_store
            .write_text_artifact(
                "run-77",
                "github-issue-chat-export",
                "private",
                Some(30),
                "jsonl",
                "{\"sample\":true}",
            )
            .expect("write artifact");

        let mut store = SessionStore::load(&session_path).expect("reload store");
        let head = store.head_id();
        store
            .append_messages(head, &[Message::user("gamma")])
            .expect("append change");

        let third = runtime
            .issue_chat_continuity_summary(issue_number)
            .expect("third summary");
        assert_ne!(first.lineage_digest_sha256, third.lineage_digest_sha256);
        assert_eq!(third.entries, 3);
        assert_eq!(third.newest_entry_id, Some(3));
        assert_eq!(third.newest_entry_role.as_deref(), Some("user"));
        assert_eq!(third.artifacts.total_records, 1);
        assert_eq!(third.artifacts.active_records, 1);
        assert!(third.artifacts.latest_artifact_id.is_some());
        assert_eq!(
            third.artifacts.latest_artifact_run_id.as_deref(),
            Some("run-77")
        );
    }

    #[tokio::test]
    async fn functional_render_issue_status_includes_chat_digest_and_artifact_fields() {
        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config("http://127.0.0.1", temp.path());
        let runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let issue_number = 78_u64;
        let session_path = session_path_for_issue(&runtime.repository_state_dir, issue_number);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("store");
        store
            .append_messages(None, &[Message::user("status check")])
            .expect("append");
        let channel_store = ChannelStore::open(
            &runtime.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )
        .expect("channel store");
        channel_store
            .write_text_artifact(
                "run-78",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "status artifact",
            )
            .expect("artifact");

        let status = runtime.render_issue_status(issue_number);
        assert!(status.contains("chat_lineage_digest_sha256: "));
        assert!(status.contains("chat_entries: 1"));
        assert!(status.contains("artifacts_total: 1"));
        assert!(status.contains("artifacts_active: 1"));
        assert!(status.contains("artifacts_latest_id: artifact-"));
        assert!(status.contains("transport_failure_streak: 0"));
        assert!(status.contains("transport_last_cycle_processed: 0"));
    }

    #[tokio::test]
    async fn functional_render_issue_health_includes_classification_and_transport_fields() {
        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config("http://127.0.0.1", temp.path());
        let runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let health = runtime.render_issue_health(78);
        assert!(health.contains("Tau health for issue #78: healthy"));
        assert!(health.contains("runtime_state: idle"));
        assert!(health.contains("active_run_id: none"));
        assert!(health.contains("transport_health_reason: "));
        assert!(health.contains("transport_health_recommendation: "));
        assert!(health.contains("transport_failure_streak: 0"));
    }

    #[tokio::test]
    async fn regression_render_issue_health_reports_failing_failure_streak() {
        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config("http://127.0.0.1", temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let mut health = runtime.state_store.transport_health().clone();
        health.failure_streak = 3;
        runtime.state_store.update_transport_health(health);
        let rendered = runtime.render_issue_health(7);
        assert!(rendered.contains("Tau health for issue #7: failing"));
        assert!(rendered.contains("failure_streak=3"));
    }

    #[tokio::test]
    async fn regression_render_issue_status_defaults_health_lines_for_legacy_state() {
        let temp = tempdir().expect("tempdir");
        let repo_state_dir = temp.path().join("owner__repo");
        std::fs::create_dir_all(&repo_state_dir).expect("repo state dir");
        std::fs::write(
            repo_state_dir.join("state.json"),
            r#"{
  "schema_version": 1,
  "last_issue_scan_at": null,
  "processed_event_keys": [],
  "issue_sessions": {}
}
"#,
        )
        .expect("write legacy state");

        let config = test_bridge_config("http://127.0.0.1", temp.path());
        let runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let status = runtime.render_issue_status(7);
        assert!(status.contains("transport_failure_streak: 0"));
        assert!(status.contains("transport_last_cycle_processed: 0"));
    }

    #[test]
    fn unit_render_issue_comment_chunks_split_and_keep_marker_in_first_chunk() {
        let event = test_issue_event();
        let report = test_prompt_run_report(&"a".repeat(240));
        let (content, footer) = render_issue_comment_response_parts(&event, &report);
        let footer_block = format!("\n\n---\n{footer}");
        let max_chars = footer_block.chars().count() + 10;
        assert!(content.chars().count() > 10);
        let chunks = render_issue_comment_chunks_with_limit(&event, &report, max_chars);
        assert!(chunks.len() > 1);
        assert!(chunks[0].contains(EVENT_KEY_MARKER_PREFIX));
        assert!(chunks
            .iter()
            .skip(1)
            .all(|chunk| !chunk.contains(EVENT_KEY_MARKER_PREFIX)));
        assert!(chunks
            .iter()
            .all(|chunk| chunk.chars().count() <= max_chars));
    }

    #[tokio::test]
    async fn functional_post_issue_comment_chunks_updates_and_appends() {
        let server = MockServer::start();
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/901")
                .body_includes("chunk-1");
            then.status(200).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });
        let append_one = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/7/comments")
                .body_includes("chunk-2");
            then.status(201).json_body(json!({
                "id": 902,
                "html_url": "https://example.test/comment/902"
            }));
        });
        let append_two = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/7/comments")
                .body_includes("chunk-3");
            then.status(201).json_body(json!({
                "id": 903,
                "html_url": "https://example.test/comment/903"
            }));
        });

        let client = GithubApiClient::new(
            server.base_url(),
            "token".to_string(),
            RepoRef::parse("owner/repo").expect("repo"),
            2_000,
            1,
            1,
        )
        .expect("client");
        let chunks = vec![
            "chunk-1".to_string(),
            "chunk-2".to_string(),
            "chunk-3".to_string(),
        ];
        let outcome = post_issue_comment_chunks(&client, 7, 901, &chunks).await;
        assert!(outcome.edit_attempted);
        assert!(outcome.edit_success);
        assert_eq!(outcome.append_count, 2);
        assert_eq!(outcome.posted_comment_id, Some(903));
        update.assert_calls(1);
        append_one.assert_calls(1);
        append_two.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_post_issue_comment_chunks_falls_back_on_edit_failure() {
        let server = MockServer::start();
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/901");
            then.status(500);
        });
        let fallback = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/7/comments")
                .body_includes("warning: failed to update placeholder comment");
            then.status(201).json_body(json!({
                "id": 910,
                "html_url": "https://example.test/comment/910"
            }));
        });
        let append = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/7/comments")
                .body_includes("chunk-2");
            then.status(201).json_body(json!({
                "id": 911,
                "html_url": "https://example.test/comment/911"
            }));
        });

        let client = GithubApiClient::new(
            server.base_url(),
            "token".to_string(),
            RepoRef::parse("owner/repo").expect("repo"),
            2_000,
            1,
            1,
        )
        .expect("client");
        let chunks = vec!["chunk-1".to_string(), "chunk-2".to_string()];
        let outcome = post_issue_comment_chunks(&client, 7, 901, &chunks).await;
        assert!(outcome.edit_attempted);
        assert!(!outcome.edit_success);
        assert_eq!(outcome.append_count, 2);
        assert_eq!(outcome.posted_comment_id, Some(911));
        update.assert_calls(1);
        fallback.assert_calls(1);
        append.assert_calls(1);
    }

    #[test]
    fn unit_parse_tau_issue_command_supports_known_commands() {
        assert_eq!(
            parse_tau_issue_command("/tau run investigate failures"),
            Some(TauIssueCommand::Run {
                prompt: "investigate failures".to_string()
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau status"),
            Some(TauIssueCommand::Status)
        );
        assert_eq!(
            parse_tau_issue_command("/tau health"),
            Some(TauIssueCommand::Health)
        );
        assert_eq!(
            parse_tau_issue_command("/tau stop"),
            Some(TauIssueCommand::Stop)
        );
        assert_eq!(
            parse_tau_issue_command("/tau help"),
            Some(TauIssueCommand::Help)
        );
        assert_eq!(
            parse_tau_issue_command("/tau summarize release blockers"),
            Some(TauIssueCommand::Summarize {
                focus: Some("release blockers".to_string())
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat start"),
            Some(TauIssueCommand::ChatStart)
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat resume"),
            Some(TauIssueCommand::ChatResume)
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat reset"),
            Some(TauIssueCommand::ChatReset)
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat export"),
            Some(TauIssueCommand::ChatExport)
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat status"),
            Some(TauIssueCommand::ChatStatus)
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat show"),
            Some(TauIssueCommand::ChatShow {
                limit: CHAT_SHOW_DEFAULT_LIMIT
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat show 25"),
            Some(TauIssueCommand::ChatShow { limit: 25 })
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat search alpha"),
            Some(TauIssueCommand::ChatSearch {
                query: "alpha".to_string(),
                role: None,
                limit: crate::session_commands::SESSION_SEARCH_DEFAULT_RESULTS,
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau chat search alpha --role user --limit 25"),
            Some(TauIssueCommand::ChatSearch {
                query: "alpha".to_string(),
                role: Some("user".to_string()),
                limit: 25,
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau artifacts"),
            Some(TauIssueCommand::Artifacts {
                purge: false,
                run_id: None
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau artifacts purge"),
            Some(TauIssueCommand::Artifacts {
                purge: true,
                run_id: None
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau artifacts run run-seeded"),
            Some(TauIssueCommand::Artifacts {
                purge: false,
                run_id: Some("run-seeded".to_string())
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau artifacts show artifact-123"),
            Some(TauIssueCommand::ArtifactShow {
                artifact_id: "artifact-123".to_string()
            })
        );
        assert_eq!(
            parse_tau_issue_command("/tau canvas show architecture --json"),
            Some(TauIssueCommand::Canvas {
                args: "show architecture --json".to_string()
            })
        );
        assert_eq!(parse_tau_issue_command("plain message"), None);
    }

    #[test]
    fn regression_parse_tau_issue_command_rejects_slash_like_inputs() {
        assert_eq!(parse_tau_issue_command("/taui run nope"), None);
        let parsed = parse_tau_issue_command("/tau run").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau artifacts extra").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau artifacts run").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed =
            parse_tau_issue_command("/tau artifacts run run-a run-b").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau artifacts show").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed =
            parse_tau_issue_command("/tau artifacts show artifact-a extra").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau canvas").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau help extra").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau health extra").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat start now").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat export now").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat status now").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat show foo").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat show 99 100").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat search").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed =
            parse_tau_issue_command("/tau chat search alpha --role nope").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed =
            parse_tau_issue_command("/tau chat search alpha --limit 0").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed =
            parse_tau_issue_command("/tau chat search alpha --limit 99").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let parsed = parse_tau_issue_command("/tau chat unknown").expect("command parse");
        assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
        let action = event_action_from_body("/tau unknown");
        assert!(matches!(
            action,
            EventAction::Command(TauIssueCommand::Invalid { .. })
        ));
    }

    #[tokio::test]
    async fn integration_github_api_client_retries_rate_limits() {
        let server = MockServer::start();
        let first = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues")
                .header("x-tau-retry-attempt", "0");
            then.status(429)
                .header("retry-after", "0")
                .body("rate limit");
        });
        let second = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues")
                .header("x-tau-retry-attempt", "1");
            then.status(200).json_body(json!([]));
        });

        let repo = RepoRef::parse("owner/repo").expect("repo parse");
        let client =
            GithubApiClient::new(server.base_url(), "token".to_string(), repo, 2_000, 3, 1)
                .expect("client");
        let issues = client
            .list_updated_issues(None)
            .await
            .expect("list issues should eventually succeed");
        assert!(issues.is_empty());
        assert_eq!(first.calls(), 1);
        assert_eq!(second.calls(), 1);
    }

    #[tokio::test]
    async fn integration_bridge_poll_processes_issue_comment_and_posts_reply() {
        let server = MockServer::start();
        let issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 10,
                "number": 7,
                "title": "Bridge me",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let comments = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/7/comments");
            then.status(200).json_body(json!([{
                "id": 200,
                "body": "hello from issue stream",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let working_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/7/comments")
                .body_includes("Tau is working on run");
            then.status(201).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/901")
                .body_includes("bridge reply")
                .body_includes("tau-event-key:issue-comment-created:200")
                .body_includes("artifact `artifacts/");
            then.status(200).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });
        let fallback_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/7/comments")
                .body_includes("warning: failed to update placeholder comment");
            then.status(201).json_body(json!({
                "id": 999,
                "html_url": "https://example.test/comment/999"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        assert_eq!(first.discovered_events, 1);
        assert_eq!(first.processed_events, 1);
        assert_eq!(first.failed_events, 0);

        let state_path = temp.path().join("owner__repo").join("state.json");
        let state_raw = std::fs::read_to_string(&state_path).expect("state file");
        let state: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
        let health = state
            .get("health")
            .and_then(serde_json::Value::as_object)
            .expect("health object");
        assert_eq!(
            health
                .get("last_cycle_discovered")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            health
                .get("last_cycle_processed")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert_eq!(
            health
                .get("last_cycle_failed")
                .and_then(serde_json::Value::as_u64),
            Some(0)
        );
        assert_eq!(
            health
                .get("failure_streak")
                .and_then(serde_json::Value::as_u64),
            Some(0)
        );
        assert!(
            health
                .get("updated_unix_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default()
                > 0
        );

        let second = runtime.poll_once().await.expect("second poll");
        assert_eq!(second.processed_events, 0);
        issues.assert_calls(2);
        comments.assert_calls(2);
        working_post.assert_calls(1);
        update.assert_calls(1);
        fallback_post.assert_calls(0);

        let outbound = std::fs::read_to_string(
            temp.path()
                .join("owner__repo")
                .join("outbound-events.jsonl"),
        )
        .expect("read outbound log");
        assert!(outbound.contains("\"posted_comment_id\":901"));
        let channel_dir = temp
            .path()
            .join("owner__repo")
            .join("channel-store/channels/github/issue-7");
        let channel_log =
            std::fs::read_to_string(channel_dir.join("log.jsonl")).expect("channel log exists");
        let channel_context = std::fs::read_to_string(channel_dir.join("context.jsonl"))
            .expect("channel context exists");
        assert!(channel_log.contains("\"direction\":\"inbound\""));
        assert!(channel_log.contains("\"direction\":\"outbound\""));
        assert!(channel_log.contains("\"artifact\""));
        assert!(channel_context.contains("bridge reply"));
        let artifact_index = std::fs::read_to_string(channel_dir.join("artifacts/index.jsonl"))
            .expect("artifact index exists");
        assert!(artifact_index.contains("\"artifact_type\":\"github-issue-reply\""));
    }

    #[tokio::test]
    async fn integration_run_prompt_for_event_downloads_issue_attachments_and_records_provenance() {
        let server = MockServer::start();
        let attachment_url = format!("{}/assets/trace.log", server.base_url());
        let attachment_download = server.mock(|when, then| {
            when.method(GET).path("/assets/trace.log");
            then.status(200)
                .header("content-type", "text/plain")
                .body("trace-line-1\ntrace-line-2\n");
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let repo = RepoRef::parse("owner/repo").expect("repo");
        let github_client = GithubApiClient::new(
            server.base_url(),
            "token".to_string(),
            repo.clone(),
            2_000,
            1,
            1,
        )
        .expect("github client");
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let event = GithubBridgeEvent {
            key: "issue-comment-created:1200".to_string(),
            kind: GithubBridgeEventKind::CommentCreated,
            issue_number: 20,
            issue_title: "Attachment".to_string(),
            author_login: "alice".to_string(),
            occurred_at: "2026-01-01T00:00:01Z".to_string(),
            body: attachment_url.clone(),
            raw_payload: json!({"id": 1200}),
        };
        let report = run_prompt_for_event(RunPromptForEventRequest {
            config: &config,
            github_client: &github_client,
            repo: &repo,
            repository_state_dir: temp.path(),
            event: &event,
            prompt: &attachment_url,
            run_id: "run-attachment",
            cancel_rx,
        })
        .await
        .expect("run prompt");
        assert_eq!(report.downloaded_attachments.len(), 1);
        assert_eq!(report.downloaded_attachments[0].source_url, attachment_url);
        attachment_download.assert_calls(1);

        let channel_store =
            ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-20")
                .expect("channel store");
        let attachment_dir = channel_store
            .attachments_dir()
            .join(sanitize_for_path("issue-comment-created:1200"));
        assert!(attachment_dir.exists());
        let attachment_entries = std::fs::read_dir(&attachment_dir)
            .expect("read attachment dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect attachments");
        assert_eq!(attachment_entries.len(), 1);
        let attachment_payload =
            std::fs::read_to_string(attachment_entries[0].path()).expect("attachment payload");
        assert!(attachment_payload.contains("trace-line-1"));

        let channel_log = std::fs::read_to_string(channel_store.log_path()).expect("channel log");
        assert!(channel_log.contains("\"downloaded_attachments\""));
        assert!(channel_log.contains("\"policy_reason_code\""));

        let attachment_manifest = channel_store
            .load_attachment_records_tolerant()
            .expect("attachment manifest");
        assert_eq!(attachment_manifest.records.len(), 1);
        assert_eq!(attachment_manifest.records[0].event_key, event.key);
        assert_eq!(attachment_manifest.records[0].actor, "alice");
        assert_eq!(attachment_manifest.records[0].source_url, attachment_url);
        assert_eq!(attachment_manifest.records[0].policy_decision, "accepted");
        assert_eq!(
            attachment_manifest.records[0].policy_reason_code,
            "allow_extension_allowlist"
        );
        assert!(attachment_manifest.records[0].expires_unix_ms.is_some());

        let artifacts = channel_store
            .load_artifact_records_tolerant()
            .expect("artifact records");
        assert_eq!(artifacts.records.len(), 1);
        let artifact_payload = std::fs::read_to_string(
            channel_store
                .channel_dir()
                .join(&artifacts.records[0].relative_path),
        )
        .expect("artifact payload");
        assert!(artifact_payload.contains("attachments: 1"));
        assert!(artifact_payload.contains("source_url=http://"));
    }

    #[tokio::test]
    async fn functional_run_prompt_for_event_attachment_policy_rejects_denylisted_extensions() {
        let server = MockServer::start();
        let accepted_url = format!("{}/assets/trace.log", server.base_url());
        let denied_url = format!("{}/assets/run.exe", server.base_url());
        let accepted_download = server.mock(|when, then| {
            when.method(GET).path("/assets/trace.log");
            then.status(200)
                .header("content-type", "text/plain")
                .body("trace-line-1\ntrace-line-2\n");
        });
        let denied_download = server.mock(|when, then| {
            when.method(GET).path("/assets/run.exe");
            then.status(200)
                .header("content-type", "application/octet-stream")
                .body("binary");
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let repo = RepoRef::parse("owner/repo").expect("repo");
        let github_client = GithubApiClient::new(
            server.base_url(),
            "token".to_string(),
            repo.clone(),
            2_000,
            1,
            1,
        )
        .expect("github client");
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let event = GithubBridgeEvent {
            key: "issue-comment-created:1201".to_string(),
            kind: GithubBridgeEventKind::CommentCreated,
            issue_number: 21,
            issue_title: "Attachment policy".to_string(),
            author_login: "alice".to_string(),
            occurred_at: "2026-01-01T00:00:01Z".to_string(),
            body: format!("{accepted_url}\n{denied_url}"),
            raw_payload: json!({"id": 1201}),
        };
        let report = run_prompt_for_event(RunPromptForEventRequest {
            config: &config,
            github_client: &github_client,
            repo: &repo,
            repository_state_dir: temp.path(),
            event: &event,
            prompt: &event.body,
            run_id: "run-attachment-policy",
            cancel_rx,
        })
        .await
        .expect("run prompt");
        assert_eq!(report.downloaded_attachments.len(), 1);
        assert_eq!(report.downloaded_attachments[0].source_url, accepted_url);
        accepted_download.assert_calls(1);
        denied_download.assert_calls(0);

        let channel_store =
            ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-21")
                .expect("channel store");
        let attachment_manifest = channel_store
            .load_attachment_records_tolerant()
            .expect("attachment manifest");
        assert_eq!(attachment_manifest.records.len(), 1);
        assert_eq!(attachment_manifest.records[0].source_url, accepted_url);
    }

    #[tokio::test]
    async fn integration_bridge_poll_denies_unpaired_actor_in_strict_mode() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 25,
                "number": 77,
                "title": "Strict policy",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/77/comments");
            then.status(200).json_body(json!([{
                "id": 7701,
                "body": "run anyway",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let working_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/77/comments")
                .body_includes("Tau is working on run");
            then.status(201).json_body(json!({
                "id": 7777,
                "html_url": "https://example.test/comment/7777"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let security_dir = temp.path().join("security");
        std::fs::create_dir_all(&security_dir).expect("security dir");
        std::fs::write(
            security_dir.join("allowlist.json"),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {}
}
"#,
        )
        .expect("write strict allowlist");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.discovered_events, 1);
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        working_post.assert_calls(0);

        let outbound = std::fs::read_to_string(
            temp.path()
                .join("owner__repo")
                .join("outbound-events.jsonl"),
        )
        .expect("read outbound log");
        assert!(outbound.contains("\"status\":\"denied\""));
        assert!(outbound.contains("\"reason_code\":\"deny_actor_not_paired_or_allowlisted\""));
    }

    #[tokio::test]
    async fn integration_bridge_poll_denies_unbound_actor_in_rbac_team_mode() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 31,
                "number": 88,
                "title": "RBAC policy",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/88/comments");
            then.status(200).json_body(json!([{
                "id": 8801,
                "body": "/tau status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/88/comments")
                .body_includes("Current status for issue");
            then.status(201).json_body(json!({
                "id": 8888,
                "html_url": "https://example.test/comment/8888"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let security_dir = temp.path().join("security");
        std::fs::create_dir_all(&security_dir).expect("security dir");
        std::fs::write(
            security_dir.join("allowlist.json"),
            r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
    "github:owner/repo": ["alice"]
  }
}
"#,
        )
        .expect("write strict allowlist");
        std::fs::write(
            security_dir.join("rbac.json"),
            r#"{
  "schema_version": 1,
  "team_mode": true,
  "bindings": [],
  "roles": {}
}
"#,
        )
        .expect("write rbac policy");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.discovered_events, 1);
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        status_post.assert_calls(0);

        let outbound = std::fs::read_to_string(
            temp.path()
                .join("owner__repo")
                .join("outbound-events.jsonl"),
        )
        .expect("read outbound log");
        assert!(outbound.contains("\"command\":\"rbac-authorization\""));
        assert!(outbound.contains("\"status\":\"denied\""));
        assert!(outbound.contains("\"reason_code\":\"deny_unbound_principal\""));
    }

    #[tokio::test]
    async fn regression_bridge_poll_replay_does_not_duplicate_responses() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 11,
                "number": 8,
                "title": "Replay",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/8/comments");
            then.status(200).json_body(json!([{
                "id": 201,
                "body": "same comment every poll",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let working_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/8/comments")
                .body_includes("Tau is working on run");
            then.status(201).json_body(json!({
                "id": 902,
                "html_url": "https://example.test/comment/902"
            }));
        });
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/902")
                .body_includes("tau-event-key:issue-comment-created:201");
            then.status(200).json_body(json!({
                "id": 902,
                "html_url": "https://example.test/comment/902"
            }));
        });
        let fallback_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/8/comments")
                .body_includes("warning: failed to update placeholder comment");
            then.status(201).json_body(json!({
                "id": 903,
                "html_url": "https://example.test/comment/903"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        assert_eq!(first.processed_events, 1);
        let second = runtime.poll_once().await.expect("second poll");
        assert_eq!(second.processed_events, 0);
        assert_eq!(second.skipped_duplicate_events, 1);
        working_post.assert_calls(1);
        update.assert_calls(1);
        fallback_post.assert_calls(0);
    }

    #[tokio::test]
    async fn regression_bridge_poll_hydrates_command_replay_markers_from_existing_bot_comments() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 40,
                "number": 19,
                "title": "Replay Marker",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/19/comments");
            then.status(200).json_body(json!([
                {
                    "id": 1901,
                    "body": "/tau status",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 1902,
                    "body": "Tau status for issue #19: idle\n\n---\n<!-- tau-event-key:issue-comment-created:1901 -->\n_Tau command `status` | status `reported`_",
                    "created_at": "2026-01-01T00:00:02Z",
                    "updated_at": "2026-01-01T00:00:02Z",
                    "user": {"login":"tau"}
                }
            ]));
        });
        let status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/19/comments")
                .body_includes("Tau status for issue #19: idle");
            then.status(201).json_body(json!({
                "id": 1903,
                "html_url": "https://example.test/comment/1903"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.discovered_events, 1);
        assert_eq!(report.processed_events, 0);
        assert_eq!(report.skipped_duplicate_events, 1);
        assert_eq!(report.failed_events, 0);
        status_post.assert_calls(0);
    }

    #[tokio::test]
    async fn integration_bridge_commands_status_stop_and_health_produce_control_comments() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 12,
                "number": 9,
                "title": "Control",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/9/comments");
            then.status(200).json_body(json!([
                {
                    "id": 301,
                    "body": "/tau status",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 302,
                    "body": "/tau stop",
                    "created_at": "2026-01-01T00:00:02Z",
                    "updated_at": "2026-01-01T00:00:02Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 303,
                    "body": "/tau health",
                    "created_at": "2026-01-01T00:00:03Z",
                    "updated_at": "2026-01-01T00:00:03Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("Tau status for issue #9: idle")
                .body_includes("transport_failure_streak: 0");
            then.status(201).json_body(json!({
                "id": 930,
                "html_url": "https://example.test/comment/930"
            }));
        });
        let stop_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("No active run for this issue. Current state is idle.");
            then.status(201).json_body(json!({
                "id": 931,
                "html_url": "https://example.test/comment/931"
            }));
        });
        let health_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("Tau health for issue #9: healthy")
                .body_includes("transport_health_reason:")
                .body_includes("transport_health_recommendation:");
            then.status(201).json_body(json!({
                "id": 932,
                "html_url": "https://example.test/comment/932"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 3);
        assert_eq!(report.failed_events, 0);
        status_post.assert_calls(1);
        stop_post.assert_calls(1);
        health_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_commands_manage_sessions() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 12,
                "number": 9,
                "title": "Chat Control",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/9/comments");
            then.status(200).json_body(json!([
                {
                    "id": 311,
                    "body": "/tau chat start",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 312,
                    "body": "/tau chat resume",
                    "created_at": "2026-01-01T00:00:02Z",
                    "updated_at": "2026-01-01T00:00:02Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 313,
                    "body": "/tau chat reset",
                    "created_at": "2026-01-01T00:00:03Z",
                    "updated_at": "2026-01-01T00:00:03Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let chat_start_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("Chat session started for issue #9.");
            then.status(201).json_body(json!({
                "id": 940,
                "html_url": "https://example.test/comment/940"
            }));
        });
        let chat_resume_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("Chat session resumed for issue #9.");
            then.status(201).json_body(json!({
                "id": 941,
                "html_url": "https://example.test/comment/941"
            }));
        });
        let chat_reset_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("Chat session reset for issue #9.");
            then.status(201).json_body(json!({
                "id": 942,
                "html_url": "https://example.test/comment/942"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 3);
        assert_eq!(report.failed_events, 0);
        chat_start_post.assert_calls(1);
        chat_resume_post.assert_calls(1);
        chat_reset_post.assert_calls(1);
        assert!(runtime.state_store.issue_session(9).is_none());
        let session_path = session_path_for_issue(&runtime.repository_state_dir, 9);
        assert!(!session_path.exists());
    }

    #[tokio::test]
    async fn integration_bridge_chat_export_posts_artifact() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 18,
                "number": 11,
                "title": "Export Chat",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/11/comments");
            then.status(200).json_body(json!([{
                "id": 411,
                "body": "/tau chat export",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let export_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/11/comments")
                .body_includes("Chat session export ready for issue #11.")
                .body_includes("artifact_path=artifacts/chat-export-11/");
            then.status(201).json_body(json!({
                "id": 960,
                "html_url": "https://example.test/comment/960"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let session_path = session_path_for_issue(&runtime.repository_state_dir, 11);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("store");
        store
            .append_messages(
                None,
                &[
                    Message::user("Export this"),
                    Message::assistant_text("Ready"),
                ],
            )
            .expect("append messages");

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        export_post.assert_calls(1);

        let channel_store = ChannelStore::open(
            &runtime.repository_state_dir.join("channel-store"),
            "github",
            "issue-11",
        )
        .expect("channel store");
        let loaded = channel_store
            .load_artifact_records_tolerant()
            .expect("load artifacts");
        assert_eq!(loaded.records.len(), 1);
        let record = &loaded.records[0];
        assert_eq!(record.artifact_type, "github-issue-chat-export");
        assert!(record.relative_path.contains("artifacts/chat-export-11/"));
        let artifact_path = channel_store.channel_dir().join(&record.relative_path);
        let payload = std::fs::read_to_string(&artifact_path).expect("read artifact");
        assert!(payload.contains("\"schema_version\""));
        assert!(payload.contains("\"message\""));
    }

    #[tokio::test]
    async fn integration_bridge_chat_status_reports_session_state() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 20,
                "number": 12,
                "title": "Chat Status",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/12/comments");
            then.status(200).json_body(json!([{
                "id": 511,
                "body": "/tau chat status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/12/comments")
                .body_includes("Chat session status for issue #12.")
                .body_includes("entries=2")
                .body_includes("lineage_digest_sha256=")
                .body_includes("artifact_active=0")
                .body_includes("artifact_total=0")
                .body_includes("session_id=issue-12")
                .body_includes("last_comment_id=900")
                .body_includes("last_run_id=run-12");
            then.status(201).json_body(json!({
                "id": 970,
                "html_url": "https://example.test/comment/970"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let session_path = session_path_for_issue(&runtime.repository_state_dir, 12);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("store");
        store
            .append_messages(
                None,
                &[
                    Message::user("Status this"),
                    Message::assistant_text("Status ready"),
                ],
            )
            .expect("append messages");
        runtime.state_store.update_issue_session(
            12,
            issue_session_id(12),
            Some(900),
            Some("run-12".to_string()),
        );

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        status_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_status_reports_missing_session() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 21,
                "number": 13,
                "title": "Chat Status None",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/13/comments");
            then.status(200).json_body(json!([{
                "id": 611,
                "body": "/tau chat status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/13/comments")
                .body_includes("No chat session found for issue #13.")
                .body_includes("entries=0")
                .body_includes("session_id=none")
                .body_includes("lineage_digest_sha256=")
                .body_includes("artifact_active=0")
                .body_includes("artifact_total=0");
            then.status(201).json_body(json!({
                "id": 971,
                "html_url": "https://example.test/comment/971"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        status_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_show_reports_recent_messages() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 22,
                "number": 14,
                "title": "Chat Show",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/14/comments");
            then.status(200).json_body(json!([{
                "id": 711,
                "body": "/tau chat show 2",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let show_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/14/comments")
                .body_includes("Chat session show for issue #14.")
                .body_includes("showing_last=2")
                .body_includes("role=assistant")
                .body_includes("role=user");
            then.status(201).json_body(json!({
                "id": 972,
                "html_url": "https://example.test/comment/972"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let session_path = session_path_for_issue(&runtime.repository_state_dir, 14);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("store");
        store
            .append_messages(
                None,
                &[
                    Message::user("First message"),
                    Message::assistant_text("Second message"),
                    Message::user("Third message"),
                ],
            )
            .expect("append messages");

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        show_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_show_reports_missing_session() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 23,
                "number": 15,
                "title": "Chat Show None",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/15/comments");
            then.status(200).json_body(json!([{
                "id": 811,
                "body": "/tau chat show",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let show_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/15/comments")
                .body_includes("No chat session found for issue #15.")
                .body_includes("entries=0");
            then.status(201).json_body(json!({
                "id": 973,
                "html_url": "https://example.test/comment/973"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        show_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_search_reports_matches() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 24,
                "number": 16,
                "title": "Chat Search",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/16/comments");
            then.status(200).json_body(json!([{
                "id": 911,
                "body": "/tau chat search alpha --limit 5",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let search_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/16/comments")
                .body_includes("Chat session search for issue #16.")
                .body_includes("query=alpha")
                .body_includes("matches=");
            then.status(201).json_body(json!({
                "id": 974,
                "html_url": "https://example.test/comment/974"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let session_path = session_path_for_issue(&runtime.repository_state_dir, 16);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("store");
        store
            .append_messages(
                None,
                &[
                    Message::user("alpha message"),
                    Message::assistant_text("beta response"),
                ],
            )
            .expect("append messages");

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        search_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_search_reports_missing_session() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 25,
                "number": 17,
                "title": "Chat Search None",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/17/comments");
            then.status(200).json_body(json!([{
                "id": 1011,
                "body": "/tau chat search alpha",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let search_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/17/comments")
                .body_includes("No chat session found for issue #17.")
                .body_includes("entries=0");
            then.status(201).json_body(json!({
                "id": 975,
                "html_url": "https://example.test/comment/975"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        search_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_help_command_posts_usage() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 14,
                "number": 10,
                "title": "Help",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/10/comments");
            then.status(200).json_body(json!([
                {
                    "id": 321,
                    "body": "/tau help",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let help_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/10/comments")
                .body_includes("Supported `/tau` commands:");
            then.status(201).json_body(json!({
                "id": 950,
                "html_url": "https://example.test/comment/950"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        help_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_canvas_command_persists_replay_safe_event() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 26,
                "number": 18,
                "title": "Canvas",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/18/comments");
            then.status(200).json_body(json!([{
                "id": 1112,
                "body": "/tau canvas create architecture",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let canvas_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/18/comments")
                .body_includes("canvas create: id=architecture")
                .body_includes("event_id=");
            then.status(201).json_body(json!({
                "id": 990,
                "html_url": "https://example.test/comment/990"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        canvas_post.assert_calls(1);

        let events_path = temp
            .path()
            .join("owner__repo/canvas/architecture/events.jsonl");
        let payload = std::fs::read_to_string(events_path).expect("events payload");
        assert!(payload.contains("\"event_id\":"));
        assert!(payload.contains("\"transport\":\"github\""));
        assert!(payload.contains("\"source_event_key\":\"issue-comment-created:1112\""));

        let links_path = temp
            .path()
            .join("owner__repo/canvas/architecture/session-links.jsonl");
        let links = std::fs::read_to_string(links_path).expect("session links");
        assert!(links.contains("\"canvas_id\":\"architecture\""));
    }

    #[tokio::test]
    async fn functional_render_issue_artifacts_filters_by_run_id() {
        let server = MockServer::start();
        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-15",
        )
        .expect("seeded store");
        seeded_store
            .write_text_artifact(
                "run-target",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "target artifact",
            )
            .expect("write target artifact");
        seeded_store
            .write_text_artifact(
                "run-other",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "other artifact",
            )
            .expect("write other artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime
            .render_issue_artifacts(15, Some("run-target"))
            .expect("render artifacts");
        assert!(report.contains("Tau artifacts for issue #15 run_id `run-target`: active=1"));
        assert!(report.contains("artifacts/run-target/"));
        assert!(!report.contains("artifacts/run-other/"));
    }

    #[tokio::test]
    async fn functional_render_issue_artifact_show_reports_active_and_expired_states() {
        let server = MockServer::start();
        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-17",
        )
        .expect("seeded store");
        let active = seeded_store
            .write_text_artifact(
                "run-active",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "active artifact",
            )
            .expect("write active artifact");
        let expired = seeded_store
            .write_text_artifact(
                "run-expired",
                "github-issue-reply",
                "private",
                Some(0),
                "md",
                "expired artifact",
            )
            .expect("write expired artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");

        let active_report = runtime
            .render_issue_artifact_show(17, &active.id)
            .expect("render active artifact");
        assert!(active_report.contains(&format!(
            "Tau artifact for issue #17 id `{}`: state=active",
            active.id
        )));
        assert!(active_report.contains("run_id: run-active"));

        let expired_report = runtime
            .render_issue_artifact_show(17, &expired.id)
            .expect("render expired artifact");
        assert!(expired_report.contains(&format!(
            "Tau artifact for issue #17 id `{}`: state=expired",
            expired.id
        )));
        assert!(expired_report
            .contains("artifact is expired and may be removed by `/tau artifacts purge`."));
    }

    #[tokio::test]
    async fn integration_bridge_artifacts_command_reports_recent_artifacts() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 14,
                "number": 11,
                "title": "Artifacts",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/11/comments");
            then.status(200).json_body(json!([
                {
                    "id": 501,
                    "body": "/tau artifacts",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let artifacts_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/11/comments")
                .body_includes("Tau artifacts for issue #11: active=1")
                .body_includes("github-issue-reply")
                .body_includes("artifacts/run-seeded/");
            then.status(201).json_body(json!({
                "id": 951,
                "html_url": "https://example.test/comment/951"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-11",
        )
        .expect("seeded store");
        seeded_store
            .write_text_artifact(
                "run-seeded",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "seeded artifact",
            )
            .expect("write seeded artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        artifacts_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_artifacts_run_filter_command_reports_matching_entries() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 18,
                "number": 15,
                "title": "Artifacts run filter",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/15/comments");
            then.status(200).json_body(json!([
                {
                    "id": 851,
                    "body": "/tau artifacts run run-target",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let artifacts_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/15/comments")
                .body_includes("Tau artifacts for issue #15 run_id `run-target`: active=1")
                .body_includes("artifacts/run-target/");
            then.status(201).json_body(json!({
                "id": 955,
                "html_url": "https://example.test/comment/955"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-15",
        )
        .expect("seeded store");
        seeded_store
            .write_text_artifact(
                "run-target",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "target artifact",
            )
            .expect("write target artifact");
        seeded_store
            .write_text_artifact(
                "run-other",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "other artifact",
            )
            .expect("write other artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        artifacts_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_artifacts_show_command_reports_artifact_details() {
        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-18",
        )
        .expect("seeded store");
        let artifact = seeded_store
            .write_text_artifact(
                "run-detail",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "detail artifact",
            )
            .expect("write detail artifact");

        let server = MockServer::start();
        let command_body = format!("/tau artifacts show {}", artifact.id);
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 20,
                "number": 18,
                "title": "Artifacts show",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(move |when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/18/comments");
            then.status(200).json_body(json!([
                {
                    "id": 871,
                    "body": command_body,
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let expected_header = format!(
            "Tau artifact for issue #18 id `{}`: state=active",
            artifact.id
        );
        let expected_path = format!("path: {}", artifact.relative_path);
        let artifacts_post = server.mock(move |when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/18/comments")
                .body_includes(&expected_header)
                .body_includes("run_id: run-detail")
                .body_includes(&expected_path);
            then.status(201).json_body(json!({
                "id": 957,
                "html_url": "https://example.test/comment/957"
            }));
        });

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        artifacts_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_artifacts_purge_command_removes_expired_entries() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 16,
                "number": 13,
                "title": "Artifact purge",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/13/comments");
            then.status(200).json_body(json!([
                {
                    "id": 701,
                    "body": "/tau artifacts purge",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let purge_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/13/comments")
                .body_includes("Tau artifact purge for issue #13")
                .body_includes("expired_removed=1")
                .body_includes("active_remaining=1");
            then.status(201).json_body(json!({
                "id": 953,
                "html_url": "https://example.test/comment/953"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-13",
        )
        .expect("seeded store");
        let expired = seeded_store
            .write_text_artifact(
                "run-expired",
                "github-issue-reply",
                "private",
                Some(0),
                "md",
                "expired artifact",
            )
            .expect("write expired artifact");
        seeded_store
            .write_text_artifact(
                "run-active",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "active artifact",
            )
            .expect("write active artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        purge_post.assert_calls(1);
        assert!(!seeded_store
            .channel_dir()
            .join(expired.relative_path)
            .exists());
    }

    #[tokio::test]
    async fn regression_bridge_artifacts_purge_command_noop_when_nothing_expired() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 17,
                "number": 14,
                "title": "Artifact purge no-op",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/14/comments");
            then.status(200).json_body(json!([
                {
                    "id": 801,
                    "body": "/tau artifacts purge",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let purge_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/14/comments")
                .body_includes("Tau artifact purge for issue #14")
                .body_includes("expired_removed=0")
                .body_includes("active_remaining=0");
            then.status(201).json_body(json!({
                "id": 954,
                "html_url": "https://example.test/comment/954"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        purge_post.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_bridge_artifacts_command_handles_malformed_index_and_empty_state() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 15,
                "number": 12,
                "title": "Artifact regression",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/12/comments");
            then.status(200).json_body(json!([
                {
                    "id": 601,
                    "body": "/tau artifacts",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let artifacts_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/12/comments")
                .body_includes("Tau artifacts for issue #12: active=0")
                .body_includes("none")
                .body_includes("index_invalid_lines: 1 (ignored)");
            then.status(201).json_body(json!({
                "id": 952,
                "html_url": "https://example.test/comment/952"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-12",
        )
        .expect("seeded store");
        std::fs::write(seeded_store.artifact_index_path(), "not-json\n").expect("seed invalid");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        artifacts_post.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_bridge_artifacts_run_filter_reports_none_for_unknown_run() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 19,
                "number": 16,
                "title": "Artifact run regression",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/16/comments");
            then.status(200).json_body(json!([
                {
                    "id": 861,
                    "body": "/tau artifacts run run-missing",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let artifacts_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/16/comments")
                .body_includes("Tau artifacts for issue #16 run_id `run-missing`: active=0")
                .body_includes("none for run_id `run-missing`");
            then.status(201).json_body(json!({
                "id": 956,
                "html_url": "https://example.test/comment/956"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-16",
        )
        .expect("seeded store");
        seeded_store
            .write_text_artifact(
                "run-other",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "other artifact",
            )
            .expect("write other artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        artifacts_post.assert_calls(1);
    }

    #[tokio::test]
    async fn regression_bridge_artifacts_show_command_reports_not_found_for_unknown_id() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 21,
                "number": 19,
                "title": "Artifact show missing",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/19/comments");
            then.status(200).json_body(json!([
                {
                    "id": 881,
                    "body": "/tau artifacts show artifact-missing",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let artifacts_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/19/comments")
                .body_includes("Tau artifact for issue #19 id `artifact-missing`: not found");
            then.status(201).json_body(json!({
                "id": 958,
                "html_url": "https://example.test/comment/958"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let seeded_store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-19",
        )
        .expect("seeded store");
        seeded_store
            .write_text_artifact(
                "run-known",
                "github-issue-reply",
                "private",
                Some(30),
                "md",
                "known artifact",
            )
            .expect("write known artifact");

        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        artifacts_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_stop_cancels_active_run_and_updates_state() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 13,
                "number": 10,
                "title": "Cancelable",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/10/comments");
            then.status(200).json_body(json!([
                {
                    "id": 401,
                    "body": "/tau run long diagnostic run",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 402,
                    "body": "/tau stop",
                    "created_at": "2026-01-01T00:00:02Z",
                    "updated_at": "2026-01-01T00:00:02Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let working_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/10/comments")
                .body_includes("Tau is working on run");
            then.status(201).json_body(json!({
                "id": 940,
                "html_url": "https://example.test/comment/940"
            }));
        });
        let stop_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/10/comments")
                .body_includes("Cancellation requested for run");
            then.status(201).json_body(json!({
                "id": 941,
                "html_url": "https://example.test/comment/941"
            }));
        });
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/940")
                .body_includes("status `cancelled`")
                .body_includes("Run cancelled by /tau stop.");
            then.status(200).json_body(json!({
                "id": 940,
                "html_url": "https://example.test/comment/940"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config_with_client(
            &server.base_url(),
            temp.path(),
            Arc::new(SlowReplyClient),
        );
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        assert_eq!(first.processed_events, 2);
        let second = runtime.poll_once().await.expect("second poll");
        assert_eq!(second.failed_events, 0);
        working_post.assert_calls(1);
        stop_post.assert_calls(1);
        update.assert_calls(1);
    }
}
