use anyhow::Result;
use tau_ai::ModelRef;
use tau_cli::Cli;
use tau_onboarding::startup_dispatch::execute_startup_runtime_from_cli_with_modes;
use tau_onboarding::startup_model_resolution::{resolve_startup_models, StartupModelResolution};
use tau_onboarding::startup_skills_bootstrap::run_startup_skills_bootstrap;
use tau_provider::build_client_with_fallbacks;
use tau_skills::execute_package_activate_on_startup;

use crate::runtime_types::RenderOptions;
use crate::startup_local_runtime::{run_local_runtime, LocalRuntimeConfig};
use crate::startup_model_catalog::{resolve_startup_model_catalog, validate_startup_model_catalog};
use crate::startup_preflight::execute_startup_preflight;
use crate::startup_transport_modes::run_transport_mode_if_requested;
use crate::training_runtime::run_training_mode_if_requested;

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
                if run_training_mode_if_requested(
                    cli,
                    client.clone(),
                    &model_ref,
                    &model_catalog,
                    &system_prompt,
                    &tool_policy,
                )
                .await?
                {
                    return Ok(());
                }

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
