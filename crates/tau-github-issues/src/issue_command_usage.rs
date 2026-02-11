fn normalize_prefix(command_prefix: &str) -> &str {
    let trimmed = command_prefix.trim();
    if trimmed.is_empty() {
        "/tau"
    } else {
        trimmed
    }
}

pub fn doctor_command_usage(command_prefix: &str) -> String {
    let prefix = normalize_prefix(command_prefix);
    format!("Usage: {prefix} doctor [--online]")
}

pub fn issue_auth_command_usage(
    command_prefix: &str,
    auth_status_usage: &str,
    auth_matrix_usage: &str,
) -> String {
    let prefix = normalize_prefix(command_prefix);
    format!(
        "Usage: {prefix} auth <status|matrix> ...\n{}\n{}",
        auth_status_usage, auth_matrix_usage
    )
}

pub fn demo_index_command_usage(
    command_prefix: &str,
    scenarios: &[&str],
    default_timeout_seconds: u64,
    max_timeout_seconds: u64,
) -> String {
    let prefix = normalize_prefix(command_prefix);
    format!(
        "Usage: {prefix} demo-index <list|run [scenario[,scenario...]] [--timeout-seconds <n>]|report>\nAllowed scenarios: {}\nDefault run timeout: {} seconds (max {}).",
        scenarios.join(","),
        default_timeout_seconds,
        max_timeout_seconds
    )
}

pub fn tau_command_usage(command_prefix: &str) -> String {
    let prefix = normalize_prefix(command_prefix);
    [
        format!("Supported `{prefix}` commands:"),
        format!("- `{prefix} run <prompt>`"),
        format!("- `{prefix} stop`"),
        format!("- `{prefix} status`"),
        format!("- `{prefix} health`"),
        format!("- `{prefix} auth <status|matrix> ...`"),
        format!("- `{prefix} doctor [--online]`"),
        format!("- `{prefix} compact`"),
        format!("- `{prefix} help`"),
        format!(
            "- `{prefix} chat <start|resume|reset|export|status|summary|replay|show [limit]|search <query>>`"
        ),
        format!("- `{prefix} artifacts [purge|run <run_id>|show <artifact_id>]`"),
        format!(
            "- `{prefix} demo-index <list|run [scenario[,scenario...]] [--timeout-seconds <n>]|report>`"
        ),
        format!("- `{prefix} canvas <create|update|show|export|import> ...`"),
        format!("- `{prefix} summarize [focus]`"),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        demo_index_command_usage, doctor_command_usage, issue_auth_command_usage, tau_command_usage,
    };

    #[test]
    fn unit_doctor_command_usage_uses_requested_prefix() {
        assert_eq!(
            doctor_command_usage("/tau"),
            "Usage: /tau doctor [--online]".to_string()
        );
    }

    #[test]
    fn functional_issue_auth_command_usage_includes_status_and_matrix_help() {
        let usage = issue_auth_command_usage("/tau", "status help", "matrix help");
        assert!(usage.contains("Usage: /tau auth <status|matrix> ..."));
        assert!(usage.contains("status help"));
        assert!(usage.contains("matrix help"));
    }

    #[test]
    fn integration_demo_index_command_usage_lists_scenarios_and_timeout_bounds() {
        let usage = demo_index_command_usage(
            "/tau",
            &["onboarding", "gateway-auth", "multi-channel-live"],
            180,
            900,
        );
        assert!(usage.contains("Usage: /tau demo-index"));
        assert!(usage.contains("Allowed scenarios: onboarding,gateway-auth,multi-channel-live"));
        assert!(usage.contains("Default run timeout: 180 seconds (max 900)."));
    }

    #[test]
    fn regression_tau_command_usage_defaults_prefix_when_blank() {
        let usage = tau_command_usage("   ");
        assert!(usage.contains("Supported `/tau` commands:"));
        assert!(usage.contains("- `/tau run <prompt>`"));
    }
}
