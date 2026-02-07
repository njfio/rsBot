use super::*;

pub(crate) async fn run_cli(cli: Cli) -> Result<()> {
    if execute_startup_preflight(&cli)? {
        return Ok(());
    }

    if cli.no_session && cli.branch_from.is_some() {
        bail!("--branch-from cannot be used together with --no-session");
    }

    let model_ref = ModelRef::parse(&cli.model)
        .map_err(|error| anyhow!("failed to parse --model '{}': {error}", cli.model))?;
    let fallback_model_refs = resolve_fallback_models(&cli, &model_ref)?;

    let client = build_client_with_fallbacks(&cli, &model_ref, &fallback_model_refs)?;
    let mut skill_lock_hints = Vec::new();
    if !cli.install_skill.is_empty() {
        let report = install_skills(&cli.install_skill, &cli.skills_dir)?;
        skill_lock_hints.extend(build_local_skill_lock_hints(&cli.install_skill)?);
        println!(
            "skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let skills_download_options = SkillsDownloadOptions {
        cache_dir: Some(
            cli.skills_cache_dir
                .clone()
                .unwrap_or_else(|| default_skills_cache_dir(&cli.skills_dir)),
        ),
        offline: cli.skills_offline,
    };
    let remote_skill_sources =
        resolve_remote_skill_sources(&cli.install_skill_url, &cli.install_skill_sha256)?;
    if !remote_skill_sources.is_empty() {
        let report = install_remote_skills_with_cache(
            &remote_skill_sources,
            &cli.skills_dir,
            &skills_download_options,
        )
        .await?;
        skill_lock_hints.extend(build_remote_skill_lock_hints(&remote_skill_sources)?);
        println!(
            "remote skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let trusted_skill_roots = resolve_skill_trust_roots(&cli)?;
    if !cli.install_skill_from_registry.is_empty() {
        let registry_url = cli.skill_registry_url.as_deref().ok_or_else(|| {
            anyhow!("--skill-registry-url is required when using --install-skill-from-registry")
        })?;
        let manifest = fetch_registry_manifest_with_cache(
            registry_url,
            cli.skill_registry_sha256.as_deref(),
            &skills_download_options,
        )
        .await?;
        let sources = resolve_registry_skill_sources(
            &manifest,
            &cli.install_skill_from_registry,
            &trusted_skill_roots,
            cli.require_signed_skills,
        )?;
        let report =
            install_remote_skills_with_cache(&sources, &cli.skills_dir, &skills_download_options)
                .await?;
        skill_lock_hints.extend(build_registry_skill_lock_hints(
            registry_url,
            &cli.install_skill_from_registry,
            &sources,
        )?);
        println!(
            "registry skills install: installed={} updated={} skipped={}",
            report.installed, report.updated, report.skipped
        );
    }
    let skills_lock_path = cli
        .skills_lock_file
        .clone()
        .unwrap_or_else(|| default_skills_lock_path(&cli.skills_dir));
    if cli.skills_lock_write {
        let lockfile =
            write_skills_lockfile(&cli.skills_dir, &skills_lock_path, &skill_lock_hints)?;
        println!(
            "{}",
            render_skills_lock_write_success(&skills_lock_path, lockfile.entries.len())
        );
    }
    if cli.skills_sync {
        let report = sync_skills_with_lockfile(&cli.skills_dir, &skills_lock_path)?;
        if report.in_sync() {
            println!("{}", render_skills_sync_in_sync(&skills_lock_path, &report));
        } else {
            bail!(
                "skills sync drift detected: path={} {}",
                skills_lock_path.display(),
                render_skills_sync_drift_details(&report)
            );
        }
    }
    let base_system_prompt = resolve_system_prompt(&cli)?;
    let catalog = load_catalog(&cli.skills_dir)
        .with_context(|| format!("failed to load skills from {}", cli.skills_dir.display()))?;
    let selected_skills = resolve_selected_skills(&catalog, &cli.skills)?;
    let system_prompt = augment_system_prompt(&base_system_prompt, &selected_skills);

    let tool_policy = build_tool_policy(&cli)?;
    let tool_policy_json = tool_policy_to_json(&tool_policy);
    if cli.print_tool_policy {
        println!("{tool_policy_json}");
    }
    let render_options = RenderOptions::from_cli(&cli);
    if run_transport_mode_if_requested(
        &cli,
        &client,
        &model_ref,
        &system_prompt,
        &tool_policy,
        render_options,
    )
    .await?
    {
        return Ok(());
    }

    let mut agent = Agent::new(
        client.clone(),
        AgentConfig {
            model: model_ref.model.clone(),
            system_prompt: system_prompt.clone(),
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
        Some(initialize_session(&mut agent, &cli, &system_prompt)?)
    };

    if cli.json_events {
        agent.subscribe(|event| {
            let value = event_to_json(event);
            println!("{value}");
        });
    }

    if let Some(prompt) = resolve_prompt_input(&cli)? {
        run_prompt(
            &mut agent,
            &mut session_runtime,
            &prompt,
            cli.turn_timeout_ms,
            render_options,
        )
        .await?;
        return Ok(());
    }

    let skills_sync_command_config = SkillsSyncCommandConfig {
        skills_dir: cli.skills_dir.clone(),
        default_lock_path: skills_lock_path.clone(),
        default_trust_root_path: cli.skill_trust_root_file.clone(),
        doctor_config: build_doctor_command_config(
            &cli,
            &model_ref,
            &fallback_model_refs,
            &skills_lock_path,
        ),
    };
    let profile_defaults = build_profile_defaults(&cli);
    let auth_command_config = build_auth_command_config(&cli);
    let command_context = CommandExecutionContext {
        tool_policy_json: &tool_policy_json,
        session_import_mode: cli.session_import_mode.into(),
        profile_defaults: &profile_defaults,
        skills_command_config: &skills_sync_command_config,
        auth_command_config: &auth_command_config,
    };
    let interactive_config = InteractiveRuntimeConfig {
        turn_timeout_ms: cli.turn_timeout_ms,
        render_options,
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
