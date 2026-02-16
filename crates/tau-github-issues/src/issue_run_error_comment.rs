use crate::github_transport_helpers::truncate_for_error;

/// Render GitHub issue comment body for failed Tau run events.
pub fn render_issue_run_error_comment(
    event_key: &str,
    run_id: &str,
    error_message: &str,
    event_key_marker_prefix: &str,
    event_key_marker_suffix: &str,
) -> String {
    format!(
        "Tau run `{}` failed for event `{}`.\n\nError: `{}`\n\n---\n{}{}{}\n_Tau run `{}` | status `failed` | model `unavailable` | tokens in/out/total `0/0/0` | cost `unavailable`_",
        run_id,
        event_key,
        truncate_for_error(error_message, 600),
        event_key_marker_prefix,
        event_key,
        event_key_marker_suffix,
        run_id
    )
}

#[cfg(test)]
mod tests {
    use super::render_issue_run_error_comment;

    const MARKER_PREFIX: &str = "<!-- tau:event-key:";
    const MARKER_SUFFIX: &str = " -->";

    #[test]
    fn unit_render_issue_run_error_comment_includes_core_identifiers() {
        let rendered = render_issue_run_error_comment(
            "issue-comment-created:1",
            "run-123",
            "network timeout",
            MARKER_PREFIX,
            MARKER_SUFFIX,
        );
        assert!(rendered.contains("Tau run `run-123` failed"));
        assert!(rendered.contains("event `issue-comment-created:1`"));
    }

    #[test]
    fn functional_render_issue_run_error_comment_includes_marker_footer() {
        let rendered = render_issue_run_error_comment(
            "issue-opened:42",
            "run-456",
            "boom",
            MARKER_PREFIX,
            MARKER_SUFFIX,
        );
        assert!(rendered.contains("<!-- tau:event-key:issue-opened:42 -->"));
    }

    #[test]
    fn integration_render_issue_run_error_comment_truncates_large_errors() {
        let large = "x".repeat(1200);
        let rendered =
            render_issue_run_error_comment("key", "run", &large, MARKER_PREFIX, MARKER_SUFFIX);
        assert!(rendered.contains("Error: `"));
        assert!(rendered.contains("..."));
    }

    #[test]
    fn regression_render_issue_run_error_comment_handles_blank_error_message() {
        let rendered =
            render_issue_run_error_comment("key", "run", "", MARKER_PREFIX, MARKER_SUFFIX);
        assert!(rendered.contains("Error: ``"));
    }
}
