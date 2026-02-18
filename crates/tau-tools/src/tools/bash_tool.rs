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
    bash_profile_name, is_command_allowed, leading_executable, os_sandbox_mode_name,
    os_sandbox_policy_mode_name, redact_secrets, required_string, resolve_and_validate_path,
    resolve_sandbox_spec, truncate_bytes, validate_directory_target, PathMode, ToolPolicy,
    ToolRateLimitExceededBehavior, SANDBOX_DOCKER_UNAVAILABLE_ERROR,
    SANDBOX_FORCE_UNAVAILABLE_ERROR, SANDBOX_REQUIRED_UNAVAILABLE_ERROR,
};

const SAFE_BASH_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TMPDIR", "TMP", "TEMP",
    "TZ",
];

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

fn current_unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn resolve_rate_limit_principal(policy: &ToolPolicy) -> String {
    policy
        .rbac_principal
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(resolve_local_principal)
}

fn sandbox_reason_code(error: &str) -> &'static str {
    match error {
        SANDBOX_REQUIRED_UNAVAILABLE_ERROR => "sandbox_policy_required",
        SANDBOX_FORCE_UNAVAILABLE_ERROR => "sandbox_force_unavailable",
        SANDBOX_DOCKER_UNAVAILABLE_ERROR => "sandbox_docker_unavailable",
        _ => "sandbox_unavailable",
    }
}

pub(super) fn evaluate_tool_rate_limit_gate(
    policy: &ToolPolicy,
    tool_name: &str,
    action_payload: Value,
) -> Option<ToolExecutionResult> {
    if policy.tool_rate_limit_max_requests == 0 || policy.tool_rate_limit_window_ms == 0 {
        return None;
    }

    let principal = resolve_rate_limit_principal(policy);
    let now_unix_ms = current_unix_timestamp_ms();
    let (retry_after_ms, principal_throttle_events, throttle_events_total) =
        policy.evaluate_rate_limit(principal.as_str(), now_unix_ms)?;

    let (decision_label, reason_code, error) = match policy.tool_rate_limit_exceeded_behavior {
        ToolRateLimitExceededBehavior::Reject => (
            "reject",
            "rate_limit_rejected",
            "tool rate limit exceeded for principal",
        ),
        ToolRateLimitExceededBehavior::Defer => (
            "defer",
            "rate_limit_deferred",
            "tool execution deferred by rate limit for principal",
        ),
    };

    Some(ToolExecutionResult::error(json!({
        "policy_rule": "rate_limit",
        "policy_decision": "deny",
        "decision": decision_label,
        "reason_code": reason_code,
        "principal": principal,
        "action": format!("tool:{tool_name}"),
        "payload": action_payload,
        "max_requests": policy.tool_rate_limit_max_requests,
        "window_ms": policy.tool_rate_limit_window_ms,
        "retry_after_ms": retry_after_ms,
        "window_resets_unix_ms": now_unix_ms.saturating_add(retry_after_ms),
        "principal_throttle_events": principal_throttle_events,
        "throttle_events_total": throttle_events_total,
        "error": format!("{error} '{principal}'"),
        "hint": "retry after retry_after_ms or adjust tool policy rate-limit settings",
    })))
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

        if let Some(mut rate_limit_result) = evaluate_tool_rate_limit_gate(
            &self.policy,
            "bash",
            json!({
                "command": command.clone(),
                "cwd": cwd.as_ref().map(|value| value.display().to_string()),
            }),
        ) {
            let principal = rate_limit_result
                .content
                .get("principal")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let retry_after_ms = rate_limit_result
                .content
                .get("retry_after_ms")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            push_policy_trace(
                &mut trace,
                trace_enabled,
                "rate_limit",
                "deny",
                format!(
                    "principal '{}' exceeded rate limit; retry_after_ms={}",
                    principal, retry_after_ms
                ),
            );
            if let Some(payload) = rate_limit_result.content.as_object_mut() {
                attach_policy_trace(payload, trace_enabled, &trace, "deny");
            }
            return rate_limit_result;
        }
        push_policy_trace(
            &mut trace,
            trace_enabled,
            "rate_limit",
            "allow",
            "request is within rate-limit budget",
        );

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
                let mut payload = serde_json::Map::new();
                payload.insert("command".to_string(), json!(command));
                payload.insert(
                    "cwd".to_string(),
                    json!(cwd.as_ref().map(|value| value.display().to_string())),
                );
                payload.insert("policy_rule".to_string(), json!("os_sandbox_mode"));
                payload.insert(
                    "reason_code".to_string(),
                    json!(sandbox_reason_code(error.as_str())),
                );
                payload.insert(
                    "sandbox_mode".to_string(),
                    json!(os_sandbox_mode_name(self.policy.os_sandbox_mode)),
                );
                payload.insert(
                    "sandbox_policy_mode".to_string(),
                    json!(os_sandbox_policy_mode_name(
                        self.policy.os_sandbox_policy_mode
                    )),
                );
                payload.insert("policy_mode".to_string(), json!("sandbox"));
                payload.insert(
                    "policy_reason_code".to_string(),
                    json!(sandbox_reason_code(error.as_str())),
                );
                payload.insert("error".to_string(), json!(error));
                attach_policy_trace(&mut payload, trace_enabled, &trace, "deny");
                return ToolExecutionResult::error(Value::Object(payload));
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
            payload.insert(
                "sandbox_backend".to_string(),
                json!(sandbox_spec.backend.clone()),
            );
            payload.insert(
                "sandbox_policy_mode".to_string(),
                json!(os_sandbox_policy_mode_name(
                    self.policy.os_sandbox_policy_mode
                )),
            );
            payload.insert("policy_mode".to_string(), json!("none"));
            payload.insert("policy_reason_code".to_string(), json!("none"));
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
        payload.insert("sandbox_backend".to_string(), json!(sandbox_spec.backend));
        payload.insert(
            "sandbox_policy_mode".to_string(),
            json!(os_sandbox_policy_mode_name(
                self.policy.os_sandbox_policy_mode
            )),
        );
        payload.insert("policy_mode".to_string(), json!("none"));
        payload.insert("policy_reason_code".to_string(), json!("none"));
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
