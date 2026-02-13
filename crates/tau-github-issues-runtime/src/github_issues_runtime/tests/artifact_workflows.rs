use super::*;

/// Confirms artifact listing and retrieval behavior across run and TTL states.
#[tokio::test]
async fn functional_render_issue_artifacts_filters_by_run_id() {
    let server = MockServer::start();
    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-15",
    )
    .expect("seeded store");
    seeded_store
        .write_text_artifact(
            "run-target",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "target artifact",
        )
        .expect("write target artifact");
    seeded_store
        .write_text_artifact(
            "run-other",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "other artifact",
        )
        .expect("write other artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime
        .render_issue_artifacts(15, Some("run-target"))
        .expect("render artifacts");
    assert!(report.contains("Tau artifacts for issue #15 run_id `run-target`: active=1"));
    assert!(report.contains("artifacts/run-target/"));
    assert!(!report.contains("artifacts/run-other/"));
}

#[tokio::test]
async fn functional_render_issue_artifact_show_reports_active_and_expired_states() {
    let server = MockServer::start();
    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-17",
    )
    .expect("seeded store");
    let active = seeded_store
        .write_text_artifact(
            "run-active",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "active artifact",
        )
        .expect("write active artifact");
    let expired = seeded_store
        .write_text_artifact(
            "run-expired",
            "github-issue-reply",
            "private",
            Some(0),
            "md",
            "expired artifact",
        )
        .expect("write expired artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");

    let active_report = runtime
        .render_issue_artifact_show(17, &active.id)
        .expect("render active artifact");
    assert!(active_report.contains(&format!(
        "Tau artifact for issue #17 id `{}`: state=active",
        active.id
    )));
    assert!(active_report.contains("run_id: run-active"));

    let expired_report = runtime
        .render_issue_artifact_show(17, &expired.id)
        .expect("render expired artifact");
    assert!(expired_report.contains(&format!(
        "Tau artifact for issue #17 id `{}`: state=expired",
        expired.id
    )));
    assert!(expired_report
        .contains("artifact is expired and may be removed by `/tau artifacts purge`."));
}

#[tokio::test]
async fn integration_bridge_artifacts_command_reports_recent_artifacts() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 14,
            "number": 11,
            "title": "Artifacts",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/11/comments");
        then.status(200).json_body(json!([
            {
                "id": 501,
                "body": "/tau artifacts",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let artifacts_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/11/comments")
            .body_includes("Tau artifacts for issue #11: active=1")
            .body_includes("github-issue-reply")
            .body_includes("artifacts/run-seeded/");
        then.status(201).json_body(json!({
            "id": 951,
            "html_url": "https://example.test/comment/951"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-11",
    )
    .expect("seeded store");
    seeded_store
        .write_text_artifact(
            "run-seeded",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "seeded artifact",
        )
        .expect("write seeded artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    artifacts_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_artifacts_run_filter_command_reports_matching_entries() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 18,
            "number": 15,
            "title": "Artifacts run filter",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/15/comments");
        then.status(200).json_body(json!([
            {
                "id": 851,
                "body": "/tau artifacts run run-target",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let artifacts_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/15/comments")
            .body_includes("Tau artifacts for issue #15 run_id `run-target`: active=1")
            .body_includes("artifacts/run-target/");
        then.status(201).json_body(json!({
            "id": 955,
            "html_url": "https://example.test/comment/955"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-15",
    )
    .expect("seeded store");
    seeded_store
        .write_text_artifact(
            "run-target",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "target artifact",
        )
        .expect("write target artifact");
    seeded_store
        .write_text_artifact(
            "run-other",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "other artifact",
        )
        .expect("write other artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    artifacts_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_artifacts_show_command_reports_artifact_details() {
    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-18",
    )
    .expect("seeded store");
    let artifact = seeded_store
        .write_text_artifact(
            "run-detail",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "detail artifact",
        )
        .expect("write detail artifact");

    let server = MockServer::start();
    let command_body = format!("/tau artifacts show {}", artifact.id);
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 20,
            "number": 18,
            "title": "Artifacts show",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(move |when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/18/comments");
        then.status(200).json_body(json!([
            {
                "id": 871,
                "body": command_body,
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let expected_header = format!(
        "Tau artifact for issue #18 id `{}`: state=active",
        artifact.id
    );
    let expected_path = format!("path: {}", artifact.relative_path);
    let artifacts_post = server.mock(move |when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/18/comments")
            .body_includes(&expected_header)
            .body_includes("run_id: run-detail")
            .body_includes(&expected_path);
        then.status(201).json_body(json!({
            "id": 957,
            "html_url": "https://example.test/comment/957"
        }));
    });

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    artifacts_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_artifacts_purge_command_removes_expired_entries() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 16,
            "number": 13,
            "title": "Artifact purge",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/13/comments");
        then.status(200).json_body(json!([
            {
                "id": 701,
                "body": "/tau artifacts purge",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let purge_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/13/comments")
            .body_includes("Tau artifact purge for issue #13")
            .body_includes("expired_removed=1")
            .body_includes("active_remaining=1");
        then.status(201).json_body(json!({
            "id": 953,
            "html_url": "https://example.test/comment/953"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-13",
    )
    .expect("seeded store");
    let expired = seeded_store
        .write_text_artifact(
            "run-expired",
            "github-issue-reply",
            "private",
            Some(0),
            "md",
            "expired artifact",
        )
        .expect("write expired artifact");
    seeded_store
        .write_text_artifact(
            "run-active",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "active artifact",
        )
        .expect("write active artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    purge_post.assert_calls(1);
    assert!(!seeded_store
        .channel_dir()
        .join(expired.relative_path)
        .exists());
}

#[tokio::test]
async fn regression_bridge_artifacts_purge_command_noop_when_nothing_expired() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 17,
            "number": 14,
            "title": "Artifact purge no-op",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/14/comments");
        then.status(200).json_body(json!([
            {
                "id": 801,
                "body": "/tau artifacts purge",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let purge_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/14/comments")
            .body_includes("Tau artifact purge for issue #14")
            .body_includes("expired_removed=0")
            .body_includes("active_remaining=0");
        then.status(201).json_body(json!({
            "id": 954,
            "html_url": "https://example.test/comment/954"
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
    purge_post.assert_calls(1);
}

#[tokio::test]
async fn regression_bridge_artifacts_command_handles_malformed_index_and_empty_state() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 15,
            "number": 12,
            "title": "Artifact regression",
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
                "id": 601,
                "body": "/tau artifacts",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let artifacts_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/12/comments")
            .body_includes("Tau artifacts for issue #12: active=0")
            .body_includes("none")
            .body_includes("index_invalid_lines: 1 (ignored)");
        then.status(201).json_body(json!({
            "id": 952,
            "html_url": "https://example.test/comment/952"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-12",
    )
    .expect("seeded store");
    std::fs::write(seeded_store.artifact_index_path(), "not-json\n").expect("seed invalid");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    artifacts_post.assert_calls(1);
}

#[tokio::test]
async fn regression_bridge_artifacts_run_filter_reports_none_for_unknown_run() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 19,
            "number": 16,
            "title": "Artifact run regression",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/16/comments");
        then.status(200).json_body(json!([
            {
                "id": 861,
                "body": "/tau artifacts run run-missing",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let artifacts_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/16/comments")
            .body_includes("Tau artifacts for issue #16 run_id `run-missing`: active=0")
            .body_includes("none for run_id `run-missing`");
        then.status(201).json_body(json!({
            "id": 956,
            "html_url": "https://example.test/comment/956"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-16",
    )
    .expect("seeded store");
    seeded_store
        .write_text_artifact(
            "run-other",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "other artifact",
        )
        .expect("write other artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    artifacts_post.assert_calls(1);
}

#[tokio::test]
async fn regression_bridge_artifacts_show_command_reports_not_found_for_unknown_id() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 21,
            "number": 19,
            "title": "Artifact show missing",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/19/comments");
        then.status(200).json_body(json!([
            {
                "id": 881,
                "body": "/tau artifacts show artifact-missing",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let artifacts_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/19/comments")
            .body_includes("Tau artifact for issue #19 id `artifact-missing`: not found");
        then.status(201).json_body(json!({
            "id": 958,
            "html_url": "https://example.test/comment/958"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let seeded_store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-19",
    )
    .expect("seeded store");
    seeded_store
        .write_text_artifact(
            "run-known",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "known artifact",
        )
        .expect("write known artifact");

    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    artifacts_post.assert_calls(1);
}
