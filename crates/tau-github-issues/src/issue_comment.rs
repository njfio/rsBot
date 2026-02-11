use crate::github_issues_helpers::{chunk_text_by_chars, split_at_char_index};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Copy)]
pub struct IssueCommentUsageView {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct IssueCommentArtifactView<'a> {
    pub relative_path: &'a str,
    pub checksum_sha256: &'a str,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct IssueCommentRunView<'a> {
    pub event_key: &'a str,
    pub run_id: &'a str,
    pub status: &'a str,
    pub model: &'a str,
    pub assistant_reply: &'a str,
    pub usage: IssueCommentUsageView,
    pub artifact: IssueCommentArtifactView<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct IssueCommentAttachmentView<'a> {
    pub policy_reason_code: &'a str,
}

fn increment_count(map: &mut BTreeMap<String, usize>, raw: &str) {
    let key = raw.trim();
    if key.is_empty() {
        return;
    }
    let counter = map.entry(key.to_string()).or_insert(0);
    *counter = counter.saturating_add(1);
}

pub fn render_issue_comment_response_parts(
    run: IssueCommentRunView<'_>,
    downloaded_attachments: &[IssueCommentAttachmentView<'_>],
) -> (String, String) {
    let mut content = run.assistant_reply.trim().to_string();
    if content.is_empty() {
        content = "I couldn't generate a textual response for this event.".to_string();
    }

    let mut footer = format!(
        "{EVENT_KEY_MARKER_PREFIX}{}{EVENT_KEY_MARKER_SUFFIX}\n_Tau run `{}` | status `{}` | model `{}` | tokens in/out/total `{}/{}/{}` | cost `unavailable`_\n_artifact `{}` | sha256 `{}` | bytes `{}`_",
        run.event_key,
        run.run_id,
        run.status,
        run.model,
        run.usage.input_tokens,
        run.usage.output_tokens,
        run.usage.total_tokens,
        run.artifact.relative_path,
        run.artifact.checksum_sha256,
        run.artifact.bytes
    );
    if !downloaded_attachments.is_empty() {
        let mut reason_counts = BTreeMap::new();
        for attachment in downloaded_attachments {
            increment_count(&mut reason_counts, attachment.policy_reason_code);
        }
        let reason_summary = if reason_counts.is_empty() {
            "none".to_string()
        } else {
            reason_counts
                .iter()
                .map(|(reason, count)| format!("{reason}:{count}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        footer.push_str(&format!(
            "\n_attachments downloaded `{}` | policy_reason_counts `{reason_summary}`_",
            downloaded_attachments.len()
        ));
    }

    (content, footer)
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

pub fn render_issue_comment_chunks_with_footer(
    content: &str,
    footer: &str,
    max_chars: usize,
) -> Vec<String> {
    let footer_block = format!("\n\n---\n{footer}");
    let footer_len = footer_block.chars().count();
    if max_chars == 0 {
        return Vec::new();
    }
    if footer_len >= max_chars {
        return vec![footer_block];
    }
    let content_len = content.chars().count();
    if content_len + footer_len <= max_chars {
        return vec![format!("{content}{footer_block}")];
    }

    let max_first_len = max_chars.saturating_sub(footer_len);
    let (first_content, remainder) = split_at_char_index(content, max_first_len);
    let mut chunks = Vec::new();
    chunks.push(format!("{first_content}{footer_block}"));
    let mut trailing = chunk_text_by_chars(&remainder, max_chars);
    chunks.append(&mut trailing);
    chunks
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
        render_issue_command_comment, render_issue_comment_chunks_with_footer,
        render_issue_comment_response_parts, IssueCommentArtifactView, IssueCommentAttachmentView,
        IssueCommentRunView, IssueCommentUsageView, EVENT_KEY_MARKER_PREFIX,
        EVENT_KEY_MARKER_SUFFIX, LEGACY_EVENT_KEY_MARKER_PREFIX,
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

    #[test]
    fn unit_render_issue_comment_chunks_with_footer_returns_empty_for_zero_limit() {
        assert!(render_issue_comment_chunks_with_footer("hello", "footer", 0).is_empty());
    }

    #[test]
    fn functional_render_issue_comment_chunks_with_footer_returns_single_chunk_when_fit() {
        let chunks = render_issue_comment_chunks_with_footer("hello", "footer", 64);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("hello"));
        assert!(chunks[0].contains("footer"));
    }

    #[test]
    fn integration_render_issue_comment_chunks_with_footer_splits_content_and_keeps_footer_first() {
        let chunks = render_issue_comment_chunks_with_footer("abcdefghijklmnopqrstuvwxyz", "f", 20);
        assert!(chunks.len() >= 2);
        assert!(!chunks[0].contains("footer"));
        assert!(chunks[0].contains("---"));
        assert!(chunks[0].contains("\nf"));
        assert!(chunks[1..].iter().all(|chunk| !chunk.contains("---")));
    }

    #[test]
    fn regression_render_issue_comment_chunks_with_footer_handles_footer_longer_than_limit() {
        let chunks = render_issue_comment_chunks_with_footer("hello", "very-long-footer", 3);
        assert_eq!(chunks, vec!["\n\n---\nvery-long-footer".to_string()]);
    }

    #[test]
    fn unit_render_issue_comment_response_parts_defaults_empty_assistant_reply() {
        let usage = IssueCommentUsageView {
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
        };
        let artifact = IssueCommentArtifactView {
            relative_path: "artifacts/run.md",
            checksum_sha256: "abc123",
            bytes: 99,
        };
        let run = IssueCommentRunView {
            event_key: "event-1",
            run_id: "run-1",
            status: "completed",
            model: "openai/gpt-4o-mini",
            assistant_reply: "  ",
            usage,
            artifact,
        };
        let (content, footer) = render_issue_comment_response_parts(run, &[]);
        assert_eq!(
            content,
            "I couldn't generate a textual response for this event."
        );
        assert!(footer.contains("Tau run `run-1`"));
        assert!(footer.contains("tokens in/out/total `10/5/15`"));
    }

    #[test]
    fn functional_render_issue_comment_response_parts_includes_attachment_reason_counts() {
        let usage = IssueCommentUsageView {
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
        };
        let artifact = IssueCommentArtifactView {
            relative_path: "artifacts/run.md",
            checksum_sha256: "abc123",
            bytes: 99,
        };
        let attachments = vec![
            IssueCommentAttachmentView {
                policy_reason_code: "allow_extension_allowlist",
            },
            IssueCommentAttachmentView {
                policy_reason_code: "allow_extension_allowlist",
            },
            IssueCommentAttachmentView {
                policy_reason_code: "allow_content_type_default",
            },
        ];
        let run = IssueCommentRunView {
            event_key: "event-1",
            run_id: "run-1",
            status: "completed",
            model: "openai/gpt-4o-mini",
            assistant_reply: "ok",
            usage,
            artifact,
        };
        let (_content, footer) = render_issue_comment_response_parts(run, &attachments);
        assert!(footer.contains("_attachments downloaded `3`"));
        assert!(footer.contains("allow_extension_allowlist:2"));
        assert!(footer.contains("allow_content_type_default:1"));
    }

    #[test]
    fn integration_render_issue_comment_response_parts_preserves_event_marker_and_model() {
        let usage = IssueCommentUsageView {
            input_tokens: 3,
            output_tokens: 2,
            total_tokens: 5,
        };
        let artifact = IssueCommentArtifactView {
            relative_path: "artifacts/run.md",
            checksum_sha256: "deadbeef",
            bytes: 123,
        };
        let run = IssueCommentRunView {
            event_key: "issue-comment-created:42",
            run_id: "run-99",
            status: "completed",
            model: "openai/gpt-4o-mini",
            assistant_reply: "result",
            usage,
            artifact,
        };
        let (_content, footer) = render_issue_comment_response_parts(run, &[]);
        assert!(footer.contains(EVENT_KEY_MARKER_PREFIX));
        assert!(footer.contains(EVENT_KEY_MARKER_SUFFIX));
        assert!(footer.contains("issue-comment-created:42"));
        assert!(footer.contains("model `openai/gpt-4o-mini`"));
    }

    #[test]
    fn regression_render_issue_comment_response_parts_ignores_blank_reason_codes() {
        let usage = IssueCommentUsageView {
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
        };
        let artifact = IssueCommentArtifactView {
            relative_path: "artifacts/run.md",
            checksum_sha256: "deadbeef",
            bytes: 10,
        };
        let attachments = vec![
            IssueCommentAttachmentView {
                policy_reason_code: "  ",
            },
            IssueCommentAttachmentView {
                policy_reason_code: "\t",
            },
        ];
        let run = IssueCommentRunView {
            event_key: "event-2",
            run_id: "run-2",
            status: "completed",
            model: "model-x",
            assistant_reply: "ok",
            usage,
            artifact,
        };
        let (_content, footer) = render_issue_comment_response_parts(run, &attachments);
        assert!(footer.contains("_attachments downloaded `2`"));
        assert!(footer.contains("policy_reason_counts `none`"));
    }
}
