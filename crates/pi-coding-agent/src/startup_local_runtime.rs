use super::*;
use crate::extension_manifest::{
    discover_extension_runtime_registrations, ExtensionRuntimeRegistrationSummary,
};

const EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION: u32 = 1;

pub(crate) struct LocalRuntimeConfig<'a> {
    pub(crate) cli: &'a Cli,
    pub(crate) client: Arc<dyn LlmClient>,
    pub(crate) model_ref: &'a ModelRef,
    pub(crate) fallback_model_refs: &'a [ModelRef],
    pub(crate) model_catalog: &'a ModelCatalog,
    pub(crate) system_prompt: &'a str,
    pub(crate) tool_policy: ToolPolicy,
    pub(crate) tool_policy_json: &'a Value,
    pub(crate) render_options: RenderOptions,
    pub(crate) skills_dir: &'a Path,
    pub(crate) skills_lock_path: &'a Path,
}

pub(crate) async fn run_local_runtime(config: LocalRuntimeConfig<'_>) -> Result<()> {
    let LocalRuntimeConfig {
        cli,
        client,
        model_ref,
        fallback_model_refs,
        model_catalog,
        system_prompt,
        tool_policy,
        tool_policy_json,
        render_options,
        skills_dir,
        skills_lock_path,
    } = config;

    let mut agent = Agent::new(
        client,
        AgentConfig {
            model: model_ref.model.clone(),
            system_prompt: system_prompt.to_string(),
            max_turns: cli.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
        },
    );
    tools::register_builtin_tools(&mut agent, tool_policy);
    if let Some(path) = cli.tool_audit_log.clone() {
        let logger = ToolAuditLogger::open(path)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("tool audit logger error: {error}");
            }
        });
    }
    if let Some(path) = cli.telemetry_log.clone() {
        let logger =
            PromptTelemetryLogger::open(path, model_ref.provider.as_str(), &model_ref.model)?;
        agent.subscribe(move |event| {
            if let Err(error) = logger.log_event(event) {
                eprintln!("telemetry logger error: {error}");
            }
        });
    }
    let mut session_runtime = if cli.no_session {
        None
    } else {
        Some(initialize_session(&mut agent, cli, system_prompt)?)
    };

    if cli.json_events {
        agent.subscribe(|event| {
            let value = event_to_json(event);
            println!("{value}");
        });
    }
    let extension_runtime_hooks = RuntimeExtensionHooksConfig {
        enabled: cli.extension_runtime_hooks,
        root: cli.extension_runtime_root.clone(),
    };
    let extension_runtime_registrations = if extension_runtime_hooks.enabled {
        discover_extension_runtime_registrations(&extension_runtime_hooks.root)
    } else {
        ExtensionRuntimeRegistrationSummary {
            root: extension_runtime_hooks.root.clone(),
            discovered: 0,
            registered_tools: Vec::new(),
            registered_commands: Vec::new(),
            skipped_invalid: 0,
            skipped_unsupported_runtime: 0,
            skipped_permission_denied: 0,
            skipped_name_conflict: 0,
            diagnostics: Vec::new(),
        }
    };
    tools::register_extension_tools(
        &mut agent,
        &extension_runtime_registrations.registered_tools,
    );
    for diagnostic in &extension_runtime_registrations.diagnostics {
        eprintln!("{diagnostic}");
    }
    register_runtime_extension_tool_hook_subscriber(&mut agent, &extension_runtime_hooks);

    if let Some(prompt) = resolve_prompt_input(cli)? {
        if cli.orchestrator_mode == CliOrchestratorMode::PlanFirst {
            run_plan_first_prompt_with_runtime_hooks(
                &mut agent,
                &mut session_runtime,
                &prompt,
                cli.turn_timeout_ms,
                render_options,
                cli.orchestrator_max_plan_steps,
                cli.orchestrator_max_executor_response_chars,
                cli.orchestrator_delegate_steps,
                &extension_runtime_hooks,
            )
            .await?;
        } else {
            run_prompt(
                &mut agent,
                &mut session_runtime,
                &prompt,
                cli.turn_timeout_ms,
                render_options,
                &extension_runtime_hooks,
            )
            .await?;
        }
        return Ok(());
    }

    let skills_sync_command_config = SkillsSyncCommandConfig {
        skills_dir: skills_dir.to_path_buf(),
        default_lock_path: skills_lock_path.to_path_buf(),
        default_trust_root_path: cli.skill_trust_root_file.clone(),
        doctor_config: {
            let mut doctor_config =
                build_doctor_command_config(cli, model_ref, fallback_model_refs, skills_lock_path);
            doctor_config.skills_dir = skills_dir.to_path_buf();
            doctor_config.skills_lock_path = skills_lock_path.to_path_buf();
            doctor_config
        },
    };
    let profile_defaults = build_profile_defaults(cli);
    let auth_command_config = build_auth_command_config(cli);
    let command_context = CommandExecutionContext {
        tool_policy_json,
        session_import_mode: cli.session_import_mode.into(),
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_sync_command_config,
        auth_command_config: &auth_command_config,
        model_catalog,
        extension_commands: &extension_runtime_registrations.registered_commands,
    };
    let interactive_config = InteractiveRuntimeConfig {
        turn_timeout_ms: cli.turn_timeout_ms,
        render_options,
        extension_runtime_hooks: &extension_runtime_hooks,
        orchestrator_mode: cli.orchestrator_mode,
        orchestrator_max_plan_steps: cli.orchestrator_max_plan_steps,
        orchestrator_max_executor_response_chars: cli.orchestrator_max_executor_response_chars,
        orchestrator_delegate_steps: cli.orchestrator_delegate_steps,
        command_context,
    };
    if let Some(command_file_path) = cli.command_file.as_deref() {
        execute_command_file(
            command_file_path,
            cli.command_file_error_mode,
            &mut agent,
            &mut session_runtime,
            command_context,
        )?;
        return Ok(());
    }
    run_interactive(agent, session_runtime, interactive_config).await
}

pub(crate) fn register_runtime_extension_tool_hook_subscriber(
    agent: &mut Agent,
    extension_runtime_hooks: &RuntimeExtensionHooksConfig,
) {
    if !extension_runtime_hooks.enabled {
        return;
    }

    let root = extension_runtime_hooks.root.clone();
    agent.subscribe(move |event| {
        let dispatch = extension_tool_hook_dispatch(event);
        let Some((hook, payload)) = dispatch else {
            return;
        };
        let summary = dispatch_extension_runtime_hook(&root, hook, &payload);
        for diagnostic in summary.diagnostics {
            eprintln!("{diagnostic}");
        }
    });
}

fn extension_tool_hook_dispatch(event: &AgentEvent) -> Option<(&'static str, Value)> {
    match event {
        AgentEvent::ToolExecutionStart {
            tool_call_id,
            tool_name,
            arguments,
        } => Some((
            "pre-tool-call",
            extension_tool_hook_payload(
                "pre-tool-call",
                serde_json::json!({
                "tool_call_id": tool_call_id,
                "tool_name": tool_name,
                "arguments": arguments,
                }),
            ),
        )),
        AgentEvent::ToolExecutionEnd {
            tool_call_id,
            tool_name,
            result,
        } => Some((
            "post-tool-call",
            extension_tool_hook_payload(
                "post-tool-call",
                serde_json::json!({
                "tool_call_id": tool_call_id,
                "tool_name": tool_name,
                "result": {
                    "is_error": result.is_error,
                    "content": result.content,
                },
                }),
            ),
        )),
        _ => None,
    }
}

fn extension_tool_hook_payload(hook: &str, data: Value) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert(
        "schema_version".to_string(),
        serde_json::Value::Number(EXTENSION_TOOL_HOOK_PAYLOAD_SCHEMA_VERSION.into()),
    );
    payload.insert(
        "hook".to_string(),
        serde_json::Value::String(hook.to_string()),
    );
    payload.insert(
        "emitted_at_ms".to_string(),
        serde_json::Value::Number(current_unix_timestamp_ms().into()),
    );
    payload.insert("data".to_string(), data.clone());
    if let Some(object) = data.as_object() {
        for (key, value) in object {
            payload.insert(key.clone(), value.clone());
        }
    }
    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use super::extension_tool_hook_dispatch;
    use pi_agent_core::{AgentEvent, ToolExecutionResult};

    #[test]
    fn unit_extension_tool_hook_dispatch_maps_start_event_payload() {
        let event = AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::json!({"path":"README.md"}),
        };
        let (hook, payload) = extension_tool_hook_dispatch(&event).expect("dispatch payload");
        assert_eq!(hook, "pre-tool-call");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "pre-tool-call");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["tool_call_id"], "call-1");
        assert_eq!(payload["data"]["tool_name"], "read");
        assert_eq!(payload["data"]["arguments"]["path"], "README.md");
    }

    #[test]
    fn unit_extension_tool_hook_dispatch_maps_end_event_payload() {
        let event = AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({"content":"hello"})),
        };
        let (hook, payload) = extension_tool_hook_dispatch(&event).expect("dispatch payload");
        assert_eq!(hook, "post-tool-call");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["hook"], "post-tool-call");
        assert!(payload["emitted_at_ms"].as_u64().is_some());
        assert_eq!(payload["data"]["tool_call_id"], "call-1");
        assert_eq!(payload["data"]["tool_name"], "read");
        assert_eq!(payload["data"]["result"]["is_error"], false);
        assert_eq!(payload["data"]["result"]["content"]["content"], "hello");
    }
}
