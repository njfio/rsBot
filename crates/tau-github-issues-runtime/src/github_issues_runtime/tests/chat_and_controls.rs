use super::*;

#[tokio::test]
async fn integration_bridge_commands_status_stop_and_health_produce_control_comments() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 12,
            "number": 9,
            "title": "Control",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/9/comments");
        then.status(200).json_body(json!([
            {
                "id": 301,
                "body": "/tau status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            },
            {
                "id": 302,
                "body": "/tau stop",
                "created_at": "2026-01-01T00:00:02Z",
                "updated_at": "2026-01-01T00:00:02Z",
                "user": {"login":"alice"}
            },
            {
                "id": 303,
                "body": "/tau health",
                "created_at": "2026-01-01T00:00:03Z",
                "updated_at": "2026-01-01T00:00:03Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/9/comments")
            .body_includes("Tau status for issue #9: idle")
            .body_includes("transport_failure_streak: 0")
            .body_includes("reason_code `issue_command_status_reported`");
        then.status(201).json_body(json!({
            "id": 930,
            "html_url": "https://example.test/comment/930"
        }));
    });
    let stop_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/9/comments")
            .body_includes("No active run for this issue. Current state is idle.")
            .body_includes("reason_code `issue_command_stop_acknowledged`");
        then.status(201).json_body(json!({
            "id": 931,
            "html_url": "https://example.test/comment/931"
        }));
    });
    let health_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/9/comments")
            .body_includes("Tau health for issue #9: healthy")
            .body_includes("transport_health_reason:")
            .body_includes("transport_health_recommendation:")
            .body_includes("reason_code `issue_command_health_reported`");
        then.status(201).json_body(json!({
            "id": 932,
            "html_url": "https://example.test/comment/932"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 3);
    assert_eq!(report.failed_events, 0);
    status_post.assert_calls(1);
    stop_post.assert_calls(1);
    health_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_command_logs_include_normalized_reason_codes() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 120,
            "number": 12,
            "title": "Reason code logs",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/12/comments");
        then.status(200).json_body(json!([
            {
                "id": 1201,
                "body": "/tau status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let _status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/12/comments")
            .body_includes("Tau status for issue #12: idle");
        then.status(201).json_body(json!({
            "id": 1202,
            "html_url": "https://example.test/comment/1202"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);

    let outbound = std::fs::read_to_string(
        temp.path()
            .join("owner__repo")
            .join("outbound-events.jsonl"),
    )
    .expect("read outbound log");
    assert!(outbound.contains("\"command\":\"status\""));
    assert!(outbound.contains("\"status\":\"reported\""));
    assert!(outbound.contains("\"reason_code\":\"issue_command_status_reported\""));
}

#[tokio::test]
async fn integration_bridge_chat_commands_manage_sessions() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 12,
            "number": 9,
            "title": "Chat Control",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues/9/comments");
        then.status(200).json_body(json!([
            {
                "id": 311,
                "body": "/tau chat start",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            },
            {
                "id": 312,
                "body": "/tau chat resume",
                "created_at": "2026-01-01T00:00:02Z",
                "updated_at": "2026-01-01T00:00:02Z",
                "user": {"login":"alice"}
            },
            {
                "id": 313,
                "body": "/tau chat reset",
                "created_at": "2026-01-01T00:00:03Z",
                "updated_at": "2026-01-01T00:00:03Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let chat_start_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/9/comments")
            .body_includes("Chat session started for issue #9.");
        then.status(201).json_body(json!({
            "id": 940,
            "html_url": "https://example.test/comment/940"
        }));
    });
    let chat_resume_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/9/comments")
            .body_includes("Chat session resumed for issue #9.");
        then.status(201).json_body(json!({
            "id": 941,
            "html_url": "https://example.test/comment/941"
        }));
    });
    let chat_reset_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/9/comments")
            .body_includes("Chat session reset for issue #9.");
        then.status(201).json_body(json!({
            "id": 942,
            "html_url": "https://example.test/comment/942"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 3);
    assert_eq!(report.failed_events, 0);
    chat_start_post.assert_calls(1);
    chat_resume_post.assert_calls(1);
    chat_reset_post.assert_calls(1);
    assert!(runtime.state_store.issue_session(9).is_none());
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 9);
    assert!(!session_path.exists());
}

#[tokio::test]
async fn integration_bridge_chat_export_posts_artifact() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 18,
            "number": 11,
            "title": "Export Chat",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/11/comments");
        then.status(200).json_body(json!([{
            "id": 411,
            "body": "/tau chat export",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let export_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/11/comments")
            .body_includes("Chat session export ready for issue #11.")
            .body_includes("artifact_path=artifacts/chat-export-11/");
        then.status(201).json_body(json!({
            "id": 960,
            "html_url": "https://example.test/comment/960"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 11);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("store");
    store
        .append_messages(
            None,
            &[
                Message::user("Export this"),
                Message::assistant_text("Ready"),
            ],
        )
        .expect("append messages");

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    export_post.assert_calls(1);

    let channel_store = ChannelStore::open(
        &runtime.repository_state_dir.join("channel-store"),
        "github",
        "issue-11",
    )
    .expect("channel store");
    let loaded = channel_store
        .load_artifact_records_tolerant()
        .expect("load artifacts");
    assert_eq!(loaded.records.len(), 1);
    let record = &loaded.records[0];
    assert_eq!(record.artifact_type, "github-issue-chat-export");
    assert!(record.relative_path.contains("artifacts/chat-export-11/"));
    let artifact_path = channel_store.channel_dir().join(&record.relative_path);
    let payload = std::fs::read_to_string(&artifact_path).expect("read artifact");
    assert!(payload.contains("\"schema_version\""));
    assert!(payload.contains("\"message\""));
}

#[tokio::test]
async fn integration_bridge_chat_status_reports_session_state() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 20,
            "number": 12,
            "title": "Chat Status",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/12/comments");
        then.status(200).json_body(json!([{
            "id": 511,
            "body": "/tau chat status",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/12/comments")
            .body_includes("Chat session status for issue #12.")
            .body_includes("entries=2")
            .body_includes("lineage_digest_sha256=")
            .body_includes("artifact_active=0")
            .body_includes("artifact_total=0")
            .body_includes("session_id=issue-12")
            .body_includes("last_comment_id=900")
            .body_includes("last_run_id=run-12");
        then.status(201).json_body(json!({
            "id": 970,
            "html_url": "https://example.test/comment/970"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 12);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("store");
    store
        .append_messages(
            None,
            &[
                Message::user("Status this"),
                Message::assistant_text("Status ready"),
            ],
        )
        .expect("append messages");
    runtime.state_store.update_issue_session(
        12,
        issue_shared_session_id(12),
        Some(900),
        Some("run-12".to_string()),
    );

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    status_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_status_reports_missing_session() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 21,
            "number": 13,
            "title": "Chat Status None",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/13/comments");
        then.status(200).json_body(json!([{
            "id": 611,
            "body": "/tau chat status",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let status_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/13/comments")
            .body_includes("No chat session found for issue #13.")
            .body_includes("entries=0")
            .body_includes("session_id=none")
            .body_includes("lineage_digest_sha256=")
            .body_includes("artifact_active=0")
            .body_includes("artifact_total=0");
        then.status(201).json_body(json!({
            "id": 971,
            "html_url": "https://example.test/comment/971"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    status_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_summary_reports_session_digest() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 30,
            "number": 18,
            "title": "Chat Summary",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/18/comments");
        then.status(200).json_body(json!([{
            "id": 1211,
            "body": "/tau chat summary",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let summary_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/18/comments")
            .body_includes("Chat summary for issue #18.")
            .body_includes("entries=2")
            .body_includes("lineage_digest_sha256=")
            .body_includes("total_processed_events=2")
            .body_includes("total_denied_events=1");
        then.status(201).json_body(json!({
            "id": 980,
            "html_url": "https://example.test/comment/980"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 18);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("store");
    store
        .append_messages(
            None,
            &[
                Message::user("summary request"),
                Message::assistant_text("summary response"),
            ],
        )
        .expect("append messages");
    runtime.state_store.update_issue_session(
        18,
        issue_shared_session_id(18),
        Some(1500),
        Some("run-18".to_string()),
    );
    runtime.state_store.record_issue_event_outcome(
        18,
        "issue-comment-created:seed-1",
        "issue_comment_created",
        "alice",
        IssueEventOutcome::Processed,
        Some("command_processed"),
    );
    runtime.state_store.record_issue_event_outcome(
        18,
        "issue-comment-created:seed-2",
        "issue_comment_created",
        "alice",
        IssueEventOutcome::Denied,
        Some("pairing_denied"),
    );

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    summary_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_replay_reports_diagnostics_hints() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 31,
            "number": 19,
            "title": "Chat Replay",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/19/comments");
        then.status(200).json_body(json!([{
            "id": 1311,
            "body": "/tau chat replay",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let replay_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/19/comments")
            .body_includes("Chat replay hints for issue #19.")
            .body_includes(
                "recent_event_keys=issue-comment-created:seed-a,issue-comment-created:seed-b",
            )
            .body_includes("last_reason_code=duplicate_event")
            .body_includes("Replay guidance: use `/tau chat status`");
        then.status(201).json_body(json!({
            "id": 981,
            "html_url": "https://example.test/comment/981"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    runtime
        .state_store
        .mark_processed("issue-comment-created:seed-a");
    runtime
        .state_store
        .mark_processed("issue-comment-created:seed-b");
    runtime.state_store.update_issue_session(
        19,
        issue_shared_session_id(19),
        Some(1600),
        Some("run-19".to_string()),
    );
    runtime.state_store.record_issue_duplicate_event(
        19,
        "issue-comment-created:seed-b",
        "issue_comment_created",
        "alice",
    );

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    replay_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_show_reports_recent_messages() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 22,
            "number": 14,
            "title": "Chat Show",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/14/comments");
        then.status(200).json_body(json!([{
            "id": 711,
            "body": "/tau chat show 2",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let show_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/14/comments")
            .body_includes("Chat session show for issue #14.")
            .body_includes("showing_last=2")
            .body_includes("role=assistant")
            .body_includes("role=user");
        then.status(201).json_body(json!({
            "id": 972,
            "html_url": "https://example.test/comment/972"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 14);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("store");
    store
        .append_messages(
            None,
            &[
                Message::user("First message"),
                Message::assistant_text("Second message"),
                Message::user("Third message"),
            ],
        )
        .expect("append messages");

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    show_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_show_reports_missing_session() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 23,
            "number": 15,
            "title": "Chat Show None",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/15/comments");
        then.status(200).json_body(json!([{
            "id": 811,
            "body": "/tau chat show",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let show_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/15/comments")
            .body_includes("No chat session found for issue #15.")
            .body_includes("entries=0");
        then.status(201).json_body(json!({
            "id": 973,
            "html_url": "https://example.test/comment/973"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    show_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_search_reports_matches() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 24,
            "number": 16,
            "title": "Chat Search",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/16/comments");
        then.status(200).json_body(json!([{
            "id": 911,
            "body": "/tau chat search alpha --limit 5",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let search_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/16/comments")
            .body_includes("Chat session search for issue #16.")
            .body_includes("query=alpha")
            .body_includes("matches=");
        then.status(201).json_body(json!({
            "id": 974,
            "html_url": "https://example.test/comment/974"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, 16);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("store");
    store
        .append_messages(
            None,
            &[
                Message::user("alpha message"),
                Message::assistant_text("beta response"),
            ],
        )
        .expect("append messages");

    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    search_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_chat_search_reports_missing_session() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 25,
            "number": 17,
            "title": "Chat Search None",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/17/comments");
        then.status(200).json_body(json!([{
            "id": 1011,
            "body": "/tau chat search alpha",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let search_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/17/comments")
            .body_includes("No chat session found for issue #17.")
            .body_includes("entries=0");
        then.status(201).json_body(json!({
            "id": 975,
            "html_url": "https://example.test/comment/975"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    search_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_help_command_posts_usage() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 14,
            "number": 10,
            "title": "Help",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/10/comments");
        then.status(200).json_body(json!([
            {
                "id": 321,
                "body": "/tau help",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let help_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/10/comments")
            .body_includes("Supported `/tau` commands:");
        then.status(201).json_body(json!({
            "id": 950,
            "html_url": "https://example.test/comment/950"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    help_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_canvas_command_persists_replay_safe_event() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 26,
            "number": 18,
            "title": "Canvas",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/18/comments");
        then.status(200).json_body(json!([{
            "id": 1112,
            "body": "/tau canvas create architecture",
            "created_at": "2026-01-01T00:00:01Z",
            "updated_at": "2026-01-01T00:00:01Z",
            "user": {"login":"alice"}
        }]));
    });
    let canvas_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/18/comments")
            .body_includes("canvas create: id=architecture")
            .body_includes("event_id=");
        then.status(201).json_body(json!({
            "id": 990,
            "html_url": "https://example.test/comment/990"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    canvas_post.assert_calls(1);

    let events_path = temp
        .path()
        .join("owner__repo/canvas/architecture/events.jsonl");
    let payload = std::fs::read_to_string(events_path).expect("events payload");
    assert!(payload.contains("\"event_id\":"));
    assert!(payload.contains("\"transport\":\"github\""));
    assert!(payload.contains("\"source_event_key\":\"issue-comment-created:1112\""));

    let links_path = temp
        .path()
        .join("owner__repo/canvas/architecture/session-links.jsonl");
    let links = std::fs::read_to_string(links_path).expect("session links");
    assert!(links.contains("\"canvas_id\":\"architecture\""));
}

#[tokio::test]
async fn integration_bridge_stop_cancels_active_run_and_updates_state() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 13,
            "number": 10,
            "title": "Cancelable",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/10/comments");
        then.status(200).json_body(json!([
            {
                "id": 401,
                "body": "/tau run long diagnostic run",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            },
            {
                "id": 402,
                "body": "/tau stop",
                "created_at": "2026-01-01T00:00:02Z",
                "updated_at": "2026-01-01T00:00:02Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let working_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/10/comments")
            .body_includes("Tau is working on run");
        then.status(201).json_body(json!({
            "id": 940,
            "html_url": "https://example.test/comment/940"
        }));
    });
    let stop_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/10/comments")
            .body_includes("Cancellation requested for run");
        then.status(201).json_body(json!({
            "id": 941,
            "html_url": "https://example.test/comment/941"
        }));
    });
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/940")
            .body_includes("status `cancelled`")
            .body_includes("Run cancelled by /tau stop.");
        then.status(200).json_body(json!({
            "id": 940,
            "html_url": "https://example.test/comment/940"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config =
        test_bridge_config_with_client(&server.base_url(), temp.path(), Arc::new(SlowReplyClient));
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    assert_eq!(first.processed_events, 2);
    let second = runtime.poll_once().await.expect("second poll");
    assert_eq!(second.failed_events, 0);
    working_post.assert_calls(1);
    stop_post.assert_calls(1);
    update.assert_calls(1);
}
