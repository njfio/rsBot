use anyhow::Result;
use serde_json::Value;
use tau_cli::Cli;
use tau_startup::resolve_startup_safety_policy;
use tau_tools::tools::ToolPolicy;

/// Public struct `StartupPolicyBundle` used across Tau components.
pub struct StartupPolicyBundle {
    pub tool_policy: ToolPolicy,
    pub tool_policy_json: Value,
    pub precedence_layers: Vec<String>,
}

pub fn resolve_startup_policy(cli: &Cli) -> Result<StartupPolicyBundle> {
    let resolved = resolve_startup_safety_policy(cli)?;
    Ok(StartupPolicyBundle {
        tool_policy: resolved.tool_policy,
        tool_policy_json: resolved.tool_policy_json,
        precedence_layers: resolved.precedence_layers,
    })
}
