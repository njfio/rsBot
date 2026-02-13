use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tau_agent_core::{Agent, AgentConfig, AgentEvent};
use tokio::sync::watch;

use super::{
    attachment_filename_from_url, collect_shared_assistant_reply, current_unix_timestamp_ms,
    evaluate_attachment_content_type_policy, evaluate_attachment_url_policy,
    extract_attachment_urls, github_principal, initialize_issue_session_runtime,
    normalize_shared_artifact_retention_days, normalize_shared_relative_channel_path,
    prompt_status_label, rbac_policy_path_for_state_dir, render_event_prompt,
    render_issue_artifact_markdown, run_prompt_with_cancellation, shared_sanitize_for_path,
    shared_sha256_hex, shared_short_key_hash, ChannelArtifactRecord, ChannelAttachmentRecord,
    ChannelLogEntry, ChannelStore, GithubApiClient, GithubBridgeEvent,
    GithubIssuesBridgeRuntimeConfig, PromptRunStatus, RepoRef, GITHUB_ATTACHMENT_MAX_BYTES,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(super) struct PromptUsageSummary {
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) request_duration_ms: u64,
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct PromptRunReport {
    pub(super) run_id: String,
    pub(super) model: String,
    pub(super) status: PromptRunStatus,
    pub(super) assistant_reply: String,
    pub(super) usage: PromptUsageSummary,
    pub(super) downloaded_attachments: Vec<DownloadedGithubAttachment>,
    pub(super) artifact: ChannelArtifactRecord,
}

pub(super) struct RunPromptForEventRequest<'a> {
    pub(super) config: &'a GithubIssuesBridgeRuntimeConfig,
    pub(super) github_client: &'a GithubApiClient,
    pub(super) repo: &'a RepoRef,
    pub(super) repository_state_dir: &'a Path,
    pub(super) event: &'a GithubBridgeEvent,
    pub(super) prompt: &'a str,
    pub(super) run_id: &'a str,
    pub(super) cancel_rx: watch::Receiver<bool>,
}

#[derive(Debug, Clone)]
pub(super) struct DownloadedGithubAttachment {
    pub(super) source_url: String,
    pub(super) original_name: String,
    pub(super) path: PathBuf,
    pub(super) relative_path: String,
    pub(super) content_type: Option<String>,
    pub(super) bytes: u64,
    pub(super) checksum_sha256: String,
    pub(super) policy_reason_code: String,
    pub(super) created_unix_ms: u64,
    pub(super) expires_unix_ms: Option<u64>,
}

/// Runs a single GitHub issue prompt against the agent and records artifacts.
pub(super) async fn run_prompt_for_event(
    request: RunPromptForEventRequest<'_>,
) -> Result<PromptRunReport> {
    let RunPromptForEventRequest {
        config,
        github_client,
        repo,
        repository_state_dir,
        event,
        prompt,
        run_id,
        mut cancel_rx,
    } = request;

    let channel_store = ChannelStore::open(
        &repository_state_dir.join("channel-store"),
        "github",
        &format!("issue-{}", event.issue_number),
    )?;
    let attachment_retention_days =
        normalize_shared_artifact_retention_days(config.artifact_retention_days);
    let downloaded_attachments = download_issue_attachments(
        github_client,
        &channel_store,
        event,
        run_id,
        attachment_retention_days,
        &event.body,
    )
    .await?;
    let session_path = channel_store.session_path();
    let mut agent = Agent::new(
        config.client.clone(),
        AgentConfig {
            model: config.model.clone(),
            system_prompt: config.system_prompt.clone(),
            max_turns: config.max_turns,
            temperature: Some(0.0),
            max_tokens: None,
            ..AgentConfig::default()
        },
    );
    let mut tool_policy = config.tool_policy.clone();
    tool_policy.rbac_principal = Some(github_principal(&event.author_login));
    tool_policy.rbac_policy_path = Some(rbac_policy_path_for_state_dir(&config.state_dir));
    crate::tools::register_builtin_tools(&mut agent, tool_policy);

    let usage = Arc::new(Mutex::new(PromptUsageSummary::default()));
    agent.subscribe({
        let usage = usage.clone();
        move |event| {
            if let AgentEvent::TurnEnd {
                usage: turn_usage,
                request_duration_ms,
                finish_reason,
                ..
            } = event
            {
                if let Ok(mut guard) = usage.lock() {
                    guard.input_tokens = guard.input_tokens.saturating_add(turn_usage.input_tokens);
                    guard.output_tokens =
                        guard.output_tokens.saturating_add(turn_usage.output_tokens);
                    guard.total_tokens = guard.total_tokens.saturating_add(turn_usage.total_tokens);
                    guard.request_duration_ms = guard
                        .request_duration_ms
                        .saturating_add(*request_duration_ms);
                    guard.finish_reason = finish_reason.clone();
                }
            }
        }
    });

    let mut session_runtime = Some(initialize_issue_session_runtime(
        &session_path,
        &config.system_prompt,
        config.session_lock_wait_ms,
        config.session_lock_stale_ms,
        &mut agent,
    )?);

    let formatted_prompt = render_event_prompt(repo, event, prompt, &downloaded_attachments);
    let start_index = agent.messages().len();
    let cancellation_signal = async move {
        loop {
            if *cancel_rx.borrow() {
                break;
            }
            if cancel_rx.changed().await.is_err() {
                break;
            }
        }
    };
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut session_runtime,
        &formatted_prompt,
        config.turn_timeout_ms,
        cancellation_signal,
        config.render_options,
    )
    .await?;
    let assistant_reply = if status == PromptRunStatus::Cancelled {
        "Run cancelled by /tau stop.".to_string()
    } else if status == PromptRunStatus::TimedOut {
        "Run timed out before completion.".to_string()
    } else {
        collect_shared_assistant_reply(&agent.messages()[start_index..])
    };
    let usage = usage
        .lock()
        .map_err(|_| anyhow!("prompt usage lock is poisoned"))?
        .clone();
    let artifact = channel_store.write_text_artifact(
        run_id,
        "github-issue-reply",
        "private",
        attachment_retention_days,
        "md",
        &render_issue_artifact_markdown(
            repo,
            event,
            run_id,
            status,
            &assistant_reply,
            &downloaded_attachments,
        ),
    )?;
    channel_store.sync_context_from_messages(agent.messages())?;
    channel_store.append_log_entry(&ChannelLogEntry {
        timestamp_unix_ms: current_unix_timestamp_ms(),
        direction: "outbound".to_string(),
        event_key: Some(event.key.clone()),
        source: "github".to_string(),
        payload: json!({
            "run_id": run_id,
            "status": prompt_status_label(status),
            "assistant_reply": assistant_reply.clone(),
            "tokens": {
                "input": usage.input_tokens,
                "output": usage.output_tokens,
                "total": usage.total_tokens,
            },
            "artifact": {
                "id": artifact.id,
                "path": artifact.relative_path,
                "checksum_sha256": artifact.checksum_sha256,
                "bytes": artifact.bytes,
                "expires_unix_ms": artifact.expires_unix_ms,
            },
            "downloaded_attachments": downloaded_attachments.iter().map(|attachment| {
                json!({
                    "source_url": attachment.source_url,
                    "original_name": attachment.original_name,
                    "path": attachment.path.display().to_string(),
                    "relative_path": attachment.relative_path,
                    "content_type": attachment.content_type,
                    "bytes": attachment.bytes,
                    "checksum_sha256": attachment.checksum_sha256,
                    "policy_reason_code": attachment.policy_reason_code,
                    "created_unix_ms": attachment.created_unix_ms,
                    "expires_unix_ms": attachment.expires_unix_ms,
                })
            }).collect::<Vec<_>>(),
        }),
    })?;
    Ok(PromptRunReport {
        run_id: run_id.to_string(),
        model: config.model.clone(),
        status,
        assistant_reply,
        usage,
        downloaded_attachments,
        artifact,
    })
}

async fn download_issue_attachments(
    github_client: &GithubApiClient,
    channel_store: &ChannelStore,
    event: &GithubBridgeEvent,
    run_id: &str,
    retention_days: Option<u64>,
    text: &str,
) -> Result<Vec<DownloadedGithubAttachment>> {
    let urls = extract_attachment_urls(text);
    if urls.is_empty() {
        return Ok(Vec::new());
    }

    let file_dir = channel_store
        .attachments_dir()
        .join(shared_sanitize_for_path(&event.key));
    std::fs::create_dir_all(&file_dir)
        .with_context(|| format!("failed to create {}", file_dir.display()))?;

    let mut downloaded = Vec::new();
    for (index, url) in urls.iter().enumerate() {
        let url_policy = evaluate_attachment_url_policy(url);
        if !url_policy.accepted {
            eprintln!(
                "github attachment blocked by url policy: event={} url={} reason={}",
                event.key, url, url_policy.reason_code
            );
            continue;
        }

        let payload = match github_client.download_url_bytes(url).await {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!(
                    "github attachment download failed: event={} url={} error={error}",
                    event.key, url
                );
                continue;
            }
        };
        if payload.bytes.len() > GITHUB_ATTACHMENT_MAX_BYTES {
            eprintln!(
                "github attachment skipped due size limit: event={} url={} bytes={} limit={}",
                event.key,
                url,
                payload.bytes.len(),
                GITHUB_ATTACHMENT_MAX_BYTES
            );
            continue;
        }
        let content_policy =
            evaluate_attachment_content_type_policy(payload.content_type.as_deref());
        if !content_policy.accepted {
            eprintln!(
                "github attachment blocked by content policy: event={} url={} content_type={} reason={}",
                event.key,
                url,
                payload
                    .content_type
                    .as_deref()
                    .unwrap_or("unknown"),
                content_policy.reason_code,
            );
            continue;
        }

        let original_name = attachment_filename_from_url(url, index + 1);
        let safe_name = shared_sanitize_for_path(&original_name);
        let safe_name = if safe_name.is_empty() {
            format!("attachment-{}.bin", index + 1)
        } else {
            safe_name
        };
        let path = file_dir.join(format!("{:02}-{}", index + 1, safe_name));
        std::fs::write(&path, &payload.bytes)
            .with_context(|| format!("failed to write {}", path.display()))?;
        let relative_path = normalize_shared_relative_channel_path(
            &path,
            &channel_store.channel_dir(),
            "attachment file",
        )
        .map_err(|error| anyhow!(error))?;
        let created_unix_ms = current_unix_timestamp_ms();
        let expires_unix_ms = retention_days
            .map(|days| days.saturating_mul(86_400_000))
            .map(|ttl| created_unix_ms.saturating_add(ttl));
        let checksum_sha256 = shared_sha256_hex(&payload.bytes);
        let policy_reason_code = if content_policy.reason_code == "allow_content_type_default" {
            url_policy.reason_code
        } else {
            content_policy.reason_code
        };
        let record = ChannelAttachmentRecord {
            id: format!(
                "attachment-{}-{}",
                created_unix_ms,
                shared_short_key_hash(&format!("{}:{}:{}:{}", run_id, event.key, index, url))
            ),
            run_id: run_id.to_string(),
            event_key: event.key.clone(),
            actor: event.author_login.clone(),
            source_url: url.clone(),
            original_name: original_name.clone(),
            relative_path: relative_path.clone(),
            content_type: payload.content_type.clone(),
            bytes: payload.bytes.len() as u64,
            checksum_sha256: checksum_sha256.clone(),
            policy_decision: "accepted".to_string(),
            policy_reason_code: policy_reason_code.to_string(),
            created_unix_ms,
            expires_unix_ms,
        };
        channel_store.append_attachment_record(&record)?;
        downloaded.push(DownloadedGithubAttachment {
            source_url: url.clone(),
            original_name,
            path,
            relative_path,
            content_type: payload.content_type,
            bytes: payload.bytes.len() as u64,
            checksum_sha256,
            policy_reason_code: policy_reason_code.to_string(),
            created_unix_ms,
            expires_unix_ms,
        });
    }

    Ok(downloaded)
}
