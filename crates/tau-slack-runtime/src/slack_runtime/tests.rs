//! Tests for Slack bridge runtime behavior and regressions.

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use httpmock::prelude::*;
use serde_json::json;
use tau_ai::{ChatRequest, ChatResponse, ChatUsage, LlmClient, Message, TauAiError};
use tempfile::tempdir;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use super::{
    dequeue_coalesced_event_for_run, event_is_stale, normalize_artifact_retention_days,
    normalize_socket_envelope, parse_slack_command, parse_socket_envelope, render_event_prompt,
    render_slack_artifact_markdown, render_slack_command_response, render_slack_response,
    run_prompt_for_event, DownloadedSlackFile, PollCycleReport, SlackApiClient, SlackBridgeEvent,
    SlackBridgeEventKind, SlackBridgeRuntime, SlackBridgeRuntimeConfig, SlackBridgeStateStore,
    SlackCommand, SlackSocketEnvelope,
};
use crate::{
    channel_store::{ChannelArtifactRecord, ChannelStore},
    current_unix_timestamp_ms,
    tools::ToolPolicy,
    RenderOptions, TransportHealthSnapshot,
};
use std::collections::VecDeque;

struct StaticReplyClient;

#[async_trait]
impl LlmClient for StaticReplyClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        Ok(ChatResponse {
            message: Message::assistant_text("slack bridge reply"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage {
                input_tokens: 13,
                output_tokens: 8,
                total_tokens: 21,
                cached_input_tokens: 0,
            },
        })
    }
}

struct SlowReplyClient;

#[async_trait]
impl LlmClient for SlowReplyClient {
    async fn complete(&self, _request: ChatRequest) -> Result<ChatResponse, TauAiError> {
        sleep(Duration::from_millis(300)).await;
        Ok(ChatResponse {
            message: Message::assistant_text("slow slack bridge reply"),
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

fn test_config(base_url: &str, state_dir: &Path) -> SlackBridgeRuntimeConfig {
    test_config_with_client(base_url, state_dir, Arc::new(StaticReplyClient))
}

fn test_config_with_client(
    base_url: &str,
    state_dir: &Path,
    client: Arc<dyn LlmClient>,
) -> SlackBridgeRuntimeConfig {
    SlackBridgeRuntimeConfig {
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
        api_base: base_url.to_string(),
        app_token: "xapp-test".to_string(),
        bot_token: "xoxb-test".to_string(),
        bot_user_id: Some("UBOT".to_string()),
        detail_thread_output: true,
        detail_thread_threshold_chars: 20,
        processed_event_cap: 32,
        max_event_age_seconds: 3_600,
        coalescing_window_ms: 0,
        reconnect_delay: Duration::from_millis(10),
        retry_max_attempts: 3,
        retry_base_delay_ms: 5,
        artifact_retention_days: 30,
    }
}

fn test_event_with_text(kind: SlackBridgeEventKind, text: &str) -> SlackBridgeEvent {
    SlackBridgeEvent {
        key: "k1".to_string(),
        kind,
        event_id: "Ev1".to_string(),
        occurred_unix_ms: 1,
        channel_id: "C1".to_string(),
        user_id: "U1".to_string(),
        text: text.to_string(),
        ts: "1.1".to_string(),
        thread_ts: None,
        files: vec![],
        raw_payload: json!({}),
    }
}

fn test_event() -> SlackBridgeEvent {
    SlackBridgeEvent {
        key: "event-c1-ts-10.0".to_string(),
        kind: SlackBridgeEventKind::AppMention,
        event_id: "Ev1".to_string(),
        occurred_unix_ms: 10_000,
        channel_id: "C1".to_string(),
        user_id: "U1".to_string(),
        text: "<@UBOT> hello".to_string(),
        ts: "10.0".to_string(),
        thread_ts: None,
        files: Vec::new(),
        raw_payload: json!({"event_id": "Ev1"}),
    }
}

fn queued_event(
    key: &str,
    user_id: &str,
    text: &str,
    occurred_unix_ms: u64,
    thread_ts: Option<&str>,
) -> SlackBridgeEvent {
    SlackBridgeEvent {
        key: key.to_string(),
        kind: SlackBridgeEventKind::AppMention,
        event_id: format!("Ev-{key}"),
        occurred_unix_ms,
        channel_id: "C1".to_string(),
        user_id: user_id.to_string(),
        text: text.to_string(),
        ts: format!("{occurred_unix_ms}.1"),
        thread_ts: thread_ts.map(ToOwned::to_owned),
        files: Vec::new(),
        raw_payload: json!({"event_id": key}),
    }
}

#[test]
fn unit_normalize_artifact_retention_days_maps_zero_to_none() {
    assert_eq!(normalize_artifact_retention_days(0), None);
    assert_eq!(normalize_artifact_retention_days(30), Some(30));
}

#[test]
fn spec_c01_try_start_queued_runs_respects_coalescing_window_before_dispatch() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event("e1", "U1", "hello", 1_000, None));

    let maybe_event = dequeue_coalesced_event_for_run(&mut queue, 2_500, 2_000);
    assert!(maybe_event.is_none());
    assert_eq!(queue.len(), 1);
}

#[test]
fn spec_c02_dequeue_coalesced_run_batches_same_user_and_thread_messages() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event(
        "e1",
        "U1",
        "line one",
        1_000,
        Some("thread-1"),
    ));
    queue.push_back(queued_event(
        "e2",
        "U1",
        "line two",
        1_500,
        Some("thread-1"),
    ));
    queue.push_back(queued_event(
        "e3",
        "U1",
        "line three",
        2_000,
        Some("thread-1"),
    ));

    let event = dequeue_coalesced_event_for_run(&mut queue, 10_000, 2_000)
        .expect("expected coalesced event");
    assert_eq!(event.text, "line one\nline two\nline three");
    assert!(queue.is_empty());
}

#[test]
fn regression_spec_c03_dequeue_coalesced_run_preserves_non_coalescible_tail() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event("e1", "U1", "alpha", 1_000, None));
    queue.push_back(queued_event("e2", "U2", "beta", 1_200, None));
    queue.push_back(queued_event("e3", "U1", "gamma", 1_400, None));

    let event =
        dequeue_coalesced_event_for_run(&mut queue, 10_000, 2_000).expect("expected first event");
    assert_eq!(event.text, "alpha");
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].key, "e2");
    assert_eq!(queue[1].key, "e3");
}

#[test]
fn regression_dequeue_coalesced_run_allows_equal_timestamps() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event("e1", "U1", "alpha", 1_000, Some("thread-1")));
    queue.push_back(queued_event("e2", "U1", "beta", 1_000, Some("thread-1")));

    let event = dequeue_coalesced_event_for_run(&mut queue, 10_000, 2_000)
        .expect("expected coalesced event");
    assert_eq!(event.text, "alpha\nbeta");
    assert!(queue.is_empty());
}

#[test]
fn regression_dequeue_coalesced_run_rejects_out_of_order_timestamps() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event("e1", "U1", "alpha", 2_000, Some("thread-1")));
    queue.push_back(queued_event("e2", "U1", "beta", 1_900, Some("thread-1")));

    let event =
        dequeue_coalesced_event_for_run(&mut queue, 10_000, 2_000).expect("expected first event");
    assert_eq!(event.text, "alpha");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].key, "e2");
}

#[test]
fn regression_dequeue_coalesced_run_skips_empty_text_fragments() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event("e1", "U1", "alpha", 1_000, Some("thread-1")));
    queue.push_back(queued_event("e2", "U1", "", 1_200, Some("thread-1")));

    let event =
        dequeue_coalesced_event_for_run(&mut queue, 10_000, 2_000).expect("expected first event");
    assert_eq!(event.text, "alpha");
    assert!(queue.is_empty());
}

#[test]
fn regression_dequeue_coalesced_run_dispatches_on_exact_window_boundary() {
    let mut queue = VecDeque::new();
    queue.push_back(queued_event("e1", "U1", "alpha", 8_000, Some("thread-1")));

    let event = dequeue_coalesced_event_for_run(&mut queue, 10_000, 2_000)
        .expect("expected boundary dequeue");
    assert_eq!(event.key, "e1");
    assert!(queue.is_empty());
}

#[tokio::test]
async fn functional_run_prompt_for_event_sets_expiry_with_default_retention() {
    let temp = tempdir().expect("tempdir");
    let config = test_config("http://unused.local/api", temp.path());
    let event = test_event();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let slack_client = SlackApiClient::new(
        config.api_base.clone(),
        config.app_token.clone(),
        config.bot_token.clone(),
        config.request_timeout_ms,
        config.retry_max_attempts,
        config.retry_base_delay_ms,
    )
    .expect("slack client");

    let report = run_prompt_for_event(
        &config,
        temp.path(),
        &event,
        "run-default-retention",
        cancel_rx,
        &slack_client,
        "UBOT",
    )
    .await
    .expect("run prompt");
    assert!(report.artifact.expires_unix_ms.is_some());
}

#[tokio::test]
async fn regression_run_prompt_for_event_zero_retention_disables_expiry() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_config("http://unused.local/api", temp.path());
    config.artifact_retention_days = 0;
    let event = test_event();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let slack_client = SlackApiClient::new(
        config.api_base.clone(),
        config.app_token.clone(),
        config.bot_token.clone(),
        config.request_timeout_ms,
        config.retry_max_attempts,
        config.retry_base_delay_ms,
    )
    .expect("slack client");

    let report = run_prompt_for_event(
        &config,
        temp.path(),
        &event,
        "run-zero-retention",
        cancel_rx,
        &slack_client,
        "UBOT",
    )
    .await
    .expect("run prompt");
    assert_eq!(report.artifact.expires_unix_ms, None);

    let store =
        ChannelStore::open(&temp.path().join("channel-store"), "slack", "C1").expect("store");
    let active = store
        .list_active_artifacts(current_unix_timestamp_ms())
        .expect("active artifacts");
    assert_eq!(active.len(), 1);
}

#[test]
fn unit_parse_socket_envelope_handles_text_binary_and_ping() {
    let text = WsMessage::Text(
        json!({
            "envelope_id": "1",
            "type": "events_api",
            "payload": {
                "type": "event_callback",
                "event_id": "Ev1",
                "event_time": 10,
                "event": {
                    "type": "app_mention",
                    "user": "U1",
                    "channel": "C1",
                    "text": "hi",
                    "ts": "10.0"
                }
            }
        })
        .to_string()
        .into(),
    );
    let parsed = parse_socket_envelope(text).expect("parse text");
    assert!(parsed.is_some());

    let binary = WsMessage::Binary(
        json!({
            "envelope_id": "2",
            "type": "events_api",
            "payload": {
                "type": "event_callback",
                "event_id": "Ev2",
                "event_time": 10,
                "event": {
                    "type": "message",
                    "channel_type": "im",
                    "user": "U2",
                    "channel": "D1",
                    "text": "dm",
                    "ts": "10.1"
                }
            }
        })
        .to_string()
        .into_bytes()
        .into(),
    );
    assert!(parse_socket_envelope(binary)
        .expect("parse binary")
        .is_some());
    assert!(parse_socket_envelope(WsMessage::Ping(vec![].into()))
        .expect("ping")
        .is_none());
}

#[test]
fn unit_normalize_socket_envelope_maps_mentions_and_dms() {
    let mention = SlackSocketEnvelope {
        envelope_id: "env1".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "Ev1",
            "event_time": 199,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> hi",
                "ts": "199.1"
            }
        }),
    };
    let mention_event = normalize_socket_envelope(&mention, "UBOT")
        .expect("normalize mention")
        .expect("mention event");
    assert_eq!(mention_event.kind, SlackBridgeEventKind::AppMention);

    let dm = SlackSocketEnvelope {
        envelope_id: "env2".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "Ev2",
            "event_time": 199,
            "event": {
                "type": "message",
                "channel_type": "im",
                "user": "U2",
                "channel": "D123",
                "text": "hello",
                "ts": "199.2"
            }
        }),
    };
    let dm_event = normalize_socket_envelope(&dm, "UBOT")
        .expect("normalize dm")
        .expect("dm event");
    assert_eq!(dm_event.kind, SlackBridgeEventKind::DirectMessage);
}

#[test]
fn functional_render_event_prompt_includes_downloaded_files() {
    let event = SlackBridgeEvent {
        key: "k1".to_string(),
        kind: SlackBridgeEventKind::AppMention,
        event_id: "Ev1".to_string(),
        occurred_unix_ms: 1,
        channel_id: "C1".to_string(),
        user_id: "U1".to_string(),
        text: "<@UBOT> analyze this".to_string(),
        ts: "1.1".to_string(),
        thread_ts: None,
        files: vec![],
        raw_payload: json!({}),
    };
    let files = vec![DownloadedSlackFile {
        id: "F1".to_string(),
        original_name: "report.txt".to_string(),
        path: Path::new("/tmp/report.txt").to_path_buf(),
        mimetype: Some("text/plain".to_string()),
        size: Some(120),
    }];
    let prompt = render_event_prompt(&event, "UBOT", &files);
    assert!(prompt.contains("Downloaded attachments"));
    assert!(prompt.contains("report.txt"));
    assert!(!prompt.contains("<@UBOT>"));
}

#[test]
fn unit_parse_slack_command_supports_known_commands() {
    let mention = test_event_with_text(SlackBridgeEventKind::AppMention, "<@UBOT> /tau help");
    assert_eq!(
        parse_slack_command(&mention, "UBOT"),
        Some(SlackCommand::Help)
    );
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau status");
    assert_eq!(parse_slack_command(&dm, "UBOT"), Some(SlackCommand::Status));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau health");
    assert_eq!(parse_slack_command(&dm, "UBOT"), Some(SlackCommand::Health));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau stop");
    assert_eq!(parse_slack_command(&dm, "UBOT"), Some(SlackCommand::Stop));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau artifacts");
    assert_eq!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Artifacts {
            purge: false,
            run_id: None
        })
    );
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau artifacts purge");
    assert_eq!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Artifacts {
            purge: true,
            run_id: None
        })
    );
    let dm = test_event_with_text(
        SlackBridgeEventKind::DirectMessage,
        "/tau artifacts run run-9",
    );
    assert_eq!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Artifacts {
            purge: false,
            run_id: Some("run-9".to_string())
        })
    );
    let dm = test_event_with_text(
        SlackBridgeEventKind::DirectMessage,
        "/tau artifacts show artifact-9",
    );
    assert_eq!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::ArtifactShow {
            artifact_id: "artifact-9".to_string()
        })
    );
    let dm = test_event_with_text(
        SlackBridgeEventKind::DirectMessage,
        "/tau canvas show architecture --json",
    );
    assert_eq!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Canvas {
            args: "show architecture --json".to_string()
        })
    );
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "hello");
    assert_eq!(parse_slack_command(&dm, "UBOT"), None);
}

#[test]
fn regression_parse_slack_command_rejects_invalid_forms() {
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau help extra");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau health extra");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau artifacts extra");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau artifacts run");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(
        SlackBridgeEventKind::DirectMessage,
        "/tau artifacts run a b",
    );
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau artifacts show");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau canvas");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
    let dm = test_event_with_text(SlackBridgeEventKind::DirectMessage, "/tau unknown");
    assert!(matches!(
        parse_slack_command(&dm, "UBOT"),
        Some(SlackCommand::Invalid { .. })
    ));
}

#[test]
fn functional_render_slack_artifact_markdown_includes_event_and_run_metadata() {
    let event = SlackBridgeEvent {
        key: "k1".to_string(),
        kind: SlackBridgeEventKind::DirectMessage,
        event_id: "Ev1".to_string(),
        occurred_unix_ms: 1,
        channel_id: "D1".to_string(),
        user_id: "U1".to_string(),
        text: "hello".to_string(),
        ts: "1.1".to_string(),
        thread_ts: None,
        files: vec![],
        raw_payload: json!({}),
    };
    let markdown = render_slack_artifact_markdown(
        &event,
        "run-1",
        crate::PromptRunStatus::Completed,
        "reply body",
        &[],
    );
    assert!(markdown.contains("# Tau Slack Artifact"));
    assert!(markdown.contains("channel_id: D1"));
    assert!(markdown.contains("run_id: run-1"));
    assert!(markdown.contains("status: completed"));
    assert!(markdown.contains("attachments: none"));
    assert!(markdown.contains("## Assistant Reply"));
    assert!(markdown.contains("reply body"));
}

#[test]
fn functional_render_slack_response_thread_splits_long_output() {
    let event = SlackBridgeEvent {
        key: "k1".to_string(),
        kind: SlackBridgeEventKind::DirectMessage,
        event_id: "Ev1".to_string(),
        occurred_unix_ms: 1,
        channel_id: "D1".to_string(),
        user_id: "U1".to_string(),
        text: "hello".to_string(),
        ts: "1.1".to_string(),
        thread_ts: None,
        files: vec![],
        raw_payload: json!({}),
    };
    let run = super::PromptRunReport {
        run_id: "run1".to_string(),
        model: "openai/gpt-4o-mini".to_string(),
        status: crate::PromptRunStatus::Completed,
        assistant_reply: "abcdefghijklmnopqrstuvwxyz".to_string(),
        usage: super::PromptUsageSummary {
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            request_duration_ms: 10,
            finish_reason: Some("stop".to_string()),
        },
        downloaded_files: vec![],
        artifact: ChannelArtifactRecord {
            id: "artifact-1".to_string(),
            run_id: "run1".to_string(),
            artifact_type: "slack-reply".to_string(),
            visibility: "private".to_string(),
            relative_path: "artifacts/run1/slack-reply-artifact-1.md".to_string(),
            bytes: 42,
            checksum_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            created_unix_ms: 1,
            expires_unix_ms: Some(2),
        },
    };
    let (summary, detail) = render_slack_response(&event, &run, true, 10);
    assert!(summary.contains("full response posted in this thread"));
    assert!(summary.contains("artifact artifacts/run1/slack-reply-artifact-1.md"));
    assert_eq!(detail.as_deref(), Some("abcdefghijklmnopqrstuvwxyz"));
}

#[tokio::test]
async fn functional_render_channel_status_includes_transport_health_fields() {
    let server = MockServer::start();
    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let status = runtime.render_channel_status("C1");
    assert!(status.contains("Tau status for channel C1: idle"));
    assert!(status.contains("transport_failure_streak: 0"));
    assert!(status.contains("transport_last_cycle_processed: 0"));
}

#[tokio::test]
async fn functional_render_channel_health_includes_classification_and_transport_fields() {
    let server = MockServer::start();
    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let health = runtime.render_channel_health("C1");
    assert!(health.contains("Tau health for channel C1: healthy"));
    assert!(health.contains("runtime_state: idle"));
    assert!(health.contains("active_run_id: none"));
    assert!(health.contains("transport_health_reason: "));
    assert!(health.contains("transport_health_recommendation: "));
    assert!(health.contains("transport_failure_streak: 0"));
}

#[tokio::test]
async fn regression_render_channel_health_reports_failing_failure_streak() {
    let server = MockServer::start();
    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");
    let mut health = runtime.state_store.transport_health().clone();
    health.failure_streak = 3;
    runtime.state_store.update_transport_health(health);

    let rendered = runtime.render_channel_health("C1");
    assert!(rendered.contains("Tau health for channel C1: failing"));
    assert!(rendered.contains("failure_streak=3"));
}

#[test]
fn unit_render_slack_command_response_appends_marker_footer() {
    let event = SlackBridgeEvent {
        key: "EvHelp:C1:12.1".to_string(),
        kind: SlackBridgeEventKind::AppMention,
        event_id: "EvHelp".to_string(),
        occurred_unix_ms: 1,
        channel_id: "C1".to_string(),
        user_id: "U1".to_string(),
        text: "<@UBOT> /tau help".to_string(),
        ts: "12.1".to_string(),
        thread_ts: None,
        files: vec![],
        raw_payload: json!({}),
    };

    let rendered = render_slack_command_response(&event, "help", "reported", "hello world");
    assert!(rendered.contains("hello world"));
    assert!(rendered.contains("tau-slack-event:EvHelp:C1:12.1"));
    assert!(rendered.contains("Tau command `help` | status `reported`"));
}

#[test]
fn regression_event_is_stale_respects_threshold() {
    let event = SlackBridgeEvent {
        key: "k1".to_string(),
        kind: SlackBridgeEventKind::DirectMessage,
        event_id: "Ev1".to_string(),
        occurred_unix_ms: 1_000,
        channel_id: "D1".to_string(),
        user_id: "U1".to_string(),
        text: "hello".to_string(),
        ts: "1.1".to_string(),
        thread_ts: None,
        files: vec![],
        raw_payload: json!({}),
    };
    assert!(event_is_stale(&event, 1, 4_000));
    assert!(!event_is_stale(&event, 10, 4_000));
}

#[test]
fn regression_state_store_caps_processed_history() {
    let temp = tempdir().expect("tempdir");
    let state_path = temp.path().join("state.json");
    let mut store = SlackBridgeStateStore::load(state_path, 2).expect("load store");
    assert!(store.mark_processed("a"));
    assert!(store.mark_processed("b"));
    assert!(store.mark_processed("c"));
    assert!(!store.contains("a"));
    assert!(store.contains("b"));
    assert!(store.contains("c"));
}

#[test]
fn regression_state_store_loads_legacy_state_without_health_snapshot() {
    let temp = tempdir().expect("tempdir");
    let state_path = temp.path().join("state.json");
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["a", "b"]
}
"#,
    )
    .expect("write legacy state");

    let store = SlackBridgeStateStore::load(state_path, 8).expect("load store");
    assert!(store.contains("a"));
    assert!(store.contains("b"));
    assert_eq!(
        store.transport_health(),
        &TransportHealthSnapshot::default()
    );
}

#[tokio::test]
async fn integration_slack_api_client_retries_rate_limits() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .header("x-tau-retry-attempt", "0");
        then.status(429)
            .header("retry-after", "0")
            .body("rate limit");
    });
    let second = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .header("x-tau-retry-attempt", "1");
        then.status(200).json_body(json!({
            "ok": true,
            "channel": "C1",
            "ts": "1.2"
        }));
    });

    let client = SlackApiClient::new(
        server.base_url(),
        "xapp-test".to_string(),
        "xoxb-test".to_string(),
        2_000,
        3,
        1,
    )
    .expect("client");

    let posted = client
        .post_message("C1", "hello", None)
        .await
        .expect("post message eventually succeeds");
    assert_eq!(posted.channel, "C1");
    assert_eq!(posted.ts, "1.2");
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
}

#[tokio::test]
async fn integration_runtime_queues_per_channel_and_processes_runs() {
    let server = MockServer::start();
    let auth = server.mock(|when, then| {
        when.method(POST).path("/auth.test");
        then.status(200)
            .json_body(json!({"ok": true, "user_id": "UBOT"}));
    });
    let post_working = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Tau is working on run");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "2.0"}));
    });
    let post_working_dm = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"D1\"")
            .body_includes("Tau is working on run");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "D1", "ts": "3.0"}));
    });
    let update = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.update")
            .body_includes("artifact artifacts/");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "2.0"}));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");
    auth.assert_calls(0);

    let envelope1 = SlackSocketEnvelope {
        envelope_id: "env1".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "Ev1",
            "event_time": (current_unix_timestamp_ms() / 1000),
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> status",
                "ts": "10.1"
            }
        }),
    };
    let envelope2 = SlackSocketEnvelope {
        envelope_id: "env2".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "Ev2",
            "event_time": (current_unix_timestamp_ms() / 1000),
            "event": {
                "type": "message",
                "channel_type": "im",
                "user": "U2",
                "channel": "D1",
                "text": "help",
                "ts": "10.2"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(envelope1, &mut report)
        .await
        .expect("handle envelope1");
    runtime
        .handle_envelope(envelope2, &mut report)
        .await
        .expect("handle envelope2");

    runtime
        .try_start_queued_runs(&mut report)
        .await
        .expect("start runs");

    let deadline = Instant::now() + Duration::from_secs(3);
    while report.completed_runs < 2 && Instant::now() < deadline {
        sleep(Duration::from_millis(50)).await;
        runtime
            .drain_finished_runs(&mut report)
            .await
            .expect("drain runs");
        runtime
            .try_start_queued_runs(&mut report)
            .await
            .expect("start runs");
    }

    assert!(report.queued_events >= 2);
    assert!(report.completed_runs >= 2);
    assert!(post_working.calls() >= 1);
    assert!(post_working_dm.calls() >= 1);
    assert!(update.calls() >= 1);

    let channel_dir = temp.path().join("channel-store/channels/slack/C1");
    let channel_log =
        std::fs::read_to_string(channel_dir.join("log.jsonl")).expect("channel log exists");
    let channel_context =
        std::fs::read_to_string(channel_dir.join("context.jsonl")).expect("channel context exists");
    assert!(channel_log.contains("\"direction\":\"inbound\""));
    assert!(channel_log.contains("\"direction\":\"outbound\""));
    assert!(channel_log.contains("\"artifact\""));
    assert!(channel_context.contains("slack bridge reply"));
    let artifact_index = std::fs::read_to_string(channel_dir.join("artifacts/index.jsonl"))
        .expect("artifact index exists");
    assert!(artifact_index.contains("\"artifact_type\":\"slack-reply\""));

    runtime
        .persist_transport_health(&report, 33, 0)
        .expect("persist transport health");
    let state_raw =
        std::fs::read_to_string(temp.path().join("state.json")).expect("read state file");
    let state_json: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
    let health = state_json
        .get("health")
        .and_then(serde_json::Value::as_object)
        .expect("health object");
    assert_eq!(
        health
            .get("last_cycle_completed")
            .and_then(serde_json::Value::as_u64),
        Some(report.completed_runs as u64)
    );
    assert_eq!(
        health
            .get("last_cycle_duplicates")
            .and_then(serde_json::Value::as_u64),
        Some(report.skipped_duplicate_events as u64)
    );
    assert!(
        health
            .get("updated_unix_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
            > 0
    );
}

#[tokio::test]
async fn integration_bridge_commands_post_control_messages() {
    let server = MockServer::start();
    let help_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Supported `/tau` commands:")
            .body_includes("tau-slack-event:EvHelp:C1:12.1");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "4.0"}));
    });
    let status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Tau status for channel C1: idle")
            .body_includes("tau-slack-event:EvStatus:C1:12.2")
            .body_includes("transport_failure_streak: 0");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "4.1"}));
    });
    let health_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Tau health for channel C1: healthy")
            .body_includes("tau-slack-event:EvHealth:C1:12.3")
            .body_includes("transport_health_reason:")
            .body_includes("transport_health_recommendation:");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "4.2"}));
    });
    let stop_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("No active run for this channel. Current state is idle.")
            .body_includes("tau-slack-event:EvStop:C1:12.4");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "4.3"}));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let now_seconds = current_unix_timestamp_ms() / 1000;
    let help = SlackSocketEnvelope {
        envelope_id: "env-help".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvHelp",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau help",
                "ts": "12.1"
            }
        }),
    };
    let status = SlackSocketEnvelope {
        envelope_id: "env-status".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvStatus",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau status",
                "ts": "12.2"
            }
        }),
    };
    let stop = SlackSocketEnvelope {
        envelope_id: "env-stop".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvStop",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau stop",
                "ts": "12.4"
            }
        }),
    };
    let health = SlackSocketEnvelope {
        envelope_id: "env-health".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvHealth",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau health",
                "ts": "12.3"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(help, &mut report)
        .await
        .expect("help");
    runtime
        .handle_envelope(status, &mut report)
        .await
        .expect("status");
    runtime
        .handle_envelope(health, &mut report)
        .await
        .expect("health");
    runtime
        .handle_envelope(stop, &mut report)
        .await
        .expect("stop");

    help_post.assert_calls(1);
    status_post.assert_calls(1);
    health_post.assert_calls(1);
    stop_post.assert_calls(1);
    assert_eq!(report.queued_events, 0);

    let outbound = std::fs::read_to_string(temp.path().join("outbound-events.jsonl"))
        .expect("read outbound events");
    assert!(outbound.contains("\"command\":\"help\""));
    assert!(outbound.contains("\"command\":\"status\""));
    assert!(outbound.contains("\"command\":\"health\""));
    assert!(outbound.contains("\"command\":\"stop\""));
    assert!(outbound.contains("\"response_marker\":\"<!-- tau-slack-event:EvHelp:C1:12.1 -->\""));
    assert!(outbound.contains("\"response_marker\":\"<!-- tau-slack-event:EvStatus:C1:12.2 -->\""));
    assert!(outbound.contains("\"response_marker\":\"<!-- tau-slack-event:EvHealth:C1:12.3 -->\""));
    assert!(outbound.contains("\"response_marker\":\"<!-- tau-slack-event:EvStop:C1:12.4 -->\""));

    let channel_log = std::fs::read_to_string(
        temp.path()
            .join("channel-store/channels/slack/C1/log.jsonl"),
    )
    .expect("read channel log");
    assert!(channel_log.contains("\"kind\":\"command_response\""));
    assert!(channel_log.contains("\"response_marker\":\"<!-- tau-slack-event:EvHelp:C1:12.1 -->\""));
}

#[tokio::test]
async fn integration_bridge_canvas_command_posts_state_and_persists_replay_event() {
    let server = MockServer::start();
    let canvas_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("canvas create: id=architecture")
            .body_includes("event_id=");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "4.9"}));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");
    let now_seconds = current_unix_timestamp_ms() / 1000;
    let canvas = SlackSocketEnvelope {
        envelope_id: "env-canvas".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvCanvas",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau canvas create architecture",
                "ts": "12.9"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(canvas, &mut report)
        .await
        .expect("canvas command");
    canvas_post.assert_calls(1);
    assert_eq!(report.queued_events, 0);

    let events_path = temp.path().join("canvas/architecture/events.jsonl");
    let payload = std::fs::read_to_string(events_path).expect("events");
    assert!(payload.contains("\"event_id\":"));
    assert!(payload.contains("\"transport\":\"slack\""));
    assert!(payload.contains("\"source_event_key\":\"EvCanvas:C1:12.9\""));
}

#[tokio::test]
async fn integration_bridge_artifacts_commands_post_reports() {
    let server = MockServer::start();
    let list_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Tau artifacts for channel C1: active=1");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "5.0"}));
    });
    let show_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Tau artifact for channel C1 id");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "5.1"}));
    });

    let temp = tempdir().expect("tempdir");
    let store =
        ChannelStore::open(&temp.path().join("channel-store"), "slack", "C1").expect("store");
    let artifact = store
        .write_text_artifact("run-1", "slack-reply", "private", Some(30), "md", "hi")
        .expect("artifact");

    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let now_seconds = current_unix_timestamp_ms() / 1000;
    let list = SlackSocketEnvelope {
        envelope_id: "env-artifacts".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvArtifacts",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau artifacts",
                "ts": "13.1"
            }
        }),
    };
    let show = SlackSocketEnvelope {
        envelope_id: "env-artifact-show".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvArtifactShow",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": format!("<@UBOT> /tau artifacts show {}", artifact.id),
                "ts": "13.2"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(list, &mut report)
        .await
        .expect("list");
    runtime
        .handle_envelope(show, &mut report)
        .await
        .expect("show");

    list_post.assert_calls(1);
    show_post.assert_calls(1);
    assert_eq!(report.queued_events, 0);
}

#[tokio::test]
async fn integration_bridge_denies_unpaired_actor_in_strict_mode() {
    let server = MockServer::start();
    let post = server.mock(|when, then| {
        when.method(POST).path("/chat.postMessage");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "1.1"}));
    });

    let temp = tempdir().expect("tempdir");
    let security_dir = temp.path().join("security");
    std::fs::create_dir_all(&security_dir).expect("security dir");
    std::fs::write(
        security_dir.join("allowlist.json"),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {}
}
"#,
    )
    .expect("write strict allowlist");

    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let now_seconds = current_unix_timestamp_ms() / 1000;
    let envelope = SlackSocketEnvelope {
        envelope_id: "env-deny".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvDeny",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U-unknown",
                "channel": "C1",
                "text": "<@UBOT> hello",
                "ts": "55.1"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(envelope, &mut report)
        .await
        .expect("handle envelope");

    assert_eq!(report.discovered_events, 1);
    assert_eq!(report.queued_events, 0);
    assert_eq!(report.failed_events, 0);
    post.assert_calls(0);

    let outbound = std::fs::read_to_string(temp.path().join("outbound-events.jsonl"))
        .expect("read outbound log");
    assert!(outbound.contains("\"status\":\"denied\""));
    assert!(outbound.contains("\"reason_code\":\"deny_actor_not_paired_or_allowlisted\""));
}

#[tokio::test]
async fn integration_bridge_denies_unbound_actor_in_rbac_team_mode() {
    let server = MockServer::start();
    let post = server.mock(|when, then| {
        when.method(POST).path("/chat.postMessage");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "1.1"}));
    });

    let temp = tempdir().expect("tempdir");
    let security_dir = temp.path().join("security");
    std::fs::create_dir_all(&security_dir).expect("security dir");
    std::fs::write(
        security_dir.join("allowlist.json"),
        r#"{
  "schema_version": 1,
  "strict": true,
  "channels": {
"slack:C1": ["U1"]
  }
}
"#,
    )
    .expect("write strict allowlist");
    std::fs::write(
        security_dir.join("rbac.json"),
        r#"{
  "schema_version": 1,
  "team_mode": true,
  "bindings": [],
  "roles": {}
}
"#,
    )
    .expect("write rbac policy");

    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let now_seconds = current_unix_timestamp_ms() / 1000;
    let envelope = SlackSocketEnvelope {
        envelope_id: "env-rbac-deny".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvRbacDeny",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau status",
                "ts": "56.1"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(envelope, &mut report)
        .await
        .expect("handle envelope");

    assert_eq!(report.discovered_events, 1);
    assert_eq!(report.queued_events, 0);
    assert_eq!(report.failed_events, 0);
    post.assert_calls(0);

    let outbound = std::fs::read_to_string(temp.path().join("outbound-events.jsonl"))
        .expect("read outbound log");
    assert!(outbound.contains("\"command\":\"rbac-authorization\""));
    assert!(outbound.contains("\"status\":\"denied\""));
    assert!(outbound.contains("\"reason_code\":\"deny_unbound_principal\""));
}

#[tokio::test]
async fn regression_duplicate_and_stale_events_do_not_trigger_runs() {
    let server = MockServer::start();
    let post = server.mock(|when, then| {
        when.method(POST).path("/chat.postMessage");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "1.1"}));
    });

    let temp = tempdir().expect("tempdir");
    let mut config =
        test_config_with_client(&server.base_url(), temp.path(), Arc::new(SlowReplyClient));
    config.max_event_age_seconds = 5;
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let now_seconds = current_unix_timestamp_ms() / 1000;
    let fresh = SlackSocketEnvelope {
        envelope_id: "env1".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvSame",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> hello",
                "ts": "11.1"
            }
        }),
    };
    let stale = SlackSocketEnvelope {
        envelope_id: "env2".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvOld",
            "event_time": now_seconds.saturating_sub(15),
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> old",
                "ts": "11.2"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(fresh.clone(), &mut report)
        .await
        .expect("fresh first");
    runtime
        .handle_envelope(fresh, &mut report)
        .await
        .expect("fresh duplicate");
    runtime
        .handle_envelope(stale, &mut report)
        .await
        .expect("stale event");

    assert_eq!(report.skipped_duplicate_events, 1);
    assert_eq!(report.skipped_stale_events, 1);

    runtime
        .try_start_queued_runs(&mut report)
        .await
        .expect("start queued");
    assert_eq!(post.calls(), 1);
}

#[tokio::test]
async fn regression_duplicate_command_events_remain_idempotent_with_markers() {
    let server = MockServer::start();
    let help_post = server.mock(|when, then| {
        when.method(POST)
            .path("/chat.postMessage")
            .body_includes("\"channel\":\"C1\"")
            .body_includes("Supported `/tau` commands:")
            .body_includes("tau-slack-event:EvDup:C1:55.1");
        then.status(200)
            .json_body(json!({"ok": true, "channel": "C1", "ts": "8.0"}));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_config(&server.base_url(), temp.path());
    let mut runtime = SlackBridgeRuntime::new(config).await.expect("runtime");

    let now_seconds = current_unix_timestamp_ms() / 1000;
    let envelope = SlackSocketEnvelope {
        envelope_id: "env-dup-command".to_string(),
        envelope_type: "events_api".to_string(),
        payload: json!({
            "type": "event_callback",
            "event_id": "EvDup",
            "event_time": now_seconds,
            "event": {
                "type": "app_mention",
                "user": "U1",
                "channel": "C1",
                "text": "<@UBOT> /tau help",
                "ts": "55.1"
            }
        }),
    };

    let mut report = PollCycleReport::default();
    runtime
        .handle_envelope(envelope.clone(), &mut report)
        .await
        .expect("first command event");
    runtime
        .handle_envelope(envelope, &mut report)
        .await
        .expect("duplicate command event");

    help_post.assert_calls(1);
    assert_eq!(report.skipped_duplicate_events, 1);

    let outbound = std::fs::read_to_string(temp.path().join("outbound-events.jsonl"))
        .expect("read outbound events");
    assert_eq!(outbound.matches("\"command\":\"help\"").count(), 1);
    assert!(outbound.contains("\"response_marker\":\"<!-- tau-slack-event:EvDup:C1:55.1 -->\""));

    let channel_log = std::fs::read_to_string(
        temp.path()
            .join("channel-store/channels/slack/C1/log.jsonl"),
    )
    .expect("read channel log");
    assert_eq!(
        channel_log.matches("\"kind\":\"command_response\"").count(),
        1
    );
    assert!(channel_log.contains("\"response_marker\":\"<!-- tau-slack-event:EvDup:C1:55.1 -->\""));
}
