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
    let skills_bootstrap = run_startup_skills_bootstrap(&cli).await?;
    let skills_lock_path = skills_bootstrap.skills_lock_path;
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

    run_local_runtime(LocalRuntimeConfig {
        cli: &cli,
        client,
        model_ref: &model_ref,
        fallback_model_refs: &fallback_model_refs,
        system_prompt: &system_prompt,
        tool_policy,
        tool_policy_json: &tool_policy_json,
        render_options,
        skills_lock_path: &skills_lock_path,
    })
    .await
}
