use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentTool, ToolExecutionResult};
use tau_ai::{Message, ToolDefinition};
use tau_extensions::{
    evaluate_extension_policy_override, execute_extension_registered_tool, ExtensionRegisteredTool,
};
use tokio::{process::Command, time::timeout};

use tau_access::{
    authorize_tool_for_principal, authorize_tool_for_principal_with_policy_path,
    evaluate_approval_gate, resolve_local_principal, ApprovalAction, ApprovalGateResult,
    RbacDecision,
};
use tau_session::{
    compute_session_entry_depths, search_session_entries, session_message_preview,
    session_message_role, SessionStore,
};

const SAFE_BASH_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMPDIR", "TMP", "TEMP",
    "TZ",
];

const BALANCED_COMMAND_ALLOWLIST: &[&str] = &[
    "awk", "cargo", "cat", "cp", "cut", "du", "echo", "env", "fd", "find", "git", "grep", "head",
    "ls", "mkdir", "mv", "printf", "pwd", "rg", "rm", "rustc", "rustup", "sed", "sleep", "sort",
    "stat", "tail", "touch", "tr", "uniq", "wc",
];

const STRICT_COMMAND_ALLOWLIST: &[&str] = &[
    "awk", "cat", "cut", "du", "echo", "env", "fd", "find", "grep", "head", "ls", "printf", "pwd",
    "rg", "sed", "sort", "stat", "tail", "tr", "uniq", "wc",
];

const SESSION_LIST_DEFAULT_LIMIT: usize = 64;
const SESSION_LIST_MAX_LIMIT: usize = 256;
const SESSION_HISTORY_DEFAULT_LIMIT: usize = 40;
const SESSION_HISTORY_MAX_LIMIT: usize = 200;
const SESSION_SEND_MAX_MESSAGE_CHARS: usize = 8_000;
const SESSION_SEARCH_TOOL_DEFAULT_LIMIT: usize = 50;
const SESSION_SEARCH_TOOL_MAX_LIMIT: usize = 200;
const SESSION_SEARCH_SCAN_DEFAULT_LIMIT: usize = 64;
const SESSION_STATS_SCAN_DEFAULT_LIMIT: usize = 64;
const SESSION_STATS_SCAN_MAX_LIMIT: usize = 256;
const SESSION_SCAN_MAX_DEPTH: usize = 8;
const SESSION_SCAN_MAX_DIRECTORIES: usize = 2_000;

#[derive(Debug, Clone, Serialize)]
struct SessionInventoryEntry {
    path: String,
    entries: usize,
    head_id: Option<u64>,
    newest_role: String,
    newest_preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionHistoryEntry {
    id: u64,
    parent_id: Option<u64>,
    role: String,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionSearchToolMatch {
    path: String,
    id: u64,
    parent_id: Option<u64>,
    role: String,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
struct SessionStatsToolRow {
    path: String,
    entries: usize,
    branch_tips: usize,
    roots: usize,
    max_depth: usize,
    latest_head: Option<u64>,
    latest_depth: Option<usize>,
    role_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct SessionStatsComputed {
    entries: usize,
    branch_tips: usize,
    roots: usize,
    max_depth: usize,
    latest_head: Option<u64>,
    latest_depth: Option<usize>,
    role_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `BashCommandProfile` values.
pub enum BashCommandProfile {
    Permissive,
    Balanced,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `ToolPolicyPreset` values.
pub enum ToolPolicyPreset {
    Permissive,
    Balanced,
    Strict,
    Hardened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Enumerates supported `OsSandboxMode` values.
pub enum OsSandboxMode {
    Off,
    Auto,
    Force,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BashSandboxSpec {
    program: String,
    args: Vec<String>,
    sandboxed: bool,
}

#[derive(Debug, Clone)]
/// Public struct `ToolPolicy` used across Tau components.
pub struct ToolPolicy {
    pub allowed_roots: Vec<PathBuf>,
    pub policy_preset: ToolPolicyPreset,
    pub max_file_read_bytes: usize,
    pub max_file_write_bytes: usize,
    pub max_command_output_bytes: usize,
    pub bash_timeout_ms: u64,
    pub max_command_length: usize,
    pub allow_command_newlines: bool,
    pub bash_profile: BashCommandProfile,
    pub allowed_commands: Vec<String>,
    pub os_sandbox_mode: OsSandboxMode,
    pub os_sandbox_command: Vec<String>,
    pub enforce_regular_files: bool,
    pub bash_dry_run: bool,
    pub tool_policy_trace: bool,
    pub extension_policy_override_root: Option<PathBuf>,
    pub rbac_principal: Option<String>,
    pub rbac_policy_path: Option<PathBuf>,
}

impl ToolPolicy {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        let mut policy = Self {
            allowed_roots,
            policy_preset: ToolPolicyPreset::Balanced,
            max_file_read_bytes: 1_000_000,
            max_file_write_bytes: 1_000_000,
            max_command_output_bytes: 16_000,
            bash_timeout_ms: 120_000,
            max_command_length: 4_096,
            allow_command_newlines: false,
            bash_profile: BashCommandProfile::Balanced,
            allowed_commands: BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            os_sandbox_mode: OsSandboxMode::Off,
            os_sandbox_command: Vec::new(),
            enforce_regular_files: true,
            bash_dry_run: false,
            tool_policy_trace: false,
            extension_policy_override_root: None,
            rbac_principal: None,
            rbac_policy_path: None,
        };
        policy.apply_preset(ToolPolicyPreset::Balanced);
        policy
    }

    pub fn set_bash_profile(&mut self, profile: BashCommandProfile) {
        self.bash_profile = profile;
        self.allowed_commands = match profile {
            BashCommandProfile::Permissive => Vec::new(),
            BashCommandProfile::Balanced => BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            BashCommandProfile::Strict => STRICT_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
        };
    }

    pub fn apply_preset(&mut self, preset: ToolPolicyPreset) {
        self.policy_preset = preset;
        match preset {
            ToolPolicyPreset::Permissive => {
                self.max_file_read_bytes = 2_000_000;
                self.max_file_write_bytes = 2_000_000;
                self.max_command_output_bytes = 32_000;
                self.bash_timeout_ms = 180_000;
                self.max_command_length = 8_192;
                self.allow_command_newlines = true;
                self.set_bash_profile(BashCommandProfile::Permissive);
                self.os_sandbox_mode = OsSandboxMode::Off;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = false;
            }
            ToolPolicyPreset::Balanced => {
                self.max_file_read_bytes = 1_000_000;
                self.max_file_write_bytes = 1_000_000;
                self.max_command_output_bytes = 16_000;
                self.bash_timeout_ms = 120_000;
                self.max_command_length = 4_096;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Balanced);
                self.os_sandbox_mode = OsSandboxMode::Off;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = true;
            }
            ToolPolicyPreset::Strict => {
                self.max_file_read_bytes = 750_000;
                self.max_file_write_bytes = 750_000;
                self.max_command_output_bytes = 8_000;
                self.bash_timeout_ms = 90_000;
                self.max_command_length = 2_048;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Strict);
                self.os_sandbox_mode = OsSandboxMode::Auto;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = true;
            }
            ToolPolicyPreset::Hardened => {
                self.max_file_read_bytes = 500_000;
                self.max_file_write_bytes = 500_000;
                self.max_command_output_bytes = 4_000;
                self.bash_timeout_ms = 60_000;
                self.max_command_length = 1_024;
                self.allow_command_newlines = false;
                self.set_bash_profile(BashCommandProfile::Strict);
                self.os_sandbox_mode = OsSandboxMode::Force;
                self.os_sandbox_command.clear();
                self.enforce_regular_files = true;
            }
        }
    }
}

pub fn tool_policy_preset_name(preset: ToolPolicyPreset) -> &'static str {
    match preset {
        ToolPolicyPreset::Permissive => "permissive",
        ToolPolicyPreset::Balanced => "balanced",
        ToolPolicyPreset::Strict => "strict",
        ToolPolicyPreset::Hardened => "hardened",
    }
}

pub fn register_builtin_tools(agent: &mut Agent, policy: ToolPolicy) {
    let policy = Arc::new(policy);
    agent.register_tool(ReadTool::new(policy.clone()));
    agent.register_tool(WriteTool::new(policy.clone()));
    agent.register_tool(EditTool::new(policy.clone()));
    agent.register_tool(SessionsListTool::new(policy.clone()));
    agent.register_tool(SessionsHistoryTool::new(policy.clone()));
    agent.register_tool(SessionsSearchTool::new(policy.clone()));
    agent.register_tool(SessionsStatsTool::new(policy.clone()));
    agent.register_tool(SessionsSendTool::new(policy.clone()));
    agent.register_tool(BashTool::new(policy));
}

pub fn register_extension_tools(agent: &mut Agent, tools: &[ExtensionRegisteredTool]) {
    for tool in tools {
        agent.register_tool(ExtensionProcessTool::new(tool.clone()));
    }
}

struct ExtensionProcessTool {
    registration: ExtensionRegisteredTool,
}

impl ExtensionProcessTool {
    fn new(registration: ExtensionRegisteredTool) -> Self {
        Self { registration }
    }
}

#[async_trait]
impl AgentTool for ExtensionProcessTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.registration.name.clone(),
            description: self.registration.description.clone(),
            parameters: self.registration.parameters.clone(),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        match execute_extension_registered_tool(&self.registration, &arguments) {
            Ok(result) => ToolExecutionResult {
                content: result.content,
                is_error: result.is_error,
            },
            Err(error) => ToolExecutionResult::error(json!({
                "tool": self.registration.name,
                "extension_id": self.registration.extension_id,
                "extension_version": self.registration.extension_version,
                "manifest": self.registration.manifest_path.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `ReadTool` used across Tau components.
pub struct ReadTool {
    policy: Arc<ToolPolicy>,
}

impl ReadTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for ReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read".to_string(),
            description: "Read a UTF-8 text file from disk".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path to read" }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };
        if let Err(error) =
            validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
        {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let metadata = match tokio::fs::metadata(&resolved).await {
            Ok(metadata) => metadata,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error.to_string(),
                }))
            }
        };

        if metadata.len() as usize > self.policy.max_file_read_bytes {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": format!(
                    "file is too large ({} bytes), limit is {} bytes",
                    metadata.len(),
                    self.policy.max_file_read_bytes
                ),
            }));
        }

        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => ToolExecutionResult::ok(json!({
                "path": resolved.display().to_string(),
                "content": content,
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `WriteTool` used across Tau components.
pub struct WriteTool {
    policy: Arc<ToolPolicy>,
}

impl WriteTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for WriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write".to_string(),
            description: "Write UTF-8 text to disk, creating parent directories if needed"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let content = match required_string(&arguments, "content") {
            Ok(content) => content,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let content_size = content.len();
        if content_size > self.policy.max_file_write_bytes {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": format!(
                    "content is too large ({} bytes), limit is {} bytes",
                    content_size,
                    self.policy.max_file_write_bytes
                ),
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "write",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "path": resolved.display().to_string(),
                "content_bytes": content_size,
            }),
        ) {
            return rbac_result;
        }

        if let Some(approval_result) = evaluate_tool_approval_gate(ApprovalAction::ToolWrite {
            path: resolved.display().to_string(),
            content_bytes: content_size,
        }) {
            return approval_result;
        }

        if let Some(parent) = resolved.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(error) = tokio::fs::create_dir_all(parent).await {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": format!("failed to create parent directory: {error}"),
                    }));
                }
            }
        }

        match tokio::fs::write(&resolved, content.as_bytes()).await {
            Ok(()) => ToolExecutionResult::ok(json!({
                "path": resolved.display().to_string(),
                "bytes_written": content.len(),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
}

/// Public struct `EditTool` used across Tau components.
pub struct EditTool {
    policy: Arc<ToolPolicy>,
}

impl EditTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for EditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit".to_string(),
            description: "Edit a file by replacing an existing string".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "find": { "type": "string" },
                    "replace": { "type": "string" },
                    "all": { "type": "boolean", "default": false }
                },
                "required": ["path", "find", "replace"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let find = match required_string(&arguments, "find") {
            Ok(find) => find,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let replace = match required_string(&arguments, "replace") {
            Ok(replace) => replace,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        if find.is_empty() {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": "'find' must not be empty",
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Edit) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };
        if let Err(error) =
            validate_file_target(&resolved, PathMode::Edit, self.policy.enforce_regular_files)
        {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "edit",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "path": resolved.display().to_string(),
                "find": find,
                "replace_bytes": replace.len(),
            }),
        ) {
            return rbac_result;
        }

        if let Some(approval_result) = evaluate_tool_approval_gate(ApprovalAction::ToolEdit {
            path: resolved.display().to_string(),
            find: find.clone(),
            replace_bytes: replace.len(),
        }) {
            return approval_result;
        }

        let replace_all = arguments
            .get("all")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let source = match tokio::fs::read_to_string(&resolved).await {
            Ok(source) => source,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error.to_string(),
                }))
            }
        };

        let occurrences = source.matches(&find).count();
        if occurrences == 0 {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": "target string not found",
            }));
        }

        let updated = if replace_all {
            source.replace(&find, &replace)
        } else {
            source.replacen(&find, &replace, 1)
        };
        if updated.len() > self.policy.max_file_write_bytes {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": format!(
                    "edited content is too large ({} bytes), limit is {} bytes",
                    updated.len(),
                    self.policy.max_file_write_bytes
                ),
            }));
        }

        if let Err(error) = tokio::fs::write(&resolved, updated.as_bytes()).await {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error.to_string(),
            }));
        }

        let replacements = if replace_all { occurrences } else { 1 };
        ToolExecutionResult::ok(json!({
            "path": resolved.display().to_string(),
            "replacements": replacements,
        }))
    }
}

/// Public struct `SessionsListTool` used across Tau components.
pub struct SessionsListTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsListTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_list".to_string(),
            description: "List session stores discovered under allowed roots".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum sessions to return (default {}, max {})",
                            SESSION_LIST_DEFAULT_LIMIT,
                            SESSION_LIST_MAX_LIMIT
                        )
                    }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_LIST_DEFAULT_LIMIT,
            SESSION_LIST_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        match collect_session_inventory(&self.policy, limit) {
            Ok((sessions, skipped_invalid)) => ToolExecutionResult::ok(json!({
                "limit": limit,
                "returned": sessions.len(),
                "skipped_invalid": skipped_invalid,
                "sessions": sessions,
            })),
            Err(error) => ToolExecutionResult::error(json!({ "error": error })),
        }
    }
}

/// Public struct `SessionsHistoryTool` used across Tau components.
pub struct SessionsHistoryTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsHistoryTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsHistoryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_history".to_string(),
            description: "Read bounded lineage/history from a session store".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to session JSONL file"
                    },
                    "head_id": {
                        "type": "integer",
                        "description": "Optional lineage head id. Defaults to current head."
                    },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum lineage entries to return from the tail (default {}, max {})",
                            SESSION_HISTORY_DEFAULT_LIMIT,
                            SESSION_HISTORY_MAX_LIMIT
                        )
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_HISTORY_DEFAULT_LIMIT,
            SESSION_HISTORY_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let head_id = match optional_u64(&arguments, "head_id") {
            Ok(head_id) => head_id,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) =
            validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
        {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let selected_head_id = head_id.or_else(|| store.head_id());
        let lineage = match store.lineage_entries(selected_head_id) {
            Ok(entries) => entries,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "head_id": selected_head_id,
                    "error": format!("failed to resolve session lineage: {error}"),
                }))
            }
        };

        let start = lineage.len().saturating_sub(limit);
        let history_entries = lineage[start..]
            .iter()
            .map(|entry| SessionHistoryEntry {
                id: entry.id,
                parent_id: entry.parent_id,
                role: session_message_role(&entry.message),
                preview: session_message_preview(&entry.message),
            })
            .collect::<Vec<_>>();

        ToolExecutionResult::ok(json!({
            "path": resolved.display().to_string(),
            "head_id": selected_head_id,
            "lineage_entries": lineage.len(),
            "returned": history_entries.len(),
            "history": history_entries,
        }))
    }
}

/// Public struct `SessionsSearchTool` used across Tau components.
pub struct SessionsSearchTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsSearchTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_search".to_string(),
            description: "Search message content across session stores under allowed roots"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Case-insensitive search query"
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional path to a specific session JSONL file"
                    },
                    "role": {
                        "type": "string",
                        "description": "Optional role filter",
                        "enum": ["system", "user", "assistant", "tool"]
                    },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum matches to return (default {}, max {})",
                            SESSION_SEARCH_TOOL_DEFAULT_LIMIT,
                            SESSION_SEARCH_TOOL_MAX_LIMIT
                        )
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let query = match required_string(&arguments, "query") {
            Ok(query) => query,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if query.trim().is_empty() {
            return ToolExecutionResult::error(json!({
                "error": "query must not be empty",
            }));
        }
        let role_filter = match optional_session_search_role(&arguments, "role") {
            Ok(role_filter) => role_filter,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_SEARCH_TOOL_DEFAULT_LIMIT,
            SESSION_SEARCH_TOOL_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let requested_path = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(|value| value.to_string());

        if let Some(path) = requested_path {
            let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
                Ok(path) => path,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": path,
                        "error": error,
                    }))
                }
            };
            if let Err(error) =
                validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
            {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error,
                }));
            }

            let store = match SessionStore::load(&resolved) {
                Ok(store) => store,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": format!("failed to load session: {error}"),
                    }))
                }
            };
            let entries_scanned = store.entries().len();
            let (matches, total_matches) =
                search_session_entries(store.entries(), &query, role_filter.as_deref(), limit);
            let results = matches
                .into_iter()
                .map(|item| SessionSearchToolMatch {
                    path: resolved.display().to_string(),
                    id: item.id,
                    parent_id: item.parent_id,
                    role: item.role,
                    preview: item.preview,
                })
                .collect::<Vec<_>>();

            return ToolExecutionResult::ok(json!({
                "query": query,
                "role": role_filter.clone().unwrap_or_else(|| "any".to_string()),
                "path": resolved.display().to_string(),
                "limit": limit,
                "sessions_scanned": 1,
                "entries_scanned": entries_scanned,
                "matches": total_matches,
                "returned": results.len(),
                "skipped_invalid": 0,
                "results": results,
            }));
        }

        let session_paths =
            match discover_session_paths(&self.policy, SESSION_SEARCH_SCAN_DEFAULT_LIMIT) {
                Ok(session_paths) => session_paths,
                Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
            };

        let mut results = Vec::new();
        let mut sessions_scanned = 0usize;
        let mut entries_scanned = 0usize;
        let mut skipped_invalid = 0usize;
        let mut total_matches = 0usize;

        for path in session_paths {
            let store = match SessionStore::load(&path) {
                Ok(store) => store,
                Err(_) => {
                    skipped_invalid += 1;
                    continue;
                }
            };

            sessions_scanned += 1;
            entries_scanned += store.entries().len();
            let (session_matches, session_total_matches) =
                search_session_entries(store.entries(), &query, role_filter.as_deref(), limit);
            total_matches += session_total_matches;
            for item in session_matches {
                if results.len() >= limit {
                    break;
                }
                results.push(SessionSearchToolMatch {
                    path: path.display().to_string(),
                    id: item.id,
                    parent_id: item.parent_id,
                    role: item.role,
                    preview: item.preview,
                });
            }
        }

        ToolExecutionResult::ok(json!({
            "query": query,
            "role": role_filter.unwrap_or_else(|| "any".to_string()),
            "limit": limit,
            "sessions_scanned": sessions_scanned,
            "entries_scanned": entries_scanned,
            "matches": total_matches,
            "returned": results.len(),
            "skipped_invalid": skipped_invalid,
            "results": results,
        }))
    }
}

fn compute_store_stats(store: &SessionStore) -> Result<SessionStatsComputed, String> {
    let entries = store.entries();
    let depths = compute_session_entry_depths(entries)
        .map_err(|error| format!("failed to compute session entry depths: {error}"))?;

    let mut role_counts = BTreeMap::new();
    for entry in entries {
        let role = session_message_role(&entry.message);
        *role_counts.entry(role).or_insert(0) += 1;
    }

    let latest_head = store.head_id();
    let latest_depth = latest_head.and_then(|id| depths.get(&id).copied());

    Ok(SessionStatsComputed {
        entries: entries.len(),
        branch_tips: store.branch_tips().len(),
        roots: entries
            .iter()
            .filter(|entry| entry.parent_id.is_none())
            .count(),
        max_depth: depths.values().copied().max().unwrap_or(0),
        latest_head,
        latest_depth,
        role_counts,
    })
}

/// Public struct `SessionsStatsTool` used across Tau components.
pub struct SessionsStatsTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsStatsTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsStatsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_stats".to_string(),
            description: "Compute session depth/head/role metrics for one or many session stores"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Optional path to a specific session JSONL file"
                    },
                    "limit": {
                        "type": "integer",
                        "description": format!(
                            "Maximum session files to scan in aggregate mode (default {}, max {})",
                            SESSION_STATS_SCAN_DEFAULT_LIMIT,
                            SESSION_STATS_SCAN_MAX_LIMIT
                        )
                    }
                },
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let limit = match optional_usize(
            &arguments,
            "limit",
            SESSION_STATS_SCAN_DEFAULT_LIMIT,
            SESSION_STATS_SCAN_MAX_LIMIT,
        ) {
            Ok(limit) => limit,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let requested_path = arguments
            .get("path")
            .and_then(Value::as_str)
            .map(|value| value.to_string());

        if let Some(path) = requested_path {
            let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Read) {
                Ok(path) => path,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": path,
                        "error": error,
                    }))
                }
            };
            if let Err(error) =
                validate_file_target(&resolved, PathMode::Read, self.policy.enforce_regular_files)
            {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": error,
                }));
            }

            let store = match SessionStore::load(&resolved) {
                Ok(store) => store,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": format!("failed to load session: {error}"),
                    }))
                }
            };
            let stats = match compute_store_stats(&store) {
                Ok(stats) => stats,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "path": resolved.display().to_string(),
                        "error": error,
                    }))
                }
            };

            return ToolExecutionResult::ok(json!({
                "mode": "single",
                "path": resolved.display().to_string(),
                "limit": limit,
                "sessions_scanned": 1,
                "skipped_invalid": 0,
                "entries": stats.entries,
                "branch_tips": stats.branch_tips,
                "roots": stats.roots,
                "max_depth": stats.max_depth,
                "latest_head": stats.latest_head,
                "latest_depth": stats.latest_depth,
                "role_counts": stats.role_counts,
            }));
        }

        let session_paths = match discover_session_paths(&self.policy, limit) {
            Ok(paths) => paths,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };

        let mut sessions = Vec::new();
        let mut skipped_invalid = 0usize;
        let mut total_entries = 0usize;
        let mut total_branch_tips = 0usize;
        let mut total_roots = 0usize;
        let mut total_max_depth = 0usize;
        let mut total_role_counts = BTreeMap::new();

        for path in session_paths {
            let store = match SessionStore::load(&path) {
                Ok(store) => store,
                Err(_) => {
                    skipped_invalid += 1;
                    continue;
                }
            };

            let stats = match compute_store_stats(&store) {
                Ok(stats) => stats,
                Err(_) => {
                    skipped_invalid += 1;
                    continue;
                }
            };

            total_entries += stats.entries;
            total_branch_tips += stats.branch_tips;
            total_roots += stats.roots;
            total_max_depth = total_max_depth.max(stats.max_depth);
            for (role, count) in &stats.role_counts {
                *total_role_counts.entry(role.clone()).or_insert(0) += count;
            }
            sessions.push(SessionStatsToolRow {
                path: path.display().to_string(),
                entries: stats.entries,
                branch_tips: stats.branch_tips,
                roots: stats.roots,
                max_depth: stats.max_depth,
                latest_head: stats.latest_head,
                latest_depth: stats.latest_depth,
                role_counts: stats.role_counts,
            });
        }
        sessions.sort_by(|left, right| left.path.cmp(&right.path));

        ToolExecutionResult::ok(json!({
            "mode": "aggregate",
            "limit": limit,
            "sessions_scanned": sessions.len(),
            "skipped_invalid": skipped_invalid,
            "entries": total_entries,
            "branch_tips": total_branch_tips,
            "roots": total_roots,
            "max_depth": total_max_depth,
            "role_counts": total_role_counts,
            "sessions": sessions,
        }))
    }
}

/// Public struct `SessionsSendTool` used across Tau components.
pub struct SessionsSendTool {
    policy: Arc<ToolPolicy>,
}

impl SessionsSendTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl AgentTool for SessionsSendTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "sessions_send".to_string(),
            description:
                "Append a user handoff message into a target session store under allowed roots"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to target session JSONL file"
                    },
                    "message": {
                        "type": "string",
                        "description": format!(
                            "User handoff message (max {} characters)",
                            SESSION_SEND_MAX_MESSAGE_CHARS
                        )
                    },
                    "parent_id": {
                        "type": "integer",
                        "description": "Optional parent entry id. Defaults to current head."
                    }
                },
                "required": ["path", "message"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let path = match required_string(&arguments, "path") {
            Ok(path) => path,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let message = match required_string(&arguments, "message") {
            Ok(message) => message,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let parent_id = match optional_u64(&arguments, "parent_id") {
            Ok(parent_id) => parent_id,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        if message.trim().is_empty() {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": "message must not be empty",
            }));
        }
        if message.chars().count() > SESSION_SEND_MAX_MESSAGE_CHARS {
            return ToolExecutionResult::error(json!({
                "path": path,
                "error": format!(
                    "message exceeds max length of {} characters",
                    SESSION_SEND_MAX_MESSAGE_CHARS
                ),
            }));
        }

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": path,
                    "error": error,
                }))
            }
        };
        if let Err(error) = validate_file_target(
            &resolved,
            PathMode::Write,
            self.policy.enforce_regular_files,
        ) {
            return ToolExecutionResult::error(json!({
                "path": resolved.display().to_string(),
                "error": error,
            }));
        }

        let mut store = match SessionStore::load(&resolved) {
            Ok(store) => store,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "error": format!("failed to load session: {error}"),
                }))
            }
        };

        let before_entries = store.entries().len();
        let previous_head_id = store.head_id();
        let selected_parent_id = parent_id.or(previous_head_id);
        let handoff_message = Message::user(message.clone());
        let new_head_id = match store.append_messages(selected_parent_id, &[handoff_message]) {
            Ok(head_id) => head_id,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved.display().to_string(),
                    "parent_id": selected_parent_id,
                    "error": format!("failed to append handoff message: {error}"),
                }))
            }
        };
        let after_entries = store.entries().len();

        ToolExecutionResult::ok(json!({
            "path": resolved.display().to_string(),
            "parent_id": selected_parent_id,
            "previous_head_id": previous_head_id,
            "new_head_id": new_head_id,
            "before_entries": before_entries,
            "after_entries": after_entries,
            "appended_entries": after_entries.saturating_sub(before_entries),
            "message_preview": session_message_preview(&Message::user(message)),
        }))
    }
}

/// Public struct `BashTool` used across Tau components.
pub struct BashTool {
    policy: Arc<ToolPolicy>,
}

impl BashTool {
    pub fn new(policy: Arc<ToolPolicy>) -> Self {
        Self { policy }
    }
}

#[derive(Debug, Clone, Serialize)]
struct PolicyTraceStep {
    check: &'static str,
    outcome: &'static str,
    detail: String,
}

fn push_policy_trace(
    trace: &mut Vec<PolicyTraceStep>,
    enabled: bool,
    check: &'static str,
    outcome: &'static str,
    detail: impl Into<String>,
) {
    if !enabled {
        return;
    }
    trace.push(PolicyTraceStep {
        check,
        outcome,
        detail: detail.into(),
    });
}

fn attach_policy_trace(
    payload: &mut serde_json::Map<String, Value>,
    enabled: bool,
    trace: &[PolicyTraceStep],
    decision: &'static str,
) {
    if !enabled {
        return;
    }
    payload.insert("policy_decision".to_string(), json!(decision));
    payload.insert("policy_trace".to_string(), json!(trace));
}

fn bash_policy_error(
    command: Option<String>,
    cwd: Option<String>,
    policy_rule: &'static str,
    error: impl Into<String>,
    allowed_commands: Option<Vec<String>>,
    trace_enabled: bool,
    trace: &[PolicyTraceStep],
) -> ToolExecutionResult {
    let mut payload = serde_json::Map::new();
    if let Some(command) = command {
        payload.insert("command".to_string(), json!(command));
    }
    if let Some(cwd) = cwd {
        payload.insert("cwd".to_string(), json!(cwd));
    }
    payload.insert("policy_rule".to_string(), json!(policy_rule));
    payload.insert("error".to_string(), json!(error.into()));
    if let Some(allowed_commands) = allowed_commands {
        payload.insert("allowed_commands".to_string(), json!(allowed_commands));
    }
    attach_policy_trace(&mut payload, trace_enabled, trace, "deny");
    ToolExecutionResult::error(Value::Object(payload))
}

fn evaluate_tool_approval_gate(action: ApprovalAction) -> Option<ToolExecutionResult> {
    let action_kind = match &action {
        ApprovalAction::ToolBash { .. } => "tool:bash",
        ApprovalAction::ToolWrite { .. } => "tool:write",
        ApprovalAction::ToolEdit { .. } => "tool:edit",
        ApprovalAction::Command { .. } => "command",
    };
    let action_payload = serde_json::to_value(&action).unwrap_or(Value::Null);
    match evaluate_approval_gate(&action) {
        Ok(ApprovalGateResult::Allowed) => None,
        Ok(ApprovalGateResult::Denied {
            request_id,
            rule_id,
            reason_code,
            message,
        }) => Some(ToolExecutionResult::error(json!({
            "policy_rule": "approval_gate",
            "action_kind": action_kind,
            "action": action_payload,
            "approval_request_id": request_id,
            "approval_rule_id": rule_id,
            "reason_code": reason_code,
            "error": message,
            "hint": "run '/approvals list' then '/approvals approve <request_id>'",
        }))),
        Err(error) => Some(ToolExecutionResult::error(json!({
            "policy_rule": "approval_gate",
            "action_kind": action_kind,
            "action": action_payload,
            "reason_code": "approval_gate_error",
            "error": format!("failed to evaluate approval gate: {error}"),
        }))),
    }
}

fn evaluate_tool_rbac_gate(
    principal: Option<&str>,
    tool_name: &str,
    policy_path: Option<&Path>,
    action_payload: Value,
) -> Option<ToolExecutionResult> {
    let principal = principal
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(resolve_local_principal);
    let decision = if let Some(policy_path) = policy_path {
        authorize_tool_for_principal_with_policy_path(
            Some(principal.as_str()),
            tool_name,
            policy_path,
        )
    } else {
        authorize_tool_for_principal(Some(principal.as_str()), tool_name)
    };
    match decision {
        Ok(RbacDecision::Allow { .. }) => None,
        Ok(RbacDecision::Deny {
            reason_code,
            matched_role,
            matched_pattern,
        }) => Some(ToolExecutionResult::error(json!({
            "policy_rule": "rbac",
            "principal": principal,
            "action": format!("tool:{tool_name}"),
            "payload": action_payload,
            "reason_code": reason_code,
            "matched_role": matched_role,
            "matched_pattern": matched_pattern,
            "error": "rbac denied tool execution",
            "hint": "run '/rbac check tool:* --json' to inspect active role policy",
        }))),
        Err(error) => Some(ToolExecutionResult::error(json!({
            "policy_rule": "rbac",
            "principal": principal,
            "action": format!("tool:{tool_name}"),
            "payload": action_payload,
            "reason_code": "rbac_policy_error",
            "error": format!("failed to evaluate rbac policy: {error}"),
        }))),
    }
}

#[async_trait]
impl AgentTool for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a shell command".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> ToolExecutionResult {
        let command = match required_string(&arguments, "command") {
            Ok(command) => command,
            Err(error) => return ToolExecutionResult::error(json!({ "error": error })),
        };
        let trace_enabled = self.policy.tool_policy_trace;
        let mut trace = Vec::new();

        let command_length = command.chars().count();
        if command_length > self.policy.max_command_length {
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "max_command_length",
                "deny",
                format!(
                    "command is too long ({} chars), limit is {} chars",
                    command_length, self.policy.max_command_length
                ),
            );
            return bash_policy_error(
                Some(command),
                None,
                "max_command_length",
                format!(
                    "command is too long ({} chars), limit is {} chars",
                    command_length, self.policy.max_command_length
                ),
                None,
                trace_enabled,
                &trace,
            );
        }
        push_policy_trace(
            &mut trace,
            trace_enabled,
            "max_command_length",
            "allow",
            format!(
                "command length {} within limit {}",
                command_length, self.policy.max_command_length
            ),
        );

        if !self.policy.allow_command_newlines && (command.contains('\n') || command.contains('\r'))
        {
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "allow_command_newlines",
                "deny",
                "multiline command detected while newlines are disallowed",
            );
            return bash_policy_error(
                Some(command),
                None,
                "allow_command_newlines",
                "multiline commands are disabled by policy",
                None,
                trace_enabled,
                &trace,
            );
        }
        push_policy_trace(
            &mut trace,
            trace_enabled,
            "allow_command_newlines",
            "allow",
            "command newline policy satisfied",
        );

        if !self.policy.allowed_commands.is_empty() {
            let Some(executable) = leading_executable(&command) else {
                push_policy_trace(
                    &mut trace,
                    trace_enabled,
                    "allowed_commands",
                    "deny",
                    "unable to parse command executable",
                );
                return bash_policy_error(
                    Some(command),
                    None,
                    "allowed_commands",
                    "unable to parse command executable",
                    None,
                    trace_enabled,
                    &trace,
                );
            };
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "executable_parse",
                "allow",
                format!("parsed executable '{executable}'"),
            );
            if !is_command_allowed(&executable, &self.policy.allowed_commands) {
                push_policy_trace(
                    &mut trace,
                    trace_enabled,
                    "allowed_commands",
                    "deny",
                    format!(
                        "command '{}' is not allowed by '{}' bash profile",
                        executable,
                        bash_profile_name(self.policy.bash_profile),
                    ),
                );
                return bash_policy_error(
                    Some(command),
                    None,
                    "allowed_commands",
                    format!(
                        "command '{}' is not allowed by '{}' bash profile",
                        executable,
                        bash_profile_name(self.policy.bash_profile),
                    ),
                    Some(self.policy.allowed_commands.clone()),
                    trace_enabled,
                    &trace,
                );
            }
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "allowed_commands",
                "allow",
                format!("command '{executable}' allowed"),
            );
        } else {
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "allowed_commands",
                "allow",
                "allowlist disabled for current profile",
            );
        }

        let cwd = match arguments.get("cwd").and_then(Value::as_str) {
            Some(cwd) => match resolve_and_validate_path(cwd, &self.policy, PathMode::Directory) {
                Ok(path) => {
                    if let Err(error) =
                        validate_directory_target(&path, self.policy.enforce_regular_files)
                    {
                        push_policy_trace(
                            &mut trace,
                            trace_enabled,
                            "cwd_validation",
                            "deny",
                            error.clone(),
                        );
                        return bash_policy_error(
                            Some(command),
                            Some(path.display().to_string()),
                            "cwd_validation",
                            error,
                            None,
                            trace_enabled,
                            &trace,
                        );
                    }
                    push_policy_trace(
                        &mut trace,
                        trace_enabled,
                        "cwd_validation",
                        "allow",
                        format!("cwd '{}' accepted", path.display()),
                    );
                    Some(path)
                }
                Err(error) => {
                    push_policy_trace(
                        &mut trace,
                        trace_enabled,
                        "allowed_roots",
                        "deny",
                        error.clone(),
                    );
                    return bash_policy_error(
                        Some(command),
                        Some(cwd.to_string()),
                        "allowed_roots",
                        error,
                        None,
                        trace_enabled,
                        &trace,
                    );
                }
            },
            None => {
                push_policy_trace(
                    &mut trace,
                    trace_enabled,
                    "cwd_validation",
                    "allow",
                    "cwd not provided; using process current directory",
                );
                None
            }
        };

        if let Some(rbac_result) = evaluate_tool_rbac_gate(
            self.policy.rbac_principal.as_deref(),
            "bash",
            self.policy.rbac_policy_path.as_deref(),
            json!({
                "command": command.clone(),
                "cwd": cwd.as_ref().map(|value| value.display().to_string()),
            }),
        ) {
            return rbac_result;
        }

        if let Some(approval_result) = evaluate_tool_approval_gate(ApprovalAction::ToolBash {
            command: command.clone(),
            cwd: cwd.as_ref().map(|value| value.display().to_string()),
        }) {
            return approval_result;
        }

        let override_payload = serde_json::json!({
            "tool": "bash",
            "command": command.clone(),
            "cwd": cwd.as_ref().map(|value| value.display().to_string()),
            "bash_profile": bash_profile_name(self.policy.bash_profile),
            "sandbox_mode": os_sandbox_mode_name(self.policy.os_sandbox_mode),
        });
        if let Some(root) = &self.policy.extension_policy_override_root {
            let override_result = evaluate_extension_policy_override(root, &override_payload);
            if !override_result.allowed {
                let denied_by = override_result
                    .denied_by
                    .clone()
                    .unwrap_or_else(|| "unknown-extension".to_string());
                let reason = override_result
                    .reason
                    .clone()
                    .unwrap_or_else(|| "extension policy override denied command".to_string());
                push_policy_trace(
                    &mut trace,
                    trace_enabled,
                    "extension_policy_override",
                    "deny",
                    format!("denied by {}: {}", denied_by, reason),
                );

                let mut payload = serde_json::Map::new();
                payload.insert("command".to_string(), json!(command));
                payload.insert(
                    "cwd".to_string(),
                    json!(cwd.as_ref().map(|value| value.display().to_string())),
                );
                payload.insert(
                    "policy_rule".to_string(),
                    json!("extension_policy_override"),
                );
                payload.insert("error".to_string(), json!(reason));
                payload.insert("denied_by".to_string(), json!(denied_by));
                payload.insert(
                    "extension_root".to_string(),
                    json!(root.display().to_string()),
                );
                payload.insert(
                    "permission_denied".to_string(),
                    json!(override_result.permission_denied),
                );
                payload.insert(
                    "diagnostics".to_string(),
                    json!(override_result.diagnostics),
                );
                attach_policy_trace(&mut payload, trace_enabled, &trace, "deny");
                return ToolExecutionResult::error(Value::Object(payload));
            }

            push_policy_trace(
                &mut trace,
                trace_enabled,
                "extension_policy_override",
                "allow",
                format!(
                    "evaluated {} policy-override extension hooks with allow decision",
                    override_result.evaluated
                ),
            );
        } else {
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "extension_policy_override",
                "allow",
                "extension policy override hooks are disabled",
            );
        }

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        let current_dir = cwd
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let sandbox_spec = match resolve_sandbox_spec(&self.policy, &shell, &command, &current_dir)
        {
            Ok(spec) => {
                push_policy_trace(
                    &mut trace,
                    trace_enabled,
                    "os_sandbox_mode",
                    "allow",
                    format!(
                        "resolved sandbox mode '{}' (sandboxed={})",
                        os_sandbox_mode_name(self.policy.os_sandbox_mode),
                        spec.sandboxed
                    ),
                );
                spec
            }
            Err(error) => {
                push_policy_trace(
                    &mut trace,
                    trace_enabled,
                    "os_sandbox_mode",
                    "deny",
                    error.clone(),
                );
                return bash_policy_error(
                    Some(command),
                    cwd.as_ref().map(|value| value.display().to_string()),
                    "os_sandbox_mode",
                    error,
                    None,
                    trace_enabled,
                    &trace,
                );
            }
        };

        if self.policy.bash_dry_run {
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "execution_mode",
                "allow",
                "bash dry-run enabled; command not executed",
            );
            let mut payload = serde_json::Map::new();
            payload.insert("command".to_string(), json!(command));
            payload.insert(
                "cwd".to_string(),
                json!(cwd.map(|value| value.display().to_string())),
            );
            payload.insert("sandboxed".to_string(), json!(sandbox_spec.sandboxed));
            payload.insert(
                "sandbox_mode".to_string(),
                json!(os_sandbox_mode_name(self.policy.os_sandbox_mode)),
            );
            payload.insert("dry_run".to_string(), json!(true));
            payload.insert("would_execute".to_string(), json!(true));
            payload.insert("status".to_string(), Value::Null);
            payload.insert("success".to_string(), json!(true));
            payload.insert("stdout".to_string(), json!(""));
            payload.insert("stderr".to_string(), json!(""));
            attach_policy_trace(&mut payload, trace_enabled, &trace, "allow");
            return ToolExecutionResult::ok(Value::Object(payload));
        }
        push_policy_trace(
            &mut trace,
            trace_enabled,
            "execution_mode",
            "allow",
            "bash command execution permitted",
        );

        let mut command_builder = Command::new(&sandbox_spec.program);
        command_builder.args(&sandbox_spec.args);
        command_builder.kill_on_drop(true);
        command_builder.env_clear();
        for key in SAFE_BASH_ENV_VARS {
            if let Ok(value) = std::env::var(key) {
                command_builder.env(key, value);
            }
        }
        command_builder.env(
            "TAU_SANDBOXED",
            if sandbox_spec.sandboxed { "1" } else { "0" },
        );

        if let Some(cwd) = &cwd {
            command_builder.current_dir(cwd);
        }

        let timeout_duration = Duration::from_millis(self.policy.bash_timeout_ms.max(1));
        let output = match timeout(timeout_duration, command_builder.output()).await {
            Ok(result) => match result {
                Ok(output) => output,
                Err(error) => {
                    let mut payload = serde_json::Map::new();
                    payload.insert("command".to_string(), json!(command));
                    payload.insert(
                        "cwd".to_string(),
                        json!(cwd.as_ref().map(|value| value.display().to_string())),
                    );
                    payload.insert("error".to_string(), json!(error.to_string()));
                    attach_policy_trace(&mut payload, trace_enabled, &trace, "allow");
                    return ToolExecutionResult::error(Value::Object(payload));
                }
            },
            Err(_) => {
                let mut payload = serde_json::Map::new();
                payload.insert("command".to_string(), json!(command));
                payload.insert(
                    "cwd".to_string(),
                    json!(cwd.as_ref().map(|value| value.display().to_string())),
                );
                payload.insert(
                    "error".to_string(),
                    json!(format!(
                        "command timed out after {} ms",
                        self.policy.bash_timeout_ms
                    )),
                );
                attach_policy_trace(&mut payload, trace_enabled, &trace, "allow");
                return ToolExecutionResult::error(Value::Object(payload));
            }
        };

        let stdout = redact_secrets(&String::from_utf8_lossy(&output.stdout));
        let stderr = redact_secrets(&String::from_utf8_lossy(&output.stderr));
        let mut payload = serde_json::Map::new();
        payload.insert("command".to_string(), json!(command));
        payload.insert(
            "cwd".to_string(),
            json!(cwd.map(|value| value.display().to_string())),
        );
        payload.insert("sandboxed".to_string(), json!(sandbox_spec.sandboxed));
        payload.insert(
            "sandbox_mode".to_string(),
            json!(os_sandbox_mode_name(self.policy.os_sandbox_mode)),
        );
        payload.insert("dry_run".to_string(), json!(false));
        payload.insert("status".to_string(), json!(output.status.code()));
        payload.insert("success".to_string(), json!(output.status.success()));
        payload.insert(
            "stdout".to_string(),
            json!(truncate_bytes(
                &stdout,
                self.policy.max_command_output_bytes
            )),
        );
        payload.insert(
            "stderr".to_string(),
            json!(truncate_bytes(
                &stderr,
                self.policy.max_command_output_bytes
            )),
        );
        attach_policy_trace(&mut payload, trace_enabled, &trace, "allow");
        ToolExecutionResult::ok(Value::Object(payload))
    }
}

fn collect_session_inventory(
    policy: &ToolPolicy,
    limit: usize,
) -> Result<(Vec<SessionInventoryEntry>, usize), String> {
    let session_paths = discover_session_paths(policy, limit)?;
    let mut sessions = Vec::with_capacity(session_paths.len());
    let mut skipped_invalid = 0usize;

    for path in session_paths {
        if sessions.len() >= limit {
            break;
        }
        match SessionStore::load(&path) {
            Ok(store) => {
                let newest = store.entries().last();
                sessions.push(SessionInventoryEntry {
                    path: path.display().to_string(),
                    entries: store.entries().len(),
                    head_id: store.head_id(),
                    newest_role: newest
                        .map(|entry| session_message_role(&entry.message))
                        .unwrap_or_else(|| "none".to_string()),
                    newest_preview: newest
                        .map(|entry| session_message_preview(&entry.message))
                        .unwrap_or_else(|| "(empty session)".to_string()),
                });
            }
            Err(_) => {
                skipped_invalid += 1;
            }
        }
    }

    sessions.sort_by(|left, right| left.path.cmp(&right.path));
    Ok((sessions, skipped_invalid))
}

fn discover_session_paths(policy: &ToolPolicy, limit: usize) -> Result<Vec<PathBuf>, String> {
    let mut roots = if policy.allowed_roots.is_empty() {
        vec![std::env::current_dir().map_err(|error| format!("failed to resolve cwd: {error}"))?]
    } else {
        policy.allowed_roots.clone()
    };
    roots.sort_by_key(|left| left.display().to_string());

    let mut found = Vec::new();
    let mut seen = BTreeSet::new();
    for root in roots {
        if found.len() >= limit {
            break;
        }
        let tau_root = root.join(".tau");
        if !tau_root.exists() {
            continue;
        }

        let mut queue = VecDeque::from([(tau_root, 0usize)]);
        let mut visited_directories = 0usize;
        while let Some((directory, depth)) = queue.pop_front() {
            if found.len() >= limit || visited_directories >= SESSION_SCAN_MAX_DIRECTORIES {
                break;
            }
            visited_directories += 1;

            let entries = std::fs::read_dir(&directory).map_err(|error| {
                format!(
                    "failed to scan session directory '{}': {error}",
                    directory.display()
                )
            })?;
            let mut child_paths = entries
                .filter_map(|entry| entry.ok().map(|item| item.path()))
                .collect::<Vec<_>>();
            child_paths.sort();

            for path in child_paths {
                if found.len() >= limit {
                    break;
                }
                let metadata = match std::fs::symlink_metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    if depth < SESSION_SCAN_MAX_DEPTH {
                        queue.push_back((path, depth + 1));
                    }
                    continue;
                }
                if !metadata.is_file() || !is_session_candidate_path(&path) {
                    continue;
                }
                let canonical = canonicalize_best_effort(&path).map_err(|error| {
                    format!(
                        "failed to canonicalize session candidate '{}': {error}",
                        path.display()
                    )
                })?;
                let key = canonical.display().to_string();
                if seen.insert(key) {
                    found.push(canonical);
                }
            }
        }
    }

    found.sort();
    Ok(found)
}

fn is_session_candidate_path(path: &Path) -> bool {
    if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
        return false;
    }

    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if file_name == "default.jsonl"
        || file_name == "session.jsonl"
        || file_name.starts_with("issue-")
    {
        return true;
    }

    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|value| value.eq_ignore_ascii_case("sessions"))
            .unwrap_or(false)
    })
}

#[derive(Debug, Clone, Copy)]
enum PathMode {
    Read,
    Write,
    Edit,
    Directory,
}

fn resolve_and_validate_path(
    user_path: &str,
    policy: &ToolPolicy,
    mode: PathMode,
) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| format!("failed to resolve cwd: {error}"))?;
    let input = PathBuf::from(user_path);
    let absolute = if input.is_absolute() {
        input
    } else {
        cwd.join(input)
    };

    let canonical = canonicalize_best_effort(&absolute).map_err(|error| {
        format!(
            "failed to canonicalize path '{}': {error}",
            absolute.display()
        )
    })?;

    if !is_path_allowed(&canonical, policy)? {
        return Err(format!(
            "path '{}' is outside allowed roots",
            canonical.display()
        ));
    }

    if matches!(mode, PathMode::Read | PathMode::Edit | PathMode::Directory) && !absolute.exists() {
        return Err(format!("path '{}' does not exist", absolute.display()));
    }

    Ok(absolute)
}

fn validate_file_target(
    path: &Path,
    mode: PathMode,
    enforce_regular_files: bool,
) -> Result<(), String> {
    if !enforce_regular_files {
        return Ok(());
    }

    if !path.exists() {
        return Ok(());
    }

    let symlink_meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect path '{}': {error}", path.display()))?;
    if symlink_meta.file_type().is_symlink() {
        return Err(format!(
            "path '{}' is a symbolic link, which is denied by policy",
            path.display()
        ));
    }

    if matches!(mode, PathMode::Read | PathMode::Edit) && !symlink_meta.file_type().is_file() {
        return Err(format!(
            "path '{}' must be a regular file for this operation",
            path.display()
        ));
    }

    if matches!(mode, PathMode::Write) && path.exists() && !symlink_meta.file_type().is_file() {
        return Err(format!(
            "path '{}' must be a regular file when overwriting existing content",
            path.display()
        ));
    }

    Ok(())
}

fn validate_directory_target(path: &Path, enforce_regular_files: bool) -> Result<(), String> {
    let symlink_meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect path '{}': {error}", path.display()))?;
    if enforce_regular_files && symlink_meta.file_type().is_symlink() {
        return Err(format!(
            "path '{}' is a symbolic link, which is denied by policy",
            path.display()
        ));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect path '{}': {error}", path.display()))?;
    if !metadata.is_dir() {
        return Err(format!(
            "path '{}' must be a directory for this operation",
            path.display()
        ));
    }

    Ok(())
}

fn is_path_allowed(path: &Path, policy: &ToolPolicy) -> Result<bool, String> {
    if policy.allowed_roots.is_empty() {
        return Ok(true);
    }

    for root in &policy.allowed_roots {
        let canonical_root = canonicalize_best_effort(root)
            .map_err(|error| format!("invalid allowed root '{}': {error}", root.display()))?;

        if path.starts_with(&canonical_root) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn canonicalize_best_effort(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path);
    }

    let mut missing_suffix: Vec<OsString> = Vec::new();
    let mut cursor = path;

    while !cursor.exists() {
        if let Some(file_name) = cursor.file_name() {
            missing_suffix.push(file_name.to_os_string());
        }

        cursor = match cursor.parent() {
            Some(parent) => parent,
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no existing ancestor for path",
                ));
            }
        };
    }

    let mut canonical = std::fs::canonicalize(cursor)?;
    for component in missing_suffix.iter().rev() {
        canonical.push(component);
    }

    Ok(canonical)
}

fn required_string(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(|value| value.to_string())
        .ok_or_else(|| format!("missing required string argument '{key}'"))
}

fn optional_usize(
    arguments: &Value,
    key: &str,
    default: usize,
    max: usize,
) -> Result<usize, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(default);
    };
    let parsed = value
        .as_u64()
        .ok_or_else(|| format!("optional argument '{key}' must be an integer"))?
        as usize;
    if parsed == 0 {
        return Err(format!("optional argument '{key}' must be greater than 0"));
    }
    if parsed > max {
        return Err(format!("optional argument '{key}' exceeds maximum {max}"));
    }
    Ok(parsed)
}

fn optional_u64(arguments: &Value, key: &str) -> Result<Option<u64>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let parsed = value
        .as_u64()
        .ok_or_else(|| format!("optional argument '{key}' must be an integer"))?;
    Ok(Some(parsed))
}

fn optional_session_search_role(arguments: &Value, key: &str) -> Result<Option<String>, String> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let raw = value
        .as_str()
        .ok_or_else(|| format!("optional argument '{key}' must be a string"))?;
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "system" | "user" | "assistant" | "tool" => Ok(Some(normalized)),
        _ => Err(format!(
            "optional argument '{key}' must be one of: system, user, assistant, tool"
        )),
    }
}

fn resolve_sandbox_spec(
    policy: &ToolPolicy,
    shell: &str,
    command: &str,
    cwd: &Path,
) -> Result<BashSandboxSpec, String> {
    if !policy.os_sandbox_command.is_empty() {
        return build_spec_from_command_template(&policy.os_sandbox_command, shell, command, cwd);
    }

    match policy.os_sandbox_mode {
        OsSandboxMode::Off => Ok(BashSandboxSpec {
            program: shell.to_string(),
            args: vec!["-lc".to_string(), command.to_string()],
            sandboxed: false,
        }),
        OsSandboxMode::Auto => {
            if let Some(spec) = auto_sandbox_spec(shell, command, cwd) {
                Ok(spec)
            } else {
                Ok(BashSandboxSpec {
                    program: shell.to_string(),
                    args: vec!["-lc".to_string(), command.to_string()],
                    sandboxed: false,
                })
            }
        }
        OsSandboxMode::Force => {
            if let Some(spec) = auto_sandbox_spec(shell, command, cwd) {
                Ok(spec)
            } else {
                Err("OS sandbox mode 'force' is enabled but no sandbox launcher is configured or available".to_string())
            }
        }
    }
}

fn build_spec_from_command_template(
    template: &[String],
    shell: &str,
    command: &str,
    cwd: &Path,
) -> Result<BashSandboxSpec, String> {
    let Some(program_template) = template.first() else {
        return Err("sandbox command template is empty".to_string());
    };
    let mut args = Vec::new();
    let mut has_shell = false;
    let mut has_command = false;
    for token in &template[1..] {
        if token == "{shell}" {
            has_shell = true;
            args.push(shell.to_string());
            continue;
        }
        if token == "{command}" {
            has_command = true;
            args.push(command.to_string());
            continue;
        }
        args.push(token.replace("{cwd}", &cwd.display().to_string()));
    }

    let program = program_template.replace("{cwd}", &cwd.display().to_string());
    if !has_shell {
        args.push(shell.to_string());
    }
    if !has_command {
        args.push("-lc".to_string());
        args.push(command.to_string());
    }

    Ok(BashSandboxSpec {
        program,
        args,
        sandboxed: true,
    })
}

fn auto_sandbox_spec(shell: &str, command: &str, cwd: &Path) -> Option<BashSandboxSpec> {
    #[cfg(not(target_os = "linux"))]
    let _ = (shell, command, cwd);

    #[cfg(target_os = "linux")]
    {
        if command_available("bwrap") {
            let mut args = vec![
                "--die-with-parent".to_string(),
                "--new-session".to_string(),
                "--unshare-all".to_string(),
                "--proc".to_string(),
                "/proc".to_string(),
                "--dev".to_string(),
                "/dev".to_string(),
                "--tmpfs".to_string(),
                "/tmp".to_string(),
            ];
            for mount in ["/usr", "/bin", "/lib", "/lib64"] {
                if Path::new(mount).exists() {
                    args.extend_from_slice(&[
                        "--ro-bind".to_string(),
                        mount.to_string(),
                        mount.to_string(),
                    ]);
                }
            }
            args.extend_from_slice(&[
                "--bind".to_string(),
                cwd.display().to_string(),
                cwd.display().to_string(),
                "--chdir".to_string(),
                cwd.display().to_string(),
                shell.to_string(),
                "-lc".to_string(),
                command.to_string(),
            ]);
            return Some(BashSandboxSpec {
                program: "bwrap".to_string(),
                args,
                sandboxed: true,
            });
        }
    }

    None
}

#[cfg(any(test, target_os = "linux"))]
fn command_available(command: &str) -> bool {
    let path = std::env::var_os("PATH");
    let Some(path) = path else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(command);
        if candidate.exists() {
            return true;
        }
    }
    false
}

fn leading_executable(command: &str) -> Option<String> {
    let tokens = shell_words::split(command).ok()?;
    for token in tokens {
        if is_shell_assignment(&token) {
            continue;
        }

        return Some(
            Path::new(&token)
                .file_name()
                .map(|file_name| file_name.to_string_lossy().to_string())
                .unwrap_or(token),
        );
    }
    None
}

fn is_shell_assignment(token: &str) -> bool {
    let Some((name, _value)) = token.split_once('=') else {
        return false;
    };

    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }

    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_command_allowed(executable: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|entry| {
        if let Some(prefix) = entry.strip_suffix('*') {
            executable.starts_with(prefix)
        } else {
            executable == entry
        }
    })
}

fn bash_profile_name(profile: BashCommandProfile) -> &'static str {
    match profile {
        BashCommandProfile::Permissive => "permissive",
        BashCommandProfile::Balanced => "balanced",
        BashCommandProfile::Strict => "strict",
    }
}

fn os_sandbox_mode_name(mode: OsSandboxMode) -> &'static str {
    match mode {
        OsSandboxMode::Off => "off",
        OsSandboxMode::Auto => "auto",
        OsSandboxMode::Force => "force",
    }
}

fn truncate_bytes(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }

    if limit == 0 {
        return "<output truncated>".to_string();
    }

    let mut end = limit.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }

    let mut output = value[..end].to_string();
    output.push_str("\n<output truncated>");
    output
}

fn redact_secrets(text: &str) -> String {
    let mut redacted = text.to_string();

    for (name, value) in std::env::vars() {
        let upper = name.to_ascii_uppercase();
        let is_sensitive = upper.ends_with("_KEY")
            || upper.ends_with("_TOKEN")
            || upper.ends_with("_SECRET")
            || upper.ends_with("_PASSWORD");
        if !is_sensitive || value.trim().len() < 6 {
            continue;
        }

        redacted = redacted.replace(&value, "[REDACTED]");
    }

    redacted
}

#[cfg(test)]
mod tests;
