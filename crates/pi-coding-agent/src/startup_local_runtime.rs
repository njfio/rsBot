use super::*;

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

    if let Some(prompt) = resolve_prompt_input(cli)? {
        if cli.orchestrator_mode == CliOrchestratorMode::PlanFirst {
            run_plan_first_prompt(
                &mut agent,
                &mut session_runtime,
                &prompt,
                cli.turn_timeout_ms,
                render_options,
                cli.orchestrator_max_plan_steps,
            )
            .await?;
        } else {
            run_prompt(
                &mut agent,
                &mut session_runtime,
                &prompt,
                cli.turn_timeout_ms,
                render_options,
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
    };
    let interactive_config = InteractiveRuntimeConfig {
        turn_timeout_ms: cli.turn_timeout_ms,
        render_options,
        orchestrator_mode: cli.orchestrator_mode,
        orchestrator_max_plan_steps: cli.orchestrator_max_plan_steps,
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
