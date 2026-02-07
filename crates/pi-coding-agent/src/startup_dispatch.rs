use super::*;

pub(crate) async fn run_cli(cli: Cli) -> Result<()> {
    if execute_startup_preflight(&cli)? {
        return Ok(());
    }

    let StartupModelResolution {
        model_ref,
        fallback_model_refs,
    } = resolve_startup_models(&cli)?;

    let client = build_client_with_fallbacks(&cli, &model_ref, &fallback_model_refs)?;
    let skills_bootstrap = run_startup_skills_bootstrap(&cli).await?;
    let skills_lock_path = skills_bootstrap.skills_lock_path;
    let system_prompt = compose_startup_system_prompt(&cli)?;
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
        system_prompt: &system_prompt,
        tool_policy,
        tool_policy_json: &tool_policy_json,
        render_options,
        skills_lock_path: &skills_lock_path,
    })
    .await
}
