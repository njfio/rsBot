//! Tests for GitHub Issues bridge command parsing, runtime workflows, and safety guardrails.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use httpmock::prelude::*;
use serde_json::json;
use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
use tempfile::tempdir;
use tokio::time::sleep;

use super::{
    collect_shared_issue_events, evaluate_attachment_content_type_policy,
    evaluate_attachment_url_policy, event_action_from_shared_body, extract_attachment_urls,
    extract_footer_event_keys, is_retryable_github_status, issue_command_reason_code,
    issue_matches_required_labels, issue_matches_required_numbers, issue_shared_session_id,
    normalize_issue_command_status, normalize_shared_artifact_retention_days,
    normalize_shared_relative_channel_path, parse_shared_rfc3339_to_unix_ms,
    parse_tau_issue_command, post_issue_comment_chunks, render_event_prompt,
    render_issue_command_comment, render_issue_comment_chunks_with_limit,
    render_issue_comment_response_parts, retry_delay, run_prompt_for_event,
    shared_sanitize_for_path, shared_session_path_for_issue, DemoIndexRunCommand,
    DownloadedGithubAttachment, EventAction, GithubApiClient, GithubBridgeEvent,
    GithubBridgeEventKind, GithubIssue, GithubIssueComment, GithubIssueLabel,
    GithubIssuesBridgeRuntime, GithubIssuesBridgeRuntimeConfig, GithubIssuesBridgeStateStore,
    GithubUser, IssueDoctorCommand, IssueEventOutcome, PromptRunReport, PromptUsageSummary,
    RepoRef, RunPromptForEventRequest, SessionStore, TauIssueAuthCommand, TauIssueAuthCommandKind,
    TauIssueCommand, CHAT_SHOW_DEFAULT_LIMIT, DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS,
    DEMO_INDEX_SCENARIOS, EVENT_KEY_MARKER_PREFIX,
};
use crate::{
    channel_store::{ChannelArtifactRecord, ChannelStore},
    tools::ToolPolicy,
    AuthCommandConfig, CredentialStoreEncryptionMode, DoctorCommandConfig,
    DoctorMultiChannelReadinessConfig, PromptRunStatus, ProviderAuthMethod, RenderOptions,
};

struct StaticReplyClient;

#[async_trait]
impl LlmClient for StaticReplyClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        Ok(ChatResponse {
            message: Message::assistant_text("bridge reply"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage {
                input_tokens: 11,
                output_tokens: 7,
                total_tokens: 18,
                cached_input_tokens: 0,
            },
        })
    }
}

struct SlowReplyClient;

#[async_trait]
impl LlmClient for SlowReplyClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        sleep(Duration::from_millis(500)).await;
        Ok(ChatResponse {
            message: Message::assistant_text("slow bridge reply"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage {
                input_tokens: 5,
                output_tokens: 3,
                total_tokens: 8,
                cached_input_tokens: 0,
            },
        })
    }
}

fn test_bridge_config(base_url: &str, state_dir: &Path) -> GithubIssuesBridgeRuntimeConfig {
    test_bridge_config_with_client(base_url, state_dir, Arc::new(StaticReplyClient))
}

fn test_bridge_config_with_client(
    base_url: &str,
    state_dir: &Path,
    client: Arc<dyn LlmClient>,
) -> GithubIssuesBridgeRuntimeConfig {
    GithubIssuesBridgeRuntimeConfig {
        client,
        model: "openai/gpt-4o-mini".to_string(),
        system_prompt: "You are Tau.".to_string(),
        max_turns: 4,
        tool_policy: ToolPolicy::new(vec![state_dir.to_path_buf()]),
        turn_timeout_ms: 0,
        request_timeout_ms: 3_000,
        render_options: RenderOptions {
            stream_output: false,
            stream_delay_ms: 0,
        },
        session_lock_wait_ms: 2_000,
        session_lock_stale_ms: 30_000,
        state_dir: state_dir.to_path_buf(),
        repo_slug: "owner/repo".to_string(),
        api_base: base_url.to_string(),
        token: "test-token".to_string(),
        bot_login: Some("tau".to_string()),
        poll_interval: Duration::from_millis(1),
        poll_once: false,
        required_labels: Vec::new(),
        required_issue_numbers: Vec::new(),
        include_issue_body: false,
        include_edited_comments: true,
        processed_event_cap: 32,
        retry_max_attempts: 3,
        retry_base_delay_ms: 5,
        artifact_retention_days: 30,
        auth_command_config: AuthCommandConfig {
            credential_store: state_dir.join("credentials.json"),
            credential_store_key: None,
            credential_store_encryption: CredentialStoreEncryptionMode::None,
            api_key: Some("integration-key".to_string()),
            openai_api_key: None,
            anthropic_api_key: None,
            google_api_key: None,
            openai_auth_mode: ProviderAuthMethod::ApiKey,
            anthropic_auth_mode: ProviderAuthMethod::ApiKey,
            google_auth_mode: ProviderAuthMethod::ApiKey,
            provider_subscription_strict: true,
            openai_codex_backend: true,
            openai_codex_cli: "codex".to_string(),
            anthropic_claude_backend: true,
            anthropic_claude_cli: "claude".to_string(),
            google_gemini_backend: true,
            google_gemini_cli: "gemini".to_string(),
            google_gcloud_cli: "gcloud".to_string(),
        },
        demo_index_repo_root: None,
        demo_index_script_path: None,
        demo_index_binary_path: None,
        doctor_config: DoctorCommandConfig {
            model: "openai/gpt-4o-mini".to_string(),
            provider_keys: Vec::new(),
            release_channel_path: state_dir.join("release-channel.json"),
            release_lookup_cache_path: state_dir.join("release-cache.json"),
            release_lookup_cache_ttl_ms: 900_000,
            browser_automation_playwright_cli: "playwright".to_string(),
            session_enabled: true,
            session_path: state_dir.join("session.jsonl"),
            skills_dir: state_dir.join("skills"),
            skills_lock_path: state_dir.join("skills.lock.json"),
            trust_root_path: None,
            multi_channel_live_readiness: DoctorMultiChannelReadinessConfig::default(),
        },
    }
}

fn write_executable_script(path: &Path, body: &str) {
    std::fs::write(path, body).expect("write script");
    let status = std::process::Command::new("chmod")
        .arg("+x")
        .arg(path)
        .status()
        .expect("chmod script");
    assert!(status.success());
}

fn write_demo_index_list_stub(path: &Path) {
    write_executable_script(
        path,
        r#"#!/usr/bin/env bash
set -euo pipefail
cat <<'JSON'
{"schema_version":1,"scenarios":[{"id":"onboarding","wrapper":"local.sh","command":"./scripts/demo/local.sh","description":"Bootstrap local Tau state.","expected_markers":["[demo:local] PASS onboard-non-interactive"],"troubleshooting":"rerun local.sh"}]}
JSON
"#,
    );
}

fn write_demo_index_run_stub(path: &Path) {
    write_executable_script(
        path,
        r#"#!/usr/bin/env bash
set -euo pipefail
report_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --report-file)
      report_file="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
payload='{"schema_version":1,"scenarios":[{"id":"onboarding","status":"passed","exit_code":0,"duration_ms":9}],"summary":{"total":1,"passed":1,"failed":0}}'
if [[ -n "${report_file}" ]]; then
  mkdir -p "$(dirname "${report_file}")"
  printf '%s\n' "${payload}" > "${report_file}"
fi
printf '%s\n' "${payload}"
"#,
    );
}

fn test_issue_event() -> GithubBridgeEvent {
    GithubBridgeEvent {
        key: "issue-comment-created:200".to_string(),
        kind: GithubBridgeEventKind::CommentCreated,
        issue_number: 7,
        issue_title: "Bridge me".to_string(),
        author_login: "alice".to_string(),
        occurred_at: "2026-01-01T00:00:01Z".to_string(),
        body: "hello from issue stream".to_string(),
        raw_payload: json!({"id": 200}),
    }
}

fn test_prompt_run_report(reply: &str) -> PromptRunReport {
    PromptRunReport {
        run_id: "run-1".to_string(),
        model: "openai/gpt-4o-mini".to_string(),
        status: PromptRunStatus::Completed,
        assistant_reply: reply.to_string(),
        usage: PromptUsageSummary {
            input_tokens: 2,
            output_tokens: 3,
            total_tokens: 5,
            request_duration_ms: 0,
            finish_reason: None,
        },
        downloaded_attachments: Vec::new(),
        artifact: ChannelArtifactRecord {
            id: "artifact-1".to_string(),
            run_id: "run-1".to_string(),
            artifact_type: "github-issue-reply".to_string(),
            visibility: "private".to_string(),
            relative_path: "artifacts/run-1.md".to_string(),
            bytes: 123,
            checksum_sha256: "checksum".to_string(),
            created_unix_ms: 0,
            expires_unix_ms: None,
        },
    }
}

mod core_and_parsing;

mod command_workflows;

mod polling_and_replay;

mod chat_and_controls;

mod artifact_workflows;
