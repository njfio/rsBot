use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
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

use crate::auth_commands::execute_auth_command;
use crate::channel_store::{
    ChannelArtifactRecord, ChannelAttachmentRecord, ChannelLogEntry, ChannelStore,
};
use crate::runtime_types::{AuthCommandConfig, DoctorCommandConfig};
use crate::tools::ToolPolicy;
use crate::{
    authorize_action_for_principal_with_policy_path, current_unix_timestamp_ms,
    evaluate_pairing_access, execute_canvas_command, github_principal,
    pairing_policy_for_state_dir, rbac_policy_path_for_state_dir, run_prompt_with_cancellation,
    session_message_preview, session_message_role, write_text_atomic, CanvasCommandConfig,
    CanvasEventOrigin, CanvasSessionLinkContext, PairingDecision, PromptRunStatus, RbacDecision,
    RenderOptions, TransportHealthSnapshot,
};
use tau_diagnostics::{
    render_doctor_report, render_doctor_report_json, run_doctor_checks_with_options,
    DoctorCheckOptions, DoctorStatus,
};
use tau_github_issues::github_issues_helpers::{
    attachment_filename_from_url, evaluate_attachment_content_type_policy,
    evaluate_attachment_url_policy, extract_attachment_urls, split_at_char_index,
};
use tau_github_issues::github_transport_helpers::{
    is_retryable_github_status, is_retryable_transport_error, parse_retry_after, retry_delay,
    truncate_for_error,
};
use tau_github_issues::issue_auth_command::{TauIssueAuthCommand, TauIssueAuthCommandKind};
use tau_github_issues::issue_auth_helpers::{
    build_issue_auth_summary_line as build_shared_issue_auth_summary_line,
    ensure_auth_json_flag as ensure_shared_auth_json_flag, IssueAuthSummaryKind,
};
use tau_github_issues::issue_command_usage::tau_command_usage as tau_shared_command_usage;
use tau_github_issues::issue_comment::{
    extract_footer_event_keys, issue_command_reason_code, normalize_issue_command_status,
    render_issue_command_comment,
};
use tau_github_issues::issue_doctor_command::IssueDoctorCommand;
use tau_github_issues::issue_event_action::{
    event_action_from_body as event_action_from_shared_body, EventAction as SharedEventAction,
};
use tau_github_issues::issue_event_collection::{
    collect_issue_events as collect_shared_issue_events, GithubBridgeEvent, GithubIssue,
    GithubIssueComment,
};
#[cfg(test)]
use tau_github_issues::issue_event_collection::{
    GithubBridgeEventKind, GithubIssueLabel, GithubUser,
};
use tau_github_issues::issue_filter::{
    build_required_issue_labels, issue_matches_required_labels, issue_matches_required_numbers,
};
use tau_github_issues::issue_prompt_helpers::{
    build_summarize_prompt as build_shared_summarize_prompt,
    collect_assistant_reply as collect_shared_assistant_reply,
};
use tau_github_issues::issue_run_error_comment::render_issue_run_error_comment as render_shared_issue_run_error_comment;
use tau_github_issues::issue_runtime_helpers::{
    is_expired_at as is_shared_expired_at, issue_session_id as issue_shared_session_id,
    normalize_artifact_retention_days as normalize_shared_artifact_retention_days,
    normalize_relative_channel_path as normalize_shared_relative_channel_path,
    parse_rfc3339_to_unix_ms as parse_shared_rfc3339_to_unix_ms,
    render_issue_artifact_pointer_line as render_shared_issue_artifact_pointer_line,
    sanitize_for_path as shared_sanitize_for_path,
    session_path_for_issue as shared_session_path_for_issue, sha256_hex as shared_sha256_hex,
    short_key_hash as shared_short_key_hash,
};
use tau_github_issues::issue_session_helpers::{
    compact_issue_session as shared_compact_issue_session,
    ensure_issue_session_initialized as shared_ensure_issue_session_initialized,
    reset_issue_session_files as shared_reset_issue_session_files,
};
use tau_session::search_session_entries;
use tau_session::SessionStore;

mod issue_command_helpers;
mod issue_render_helpers;
mod issue_session_runtime;

use issue_command_helpers::{
    default_demo_index_binary_path, default_demo_index_repo_root, parse_tau_issue_command,
};
use issue_render_helpers::{
    doctor_status_label, prompt_status_label, render_event_prompt, render_issue_artifact_markdown,
    render_issue_comment_chunks,
};
#[cfg(test)]
use issue_render_helpers::{
    render_issue_comment_chunks_with_limit, render_issue_comment_response_parts,
};
use issue_session_runtime::initialize_issue_session_runtime;

const GITHUB_STATE_SCHEMA_VERSION: u32 = 1;
const GITHUB_COMMENT_MAX_CHARS: usize = 65_000;
const EVENT_KEY_MARKER_PREFIX: &str = tau_github_issues::issue_comment::EVENT_KEY_MARKER_PREFIX;
const EVENT_KEY_MARKER_SUFFIX: &str = tau_github_issues::issue_comment::EVENT_KEY_MARKER_SUFFIX;
const CHAT_SHOW_DEFAULT_LIMIT: usize = 10;
const CHAT_SHOW_MAX_LIMIT: usize = 50;
const CHAT_SEARCH_MAX_LIMIT: usize = 50;
const GITHUB_ATTACHMENT_MAX_BYTES: usize = 10 * 1024 * 1024;
const DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS: u64 = 180;
const DEMO_INDEX_MAX_TIMEOUT_SECONDS: u64 = 900;
const DEMO_INDEX_SCENARIOS: [&str; 4] = [
    "onboarding",
    "gateway-auth",
    "multi-channel-live",
    "deployment-wasm",
];

#[derive(Clone)]
pub struct GithubIssuesBridgeRuntimeConfig {
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
    pub poll_once: bool,
    pub required_labels: Vec<String>,
    pub required_issue_numbers: Vec<u64>,
    pub include_issue_body: bool,
    pub include_edited_comments: bool,
    pub processed_event_cap: usize,
    pub retry_max_attempts: usize,
    pub retry_base_delay_ms: u64,
    pub artifact_retention_days: u64,
    pub auth_command_config: AuthCommandConfig,
    pub demo_index_repo_root: Option<PathBuf>,
    pub demo_index_script_path: Option<PathBuf>,
    pub demo_index_binary_path: Option<PathBuf>,
    pub doctor_config: DoctorCommandConfig,
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
    #[serde(default)]
    active_run_id: Option<String>,
    #[serde(default)]
    last_event_key: Option<String>,
    #[serde(default)]
    last_event_kind: Option<String>,
    #[serde(default)]
    last_actor_login: Option<String>,
    #[serde(default)]
    last_reason_code: Option<String>,
    #[serde(default)]
    last_processed_unix_ms: Option<u64>,
    #[serde(default)]
    total_processed_events: u64,
    #[serde(default)]
    total_duplicate_events: u64,
    #[serde(default)]
    total_failed_events: u64,
    #[serde(default)]
    total_denied_events: u64,
    #[serde(default)]
    total_runs_started: u64,
    #[serde(default)]
    total_runs_completed: u64,
    #[serde(default)]
    total_runs_failed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IssueEventOutcome {
    Processed,
    Denied,
    Failed,
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

    fn processed_event_tail(&self, limit: usize) -> Vec<String> {
        if limit == 0 || self.state.processed_event_keys.is_empty() {
            return Vec::new();
        }
        let total = self.state.processed_event_keys.len();
        let start = total.saturating_sub(limit);
        self.state.processed_event_keys[start..].to_vec()
    }

    fn processed_event_cap(&self) -> usize {
        self.cap
    }

    fn issue_session(&self, issue_number: u64) -> Option<&GithubIssueChatSessionState> {
        self.state.issue_sessions.get(&issue_number.to_string())
    }

    fn issue_session_mut(&mut self, issue_number: u64) -> &mut GithubIssueChatSessionState {
        self.state
            .issue_sessions
            .entry(issue_number.to_string())
            .or_insert_with(|| GithubIssueChatSessionState {
                session_id: issue_shared_session_id(issue_number),
                last_comment_id: None,
                last_run_id: None,
                active_run_id: None,
                last_event_key: None,
                last_event_kind: None,
                last_actor_login: None,
                last_reason_code: None,
                last_processed_unix_ms: None,
                total_processed_events: 0,
                total_duplicate_events: 0,
                total_failed_events: 0,
                total_denied_events: 0,
                total_runs_started: 0,
                total_runs_completed: 0,
                total_runs_failed: 0,
            })
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
                    active_run_id: None,
                    last_event_key: None,
                    last_event_kind: None,
                    last_actor_login: None,
                    last_reason_code: None,
                    last_processed_unix_ms: None,
                    total_processed_events: 0,
                    total_duplicate_events: 0,
                    total_failed_events: 0,
                    total_denied_events: 0,
                    total_runs_started: 0,
                    total_runs_completed: 0,
                    total_runs_failed: 0,
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

    fn record_issue_duplicate_event(
        &mut self,
        issue_number: u64,
        event_key: &str,
        event_kind: &str,
        actor_login: &str,
    ) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.last_event_key.as_deref() != Some(event_key) {
            entry.last_event_key = Some(event_key.to_string());
        }
        if entry.last_event_kind.as_deref() != Some(event_kind) {
            entry.last_event_kind = Some(event_kind.to_string());
        }
        if entry.last_actor_login.as_deref() != Some(actor_login) {
            entry.last_actor_login = Some(actor_login.to_string());
        }
        if entry.last_reason_code.as_deref() != Some("duplicate_event") {
            entry.last_reason_code = Some("duplicate_event".to_string());
        }
        let processed_unix_ms = current_unix_timestamp_ms();
        if entry.last_processed_unix_ms != Some(processed_unix_ms) {
            entry.last_processed_unix_ms = Some(processed_unix_ms);
        }
        entry.total_duplicate_events = entry.total_duplicate_events.saturating_add(1);
        true
    }

    fn record_issue_event_outcome(
        &mut self,
        issue_number: u64,
        event_key: &str,
        event_kind: &str,
        actor_login: &str,
        outcome: IssueEventOutcome,
        reason_code: Option<&str>,
    ) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.last_event_key.as_deref() != Some(event_key) {
            entry.last_event_key = Some(event_key.to_string());
        }
        if entry.last_event_kind.as_deref() != Some(event_kind) {
            entry.last_event_kind = Some(event_kind.to_string());
        }
        if entry.last_actor_login.as_deref() != Some(actor_login) {
            entry.last_actor_login = Some(actor_login.to_string());
        }
        if let Some(reason_code) = reason_code {
            if entry.last_reason_code.as_deref() != Some(reason_code) {
                entry.last_reason_code = Some(reason_code.to_string());
            }
        }
        let processed_unix_ms = current_unix_timestamp_ms();
        if entry.last_processed_unix_ms != Some(processed_unix_ms) {
            entry.last_processed_unix_ms = Some(processed_unix_ms);
        }
        entry.total_processed_events = entry.total_processed_events.saturating_add(1);
        match outcome {
            IssueEventOutcome::Processed => {}
            IssueEventOutcome::Denied => {
                entry.total_denied_events = entry.total_denied_events.saturating_add(1);
            }
            IssueEventOutcome::Failed => {
                entry.total_failed_events = entry.total_failed_events.saturating_add(1);
            }
        }
        true
    }

    fn record_issue_run_started(&mut self, issue_number: u64, run_id: &str) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.active_run_id.as_deref() != Some(run_id) {
            entry.active_run_id = Some(run_id.to_string());
        }
        if entry.last_run_id.as_deref() != Some(run_id) {
            entry.last_run_id = Some(run_id.to_string());
        }
        entry.total_runs_started = entry.total_runs_started.saturating_add(1);
        true
    }

    fn record_issue_run_finished(&mut self, issue_number: u64, run_id: &str, failed: bool) -> bool {
        let entry = self.issue_session_mut(issue_number);
        if entry.last_run_id.as_deref() != Some(run_id) {
            entry.last_run_id = Some(run_id.to_string());
        }
        if entry.active_run_id.is_some() {
            entry.active_run_id = None;
        }
        entry.total_runs_completed = entry.total_runs_completed.saturating_add(1);
        if failed {
            entry.total_runs_failed = entry.total_runs_failed.saturating_add(1);
        }
        true
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
            let api_base = self.api_base.clone();
            let owner = self.repo.owner.clone();
            let repo = self.repo.name.clone();
            let page_value = page.to_string();
            let since_value = since.map(ToOwned::to_owned);
            let chunk: Vec<GithubIssue> = self
                .request_json("list issues", || {
                    let mut request = self
                        .http
                        .get(format!("{}/repos/{owner}/{repo}/issues", api_base));
                    request = request.query(&[
                        ("state", "open"),
                        ("sort", "updated"),
                        ("direction", "asc"),
                        ("per_page", "100"),
                        ("page", page_value.as_str()),
                    ]);
                    if let Some(since_value) = since_value.as_deref() {
                        request = request.query(&[("since", since_value)]);
                    }
                    request
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
            let api_base = self.api_base.clone();
            let owner = self.repo.owner.clone();
            let repo = self.repo.name.clone();
            let page_value = page.to_string();
            let chunk: Vec<GithubIssueComment> = self
                .request_json("list issue comments", || {
                    self.http
                        .get(format!(
                            "{}/repos/{}/{}/issues/{}/comments",
                            api_base, owner, repo, issue_number
                        ))
                        .query(&[
                            ("sort", "created"),
                            ("direction", "asc"),
                            ("per_page", "100"),
                            ("page", page_value.as_str()),
                        ])
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
struct DemoIndexRunCommand {
    scenarios: Vec<String>,
    timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoIndexScenarioDescriptor {
    id: String,
    wrapper: String,
    command: String,
    description: String,
    #[serde(default)]
    expected_markers: Vec<String>,
    troubleshooting: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoIndexScenarioInventory {
    #[serde(default)]
    scenarios: Vec<DemoIndexScenarioDescriptor>,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoIndexScenarioResult {
    id: String,
    status: String,
    exit_code: i32,
    duration_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoIndexRunSummary {
    total: u64,
    passed: u64,
    failed: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DemoIndexRunReport {
    #[serde(default)]
    scenarios: Vec<DemoIndexScenarioResult>,
    summary: DemoIndexRunSummary,
}

#[derive(Debug, Clone)]
struct DemoIndexRunExecution {
    run_id: String,
    command_line: String,
    exit_code: i32,
    summary: Option<DemoIndexRunReport>,
    report_artifact: ChannelArtifactRecord,
    log_artifact: ChannelArtifactRecord,
}

#[derive(Debug, Clone)]
struct IssueAuthExecution {
    run_id: String,
    command_name: &'static str,
    summary_line: String,
    subscription_strict: bool,
    report_artifact: ChannelArtifactRecord,
    json_artifact: ChannelArtifactRecord,
}

#[derive(Debug, Clone)]
struct IssueDoctorExecution {
    run_id: String,
    checks: usize,
    pass: usize,
    warn: usize,
    fail: usize,
    highlighted: Vec<String>,
    report_artifact: ChannelArtifactRecord,
    json_artifact: ChannelArtifactRecord,
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
    ChatSummary,
    ChatReplay,
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
    DemoIndexList,
    DemoIndexRun {
        command: DemoIndexRunCommand,
    },
    DemoIndexReport,
    Auth {
        command: TauIssueAuthCommand,
    },
    Doctor {
        command: IssueDoctorCommand,
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

type EventAction = SharedEventAction<TauIssueCommand>;

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

pub async fn run_github_issues_bridge(config: GithubIssuesBridgeRuntimeConfig) -> Result<()> {
    let mut runtime = GithubIssuesBridgeRuntime::new(config).await?;
    runtime.run().await
}

struct GithubIssuesBridgeRuntime {
    config: GithubIssuesBridgeRuntimeConfig,
    repo: RepoRef,
    github_client: GithubApiClient,
    required_issue_labels: HashSet<String>,
    required_issue_numbers: HashSet<u64>,
    state_store: GithubIssuesBridgeStateStore,
    inbound_log: JsonlEventLog,
    outbound_log: JsonlEventLog,
    bot_login: String,
    repository_state_dir: PathBuf,
    demo_index_repo_root: PathBuf,
    demo_index_script_path: PathBuf,
    demo_index_binary_path: PathBuf,
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
        let repository_state_dir = config.state_dir.join(shared_sanitize_for_path(&format!(
            "{}__{}",
            repo.owner, repo.name
        )));
        std::fs::create_dir_all(&repository_state_dir)
            .with_context(|| format!("failed to create {}", repository_state_dir.display()))?;

        let state_store = GithubIssuesBridgeStateStore::load(
            repository_state_dir.join("state.json"),
            config.processed_event_cap,
        )?;
        let inbound_log = JsonlEventLog::open(repository_state_dir.join("inbound-events.jsonl"))?;
        let outbound_log = JsonlEventLog::open(repository_state_dir.join("outbound-events.jsonl"))?;
        let demo_index_repo_root = config
            .demo_index_repo_root
            .clone()
            .unwrap_or_else(default_demo_index_repo_root);
        let demo_index_script_path = config
            .demo_index_script_path
            .clone()
            .unwrap_or_else(|| demo_index_repo_root.join("scripts/demo/index.sh"));
        let demo_index_binary_path = config
            .demo_index_binary_path
            .clone()
            .unwrap_or_else(default_demo_index_binary_path);
        let required_issue_labels =
            build_required_issue_labels(config.required_labels.iter().map(|label| label.as_str()));
        let required_issue_numbers = config
            .required_issue_numbers
            .iter()
            .copied()
            .filter(|issue_number| *issue_number > 0)
            .collect::<HashSet<_>>();
        Ok(Self {
            config,
            repo,
            github_client,
            required_issue_labels,
            required_issue_numbers,
            state_store,
            inbound_log,
            outbound_log,
            bot_login,
            repository_state_dir,
            demo_index_repo_root,
            demo_index_script_path,
            demo_index_binary_path,
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
                    if self.config.poll_once {
                        let mut finalize_report = PollCycleReport::default();
                        let mut state_dirty = false;
                        self.drain_finished_runs(&mut finalize_report, &mut state_dirty, true)
                            .await?;
                        if state_dirty {
                            self.state_store.save()?;
                        }
                        println!(
                            "github bridge one-shot complete: repo={} failed_runs={}",
                            self.repo.as_slug(),
                            finalize_report.failed_events
                        );
                        return Ok(());
                    }
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
                    if self.config.poll_once {
                        return Err(error);
                    }
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
        self.drain_finished_runs(&mut report, &mut state_dirty, false)
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
            if !issue_matches_required_numbers(issue.number, &self.required_issue_numbers) {
                continue;
            }
            if !issue_matches_required_labels(
                issue.labels.iter().map(|label| label.name.as_str()),
                &self.required_issue_labels,
            ) {
                continue;
            }

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

            let events = collect_shared_issue_events(
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
                    if self.state_store.record_issue_duplicate_event(
                        event.issue_number,
                        &event.key,
                        event.kind.as_str(),
                        &event.author_login,
                    ) {
                        state_dirty = true;
                    }
                    continue;
                }

                let action = event_action_from_shared_body(&event.body, parse_tau_issue_command);
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
                    if self.state_store.record_issue_event_outcome(
                        event.issue_number,
                        &event.key,
                        event.kind.as_str(),
                        &event.author_login,
                        IssueEventOutcome::Denied,
                        Some(reason_code.as_str()),
                    ) {
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
                        if self.state_store.record_issue_event_outcome(
                            event.issue_number,
                            &event.key,
                            event.kind.as_str(),
                            &event.author_login,
                            IssueEventOutcome::Denied,
                            Some(reason_code.as_str()),
                        ) {
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
                        if self.state_store.record_issue_event_outcome(
                            event.issue_number,
                            &event.key,
                            event.kind.as_str(),
                            &event.author_login,
                            IssueEventOutcome::Failed,
                            Some("rbac_policy_error"),
                        ) {
                            state_dirty = true;
                        }
                        report.failed_events = report.failed_events.saturating_add(1);
                        continue;
                    }
                }

                let suppress_processed_outcome =
                    matches!(&action, EventAction::Command(TauIssueCommand::ChatReset));
                if let Err(error) = self
                    .handle_event_action(&event, action, &mut report, &mut state_dirty)
                    .await
                {
                    if self.state_store.record_issue_event_outcome(
                        event.issue_number,
                        &event.key,
                        event.kind.as_str(),
                        &event.author_login,
                        IssueEventOutcome::Failed,
                        Some("event_action_failed"),
                    ) {
                        state_dirty = true;
                    }
                    report.failed_events = report.failed_events.saturating_add(1);
                    eprintln!(
                        "github bridge event failed: repo={} issue=#{} key={} error={error}",
                        self.repo.as_slug(),
                        event.issue_number,
                        event.key
                    );
                } else if !suppress_processed_outcome
                    && self.state_store.record_issue_event_outcome(
                        event.issue_number,
                        &event.key,
                        event.kind.as_str(),
                        &event.author_login,
                        IssueEventOutcome::Processed,
                        Some("event_processed"),
                    )
                {
                    state_dirty = true;
                }
            }
        }

        self.drain_finished_runs(&mut report, &mut state_dirty, false)
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
        include_pending: bool,
    ) -> Result<()> {
        let finished_issues = self
            .active_runs
            .iter()
            .filter_map(|(issue_number, run)| {
                if include_pending || run.handle.is_finished() {
                    Some(*issue_number)
                } else {
                    None
                }
            })
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
                        issue_shared_session_id(result.issue_number),
                        result.posted_comment_id,
                        Some(result.run_id.clone()),
                    ) {
                        *state_dirty = true;
                    }
                    if self.state_store.record_issue_run_finished(
                        result.issue_number,
                        &result.run_id,
                        result.error.is_some(),
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
                    if self.state_store.record_issue_run_finished(
                        issue_number,
                        &active.run_id,
                        true,
                    ) {
                        *state_dirty = true;
                    }
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
            shared_short_key_hash(&event.key)
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
            issue_shared_session_id(event.issue_number),
            Some(working_comment_id),
            Some(run_id.clone()),
        ) {
            *state_dirty = true;
        }
        if self
            .state_store
            .record_issue_run_started(event.issue_number, &run_id)
        {
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
                let prompt = build_shared_summarize_prompt(
                    &self.repo.as_slug(),
                    event.issue_number,
                    focus.as_deref(),
                );
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
            TauIssueCommand::DemoIndexList => {
                let (status, message, error_text) =
                    match self.render_demo_index_inventory(event.issue_number).await {
                        Ok(message) => ("reported", message, None),
                        Err(error) => (
                            "failed",
                            format!(
                                "Tau demo-index list failed for issue #{}.\n\nError: {}",
                                event.issue_number,
                                truncate_for_error(&error.to_string(), 280)
                            ),
                            Some(error.to_string()),
                        ),
                    };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "demo-index-list",
                        status,
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "demo-index-list",
                    "status": status,
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "error": error_text,
                }))?;
            }
            TauIssueCommand::DemoIndexRun { command } => {
                match self
                    .execute_demo_index_run(event.issue_number, &event.key, &command)
                    .await
                {
                    Ok(execution) => {
                        let run_status = if execution.exit_code == 0
                            && execution
                                .summary
                                .as_ref()
                                .map(|summary| summary.summary.failed == 0)
                                .unwrap_or(true)
                        {
                            "completed"
                        } else {
                            "failed"
                        };
                        let mut lines = vec![format!(
                            "Tau demo-index run for issue #{}: status={} run_id={}",
                            event.issue_number, run_status, execution.run_id
                        )];
                        lines.push(format!("scenarios: {}", command.scenarios.join(",")));
                        lines.push(format!("timeout_seconds: {}", command.timeout_seconds));
                        lines.push(format!("exit_code: {}", execution.exit_code));
                        if let Some(summary) = &execution.summary {
                            lines.push(format!(
                                "summary: total={} passed={} failed={}",
                                summary.summary.total,
                                summary.summary.passed,
                                summary.summary.failed
                            ));
                            for scenario in &summary.scenarios {
                                lines.push(format!(
                                    "- {} status={} exit_code={} duration_ms={}",
                                    scenario.id,
                                    scenario.status,
                                    scenario.exit_code,
                                    scenario.duration_ms
                                ));
                            }
                        } else {
                            lines.push(
                                "summary: unavailable (demo-index JSON payload was malformed)"
                                    .to_string(),
                            );
                        }
                        lines.push(render_shared_issue_artifact_pointer_line(
                            "report_artifact",
                            &execution.report_artifact.id,
                            &execution.report_artifact.relative_path,
                            execution.report_artifact.bytes,
                        ));
                        lines.push(render_shared_issue_artifact_pointer_line(
                            "log_artifact",
                            &execution.log_artifact.id,
                            &execution.log_artifact.relative_path,
                            execution.log_artifact.bytes,
                        ));
                        lines.push(
                            "Use `/tau demo-index report` to inspect latest report pointers."
                                .to_string(),
                        );
                        let message = lines.join("\n");
                        let posted = self
                            .post_issue_command_comment(
                                event.issue_number,
                                &event.key,
                                "demo-index-run",
                                run_status,
                                &message,
                            )
                            .await?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key,
                            "issue_number": event.issue_number,
                            "command": "demo-index-run",
                            "status": run_status,
                            "posted_comment_id": posted.id,
                            "posted_comment_url": posted.html_url,
                            "run_id": execution.run_id,
                            "command_line": execution.command_line,
                            "scenarios": command.scenarios,
                            "timeout_seconds": command.timeout_seconds,
                            "exit_code": execution.exit_code,
                            "summary": execution.summary.as_ref().map(|summary| json!({
                                "total": summary.summary.total,
                                "passed": summary.summary.passed,
                                "failed": summary.summary.failed,
                                "scenarios": summary.scenarios.iter().map(|scenario| json!({
                                    "id": scenario.id,
                                    "status": scenario.status,
                                    "exit_code": scenario.exit_code,
                                    "duration_ms": scenario.duration_ms,
                                })).collect::<Vec<_>>(),
                            })),
                            "report_artifact": {
                                "id": execution.report_artifact.id,
                                "path": execution.report_artifact.relative_path,
                                "bytes": execution.report_artifact.bytes,
                                "checksum_sha256": execution.report_artifact.checksum_sha256,
                            },
                            "log_artifact": {
                                "id": execution.log_artifact.id,
                                "path": execution.log_artifact.relative_path,
                                "bytes": execution.log_artifact.bytes,
                                "checksum_sha256": execution.log_artifact.checksum_sha256,
                            },
                        }))?;
                    }
                    Err(error) => {
                        let message = format!(
                            "Tau demo-index run failed for issue #{}.\n\nError: {}",
                            event.issue_number,
                            truncate_for_error(&error.to_string(), 280)
                        );
                        let posted = self
                            .post_issue_command_comment(
                                event.issue_number,
                                &event.key,
                                "demo-index-run",
                                "failed",
                                &message,
                            )
                            .await?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key,
                            "issue_number": event.issue_number,
                            "command": "demo-index-run",
                            "status": "failed",
                            "posted_comment_id": posted.id,
                            "posted_comment_url": posted.html_url,
                            "scenarios": command.scenarios,
                            "timeout_seconds": command.timeout_seconds,
                            "error": error.to_string(),
                        }))?;
                    }
                }
            }
            TauIssueCommand::DemoIndexReport => {
                let (status, message, error_text) =
                    match self.render_issue_demo_index_reports(event.issue_number) {
                        Ok(message) => ("reported", message, None),
                        Err(error) => (
                            "failed",
                            format!(
                                "Tau demo-index report lookup failed for issue #{}.\n\nError: {}",
                                event.issue_number,
                                truncate_for_error(&error.to_string(), 280)
                            ),
                            Some(error.to_string()),
                        ),
                    };
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "demo-index-report",
                        status,
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "demo-index-report",
                    "status": status,
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "error": error_text,
                }))?;
            }
            TauIssueCommand::Auth { command } => {
                match self.execute_issue_auth_command(event.issue_number, &event.key, &command) {
                    Ok(execution) => {
                        let posture = self.render_issue_auth_posture_lines();
                        let mut lines = vec![format!(
                            "Tau auth diagnostics for issue #{}: command={} run_id={}",
                            event.issue_number, execution.command_name, execution.run_id
                        )];
                        lines.push(execution.summary_line.clone());
                        lines.push(format!(
                            "subscription_strict: {}",
                            execution.subscription_strict
                        ));
                        lines.extend(posture);
                        lines.push(render_shared_issue_artifact_pointer_line(
                            "report_artifact",
                            &execution.report_artifact.id,
                            &execution.report_artifact.relative_path,
                            execution.report_artifact.bytes,
                        ));
                        lines.push(render_shared_issue_artifact_pointer_line(
                            "json_artifact",
                            &execution.json_artifact.id,
                            &execution.json_artifact.relative_path,
                            execution.json_artifact.bytes,
                        ));
                        lines.push(
                            "Use `/tau artifacts show <artifact_id>` to inspect full diagnostics."
                                .to_string(),
                        );
                        let message = lines.join("\n");
                        let posted = self
                            .post_issue_command_comment(
                                event.issue_number,
                                &event.key,
                                if matches!(command.kind, TauIssueAuthCommandKind::Status) {
                                    "auth-status"
                                } else {
                                    "auth-matrix"
                                },
                                "reported",
                                &message,
                            )
                            .await?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key,
                            "issue_number": event.issue_number,
                            "command": execution.command_name,
                            "status": "reported",
                            "posted_comment_id": posted.id,
                            "posted_comment_url": posted.html_url,
                            "run_id": execution.run_id,
                            "subscription_strict": execution.subscription_strict,
                            "summary": execution.summary_line,
                            "report_artifact": {
                                "id": execution.report_artifact.id,
                                "path": execution.report_artifact.relative_path,
                                "bytes": execution.report_artifact.bytes,
                                "checksum_sha256": execution.report_artifact.checksum_sha256,
                            },
                            "json_artifact": {
                                "id": execution.json_artifact.id,
                                "path": execution.json_artifact.relative_path,
                                "bytes": execution.json_artifact.bytes,
                                "checksum_sha256": execution.json_artifact.checksum_sha256,
                            },
                        }))?;
                    }
                    Err(error) => {
                        let message = format!(
                            "Tau auth diagnostics failed for issue #{}.\n\nError: {}",
                            event.issue_number,
                            truncate_for_error(&error.to_string(), 280)
                        );
                        let posted = self
                            .post_issue_command_comment(
                                event.issue_number,
                                &event.key,
                                "auth",
                                "failed",
                                &message,
                            )
                            .await?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key,
                            "issue_number": event.issue_number,
                            "command": "auth",
                            "status": "failed",
                            "posted_comment_id": posted.id,
                            "posted_comment_url": posted.html_url,
                            "error": error.to_string(),
                        }))?;
                    }
                }
            }
            TauIssueCommand::Doctor { command } => {
                match self.execute_issue_doctor_command(event.issue_number, &event.key, command) {
                    Ok(execution) => {
                        let status = if execution.fail > 0 {
                            "degraded"
                        } else if execution.warn > 0 {
                            "warning"
                        } else {
                            "healthy"
                        };
                        let mut lines = vec![format!(
                            "Tau doctor diagnostics for issue #{}: status={} run_id={}",
                            event.issue_number, status, execution.run_id
                        )];
                        lines.push(format!(
                            "summary: checks={} pass={} warn={} fail={} online={}",
                            execution.checks,
                            execution.pass,
                            execution.warn,
                            execution.fail,
                            command.online
                        ));
                        if !execution.highlighted.is_empty() {
                            lines.push("highlights:".to_string());
                            for highlighted in &execution.highlighted {
                                lines.push(format!("- {highlighted}"));
                            }
                        }
                        lines.push(render_shared_issue_artifact_pointer_line(
                            "report_artifact",
                            &execution.report_artifact.id,
                            &execution.report_artifact.relative_path,
                            execution.report_artifact.bytes,
                        ));
                        lines.push(render_shared_issue_artifact_pointer_line(
                            "json_artifact",
                            &execution.json_artifact.id,
                            &execution.json_artifact.relative_path,
                            execution.json_artifact.bytes,
                        ));
                        lines.push(
                            "Use `/tau artifacts show <artifact_id>` to inspect full diagnostics."
                                .to_string(),
                        );
                        let message = lines.join("\n");
                        let posted = self
                            .post_issue_command_comment(
                                event.issue_number,
                                &event.key,
                                "doctor",
                                status,
                                &message,
                            )
                            .await?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key,
                            "issue_number": event.issue_number,
                            "command": "doctor",
                            "status": status,
                            "posted_comment_id": posted.id,
                            "posted_comment_url": posted.html_url,
                            "run_id": execution.run_id,
                            "online": command.online,
                            "summary": {
                                "checks": execution.checks,
                                "pass": execution.pass,
                                "warn": execution.warn,
                                "fail": execution.fail,
                            },
                            "report_artifact": {
                                "id": execution.report_artifact.id,
                                "path": execution.report_artifact.relative_path,
                                "bytes": execution.report_artifact.bytes,
                                "checksum_sha256": execution.report_artifact.checksum_sha256,
                            },
                            "json_artifact": {
                                "id": execution.json_artifact.id,
                                "path": execution.json_artifact.relative_path,
                                "bytes": execution.json_artifact.bytes,
                                "checksum_sha256": execution.json_artifact.checksum_sha256,
                            },
                        }))?;
                    }
                    Err(error) => {
                        let message = format!(
                            "Tau doctor diagnostics failed for issue #{}.\n\nError: {}",
                            event.issue_number,
                            truncate_for_error(&error.to_string(), 280)
                        );
                        let posted = self
                            .post_issue_command_comment(
                                event.issue_number,
                                &event.key,
                                "doctor",
                                "failed",
                                &message,
                            )
                            .await?;
                        self.outbound_log.append(&json!({
                            "timestamp_unix_ms": current_unix_timestamp_ms(),
                            "repo": self.repo.as_slug(),
                            "event_key": event.key,
                            "issue_number": event.issue_number,
                            "command": "doctor",
                            "status": "failed",
                            "posted_comment_id": posted.id,
                            "posted_comment_url": posted.html_url,
                            "online": command.online,
                            "error": error.to_string(),
                        }))?;
                    }
                }
            }
            TauIssueCommand::Canvas { args } => {
                let session_path =
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
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
                            channel: Some(issue_shared_session_id(event.issue_number)),
                            source_event_key: Some(event.key.clone()),
                            source_unix_ms: parse_shared_rfc3339_to_unix_ms(&event.occurred_at),
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
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let compact_report = shared_compact_issue_session(
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
                let message = tau_shared_command_usage("/tau");
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
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let (before_entries, after_entries, head_id) =
                    shared_ensure_issue_session_initialized(
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
                    issue_shared_session_id(event.issue_number),
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
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
                let (before_entries, after_entries, head_id) =
                    shared_ensure_issue_session_initialized(
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
                    issue_shared_session_id(event.issue_number),
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
                    let session_path = shared_session_path_for_issue(
                        &self.repository_state_dir,
                        event.issue_number,
                    );
                    let (removed_session, removed_lock) =
                        shared_reset_issue_session_files(&session_path)?;
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
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
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
                    normalize_shared_artifact_retention_days(self.config.artifact_retention_days),
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
                let (
                    session_id,
                    last_comment_id,
                    last_run_id,
                    active_run_id,
                    last_event_key,
                    last_event_kind,
                    last_actor_login,
                    last_reason_code,
                    last_processed_unix_ms,
                    total_processed_events,
                    total_duplicate_events,
                    total_failed_events,
                    total_denied_events,
                    total_runs_started,
                    total_runs_completed,
                    total_runs_failed,
                    has_session,
                ) = match session_state {
                    Some(state) => (
                        state.session_id.as_str(),
                        state.last_comment_id,
                        state.last_run_id.as_deref(),
                        state.active_run_id.as_deref(),
                        state.last_event_key.as_deref(),
                        state.last_event_kind.as_deref(),
                        state.last_actor_login.as_deref(),
                        state.last_reason_code.as_deref(),
                        state.last_processed_unix_ms,
                        state.total_processed_events,
                        state.total_duplicate_events,
                        state.total_failed_events,
                        state.total_denied_events,
                        state.total_runs_started,
                        state.total_runs_completed,
                        state.total_runs_failed,
                        true,
                    ),
                    None => (
                        "none", None, None, None, None, None, None, None, None, 0, 0, 0, 0, 0, 0,
                        0, false,
                    ),
                };
                let mut lines = Vec::new();
                if continuity.entries == 0 && !has_session {
                    lines.push(format!(
                        "No chat session found for issue #{}.",
                        event.issue_number
                    ));
                } else {
                    lines.push(format!(
                        "Chat session status for issue #{}.",
                        event.issue_number
                    ));
                }
                lines.push(format!("entries={}", continuity.entries));
                lines.push(format!(
                    "head={}",
                    continuity
                        .head_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "oldest_entry_id={}",
                    continuity
                        .oldest_entry_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "newest_entry_id={}",
                    continuity
                        .newest_entry_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "newest_entry_role={}",
                    continuity.newest_entry_role.as_deref().unwrap_or("none")
                ));
                lines.push(format!("session_id={}", session_id));
                lines.push(format!(
                    "last_comment_id={}",
                    last_comment_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!("last_run_id={}", last_run_id.unwrap_or("none")));
                lines.push(format!("active_run_id={}", active_run_id.unwrap_or("none")));
                lines.push(format!(
                    "last_event_key={}",
                    last_event_key.unwrap_or("none")
                ));
                lines.push(format!(
                    "last_event_kind={}",
                    last_event_kind.unwrap_or("none")
                ));
                lines.push(format!(
                    "last_actor_login={}",
                    last_actor_login.unwrap_or("none")
                ));
                lines.push(format!(
                    "last_reason_code={}",
                    last_reason_code.unwrap_or("none")
                ));
                lines.push(format!(
                    "last_processed_unix_ms={}",
                    last_processed_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!("total_processed_events={}", total_processed_events));
                lines.push(format!("total_duplicate_events={}", total_duplicate_events));
                lines.push(format!("total_failed_events={}", total_failed_events));
                lines.push(format!("total_denied_events={}", total_denied_events));
                lines.push(format!("total_runs_started={}", total_runs_started));
                lines.push(format!("total_runs_completed={}", total_runs_completed));
                lines.push(format!("total_runs_failed={}", total_runs_failed));
                lines.push(format!(
                    "lineage_digest_sha256={}",
                    continuity.lineage_digest_sha256
                ));
                lines.push(format!(
                    "artifact_active={}",
                    continuity.artifacts.active_records
                ));
                lines.push(format!(
                    "artifact_total={}",
                    continuity.artifacts.total_records
                ));
                lines.push(format!(
                    "artifact_latest_id={}",
                    continuity
                        .artifacts
                        .latest_artifact_id
                        .as_deref()
                        .unwrap_or("none")
                ));
                lines.push(format!(
                    "artifact_latest_run_id={}",
                    continuity
                        .artifacts
                        .latest_artifact_run_id
                        .as_deref()
                        .unwrap_or("none")
                ));
                lines.push(format!(
                    "artifact_latest_created_unix_ms={}",
                    continuity
                        .artifacts
                        .latest_artifact_created_unix_ms
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "artifact_index_invalid_lines={}",
                    continuity.artifacts.invalid_index_lines
                ));
                let message = lines.join("\n");
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
                            "active_run_id": active_run_id,
                            "last_event_key": last_event_key,
                            "last_event_kind": last_event_kind,
                            "last_actor_login": last_actor_login,
                            "last_reason_code": last_reason_code,
                            "last_processed_unix_ms": last_processed_unix_ms,
                            "total_processed_events": total_processed_events,
                            "total_duplicate_events": total_duplicate_events,
                            "total_failed_events": total_failed_events,
                            "total_denied_events": total_denied_events,
                            "total_runs_started": total_runs_started,
                            "total_runs_completed": total_runs_completed,
                            "total_runs_failed": total_runs_failed,
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
            TauIssueCommand::ChatSummary => {
                let continuity = self.issue_chat_continuity_summary(event.issue_number)?;
                let session_state = self.state_store.issue_session(event.issue_number);
                let mut lines = vec![format!("Chat summary for issue #{}.", event.issue_number)];
                lines.push(format!("entries={}", continuity.entries));
                lines.push(format!(
                    "head={}",
                    continuity
                        .head_id
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "none".to_string())
                ));
                lines.push(format!(
                    "newest_entry_role={}",
                    continuity.newest_entry_role.as_deref().unwrap_or("none")
                ));
                lines.push(format!(
                    "lineage_digest_sha256={}",
                    continuity.lineage_digest_sha256
                ));
                if let Some(session_state) = session_state {
                    lines.push(format!(
                        "last_run_id={}",
                        session_state.last_run_id.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "active_run_id={}",
                        session_state.active_run_id.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "total_processed_events={}",
                        session_state.total_processed_events
                    ));
                    lines.push(format!(
                        "total_duplicate_events={}",
                        session_state.total_duplicate_events
                    ));
                    lines.push(format!(
                        "total_failed_events={}",
                        session_state.total_failed_events
                    ));
                    lines.push(format!(
                        "total_denied_events={}",
                        session_state.total_denied_events
                    ));
                } else {
                    lines.push("session_id=none".to_string());
                }
                lines.push(format!(
                    "artifacts_active={} artifacts_total={}",
                    continuity.artifacts.active_records, continuity.artifacts.total_records
                ));
                let message = lines.join("\n");
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-summary",
                        "reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-summary",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                }))?;
            }
            TauIssueCommand::ChatReplay => {
                let session_state = self.state_store.issue_session(event.issue_number);
                let processed_tail = self.state_store.processed_event_tail(5);
                let mut lines = vec![format!(
                    "Chat replay hints for issue #{}.",
                    event.issue_number
                )];
                lines.push(format!(
                    "include_edited_comments={}",
                    self.config.include_edited_comments
                ));
                lines.push(format!(
                    "processed_event_window={}/{}",
                    processed_tail.len(),
                    self.state_store.processed_event_cap()
                ));
                if processed_tail.is_empty() {
                    lines.push("recent_event_keys=none".to_string());
                } else {
                    lines.push(format!("recent_event_keys={}", processed_tail.join(",")));
                }
                if let Some(state) = session_state {
                    lines.push(format!("session_id={}", state.session_id));
                    lines.push(format!(
                        "last_event_key={}",
                        state.last_event_key.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "last_event_kind={}",
                        state.last_event_kind.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "last_actor_login={}",
                        state.last_actor_login.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "last_reason_code={}",
                        state.last_reason_code.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "last_processed_unix_ms={}",
                        state
                            .last_processed_unix_ms
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    ));
                    lines.push(format!(
                        "active_run_id={}",
                        state.active_run_id.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "last_run_id={}",
                        state.last_run_id.as_deref().unwrap_or("none")
                    ));
                    lines.push(format!(
                        "total_duplicate_events={}",
                        state.total_duplicate_events
                    ));
                    lines.push(format!("total_failed_events={}", state.total_failed_events));
                } else {
                    lines.push("session_id=none".to_string());
                }
                lines.push(
                    "Replay guidance: use `/tau chat status` for full diagnostics, `/tau chat show` for recent transcript, and `/tau chat search <query>` for targeted replay context."
                        .to_string(),
                );
                let message = lines.join("\n");
                let posted = self
                    .post_issue_command_comment(
                        event.issue_number,
                        &event.key,
                        "chat-replay",
                        "reported",
                        &message,
                    )
                    .await?;
                self.outbound_log.append(&json!({
                    "timestamp_unix_ms": current_unix_timestamp_ms(),
                    "repo": self.repo.as_slug(),
                    "event_key": event.key,
                    "issue_number": event.issue_number,
                    "command": "chat-replay",
                    "status": "reported",
                    "posted_comment_id": posted.id,
                    "posted_comment_url": posted.html_url,
                    "recent_event_keys": processed_tail,
                }))?;
            }
            TauIssueCommand::ChatShow { limit } => {
                let session_path =
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
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
                            tau_session::SESSION_SEARCH_PREVIEW_CHARS
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
                    shared_session_path_for_issue(&self.repository_state_dir, event.issue_number);
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
            .filter(|record| !is_shared_expired_at(record.expires_unix_ms, now_unix_ms))
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
        let session_path = shared_session_path_for_issue(&self.repository_state_dir, issue_number);
        let store = SessionStore::load(&session_path)?;
        let head_id = store.head_id();
        let lineage = store.lineage_entries(head_id)?;
        let lineage_jsonl = store.export_lineage_jsonl(head_id)?;
        let digest = shared_sha256_hex(lineage_jsonl.as_bytes());
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
            lines.push(format!(
                "chat_active_run_id: {}",
                session.active_run_id.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_event_key: {}",
                session.last_event_key.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_event_kind: {}",
                session.last_event_kind.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_actor_login: {}",
                session.last_actor_login.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_reason_code: {}",
                session.last_reason_code.as_deref().unwrap_or("none")
            ));
            lines.push(format!(
                "chat_last_processed_unix_ms: {}",
                session
                    .last_processed_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string())
            ));
            lines.push(format!(
                "chat_total_processed_events: {}",
                session.total_processed_events
            ));
            lines.push(format!(
                "chat_total_duplicate_events: {}",
                session.total_duplicate_events
            ));
            lines.push(format!(
                "chat_total_failed_events: {}",
                session.total_failed_events
            ));
            lines.push(format!(
                "chat_total_denied_events: {}",
                session.total_denied_events
            ));
            lines.push(format!(
                "chat_total_runs_started: {}",
                session.total_runs_started
            ));
            lines.push(format!(
                "chat_total_runs_completed: {}",
                session.total_runs_completed
            ));
            lines.push(format!(
                "chat_total_runs_failed: {}",
                session.total_runs_failed
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

    async fn execute_demo_index_script(
        &self,
        args: &[String],
        include_binary: bool,
    ) -> Result<std::process::Output> {
        if !self.demo_index_script_path.exists() {
            bail!(
                "demo-index script not found at {}",
                self.demo_index_script_path.display()
            );
        }
        let mut command = tokio::process::Command::new(&self.demo_index_script_path);
        command.args(args);
        command.arg("--repo-root").arg(&self.demo_index_repo_root);
        if include_binary {
            command.arg("--binary").arg(&self.demo_index_binary_path);
            command.arg("--skip-build");
        }
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.output().await.with_context(|| {
            format!(
                "failed to execute demo-index script {}",
                self.demo_index_script_path.display()
            )
        })
    }

    async fn render_demo_index_inventory(&self, issue_number: u64) -> Result<String> {
        let args = vec!["--list".to_string(), "--json".to_string()];
        let output = self.execute_demo_index_script(&args, false).await?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            bail!(
                "demo-index list failed with exit code {}: {}",
                output.status.code().unwrap_or(1),
                truncate_for_error(&stderr, 240)
            );
        }
        let inventory: DemoIndexScenarioInventory =
            serde_json::from_str(&stdout).with_context(|| {
                format!(
                    "failed to parse demo-index list json output: {}",
                    truncate_for_error(&stdout, 240)
                )
            })?;
        let mut lines = vec![format!(
            "Tau demo-index scenario inventory for issue #{}: {} scenario(s).",
            issue_number,
            inventory.scenarios.len()
        )];
        for scenario in inventory.scenarios {
            lines.push(format!("- `{}`: {}", scenario.id, scenario.description));
            lines.push(format!(
                "  wrapper: {} | command: {}",
                scenario.wrapper, scenario.command
            ));
            if let Some(marker) = scenario.expected_markers.first() {
                lines.push(format!("  expected_marker: {}", marker));
            }
            lines.push(format!("  troubleshooting: {}", scenario.troubleshooting));
        }
        lines.push(String::new());
        lines.push(self.render_issue_demo_index_reports(issue_number)?);
        Ok(lines.join("\n"))
    }

    async fn execute_demo_index_run(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &DemoIndexRunCommand,
    ) -> Result<DemoIndexRunExecution> {
        let run_id = format!(
            "demo-index-{}-{}-{}",
            issue_number,
            current_unix_timestamp_ms(),
            shared_short_key_hash(event_key)
        );
        let report_dir = self.repository_state_dir.join("demo-index-reports");
        std::fs::create_dir_all(&report_dir)
            .with_context(|| format!("failed to create {}", report_dir.display()))?;
        let report_file = report_dir.join(format!("{}.json", run_id));
        let only = command.scenarios.join(",");
        let args = vec![
            "--json".to_string(),
            "--report-file".to_string(),
            report_file.display().to_string(),
            "--only".to_string(),
            only.clone(),
            "--timeout-seconds".to_string(),
            command.timeout_seconds.to_string(),
            "--fail-fast".to_string(),
        ];
        let output = self.execute_demo_index_script(&args, true).await?;
        let exit_code = output.status.code().unwrap_or(1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let report_payload = if report_file.exists() {
            std::fs::read_to_string(&report_file)
                .with_context(|| format!("failed to read {}", report_file.display()))?
        } else {
            stdout.clone()
        };
        let summary = serde_json::from_str::<DemoIndexRunReport>(&report_payload)
            .or_else(|_| serde_json::from_str::<DemoIndexRunReport>(&stdout))
            .ok();

        let command_line = format!(
            "{} --json --report-file {} --only {} --timeout-seconds {} --fail-fast --repo-root {} --binary {} --skip-build",
            self.demo_index_script_path.display(),
            report_file.display(),
            only,
            command.timeout_seconds,
            self.demo_index_repo_root.display(),
            self.demo_index_binary_path.display()
        );
        let channel_store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let retention_days =
            normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
        let report_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-demo-index-report",
            "private",
            retention_days,
            "json",
            &report_payload,
        )?;
        let log_payload = format!(
            "command: {command_line}\nexit_code: {exit_code}\n\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
        );
        let log_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-demo-index-log",
            "private",
            retention_days,
            "log",
            &log_payload,
        )?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "github".to_string(),
            payload: json!({
                "command": "demo-index-run",
                "run_id": run_id,
                "scenarios": command.scenarios.clone(),
                "timeout_seconds": command.timeout_seconds,
                "exit_code": exit_code,
                "report_artifact": {
                    "id": report_artifact.id,
                    "path": report_artifact.relative_path,
                    "checksum_sha256": report_artifact.checksum_sha256,
                    "bytes": report_artifact.bytes,
                    "expires_unix_ms": report_artifact.expires_unix_ms,
                },
                "log_artifact": {
                    "id": log_artifact.id,
                    "path": log_artifact.relative_path,
                    "checksum_sha256": log_artifact.checksum_sha256,
                    "bytes": log_artifact.bytes,
                    "expires_unix_ms": log_artifact.expires_unix_ms,
                },
            }),
        })?;
        Ok(DemoIndexRunExecution {
            run_id,
            command_line,
            exit_code,
            summary,
            report_artifact,
            log_artifact,
        })
    }

    fn render_issue_demo_index_reports(&self, issue_number: u64) -> Result<String> {
        let store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let loaded = store.load_artifact_records_tolerant()?;
        let now_unix_ms = current_unix_timestamp_ms();
        let mut reports = loaded
            .records
            .into_iter()
            .filter(|artifact| artifact.artifact_type == "github-issue-demo-index-report")
            .filter(|artifact| !is_shared_expired_at(artifact.expires_unix_ms, now_unix_ms))
            .collect::<Vec<_>>();
        reports.sort_by(|left, right| {
            right
                .created_unix_ms
                .cmp(&left.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut lines = vec![format!(
            "Tau demo-index latest report pointers for issue #{}: {}",
            issue_number,
            reports.len()
        )];
        if reports.is_empty() {
            lines.push("none".to_string());
        } else {
            for artifact in reports.iter().take(5) {
                lines.push(format!(
                    "- id `{}` run_id `{}` created_unix_ms `{}` bytes `{}` path `{}`",
                    artifact.id,
                    artifact.run_id,
                    artifact.created_unix_ms,
                    artifact.bytes,
                    artifact.relative_path,
                ));
            }
            if reports.len() > 5 {
                lines.push(format!(
                    "... {} additional reports omitted",
                    reports.len() - 5
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

    fn execute_issue_auth_command(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &TauIssueAuthCommand,
    ) -> Result<IssueAuthExecution> {
        let command_name = match command.kind {
            TauIssueAuthCommandKind::Status => "status",
            TauIssueAuthCommandKind::Matrix => "matrix",
        };
        let command_key = match command.kind {
            TauIssueAuthCommandKind::Status => "auth-status",
            TauIssueAuthCommandKind::Matrix => "auth-matrix",
        };
        let run_id = format!(
            "{}-{}-{}-{}",
            command_key,
            issue_number,
            current_unix_timestamp_ms(),
            shared_short_key_hash(event_key)
        );
        let report_payload = execute_auth_command(&self.config.auth_command_config, &command.args);
        let json_args = ensure_shared_auth_json_flag(&command.args);
        let report_payload_json =
            execute_auth_command(&self.config.auth_command_config, &json_args);
        let summary_kind = match command.kind {
            TauIssueAuthCommandKind::Status => IssueAuthSummaryKind::Status,
            TauIssueAuthCommandKind::Matrix => IssueAuthSummaryKind::Matrix,
        };
        let summary_line = build_shared_issue_auth_summary_line(summary_kind, &report_payload_json);
        let channel_store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let retention_days =
            normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
        let report_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-auth-report",
            "private",
            retention_days,
            "txt",
            &report_payload,
        )?;
        let json_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-auth-json",
            "private",
            retention_days,
            "json",
            &report_payload_json,
        )?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "github".to_string(),
            payload: json!({
                "command": command_key,
                "run_id": run_id,
                "args": command.args,
                "json_args": json_args,
                "subscription_strict": self.config.auth_command_config.provider_subscription_strict,
                "summary": summary_line,
                "report_artifact": {
                    "id": report_artifact.id,
                    "path": report_artifact.relative_path,
                    "checksum_sha256": report_artifact.checksum_sha256,
                    "bytes": report_artifact.bytes,
                    "expires_unix_ms": report_artifact.expires_unix_ms,
                },
                "json_artifact": {
                    "id": json_artifact.id,
                    "path": json_artifact.relative_path,
                    "checksum_sha256": json_artifact.checksum_sha256,
                    "bytes": json_artifact.bytes,
                    "expires_unix_ms": json_artifact.expires_unix_ms,
                },
            }),
        })?;
        Ok(IssueAuthExecution {
            run_id,
            command_name,
            summary_line,
            subscription_strict: self.config.auth_command_config.provider_subscription_strict,
            report_artifact,
            json_artifact,
        })
    }

    fn render_issue_auth_posture_lines(&self) -> Vec<String> {
        vec![
            format!(
                "provider_mode: openai={} anthropic={} google={}",
                self.config.auth_command_config.openai_auth_mode.as_str(),
                self.config.auth_command_config.anthropic_auth_mode.as_str(),
                self.config.auth_command_config.google_auth_mode.as_str()
            ),
            format!(
                "login_backend_enabled: openai_codex={} anthropic_claude={} google_gemini={}",
                self.config.auth_command_config.openai_codex_backend,
                self.config.auth_command_config.anthropic_claude_backend,
                self.config.auth_command_config.google_gemini_backend
            ),
        ]
    }

    fn execute_issue_doctor_command(
        &self,
        issue_number: u64,
        event_key: &str,
        command: IssueDoctorCommand,
    ) -> Result<IssueDoctorExecution> {
        let run_id = format!(
            "doctor-{}-{}-{}",
            issue_number,
            current_unix_timestamp_ms(),
            shared_short_key_hash(event_key)
        );
        let checks = run_doctor_checks_with_options(
            &self.config.doctor_config,
            DoctorCheckOptions {
                online: command.online,
            },
        );
        let pass = checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Pass)
            .count();
        let warn = checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Warn)
            .count();
        let fail = checks
            .iter()
            .filter(|check| check.status == DoctorStatus::Fail)
            .count();
        let highlighted = checks
            .iter()
            .filter(|check| check.status != DoctorStatus::Pass)
            .take(5)
            .map(|check| {
                format!(
                    "key={} status={} code={}",
                    check.key,
                    doctor_status_label(check.status),
                    check.code
                )
            })
            .collect::<Vec<_>>();
        let report_payload = render_doctor_report(&checks);
        let report_payload_json = render_doctor_report_json(&checks);
        let channel_store = ChannelStore::open(
            &self.repository_state_dir.join("channel-store"),
            "github",
            &format!("issue-{issue_number}"),
        )?;
        let retention_days =
            normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
        let report_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-doctor-report",
            "private",
            retention_days,
            "txt",
            &report_payload,
        )?;
        let json_artifact = channel_store.write_text_artifact(
            &run_id,
            "github-issue-doctor-json",
            "private",
            retention_days,
            "json",
            &report_payload_json,
        )?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: current_unix_timestamp_ms(),
            direction: "outbound".to_string(),
            event_key: Some(event_key.to_string()),
            source: "github".to_string(),
            payload: json!({
                "command": "doctor",
                "run_id": run_id,
                "online": command.online,
                "summary": {
                    "checks": checks.len(),
                    "pass": pass,
                    "warn": warn,
                    "fail": fail,
                },
                "report_artifact": {
                    "id": report_artifact.id,
                    "path": report_artifact.relative_path,
                    "checksum_sha256": report_artifact.checksum_sha256,
                    "bytes": report_artifact.bytes,
                    "expires_unix_ms": report_artifact.expires_unix_ms,
                },
                "json_artifact": {
                    "id": json_artifact.id,
                    "path": json_artifact.relative_path,
                    "checksum_sha256": json_artifact.checksum_sha256,
                    "bytes": json_artifact.bytes,
                    "expires_unix_ms": json_artifact.expires_unix_ms,
                },
            }),
        })?;
        Ok(IssueDoctorExecution {
            run_id,
            checks: checks.len(),
            pass,
            warn,
            fail,
            highlighted,
            report_artifact,
            json_artifact,
        })
    }

    async fn post_issue_command_comment(
        &self,
        issue_number: u64,
        event_key: &str,
        command: &str,
        status: &str,
        message: &str,
    ) -> Result<GithubCommentCreateResponse> {
        let normalized_status = normalize_issue_command_status(status).to_string();
        let reason_code = issue_command_reason_code(command, &normalized_status);
        let mut content = if message.trim().is_empty() {
            "Tau command response.".to_string()
        } else {
            message.trim().to_string()
        };
        let mut overflow_artifact: Option<ChannelArtifactRecord> = None;
        let mut body = render_issue_command_comment(
            event_key,
            command,
            &normalized_status,
            &reason_code,
            &content,
        );
        if body.chars().count() > GITHUB_COMMENT_MAX_CHARS {
            let channel_store = ChannelStore::open(
                &self.repository_state_dir.join("channel-store"),
                "github",
                &format!("issue-{issue_number}"),
            )?;
            let run_id = format!(
                "command-overflow-{}-{}-{}",
                issue_number,
                current_unix_timestamp_ms(),
                shared_short_key_hash(event_key)
            );
            let retention_days =
                normalize_shared_artifact_retention_days(self.config.artifact_retention_days);
            let artifact = channel_store.write_text_artifact(
                &run_id,
                "github-issue-command-overflow",
                "private",
                retention_days,
                "txt",
                &content,
            )?;
            let overflow_suffix = format!(
                "output_truncated: true\n{}",
                render_shared_issue_artifact_pointer_line(
                    "overflow_artifact",
                    &artifact.id,
                    &artifact.relative_path,
                    artifact.bytes,
                )
            );
            let mut excerpt_len = content.chars().count();
            loop {
                let excerpt = split_at_char_index(&content, excerpt_len).0;
                content = if excerpt.trim().is_empty() {
                    overflow_suffix.clone()
                } else {
                    format!("{}\n\n{}", excerpt.trim_end(), overflow_suffix)
                };
                body = render_issue_command_comment(
                    event_key,
                    command,
                    &normalized_status,
                    &reason_code,
                    &content,
                );
                if body.chars().count() <= GITHUB_COMMENT_MAX_CHARS || excerpt_len == 0 {
                    break;
                }
                let overflow = body.chars().count() - GITHUB_COMMENT_MAX_CHARS;
                excerpt_len = excerpt_len.saturating_sub(overflow.saturating_add(8));
            }
            overflow_artifact = Some(artifact);
        }
        let posted = self
            .github_client
            .create_issue_comment(issue_number, &body)
            .await?;
        self.outbound_log.append(&json!({
            "timestamp_unix_ms": current_unix_timestamp_ms(),
            "repo": self.repo.as_slug(),
            "event_key": event_key,
            "issue_number": issue_number,
            "command": command,
            "status": normalized_status,
            "reason_code": reason_code,
            "posted_comment_id": posted.id,
            "posted_comment_url": posted.html_url,
            "overflow_artifact": overflow_artifact.as_ref().map(|artifact| json!({
                "id": artifact.id,
                "path": artifact.relative_path,
                "bytes": artifact.bytes,
                "checksum_sha256": artifact.checksum_sha256,
            })),
        }))?;
        Ok(posted)
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
            TauIssueCommand::ChatSummary => "command:/tau-chat-summary".to_string(),
            TauIssueCommand::ChatReplay => "command:/tau-chat-replay".to_string(),
            TauIssueCommand::ChatShow { .. } => "command:/tau-chat-show".to_string(),
            TauIssueCommand::ChatSearch { .. } => "command:/tau-chat-search".to_string(),
            TauIssueCommand::Artifacts { .. } => "command:/tau-artifacts".to_string(),
            TauIssueCommand::ArtifactShow { .. } => "command:/tau-artifacts-show".to_string(),
            TauIssueCommand::DemoIndexList => "command:/tau-demo-index".to_string(),
            TauIssueCommand::DemoIndexRun { .. } => "command:/tau-demo-index".to_string(),
            TauIssueCommand::DemoIndexReport => "command:/tau-demo-index".to_string(),
            TauIssueCommand::Auth { command } => match command.kind {
                TauIssueAuthCommandKind::Status => "command:/tau-auth-status".to_string(),
                TauIssueAuthCommandKind::Matrix => "command:/tau-auth-matrix".to_string(),
            },
            TauIssueCommand::Doctor { .. } => "command:/tau-doctor".to_string(),
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
            vec![render_shared_issue_run_error_comment(
                &event.key,
                &run_id,
                &error.to_string(),
                EVENT_KEY_MARKER_PREFIX,
                EVENT_KEY_MARKER_SUFFIX,
            )],
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
        normalize_shared_artifact_retention_days(config.artifact_retention_days);
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
            ..AgentConfig::default()
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
        collect_shared_assistant_reply(&agent.messages()[start_index..])
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
        .join(shared_sanitize_for_path(&event.key));
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
        let safe_name = shared_sanitize_for_path(&original_name);
        let safe_name = if safe_name.is_empty() {
            format!("attachment-{}.bin", index + 1)
        } else {
            safe_name
        };
        let path = file_dir.join(format!("{:02}-{}", index + 1, safe_name));
        std::fs::write(&path, &payload.bytes)
            .with_context(|| format!("failed to write {}", path.display()))?;
        let relative_path = normalize_shared_relative_channel_path(
            &path,
            &channel_store.channel_dir(),
            "attachment file",
        )
        .map_err(|error| anyhow!(error))?;
        let created_unix_ms = current_unix_timestamp_ms();
        let expires_unix_ms = retention_days
            .map(|days| days.saturating_mul(86_400_000))
            .map(|ttl| created_unix_ms.saturating_add(ttl));
        let checksum_sha256 = shared_sha256_hex(&payload.bytes);
        let policy_reason_code = if content_policy.reason_code == "allow_content_type_default" {
            url_policy.reason_code
        } else {
            content_policy.reason_code
        };
        let record = ChannelAttachmentRecord {
            id: format!(
                "attachment-{}-{}",
                created_unix_ms,
                shared_short_key_hash(&format!("{}:{}:{}:{}", run_id, event.key, index, url))
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
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
        collect_shared_issue_events, evaluate_attachment_content_type_policy,
        evaluate_attachment_url_policy, event_action_from_shared_body, extract_attachment_urls,
        extract_footer_event_keys, is_retryable_github_status, issue_command_reason_code,
        issue_matches_required_labels, issue_matches_required_numbers, issue_shared_session_id,
        normalize_issue_command_status, normalize_shared_artifact_retention_days,
        normalize_shared_relative_channel_path, parse_shared_rfc3339_to_unix_ms,
        parse_tau_issue_command, post_issue_comment_chunks, render_event_prompt,
        render_issue_command_comment, render_issue_comment_chunks_with_limit,
        render_issue_comment_response_parts, retry_delay, run_prompt_for_event,
        shared_sanitize_for_path, shared_session_path_for_issue, DemoIndexRunCommand,
        DownloadedGithubAttachment, EventAction, GithubApiClient, GithubBridgeEvent,
        GithubBridgeEventKind, GithubIssue, GithubIssueComment, GithubIssueLabel,
        GithubIssuesBridgeRuntime, GithubIssuesBridgeRuntimeConfig, GithubIssuesBridgeStateStore,
        GithubUser, IssueDoctorCommand, IssueEventOutcome, PromptRunReport, PromptUsageSummary,
        RepoRef, RunPromptForEventRequest, SessionStore, TauIssueAuthCommand,
        TauIssueAuthCommandKind, TauIssueCommand, CHAT_SHOW_DEFAULT_LIMIT,
        DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS, DEMO_INDEX_SCENARIOS, EVENT_KEY_MARKER_PREFIX,
    };
    use crate::{
        channel_store::{ChannelArtifactRecord, ChannelStore},
        tools::ToolPolicy,
        AuthCommandConfig, CredentialStoreEncryptionMode, DoctorCommandConfig,
        DoctorMultiChannelReadinessConfig, PromptRunStatus, ProviderAuthMethod, RenderOptions,
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
            poll_once: false,
            required_labels: Vec::new(),
            required_issue_numbers: Vec::new(),
            include_issue_body: false,
            include_edited_comments: true,
            processed_event_cap: 32,
            retry_max_attempts: 3,
            retry_base_delay_ms: 5,
            artifact_retention_days: 30,
            auth_command_config: AuthCommandConfig {
                credential_store: state_dir.join("credentials.json"),
                credential_store_key: None,
                credential_store_encryption: CredentialStoreEncryptionMode::None,
                api_key: Some("integration-key".to_string()),
                openai_api_key: None,
                anthropic_api_key: None,
                google_api_key: None,
                openai_auth_mode: ProviderAuthMethod::ApiKey,
                anthropic_auth_mode: ProviderAuthMethod::ApiKey,
                google_auth_mode: ProviderAuthMethod::ApiKey,
                provider_subscription_strict: true,
                openai_codex_backend: true,
                openai_codex_cli: "codex".to_string(),
                anthropic_claude_backend: true,
                anthropic_claude_cli: "claude".to_string(),
                google_gemini_backend: true,
                google_gemini_cli: "gemini".to_string(),
                google_gcloud_cli: "gcloud".to_string(),
            },
            demo_index_repo_root: None,
            demo_index_script_path: None,
            demo_index_binary_path: None,
            doctor_config: DoctorCommandConfig {
                model: "openai/gpt-4o-mini".to_string(),
                provider_keys: Vec::new(),
                release_channel_path: state_dir.join("release-channel.json"),
                release_lookup_cache_path: state_dir.join("release-cache.json"),
                release_lookup_cache_ttl_ms: 900_000,
                browser_automation_playwright_cli: "playwright".to_string(),
                session_enabled: true,
                session_path: state_dir.join("session.jsonl"),
                skills_dir: state_dir.join("skills"),
                skills_lock_path: state_dir.join("skills.lock.json"),
                trust_root_path: None,
                multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
            },
        }
    }

    fn write_executable_script(path: &Path, body: &str) {
        std::fs::write(path, body).expect("write script");
        let status = std::process::Command::new("chmod")
            .arg("+x")
            .arg(path)
            .status()
            .expect("chmod script");
        assert!(status.success());
    }

    fn write_demo_index_list_stub(path: &Path) {
        write_executable_script(
            path,
            r#"#!/usr/bin/env bash
set -euo pipefail
cat <<'JSON'
{"schema_version":1,"scenarios":[{"id":"onboarding","wrapper":"local.sh","command":"./scripts/demo/local.sh","description":"Bootstrap local Tau state.","expected_markers":["[demo:local] PASS onboard-non-interactive"],"troubleshooting":"rerun local.sh"}]}
JSON
"#,
        );
    }

    fn write_demo_index_run_stub(path: &Path) {
        write_executable_script(
            path,
            r#"#!/usr/bin/env bash
set -euo pipefail
report_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --report-file)
      report_file="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
payload='{"schema_version":1,"scenarios":[{"id":"onboarding","status":"passed","exit_code":0,"duration_ms":9}],"summary":{"total":1,"passed":1,"failed":0}}'
if [[ -n "${report_file}" ]]; then
  mkdir -p "$(dirname "${report_file}")"
  printf '%s\n' "${payload}" > "${report_file}"
fi
printf '%s\n' "${payload}"
"#,
        );
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

    mod core_and_parsing;

    #[tokio::test]
    async fn functional_bridge_demo_index_list_command_reports_inventory() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 23,
                "number": 23,
                "title": "Demo index list",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/23/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2301,
                    "body": "/tau demo-index list",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let list_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/23/comments")
                .body_includes("Tau demo-index scenario inventory for issue #23: 1 scenario(s).")
                .body_includes("`onboarding`")
                .body_includes("Tau demo-index latest report pointers for issue #23: 0");
            then.status(201).json_body(json!({
                "id": 2302,
                "html_url": "https://example.test/comment/2302"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("demo-index-stub.sh");
        write_demo_index_list_stub(&script_path);
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.demo_index_repo_root = Some(temp.path().to_path_buf());
        config.demo_index_script_path = Some(script_path);
        config.demo_index_binary_path = Some(temp.path().join("tau-coding-agent"));
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        list_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_demo_index_run_command_posts_artifact_pointers() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 24,
                "number": 24,
                "title": "Demo index run",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/24/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2401,
                    "body": "/tau demo-index run onboarding --timeout-seconds 120",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let run_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/24/comments")
                .body_includes("Tau demo-index run for issue #24: status=completed")
                .body_includes("summary: total=1 passed=1 failed=0")
                .body_includes("report_artifact:")
                .body_includes("log_artifact:")
                .body_includes("Use `/tau demo-index report`");
            then.status(201).json_body(json!({
                "id": 2402,
                "html_url": "https://example.test/comment/2402"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("demo-index-stub.sh");
        write_demo_index_run_stub(&script_path);
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.demo_index_repo_root = Some(temp.path().to_path_buf());
        config.demo_index_script_path = Some(script_path);
        config.demo_index_binary_path = Some(temp.path().join("tau-coding-agent"));
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        run_post.assert_calls(1);

        let store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-24",
        )
        .expect("channel store");
        let artifacts = store
            .load_artifact_records_tolerant()
            .expect("artifact records");
        let report_count = artifacts
            .records
            .iter()
            .filter(|record| record.artifact_type == "github-issue-demo-index-report")
            .count();
        let log_count = artifacts
            .records
            .iter()
            .filter(|record| record.artifact_type == "github-issue-demo-index-log")
            .count();
        assert_eq!(report_count, 1);
        assert_eq!(log_count, 1);
    }

    #[tokio::test]
    async fn regression_bridge_demo_index_run_command_replay_guard_prevents_duplicate_execution() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 25,
                "number": 25,
                "title": "Demo index replay",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/25/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2501,
                    "body": "/tau demo-index run onboarding --timeout-seconds 120",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let run_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/25/comments")
                .body_includes("Tau demo-index run for issue #25: status=completed");
            then.status(201).json_body(json!({
                "id": 2502,
                "html_url": "https://example.test/comment/2502"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let script_path = temp.path().join("demo-index-stub.sh");
        write_demo_index_run_stub(&script_path);
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.demo_index_repo_root = Some(temp.path().to_path_buf());
        config.demo_index_script_path = Some(script_path);
        config.demo_index_binary_path = Some(temp.path().join("tau-coding-agent"));
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        let second = runtime.poll_once().await.expect("second poll");
        assert_eq!(first.processed_events, 1);
        assert_eq!(first.failed_events, 0);
        assert_eq!(second.processed_events, 0);
        run_post.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_bridge_doctor_command_reports_summary_and_artifact_pointers() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 260,
                "number": 26,
                "title": "Doctor summary",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/26/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2601,
                    "body": "/tau doctor",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let doctor_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/26/comments")
                .body_includes("Tau doctor diagnostics for issue #26: status=")
                .body_includes("summary: checks=")
                .body_includes("report_artifact:")
                .body_includes("json_artifact:");
            then.status(201).json_body(json!({
                "id": 2602,
                "html_url": "https://example.test/comment/2602"
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
        doctor_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_doctor_command_persists_report_artifacts() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 270,
                "number": 27,
                "title": "Doctor artifacts",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/27/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2701,
                    "body": "/tau doctor",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let doctor_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/27/comments")
                .body_includes("Tau doctor diagnostics for issue #27: status=");
            then.status(201).json_body(json!({
                "id": 2702,
                "html_url": "https://example.test/comment/2702"
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
        doctor_post.assert_calls(1);

        let store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-27",
        )
        .expect("channel store");
        let artifacts = store
            .load_artifact_records_tolerant()
            .expect("artifact records");
        let report_count = artifacts
            .records
            .iter()
            .filter(|record| record.artifact_type == "github-issue-doctor-report")
            .count();
        let json_count = artifacts
            .records
            .iter()
            .filter(|record| record.artifact_type == "github-issue-doctor-json")
            .count();
        assert_eq!(report_count, 1);
        assert_eq!(json_count, 1);
    }

    #[tokio::test]
    async fn regression_bridge_doctor_command_replay_guard_prevents_duplicate_execution() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 280,
                "number": 28,
                "title": "Doctor replay",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/28/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2801,
                    "body": "/tau doctor",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let doctor_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/28/comments")
                .body_includes("Tau doctor diagnostics for issue #28: status=");
            then.status(201).json_body(json!({
                "id": 2802,
                "html_url": "https://example.test/comment/2802"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        let second = runtime.poll_once().await.expect("second poll");
        assert_eq!(first.processed_events, 1);
        assert_eq!(first.failed_events, 0);
        assert_eq!(second.processed_events, 0);
        doctor_post.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_bridge_auth_status_command_reports_summary_and_artifact_pointers() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 290,
                "number": 29,
                "title": "Auth status",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/29/comments");
            then.status(200).json_body(json!([
                {
                    "id": 2901,
                    "body": "/tau auth status",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let auth_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/29/comments")
                .body_includes("Tau auth diagnostics for issue #29: command=status")
                .body_includes("subscription_strict:")
                .body_includes("provider_mode:")
                .body_includes("report_artifact:")
                .body_includes("json_artifact:");
            then.status(201).json_body(json!({
                "id": 2902,
                "html_url": "https://example.test/comment/2902"
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
        auth_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_auth_matrix_command_persists_report_artifacts() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 300,
                "number": 30,
                "title": "Auth matrix",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/30/comments");
            then.status(200).json_body(json!([
                {
                    "id": 3001,
                    "body": "/tau auth matrix --mode-support supported",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let auth_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/30/comments")
                .body_includes("Tau auth diagnostics for issue #30: command=matrix");
            then.status(201).json_body(json!({
                "id": 3002,
                "html_url": "https://example.test/comment/3002"
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
        auth_post.assert_calls(1);

        let store = ChannelStore::open(
            &temp.path().join("owner__repo").join("channel-store"),
            "github",
            "issue-30",
        )
        .expect("channel store");
        let artifacts = store
            .load_artifact_records_tolerant()
            .expect("artifact records");
        let report_count = artifacts
            .records
            .iter()
            .filter(|record| record.artifact_type == "github-issue-auth-report")
            .count();
        let json_count = artifacts
            .records
            .iter()
            .filter(|record| record.artifact_type == "github-issue-auth-json")
            .count();
        assert_eq!(report_count, 1);
        assert_eq!(json_count, 1);
    }

    #[tokio::test]
    async fn regression_bridge_auth_status_command_replay_guard_prevents_duplicate_execution() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 310,
                "number": 31,
                "title": "Auth replay",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/31/comments");
            then.status(200).json_body(json!([
                {
                    "id": 3101,
                    "body": "/tau auth status",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let auth_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/31/comments")
                .body_includes("Tau auth diagnostics for issue #31: command=status");
            then.status(201).json_body(json!({
                "id": 3102,
                "html_url": "https://example.test/comment/3102"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        let second = runtime.poll_once().await.expect("second poll");
        assert_eq!(first.processed_events, 1);
        assert_eq!(first.failed_events, 0);
        assert_eq!(second.processed_events, 0);
        auth_post.assert_calls(1);
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
    async fn integration_bridge_poll_filters_issues_by_required_label() {
        let server = MockServer::start();
        let issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([
                {
                    "id": 10,
                    "number": 7,
                    "title": "Bridge me",
                    "body": "",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:05Z",
                    "user": {"login":"alice"},
                    "labels": [{"name":"tau-ready"}]
                },
                {
                    "id": 11,
                    "number": 8,
                    "title": "Skip me",
                    "body": "",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:06Z",
                    "user": {"login":"alice"},
                    "labels": [{"name":"other"}]
                }
            ]));
        });
        let comments_7 = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/7/comments");
            then.status(200).json_body(json!([{
                "id": 200,
                "body": "hello from issue stream",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let comments_8 = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/8/comments");
            then.status(200).json_body(json!([]));
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
                .body_includes("tau-event-key:issue-comment-created:200");
            then.status(200).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.required_labels = vec!["tau-ready".to_string()];
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        let second = runtime.poll_once().await.expect("second poll");

        assert_eq!(first.discovered_events, 1);
        assert_eq!(first.processed_events, 1);
        assert_eq!(second.processed_events, 0);
        issues.assert_calls(2);
        comments_7.assert_calls(2);
        comments_8.assert_calls(0);
        working_post.assert_calls(1);
        update.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_poll_filters_issues_by_required_number() {
        let server = MockServer::start();
        let issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([
                {
                    "id": 10,
                    "number": 7,
                    "title": "Bridge me",
                    "body": "",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:05Z",
                    "user": {"login":"alice"}
                },
                {
                    "id": 11,
                    "number": 8,
                    "title": "Skip me",
                    "body": "",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:06Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let comments_7 = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/7/comments");
            then.status(200).json_body(json!([{
                "id": 200,
                "body": "hello from issue stream",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let comments_8 = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues/8/comments");
            then.status(200).json_body(json!([]));
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
                .body_includes("tau-event-key:issue-comment-created:200");
            then.status(200).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.required_issue_numbers = vec![7];
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let first = runtime.poll_once().await.expect("first poll");
        let second = runtime.poll_once().await.expect("second poll");

        assert_eq!(first.discovered_events, 1);
        assert_eq!(first.processed_events, 1);
        assert_eq!(second.processed_events, 0);
        issues.assert_calls(2);
        comments_7.assert_calls(2);
        comments_8.assert_calls(0);
        working_post.assert_calls(1);
        update.assert_calls(1);
    }

    #[tokio::test]
    async fn functional_bridge_run_poll_once_completes_single_cycle_and_exits() {
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
                .body_includes("tau-event-key:issue-comment-created:200");
            then.status(200).json_body(json!({
                "id": 901,
                "html_url": "https://example.test/comment/901"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.poll_once = true;
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        runtime.run().await.expect("poll-once run");

        issues.assert_calls(1);
        comments.assert_calls(1);
        working_post.assert_calls(1);
        update.assert_calls(1);

        let state_path = temp.path().join("owner__repo").join("state.json");
        let state_raw = std::fs::read_to_string(&state_path).expect("state file");
        let state: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
        let issue_session = state
            .get("issue_sessions")
            .and_then(serde_json::Value::as_object)
            .and_then(|sessions| sessions.get("7"))
            .expect("issue session");
        assert!(issue_session
            .get("last_run_id")
            .and_then(serde_json::Value::as_str)
            .is_some());
    }

    #[tokio::test]
    async fn regression_bridge_run_poll_once_propagates_poll_errors() {
        let server = MockServer::start();
        let issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(500).body("boom");
        });

        let temp = tempdir().expect("tempdir");
        let mut config = test_bridge_config(&server.base_url(), temp.path());
        config.poll_once = true;
        config.retry_max_attempts = 1;
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let error = runtime.run().await.expect_err("poll-once should fail");
        assert!(error
            .to_string()
            .contains("github api list issues failed with status 500"));
        issues.assert_calls(1);

        let state_path = temp.path().join("owner__repo").join("state.json");
        let state_raw = std::fs::read_to_string(&state_path).expect("state file");
        let state: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
        let health = state
            .get("health")
            .and_then(serde_json::Value::as_object)
            .expect("health object");
        assert_eq!(
            health
                .get("failure_streak")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
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
            .join(shared_sanitize_for_path("issue-comment-created:1200"));
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
                .body_includes("transport_failure_streak: 0")
                .body_includes("reason_code `issue_command_status_reported`");
            then.status(201).json_body(json!({
                "id": 930,
                "html_url": "https://example.test/comment/930"
            }));
        });
        let stop_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/9/comments")
                .body_includes("No active run for this issue. Current state is idle.")
                .body_includes("reason_code `issue_command_stop_acknowledged`");
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
                .body_includes("transport_health_recommendation:")
                .body_includes("reason_code `issue_command_health_reported`");
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
    async fn integration_bridge_command_logs_include_normalized_reason_codes() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 120,
                "number": 12,
                "title": "Reason code logs",
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
                    "id": 1201,
                    "body": "/tau status",
                    "created_at": "2026-01-01T00:00:01Z",
                    "updated_at": "2026-01-01T00:00:01Z",
                    "user": {"login":"alice"}
                }
            ]));
        });
        let _status_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/12/comments")
                .body_includes("Tau status for issue #12: idle");
            then.status(201).json_body(json!({
                "id": 1202,
                "html_url": "https://example.test/comment/1202"
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

        let outbound = std::fs::read_to_string(
            temp.path()
                .join("owner__repo")
                .join("outbound-events.jsonl"),
        )
        .expect("read outbound log");
        assert!(outbound.contains("\"command\":\"status\""));
        assert!(outbound.contains("\"status\":\"reported\""));
        assert!(outbound.contains("\"reason_code\":\"issue_command_status_reported\""));
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
        let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 9);
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
        let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 11);
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
        let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 12);
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
            issue_shared_session_id(12),
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
    async fn integration_bridge_chat_summary_reports_session_digest() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 30,
                "number": 18,
                "title": "Chat Summary",
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
                "id": 1211,
                "body": "/tau chat summary",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let summary_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/18/comments")
                .body_includes("Chat summary for issue #18.")
                .body_includes("entries=2")
                .body_includes("lineage_digest_sha256=")
                .body_includes("total_processed_events=2")
                .body_includes("total_denied_events=1");
            then.status(201).json_body(json!({
                "id": 980,
                "html_url": "https://example.test/comment/980"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 18);
        if let Some(parent) = session_path.parent() {
            std::fs::create_dir_all(parent).expect("create session dir");
        }
        let mut store = SessionStore::load(&session_path).expect("store");
        store
            .append_messages(
                None,
                &[
                    Message::user("summary request"),
                    Message::assistant_text("summary response"),
                ],
            )
            .expect("append messages");
        runtime.state_store.update_issue_session(
            18,
            issue_shared_session_id(18),
            Some(1500),
            Some("run-18".to_string()),
        );
        runtime.state_store.record_issue_event_outcome(
            18,
            "issue-comment-created:seed-1",
            "issue_comment_created",
            "alice",
            IssueEventOutcome::Processed,
            Some("command_processed"),
        );
        runtime.state_store.record_issue_event_outcome(
            18,
            "issue-comment-created:seed-2",
            "issue_comment_created",
            "alice",
            IssueEventOutcome::Denied,
            Some("pairing_denied"),
        );

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        summary_post.assert_calls(1);
    }

    #[tokio::test]
    async fn integration_bridge_chat_replay_reports_diagnostics_hints() {
        let server = MockServer::start();
        let _issues = server.mock(|when, then| {
            when.method(GET).path("/repos/owner/repo/issues");
            then.status(200).json_body(json!([{
                "id": 31,
                "number": 19,
                "title": "Chat Replay",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            }]));
        });
        let _comments = server.mock(|when, then| {
            when.method(GET)
                .path("/repos/owner/repo/issues/19/comments");
            then.status(200).json_body(json!([{
                "id": 1311,
                "body": "/tau chat replay",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }]));
        });
        let replay_post = server.mock(|when, then| {
            when.method(POST)
                .path("/repos/owner/repo/issues/19/comments")
                .body_includes("Chat replay hints for issue #19.")
                .body_includes(
                    "recent_event_keys=issue-comment-created:seed-a,issue-comment-created:seed-b",
                )
                .body_includes("last_reason_code=duplicate_event")
                .body_includes("Replay guidance: use `/tau chat status`");
            then.status(201).json_body(json!({
                "id": 981,
                "html_url": "https://example.test/comment/981"
            }));
        });

        let temp = tempdir().expect("tempdir");
        let config = test_bridge_config(&server.base_url(), temp.path());
        let mut runtime = GithubIssuesBridgeRuntime::new(config)
            .await
            .expect("runtime");
        runtime
            .state_store
            .mark_processed("issue-comment-created:seed-a");
        runtime
            .state_store
            .mark_processed("issue-comment-created:seed-b");
        runtime.state_store.update_issue_session(
            19,
            issue_shared_session_id(19),
            Some(1600),
            Some("run-19".to_string()),
        );
        runtime.state_store.record_issue_duplicate_event(
            19,
            "issue-comment-created:seed-b",
            "issue_comment_created",
            "alice",
        );

        let report = runtime.poll_once().await.expect("poll");
        assert_eq!(report.processed_events, 1);
        assert_eq!(report.failed_events, 0);
        replay_post.assert_calls(1);
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
        let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 14);
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
        let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 16);
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
