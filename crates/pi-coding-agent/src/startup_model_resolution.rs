use super::*;

pub(crate) struct StartupModelResolution {
    pub(crate) model_ref: ModelRef,
    pub(crate) fallback_model_refs: Vec<ModelRef>,
}

pub(crate) fn resolve_startup_models(cli: &Cli) -> Result<StartupModelResolution> {
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
