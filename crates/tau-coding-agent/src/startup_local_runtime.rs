use super::*;
use crate::extension_manifest::{
    discover_extension_runtime_registrations, ExtensionRuntimeRegistrationSummary,
};
use tau_onboarding::startup_local_runtime::{
    register_runtime_extension_tool_hook_subscriber as register_onboarding_runtime_extension_tool_hook_subscriber,
    resolve_extension_runtime_registrations, resolve_local_runtime_entry_mode,
    resolve_orchestrator_route_table, LocalRuntimeEntryMode,
};

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
        let outcome = initialize_session(
            &cli.session,
            cli.session_lock_wait_ms,
            cli.session_lock_stale_ms,
            cli.branch_from,
            system_prompt,
        )?;
        if !outcome.lineage.is_empty() {
            agent.replace_messages(outcome.lineage);
        }
        Some(outcome.runtime)
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
    let orchestrator_route_table = resolve_orchestrator_route_table(
        cli.orchestrator_route_table.as_deref(),
        load_multi_agent_route_table,
    )?;
    let orchestrator_route_trace_log = cli.telemetry_log.as_deref();
    let extension_runtime_registrations = resolve_extension_runtime_registrations(
        extension_runtime_hooks.enabled,
        &extension_runtime_hooks.root,
        |root| discover_extension_runtime_registrations(root, crate::commands::COMMAND_NAMES),
        |root| ExtensionRuntimeRegistrationSummary {
            root: root.to_path_buf(),
            discovered: 0,
            registered_tools: Vec::new(),
            registered_commands: Vec::new(),
            skipped_invalid: 0,
            skipped_unsupported_runtime: 0,
            skipped_permission_denied: 0,
            skipped_name_conflict: 0,
            diagnostics: Vec::new(),
        },
    );
    tools::register_extension_tools(
        &mut agent,
        &extension_runtime_registrations.registered_tools,
    );
    for diagnostic in &extension_runtime_registrations.diagnostics {
        eprintln!("{diagnostic}");
    }
    register_runtime_extension_tool_hook_subscriber(&mut agent, &extension_runtime_hooks);

    let entry_mode = resolve_local_runtime_entry_mode(
        resolve_prompt_input(cli)?,
        cli.orchestrator_mode == CliOrchestratorMode::PlanFirst,
        cli.command_file.as_deref(),
    );

    match &entry_mode {
        LocalRuntimeEntryMode::PlanFirstPrompt(prompt) => {
            run_plan_first_prompt_with_runtime_hooks(
                &mut agent,
                &mut session_runtime,
                prompt,
                cli.turn_timeout_ms,
                render_options,
                cli.orchestrator_max_plan_steps,
                cli.orchestrator_max_delegated_steps,
                cli.orchestrator_max_executor_response_chars,
                cli.orchestrator_max_delegated_step_response_chars,
                cli.orchestrator_max_delegated_total_response_chars,
                cli.orchestrator_delegate_steps,
                &orchestrator_route_table,
                orchestrator_route_trace_log,
                tool_policy_json,
                &extension_runtime_hooks,
            )
            .await?;
            return Ok(());
        }
        LocalRuntimeEntryMode::Prompt(prompt) => {
            run_prompt(
                &mut agent,
                &mut session_runtime,
                prompt,
                cli.turn_timeout_ms,
                render_options,
                &extension_runtime_hooks,
            )
            .await?;
            return Ok(());
        }
        LocalRuntimeEntryMode::CommandFile(_) | LocalRuntimeEntryMode::Interactive => {}
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
        orchestrator_max_delegated_steps: cli.orchestrator_max_delegated_steps,
        orchestrator_max_executor_response_chars: cli.orchestrator_max_executor_response_chars,
        orchestrator_max_delegated_step_response_chars: cli
            .orchestrator_max_delegated_step_response_chars,
        orchestrator_max_delegated_total_response_chars: cli
            .orchestrator_max_delegated_total_response_chars,
        orchestrator_delegate_steps: cli.orchestrator_delegate_steps,
        orchestrator_route_table: &orchestrator_route_table,
        orchestrator_route_trace_log,
        command_context,
    };

    if let LocalRuntimeEntryMode::CommandFile(command_file_path) = entry_mode {
        execute_command_file(
            &command_file_path,
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
    register_onboarding_runtime_extension_tool_hook_subscriber(
        agent,
        extension_runtime_hooks.enabled,
        extension_runtime_hooks.root.clone(),
        |root, hook, payload| dispatch_extension_runtime_hook(root, hook, payload).diagnostics,
    );
}
