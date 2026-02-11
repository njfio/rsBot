use std::path::Path;

pub struct IssueEventPromptAttachmentView<'a> {
    pub source_url: &'a str,
    pub original_name: &'a str,
    pub path: &'a Path,
    pub content_type: Option<&'a str>,
    pub bytes: u64,
    pub policy_reason_code: &'a str,
    pub expires_unix_ms: Option<u64>,
}

pub struct IssueArtifactAttachmentView<'a> {
    pub source_url: &'a str,
    pub original_name: &'a str,
    pub path: &'a Path,
    pub relative_path: &'a str,
    pub content_type: Option<&'a str>,
    pub bytes: u64,
    pub checksum_sha256: &'a str,
    pub policy_reason_code: &'a str,
    pub created_unix_ms: u64,
    pub expires_unix_ms: Option<u64>,
}

pub fn render_event_prompt(
    repo_slug: &str,
    issue_number: u64,
    issue_title: &str,
    author_login: &str,
    event_kind: &str,
    prompt: &str,
    downloaded_attachments: &[IssueEventPromptAttachmentView<'_>],
) -> String {
    let mut rendered = format!(
        "You are responding as Tau inside GitHub issues.\nRepository: {}\nIssue: #{} ({})\nAuthor: @{}\nEvent: {}\n\nUser message:\n{}\n\nProvide a direct, actionable response suitable for a GitHub issue comment.",
        repo_slug, issue_number, issue_title, author_login, event_kind, prompt
    );
    if !downloaded_attachments.is_empty() {
        rendered.push_str("\n\nDownloaded attachments:\n");
        for attachment in downloaded_attachments {
            rendered.push_str(&format!(
                "- name={} path={} content_type={} bytes={} source_url={} policy_reason={} expires_unix_ms={}\n",
                attachment.original_name,
                attachment.path.display(),
                attachment.content_type.unwrap_or("unknown"),
                attachment.bytes,
                attachment.source_url,
                attachment.policy_reason_code,
                attachment
                    .expires_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ));
        }
    }
    rendered
}

pub fn render_issue_artifact_markdown(
    repo_slug: &str,
    issue_number: u64,
    event_key: &str,
    run_id: &str,
    status_label: &str,
    assistant_reply: &str,
    downloaded_attachments: &[IssueArtifactAttachmentView<'_>],
) -> String {
    let mut lines = vec![
        "# Tau Artifact".to_string(),
        format!("repository: {repo_slug}"),
        format!("issue_number: {issue_number}"),
        format!("event_key: {event_key}"),
        format!("run_id: {run_id}"),
        format!("status: {status_label}"),
    ];
    if downloaded_attachments.is_empty() {
        lines.push("attachments: none".to_string());
    } else {
        lines.push(format!("attachments: {}", downloaded_attachments.len()));
        for attachment in downloaded_attachments {
            lines.push(format!(
                "- name={} path={} relative_path={} content_type={} bytes={} source_url={} checksum_sha256={} policy_reason={} created_unix_ms={} expires_unix_ms={}",
                attachment.original_name,
                attachment.path.display(),
                attachment.relative_path,
                attachment.content_type.unwrap_or("unknown"),
                attachment.bytes,
                attachment.source_url,
                attachment.checksum_sha256,
                attachment.policy_reason_code,
                attachment.created_unix_ms,
                attachment
                    .expires_unix_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ));
        }
    }
    lines.push(String::new());
    lines.push("## Assistant Reply".to_string());
    lines.push(assistant_reply.trim().to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        render_event_prompt, render_issue_artifact_markdown, IssueArtifactAttachmentView,
        IssueEventPromptAttachmentView,
    };
    use std::path::Path;

    #[test]
    fn unit_render_event_prompt_includes_core_metadata() {
        let prompt = render_event_prompt(
            "owner/repo",
            42,
            "Issue Title",
            "alice",
            "issue_comment_created",
            "inspect this",
            &[],
        );
        assert!(prompt.contains("Repository: owner/repo"));
        assert!(prompt.contains("Issue: #42 (Issue Title)"));
        assert!(prompt.contains("Author: @alice"));
    }

    #[test]
    fn functional_render_event_prompt_includes_attachment_details() {
        let attachments = vec![IssueEventPromptAttachmentView {
            source_url: "https://example.com/files/trace.log",
            original_name: "trace.log",
            path: Path::new("/tmp/attachments/trace.log"),
            content_type: Some("text/plain"),
            bytes: 42,
            policy_reason_code: "allow_extension_allowlist",
            expires_unix_ms: Some(1000),
        }];
        let prompt = render_event_prompt(
            "owner/repo",
            42,
            "Issue Title",
            "alice",
            "issue_comment_created",
            "inspect this",
            &attachments,
        );
        assert!(prompt.contains("Downloaded attachments:"));
        assert!(prompt.contains("name=trace.log"));
        assert!(prompt.contains("source_url=https://example.com/files/trace.log"));
    }

    #[test]
    fn integration_render_issue_artifact_markdown_includes_attachment_inventory() {
        let attachments = vec![IssueArtifactAttachmentView {
            source_url: "https://example.com/files/trace.log",
            original_name: "trace.log",
            path: Path::new("/tmp/attachments/trace.log"),
            relative_path: "attachments/issue-comment-created_1/01-trace.log",
            content_type: Some("text/plain"),
            bytes: 42,
            checksum_sha256: "abc123",
            policy_reason_code: "allow_extension_allowlist",
            created_unix_ms: 1,
            expires_unix_ms: Some(1000),
        }];
        let markdown = render_issue_artifact_markdown(
            "owner/repo",
            42,
            "issue-comment-created:1",
            "run-1",
            "completed",
            "assistant reply",
            &attachments,
        );
        assert!(markdown.contains("attachments: 1"));
        assert!(markdown.contains("relative_path=attachments/issue-comment-created_1/01-trace.log"));
        assert!(markdown.contains("## Assistant Reply"));
    }

    #[test]
    fn regression_render_issue_artifact_markdown_defaults_to_no_attachments_and_trims_reply() {
        let markdown = render_issue_artifact_markdown(
            "owner/repo",
            42,
            "issue-comment-created:1",
            "run-1",
            "completed",
            "  assistant reply  ",
            &[],
        );
        assert!(markdown.contains("attachments: none"));
        assert!(markdown.contains("\nassistant reply"));
        assert!(!markdown.contains("  assistant reply  "));
    }
}
