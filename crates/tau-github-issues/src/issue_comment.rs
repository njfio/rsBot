pub const EVENT_KEY_MARKER_PREFIX: &str = "<!-- tau-event-key:";
pub const EVENT_KEY_MARKER_SUFFIX: &str = " -->";

pub fn normalize_issue_command_status(status: &str) -> &'static str {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "acknowledged" | "accepted" => "acknowledged",
        "failed" | "error" => "failed",
        "reported" | "completed" | "healthy" | "warning" | "degraded" => "reported",
        _ => "reported",
    }
}

fn sanitize_reason_token(raw: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_sep = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
            last_was_sep = false;
        } else if !last_was_sep {
            normalized.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = normalized.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn issue_command_reason_code(command: &str, status: &str) -> String {
    format!(
        "issue_command_{}_{}",
        sanitize_reason_token(command),
        sanitize_reason_token(status)
    )
}

pub fn render_issue_command_comment(
    event_key: &str,
    command: &str,
    status: &str,
    reason_code: &str,
    message: &str,
) -> String {
    let content = if message.trim().is_empty() {
        "Tau command response.".to_string()
    } else {
        message.trim().to_string()
    };
    let command = if command.trim().is_empty() {
        "unknown"
    } else {
        command.trim()
    };
    let status = if status.trim().is_empty() {
        "reported"
    } else {
        status.trim()
    };
    let reason_code = if reason_code.trim().is_empty() {
        "issue_command_unknown_reported"
    } else {
        reason_code.trim()
    };
    format!(
        "{content}\n\n---\n{EVENT_KEY_MARKER_PREFIX}{event_key}{EVENT_KEY_MARKER_SUFFIX}\n_Tau command `{command}` | status `{status}` | reason_code `{reason_code}`_"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        issue_command_reason_code, normalize_issue_command_status, render_issue_command_comment,
        EVENT_KEY_MARKER_PREFIX, EVENT_KEY_MARKER_SUFFIX,
    };

    #[test]
    fn unit_normalize_issue_command_status_maps_known_variants() {
        assert_eq!(
            normalize_issue_command_status("acknowledged"),
            "acknowledged"
        );
        assert_eq!(normalize_issue_command_status("accepted"), "acknowledged");
        assert_eq!(normalize_issue_command_status("failed"), "failed");
        assert_eq!(normalize_issue_command_status("healthy"), "reported");
        assert_eq!(normalize_issue_command_status("unknown-value"), "reported");
    }

    #[test]
    fn functional_issue_command_reason_code_normalizes_tokens() {
        assert_eq!(
            issue_command_reason_code("auth-status", "reported"),
            "issue_command_auth_status_reported"
        );
        assert_eq!(
            issue_command_reason_code("demo index run", "failed!"),
            "issue_command_demo_index_run_failed"
        );
    }

    #[test]
    fn integration_render_issue_command_comment_renders_marker_footer_and_fields() {
        let rendered = render_issue_command_comment(
            "event-123",
            "status",
            "reported",
            "issue_command_status_reported",
            "Current bridge status is healthy.",
        );
        assert!(rendered.contains("Current bridge status is healthy."));
        assert!(rendered.contains(EVENT_KEY_MARKER_PREFIX));
        assert!(rendered.contains(EVENT_KEY_MARKER_SUFFIX));
        assert!(rendered.contains("Tau command `status`"));
        assert!(rendered.contains("reason_code `issue_command_status_reported`"));
    }

    #[test]
    fn regression_render_issue_command_comment_defaults_when_fields_are_blank() {
        let rendered = render_issue_command_comment("event-123", " ", " ", " ", " ");
        assert!(rendered.contains("Tau command response."));
        assert!(rendered.contains("Tau command `unknown` | status `reported`"));
        assert!(rendered.contains("reason_code `issue_command_unknown_reported`"));
    }
}
