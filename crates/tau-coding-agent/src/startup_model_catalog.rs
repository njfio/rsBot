//! Startup model-catalog resolution facade.
//!
//! Re-exports startup model-catalog resolve/validate helpers so dispatch can
//! enforce model catalog contracts before runtime starts.

pub(crate) use tau_startup::{resolve_startup_model_catalog, validate_startup_model_catalog};
