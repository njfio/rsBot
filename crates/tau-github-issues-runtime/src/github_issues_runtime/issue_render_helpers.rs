//! Rendering helpers for issue comments, run summaries, and status output.

use tau_diagnostics::DoctorStatus;
use tau_github_issues::issue_comment::{
    render_issue_comment_chunks_with_footer,
    render_issue_comment_response_parts as render_shared_issue_comment_response_parts,
    IssueCommentArtifactView, IssueCommentAttachmentView, IssueCommentRunView,
    IssueCommentUsageView,
};
use tau_github_issues::issue_event_collection::GithubBridgeEvent;
use tau_github_issues::issue_render::{
    render_event_prompt as render_shared_event_prompt,
    render_issue_artifact_markdown as render_shared_issue_artifact_markdown,
    IssueArtifactAttachmentView, IssueEventPromptAttachmentView,
};

use super::{DownloadedGithubAttachment, PromptRunReport, RepoRef, GITHUB_COMMENT_MAX_CHARS};
use crate::PromptRunStatus;

/// Renders the prompt payload passed to the agent for a single GitHub event.
pub(super) fn render_event_prompt(
    repo: &RepoRef,
    event: &GithubBridgeEvent,
    prompt: &str,
    downloaded_attachments: &[DownloadedGithubAttachment],
) -> String {
    let repo_slug = repo.as_slug();
    let attachment_views = downloaded_attachments
        .iter()
        .map(|attachment| IssueEventPromptAttachmentView {
            source_url: &attachment.source_url,
            original_name: &attachment.original_name,
            path: &attachment.path,
            content_type: attachment.content_type.as_deref(),
            bytes: attachment.bytes,
            policy_reason_code: &attachment.policy_reason_code,
            expires_unix_ms: attachment.expires_unix_ms,
        })
        .collect::<Vec<_>>();
    render_shared_event_prompt(
        &repo_slug,
        event.issue_number,
        &event.issue_title,
        &event.author_login,
        event.kind.as_str(),
        prompt,
        &attachment_views,
    )
}

/// Produces status and detail blocks for the final run response comment.
pub(super) fn render_issue_comment_response_parts(
    event: &GithubBridgeEvent,
    run: &PromptRunReport,
) -> (String, String) {
    let usage = &run.usage;
    let status = format!("{:?}", run.status).to_lowercase();
    let usage_view = IssueCommentUsageView {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
    };
    let artifact_view = IssueCommentArtifactView {
        relative_path: &run.artifact.relative_path,
        checksum_sha256: &run.artifact.checksum_sha256,
        bytes: run.artifact.bytes,
    };
    let attachment_views = run
        .downloaded_attachments
        .iter()
        .map(|attachment| IssueCommentAttachmentView {
            policy_reason_code: &attachment.policy_reason_code,
        })
        .collect::<Vec<_>>();
    let run_view = IssueCommentRunView {
        event_key: &event.key,
        run_id: &run.run_id,
        status: &status,
        model: &run.model,
        assistant_reply: &run.assistant_reply,
        usage: usage_view,
        artifact: artifact_view,
    };
    render_shared_issue_comment_response_parts(run_view, &attachment_views)
}

pub(super) fn render_issue_comment_chunks(
    event: &GithubBridgeEvent,
    run: &PromptRunReport,
) -> Vec<String> {
    render_issue_comment_chunks_with_limit(event, run, GITHUB_COMMENT_MAX_CHARS)
}

pub(super) fn render_issue_comment_chunks_with_limit(
    event: &GithubBridgeEvent,
    run: &PromptRunReport,
    max_chars: usize,
) -> Vec<String> {
    let (content, footer) = render_issue_comment_response_parts(event, run);
    render_issue_comment_chunks_with_footer(&content, &footer, max_chars)
}

pub(super) fn render_issue_artifact_markdown(
    repo: &RepoRef,
    event: &GithubBridgeEvent,
    run_id: &str,
    status: PromptRunStatus,
    assistant_reply: &str,
    downloaded_attachments: &[DownloadedGithubAttachment],
) -> String {
    let repo_slug = repo.as_slug();
    let attachment_views = downloaded_attachments
        .iter()
        .map(|attachment| IssueArtifactAttachmentView {
            source_url: &attachment.source_url,
            original_name: &attachment.original_name,
            path: &attachment.path,
            relative_path: &attachment.relative_path,
            content_type: attachment.content_type.as_deref(),
            bytes: attachment.bytes,
            checksum_sha256: &attachment.checksum_sha256,
            policy_reason_code: &attachment.policy_reason_code,
            created_unix_ms: attachment.created_unix_ms,
            expires_unix_ms: attachment.expires_unix_ms,
        })
        .collect::<Vec<_>>();
    render_shared_issue_artifact_markdown(
        &repo_slug,
        event.issue_number,
        &event.key,
        run_id,
        prompt_status_label(status),
        assistant_reply,
        &attachment_views,
    )
}

pub(super) fn prompt_status_label(status: PromptRunStatus) -> &'static str {
    match status {
        PromptRunStatus::Completed => "completed",
        PromptRunStatus::Cancelled => "cancelled",
        PromptRunStatus::TimedOut => "timed_out",
    }
}

pub(super) fn doctor_status_label(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Pass => "pass",
        DoctorStatus::Warn => "warn",
        DoctorStatus::Fail => "fail",
    }
}
