use std::path::{Path, PathBuf};

pub fn normalize_relative_channel_path(
    path: &Path,
    channel_root: &Path,
    label: &str,
) -> Result<String, String> {
    let relative = path.strip_prefix(channel_root).map_err(|_| {
        format!(
            "failed to derive relative path for {label}: {}",
            path.display()
        )
    })?;
    let normalized = relative.to_string_lossy().replace('\\', "/");
    if normalized.trim().is_empty() {
        return Err(format!(
            "derived empty relative path for {label}: {}",
            path.display()
        ));
    }
    Ok(normalized)
}

pub fn normalize_artifact_retention_days(days: u64) -> Option<u64> {
    if days == 0 {
        None
    } else {
        Some(days)
    }
}

pub fn render_issue_artifact_pointer_line(
    label: &str,
    artifact_id: &str,
    relative_path: &str,
    bytes: u64,
) -> String {
    format!("{label}: id=`{artifact_id}` path=`{relative_path}` bytes=`{bytes}`")
}

pub fn session_path_for_issue(repo_state_dir: &Path, issue_number: u64) -> PathBuf {
    repo_state_dir
        .join("sessions")
        .join(format!("issue-{issue_number}.jsonl"))
}

pub fn issue_session_id(issue_number: u64) -> String {
    format!("issue-{issue_number}")
}

pub fn parse_rfc3339_to_unix_ms(raw: &str) -> Option<u64> {
    let parsed = chrono::DateTime::parse_from_rfc3339(raw).ok()?;
    u64::try_from(parsed.timestamp_millis()).ok()
}

pub fn sanitize_for_path(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn is_expired_at(expires_unix_ms: Option<u64>, now_unix_ms: u64) -> bool {
    expires_unix_ms
        .map(|value| value <= now_unix_ms)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        is_expired_at, issue_session_id, normalize_artifact_retention_days,
        normalize_relative_channel_path, parse_rfc3339_to_unix_ms,
        render_issue_artifact_pointer_line, sanitize_for_path, session_path_for_issue,
    };
    use std::path::Path;

    #[test]
    fn unit_normalize_artifact_retention_days_maps_zero_to_none() {
        assert_eq!(normalize_artifact_retention_days(0), None);
        assert_eq!(normalize_artifact_retention_days(30), Some(30));
    }

    #[test]
    fn functional_normalize_relative_channel_path_normalizes_windows_separators() {
        let root = Path::new("/tmp/channel");
        let file = Path::new("/tmp/channel/attachments/nested\\trace.log");
        let normalized = normalize_relative_channel_path(file, root, "attachment")
            .expect("normalized relative path");
        assert_eq!(normalized, "attachments/nested/trace.log");
    }

    #[test]
    fn integration_render_issue_artifact_pointer_line_formats_expected_shape() {
        let rendered =
            render_issue_artifact_pointer_line("artifact", "id-1", "artifacts/run-1.md", 42);
        assert_eq!(
            rendered,
            "artifact: id=`id-1` path=`artifacts/run-1.md` bytes=`42`"
        );
    }

    #[test]
    fn regression_normalize_relative_channel_path_rejects_outside_and_empty_paths() {
        let root = Path::new("/tmp/channel");
        let outside = Path::new("/tmp/other/artifacts/run-1.md");
        let outside_error = normalize_relative_channel_path(outside, root, "artifact")
            .expect_err("outside path should fail");
        assert!(outside_error.contains("failed to derive relative path"));

        let root_path_error = normalize_relative_channel_path(root, root, "artifact")
            .expect_err("root path should fail");
        assert!(root_path_error.contains("derived empty relative path"));
    }

    #[test]
    fn unit_issue_session_id_formats_issue_identifier() {
        assert_eq!(issue_session_id(42), "issue-42");
    }

    #[test]
    fn integration_session_path_for_issue_builds_expected_repo_relative_path() {
        let root = Path::new("/tmp/repo");
        let path = session_path_for_issue(root, 9);
        assert_eq!(path, Path::new("/tmp/repo/sessions/issue-9.jsonl"));
    }

    #[test]
    fn unit_parse_rfc3339_to_unix_ms_handles_valid_and_invalid_values() {
        assert!(parse_rfc3339_to_unix_ms("2026-01-01T00:00:01Z").is_some());
        assert_eq!(parse_rfc3339_to_unix_ms("invalid"), None);
    }

    #[test]
    fn functional_sanitize_for_path_replaces_unsafe_characters() {
        assert_eq!(sanitize_for_path("owner/repo"), "owner_repo");
        assert_eq!(
            sanitize_for_path("issue-comment-created:1200"),
            "issue-comment-created_1200"
        );
    }

    #[test]
    fn regression_is_expired_at_handles_none_and_boundary_values() {
        assert!(!is_expired_at(None, 100));
        assert!(!is_expired_at(Some(101), 100));
        assert!(is_expired_at(Some(100), 100));
    }
}
