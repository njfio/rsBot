//! Project-index command facade.
//!
//! Exposes project indexing runtime commands used by coding-agent operator flows
//! while deferring indexing/repair semantics to `tau-ops`.

pub(crate) use tau_ops::execute_project_index_command;
