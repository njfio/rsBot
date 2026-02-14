//! Command workflow and report wiring coverage for runtime tests.

use super::*;

/// Verifies demo-index command rendering and run/report wiring in bridge flows.
#[tokio::test]
async fn functional_bridge_demo_index_list_command_reports_inventory() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 23,
            "number": 23,
            "title": "Demo index list",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/23/comments");
        then.status(200).json_body(json!([
            {
                "id": 2301,
                "body": "/tau demo-index list",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let list_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/23/comments")
            .body_includes("Tau demo-index scenario inventory for issue #23: 1 scenario(s).")
            .body_includes("`onboarding`")
            .body_includes("Tau demo-index latest report pointers for issue #23: 0");
        then.status(201).json_body(json!({
            "id": 2302,
            "html_url": "https://example.test/comment/2302"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("demo-index-stub.sh");
    write_demo_index_list_stub(&script_path);
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.demo_index_repo_root = Some(temp.path().to_path_buf());
    config.demo_index_script_path = Some(script_path);
    config.demo_index_binary_path = Some(temp.path().join("tau-coding-agent"));
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    list_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_demo_index_run_command_posts_artifact_pointers() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 24,
            "number": 24,
            "title": "Demo index run",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/24/comments");
        then.status(200).json_body(json!([
            {
                "id": 2401,
                "body": "/tau demo-index run onboarding --timeout-seconds 120",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let run_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/24/comments")
            .body_includes("Tau demo-index run for issue #24: status=completed")
            .body_includes("summary: total=1 passed=1 failed=0")
            .body_includes("report_artifact:")
            .body_includes("log_artifact:")
            .body_includes("Use `/tau demo-index report`");
        then.status(201).json_body(json!({
            "id": 2402,
            "html_url": "https://example.test/comment/2402"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("demo-index-stub.sh");
    write_demo_index_run_stub(&script_path);
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.demo_index_repo_root = Some(temp.path().to_path_buf());
    config.demo_index_script_path = Some(script_path);
    config.demo_index_binary_path = Some(temp.path().join("tau-coding-agent"));
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let report = runtime.poll_once().await.expect("poll");
    assert_eq!(report.processed_events, 1);
    assert_eq!(report.failed_events, 0);
    run_post.assert_calls(1);

    let store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-24",
    )
    .expect("channel store");
    let artifacts = store
        .load_artifact_records_tolerant()
        .expect("artifact records");
    let report_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-demo-index-report")
        .count();
    let log_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-demo-index-log")
        .count();
    assert_eq!(report_count, 1);
    assert_eq!(log_count, 1);
}

#[tokio::test]
async fn regression_bridge_demo_index_run_command_replay_guard_prevents_duplicate_execution() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 25,
            "number": 25,
            "title": "Demo index replay",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/25/comments");
        then.status(200).json_body(json!([
            {
                "id": 2501,
                "body": "/tau demo-index run onboarding --timeout-seconds 120",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let run_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/25/comments")
            .body_includes("Tau demo-index run for issue #25: status=completed");
        then.status(201).json_body(json!({
            "id": 2502,
            "html_url": "https://example.test/comment/2502"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let script_path = temp.path().join("demo-index-stub.sh");
    write_demo_index_run_stub(&script_path);
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.demo_index_repo_root = Some(temp.path().to_path_buf());
    config.demo_index_script_path = Some(script_path);
    config.demo_index_binary_path = Some(temp.path().join("tau-coding-agent"));
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    let second = runtime.poll_once().await.expect("second poll");
    assert_eq!(first.processed_events, 1);
    assert_eq!(first.failed_events, 0);
    assert_eq!(second.processed_events, 0);
    run_post.assert_calls(1);
}

#[tokio::test]
async fn functional_bridge_doctor_command_reports_summary_and_artifact_pointers() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 260,
            "number": 26,
            "title": "Doctor summary",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/26/comments");
        then.status(200).json_body(json!([
            {
                "id": 2601,
                "body": "/tau doctor",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let doctor_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/26/comments")
            .body_includes("Tau doctor diagnostics for issue #26: status=")
            .body_includes("summary: checks=")
            .body_includes("report_artifact:")
            .body_includes("json_artifact:");
        then.status(201).json_body(json!({
            "id": 2602,
            "html_url": "https://example.test/comment/2602"
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
    doctor_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_doctor_command_persists_report_artifacts() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 270,
            "number": 27,
            "title": "Doctor artifacts",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/27/comments");
        then.status(200).json_body(json!([
            {
                "id": 2701,
                "body": "/tau doctor",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let doctor_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/27/comments")
            .body_includes("Tau doctor diagnostics for issue #27: status=");
        then.status(201).json_body(json!({
            "id": 2702,
            "html_url": "https://example.test/comment/2702"
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
    doctor_post.assert_calls(1);

    let store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-27",
    )
    .expect("channel store");
    let artifacts = store
        .load_artifact_records_tolerant()
        .expect("artifact records");
    let report_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-doctor-report")
        .count();
    let json_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-doctor-json")
        .count();
    assert_eq!(report_count, 1);
    assert_eq!(json_count, 1);
}

#[tokio::test]
async fn regression_bridge_doctor_command_replay_guard_prevents_duplicate_execution() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 280,
            "number": 28,
            "title": "Doctor replay",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/28/comments");
        then.status(200).json_body(json!([
            {
                "id": 2801,
                "body": "/tau doctor",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let doctor_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/28/comments")
            .body_includes("Tau doctor diagnostics for issue #28: status=");
        then.status(201).json_body(json!({
            "id": 2802,
            "html_url": "https://example.test/comment/2802"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    let second = runtime.poll_once().await.expect("second poll");
    assert_eq!(first.processed_events, 1);
    assert_eq!(first.failed_events, 0);
    assert_eq!(second.processed_events, 0);
    doctor_post.assert_calls(1);
}

#[tokio::test]
async fn functional_bridge_auth_status_command_reports_summary_and_artifact_pointers() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 290,
            "number": 29,
            "title": "Auth status",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/29/comments");
        then.status(200).json_body(json!([
            {
                "id": 2901,
                "body": "/tau auth status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let auth_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/29/comments")
            .body_includes("Tau auth diagnostics for issue #29: command=status")
            .body_includes("subscription_strict:")
            .body_includes("provider_mode:")
            .body_includes("report_artifact:")
            .body_includes("json_artifact:");
        then.status(201).json_body(json!({
            "id": 2902,
            "html_url": "https://example.test/comment/2902"
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
    auth_post.assert_calls(1);
}

#[tokio::test]
async fn integration_bridge_auth_matrix_command_persists_report_artifacts() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 300,
            "number": 30,
            "title": "Auth matrix",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/30/comments");
        then.status(200).json_body(json!([
            {
                "id": 3001,
                "body": "/tau auth matrix --mode-support supported",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let auth_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/30/comments")
            .body_includes("Tau auth diagnostics for issue #30: command=matrix");
        then.status(201).json_body(json!({
            "id": 3002,
            "html_url": "https://example.test/comment/3002"
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
    auth_post.assert_calls(1);

    let store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-30",
    )
    .expect("channel store");
    let artifacts = store
        .load_artifact_records_tolerant()
        .expect("artifact records");
    let report_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-auth-report")
        .count();
    let json_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-auth-json")
        .count();
    assert_eq!(report_count, 1);
    assert_eq!(json_count, 1);
}

#[tokio::test]
async fn regression_bridge_auth_status_command_replay_guard_prevents_duplicate_execution() {
    let server = MockServer::start();
    let _issues = server.mock(|when, then| {
        when.method(GET).path("/repos/owner/repo/issues");
        then.status(200).json_body(json!([{
            "id": 310,
            "number": 31,
            "title": "Auth replay",
            "body": "",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:05Z",
            "user": {"login":"alice"}
        }]));
    });
    let _comments = server.mock(|when, then| {
        when.method(GET)
            .path("/repos/owner/repo/issues/31/comments");
        then.status(200).json_body(json!([
            {
                "id": 3101,
                "body": "/tau auth status",
                "created_at": "2026-01-01T00:00:01Z",
                "updated_at": "2026-01-01T00:00:01Z",
                "user": {"login":"alice"}
            }
        ]));
    });
    let auth_post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/31/comments")
            .body_includes("Tau auth diagnostics for issue #31: command=status");
        then.status(201).json_body(json!({
            "id": 3102,
            "html_url": "https://example.test/comment/3102"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let first = runtime.poll_once().await.expect("first poll");
    let second = runtime.poll_once().await.expect("second poll");
    assert_eq!(first.processed_events, 1);
    assert_eq!(first.failed_events, 0);
    assert_eq!(second.processed_events, 0);
    auth_post.assert_calls(1);
}
