//! RPC capabilities command facade.
//!
//! Exposes startup RPC capability reporting helpers and payload builders used by
//! compatibility checks and operator diagnostics.

#[cfg(test)]
pub(crate) use tau_runtime::rpc_capabilities_payload;
pub(crate) use tau_startup::execute_rpc_capabilities_command;
