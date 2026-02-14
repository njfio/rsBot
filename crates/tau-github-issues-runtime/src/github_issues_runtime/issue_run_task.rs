//! Run-task execution flow that wires command handling to issue updates.

use std::{path::PathBuf, time::Instant};

use tokio::sync::watch;

use super::{
    current_unix_timestamp_ms, prompt_status_label, render_issue_comment_chunks,
    render_shared_issue_run_error_comment, run_prompt_for_event, truncate_for_error,
    GithubApiClient, GithubBridgeEvent, GithubIssuesBridgeRuntimeConfig, PromptUsageSummary,
    RepoRef, RunPromptForEventRequest, EVENT_KEY_MARKER_PREFIX, EVENT_KEY_MARKER_SUFFIX,
};

#[derive(Debug)]
pub(super) struct RunTaskResult {
    pub(super) issue_number: u64,
    pub(super) event_key: String,
    pub(super) run_id: String,
    pub(super) started_unix_ms: u64,
    pub(super) completed_unix_ms: u64,
    pub(super) duration_ms: u64,
    pub(super) status: String,
    pub(super) posted_comment_id: Option<u64>,
    pub(super) comment_edit_attempted: bool,
    pub(super) comment_edit_success: bool,
    pub(super) comment_append_count: usize,
    pub(super) model: String,
    pub(super) usage: PromptUsageSummary,
    pub(super) error: Option<String>,
}

pub(super) struct IssueRunTaskParams {
    pub(super) github_client: GithubApiClient,
    pub(super) config: GithubIssuesBridgeRuntimeConfig,
    pub(super) repo: RepoRef,
    pub(super) repository_state_dir: PathBuf,
    pub(super) event: GithubBridgeEvent,
    pub(super) prompt: String,
    pub(super) run_id: String,
    pub(super) working_comment_id: u64,
    pub(super) cancel_rx: watch::Receiver<bool>,
    pub(super) started_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) struct CommentUpdateOutcome {
    pub(super) posted_comment_id: Option<u64>,
    pub(super) edit_attempted: bool,
    pub(super) edit_success: bool,
    pub(super) append_count: usize,
}

/// Executes one issue event run and posts the resulting GitHub comment updates.
pub(super) async fn execute_issue_run_task(params: IssueRunTaskParams) -> RunTaskResult {
    let IssueRunTaskParams {
        github_client,
        config,
        repo,
        repository_state_dir,
        event,
        prompt,
        run_id,
        working_comment_id,
        cancel_rx,
        started_unix_ms,
    } = params;
    let started = Instant::now();
    let run_result = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: &repository_state_dir,
        event: &event,
        prompt: &prompt,
        run_id: &run_id,
        cancel_rx,
    })
    .await;

    let completed_unix_ms = current_unix_timestamp_ms();
    let duration_ms = started.elapsed().as_millis() as u64;

    let (status, usage, chunks, error) = match run_result {
        Ok(run) => {
            let status = prompt_status_label(run.status).to_string();
            (
                status,
                run.usage.clone(),
                render_issue_comment_chunks(&event, &run),
                None,
            )
        }
        Err(error) => (
            "failed".to_string(),
            PromptUsageSummary::default(),
            vec![render_shared_issue_run_error_comment(
                &event.key,
                &run_id,
                &error.to_string(),
                EVENT_KEY_MARKER_PREFIX,
                EVENT_KEY_MARKER_SUFFIX,
            )],
            Some(error.to_string()),
        ),
    };

    let comment_outcome = post_issue_comment_chunks(
        &github_client,
        event.issue_number,
        working_comment_id,
        &chunks,
    )
    .await;

    RunTaskResult {
        issue_number: event.issue_number,
        event_key: event.key,
        run_id,
        started_unix_ms,
        completed_unix_ms,
        duration_ms,
        status,
        posted_comment_id: comment_outcome.posted_comment_id,
        comment_edit_attempted: comment_outcome.edit_attempted,
        comment_edit_success: comment_outcome.edit_success,
        comment_append_count: comment_outcome.append_count,
        model: config.model,
        usage,
        error,
    }
}

pub(super) async fn post_issue_comment_chunks(
    github_client: &GithubApiClient,
    issue_number: u64,
    working_comment_id: u64,
    chunks: &[String],
) -> CommentUpdateOutcome {
    let mut outcome = CommentUpdateOutcome {
        posted_comment_id: None,
        edit_attempted: false,
        edit_success: false,
        append_count: 0,
    };
    let Some((first, rest)) = chunks.split_first() else {
        return outcome;
    };

    outcome.edit_attempted = true;
    match github_client
        .update_issue_comment(working_comment_id, first)
        .await
    {
        Ok(comment) => {
            outcome.edit_success = true;
            outcome.posted_comment_id = Some(comment.id);
        }
        Err(update_error) => {
            let fallback_body = format!(
                "{first}\n\n_(warning: failed to update placeholder comment: {})_",
                truncate_for_error(&update_error.to_string(), 200)
            );
            match github_client
                .create_issue_comment(issue_number, &fallback_body)
                .await
            {
                Ok(comment) => {
                    outcome.append_count = outcome.append_count.saturating_add(1);
                    outcome.posted_comment_id = Some(comment.id);
                }
                Err(_) => {
                    return outcome;
                }
            }
        }
    };

    for chunk in rest {
        match github_client
            .create_issue_comment(issue_number, chunk)
            .await
        {
            Ok(comment) => {
                outcome.append_count = outcome.append_count.saturating_add(1);
                outcome.posted_comment_id = Some(comment.id);
            }
            Err(_) => break,
        }
    }
    outcome
}
