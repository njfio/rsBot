use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tau_core::{current_unix_timestamp_ms, write_text_atomic};

#[cfg(test)]
use serde_json::Value;

pub const APPROVALS_USAGE: &str =
    "usage: /approvals <list|approve|reject> [--json] [--status <pending|approved|rejected|expired|consumed>] [request_id] [reason]";

const APPROVAL_POLICY_SCHEMA_VERSION: u32 = 1;
const APPROVAL_STORE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_APPROVAL_TIMEOUT_SECONDS: u64 = 900;
const APPROVAL_POLICY_PATH_ENV: &str = "TAU_APPROVAL_POLICY_PATH";
const APPROVAL_STORE_PATH_ENV: &str = "TAU_APPROVAL_STORE_PATH";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Enumerates supported `ApprovalRequestStatus` values.
pub enum ApprovalRequestStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Consumed,
}

impl ApprovalRequestStatus {
    fn as_str(self) -> &'static str {
        match self {
            ApprovalRequestStatus::Pending => "pending",
            ApprovalRequestStatus::Approved => "approved",
            ApprovalRequestStatus::Rejected => "rejected",
            ApprovalRequestStatus::Expired => "expired",
            ApprovalRequestStatus::Consumed => "consumed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct ApprovalPolicyRule {
    id: String,
    action: String,
    #[serde(default)]
    command_prefixes: Vec<String>,
    #[serde(default)]
    path_prefixes: Vec<String>,
    #[serde(default)]
    command_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct ApprovalPolicyFile {
    #[serde(default = "approval_policy_schema_version")]
    schema_version: u32,
    #[serde(default)]
    enabled: bool,
    #[serde(default = "approval_policy_default_strict_mode")]
    strict_mode: bool,
    #[serde(default = "approval_policy_default_timeout_seconds")]
    timeout_seconds: u64,
    #[serde(default)]
    rules: Vec<ApprovalPolicyRule>,
}

impl Default for ApprovalPolicyFile {
    fn default() -> Self {
        Self {
            schema_version: APPROVAL_POLICY_SCHEMA_VERSION,
            enabled: false,
            strict_mode: true,
            timeout_seconds: DEFAULT_APPROVAL_TIMEOUT_SECONDS,
            rules: Vec::new(),
        }
    }
}

fn approval_policy_schema_version() -> u32 {
    APPROVAL_POLICY_SCHEMA_VERSION
}

fn approval_policy_default_strict_mode() -> bool {
    true
}

fn approval_policy_default_timeout_seconds() -> u64 {
    DEFAULT_APPROVAL_TIMEOUT_SECONDS
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct ApprovalRequestRecord {
    id: String,
    rule_id: String,
    action_kind: String,
    action_summary: String,
    fingerprint: String,
    status: ApprovalRequestStatus,
    created_at_ms: u64,
    expires_at_ms: u64,
    decision_at_ms: Option<u64>,
    decision_reason: Option<String>,
    decision_actor: Option<String>,
    consumed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
struct ApprovalStoreFile {
    #[serde(default = "approval_store_schema_version")]
    schema_version: u32,
    #[serde(default = "approval_store_default_next_request_id")]
    next_request_id: u64,
    #[serde(default)]
    requests: Vec<ApprovalRequestRecord>,
}

impl Default for ApprovalStoreFile {
    fn default() -> Self {
        Self {
            schema_version: APPROVAL_STORE_SCHEMA_VERSION,
            next_request_id: 1,
            requests: Vec::new(),
        }
    }
}

fn approval_store_schema_version() -> u32 {
    APPROVAL_STORE_SCHEMA_VERSION
}

fn approval_store_default_next_request_id() -> u64 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Enumerates supported `ApprovalAction` values.
pub enum ApprovalAction {
    ToolBash {
        command: String,
        cwd: Option<String>,
    },
    ToolWrite {
        path: String,
        content_bytes: usize,
    },
    ToolEdit {
        path: String,
        find: String,
        replace_bytes: usize,
    },
    Command {
        name: String,
        args: String,
    },
}

impl ApprovalAction {
    fn action_kind(&self) -> &'static str {
        match self {
            ApprovalAction::ToolBash { .. } => "tool:bash",
            ApprovalAction::ToolWrite { .. } => "tool:write",
            ApprovalAction::ToolEdit { .. } => "tool:edit",
            ApprovalAction::Command { .. } => "command",
        }
    }

    fn summary(&self) -> String {
        match self {
            ApprovalAction::ToolBash { command, cwd } => format!(
                "bash command='{}' cwd={}",
                command,
                cwd.as_deref().unwrap_or("default")
            ),
            ApprovalAction::ToolWrite {
                path,
                content_bytes,
            } => {
                format!("write path={} bytes={}", path, content_bytes)
            }
            ApprovalAction::ToolEdit {
                path,
                find,
                replace_bytes,
            } => format!(
                "edit path={} find='{}' replace_bytes={}",
                path, find, replace_bytes
            ),
            ApprovalAction::Command { name, args } => {
                format!("command name={} args='{}'", name, args)
            }
        }
    }

    fn command_name(&self) -> Option<&str> {
        match self {
            ApprovalAction::Command { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }

    fn path_value(&self) -> Option<&str> {
        match self {
            ApprovalAction::ToolWrite { path, .. } | ApprovalAction::ToolEdit { path, .. } => {
                Some(path.as_str())
            }
            _ => None,
        }
    }

    fn command_value(&self) -> Option<&str> {
        match self {
            ApprovalAction::ToolBash { command, .. } => Some(command.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Enumerates supported `ApprovalGateResult` values.
pub enum ApprovalGateResult {
    Allowed,
    Denied {
        request_id: String,
        rule_id: String,
        reason_code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ApprovalsCommand {
    List {
        format: ApprovalOutputFormat,
        status_filter: Option<ApprovalRequestStatus>,
    },
    Approve {
        request_id: String,
        reason: Option<String>,
    },
    Reject {
        request_id: String,
        reason: Option<String>,
    },
}

pub fn execute_approvals_command(command_args: &str) -> String {
    let policy_path = default_approval_policy_path();
    let state_path = default_approval_store_path();
    execute_approvals_command_with_paths_internal(command_args, &policy_path, &state_path, None)
}

pub fn execute_approvals_command_with_paths_and_actor(
    command_args: &str,
    policy_path: &Path,
    state_path: &Path,
    decision_actor: Option<&str>,
) -> String {
    execute_approvals_command_with_paths_internal(
        command_args,
        policy_path,
        state_path,
        decision_actor,
    )
}

pub fn evaluate_approval_gate(action: &ApprovalAction) -> Result<ApprovalGateResult> {
    let policy_path = default_approval_policy_path();
    let state_path = default_approval_store_path();
    evaluate_approval_gate_with_paths(action, &policy_path, &state_path)
}

fn evaluate_approval_gate_with_paths(
    action: &ApprovalAction,
    policy_path: &Path,
    state_path: &Path,
) -> Result<ApprovalGateResult> {
    let _guard = approval_store_guard();
    let policy = load_approval_policy(policy_path)?;
    if !policy.enabled {
        return Ok(ApprovalGateResult::Allowed);
    }

    let Some(rule) = match_approval_rule(&policy, action) else {
        return Ok(ApprovalGateResult::Allowed);
    };

    let now_ms = current_unix_timestamp_ms();
    let mut store = load_approval_store(state_path)?;
    let mutated = expire_pending_requests(&mut store, now_ms);
    let fingerprint = approval_fingerprint(action)?;

    if let Some(existing_index) = store.requests.iter().rposition(|request| {
        request.fingerprint == fingerprint
            && request.rule_id == rule.id
            && matches!(
                request.status,
                ApprovalRequestStatus::Pending
                    | ApprovalRequestStatus::Approved
                    | ApprovalRequestStatus::Rejected
                    | ApprovalRequestStatus::Expired
            )
    }) {
        match store.requests[existing_index].status {
            ApprovalRequestStatus::Approved => {
                {
                    let existing = &mut store.requests[existing_index];
                    existing.status = ApprovalRequestStatus::Consumed;
                    existing.consumed_at_ms = Some(now_ms);
                }
                save_approval_store(state_path, &store)?;
                return Ok(ApprovalGateResult::Allowed);
            }
            ApprovalRequestStatus::Pending => {
                let request_id = store.requests[existing_index].id.clone();
                let rule_id = store.requests[existing_index].rule_id.clone();
                if mutated {
                    save_approval_store(state_path, &store)?;
                }
                return Ok(ApprovalGateResult::Denied {
                    request_id,
                    rule_id,
                    reason_code: "approval_pending".to_string(),
                    message: "approval request is pending".to_string(),
                });
            }
            ApprovalRequestStatus::Rejected => {
                let request_id = store.requests[existing_index].id.clone();
                let rule_id = store.requests[existing_index].rule_id.clone();
                let message = store.requests[existing_index]
                    .decision_reason
                    .clone()
                    .unwrap_or_else(|| "approval request was rejected".to_string());
                if mutated {
                    save_approval_store(state_path, &store)?;
                }
                return Ok(ApprovalGateResult::Denied {
                    request_id,
                    rule_id,
                    reason_code: "approval_rejected".to_string(),
                    message,
                });
            }
            ApprovalRequestStatus::Expired => {
                if !policy.strict_mode {
                    if mutated {
                        save_approval_store(state_path, &store)?;
                    }
                    return Ok(ApprovalGateResult::Allowed);
                }
            }
            ApprovalRequestStatus::Consumed => {}
        }
    }

    let request_id = format!("req-{}", store.next_request_id);
    store.next_request_id = store.next_request_id.saturating_add(1);
    let timeout_ms = policy.timeout_seconds.saturating_mul(1_000);
    let request = ApprovalRequestRecord {
        id: request_id.clone(),
        rule_id: rule.id.clone(),
        action_kind: action.action_kind().to_string(),
        action_summary: action.summary(),
        fingerprint,
        status: ApprovalRequestStatus::Pending,
        created_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(timeout_ms),
        decision_at_ms: None,
        decision_reason: None,
        decision_actor: None,
        consumed_at_ms: None,
    };
    store.requests.push(request);
    save_approval_store(state_path, &store)?;

    Ok(ApprovalGateResult::Denied {
        request_id,
        rule_id: rule.id.clone(),
        reason_code: "approval_required".to_string(),
        message: "approval required before action execution".to_string(),
    })
}

#[cfg(test)]
fn execute_approvals_command_with_paths(
    command_args: &str,
    policy_path: &Path,
    state_path: &Path,
) -> String {
    execute_approvals_command_with_paths_internal(command_args, policy_path, state_path, None)
}

fn execute_approvals_command_with_paths_internal(
    command_args: &str,
    policy_path: &Path,
    state_path: &Path,
    decision_actor: Option<&str>,
) -> String {
    let command = match parse_approvals_command(command_args) {
        Ok(command) => command,
        Err(_) => return APPROVALS_USAGE.to_string(),
    };

    match command {
        ApprovalsCommand::List {
            format,
            status_filter,
        } => match execute_approvals_list(policy_path, state_path, status_filter, format) {
            Ok(output) => output,
            Err(error) => format!("approvals error: {error}"),
        },
        ApprovalsCommand::Approve { request_id, reason } => {
            match update_approval_decision(
                state_path,
                request_id.as_str(),
                ApprovalRequestStatus::Approved,
                reason,
                decision_actor,
            ) {
                Ok(output) => output,
                Err(error) => format!("approvals error: {error}"),
            }
        }
        ApprovalsCommand::Reject { request_id, reason } => {
            match update_approval_decision(
                state_path,
                request_id.as_str(),
                ApprovalRequestStatus::Rejected,
                reason,
                decision_actor,
            ) {
                Ok(output) => output,
                Err(error) => format!("approvals error: {error}"),
            }
        }
    }
}

fn execute_approvals_list(
    policy_path: &Path,
    state_path: &Path,
    status_filter: Option<ApprovalRequestStatus>,
    format: ApprovalOutputFormat,
) -> Result<String> {
    let _guard = approval_store_guard();
    let policy = load_approval_policy(policy_path)?;
    let mut store = load_approval_store(state_path)?;
    let now_ms = current_unix_timestamp_ms();
    if expire_pending_requests(&mut store, now_ms) {
        save_approval_store(state_path, &store)?;
    }

    Ok(match format {
        ApprovalOutputFormat::Text => {
            render_approvals_list_text(&policy, &store, status_filter, policy_path, state_path)
        }
        ApprovalOutputFormat::Json => {
            render_approvals_list_json(&policy, &store, status_filter, policy_path, state_path)
        }
    })
}

fn update_approval_decision(
    state_path: &Path,
    request_id: &str,
    status: ApprovalRequestStatus,
    reason: Option<String>,
    decision_actor: Option<&str>,
) -> Result<String> {
    let _guard = approval_store_guard();
    let mut store = load_approval_store(state_path)?;
    let now_ms = current_unix_timestamp_ms();
    if expire_pending_requests(&mut store, now_ms) {
        save_approval_store(state_path, &store)?;
    }

    let Some(record_index) = store
        .requests
        .iter()
        .position(|record| record.id == request_id)
    else {
        bail!("approval request '{}' not found", request_id);
    };

    if store.requests[record_index].status != ApprovalRequestStatus::Pending {
        bail!(
            "approval request '{}' is not pending (status={})",
            request_id,
            store.requests[record_index].status.as_str()
        );
    }

    if !(status == ApprovalRequestStatus::Approved || status == ApprovalRequestStatus::Rejected) {
        bail!("invalid approval decision status '{}'", status.as_str());
    }

    {
        let record = &mut store.requests[record_index];
        record.status = status;
        record.decision_at_ms = Some(now_ms);
        record.decision_actor = Some(normalize_decision_actor(decision_actor));
        record.decision_reason = reason.filter(|value| !value.trim().is_empty());
    }
    let decision_reason = store.requests[record_index]
        .decision_reason
        .clone()
        .unwrap_or_else(|| "none".to_string());
    let decision_actor = store.requests[record_index]
        .decision_actor
        .clone()
        .unwrap_or_else(|| "none".to_string());
    save_approval_store(state_path, &store)?;

    Ok(format!(
        "approvals decision: request_id={} status={} reason={} decision_actor={}",
        request_id,
        status.as_str(),
        decision_reason,
        decision_actor
    ))
}

fn parse_approvals_command(command_args: &str) -> Result<ApprovalsCommand> {
    let tokens = command_args
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("{APPROVALS_USAGE}");
    }

    match tokens[0] {
        "list" => parse_approvals_list_args(&tokens[1..]),
        "approve" => parse_approvals_decision_args(&tokens, true),
        "reject" => parse_approvals_decision_args(&tokens, false),
        _ => bail!("{APPROVALS_USAGE}"),
    }
}

fn parse_approvals_list_args(tokens: &[&str]) -> Result<ApprovalsCommand> {
    let mut format = ApprovalOutputFormat::Text;
    let mut status_filter = None;
    let mut index = 0usize;
    while index < tokens.len() {
        match tokens[index] {
            "--json" => {
                format = ApprovalOutputFormat::Json;
                index += 1;
            }
            "--status" => {
                index += 1;
                let Some(raw_status) = tokens.get(index) else {
                    bail!("{APPROVALS_USAGE}");
                };
                status_filter = Some(parse_approval_status(raw_status)?);
                index += 1;
            }
            _ => bail!("{APPROVALS_USAGE}"),
        }
    }
    Ok(ApprovalsCommand::List {
        format,
        status_filter,
    })
}

fn parse_approvals_decision_args(tokens: &[&str], approve: bool) -> Result<ApprovalsCommand> {
    if tokens.len() < 2 {
        bail!("{APPROVALS_USAGE}");
    }
    let request_id = tokens[1].to_string();
    let reason = if tokens.len() > 2 {
        Some(tokens[2..].join(" "))
    } else {
        None
    };
    Ok(if approve {
        ApprovalsCommand::Approve { request_id, reason }
    } else {
        ApprovalsCommand::Reject { request_id, reason }
    })
}

fn parse_approval_status(raw: &str) -> Result<ApprovalRequestStatus> {
    match raw {
        "pending" => Ok(ApprovalRequestStatus::Pending),
        "approved" => Ok(ApprovalRequestStatus::Approved),
        "rejected" => Ok(ApprovalRequestStatus::Rejected),
        "expired" => Ok(ApprovalRequestStatus::Expired),
        "consumed" => Ok(ApprovalRequestStatus::Consumed),
        _ => bail!(
            "invalid approvals status '{}': expected pending|approved|rejected|expired|consumed",
            raw
        ),
    }
}

fn default_approval_policy_path() -> PathBuf {
    std::env::var(APPROVAL_POLICY_PATH_ENV)
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tau/approvals/policy.json"))
}

fn default_approval_store_path() -> PathBuf {
    std::env::var(APPROVAL_STORE_PATH_ENV)
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".tau/approvals/requests.json"))
}

pub fn approval_paths_for_state_dir(state_dir: &Path) -> (PathBuf, PathBuf) {
    let state_name = state_dir.file_name().and_then(|value| value.to_str());
    let tau_root = match state_name {
        Some("github")
        | Some("slack")
        | Some("events")
        | Some("channel-store")
        | Some("multi-channel") => state_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(state_dir),
        _ => state_dir,
    };
    let approvals_root = tau_root.join("approvals");
    (
        approvals_root.join("policy.json"),
        approvals_root.join("requests.json"),
    )
}

fn normalize_decision_actor(decision_actor: Option<&str>) -> String {
    decision_actor
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local-command")
        .to_string()
}

fn approval_store_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn load_approval_policy(path: &Path) -> Result<ApprovalPolicyFile> {
    if !path.exists() {
        return Ok(ApprovalPolicyFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read approval policy file {}", path.display()))?;
    let parsed = serde_json::from_str::<ApprovalPolicyFile>(&raw)
        .with_context(|| format!("failed to parse approval policy file {}", path.display()))?;
    validate_approval_policy(&parsed)?;
    Ok(parsed)
}

fn validate_approval_policy(policy: &ApprovalPolicyFile) -> Result<()> {
    if policy.schema_version != APPROVAL_POLICY_SCHEMA_VERSION {
        bail!(
            "unsupported approval policy schema version {} (expected {})",
            policy.schema_version,
            APPROVAL_POLICY_SCHEMA_VERSION
        );
    }
    if policy.timeout_seconds == 0 {
        bail!("approval policy timeout_seconds must be greater than 0");
    }

    let mut rule_ids = HashSet::new();
    for rule in &policy.rules {
        if rule.id.trim().is_empty() {
            bail!("approval policy rule id must not be empty");
        }
        if !rule_ids.insert(rule.id.clone()) {
            bail!("approval policy rule ids must be unique: '{}'", rule.id);
        }
        if !matches!(
            rule.action.as_str(),
            "tool:bash" | "tool:write" | "tool:edit" | "command"
        ) {
            bail!("unsupported approval policy rule action '{}'", rule.action);
        }
    }
    Ok(())
}

fn load_approval_store(path: &Path) -> Result<ApprovalStoreFile> {
    if !path.exists() {
        return Ok(ApprovalStoreFile::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read approval request store {}", path.display()))?;
    let parsed = serde_json::from_str::<ApprovalStoreFile>(&raw)
        .with_context(|| format!("failed to parse approval request store {}", path.display()))?;
    if parsed.schema_version != APPROVAL_STORE_SCHEMA_VERSION {
        bail!(
            "unsupported approval request store schema version {} (expected {})",
            parsed.schema_version,
            APPROVAL_STORE_SCHEMA_VERSION
        );
    }
    Ok(parsed)
}

fn save_approval_store(path: &Path, store: &ApprovalStoreFile) -> Result<()> {
    let serialized =
        serde_json::to_string_pretty(store).context("failed to serialize approval store")?;
    write_text_atomic(path, &serialized)
        .with_context(|| format!("failed to write approval store {}", path.display()))
}

fn expire_pending_requests(store: &mut ApprovalStoreFile, now_ms: u64) -> bool {
    let mut mutated = false;
    for request in &mut store.requests {
        if request.status == ApprovalRequestStatus::Pending && now_ms > request.expires_at_ms {
            request.status = ApprovalRequestStatus::Expired;
            request.decision_at_ms = Some(now_ms);
            request.decision_reason = Some("approval_timeout".to_string());
            request.decision_actor = Some("system-timeout".to_string());
            mutated = true;
        }
    }
    mutated
}

fn approval_fingerprint(action: &ApprovalAction) -> Result<String> {
    let serialized = serde_json::to_vec(action).context("failed to serialize approval action")?;
    let digest = Sha256::digest(serialized);
    Ok(format!("{digest:x}"))
}

fn match_approval_rule<'a>(
    policy: &'a ApprovalPolicyFile,
    action: &ApprovalAction,
) -> Option<&'a ApprovalPolicyRule> {
    policy.rules.iter().find(|rule| {
        if rule.action != action.action_kind() {
            return false;
        }

        if let Some(command) = action.command_value() {
            return rule.command_prefixes.is_empty()
                || rule
                    .command_prefixes
                    .iter()
                    .any(|prefix| command.trim_start().starts_with(prefix));
        }

        if let Some(path) = action.path_value() {
            if rule.path_prefixes.is_empty() {
                return true;
            }
            let normalized = normalize_path_like(path);
            return rule
                .path_prefixes
                .iter()
                .map(|prefix| normalize_path_like(prefix))
                .any(|prefix| normalized.starts_with(prefix.as_str()));
        }

        if let Some(name) = action.command_name() {
            return rule.command_names.is_empty()
                || rule
                    .command_names
                    .iter()
                    .any(|command_name| command_name == name);
        }

        false
    })
}

fn normalize_path_like(raw: &str) -> String {
    raw.replace('\\', "/")
}

fn render_approvals_list_text(
    policy: &ApprovalPolicyFile,
    store: &ApprovalStoreFile,
    status_filter: Option<ApprovalRequestStatus>,
    policy_path: &Path,
    state_path: &Path,
) -> String {
    let mut requests = store
        .requests
        .iter()
        .filter(|request| {
            status_filter
                .map(|filter| request.status == filter)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    requests.sort_by(|left, right| {
        right
            .created_at_ms
            .cmp(&left.created_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });

    let counts = approval_status_counts(store);
    let mut lines = vec![format!(
        "approvals summary: enabled={} strict_mode={} timeout_seconds={} total={} pending={} approved={} rejected={} expired={} consumed={} policy={} store={}",
        policy.enabled,
        policy.strict_mode,
        policy.timeout_seconds,
        store.requests.len(),
        counts.pending,
        counts.approved,
        counts.rejected,
        counts.expired,
        counts.consumed,
        policy_path.display(),
        state_path.display(),
    )];
    for request in requests {
        lines.push(format!(
            "approval request: id={} status={} action={} rule={} created_at_ms={} expires_at_ms={} summary={} decision_reason={}",
            request.id,
            request.status.as_str(),
            request.action_kind,
            request.rule_id,
            request.created_at_ms,
            request.expires_at_ms,
            request.action_summary,
            request.decision_reason.as_deref().unwrap_or("none")
        ));
    }
    lines.join("\n")
}

fn render_approvals_list_json(
    policy: &ApprovalPolicyFile,
    store: &ApprovalStoreFile,
    status_filter: Option<ApprovalRequestStatus>,
    policy_path: &Path,
    state_path: &Path,
) -> String {
    let requests = store
        .requests
        .iter()
        .filter(|request| {
            status_filter
                .map(|filter| request.status == filter)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    let counts = approval_status_counts(store);

    serde_json::json!({
        "summary": {
            "enabled": policy.enabled,
            "strict_mode": policy.strict_mode,
            "timeout_seconds": policy.timeout_seconds,
            "policy_path": policy_path.display().to_string(),
            "state_path": state_path.display().to_string(),
            "total": store.requests.len(),
            "pending": counts.pending,
            "approved": counts.approved,
            "rejected": counts.rejected,
            "expired": counts.expired,
            "consumed": counts.consumed,
            "status_filter": status_filter.map(ApprovalRequestStatus::as_str),
        },
        "requests": requests,
    })
    .to_string()
}

struct ApprovalStatusCounts {
    pending: usize,
    approved: usize,
    rejected: usize,
    expired: usize,
    consumed: usize,
}

fn approval_status_counts(store: &ApprovalStoreFile) -> ApprovalStatusCounts {
    let mut counts = ApprovalStatusCounts {
        pending: 0,
        approved: 0,
        rejected: 0,
        expired: 0,
        consumed: 0,
    };
    for request in &store.requests {
        match request.status {
            ApprovalRequestStatus::Pending => counts.pending = counts.pending.saturating_add(1),
            ApprovalRequestStatus::Approved => counts.approved = counts.approved.saturating_add(1),
            ApprovalRequestStatus::Rejected => counts.rejected = counts.rejected.saturating_add(1),
            ApprovalRequestStatus::Expired => counts.expired = counts.expired.saturating_add(1),
            ApprovalRequestStatus::Consumed => counts.consumed = counts.consumed.saturating_add(1),
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn policy_path(root: &Path) -> PathBuf {
        root.join("policy.json")
    }

    fn state_path(root: &Path) -> PathBuf {
        root.join("requests.json")
    }

    fn write_policy(path: &Path, payload: &Value) {
        std::fs::write(path, format!("{payload}\n")).expect("write policy");
    }

    #[test]
    fn unit_parse_approvals_command_supports_list_approve_reject() {
        let parsed = parse_approvals_command("list --json --status pending").expect("parse list");
        assert_eq!(
            parsed,
            ApprovalsCommand::List {
                format: ApprovalOutputFormat::Json,
                status_filter: Some(ApprovalRequestStatus::Pending)
            }
        );
        assert_eq!(
            parse_approvals_command("approve req-1 looks safe").expect("parse approve"),
            ApprovalsCommand::Approve {
                request_id: "req-1".to_string(),
                reason: Some("looks safe".to_string())
            }
        );
        assert_eq!(
            parse_approvals_command("reject req-2 blocked").expect("parse reject"),
            ApprovalsCommand::Reject {
                request_id: "req-2".to_string(),
                reason: Some("blocked".to_string())
            }
        );
    }

    #[test]
    fn functional_execute_approvals_command_lists_and_updates_decisions() {
        let temp = tempdir().expect("tempdir");
        let policy_path = policy_path(temp.path());
        let state_path = state_path(temp.path());
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "enabled": true,
                "strict_mode": true,
                "timeout_seconds": 3600,
                "rules": [
                    {
                        "id": "danger-command",
                        "action": "command",
                        "command_names": ["/danger"]
                    }
                ]
            }),
        );

        let denied = evaluate_approval_gate_with_paths(
            &ApprovalAction::Command {
                name: "/danger".to_string(),
                args: "now".to_string(),
            },
            &policy_path,
            &state_path,
        )
        .expect("evaluate gate");
        let request_id = match denied {
            ApprovalGateResult::Denied { request_id, .. } => request_id,
            ApprovalGateResult::Allowed => panic!("expected denial"),
        };

        let list = execute_approvals_command_with_paths("list", &policy_path, &state_path);
        assert!(list.contains("approvals summary:"));
        assert!(list.contains(request_id.as_str()));

        let approve = execute_approvals_command_with_paths(
            format!("approve {} approved", request_id).as_str(),
            &policy_path,
            &state_path,
        );
        assert!(approve.contains("status=approved"));

        let approved_list = execute_approvals_command_with_paths(
            "list --status approved",
            &policy_path,
            &state_path,
        );
        assert!(approved_list.contains("status=approved"));
    }

    #[test]
    fn integration_evaluate_approval_gate_requires_then_allows_after_approval() {
        let temp = tempdir().expect("tempdir");
        let policy_path = policy_path(temp.path());
        let state_path = state_path(temp.path());
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "enabled": true,
                "strict_mode": true,
                "timeout_seconds": 3600,
                "rules": [
                    {
                        "id": "write-sensitive",
                        "action": "tool:write",
                        "path_prefixes": ["/tmp/"]
                    }
                ]
            }),
        );
        let action = ApprovalAction::ToolWrite {
            path: "/tmp/sensitive.txt".to_string(),
            content_bytes: 4,
        };
        let first = evaluate_approval_gate_with_paths(&action, &policy_path, &state_path)
            .expect("first gate eval");
        let request_id = match first {
            ApprovalGateResult::Denied { request_id, .. } => request_id,
            ApprovalGateResult::Allowed => panic!("expected denied"),
        };

        execute_approvals_command_with_paths(
            format!("approve {}", request_id).as_str(),
            &policy_path,
            &state_path,
        );

        let second = evaluate_approval_gate_with_paths(&action, &policy_path, &state_path)
            .expect("second gate eval");
        assert_eq!(second, ApprovalGateResult::Allowed);

        let third = evaluate_approval_gate_with_paths(&action, &policy_path, &state_path)
            .expect("third gate eval");
        assert!(matches!(third, ApprovalGateResult::Denied { .. }));
    }

    #[test]
    fn functional_execute_approvals_command_with_actor_persists_decision_actor() {
        let temp = tempdir().expect("tempdir");
        let policy_path = policy_path(temp.path());
        let state_path = state_path(temp.path());
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "enabled": true,
                "strict_mode": true,
                "timeout_seconds": 3600,
                "rules": [
                    {
                        "id": "command-review",
                        "action": "command",
                        "command_names": ["/danger"]
                    }
                ]
            }),
        );

        let denied = evaluate_approval_gate_with_paths(
            &ApprovalAction::Command {
                name: "/danger".to_string(),
                args: "now".to_string(),
            },
            &policy_path,
            &state_path,
        )
        .expect("evaluate gate");
        let request_id = match denied {
            ApprovalGateResult::Denied { request_id, .. } => request_id,
            ApprovalGateResult::Allowed => panic!("expected denied"),
        };

        let output = execute_approvals_command_with_paths_and_actor(
            format!("approve {} approved", request_id).as_str(),
            &policy_path,
            &state_path,
            Some("telegram:ops-room:operator-1"),
        );
        assert!(output.contains("decision_actor=telegram:ops-room:operator-1"));

        let store = load_approval_store(&state_path).expect("load store");
        let request = store
            .requests
            .iter()
            .find(|entry| entry.id == request_id)
            .expect("request");
        assert_eq!(request.status, ApprovalRequestStatus::Approved);
        assert_eq!(
            request.decision_actor.as_deref(),
            Some("telegram:ops-room:operator-1")
        );
    }

    #[test]
    fn unit_approval_paths_for_state_dir_supports_transport_roots() {
        let transport_state = PathBuf::from(".tau/multi-channel");
        let (policy_path, store_path) = approval_paths_for_state_dir(&transport_state);
        assert_eq!(policy_path, PathBuf::from(".tau/approvals/policy.json"));
        assert_eq!(store_path, PathBuf::from(".tau/approvals/requests.json"));

        let generic_state = PathBuf::from("runtime-state");
        let (policy_path, store_path) = approval_paths_for_state_dir(&generic_state);
        assert_eq!(
            policy_path,
            PathBuf::from("runtime-state/approvals/policy.json")
        );
        assert_eq!(
            store_path,
            PathBuf::from("runtime-state/approvals/requests.json")
        );
    }

    #[test]
    fn regression_strict_timeout_defaults_to_deny_and_requeues() {
        let temp = tempdir().expect("tempdir");
        let policy_path = policy_path(temp.path());
        let state_path = state_path(temp.path());
        write_policy(
            &policy_path,
            &serde_json::json!({
                "schema_version": 1,
                "enabled": true,
                "strict_mode": true,
                "timeout_seconds": 1,
                "rules": [
                    {
                        "id": "bash-sensitive",
                        "action": "tool:bash",
                        "command_prefixes": ["deploy "]
                    }
                ]
            }),
        );

        let mut store = ApprovalStoreFile::default();
        store.requests.push(ApprovalRequestRecord {
            id: "req-1".to_string(),
            rule_id: "bash-sensitive".to_string(),
            action_kind: "tool:bash".to_string(),
            action_summary: "bash command='deploy now' cwd=default".to_string(),
            fingerprint: approval_fingerprint(&ApprovalAction::ToolBash {
                command: "deploy now".to_string(),
                cwd: None,
            })
            .expect("fingerprint"),
            status: ApprovalRequestStatus::Pending,
            created_at_ms: 1,
            expires_at_ms: 2,
            decision_at_ms: None,
            decision_reason: None,
            decision_actor: None,
            consumed_at_ms: None,
        });
        store.next_request_id = 2;
        save_approval_store(&state_path, &store).expect("seed store");

        let decision = evaluate_approval_gate_with_paths(
            &ApprovalAction::ToolBash {
                command: "deploy now".to_string(),
                cwd: None,
            },
            &policy_path,
            &state_path,
        )
        .expect("evaluate gate");
        assert!(matches!(
            decision,
            ApprovalGateResult::Denied {
                reason_code,
                ..
            } if reason_code == "approval_required"
        ));

        let updated = load_approval_store(&state_path).expect("load updated store");
        assert!(updated
            .requests
            .iter()
            .any(|request| request.status == ApprovalRequestStatus::Expired));
        assert!(updated
            .requests
            .iter()
            .any(|request| request.status == ApprovalRequestStatus::Pending));
    }

    #[test]
    fn regression_parse_approvals_command_rejects_invalid_forms() {
        let error = parse_approvals_command("").expect_err("empty should fail");
        assert!(error.to_string().contains(APPROVALS_USAGE));
        let error = parse_approvals_command("list --status maybe").expect_err("invalid status");
        assert!(error.to_string().contains("invalid approvals status"));
        let error = parse_approvals_command("approve").expect_err("missing id");
        assert!(error.to_string().contains(APPROVALS_USAGE));
    }
}
