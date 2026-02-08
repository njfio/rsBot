use super::*;

pub(crate) async fn run_cli(cli: Cli) -> Result<()> {
    if execute_startup_preflight(&cli)? {
        return Ok(());
    }

    let StartupModelResolution {
        model_ref,
        fallback_model_refs,
    } = resolve_startup_models(&cli)?;
    let model_catalog = resolve_startup_model_catalog(&cli).await?;
    validate_startup_model_catalog(&model_catalog, &model_ref, &fallback_model_refs)?;

    let client = build_client_with_fallbacks(&cli, &model_ref, &fallback_model_refs)?;
    let skills_bootstrap = run_startup_skills_bootstrap(&cli).await?;
    let startup_package_activation = execute_package_activate_on_startup(&cli)?;
    let effective_skills_dir =
        resolve_runtime_skills_dir(&cli, startup_package_activation.is_some());
    let skills_lock_path = resolve_runtime_skills_lock_path(
        &cli,
        &skills_bootstrap.skills_lock_path,
        &effective_skills_dir,
    );
    let system_prompt = compose_startup_system_prompt(&cli, &effective_skills_dir)?;
    let StartupPolicyBundle {
        tool_policy,
        tool_policy_json,
    } = resolve_startup_policy(&cli)?;
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
        model_catalog: &model_catalog,
        system_prompt: &system_prompt,
        tool_policy,
        tool_policy_json: &tool_policy_json,
        render_options,
        skills_dir: &effective_skills_dir,
        skills_lock_path: &skills_lock_path,
    })
    .await
}

fn resolve_runtime_skills_dir(cli: &Cli, activation_applied: bool) -> PathBuf {
    if !activation_applied {
        return cli.skills_dir.clone();
    }
    let activated_skills_dir = cli.package_activate_destination.join("skills");
    if activated_skills_dir.is_dir() {
        return activated_skills_dir;
    }
    cli.skills_dir.clone()
}

fn resolve_runtime_skills_lock_path(
    cli: &Cli,
    bootstrap_lock_path: &Path,
    effective_skills_dir: &Path,
) -> PathBuf {
    if effective_skills_dir == cli.skills_dir {
        bootstrap_lock_path.to_path_buf()
    } else {
        default_skills_lock_path(effective_skills_dir)
    }
}
