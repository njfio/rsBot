use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};
use tau_access::{
    authorize_tool_for_principal, authorize_tool_for_principal_with_policy_path,
    evaluate_approval_gate, resolve_local_principal, ApprovalAction, ApprovalGateResult,
    RbacDecision,
};
use tau_agent_core::{AgentTool, ToolExecutionResult};
use tau_ai::ToolDefinition;
use tau_extensions::evaluate_extension_policy_override;
use tokio::{process::Command, time::timeout};

use super::{
    required_string, resolve_and_validate_path, validate_directory_target, BashCommandProfile,
    OsSandboxMode, PathMode, ToolPolicy,
};

const SAFE_BASH_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMPDIR", "TMP", "TEMP",
    "TZ",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BashSandboxSpec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) sandboxed: bool,
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

pub(super) fn evaluate_tool_approval_gate(action: ApprovalAction) -> Option<ToolExecutionResult> {
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

pub(super) fn evaluate_tool_rbac_gate(
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
pub(super) fn resolve_sandbox_spec(
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

pub(super) fn build_spec_from_command_template(
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
pub(super) fn command_available(command: &str) -> bool {
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

pub(super) fn leading_executable(command: &str) -> Option<String> {
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

pub(super) fn is_command_allowed(executable: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|entry| {
        if let Some(prefix) = entry.strip_suffix('*') {
            executable.starts_with(prefix)
        } else {
            executable == entry
        }
    })
}

pub(super) fn bash_profile_name(profile: BashCommandProfile) -> &'static str {
    match profile {
        BashCommandProfile::Permissive => "permissive",
        BashCommandProfile::Balanced => "balanced",
        BashCommandProfile::Strict => "strict",
    }
}

pub(super) fn os_sandbox_mode_name(mode: OsSandboxMode) -> &'static str {
    match mode {
        OsSandboxMode::Off => "off",
        OsSandboxMode::Auto => "auto",
        OsSandboxMode::Force => "force",
    }
}

pub(super) fn truncate_bytes(value: &str, limit: usize) -> String {
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

pub(super) fn redact_secrets(text: &str) -> String {
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
