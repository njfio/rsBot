use super::*;
use crate::extension_manifest::{
    discover_extension_runtime_registrations, ExtensionRuntimeRegistrationSummary,
};
use tau_onboarding::startup_local_runtime::{
    build_local_runtime_agent as build_onboarding_local_runtime_agent,
    build_local_runtime_command_defaults as build_onboarding_local_runtime_command_defaults,
    build_local_runtime_extension_bootstrap as build_onboarding_local_runtime_extension_bootstrap,
    execute_prompt_or_command_file_entry_mode as execute_onboarding_prompt_or_command_file_entry_mode,
    register_runtime_event_reporter_if_configured as register_onboarding_runtime_event_reporter_if_configured,
    register_runtime_extension_tool_hook_subscriber as register_onboarding_runtime_extension_tool_hook_subscriber,
    register_runtime_extension_tools as register_onboarding_runtime_extension_tools,
    register_runtime_json_event_subscriber as register_onboarding_runtime_json_event_subscriber,
    resolve_local_runtime_entry_mode, resolve_session_runtime, LocalRuntimeCommandDefaults,
    LocalRuntimeExtensionBootstrap, PromptEntryRuntimeMode, PromptOrCommandFileEntryOutcome,
    SessionBootstrapOutcome,
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

    let mut agent = build_onboarding_local_runtime_agent(
        client,
        model_ref,
        system_prompt,
        cli.max_turns,
        tool_policy,
    );
    register_onboarding_runtime_event_reporter_if_configured(
        &mut agent,
        cli.tool_audit_log.clone(),
        ToolAuditLogger::open,
        |logger, event| logger.log_event(event),
        |error| eprintln!("tool audit logger error: {error}"),
    )?;
    register_onboarding_runtime_event_reporter_if_configured(
        &mut agent,
        cli.telemetry_log.clone(),
        |path| PromptTelemetryLogger::open(path, model_ref.provider.as_str(), &model_ref.model),
        |logger, event| logger.log_event(event),
        |error| eprintln!("telemetry logger error: {error}"),
    )?;
    let mut session_runtime = resolve_session_runtime(
        cli.no_session,
        || {
            let outcome = initialize_session(
                &cli.session,
                cli.session_lock_wait_ms,
                cli.session_lock_stale_ms,
                cli.branch_from,
                system_prompt,
            )?;
            Ok(SessionBootstrapOutcome {
                runtime: outcome.runtime,
                lineage: outcome.lineage,
            })
        },
        |lineage| agent.replace_messages(lineage),
    )?;

    register_onboarding_runtime_json_event_subscriber(
        &mut agent,
        cli.json_events,
        event_to_json,
        |value| println!("{value}"),
    );
    let extension_runtime_hooks = RuntimeExtensionHooksConfig {
        enabled: cli.extension_runtime_hooks,
        root: cli.extension_runtime_root.clone(),
    };
    let LocalRuntimeExtensionBootstrap {
        orchestrator_route_table,
        orchestrator_route_trace_log,
        extension_runtime_registrations,
    } = build_onboarding_local_runtime_extension_bootstrap(
        cli,
        extension_runtime_hooks.enabled,
        &extension_runtime_hooks.root,
        load_multi_agent_route_table,
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
    )?;
    let orchestrator_route_trace_log = orchestrator_route_trace_log.as_deref();
    register_onboarding_runtime_extension_tools(
        &mut agent,
        &extension_runtime_registrations.registered_tools,
        &extension_runtime_registrations.diagnostics,
        tools::register_extension_tools,
        |diagnostic| eprintln!("{diagnostic}"),
    );
    register_runtime_extension_tool_hook_subscriber(&mut agent, &extension_runtime_hooks);

    let entry_mode = resolve_local_runtime_entry_mode(
        resolve_prompt_input(cli)?,
        cli.orchestrator_mode == CliOrchestratorMode::PlanFirst,
        cli.command_file.as_deref(),
    );
    let LocalRuntimeCommandDefaults {
        profile_defaults,
        auth_command_config,
        doctor_config,
    } = build_onboarding_local_runtime_command_defaults(
        cli,
        model_ref,
        fallback_model_refs,
        skills_dir,
        skills_lock_path,
    );
    let skills_sync_command_config = SkillsSyncCommandConfig {
        skills_dir: skills_dir.to_path_buf(),
        default_lock_path: skills_lock_path.to_path_buf(),
        default_trust_root_path: cli.skill_trust_root_file.clone(),
        doctor_config,
    };
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

    match execute_onboarding_prompt_or_command_file_entry_mode(&entry_mode, |prompt_mode| async {
        match prompt_mode {
            PromptEntryRuntimeMode::PlanFirstPrompt(prompt) => {
                run_plan_first_prompt_with_runtime_hooks(
                    &mut agent,
                    &mut session_runtime,
                    &prompt,
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
            }
            PromptEntryRuntimeMode::Prompt(prompt) => {
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
        }
        Ok(())
    })
    .await?
    {
        PromptOrCommandFileEntryOutcome::PromptHandled => return Ok(()),
        PromptOrCommandFileEntryOutcome::CommandFile(command_file_path) => {
            execute_command_file(
                &command_file_path,
                cli.command_file_error_mode,
                &mut agent,
                &mut session_runtime,
                command_context,
            )?;
            return Ok(());
        }
        PromptOrCommandFileEntryOutcome::None => {}
    };

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
