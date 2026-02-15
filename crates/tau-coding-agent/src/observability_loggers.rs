//! Observability logger runtime facade.
//!
//! Re-exports structured logger/reporter runtime helpers so coding-agent startup
//! can configure diagnostics consistently across command and transport modes.

pub(crate) use tau_runtime::observability_loggers_runtime::*;
