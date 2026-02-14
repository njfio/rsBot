use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use serde_json::{json, Value};
use tau_access::ApprovalAction;
use tau_agent_core::{Agent, AgentTool, ToolExecutionResult};
use tau_ai::ToolDefinition;
use tau_extensions::{execute_extension_registered_tool, ExtensionRegisteredTool};

mod bash_tool;
mod session_tools;
mod tool_policy;

use bash_tool::{evaluate_tool_approval_gate, evaluate_tool_rbac_gate};

pub use bash_tool::BashTool;
pub use session_tools::{
    SessionsHistoryTool, SessionsListTool, SessionsSearchTool, SessionsSendTool, SessionsStatsTool,
};
pub use tool_policy::{
    tool_policy_preset_name, BashCommandProfile, OsSandboxMode, ToolPolicy, ToolPolicyPreset,
};

#[cfg(test)]
use bash_tool::{
    bash_profile_name, build_spec_from_command_template, command_available, is_command_allowed,
    leading_executable, os_sandbox_mode_name, redact_secrets, resolve_sandbox_spec, truncate_bytes,
};
#[cfg(test)]
use session_tools::is_session_candidate_path;

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

#[cfg(test)]
mod tests;
