use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use pi_agent_core::{Agent, AgentTool, ToolExecutionResult};
use pi_ai::ToolDefinition;
use serde_json::{json, Value};
use tokio::{process::Command, time::timeout};

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

#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub allowed_roots: Vec<PathBuf>,
    pub max_file_read_bytes: usize,
    pub max_command_output_bytes: usize,
    pub bash_timeout_ms: u64,
    pub max_command_length: usize,
    pub allow_command_newlines: bool,
    pub bash_profile: BashCommandProfile,
    pub allowed_commands: Vec<String>,
}

impl ToolPolicy {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self {
            allowed_roots,
            max_file_read_bytes: 1_000_000,
            max_command_output_bytes: 16_000,
            bash_timeout_ms: 120_000,
            max_command_length: 4_096,
            allow_command_newlines: false,
            bash_profile: BashCommandProfile::Balanced,
            allowed_commands: BALANCED_COMMAND_ALLOWLIST
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
        }
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
}

pub fn register_builtin_tools(agent: &mut Agent, policy: ToolPolicy) {
    let policy = Arc::new(policy);
    agent.register_tool(ReadTool::new(policy.clone()));
    agent.register_tool(WriteTool::new(policy.clone()));
    agent.register_tool(EditTool::new(policy.clone()));
    agent.register_tool(BashTool::new(policy));
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

        let metadata = match tokio::fs::metadata(&resolved).await {
            Ok(metadata) => metadata,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved,
                    "error": error.to_string(),
                }))
            }
        };

        if metadata.len() as usize > self.policy.max_file_read_bytes {
            return ToolExecutionResult::error(json!({
                "path": resolved,
                "error": format!(
                    "file is too large ({} bytes), limit is {} bytes",
                    metadata.len(),
                    self.policy.max_file_read_bytes
                ),
            }));
        }

        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => ToolExecutionResult::ok(json!({
                "path": resolved,
                "content": content,
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "path": resolved,
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

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };

        if let Some(parent) = Path::new(&resolved).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(error) = tokio::fs::create_dir_all(parent).await {
                    return ToolExecutionResult::error(json!({
                        "path": resolved,
                        "error": format!("failed to create parent directory: {error}"),
                    }));
                }
            }
        }

        match tokio::fs::write(&resolved, content.as_bytes()).await {
            Ok(()) => ToolExecutionResult::ok(json!({
                "path": resolved,
                "bytes_written": content.len(),
            })),
            Err(error) => ToolExecutionResult::error(json!({
                "path": resolved,
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

        let resolved = match resolve_and_validate_path(&path, &self.policy, PathMode::Write) {
            Ok(path) => path,
            Err(error) => {
                return ToolExecutionResult::error(json!({ "path": path, "error": error }))
            }
        };

        let replace_all = arguments
            .get("all")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let source = match tokio::fs::read_to_string(&resolved).await {
            Ok(source) => source,
            Err(error) => {
                return ToolExecutionResult::error(json!({
                    "path": resolved,
                    "error": error.to_string(),
                }))
            }
        };

        let occurrences = source.matches(&find).count();
        if occurrences == 0 {
            return ToolExecutionResult::error(json!({
                "path": resolved,
                "error": "target string not found",
            }));
        }

        let updated = if replace_all {
            source.replace(&find, &replace)
        } else {
            source.replacen(&find, &replace, 1)
        };

        if let Err(error) = tokio::fs::write(&resolved, updated.as_bytes()).await {
            return ToolExecutionResult::error(json!({
                "path": resolved,
                "error": error.to_string(),
            }));
        }

        let replacements = if replace_all { occurrences } else { 1 };
        ToolExecutionResult::ok(json!({
            "path": resolved,
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

        let command_length = command.chars().count();
        if command_length > self.policy.max_command_length {
            return ToolExecutionResult::error(json!({
                "command": command,
                "error": format!(
                    "command is too long ({} chars), limit is {} chars",
                    command_length,
                    self.policy.max_command_length
                ),
            }));
        }

        if !self.policy.allow_command_newlines && (command.contains('\n') || command.contains('\r'))
        {
            return ToolExecutionResult::error(json!({
                "command": command,
                "error": "multiline commands are disabled by policy",
            }));
        }

        if !self.policy.allowed_commands.is_empty() {
            let Some(executable) = leading_executable(&command) else {
                return ToolExecutionResult::error(json!({
                    "command": command,
                    "error": "unable to parse command executable",
                }));
            };
            if !is_command_allowed(&executable, &self.policy.allowed_commands) {
                return ToolExecutionResult::error(json!({
                    "command": command,
                    "error": format!(
                        "command '{}' is not allowed by '{}' bash profile",
                        executable,
                        bash_profile_name(self.policy.bash_profile),
                    ),
                    "allowed_commands": self.policy.allowed_commands,
                }));
            }
        }

        let cwd = match arguments.get("cwd").and_then(Value::as_str) {
            Some(cwd) => match resolve_and_validate_path(cwd, &self.policy, PathMode::Read) {
                Ok(path) => Some(path),
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "cwd": cwd,
                        "error": error,
                    }))
                }
            },
            None => None,
        };

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        let mut command_builder = Command::new(shell);
        command_builder.arg("-lc").arg(&command);
        command_builder.kill_on_drop(true);
        command_builder.env_clear();
        for key in SAFE_BASH_ENV_VARS {
            if let Ok(value) = std::env::var(key) {
                command_builder.env(key, value);
            }
        }
        command_builder.env("PI_SANDBOXED", "1");

        if let Some(cwd) = &cwd {
            command_builder.current_dir(cwd);
        }

        let timeout_duration = Duration::from_millis(self.policy.bash_timeout_ms.max(1));
        let output = match timeout(timeout_duration, command_builder.output()).await {
            Ok(result) => match result {
                Ok(output) => output,
                Err(error) => {
                    return ToolExecutionResult::error(json!({
                        "command": command,
                        "cwd": cwd,
                        "error": error.to_string(),
                    }))
                }
            },
            Err(_) => {
                return ToolExecutionResult::error(json!({
                    "command": command,
                    "cwd": cwd,
                    "error": format!("command timed out after {} ms", self.policy.bash_timeout_ms),
                }))
            }
        };

        let stdout = redact_secrets(&String::from_utf8_lossy(&output.stdout));
        let stderr = redact_secrets(&String::from_utf8_lossy(&output.stderr));
        ToolExecutionResult::ok(json!({
            "command": command,
            "cwd": cwd,
            "status": output.status.code(),
            "success": output.status.success(),
            "stdout": truncate_bytes(&stdout, self.policy.max_command_output_bytes),
            "stderr": truncate_bytes(&stderr, self.policy.max_command_output_bytes),
        }))
    }
}

#[derive(Debug, Clone, Copy)]
enum PathMode {
    Read,
    Write,
}

fn resolve_and_validate_path(
    user_path: &str,
    policy: &ToolPolicy,
    mode: PathMode,
) -> Result<String, String> {
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

    if matches!(mode, PathMode::Read) && !canonical.exists() {
        return Err(format!("path '{}' does not exist", canonical.display()));
    }

    Ok(canonical.display().to_string())
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
    use std::sync::Arc;

    use proptest::prelude::*;
    use tempfile::tempdir;

    use super::{
        bash_profile_name, canonicalize_best_effort, is_command_allowed, leading_executable,
        redact_secrets, truncate_bytes, AgentTool, BashCommandProfile, BashTool, EditTool,
        ToolExecutionResult, ToolPolicy, WriteTool,
    };

    fn test_policy(path: &Path) -> Arc<ToolPolicy> {
        Arc::new(ToolPolicy::new(vec![path.to_path_buf()]))
    }

    use std::path::Path;

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
        let key = "PI_TEST_SECRET_NOT_INHERITED";
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
}
