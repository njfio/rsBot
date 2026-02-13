use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
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
use tau_github_issues::github_transport_helpers::truncate_for_error;
#[cfg(test)]
use tau_github_issues::github_transport_helpers::{is_retryable_github_status, retry_delay};
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
    collect_issue_events as collect_shared_issue_events, GithubBridgeEvent,
};
#[cfg(test)]
use tau_github_issues::issue_event_collection::{
    GithubBridgeEventKind, GithubIssue, GithubIssueComment, GithubIssueLabel, GithubUser,
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

mod github_api_client;
mod issue_command_helpers;
mod issue_render_helpers;
mod issue_run_task;
mod issue_session_runtime;
mod issue_state_store;
mod prompt_execution;

use github_api_client::{GithubApiClient, GithubCommentCreateResponse};
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
#[cfg(test)]
use issue_run_task::post_issue_comment_chunks;
use issue_run_task::{execute_issue_run_task, IssueRunTaskParams, RunTaskResult};
use issue_session_runtime::initialize_issue_session_runtime;
use issue_state_store::{GithubIssuesBridgeStateStore, IssueEventOutcome, JsonlEventLog};
use prompt_execution::{
    run_prompt_for_event, DownloadedGithubAttachment, PromptRunReport, PromptUsageSummary,
    RunPromptForEventRequest,
};

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

    mod command_workflows;

    mod polling_and_replay;

    mod chat_and_controls;

    mod artifact_workflows;
}
