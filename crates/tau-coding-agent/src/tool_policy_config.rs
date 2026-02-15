//! Tool-policy startup configuration facade.
//!
//! This module re-exports startup safety-policy resolution helpers used by
//! runtime dispatch and startup tests to enforce tool-policy invariants.

// Re-export retained for startup/runtime tests through `crate::tool_policy_config`.
pub(crate) use tau_startup::{
    resolve_startup_safety_policy, startup_safety_policy_precedence_layers,
};
pub(crate) use tau_tools::tool_policy_config::{
    build_tool_policy, parse_sandbox_command_tokens, tool_policy_to_json,
};
