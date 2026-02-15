//! Model catalog facade for coding-agent startup/runtime flows.
//!
//! Re-exports provider catalog APIs used by startup model resolution and tests
//! so the binary crate consumes one stable catalog contract surface.

#[cfg(test)]
pub(crate) use tau_provider::default_model_catalog_cache_path;
pub(crate) use tau_provider::ModelCatalog;
#[cfg(test)]
pub(crate) use tau_provider::{
    parse_models_list_args, render_model_show, render_models_list, MODELS_LIST_USAGE,
    MODEL_SHOW_USAGE,
};
