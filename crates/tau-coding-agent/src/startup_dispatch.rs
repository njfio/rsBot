use super::*;
use tau_onboarding::startup_dispatch::execute_startup_runtime_from_cli_with_modes;

pub(crate) async fn run_cli(cli: Cli) -> Result<()> {
    if execute_startup_preflight(&cli)? {
        return Ok(());
    }

    execute_startup_runtime_from_cli_with_modes(
        &cli,
        |cli| -> Result<(ModelRef, Vec<ModelRef>)> {
            let StartupModelResolution {
                model_ref,
                fallback_model_refs,
            } = resolve_startup_models(cli)?;
            Ok((model_ref, fallback_model_refs))
        },
        |cli| Box::pin(resolve_startup_model_catalog(cli)),
        |model_catalog, model_ref, fallback_model_refs: &Vec<ModelRef>| {
            validate_startup_model_catalog(model_catalog, model_ref, fallback_model_refs)
        },
        |cli, model_ref, fallback_model_refs: &Vec<ModelRef>| {
            build_client_with_fallbacks(cli, model_ref, fallback_model_refs)
        },
        |cli| Box::pin(run_startup_skills_bootstrap(cli)),
        execute_package_activate_on_startup,
        |skills_bootstrap| skills_bootstrap.skills_lock_path.clone(),
        RenderOptions::from_cli,
        |cli, client, model_ref, system_prompt, tool_policy, render_options| {
            Box::pin(run_transport_mode_if_requested(
                cli,
                client,
                model_ref,
                system_prompt,
                tool_policy,
                render_options,
            ))
        },
        |cli,
         client,
         model_ref,
         fallback_model_refs,
         model_catalog,
         system_prompt,
         tool_policy,
         tool_policy_json,
         render_options,
         effective_skills_dir,
         skills_lock_path| {
            Box::pin(async move {
                run_local_runtime(LocalRuntimeConfig {
                    cli,
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
            })
        },
    )
    .await
}
