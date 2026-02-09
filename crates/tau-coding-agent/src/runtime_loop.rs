use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use tau_agent_core::Agent;
use tau_ai::StreamDeltaHandler;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::{
    apply_extension_message_transforms, current_unix_timestamp_ms, dispatch_extension_runtime_hook,
    ensure_non_empty_text, handle_command_with_session_import_mode, persist_messages,
    print_assistant_messages, run_plan_first_prompt, run_plan_first_prompt_with_policy_context,
    run_plan_first_prompt_with_policy_context_and_routing, Cli, CliOrchestratorMode, CommandAction,
    CommandExecutionContext, MultiAgentRouteTable, RenderOptions, SessionRuntime,
};

const EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptRunStatus {
    Completed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeExtensionHooksConfig {
    pub(crate) enabled: bool,
    pub(crate) root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeHookRunStatus {
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

impl RuntimeHookRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InteractiveRuntimeConfig<'a> {
    pub(crate) turn_timeout_ms: u64,
    pub(crate) render_options: RenderOptions,
    pub(crate) extension_runtime_hooks: &'a RuntimeExtensionHooksConfig,
    pub(crate) orchestrator_mode: CliOrchestratorMode,
    pub(crate) orchestrator_max_plan_steps: usize,
    pub(crate) orchestrator_max_delegated_steps: usize,
    pub(crate) orchestrator_max_executor_response_chars: usize,
    pub(crate) orchestrator_max_delegated_step_response_chars: usize,
    pub(crate) orchestrator_max_delegated_total_response_chars: usize,
    pub(crate) orchestrator_delegate_steps: bool,
    pub(crate) orchestrator_route_table: &'a MultiAgentRouteTable,
    pub(crate) orchestrator_route_trace_log: Option<&'a Path>,
    pub(crate) command_context: CommandExecutionContext<'a>,
}

pub(crate) async fn run_prompt(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<()> {
    let status = run_prompt_with_runtime_hooks(
        agent,
        session_runtime,
        prompt,
        turn_timeout_ms,
        tokio::signal::ctrl_c(),
        render_options,
        extension_runtime_hooks,
    )
    .await?;
    report_prompt_status(status);
    Ok(())
}

pub(crate) async fn run_interactive(
    mut agent: Agent,
    mut session_runtime: Option<SessionRuntime>,
    config: InteractiveRuntimeConfig<'_>,
) -> Result<()> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        print!("tau> ");
        std::io::stdout()
            .flush()
            .context("failed to flush stdout")?;

        let Some(line) = lines.next_line().await? else {
            break;
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('/') {
            if handle_command_with_session_import_mode(
                trimmed,
                &mut agent,
                &mut session_runtime,
                config.command_context.tool_policy_json,
                config.command_context.session_import_mode,
                config.command_context.profile_defaults,
                config.command_context.skills_command_config,
                config.command_context.auth_command_config,
                config.command_context.model_catalog,
                config.command_context.extension_commands,
            )? == CommandAction::Exit
            {
                break;
            }
            continue;
        }

        if config.orchestrator_mode == CliOrchestratorMode::PlanFirst {
            run_plan_first_prompt_with_runtime_hooks(
                &mut agent,
                &mut session_runtime,
                trimmed,
                config.turn_timeout_ms,
                config.render_options,
                config.orchestrator_max_plan_steps,
                config.orchestrator_max_delegated_steps,
                config.orchestrator_max_executor_response_chars,
                config.orchestrator_max_delegated_step_response_chars,
                config.orchestrator_max_delegated_total_response_chars,
                config.orchestrator_delegate_steps,
                config.orchestrator_route_table,
                config.orchestrator_route_trace_log,
                config.command_context.tool_policy_json,
                config.extension_runtime_hooks,
            )
            .await?;
        } else {
            let status = run_prompt_with_runtime_hooks(
                &mut agent,
                &mut session_runtime,
                trimmed,
                config.turn_timeout_ms,
                tokio::signal::ctrl_c(),
                config.render_options,
                config.extension_runtime_hooks,
            )
            .await?;
            report_prompt_status(status);
        }
    }

    Ok(())
}

async fn run_prompt_with_runtime_hooks<F>(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    cancellation_signal: F,
    render_options: RenderOptions,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<PromptRunStatus>
where
    F: Future,
{
    let effective_prompt = apply_runtime_message_transform(extension_runtime_hooks, prompt);
    dispatch_runtime_hook(
        extension_runtime_hooks,
        "run-start",
        build_runtime_hook_payload("run-start", &effective_prompt, turn_timeout_ms, None, None),
    );

    let result = run_prompt_with_cancellation(
        agent,
        session_runtime,
        &effective_prompt,
        turn_timeout_ms,
        cancellation_signal,
        render_options,
    )
    .await;

    match result {
        Ok(status) => {
            let run_status = match status {
                PromptRunStatus::Completed => RuntimeHookRunStatus::Completed,
                PromptRunStatus::Cancelled => RuntimeHookRunStatus::Cancelled,
                PromptRunStatus::TimedOut => RuntimeHookRunStatus::TimedOut,
            };
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(run_status),
                    None,
                ),
            );
            Ok(status)
        }
        Err(error) => {
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(RuntimeHookRunStatus::Failed),
                    Some(error.to_string()),
                ),
            );
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_plan_first_prompt_with_runtime_hooks(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    render_options: RenderOptions,
    orchestrator_max_plan_steps: usize,
    orchestrator_max_delegated_steps: usize,
    orchestrator_max_executor_response_chars: usize,
    orchestrator_max_delegated_step_response_chars: usize,
    orchestrator_max_delegated_total_response_chars: usize,
    orchestrator_delegate_steps: bool,
    orchestrator_route_table: &MultiAgentRouteTable,
    orchestrator_route_trace_log: Option<&Path>,
    tool_policy_json: &serde_json::Value,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<()> {
    let effective_prompt = apply_runtime_message_transform(extension_runtime_hooks, prompt);
    dispatch_runtime_hook(
        extension_runtime_hooks,
        "run-start",
        build_runtime_hook_payload("run-start", &effective_prompt, turn_timeout_ms, None, None),
    );

    let policy_context = if orchestrator_delegate_steps {
        Some(
            render_orchestrator_policy_inheritance_context(
                tool_policy_json,
                extension_runtime_hooks,
            )
            .context(
                "plan-first orchestrator failed: delegated policy inheritance context build failed",
            )?,
        )
    } else {
        None
    };

    let uses_default_router = orchestrator_route_table == &MultiAgentRouteTable::default();
    let result = if uses_default_router && orchestrator_route_trace_log.is_none() {
        if let Some(policy_context) = policy_context.as_deref() {
            run_plan_first_prompt_with_policy_context(
                agent,
                session_runtime,
                &effective_prompt,
                turn_timeout_ms,
                render_options,
                orchestrator_max_plan_steps,
                orchestrator_max_delegated_steps,
                orchestrator_max_executor_response_chars,
                orchestrator_max_delegated_step_response_chars,
                orchestrator_max_delegated_total_response_chars,
                orchestrator_delegate_steps,
                Some(policy_context),
            )
            .await
        } else {
            run_plan_first_prompt(
                agent,
                session_runtime,
                &effective_prompt,
                turn_timeout_ms,
                render_options,
                orchestrator_max_plan_steps,
                orchestrator_max_delegated_steps,
                orchestrator_max_executor_response_chars,
                orchestrator_max_delegated_step_response_chars,
                orchestrator_max_delegated_total_response_chars,
                orchestrator_delegate_steps,
            )
            .await
        }
    } else {
        run_plan_first_prompt_with_policy_context_and_routing(
            agent,
            session_runtime,
            &effective_prompt,
            turn_timeout_ms,
            render_options,
            orchestrator_max_plan_steps,
            orchestrator_max_delegated_steps,
            orchestrator_max_executor_response_chars,
            orchestrator_max_delegated_step_response_chars,
            orchestrator_max_delegated_total_response_chars,
            orchestrator_delegate_steps,
            policy_context.as_deref(),
            orchestrator_route_table,
            orchestrator_route_trace_log,
        )
        .await
    };

    match result {
        Ok(()) => {
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(RuntimeHookRunStatus::Completed),
                    None,
                ),
            );
            Ok(())
        }
        Err(error) => {
            dispatch_runtime_hook(
                extension_runtime_hooks,
                "run-end",
                build_runtime_hook_payload(
                    "run-end",
                    &effective_prompt,
                    turn_timeout_ms,
                    Some(RuntimeHookRunStatus::Failed),
                    Some(error.to_string()),
                ),
            );
            Err(error)
        }
    }
}

fn apply_runtime_message_transform(config: &RuntimeExtensionHooksConfig, prompt: &str) -> String {
    if !config.enabled {
        return prompt.to_string();
    }
    let transform = apply_extension_message_transforms(&config.root, prompt);
    for diagnostic in transform.diagnostics {
        eprintln!("{diagnostic}");
    }
    transform.prompt
}

fn render_orchestrator_policy_inheritance_context(
    tool_policy_json: &serde_json::Value,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) -> Result<String> {
    let object = tool_policy_json
        .as_object()
        .ok_or_else(|| anyhow!("tool policy JSON must be an object"))?;
    let schema_version = object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("tool policy JSON missing numeric field 'schema_version'"))?;
    let preset = object
        .get("preset")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'preset'"))?;
    let bash_profile = object
        .get("bash_profile")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'bash_profile'"))?;
    let os_sandbox_mode = object
        .get("os_sandbox_mode")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow!("tool policy JSON missing string field 'os_sandbox_mode'"))?;
    let bash_dry_run = object
        .get("bash_dry_run")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow!("tool policy JSON missing boolean field 'bash_dry_run'"))?;
    let enforce_regular_files = object
        .get("enforce_regular_files")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| anyhow!("tool policy JSON missing boolean field 'enforce_regular_files'"))?;
    let max_command_length = object
        .get("max_command_length")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow!("tool policy JSON missing numeric field 'max_command_length'"))?;
    let max_command_output_bytes = object
        .get("max_command_output_bytes")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            anyhow!("tool policy JSON missing numeric field 'max_command_output_bytes'")
        })?;
    let allowed_roots = object
        .get("allowed_roots")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("tool policy JSON missing array field 'allowed_roots'"))?;
    let allowed_commands = object
        .get("allowed_commands")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| anyhow!("tool policy JSON missing array field 'allowed_commands'"))?;

    Ok(format!(
        "schema_version={schema_version};preset={preset};bash_profile={bash_profile};os_sandbox_mode={os_sandbox_mode};bash_dry_run={bash_dry_run};enforce_regular_files={enforce_regular_files};max_command_length={max_command_length};max_command_output_bytes={max_command_output_bytes};allowed_roots={};allowed_commands={};extension_runtime_hooks_enabled={}",
        allowed_roots.len(),
        allowed_commands.len(),
        extension_runtime_hooks.enabled,
    ))
}

fn dispatch_runtime_hook(
    config: &RuntimeExtensionHooksConfig,
    hook: &str,
    payload: serde_json::Value,
) {
    if !config.enabled {
        return;
    }
    let summary = dispatch_extension_runtime_hook(&config.root, hook, &payload);
    for diagnostic in summary.diagnostics {
        eprintln!("{diagnostic}");
    }
}

fn build_runtime_hook_payload(
    hook: &str,
    prompt: &str,
    turn_timeout_ms: u64,
    status: Option<RuntimeHookRunStatus>,
    error: Option<String>,
) -> serde_json::Value {
    let mut data = serde_json::Map::new();
    data.insert(
        "prompt".to_string(),
        serde_json::Value::String(prompt.to_string()),
    );
    data.insert(
        "turn_timeout_ms".to_string(),
        serde_json::Value::Number(turn_timeout_ms.into()),
    );
    if let Some(status) = status {
        data.insert(
            "status".to_string(),
            serde_json::Value::String(status.as_str().to_string()),
        );
    }
    if let Some(error) = error {
        data.insert("error".to_string(), serde_json::Value::String(error));
    }

    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema_version".to_string(),
        serde_json::Value::Number(EXTENSION_HOOK_PAYLOAD_SCHEMA_VERSION.into()),
    );
    payload.insert(
        "hook".to_string(),
        serde_json::Value::String(hook.to_string()),
    );
    payload.insert(
        "emitted_at_ms".to_string(),
        serde_json::Value::Number(current_unix_timestamp_ms().into()),
    );
    payload.insert("data".to_string(), serde_json::Value::Object(data.clone()));
    for (key, value) in data {
        payload.insert(key, value);
    }
    serde_json::Value::Object(payload)
}

pub(crate) async fn run_prompt_with_cancellation<F>(
    agent: &mut Agent,
    session_runtime: &mut Option<SessionRuntime>,
    prompt: &str,
    turn_timeout_ms: u64,
    cancellation_signal: F,
    render_options: RenderOptions,
) -> Result<PromptRunStatus>
where
    F: Future,
{
    let checkpoint = agent.messages().to_vec();
    let streamed_output = Arc::new(AtomicBool::new(false));
    let stream_delta_handler = if render_options.stream_output {
        let streamed_output = streamed_output.clone();
        let stream_delay_ms = render_options.stream_delay_ms;
        Some(Arc::new(move |delta: String| {
            if delta.is_empty() {
                return;
            }
            streamed_output.store(true, Ordering::Relaxed);
            print!("{delta}");
            let _ = std::io::stdout().flush();
            if stream_delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(stream_delay_ms));
            }
        }) as StreamDeltaHandler)
    } else {
        None
    };
    tokio::pin!(cancellation_signal);

    enum PromptOutcome<T> {
        Result(T),
        Cancelled,
        TimedOut,
    }

    let prompt_result = if turn_timeout_ms == 0 {
        tokio::select! {
            result = agent.prompt_with_stream(prompt, stream_delta_handler.clone()) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
        }
    } else {
        let timeout = tokio::time::sleep(Duration::from_millis(turn_timeout_ms));
        tokio::pin!(timeout);
        tokio::select! {
            result = agent.prompt_with_stream(prompt, stream_delta_handler.clone()) => PromptOutcome::Result(result),
            _ = &mut cancellation_signal => PromptOutcome::Cancelled,
            _ = &mut timeout => PromptOutcome::TimedOut,
        }
    };

    let prompt_result = match prompt_result {
        PromptOutcome::Result(result) => result,
        PromptOutcome::Cancelled => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::Cancelled);
        }
        PromptOutcome::TimedOut => {
            agent.replace_messages(checkpoint);
            return Ok(PromptRunStatus::TimedOut);
        }
    };

    let new_messages = prompt_result?;
    persist_messages(session_runtime, &new_messages)?;
    print_assistant_messages(
        &new_messages,
        render_options,
        streamed_output.load(Ordering::Relaxed),
    );
    Ok(PromptRunStatus::Completed)
}

pub(crate) fn resolve_prompt_input(cli: &Cli) -> Result<Option<String>> {
    if let Some(prompt) = &cli.prompt {
        return Ok(Some(prompt.clone()));
    }

    if let Some(path) = cli.prompt_template_file.as_ref() {
        let template = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read prompt template file {}", path.display()))?;
        let template =
            ensure_non_empty_text(template, format!("prompt template file {}", path.display()))?;
        let vars = parse_prompt_template_vars(&cli.prompt_template_var)?;
        let rendered = render_prompt_template(&template, &vars)?;
        return Ok(Some(ensure_non_empty_text(
            rendered,
            format!("rendered prompt template {}", path.display()),
        )?));
    }

    let Some(path) = cli.prompt_file.as_ref() else {
        return Ok(None);
    };

    if path == std::path::Path::new("-") {
        let mut prompt = String::new();
        std::io::stdin()
            .read_to_string(&mut prompt)
            .context("failed to read prompt from stdin")?;
        return Ok(Some(ensure_non_empty_text(
            prompt,
            "stdin prompt".to_string(),
        )?));
    }

    let prompt = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read prompt file {}", path.display()))?;

    Ok(Some(ensure_non_empty_text(
        prompt,
        format!("prompt file {}", path.display()),
    )?))
}

fn parse_prompt_template_vars(raw_specs: &[String]) -> Result<BTreeMap<String, String>> {
    let mut vars = BTreeMap::new();
    for raw_spec in raw_specs {
        let spec = raw_spec.trim();
        let (raw_key, value) = spec.split_once('=').ok_or_else(|| {
            anyhow!(
                "invalid --prompt-template-var '{}', expected key=value",
                raw_spec
            )
        })?;
        let key = raw_key.trim();
        if key.is_empty() {
            bail!(
                "invalid --prompt-template-var '{}', key must be non-empty",
                raw_spec
            );
        }
        if vars.contains_key(key) {
            bail!("duplicate --prompt-template-var key '{}'", key);
        }
        vars.insert(key.to_string(), value.to_string());
    }
    Ok(vars)
}

fn render_prompt_template(template: &str, vars: &BTreeMap<String, String>) -> Result<String> {
    let mut rendered = String::new();
    let mut used_keys = BTreeSet::new();
    let mut cursor = 0_usize;

    while let Some(start_rel) = template[cursor..].find("{{") {
        let start = cursor + start_rel;
        rendered.push_str(&template[cursor..start]);
        let placeholder_start = start + 2;
        let end_rel = template[placeholder_start..]
            .find("}}")
            .ok_or_else(|| anyhow!("prompt template contains unterminated placeholder"))?;
        let end = placeholder_start + end_rel;
        let key = template[placeholder_start..end].trim();
        if key.is_empty() {
            bail!("prompt template contains empty placeholder");
        }
        let value = vars.get(key).ok_or_else(|| {
            anyhow!(
                "prompt template placeholder '{}' is missing a --prompt-template-var value",
                key
            )
        })?;
        rendered.push_str(value);
        used_keys.insert(key.to_string());
        cursor = end + 2;
    }
    rendered.push_str(&template[cursor..]);

    let unused = vars
        .keys()
        .filter(|key| !used_keys.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    if !unused.is_empty() {
        bail!("unused --prompt-template-var keys: {}", unused.join(", "));
    }

    Ok(rendered)
}

fn report_prompt_status(status: PromptRunStatus) {
    if status == PromptRunStatus::Cancelled {
        println!("\nrequest cancelled\n");
    } else if status == PromptRunStatus::TimedOut {
        println!("\nrequest timed out\n");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_runtime_message_transform, build_runtime_hook_payload,
        render_orchestrator_policy_inheritance_context, RuntimeExtensionHooksConfig,
        RuntimeHookRunStatus,
    };
    use tempfile::tempdir;

    #[test]
    fn unit_build_runtime_hook_payload_includes_status_and_error() {
        let payload = build_runtime_hook_payload(
            "run-end",
            "hello",
            5000,
            Some(RuntimeHookRunStatus::Failed),
            Some("network timeout".to_string()),
        );
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "run-end");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["prompt"], "hello");
        assert_eq!(payload["data"]["turn_timeout_ms"], 5000);
        assert_eq!(payload["data"]["status"], "failed");
        assert_eq!(payload["data"]["error"], "network timeout");
    }

    #[test]
    fn regression_build_runtime_hook_payload_omits_optional_fields_when_unset() {
        let payload = build_runtime_hook_payload("run-start", "hello", 0, None, None);
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "run-start");
        assert_eq!(payload["data"]["prompt"], "hello");
        assert_eq!(payload["data"]["turn_timeout_ms"], 0);
        assert!(payload["data"].get("status").is_none());
        assert!(payload["data"].get("error").is_none());
    }

    #[test]
    fn unit_apply_runtime_message_transform_disabled_returns_original_prompt() {
        let config = RuntimeExtensionHooksConfig {
            enabled: false,
            root: std::path::PathBuf::from(".tau/extensions"),
        };
        let transformed = apply_runtime_message_transform(&config, "hello");
        assert_eq!(transformed, "hello");
    }

    #[test]
    fn regression_apply_runtime_message_transform_missing_root_returns_original_prompt() {
        let temp = tempdir().expect("tempdir");
        let missing_root = temp.path().join("missing");
        let config = RuntimeExtensionHooksConfig {
            enabled: true,
            root: missing_root,
        };
        let transformed = apply_runtime_message_transform(&config, "hello");
        assert_eq!(transformed, "hello");
    }

    #[test]
    fn unit_render_orchestrator_policy_inheritance_context_is_deterministic() {
        let config = RuntimeExtensionHooksConfig {
            enabled: true,
            root: std::path::PathBuf::from(".tau/extensions"),
        };
        let tool_policy_json = serde_json::json!({
            "schema_version": 1,
            "preset": "balanced",
            "bash_profile": "balanced",
            "os_sandbox_mode": "off",
            "bash_dry_run": false,
            "enforce_regular_files": true,
            "max_command_length": 4096,
            "max_command_output_bytes": 16000,
            "allowed_roots": ["/tmp/project"],
            "allowed_commands": ["cat", "ls"],
        });
        let context = render_orchestrator_policy_inheritance_context(&tool_policy_json, &config)
            .expect("policy context should render");
        assert_eq!(
            context,
            "schema_version=1;preset=balanced;bash_profile=balanced;os_sandbox_mode=off;bash_dry_run=false;enforce_regular_files=true;max_command_length=4096;max_command_output_bytes=16000;allowed_roots=1;allowed_commands=2;extension_runtime_hooks_enabled=true"
        );
    }

    #[test]
    fn regression_render_orchestrator_policy_inheritance_context_fails_closed_on_invalid_payload() {
        let config = RuntimeExtensionHooksConfig {
            enabled: false,
            root: std::path::PathBuf::from(".tau/extensions"),
        };
        let invalid_tool_policy_json = serde_json::json!({
            "preset": "balanced",
            "allowed_roots": ["/tmp/project"],
            "allowed_commands": ["cat"],
        });
        let error =
            render_orchestrator_policy_inheritance_context(&invalid_tool_policy_json, &config)
                .expect_err("missing required fields should fail");
        assert!(error
            .to_string()
            .contains("missing numeric field 'schema_version'"));
    }
}
