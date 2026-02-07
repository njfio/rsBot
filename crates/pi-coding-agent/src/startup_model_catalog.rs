use super::*;

pub(crate) async fn resolve_startup_model_catalog(cli: &Cli) -> Result<ModelCatalog> {
    let options = ModelCatalogLoadOptions {
        cache_path: cli.model_catalog_cache.clone(),
        refresh_url: cli.model_catalog_url.clone(),
        offline: cli.model_catalog_offline,
        stale_after_hours: cli.model_catalog_stale_after_hours,
        request_timeout_ms: cli.request_timeout_ms,
    };
    let catalog = load_model_catalog_with_cache(&options).await?;
    println!(
        "model catalog: {}",
        catalog.diagnostics_line(cli.model_catalog_stale_after_hours)
    );
    Ok(catalog)
}

pub(crate) fn validate_startup_model_catalog(
    catalog: &ModelCatalog,
    primary: &ModelRef,
    fallback_models: &[ModelRef],
) -> Result<()> {
    ensure_model_supports_tools(catalog, primary)?;
    for fallback in fallback_models {
        ensure_model_supports_tools(catalog, fallback)?;
    }
    Ok(())
}
