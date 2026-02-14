//! Polling retry, replay safety, and update path coverage for runtime tests.

use super::*;

/// Exercises polling retry and replay-safe comment update behavior.
#[tokio::test]
async fn integration_github_api_client_retries_rate_limits() {
    let server = MockServer::start();
    let first = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues")
            .header("x-tau-retry-attempt", "0");
        then.status(429)
            .header("retry-after", "0")
            .body("rate limit");
    });
    let second = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues")
            .header("x-tau-retry-attempt", "1");
        then.status(200).json_body(json!([]));
    });

    let repo = RepoRef::parse("owner/repo").expect("repo parse");
    let client = GithubApiClient::new(server.base_url(), "token".to_string(), repo, 2_000, 3, 1)
        .expect("client");
    let issues = client
        .list_updated_issues(None)
        .await
        .expect("list issues should eventually succeed");
    assert!(issues.is_empty());
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
}

#[tokio::test]
async fn integration_bridge_poll_processes_issue_comment_and_posts_reply() {
    let server = MockServer::start();
    let issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 10,
            "number": 7,
            "title": "Bridge me",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let comments = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/7/comments");
        then.status(200).json_body(json!([{
            "id": 200,
            "body": "hello from issue stream",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/901")
            .body_includes("bridge reply")
            .body_includes("tau-event-key:issue-comment-created:200")
            .body_includes("artifact `artifacts/");
        then.status(200).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });
    let fallback_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("warning: failed to update placeholder comment");
        then.status(201).json_body(json!({
            "id": 999,
            "html_url": "https://example.test/comment/999"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    assert_eq!(first.discovered_events, 1);
    assert_eq!(first.processed_events, 1);
    assert_eq!(first.failed_events, 0);

    let state_path = temp.path().join("owner__repo").join("state.json");
    let state_raw = std::fs::read_to_string(&state_path).expect("state file");
    let state: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
    let health = state
        .get("health")
        .and_then(serde_json::Value::as_object)
        .expect("health object");
    assert_eq!(
        health
            .get("last_cycle_discovered")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        health
            .get("last_cycle_processed")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        health
            .get("last_cycle_failed")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        health
            .get("failure_streak")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert!(
        health
            .get("updated_unix_ms")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
            > 0
    );

    let second = runtime.poll_once().await.expect("second poll");
    assert_eq!(second.processed_events, 0);
    issues.assert_calls(2);
    comments.assert_calls(2);
    working_post.assert_calls(1);
    update.assert_calls(1);
    fallback_post.assert_calls(0);

    let outbound = std::fs::read_to_string(
        temp.path()
            .join("owner__repo")
            .join("outbound-events.jsonl"),
    )
    .expect("read outbound log");
    assert!(outbound.contains("\"posted_comment_id\":901"));
    let channel_dir = temp
        .path()
        .join("owner__repo")
        .join("channel-store/channels/github/issue-7");
    let channel_log =
        std::fs::read_to_string(channel_dir.join("log.jsonl")).expect("channel log exists");
    let channel_context =
        std::fs::read_to_string(channel_dir.join("context.jsonl")).expect("channel context exists");
    assert!(channel_log.contains("\"direction\":\"inbound\""));
    assert!(channel_log.contains("\"direction\":\"outbound\""));
    assert!(channel_log.contains("\"artifact\""));
    assert!(channel_context.contains("bridge reply"));
    let artifact_index = std::fs::read_to_string(channel_dir.join("artifacts/index.jsonl"))
        .expect("artifact index exists");
    assert!(artifact_index.contains("\"artifact_type\":\"github-issue-reply\""));
}

#[tokio::test]
async fn integration_bridge_poll_filters_issues_by_required_label() {
    let server = MockServer::start();
    let issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([
            {
                "id": 10,
                "number": 7,
                "title": "Bridge me",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"},
                "labels": [{"name":"tau-ready"}]
            },
            {
                "id": 11,
                "number": 8,
                "title": "Skip me",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:06Z",
                "user": {"login":"alice"},
                "labels": [{"name":"other"}]
            }
        ]));
    });
    let comments_7 = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/7/comments");
        then.status(200).json_body(json!([{
            "id": 200,
            "body": "hello from issue stream",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let comments_8 = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/8/comments");
        then.status(200).json_body(json!([]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/901")
            .body_includes("bridge reply")
            .body_includes("tau-event-key:issue-comment-created:200");
        then.status(200).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.required_labels = vec!["tau-ready".to_string()];
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    let second = runtime.poll_once().await.expect("second poll");

    assert_eq!(first.discovered_events, 1);
    assert_eq!(first.processed_events, 1);
    assert_eq!(second.processed_events, 0);
    issues.assert_calls(2);
    comments_7.assert_calls(2);
    comments_8.assert_calls(0);
    working_post.assert_calls(1);
    update.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_poll_filters_issues_by_required_number() {
    let server = MockServer::start();
    let issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([
            {
                "id": 10,
                "number": 7,
                "title": "Bridge me",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:05Z",
                "user": {"login":"alice"}
            },
            {
                "id": 11,
                "number": 8,
                "title": "Skip me",
                "body": "",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:06Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let comments_7 = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/7/comments");
        then.status(200).json_body(json!([{
            "id": 200,
            "body": "hello from issue stream",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let comments_8 = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/8/comments");
        then.status(200).json_body(json!([]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/901")
            .body_includes("bridge reply")
            .body_includes("tau-event-key:issue-comment-created:200");
        then.status(200).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.required_issue_numbers = vec![7];
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    let second = runtime.poll_once().await.expect("second poll");

    assert_eq!(first.discovered_events, 1);
    assert_eq!(first.processed_events, 1);
    assert_eq!(second.processed_events, 0);
    issues.assert_calls(2);
    comments_7.assert_calls(2);
    comments_8.assert_calls(0);
    working_post.assert_calls(1);
    update.assert_calls(1);
}

#[tokio::test]
async fn functional_bridge_run_poll_once_completes_single_cycle_and_exits() {
    let server = MockServer::start();
    let issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 10,
            "number": 7,
            "title": "Bridge me",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let comments = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/7/comments");
        then.status(200).json_body(json!([{
            "id": 200,
            "body": "hello from issue stream",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/901")
            .body_includes("bridge reply")
            .body_includes("tau-event-key:issue-comment-created:200");
        then.status(200).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.poll_once = true;
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    runtime.run().await.expect("poll-once run");

    issues.assert_calls(1);
    comments.assert_calls(1);
    working_post.assert_calls(1);
    update.assert_calls(1);

    let state_path = temp.path().join("owner__repo").join("state.json");
    let state_raw = std::fs::read_to_string(&state_path).expect("state file");
    let state: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
    let issue_session = state
        .get("issue_sessions")
        .and_then(serde_json::Value::as_object)
        .and_then(|sessions| sessions.get("7"))
        .expect("issue session");
    assert!(issue_session
        .get("last_run_id")
        .and_then(serde_json::Value::as_str)
        .is_some());
}

#[tokio::test]
async fn regression_bridge_run_poll_once_propagates_poll_errors() {
    let server = MockServer::start();
    let issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(500).body("boom");
    });

    let temp = tempdir().expect("tempdir");
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.poll_once = true;
    config.retry_max_attempts = 1;
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let error = runtime.run().await.expect_err("poll-once should fail");
    assert!(error
        .to_string()
        .contains("github api list issues failed with status 500"));
    issues.assert_calls(1);

    let state_path = temp.path().join("owner__repo").join("state.json");
    let state_raw = std::fs::read_to_string(&state_path).expect("state file");
    let state: serde_json::Value = serde_json::from_str(&state_raw).expect("state json");
    let health = state
        .get("health")
        .and_then(serde_json::Value::as_object)
        .expect("health object");
    assert_eq!(
        health
            .get("failure_streak")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[tokio::test]
async fn integration_run_prompt_for_event_downloads_issue_attachments_and_records_provenance() {
    let server = MockServer::start();
    let attachment_url = format!("{}/assets/trace.log", server.base_url());
    let attachment_download = server.mock(|when, then| {
        when.method(GET).path("/assets/trace.log");
        then.status(200)
            .header("content-type", "text/plain")
            .body("trace-line-1\ntrace-line-2\n");
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let repo = RepoRef::parse("owner/repo").expect("repo");
    let github_client = GithubApiClient::new(
        server.base_url(),
        "token".to_string(),
        repo.clone(),
        2_000,
        1,
        1,
    )
    .expect("github client");
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let event = GithubBridgeEvent {
        key: "issue-comment-created:1200".to_string(),
        kind: GithubBridgeEventKind::CommentCreated,
        issue_number: 20,
        issue_title: "Attachment".to_string(),
        author_login: "alice".to_string(),
        occurred_at: "2026-01-01T00:00:01Z".to_string(),
        body: attachment_url.clone(),
        raw_payload: json!({"id": 1200}),
    };
    let report = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: temp.path(),
        event: &event,
        prompt: &attachment_url,
        run_id: "run-attachment",
        cancel_rx,
    })
    .await
    .expect("run prompt");
    assert_eq!(report.downloaded_attachments.len(), 1);
    assert_eq!(report.downloaded_attachments[0].source_url, attachment_url);
    attachment_download.assert_calls(1);

    let channel_store =
        ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-20")
            .expect("channel store");
    let attachment_dir = channel_store
        .attachments_dir()
        .join(shared_sanitize_for_path("issue-comment-created:1200"));
    assert!(attachment_dir.exists());
    let attachment_entries = std::fs::read_dir(&attachment_dir)
        .expect("read attachment dir")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect attachments");
    assert_eq!(attachment_entries.len(), 1);
    let attachment_payload =
        std::fs::read_to_string(attachment_entries[0].path()).expect("attachment payload");
    assert!(attachment_payload.contains("trace-line-1"));

    let channel_log = std::fs::read_to_string(channel_store.log_path()).expect("channel log");
    assert!(channel_log.contains("\"downloaded_attachments\""));
    assert!(channel_log.contains("\"policy_reason_code\""));

    let attachment_manifest = channel_store
        .load_attachment_records_tolerant()
        .expect("attachment manifest");
    assert_eq!(attachment_manifest.records.len(), 1);
    assert_eq!(attachment_manifest.records[0].event_key, event.key);
    assert_eq!(attachment_manifest.records[0].actor, "alice");
    assert_eq!(attachment_manifest.records[0].source_url, attachment_url);
    assert_eq!(attachment_manifest.records[0].policy_decision, "accepted");
    assert_eq!(
        attachment_manifest.records[0].policy_reason_code,
        "allow_extension_allowlist"
    );
    assert!(attachment_manifest.records[0].expires_unix_ms.is_some());

    let artifacts = channel_store
        .load_artifact_records_tolerant()
        .expect("artifact records");
    assert_eq!(artifacts.records.len(), 1);
    let artifact_payload = std::fs::read_to_string(
        channel_store
            .channel_dir()
            .join(&artifacts.records[0].relative_path),
    )
    .expect("artifact payload");
    assert!(artifact_payload.contains("attachments: 1"));
    assert!(artifact_payload.contains("source_url=http://"));
}

#[tokio::test]
async fn functional_run_prompt_for_event_attachment_policy_rejects_denylisted_extensions() {
    let server = MockServer::start();
    let accepted_url = format!("{}/assets/trace.log", server.base_url());
    let denied_url = format!("{}/assets/run.exe", server.base_url());
    let accepted_download = server.mock(|when, then| {
        when.method(GET).path("/assets/trace.log");
        then.status(200)
            .header("content-type", "text/plain")
            .body("trace-line-1\ntrace-line-2\n");
    });
    let denied_download = server.mock(|when, then| {
        when.method(GET).path("/assets/run.exe");
        then.status(200)
            .header("content-type", "application/octet-stream")
            .body("binary");
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let repo = RepoRef::parse("owner/repo").expect("repo");
    let github_client = GithubApiClient::new(
        server.base_url(),
        "token".to_string(),
        repo.clone(),
        2_000,
        1,
        1,
    )
    .expect("github client");
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let event = GithubBridgeEvent {
        key: "issue-comment-created:1201".to_string(),
        kind: GithubBridgeEventKind::CommentCreated,
        issue_number: 21,
        issue_title: "Attachment policy".to_string(),
        author_login: "alice".to_string(),
        occurred_at: "2026-01-01T00:00:01Z".to_string(),
        body: format!("{accepted_url}\n{denied_url}"),
        raw_payload: json!({"id": 1201}),
    };
    let report = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: temp.path(),
        event: &event,
        prompt: &event.body,
        run_id: "run-attachment-policy",
        cancel_rx,
    })
    .await
    .expect("run prompt");
    assert_eq!(report.downloaded_attachments.len(), 1);
    assert_eq!(report.downloaded_attachments[0].source_url, accepted_url);
    accepted_download.assert_calls(1);
    denied_download.assert_calls(0);

    let channel_store =
        ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-21")
            .expect("channel store");
    let attachment_manifest = channel_store
        .load_attachment_records_tolerant()
        .expect("attachment manifest");
    assert_eq!(attachment_manifest.records.len(), 1);
    assert_eq!(attachment_manifest.records[0].source_url, accepted_url);
}

#[tokio::test]
async fn integration_bridge_poll_denies_unpaired_actor_in_strict_mode() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 25,
            "number": 77,
            "title": "Strict policy",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/77/comments");
        then.status(200).json_body(json!([{
            "id": 7701,
            "body": "run anyway",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/77/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 7777,
            "html_url": "https://example.test/comment/7777"
        }));
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

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.discovered_events, 1);
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    working_post.assert_calls(0);

    let outbound = std::fs::read_to_string(
        temp.path()
            .join("owner__repo")
            .join("outbound-events.jsonl"),
    )
    .expect("read outbound log");
    assert!(outbound.contains("\"status\":\"denied\""));
    assert!(outbound.contains("\"reason_code\":\"deny_actor_not_paired_or_allowlisted\""));
}

#[tokio::test]
async fn integration_bridge_poll_denies_unbound_actor_in_rbac_team_mode() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 31,
            "number": 88,
            "title": "RBAC policy",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/88/comments");
        then.status(200).json_body(json!([{
            "id": 8801,
            "body": "/tau status",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/88/comments")
            .body_includes("Current status for issue");
        then.status(201).json_body(json!({
            "id": 8888,
            "html_url": "https://example.test/comment/8888"
        }));
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
"github:owner/repo": ["alice"]
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

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.discovered_events, 1);
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    status_post.assert_calls(0);

    let outbound = std::fs::read_to_string(
        temp.path()
            .join("owner__repo")
            .join("outbound-events.jsonl"),
    )
    .expect("read outbound log");
    assert!(outbound.contains("\"command\":\"rbac-authorization\""));
    assert!(outbound.contains("\"status\":\"denied\""));
    assert!(outbound.contains("\"reason_code\":\"deny_unbound_principal\""));
}

#[tokio::test]
async fn regression_bridge_poll_replay_does_not_duplicate_responses() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 11,
            "number": 8,
            "title": "Replay",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/8/comments");
        then.status(200).json_body(json!([{
            "id": 201,
            "body": "same comment every poll",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/8/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 902,
            "html_url": "https://example.test/comment/902"
        }));
    });
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/902")
            .body_includes("tau-event-key:issue-comment-created:201");
        then.status(200).json_body(json!({
            "id": 902,
            "html_url": "https://example.test/comment/902"
        }));
    });
    let fallback_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/8/comments")
            .body_includes("warning: failed to update placeholder comment");
        then.status(201).json_body(json!({
            "id": 903,
            "html_url": "https://example.test/comment/903"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    assert_eq!(first.processed_events, 1);
    let second = runtime.poll_once().await.expect("second poll");
    assert_eq!(second.processed_events, 0);
    assert_eq!(second.skipped_duplicate_events, 1);
    working_post.assert_calls(1);
    update.assert_calls(1);
    fallback_post.assert_calls(0);
}

#[tokio::test]
async fn regression_bridge_poll_hydrates_command_replay_markers_from_existing_bot_comments() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 40,
            "number": 19,
            "title": "Replay Marker",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/19/comments");
        then.status(200).json_body(json!([
            {
                "id": 1901,
                "body": "/tau status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            },
            {
                "id": 1902,
                "body": "Tau status for issue #19: idle\n\n---\n<!-- tau-event-key:issue-comment-created:1901 -->\n_Tau command `status` | status `reported`_",
                "created_at": "2026-01-01T00:00:02Z",
                "updated_at": "2026-01-01T00:00:02Z",
                "user": {"login":"tau"}
            }
        ]));
    });
    let status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/19/comments")
            .body_includes("Tau status for issue #19: idle");
        then.status(201).json_body(json!({
            "id": 1903,
            "html_url": "https://example.test/comment/1903"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.discovered_events, 1);
    assert_eq!(report.processed_events, 0);
    assert_eq!(report.skipped_duplicate_events, 1);
    assert_eq!(report.failed_events, 0);
    status_post.assert_calls(0);
}
