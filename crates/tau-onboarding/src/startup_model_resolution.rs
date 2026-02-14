use anyhow::{anyhow, bail, Result};
use tau_ai::ModelRef;
use tau_cli::Cli;
use tau_provider::resolve_fallback_models;

/// Public struct `StartupModelResolution` used across Tau components.
pub struct StartupModelResolution {
    pub model_ref: ModelRef,
    pub fallback_model_refs: Vec<ModelRef>,
}

pub fn resolve_startup_models(cli: &Cli) -> Result<StartupModelResolution> {
    if cli.no_session && cli.branch_from.is_some() {
        bail!("--branch-from cannot be used together with --no-session");
    }

    let model_ref = ModelRef::parse(&cli.model)
        .map_err(|error| anyhow!("failed to parse --model '{}': {error}", cli.model))?;
    let fallback_model_refs = resolve_fallback_models(cli, &model_ref)?;

    Ok(StartupModelResolution {
        model_ref,
        fallback_model_refs,
    })
}
