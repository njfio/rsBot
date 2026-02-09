use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use tau_agent_core::{Agent, AgentTool, ToolExecutionResult};
use tau_ai::ToolDefinition;
use tokio::{process::Command, time::timeout};

use crate::extension_manifest::{
    evaluate_extension_policy_override, execute_extension_registered_tool, ExtensionRegisteredTool,
};
use crate::{
    authorize_tool_for_principal, authorize_tool_for_principal_with_policy_path,
    evaluate_approval_gate, resolve_local_principal, ApprovalAction, ApprovalGateResult,
    RbacDecision,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BashCommandProfile {
    Permissive,
    Balanced,
    Strict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicyPreset {
    Permissive,
    Balanced,
    Strict,
    Hardened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
mod tests {
    use std::{fs, sync::Arc};

    use proptest::prelude::*;
    use tempfile::tempdir;

    use super::{
        bash_profile_name, build_spec_from_command_template, canonicalize_best_effort,
        command_available, evaluate_tool_approval_gate, evaluate_tool_rbac_gate,
        is_command_allowed, leading_executable, os_sandbox_mode_name, redact_secrets,
        resolve_sandbox_spec, truncate_bytes, AgentTool, BashCommandProfile, BashTool, EditTool,
        OsSandboxMode, ToolExecutionResult, ToolPolicy, ToolPolicyPreset, WriteTool,
    };
    use crate::ApprovalAction;

    fn test_policy(path: &Path) -> Arc<ToolPolicy> {
        Arc::new(ToolPolicy::new(vec![path.to_path_buf()]))
    }

    fn make_executable(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).expect("metadata").permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).expect("set executable permissions");
        }
    }

    #[cfg(unix)]
    use std::os::unix::fs::symlink as symlink_file;
    use std::path::Path;

    #[test]
    fn unit_tool_policy_hardened_preset_applies_expected_configuration() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.apply_preset(ToolPolicyPreset::Hardened);

        assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
        assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
        assert_eq!(policy.max_command_length, 1_024);
        assert_eq!(policy.max_command_output_bytes, 4_000);
        assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Force);
        assert!(policy.enforce_regular_files);
    }

    #[test]
    fn regression_tool_approval_gate_is_noop_when_policy_is_missing() {
        let result = evaluate_tool_approval_gate(ApprovalAction::ToolWrite {
            path: "/tmp/example.txt".to_string(),
            content_bytes: 12,
        });
        assert!(result.is_none());
    }

    #[test]
    fn regression_tool_rbac_gate_is_noop_when_policy_is_missing() {
        let result = evaluate_tool_rbac_gate(
            Some("local:operator"),
            "write",
            None,
            serde_json::json!({
                "path": "/tmp/example.txt",
                "content_bytes": 12,
            }),
        );
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn edit_tool_replaces_single_match() {
        let temp = tempdir().expect("tempdir");
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "a a a").await.expect("write file");

        let tool = EditTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "path": file,
                "find": "a",
                "replace": "b"
            }))
            .await;

        assert!(!result.is_error);
        let content = tokio::fs::read_to_string(temp.path().join("test.txt"))
            .await
            .expect("read file");
        assert_eq!(content, "b a a");
    }

    #[tokio::test]
    async fn edit_tool_replaces_all_matches() {
        let temp = tempdir().expect("tempdir");
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "a a a").await.expect("write file");

        let tool = EditTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "path": file,
                "find": "a",
                "replace": "b",
                "all": true
            }))
            .await;

        assert!(!result.is_error);
        let content = tokio::fs::read_to_string(temp.path().join("test.txt"))
            .await
            .expect("read file");
        assert_eq!(content, "b b b");
    }

    #[tokio::test]
    async fn regression_edit_tool_rejects_result_larger_than_write_limit() {
        let temp = tempdir().expect("tempdir");
        let file = temp.path().join("test.txt");
        tokio::fs::write(&file, "a").await.expect("write file");

        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.max_file_write_bytes = 3;
        let tool = EditTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "path": file,
                "find": "a",
                "replace": "longer",
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("edited content is too large"));
    }

    #[tokio::test]
    async fn write_tool_creates_parent_directory() {
        let temp = tempdir().expect("tempdir");
        let file = temp.path().join("nested/output.txt");

        let tool = WriteTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "path": file,
                "content": "hello"
            }))
            .await;

        assert!(!result.is_error);
        let content = tokio::fs::read_to_string(temp.path().join("nested/output.txt"))
            .await
            .expect("read file");
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn functional_write_tool_enforces_max_file_write_bytes() {
        let temp = tempdir().expect("tempdir");
        let file = temp.path().join("too-large.txt");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.max_file_write_bytes = 4;
        let tool = WriteTool::new(Arc::new(policy));

        let result = tool
            .execute(serde_json::json!({
                "path": file,
                "content": "hello"
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("content is too large (5 bytes), limit is 4 bytes"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn functional_write_tool_rejects_symlink_targets_by_default() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("target.txt");
        tokio::fs::write(&target, "safe")
            .await
            .expect("write target");
        let symlink = temp.path().join("link.txt");
        symlink_file(&target, &symlink).expect("create symlink");

        let tool = WriteTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "path": symlink,
                "content": "changed"
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("symbolic link, which is denied by policy"));
    }

    #[tokio::test]
    async fn bash_tool_runs_command() {
        let temp = tempdir().expect("tempdir");
        let tool = BashTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "command": "printf 'ok'",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(!result.is_error);
        assert_eq!(
            result
                .content
                .get("stdout")
                .and_then(serde_json::Value::as_str),
            Some("ok")
        );
        assert_eq!(
            result
                .content
                .get("sandbox_mode")
                .and_then(serde_json::Value::as_str),
            Some("off")
        );
        assert_eq!(
            result
                .content
                .get("sandboxed")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
    }

    #[tokio::test]
    async fn regression_bash_tool_rejects_non_directory_cwd() {
        let temp = tempdir().expect("tempdir");
        let file = temp.path().join("not-a-dir.txt");
        tokio::fs::write(&file, "x").await.expect("write file");

        let tool = BashTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "command": "printf 'ok'",
                "cwd": file,
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("must be a directory for this operation"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_bash_tool_rejects_symlink_cwd_when_enforced() {
        let temp = tempdir().expect("tempdir");
        let real_dir = temp.path().join("real");
        tokio::fs::create_dir_all(&real_dir)
            .await
            .expect("create real dir");
        let link_dir = temp.path().join("link");
        symlink_file(&real_dir, &link_dir).expect("create symlink");

        let tool = BashTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "command": "pwd",
                "cwd": link_dir,
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("symbolic link, which is denied by policy"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn integration_bash_tool_allows_symlink_cwd_when_enforcement_disabled() {
        let temp = tempdir().expect("tempdir");
        let real_dir = temp.path().join("real");
        tokio::fs::create_dir_all(&real_dir)
            .await
            .expect("create real dir");
        let link_dir = temp.path().join("link");
        symlink_file(&real_dir, &link_dir).expect("create symlink");

        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.enforce_regular_files = false;
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": "pwd",
                "cwd": link_dir,
            }))
            .await;

        assert!(!result.is_error);
        assert_eq!(
            result
                .content
                .get("success")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn bash_tool_times_out_long_command() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.bash_timeout_ms = 100;
        let tool = BashTool::new(Arc::new(policy));

        let result = tool
            .execute(serde_json::json!({
                "command": "sleep 2",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("command timed out after 100 ms"));
    }

    #[tokio::test]
    async fn unit_bash_tool_rejects_multiline_commands_by_default() {
        let temp = tempdir().expect("tempdir");
        let tool = BashTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "command": "printf 'a'\nprintf 'b'",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("multiline commands are disabled"));
    }

    #[tokio::test]
    async fn regression_bash_tool_blocks_command_not_in_allowlist() {
        let temp = tempdir().expect("tempdir");
        let tool = BashTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "command": "python --version",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert!(result
            .content
            .to_string()
            .contains("is not allowed by 'balanced' bash profile"));
        assert_eq!(
            result
                .content
                .get("policy_rule")
                .and_then(serde_json::Value::as_str),
            Some("allowed_commands")
        );
        assert!(result.content.get("policy_trace").is_none());
    }

    #[tokio::test]
    async fn integration_bash_tool_policy_trace_emits_deny_decision_details() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.tool_policy_trace = true;
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": "python --version",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert_eq!(
            result
                .content
                .get("policy_decision")
                .and_then(serde_json::Value::as_str),
            Some("deny")
        );
        let trace = result
            .content
            .get("policy_trace")
            .and_then(serde_json::Value::as_array)
            .expect("trace should be present for trace mode");
        assert!(!trace.is_empty());
        assert!(trace.iter().any(|step| {
            step.get("check").and_then(serde_json::Value::as_str) == Some("allowed_commands")
                && step.get("outcome").and_then(serde_json::Value::as_str) == Some("deny")
        }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn functional_bash_tool_policy_override_deny_blocks_execution() {
        let temp = tempdir().expect("tempdir");
        let extensions_root = temp.path().join("extensions");
        let extension_dir = extensions_root.join("policy-enforcer");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"deny\",\"reason\":\"command denied\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let marker = temp.path().join("marker.txt");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.extension_policy_override_root = Some(extensions_root);
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": format!("printf 'x' > {}", marker.display()),
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert_eq!(
            result
                .content
                .get("policy_rule")
                .and_then(serde_json::Value::as_str),
            Some("extension_policy_override")
        );
        assert_eq!(
            result
                .content
                .get("denied_by")
                .and_then(serde_json::Value::as_str),
            Some("policy-enforcer@1.0.0")
        );
        assert!(result
            .content
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .contains("command denied"));
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn functional_bash_tool_policy_override_missing_permission_denies_before_spawn() {
        let temp = tempdir().expect("tempdir");
        let extensions_root = temp.path().join("extensions");
        let extension_dir = extensions_root.join("missing-permission");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"allow\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "missing-permission",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let marker = temp.path().join("marker.txt");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.extension_policy_override_root = Some(extensions_root);
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": format!("printf 'x' > {}", marker.display()),
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert_eq!(
            result
                .content
                .get("policy_rule")
                .and_then(serde_json::Value::as_str),
            Some("extension_policy_override")
        );
        assert!(result
            .content
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .contains("requires 'run-commands' permission"));
        assert_eq!(
            result
                .content
                .get("permission_denied")
                .and_then(serde_json::Value::as_u64),
            Some(1)
        );
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn integration_bash_tool_policy_override_allow_permits_execution() {
        let temp = tempdir().expect("tempdir");
        let extensions_root = temp.path().join("extensions");
        let extension_dir = extensions_root.join("policy-enforcer");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf '{\"decision\":\"allow\"}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "policy-enforcer",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.extension_policy_override_root = Some(extensions_root);
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": "printf 'ok'",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(!result.is_error);
        assert_eq!(
            result
                .content
                .get("stdout")
                .and_then(serde_json::Value::as_str),
            Some("ok")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn regression_bash_tool_policy_override_invalid_response_fails_closed() {
        let temp = tempdir().expect("tempdir");
        let extensions_root = temp.path().join("extensions");
        let extension_dir = extensions_root.join("broken-policy");
        fs::create_dir_all(&extension_dir).expect("create extension dir");

        let script_path = extension_dir.join("policy.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nread -r _input\nprintf '{\"decision\":123}'\n",
        )
        .expect("write script");
        make_executable(&script_path);

        fs::write(
            extension_dir.join("extension.json"),
            r#"{
  "schema_version": 1,
  "id": "broken-policy",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "policy.sh",
  "hooks": ["policy-override"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
        )
        .expect("write manifest");

        let marker = temp.path().join("marker.txt");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.extension_policy_override_root = Some(extensions_root);
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": format!("printf 'x' > {}", marker.display()),
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert_eq!(
            result
                .content
                .get("policy_rule")
                .and_then(serde_json::Value::as_str),
            Some("extension_policy_override")
        );
        assert!(result
            .content
            .get("diagnostics")
            .and_then(serde_json::Value::as_array)
            .expect("diagnostics array")
            .iter()
            .any(|value| value
                .as_str()
                .unwrap_or_default()
                .contains("invalid response")));
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn integration_bash_tool_dry_run_validates_without_execution() {
        let temp = tempdir().expect("tempdir");
        let marker = temp.path().join("marker.txt");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.bash_dry_run = true;
        let tool = BashTool::new(Arc::new(policy));

        let result = tool
            .execute(serde_json::json!({
                "command": format!("printf 'x' > {}", marker.display()),
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(!result.is_error);
        assert_eq!(
            result
                .content
                .get("dry_run")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            result
                .content
                .get("would_execute")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            result
                .content
                .get("policy_decision")
                .and_then(serde_json::Value::as_str),
            None
        );
        assert!(result.content.get("policy_trace").is_none());
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn functional_bash_tool_trace_includes_allow_decision_for_dry_run() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.bash_dry_run = true;
        policy.tool_policy_trace = true;
        let tool = BashTool::new(Arc::new(policy));

        let result = tool
            .execute(serde_json::json!({
                "command": "printf 'ok'",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(!result.is_error);
        assert_eq!(
            result
                .content
                .get("policy_decision")
                .and_then(serde_json::Value::as_str),
            Some("allow")
        );
        let trace = result
            .content
            .get("policy_trace")
            .and_then(serde_json::Value::as_array)
            .expect("trace should be present for trace mode");
        assert!(!trace.is_empty());
    }

    #[tokio::test]
    async fn regression_bash_tool_rejects_commands_longer_than_policy_limit() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.max_command_length = 4;
        let tool = BashTool::new(Arc::new(policy));
        let result = tool
            .execute(serde_json::json!({
                "command": "printf",
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        assert!(result.is_error);
        assert!(result.content.to_string().contains("command is too long"));
    }

    #[tokio::test]
    async fn functional_bash_tool_does_not_inherit_sensitive_environment_variables() {
        let temp = tempdir().expect("tempdir");
        let key = "TAU_TEST_SECRET_NOT_INHERITED";
        let previous = std::env::var(key).ok();
        std::env::set_var(key, "very-secret-value");

        let tool = BashTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "command": format!("printf \"${{{key}:-missing}}\""),
                "cwd": temp.path().display().to_string(),
            }))
            .await;

        if let Some(value) = previous {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }

        assert!(!result.is_error);
        assert_eq!(
            result
                .content
                .get("stdout")
                .and_then(serde_json::Value::as_str),
            Some("missing")
        );
    }

    #[tokio::test]
    async fn write_tool_blocks_paths_outside_allowed_roots() {
        let temp = tempdir().expect("tempdir");
        let outside = temp
            .path()
            .parent()
            .expect("parent path")
            .join("outside.txt");

        let tool = WriteTool::new(test_policy(temp.path()));
        let result = tool
            .execute(serde_json::json!({
                "path": outside,
                "content": "data"
            }))
            .await;

        assert!(result.is_error);
        assert!(result.content.to_string().contains("outside allowed roots"));
    }

    #[test]
    fn tool_result_text_serializes_json() {
        let result = ToolExecutionResult::ok(serde_json::json!({ "a": 1 }));
        assert!(result.as_text().contains('"'));
    }

    #[test]
    fn unit_build_spec_from_template_injects_shell_and_command_defaults() {
        let temp = tempdir().expect("tempdir");
        let cwd = temp.path();
        let template = vec![
            "sandbox-run".to_string(),
            "--cwd".to_string(),
            "{cwd}".to_string(),
        ];
        let spec = build_spec_from_command_template(&template, "/bin/sh", "printf 'ok'", cwd)
            .expect("template should build");

        assert_eq!(spec.program, "sandbox-run");
        assert_eq!(
            spec.args,
            vec![
                "--cwd".to_string(),
                cwd.display().to_string(),
                "/bin/sh".to_string(),
                "-lc".to_string(),
                "printf 'ok'".to_string(),
            ]
        );
        assert!(spec.sandboxed);
    }

    #[test]
    fn regression_resolve_sandbox_spec_force_requires_launcher_or_template() {
        let temp = tempdir().expect("tempdir");
        let mut policy = ToolPolicy::new(vec![temp.path().to_path_buf()]);
        policy.os_sandbox_mode = OsSandboxMode::Force;

        let result = resolve_sandbox_spec(&policy, "sh", "printf 'ok'", temp.path());
        if cfg!(target_os = "linux") && command_available("bwrap") {
            let spec = result.expect("expected bwrap sandbox spec");
            assert_eq!(spec.program, "bwrap");
            assert!(spec.sandboxed);
            return;
        }

        let error = result.expect_err("force mode should fail without a launcher");
        assert!(error.contains("mode 'force'"));
    }

    #[test]
    fn truncate_bytes_keeps_valid_utf8_boundaries() {
        let value = "hello🙂world";
        let truncated = truncate_bytes(value, 7);
        assert!(truncated.starts_with("hello"));
        assert!(truncated.contains("<output truncated>"));
    }

    proptest! {
        #[test]
        fn property_truncate_bytes_always_returns_valid_utf8(input in any::<String>(), limit in 0usize..256) {
            let truncated = truncate_bytes(&input, limit);
            prop_assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
            if input.len() <= limit {
                prop_assert_eq!(truncated, input);
            } else {
                prop_assert!(truncated.contains("<output truncated>"));
            }
        }

        #[test]
        fn property_leading_executable_handles_arbitrary_shellish_strings(prefix in "[A-Za-z_][A-Za-z0-9_]{0,8}", body in any::<String>()) {
            let command = format!("{prefix}=1 {body}");
            let _ = leading_executable(&command);
        }
    }

    #[test]
    fn redact_secrets_replaces_sensitive_env_values() {
        std::env::set_var("TEST_API_KEY", "secret-value-123");
        let redacted = redact_secrets("token=secret-value-123");
        assert_eq!(redacted, "token=[REDACTED]");
    }

    #[test]
    fn canonicalize_best_effort_handles_non_existing_child() {
        let temp = tempdir().expect("tempdir");
        let target = temp.path().join("a/b/c.txt");
        let canonical = canonicalize_best_effort(&target).expect("canonicalization should work");
        assert!(canonical.ends_with("a/b/c.txt"));
    }

    #[test]
    fn unit_leading_executable_parses_assignments_and_paths() {
        assert_eq!(
            leading_executable("FOO=1 /usr/bin/git status"),
            Some("git".to_string())
        );
        assert_eq!(
            leading_executable("BAR=baz cargo test"),
            Some("cargo".to_string())
        );
    }

    #[test]
    fn functional_command_allowlist_supports_prefix_patterns() {
        let allowlist = vec!["git".to_string(), "cargo-*".to_string()];
        assert!(is_command_allowed("git", &allowlist));
        assert!(is_command_allowed("cargo-nextest", &allowlist));
        assert!(!is_command_allowed("python", &allowlist));
    }

    #[test]
    fn regression_bash_profile_name_is_stable() {
        assert_eq!(
            bash_profile_name(BashCommandProfile::Permissive),
            "permissive"
        );
        assert_eq!(bash_profile_name(BashCommandProfile::Balanced), "balanced");
        assert_eq!(bash_profile_name(BashCommandProfile::Strict), "strict");
    }

    #[test]
    fn regression_os_sandbox_mode_name_is_stable() {
        assert_eq!(os_sandbox_mode_name(OsSandboxMode::Off), "off");
        assert_eq!(os_sandbox_mode_name(OsSandboxMode::Auto), "auto");
        assert_eq!(os_sandbox_mode_name(OsSandboxMode::Force), "force");
    }
}
