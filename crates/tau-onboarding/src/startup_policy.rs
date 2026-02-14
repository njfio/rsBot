use anyhow::Result;
use serde_json::Value;
use tau_cli::Cli;
use tau_tools::tool_policy_config::{build_tool_policy, tool_policy_to_json};
use tau_tools::tools::ToolPolicy;

/// Public struct `StartupPolicyBundle` used across Tau components.
pub struct StartupPolicyBundle {
    pub tool_policy: ToolPolicy,
    pub tool_policy_json: Value,
}

pub fn resolve_startup_policy(cli: &Cli) -> Result<StartupPolicyBundle> {
    let tool_policy = build_tool_policy(cli)?;
    let tool_policy_json = tool_policy_to_json(&tool_policy);
    if cli.print_tool_policy {
        println!("{tool_policy_json}");
    }
    Ok(StartupPolicyBundle {
        tool_policy,
        tool_policy_json,
    })
}
