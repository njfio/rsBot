//! Prompt and response rendering helpers for Slack bridge runtime flows.

use tau_ai::{Message, MessageRole};

use super::{
    DownloadedSlackFile, PromptRunReport, PromptRunStatus, SlackBridgeEvent, SlackBridgeEventKind,
    SLACK_METADATA_MARKER_PREFIX, SLACK_METADATA_MARKER_SUFFIX,
};
use crate::slack_helpers::{truncate_for_error, truncate_for_slack};

pub(super) fn normalize_slack_message_text(event: &SlackBridgeEvent, bot_user_id: &str) -> String {
    let mut message_text = event.text.trim().to_string();
    if event.kind == SlackBridgeEventKind::AppMention {
        let mention = format!("<@{bot_user_id}>");
        message_text = message_text.replace(&mention, "");
        message_text = message_text.trim().to_string();
    }
    message_text
}

pub(super) fn render_event_prompt(
    event: &SlackBridgeEvent,
    bot_user_id: &str,
    downloaded_files: &[DownloadedSlackFile],
) -> String {
    let message_text = normalize_slack_message_text(event, bot_user_id);

    let mut prompt = format!(
        "You are responding as Tau inside Slack.\nChannel: {}\nUser: <@{}>\nEvent kind: {}\nMessage ts: {}\n\nUser message:\n{}",
        event.channel_id,
        event.user_id,
        event.kind.as_str(),
        event.ts,
        if message_text.is_empty() {
            "(empty message)"
        } else {
            &message_text
        }
    );

    if !downloaded_files.is_empty() {
        prompt.push_str("\n\nDownloaded attachments:\n");
        for file in downloaded_files {
            prompt.push_str(&format!(
                "- id={} name={} path={} mimetype={} size={}\n",
                file.id,
                file.original_name,
                file.path.display(),
                file.mimetype
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                file.size.unwrap_or(0)
            ));
        }
    }

    prompt.push_str("\nProvide a direct, concise Slack-ready response.");
    prompt
}

pub(super) fn render_slack_response(
    event: &SlackBridgeEvent,
    run: &PromptRunReport,
    detail_thread_output: bool,
    detail_threshold_chars: usize,
) -> (String, Option<String>) {
    let reply = run.assistant_reply.trim();
    let base_reply = if reply.is_empty() {
        "I couldn't generate a textual response for this Slack event."
    } else {
        reply
    };
    let usage = &run.usage;
    let status = format!("{:?}", run.status).to_lowercase();

    let mut summary_body = base_reply.to_string();
    let mut detail_body = None;

    if detail_thread_output && base_reply.chars().count() > detail_threshold_chars.max(1) {
        let summary = truncate_for_slack(base_reply, detail_threshold_chars.max(1));
        summary_body = format!("{}\n\n(full response posted in this thread)", summary);
        detail_body = Some(base_reply.to_string());
    }

    summary_body.push_str("\n\n---\n");
    summary_body.push_str(&format!(
        "{}\nTau run {} | status {} | model {} | tokens {}/{}/{}",
        slack_metadata_marker(&event.key),
        run.run_id,
        status,
        run.model,
        usage.input_tokens,
        usage.output_tokens,
        usage.total_tokens
    ));
    summary_body.push_str(&format!(
        "\nartifact {} | sha256 {} | bytes {}",
        run.artifact.relative_path, run.artifact.checksum_sha256, run.artifact.bytes
    ));

    if !run.downloaded_files.is_empty() {
        summary_body.push_str("\nattachments downloaded:");
        for file in &run.downloaded_files {
            summary_body.push_str(&format!(
                "\n- {} ({})",
                file.original_name,
                file.path.display()
            ));
        }
    }

    (truncate_for_slack(&summary_body, 38_000), detail_body)
}

pub(super) fn render_slack_run_error_message(
    event: &SlackBridgeEvent,
    run_id: &str,
    error: &anyhow::Error,
) -> String {
    truncate_for_slack(
        &format!(
            "Tau run {} failed for event {}.\n\nError: {}\n\n---\n{}",
            run_id,
            event.key,
            truncate_for_error(&error.to_string(), 600),
            slack_metadata_marker(&event.key),
        ),
        38_000,
    )
}

pub(super) fn slack_metadata_marker(event_key: &str) -> String {
    format!("{SLACK_METADATA_MARKER_PREFIX}{event_key}{SLACK_METADATA_MARKER_SUFFIX}")
}

pub(super) fn render_slack_artifact_markdown(
    event: &SlackBridgeEvent,
    run_id: &str,
    status: PromptRunStatus,
    assistant_reply: &str,
    downloaded_files: &[DownloadedSlackFile],
) -> String {
    let mut lines = vec![
        "# Tau Slack Artifact".to_string(),
        format!("channel_id: {}", event.channel_id),
        format!("event_key: {}", event.key),
        format!("event_kind: {}", event.kind.as_str()),
        format!("run_id: {}", run_id),
        format!("status: {}", prompt_status_label(status)),
    ];
    if downloaded_files.is_empty() {
        lines.push("attachments: none".to_string());
    } else {
        lines.push(format!("attachments: {}", downloaded_files.len()));
        for file in downloaded_files {
            lines.push(format!(
                "- {} ({})",
                file.original_name,
                file.path.display()
            ));
        }
    }
    lines.push(String::new());
    lines.push("## Assistant Reply".to_string());
    lines.push(assistant_reply.trim().to_string());
    lines.join("\n")
}

pub(super) fn normalize_artifact_retention_days(days: u64) -> Option<u64> {
    if days == 0 {
        None
    } else {
        Some(days)
    }
}

pub(super) fn collect_assistant_reply(messages: &[Message]) -> String {
    let content = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .map(Message::text_content)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if content.trim().is_empty() {
        "I couldn't generate a textual response for this event.".to_string()
    } else {
        content
    }
}

pub(super) fn prompt_status_label(status: PromptRunStatus) -> &'static str {
    match status {
        PromptRunStatus::Completed => "completed",
        PromptRunStatus::Cancelled => "cancelled",
        PromptRunStatus::TimedOut => "timed_out",
    }
}
