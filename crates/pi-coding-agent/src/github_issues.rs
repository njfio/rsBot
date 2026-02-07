use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use pi_agent_core::{Agent, AgentConfig, AgentEvent};
use pi_ai::LlmClient;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::watch;

use crate::{
    current_unix_timestamp_ms, run_prompt_with_cancellation, write_text_atomic, PromptRunStatus,
    RenderOptions, SessionRuntime,
};
use crate::{session::SessionStore, tools::ToolPolicy};

const GITHUB_STATE_SCHEMA_VERSION: u32 = 1;
const EVENT_KEY_MARKER_PREFIX: &str = "<!-- rsbot-event-key:";
const EVENT_KEY_MARKER_SUFFIX: &str = " -->";

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
}

impl Default for GithubIssuesBridgeState {
    fn default() -> Self {
        Self {
            schema_version: GITHUB_STATE_SCHEMA_VERSION,
            last_issue_scan_at: None,
            processed_event_keys: Vec::new(),
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
            serde_json::from_str::<GithubIssuesBridgeState>(&raw).with_context(|| {
                format!(
                    "failed to parse github issues bridge state file {}",
                    path.display()
                )
            })?
        } else {
            GithubIssuesBridgeState::default()
        };

        if state.schema_version != GITHUB_STATE_SCHEMA_VERSION {
            bail!(
                "unsupported github issues bridge state schema: expected {}, found {}",
                GITHUB_STATE_SCHEMA_VERSION,
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
            reqwest::header::HeaderValue::from_static("rsBot-github-issues-bridge"),
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

    async fn request_json<T, F>(&self, operation: &str, mut request_builder: F) -> Result<T>
    where
        T: DeserializeOwned,
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let mut attempt = 0_usize;
        loop {
            attempt = attempt.saturating_add(1);
            let response = request_builder()
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PiIssueCommand {
    Run { prompt: String },
    Stop,
    Status,
    Compact,
    Summarize { focus: Option<String> },
    Invalid { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EventAction {
    RunPrompt { prompt: String },
    Command(PiIssueCommand),
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
        loop {
            match self.poll_once().await {
                Ok(report) => {
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
        let mut report = PollCycleReport::default();
        tokio::task::yield_now().await;
        self.drain_finished_runs(&mut report).await?;

        let issues = self
            .github_client
            .list_updated_issues(self.state_store.last_issue_scan_at())
            .await?;
        let mut state_dirty = false;
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
                self.inbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key.clone(),
                    "kind": event.kind.as_str(),
                    "issue_number": event.issue_number,
                    "action": format!("{action:?}"),
                    "payload": event.raw_payload,
                }))?;

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

        self.drain_finished_runs(&mut report).await?;

        if self
            .state_store
            .update_last_issue_scan_at(latest_issue_scan)
        {
            state_dirty = true;
        }
        if state_dirty {
            self.state_store.save()?;
        }
        Ok(report)
    }

    async fn drain_finished_runs(&mut self, report: &mut PollCycleReport) -> Result<()> {
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
                    self.outbound_log.append(&json!({
                        "timestamp_unix_ms": current_unix_timestamp_ms(),
                        "repo": self.repo.as_slug(),
                        "event_key": result.event_key,
                        "issue_number": result.issue_number,
                        "run_id": result.run_id,
                        "status": result.status,
                        "posted_comment_id": result.posted_comment_id,
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
                        "A run is already active for this issue.\n\n{}\n\nUse `/pi stop` to cancel it first.",
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
                    "⏳ rsBot is working on run `{}` for event `{}`.",
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
        command: PiIssueCommand,
        report: &mut PollCycleReport,
        state_dirty: &mut bool,
    ) -> Result<()> {
        match command {
            PiIssueCommand::Run { prompt } => {
                return self
                    .enqueue_issue_run(event, prompt, report, state_dirty)
                    .await;
            }
            PiIssueCommand::Summarize { focus } => {
                let prompt = build_summarize_prompt(&self.repo, event, focus.as_deref());
                return self
                    .enqueue_issue_run(event, prompt, report, state_dirty)
                    .await;
            }
            PiIssueCommand::Stop => {
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
                    .github_client
                    .create_issue_comment(event.issue_number, &message)
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
            PiIssueCommand::Status => {
                let status = self.render_issue_status(event.issue_number);
                let posted = self
                    .github_client
                    .create_issue_comment(event.issue_number, &status)
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
            PiIssueCommand::Compact => {
                let session_path =
                    session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let compact_report = compact_issue_session(
                    &session_path,
                    self.config.session_lock_wait_ms,
                    self.config.session_lock_stale_ms,
                )?;
                let posted = self
                    .github_client
                    .create_issue_comment(
                        event.issue_number,
                        &format!(
                            "Session compact complete for issue #{}.\n\nremoved_entries={} retained_entries={} head={}",
                            event.issue_number,
                            compact_report.removed_entries,
                            compact_report.retained_entries,
                            compact_report
                                .head_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "none".to_string())
                        ),
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
            PiIssueCommand::Invalid { message } => {
                let posted = self
                    .github_client
                    .create_issue_comment(event.issue_number, &message)
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

    fn render_issue_status(&self, issue_number: u64) -> String {
        let active = self.active_runs.get(&issue_number);
        let latest = self.latest_runs.get(&issue_number);
        let state = if active.is_some() { "running" } else { "idle" };
        let mut lines = vec![format!("rsBot status for issue #{issue_number}: {state}")];
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
        lines.join("\n")
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
    let run_result = run_prompt_for_event(
        &config,
        &repo,
        &repository_state_dir,
        &event,
        &prompt,
        &run_id,
        cancel_rx,
    )
    .await;

    let completed_unix_ms = current_unix_timestamp_ms();
    let duration_ms = started.elapsed().as_millis() as u64;

    let (status, usage, body, error) = match run_result {
        Ok(run) => {
            let status = prompt_status_label(run.status).to_string();
            (
                status,
                run.usage.clone(),
                render_issue_comment_response(&event, &run),
                None,
            )
        }
        Err(error) => (
            "failed".to_string(),
            PromptUsageSummary::default(),
            render_issue_run_error_comment(&event, &run_id, &error),
            Some(error.to_string()),
        ),
    };

    let posted_comment_id = match github_client
        .update_issue_comment(working_comment_id, &body)
        .await
    {
        Ok(comment) => Some(comment.id),
        Err(update_error) => {
            let fallback_body = format!(
                "{body}\n\n_(warning: failed to update placeholder comment: {})_",
                truncate_for_error(&update_error.to_string(), 200)
            );
            match github_client
                .create_issue_comment(event.issue_number, &fallback_body)
                .await
            {
                Ok(comment) => Some(comment.id),
                Err(_) => None,
            }
        }
    };

    RunTaskResult {
        issue_number: event.issue_number,
        event_key: event.key,
        run_id,
        started_unix_ms,
        completed_unix_ms,
        duration_ms,
        status,
        posted_comment_id,
        model: config.model,
        usage,
        error,
    }
}

async fn run_prompt_for_event(
    config: &GithubIssuesBridgeRuntimeConfig,
    repo: &RepoRef,
    repository_state_dir: &Path,
    event: &GithubBridgeEvent,
    prompt: &str,
    run_id: &str,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<PromptRunReport> {
    let session_path = session_path_for_issue(repository_state_dir, event.issue_number);
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

    let mut session_runtime = Some(initialize_issue_session_runtime(
        &session_path,
        &config.system_prompt,
        config.session_lock_wait_ms,
        config.session_lock_stale_ms,
        &mut agent,
    )?);

    let formatted_prompt = render_event_prompt(repo, event, prompt);
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
        "Run cancelled by /pi stop.".to_string()
    } else if status == PromptRunStatus::TimedOut {
        "Run timed out before completion.".to_string()
    } else {
        collect_assistant_reply(&agent.messages()[start_index..])
    };
    let usage = usage
        .lock()
        .map_err(|_| anyhow!("prompt usage lock is poisoned"))?
        .clone();
    Ok(PromptRunReport {
        run_id: run_id.to_string(),
        model: config.model.clone(),
        status,
        assistant_reply,
        usage,
    })
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

fn collect_assistant_reply(messages: &[pi_ai::Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == pi_ai::MessageRole::Assistant)
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

fn render_event_prompt(repo: &RepoRef, event: &GithubBridgeEvent, prompt: &str) -> String {
    format!(
        "You are responding as rsBot inside GitHub issues.\nRepository: {}\nIssue: #{} ({})\nAuthor: @{}\nEvent: {}\n\nUser message:\n{}\n\nProvide a direct, actionable response suitable for a GitHub issue comment.",
        repo.as_slug(),
        event.issue_number,
        event.issue_title,
        event.author_login,
        event.kind.as_str(),
        prompt
    )
}

fn render_issue_comment_response(event: &GithubBridgeEvent, run: &PromptRunReport) -> String {
    let mut body = run.assistant_reply.trim().to_string();
    if body.is_empty() {
        body = "I couldn't generate a textual response for this event.".to_string();
    }
    let usage = &run.usage;
    let status = format!("{:?}", run.status).to_lowercase();
    body.push_str("\n\n---\n");
    body.push_str(&format!(
        "{EVENT_KEY_MARKER_PREFIX}{}{EVENT_KEY_MARKER_SUFFIX}\n_rsBot run `{}` | status `{}` | model `{}` | tokens in/out/total `{}/{}/{}` | cost `unavailable`_",
        event.key,
        run.run_id,
        status,
        run.model,
        usage.input_tokens,
        usage.output_tokens,
        usage.total_tokens
    ));
    body
}

fn render_issue_run_error_comment(
    event: &GithubBridgeEvent,
    run_id: &str,
    error: &anyhow::Error,
) -> String {
    format!(
        "rsBot run `{}` failed for event `{}`.\n\nError: `{}`\n\n---\n{EVENT_KEY_MARKER_PREFIX}{}{EVENT_KEY_MARKER_SUFFIX}\n_rsBot run `{}` | status `failed` | model `unavailable` | tokens in/out/total `0/0/0` | cost `unavailable`_",
        run_id,
        event.key,
        truncate_for_error(&error.to_string(), 600),
        event.key,
        run_id
    )
}

fn event_action_from_body(body: &str) -> EventAction {
    match parse_pi_issue_command(body) {
        Some(command) => EventAction::Command(command),
        None => EventAction::RunPrompt {
            prompt: body.trim().to_string(),
        },
    }
}

fn parse_pi_issue_command(body: &str) -> Option<PiIssueCommand> {
    let trimmed = body.trim();
    let mut pieces = trimmed.split_whitespace();
    let command_prefix = pieces.next()?;
    if command_prefix != "/pi" {
        return None;
    }

    let args = trimmed[command_prefix.len()..].trim();
    if args.is_empty() {
        return Some(PiIssueCommand::Invalid {
            message: pi_command_usage(),
        });
    }
    let mut parts = args.splitn(2, char::is_whitespace);
    let command = parts.next().unwrap_or_default();
    let remainder = parts.next().unwrap_or_default().trim();
    let parsed = match command {
        "run" => {
            if remainder.is_empty() {
                PiIssueCommand::Invalid {
                    message: "Usage: /pi run <prompt>".to_string(),
                }
            } else {
                PiIssueCommand::Run {
                    prompt: remainder.to_string(),
                }
            }
        }
        "stop" => {
            if remainder.is_empty() {
                PiIssueCommand::Stop
            } else {
                PiIssueCommand::Invalid {
                    message: "Usage: /pi stop".to_string(),
                }
            }
        }
        "status" => {
            if remainder.is_empty() {
                PiIssueCommand::Status
            } else {
                PiIssueCommand::Invalid {
                    message: "Usage: /pi status".to_string(),
                }
            }
        }
        "compact" => {
            if remainder.is_empty() {
                PiIssueCommand::Compact
            } else {
                PiIssueCommand::Invalid {
                    message: "Usage: /pi compact".to_string(),
                }
            }
        }
        "summarize" => {
            let focus = (!remainder.is_empty()).then(|| remainder.to_string());
            PiIssueCommand::Summarize { focus }
        }
        _ => PiIssueCommand::Invalid {
            message: format!("Unknown command `{}`.\n\n{}", command, pi_command_usage()),
        },
    };
    Some(parsed)
}

fn pi_command_usage() -> String {
    [
        "Supported `/pi` commands:",
        "- `/pi run <prompt>`",
        "- `/pi stop`",
        "- `/pi status`",
        "- `/pi compact`",
        "- `/pi summarize [focus]`",
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
    while let Some(start) = cursor.find(EVENT_KEY_MARKER_PREFIX) {
        let after_start = &cursor[start + EVENT_KEY_MARKER_PREFIX.len()..];
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

fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let raw = headers.get("retry-after")?.to_str().ok()?;
    let seconds = raw.trim().parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

fn retry_delay(base_delay_ms: u64, attempt: usize, retry_after: Option<Duration>) -> Duration {
    if let Some(delay) = retry_after {
        return delay.max(Duration::from_millis(base_delay_ms));
    }
    let exponent = attempt.saturating_sub(1).min(10) as u32;
    let scaled = base_delay_ms.saturating_mul(2_u64.saturating_pow(exponent));
    Duration::from_millis(scaled.min(30_000))
}

fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

fn is_retryable_github_status(status: u16) -> bool {
    status == 429 || status >= 500
}

fn truncate_for_error(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated = text.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
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
    use std::{path::Path, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use httpmock::prelude::*;
    use pi_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, PiAiError};
    use serde_json::json;
    use tempfile::tempdir;
    use tokio::time::sleep;

    use super::{
        collect_issue_events, event_action_from_body, extract_footer_event_keys,
        is_retryable_github_status, parse_pi_issue_command, retry_delay, sanitize_for_path,
        session_path_for_issue, EventAction, GithubApiClient, GithubBridgeEventKind, GithubIssue,
        GithubIssueComment, GithubIssuesBridgeRuntime, GithubIssuesBridgeRuntimeConfig,
        GithubIssuesBridgeStateStore, GithubUser, PiIssueCommand, RepoRef,
    };
    use crate::{tools::ToolPolicy, RenderOptions};

    struct StaticReplyClient;

    #[async_trait]
    impl LlmClient for StaticReplyClient {
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
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
        async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, PiAiError> {
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
            repo_slug: "owner/repo".to_string(),
            api_base: base_url.to_string(),
            token: "test-token".to_string(),
            bot_login: Some("rsbot".to_string()),
            poll_interval: Duration::from_millis(1),
            include_issue_body: false,
            include_edited_comments: true,
            processed_event_cap: 32,
            retry_max_attempts: 3,
            retry_base_delay_ms: 5,
        }
    }

    #[test]
    fn unit_repo_ref_parse_accepts_owner_repo_shape() {
        let repo = RepoRef::parse("njfio/rsBot").expect("parse repo");
        assert_eq!(repo.owner, "njfio");
        assert_eq!(repo.name, "rsBot");

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
        let events = collect_issue_events(&issue, &comments, "rsbot", true, true);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, GithubBridgeEventKind::Opened);
        assert_eq!(events[1].kind, GithubBridgeEventKind::CommentCreated);
        assert_eq!(events[2].kind, GithubBridgeEventKind::CommentEdited);
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
    fn unit_retry_helpers_identify_retryable_status_and_delays() {
        assert!(is_retryable_github_status(429));
        assert!(is_retryable_github_status(500));
        assert!(!is_retryable_github_status(404));
        let delay = retry_delay(100, 3, None);
        assert_eq!(delay, Duration::from_millis(400));
    }

    #[test]
    fn unit_footer_key_extraction_and_path_helpers_are_stable() {
        let text = "hello\n<!-- rsbot-event-key:abc -->\nworld\n<!-- rsbot-event-key:def -->";
        let keys = extract_footer_event_keys(text);
        assert_eq!(keys, vec!["abc".to_string(), "def".to_string()]);

        let root = Path::new("/tmp/state");
        let session = session_path_for_issue(root, 9);
        assert!(session.ends_with("sessions/issue-9.jsonl"));
        assert_eq!(sanitize_for_path("owner/repo"), "owner_repo");
    }

    #[test]
    fn unit_parse_pi_issue_command_supports_known_commands() {
        assert_eq!(
            parse_pi_issue_command("/pi run investigate failures"),
            Some(PiIssueCommand::Run {
                prompt: "investigate failures".to_string()
            })
        );
        assert_eq!(
            parse_pi_issue_command("/pi status"),
            Some(PiIssueCommand::Status)
        );
        assert_eq!(
            parse_pi_issue_command("/pi stop"),
            Some(PiIssueCommand::Stop)
        );
        assert_eq!(
            parse_pi_issue_command("/pi summarize release blockers"),
            Some(PiIssueCommand::Summarize {
                focus: Some("release blockers".to_string())
            })
        );
        assert_eq!(parse_pi_issue_command("plain message"), None);
    }

    #[test]
    fn regression_parse_pi_issue_command_rejects_slash_like_inputs() {
        assert_eq!(parse_pi_issue_command("/pii run nope"), None);
        let parsed = parse_pi_issue_command("/pi run").expect("command parse");
        assert!(matches!(parsed, PiIssueCommand::Invalid { .. }));
        let action = event_action_from_body("/pi unknown");
        assert!(matches!(
            action,
            EventAction::Command(PiIssueCommand::Invalid { .. })
        ));
    }

    #[tokio::test]
    async fn integration_github_api_client_retries_rate_limits() {
        let server = MockServer::start();
        let first = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues")
                .header("x-rsbot-retry-attempt", "0");
            then.status(429)
                .header("retry-after", "0")
                .body("rate limit");
        });
        let second = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues")
                .header("x-rsbot-retry-attempt", "1");
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
                .body_includes("rsBot is working on run");
            then.status(201).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/901")
                .body_includes("bridge reply")
                .body_includes("rsbot-event-key:issue-comment-created:200");
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
                .body_includes("rsBot is working on run");
            then.status(201).json_body(json!({
                "id": 902,
                "html_url": "https://example.test/comment/902"
            }));
        });
        let update = server.mock(|when, then| {
            when.method(PATCH)
                .path("/repos/owner/repo/issues/comments/902")
                .body_includes("rsbot-event-key:issue-comment-created:201");
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
    async fn integration_bridge_commands_status_and_stop_produce_control_comments() {
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
                    "body": "/pi status",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 302,
                    "body": "/pi stop",
                    "created_at": "2026-01-01T00:00:02Z",
                    "updated_at": "2026-01-01T00:00:02Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("rsBot status for issue #9: idle");
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

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 2);
        assert_eq!(report.failed_events, 0);
        status_post.assert_calls(1);
        stop_post.assert_calls(1);
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
                    "body": "/pi run long diagnostic run",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 402,
                    "body": "/pi stop",
                    "created_at": "2026-01-01T00:00:02Z",
                    "updated_at": "2026-01-01T00:00:02Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let working_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/10/comments")
                .body_includes("rsBot is working on run");
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
                .body_includes("Run cancelled by /pi stop.");
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
