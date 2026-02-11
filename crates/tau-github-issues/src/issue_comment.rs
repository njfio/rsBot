pub const EVENT_KEY_MARKER_PREFIX: &str = "<!-- tau-event-key:";
pub const LEGACY_EVENT_KEY_MARKER_PREFIX: &str = "<!-- rsbot-event-key:";
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

pub fn extract_footer_event_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut cursor = text;
    loop {
        let tau = cursor.find(EVENT_KEY_MARKER_PREFIX);
        let legacy = cursor.find(LEGACY_EVENT_KEY_MARKER_PREFIX);
        let (start, marker_prefix) = match (tau, legacy) {
            (Some(tau_start), Some(legacy_start)) if tau_start <= legacy_start => {
                (tau_start, EVENT_KEY_MARKER_PREFIX)
            }
            (Some(_), Some(legacy_start)) => (legacy_start, LEGACY_EVENT_KEY_MARKER_PREFIX),
            (Some(tau_start), None) => (tau_start, EVENT_KEY_MARKER_PREFIX),
            (None, Some(legacy_start)) => (legacy_start, LEGACY_EVENT_KEY_MARKER_PREFIX),
            (None, None) => break,
        };
        let after_start = &cursor[start + marker_prefix.len()..];
        let Some(end) = after_start.find(EVENT_KEY_MARKER_SUFFIX) else {
            break;
        };
        let key = after_start[..end].trim();
        if !key.is_empty() {
            keys.push(key.to_string());
        }
        cursor = &after_start[end + EVENT_KEY_MARKER_SUFFIX.len()..];
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::{
        extract_footer_event_keys, issue_command_reason_code, normalize_issue_command_status,
        render_issue_command_comment, EVENT_KEY_MARKER_PREFIX, EVENT_KEY_MARKER_SUFFIX,
        LEGACY_EVENT_KEY_MARKER_PREFIX,
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

    #[test]
    fn integration_extract_footer_event_keys_reads_tau_and_legacy_markers() {
        let body = format!(
            "first\n{EVENT_KEY_MARKER_PREFIX}tau-1{EVENT_KEY_MARKER_SUFFIX}\nsecond\n{LEGACY_EVENT_KEY_MARKER_PREFIX}legacy-2{EVENT_KEY_MARKER_SUFFIX}"
        );
        assert_eq!(
            extract_footer_event_keys(&body),
            vec!["tau-1".to_string(), "legacy-2".to_string()]
        );
    }

    #[test]
    fn regression_extract_footer_event_keys_ignores_unterminated_markers() {
        let body = format!(
            "before {EVENT_KEY_MARKER_PREFIX}tau-1{EVENT_KEY_MARKER_SUFFIX} after {EVENT_KEY_MARKER_PREFIX}broken"
        );
        assert_eq!(extract_footer_event_keys(&body), vec!["tau-1".to_string()]);
    }
}
