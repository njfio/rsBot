use super::*;

pub(crate) struct StartupPolicyBundle {
    pub(crate) tool_policy: ToolPolicy,
    pub(crate) tool_policy_json: Value,
}

pub(crate) fn resolve_startup_policy(cli: &Cli) -> Result<StartupPolicyBundle> {
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
