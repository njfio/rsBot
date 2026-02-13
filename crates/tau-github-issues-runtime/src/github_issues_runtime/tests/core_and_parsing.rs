use super::*;

/// Covers baseline parsing and filtering helpers for issue event ingestion.
#[test]
fn unit_normalize_artifact_retention_days_maps_zero_to_none() {
    assert_eq!(normalize_shared_artifact_retention_days(0), None);
    assert_eq!(normalize_shared_artifact_retention_days(30), Some(30));
}

#[test]
fn unit_repo_ref_parse_accepts_owner_repo_shape() {
    let repo = RepoRef::parse("njfio/Tau").expect("parse repo");
    assert_eq!(repo.owner, "njfio");
    assert_eq!(repo.name, "Tau");

    let error = RepoRef::parse("missing").expect_err("invalid repo should fail");
    assert!(error.to_string().contains("expected owner/repo"));
}

#[test]
fn unit_issue_matches_required_labels_filters_case_insensitively() {
    let issue = GithubIssue {
        id: 100,
        number: 42,
        title: "Issue".to_string(),
        body: Some("initial issue body".to_string()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:10Z".to_string(),
        user: GithubUser {
            login: "alice".to_string(),
        },
        labels: vec![GithubIssueLabel {
            name: "Tau-Ready".to_string(),
        }],
        pull_request: None,
    };
    let required = HashSet::from([String::from("tau-ready")]);
    assert!(issue_matches_required_labels(
        issue.labels.iter().map(|label| label.name.as_str()),
        &required
    ));
    let required = HashSet::from([String::from("other")]);
    assert!(!issue_matches_required_labels(
        issue.labels.iter().map(|label| label.name.as_str()),
        &required
    ));
}

#[test]
fn unit_issue_matches_required_numbers_respects_filter_set() {
    let required = HashSet::from([7_u64, 11_u64]);
    assert!(issue_matches_required_numbers(7, &required));
    assert!(!issue_matches_required_numbers(9, &required));
    let required = HashSet::new();
    assert!(issue_matches_required_numbers(42, &required));
}

#[test]
fn functional_collect_issue_events_supports_created_and_edited_comments() {
    let issue = GithubIssue {
        id: 100,
        number: 42,
        title: "Issue".to_string(),
        body: Some("initial issue body".to_string()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:10Z".to_string(),
        user: GithubUser {
            login: "alice".to_string(),
        },
        labels: Vec::new(),
        pull_request: None,
    };
    let comments = vec![
        GithubIssueComment {
            id: 1,
            body: Some("first".to_string()),
            created_at: "2026-01-01T00:00:01Z".to_string(),
            updated_at: "2026-01-01T00:00:01Z".to_string(),
            user: GithubUser {
                login: "bob".to_string(),
            },
        },
        GithubIssueComment {
            id: 2,
            body: Some("second edited".to_string()),
            created_at: "2026-01-01T00:00:02Z".to_string(),
            updated_at: "2026-01-01T00:10:02Z".to_string(),
            user: GithubUser {
                login: "carol".to_string(),
            },
        },
    ];
    let events = collect_shared_issue_events(&issue, &comments, "tau", true, true);
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].kind, GithubBridgeEventKind::Opened);
    assert_eq!(events[1].kind, GithubBridgeEventKind::CommentCreated);
    assert_eq!(events[2].kind, GithubBridgeEventKind::CommentEdited);
}

#[tokio::test]
async fn functional_run_prompt_for_event_sets_expiry_with_default_retention() {
    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config("http://unused.local", temp.path());
    let repo = RepoRef::parse("owner/repo").expect("repo");
    let github_client = GithubApiClient::new(
        "http://unused.local".to_string(),
        "token".to_string(),
        repo.clone(),
        2_000,
        1,
        1,
    )
    .expect("github client");
    let event = test_issue_event();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let report = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: temp.path(),
        event: &event,
        prompt: "hello from test",
        run_id: "run-default-retention",
        cancel_rx,
    })
    .await
    .expect("run prompt");
    assert!(report.artifact.expires_unix_ms.is_some());
}

#[tokio::test]
async fn regression_run_prompt_for_event_zero_retention_disables_expiry() {
    let temp = tempdir().expect("tempdir");
    let mut config = test_bridge_config("http://unused.local", temp.path());
    config.artifact_retention_days = 0;
    let repo = RepoRef::parse("owner/repo").expect("repo");
    let github_client = GithubApiClient::new(
        "http://unused.local".to_string(),
        "token".to_string(),
        repo.clone(),
        2_000,
        1,
        1,
    )
    .expect("github client");
    let event = test_issue_event();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let report = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: temp.path(),
        event: &event,
        prompt: "hello from test",
        run_id: "run-zero-retention",
        cancel_rx,
    })
    .await
    .expect("run prompt");
    assert_eq!(report.artifact.expires_unix_ms, None);

    let store = ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-7")
        .expect("open store");
    let active = store
        .list_active_artifacts(crate::current_unix_timestamp_ms())
        .expect("list active");
    assert_eq!(active.len(), 1);
}

#[tokio::test]
async fn regression_zero_retention_keeps_attachment_manifest_entries_non_expiring() {
    let server = MockServer::start();
    let attachment_url = format!("{}/assets/trace.log", server.base_url());
    let attachment_download = server.mock(|when, then| {
        when.method(GET).path("/assets/trace.log");
        then.status(200)
            .header("content-type", "text/plain")
            .body("trace-line-1\ntrace-line-2\n");
    });
    let temp = tempdir().expect("tempdir");
    let mut config = test_bridge_config(&server.base_url(), temp.path());
    config.artifact_retention_days = 0;
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
        key: "issue-comment-created:1202".to_string(),
        kind: GithubBridgeEventKind::CommentCreated,
        issue_number: 22,
        issue_title: "Attachment retention".to_string(),
        author_login: "alice".to_string(),
        occurred_at: "2026-01-01T00:00:01Z".to_string(),
        body: attachment_url.clone(),
        raw_payload: json!({"id": 1202}),
    };
    let report = run_prompt_for_event(RunPromptForEventRequest {
        config: &config,
        github_client: &github_client,
        repo: &repo,
        repository_state_dir: temp.path(),
        event: &event,
        prompt: &event.body,
        run_id: "run-zero-retention-attachment",
        cancel_rx,
    })
    .await
    .expect("run prompt");
    assert_eq!(report.downloaded_attachments.len(), 1);
    assert_eq!(report.downloaded_attachments[0].expires_unix_ms, None);
    attachment_download.assert_calls(1);

    let store = ChannelStore::open(&temp.path().join("channel-store"), "github", "issue-22")
        .expect("channel store");
    let attachment_manifest = store
        .load_attachment_records_tolerant()
        .expect("attachment manifest");
    assert_eq!(attachment_manifest.records.len(), 1);
    assert_eq!(attachment_manifest.records[0].expires_unix_ms, None);

    let purge = store
        .purge_expired_artifacts(crate::current_unix_timestamp_ms().saturating_add(31 * 86_400_000))
        .expect("purge");
    assert_eq!(purge.attachment_expired_removed, 0);
    assert!(store
        .channel_dir()
        .join(&attachment_manifest.records[0].relative_path)
        .exists());
}

#[test]
fn regression_state_store_caps_processed_event_history() {
    let temp = tempdir().expect("tempdir");
    let state_path = temp.path().join("state.json");
    let mut state = GithubIssuesBridgeStateStore::load(state_path, 2).expect("load store");
    assert!(state.mark_processed("a"));
    assert!(state.mark_processed("b"));
    assert!(state.mark_processed("c"));
    assert!(!state.contains("a"));
    assert!(state.contains("b"));
    assert!(state.contains("c"));
}

#[test]
fn unit_state_store_upserts_issue_session_state() {
    let temp = tempdir().expect("tempdir");
    let state_path = temp.path().join("state.json");
    let mut state = GithubIssuesBridgeStateStore::load(state_path, 8).expect("load store");

    assert!(state.update_issue_session(
        42,
        "issue-42".to_string(),
        Some(101),
        Some("run-1".to_string())
    ));
    let session = state.issue_session(42).expect("session state");
    assert_eq!(session.session_id, "issue-42");
    assert_eq!(session.last_comment_id, Some(101));
    assert_eq!(session.last_run_id.as_deref(), Some("run-1"));

    assert!(!state.update_issue_session(
        42,
        "issue-42".to_string(),
        Some(101),
        Some("run-1".to_string())
    ));
    assert!(state.update_issue_session(42, "issue-42".to_string(), Some(202), None));
    let session = state.issue_session(42).expect("updated session state");
    assert_eq!(session.last_comment_id, Some(202));
    assert_eq!(session.last_run_id.as_deref(), Some("run-1"));

    assert!(state.clear_issue_session(42));
    assert!(state.issue_session(42).is_none());
    assert!(!state.clear_issue_session(42));
}

#[test]
fn regression_state_store_loads_legacy_state_without_issue_sessions() {
    let temp = tempdir().expect("tempdir");
    let state_path = temp.path().join("state.json");
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": ["a", "b"]
}"#,
    )
    .expect("write legacy state");

    let state = GithubIssuesBridgeStateStore::load(state_path, 8).expect("load store");
    assert_eq!(state.last_issue_scan_at(), Some("2026-01-01T00:00:00Z"));
    assert!(state.contains("a"));
    assert!(state.contains("b"));
    assert!(state.issue_session(9).is_none());
    assert_eq!(
        state.transport_health(),
        &crate::TransportHealthSnapshot::default()
    );
}

#[test]
fn regression_state_store_loads_with_corrupt_state_file() {
    let temp = tempdir().expect("tempdir");
    let state_path = temp.path().join("state.json");
    std::fs::write(&state_path, "{not-json").expect("write corrupt state");

    let state = GithubIssuesBridgeStateStore::load(state_path, 8).expect("load store");
    assert!(state.last_issue_scan_at().is_none());
    assert!(!state.contains("a"));
    assert!(state.issue_session(1).is_none());
}

#[test]
fn unit_retry_helpers_identify_retryable_status_and_delays() {
    assert!(is_retryable_github_status(429));
    assert!(is_retryable_github_status(500));
    assert!(!is_retryable_github_status(404));
    let delay = retry_delay(100, 3, None);
    assert_eq!(delay, Duration::from_millis(400));
}

#[test]
fn unit_parse_rfc3339_to_unix_ms_handles_valid_and_invalid_values() {
    assert!(parse_shared_rfc3339_to_unix_ms("2026-01-01T00:00:01Z").is_some());
    assert_eq!(parse_shared_rfc3339_to_unix_ms("invalid"), None);
}

#[test]
fn unit_footer_key_extraction_and_path_helpers_are_stable() {
    let text = "hello\n<!-- tau-event-key:abc -->\nworld\n<!-- rsbot-event-key:def -->";
    let keys = extract_footer_event_keys(text);
    assert_eq!(keys, vec!["abc".to_string(), "def".to_string()]);

    let root = Path::new("/tmp/state");
    let session = shared_session_path_for_issue(root, 9);
    assert!(session.ends_with("sessions/issue-9.jsonl"));
    assert_eq!(shared_sanitize_for_path("owner/repo"), "owner_repo");
}

#[test]
fn unit_normalize_relative_channel_path_requires_descendant_paths() {
    let channel_root = Path::new("/tmp/tau-channel");
    let file_path = channel_root.join("attachments/issue-comment-created_1/01-trace.log");
    let relative = normalize_shared_relative_channel_path(&file_path, channel_root, "attachment")
        .map_err(|error| anyhow::anyhow!(error))
        .expect("path");
    assert_eq!(relative, "attachments/issue-comment-created_1/01-trace.log");

    let outside = Path::new("/tmp/not-channel/trace.log");
    let error = normalize_shared_relative_channel_path(outside, channel_root, "attachment")
        .map_err(|error| anyhow::anyhow!(error))
        .expect_err("outside channel root should fail");
    assert!(error.to_string().contains("failed to derive relative path"));
}

#[test]
fn unit_render_issue_command_comment_appends_marker_footer() {
    let rendered = render_issue_command_comment(
        "issue-comment-created:123",
        "chat-status",
        "reported",
        "issue_command_chat_status_reported",
        "Chat session status for issue #12.",
    );
    assert!(rendered.contains("Chat session status for issue #12."));
    assert!(rendered.contains("tau-event-key:issue-comment-created:123"));
    assert!(rendered.contains(
        "Tau command `chat-status` | status `reported` | reason_code `issue_command_chat_status_reported`"
    ));
}

#[test]
fn unit_issue_command_status_and_reason_code_helpers_are_stable() {
    assert_eq!(normalize_issue_command_status("healthy"), "reported");
    assert_eq!(normalize_issue_command_status("completed"), "reported");
    assert_eq!(
        normalize_issue_command_status("acknowledged"),
        "acknowledged"
    );
    assert_eq!(normalize_issue_command_status("failed"), "failed");
    assert_eq!(
        issue_command_reason_code("auth-status", "reported"),
        "issue_command_auth_status_reported"
    );
    assert_eq!(
        issue_command_reason_code("demo-index-run", "failed"),
        "issue_command_demo_index_run_failed"
    );
}

#[test]
fn functional_render_issue_command_comment_normalizes_footer_schema_across_commands() {
    let stop_status = normalize_issue_command_status("acknowledged");
    let stop_reason = issue_command_reason_code("stop", stop_status);
    let stop = render_issue_command_comment(
        "issue-comment-created:stop",
        "stop",
        stop_status,
        &stop_reason,
        "stop response",
    );
    assert!(stop.contains(
        "Tau command `stop` | status `acknowledged` | reason_code `issue_command_stop_acknowledged`"
    ));

    let health_status = normalize_issue_command_status("healthy");
    let health_reason = issue_command_reason_code("health", health_status);
    let health = render_issue_command_comment(
        "issue-comment-created:health",
        "health",
        health_status,
        &health_reason,
        "health response",
    );
    assert!(health.contains(
        "Tau command `health` | status `reported` | reason_code `issue_command_health_reported`"
    ));
}

#[test]
fn unit_extract_attachment_urls_supports_markdown_and_bare_links() {
    let text = "See [trace](https://example.com/files/trace.log) and https://example.com/images/graph.png plus duplicate https://example.com/files/trace.log";
    let urls = extract_attachment_urls(text);
    assert_eq!(urls.len(), 2);
    assert_eq!(urls[0], "https://example.com/files/trace.log");
    assert_eq!(urls[1], "https://example.com/images/graph.png");
}

#[test]
fn unit_extract_attachment_urls_accepts_localhost_port_with_extension() {
    let url = "http://127.0.0.1:1234/assets/trace.log";
    let urls = extract_attachment_urls(url);
    assert_eq!(urls, vec![url.to_string()]);
    assert!(tau_github_issues::github_issues_helpers::is_supported_attachment_url(url));
}

#[test]
fn unit_attachment_url_policy_enforces_allowlist_and_denylist() {
    let denied = evaluate_attachment_url_policy("https://example.com/files/run.exe");
    assert!(!denied.accepted);
    assert_eq!(denied.reason_code, "deny_extension_denylist");

    let unknown = evaluate_attachment_url_policy("https://example.com/files/run.unknown");
    assert!(!unknown.accepted);
    assert_eq!(unknown.reason_code, "deny_extension_not_allowlisted");

    let allowed = evaluate_attachment_url_policy("https://example.com/files/run.log");
    assert!(allowed.accepted);
    assert_eq!(allowed.reason_code, "allow_extension_allowlist");
}

#[test]
fn unit_attachment_content_type_policy_blocks_dangerous_values() {
    let denied = evaluate_attachment_content_type_policy(Some("application/x-msdownload"));
    assert!(!denied.accepted);
    assert_eq!(denied.reason_code, "deny_content_type_dangerous");

    let allowed = evaluate_attachment_content_type_policy(Some("text/plain"));
    assert!(allowed.accepted);
    assert_eq!(allowed.reason_code, "allow_content_type_default");
}

#[test]
fn functional_render_event_prompt_includes_downloaded_attachments() {
    let repo = RepoRef::parse("owner/repo").expect("repo");
    let event = test_issue_event();
    let attachments = vec![DownloadedGithubAttachment {
        source_url: "https://example.com/files/trace.log".to_string(),
        original_name: "trace.log".to_string(),
        path: PathBuf::from("/tmp/attachments/trace.log"),
        relative_path: "attachments/issue-comment-created_1/01-trace.log".to_string(),
        content_type: Some("text/plain".to_string()),
        bytes: 42,
        checksum_sha256: "abc123".to_string(),
        policy_reason_code: "allow_extension_allowlist".to_string(),
        created_unix_ms: 1,
        expires_unix_ms: Some(1000),
    }];
    let prompt = render_event_prompt(&repo, &event, "inspect this", &attachments);
    assert!(prompt.contains("Downloaded attachments:"));
    assert!(prompt.contains("name=trace.log"));
    assert!(prompt.contains("source_url=https://example.com/files/trace.log"));
    assert!(prompt.contains("policy_reason=allow_extension_allowlist"));
}

#[tokio::test]
async fn unit_issue_chat_continuity_summary_digest_is_deterministic_and_tracks_changes() {
    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config("http://127.0.0.1", temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let issue_number = 77_u64;
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, issue_number);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("session store");
    store
        .append_messages(
            None,
            &[Message::user("alpha"), Message::assistant_text("beta")],
        )
        .expect("append entries");

    let first = runtime
        .issue_chat_continuity_summary(issue_number)
        .expect("first summary");
    let second = runtime
        .issue_chat_continuity_summary(issue_number)
        .expect("second summary");
    assert_eq!(first.lineage_digest_sha256, second.lineage_digest_sha256);
    assert_eq!(first.entries, 2);
    assert_eq!(first.oldest_entry_id, Some(1));
    assert_eq!(first.newest_entry_id, Some(2));
    assert_eq!(first.newest_entry_role.as_deref(), Some("assistant"));
    assert_eq!(first.artifacts.total_records, 0);
    assert_eq!(first.artifacts.active_records, 0);

    let channel_store = ChannelStore::open(
        &runtime.repository_state_dir.join("channel-store"),
        "github",
        &format!("issue-{issue_number}"),
    )
    .expect("channel store");
    channel_store
        .write_text_artifact(
            "run-77",
            "github-issue-chat-export",
            "private",
            Some(30),
            "jsonl",
            "{\"sample\":true}",
        )
        .expect("write artifact");

    let mut store = SessionStore::load(&session_path).expect("reload store");
    let head = store.head_id();
    store
        .append_messages(head, &[Message::user("gamma")])
        .expect("append change");

    let third = runtime
        .issue_chat_continuity_summary(issue_number)
        .expect("third summary");
    assert_ne!(first.lineage_digest_sha256, third.lineage_digest_sha256);
    assert_eq!(third.entries, 3);
    assert_eq!(third.newest_entry_id, Some(3));
    assert_eq!(third.newest_entry_role.as_deref(), Some("user"));
    assert_eq!(third.artifacts.total_records, 1);
    assert_eq!(third.artifacts.active_records, 1);
    assert!(third.artifacts.latest_artifact_id.is_some());
    assert_eq!(
        third.artifacts.latest_artifact_run_id.as_deref(),
        Some("run-77")
    );
}

#[tokio::test]
async fn functional_render_issue_status_includes_chat_digest_and_artifact_fields() {
    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config("http://127.0.0.1", temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let issue_number = 78_u64;
    let session_path = shared_session_path_for_issue(&runtime.repository_state_dir, issue_number);
    if let Some(parent) = session_path.parent() {
        std::fs::create_dir_all(parent).expect("create session dir");
    }
    let mut store = SessionStore::load(&session_path).expect("store");
    store
        .append_messages(None, &[Message::user("status check")])
        .expect("append");
    let channel_store = ChannelStore::open(
        &runtime.repository_state_dir.join("channel-store"),
        "github",
        &format!("issue-{issue_number}"),
    )
    .expect("channel store");
    channel_store
        .write_text_artifact(
            "run-78",
            "github-issue-reply",
            "private",
            Some(30),
            "md",
            "status artifact",
        )
        .expect("artifact");

    let status = runtime.render_issue_status(issue_number);
    assert!(status.contains("chat_lineage_digest_sha256: "));
    assert!(status.contains("chat_entries: 1"));
    assert!(status.contains("artifacts_total: 1"));
    assert!(status.contains("artifacts_active: 1"));
    assert!(status.contains("artifacts_latest_id: artifact-"));
    assert!(status.contains("transport_failure_streak: 0"));
    assert!(status.contains("transport_last_cycle_processed: 0"));
}

#[tokio::test]
async fn functional_render_issue_health_includes_classification_and_transport_fields() {
    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config("http://127.0.0.1", temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let health = runtime.render_issue_health(78);
    assert!(health.contains("Tau health for issue #78: healthy"));
    assert!(health.contains("runtime_state: idle"));
    assert!(health.contains("active_run_id: none"));
    assert!(health.contains("transport_health_reason: "));
    assert!(health.contains("transport_health_recommendation: "));
    assert!(health.contains("transport_failure_streak: 0"));
}

#[tokio::test]
async fn regression_render_issue_health_reports_failing_failure_streak() {
    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config("http://127.0.0.1", temp.path());
    let mut runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let mut health = runtime.state_store.transport_health().clone();
    health.failure_streak = 3;
    runtime.state_store.update_transport_health(health);
    let rendered = runtime.render_issue_health(7);
    assert!(rendered.contains("Tau health for issue #7: failing"));
    assert!(rendered.contains("failure_streak=3"));
}

#[tokio::test]
async fn regression_render_issue_status_defaults_health_lines_for_legacy_state() {
    let temp = tempdir().expect("tempdir");
    let repo_state_dir = temp.path().join("owner__repo");
    std::fs::create_dir_all(&repo_state_dir).expect("repo state dir");
    std::fs::write(
        repo_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": null,
  "processed_event_keys": [],
  "issue_sessions": {}
}
"#,
    )
    .expect("write legacy state");

    let config = test_bridge_config("http://127.0.0.1", temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let status = runtime.render_issue_status(7);
    assert!(status.contains("transport_failure_streak: 0"));
    assert!(status.contains("transport_last_cycle_processed: 0"));
}

#[test]
fn unit_render_issue_comment_chunks_split_and_keep_marker_in_first_chunk() {
    let event = test_issue_event();
    let report = test_prompt_run_report(&"a".repeat(240));
    let (content, footer) = render_issue_comment_response_parts(&event, &report);
    let footer_block = format!("\n\n---\n{footer}");
    let max_chars = footer_block.chars().count() + 10;
    assert!(content.chars().count() > 10);
    let chunks = render_issue_comment_chunks_with_limit(&event, &report, max_chars);
    assert!(chunks.len() > 1);
    assert!(chunks[0].contains(EVENT_KEY_MARKER_PREFIX));
    assert!(chunks
        .iter()
        .skip(1)
        .all(|chunk| !chunk.contains(EVENT_KEY_MARKER_PREFIX)));
    assert!(chunks
        .iter()
        .all(|chunk| chunk.chars().count() <= max_chars));
}

#[test]
fn unit_render_issue_comment_response_parts_includes_attachment_policy_reason_counts() {
    let event = test_issue_event();
    let mut report = test_prompt_run_report("summary");
    report.downloaded_attachments = vec![
        DownloadedGithubAttachment {
            source_url: "https://example.com/a.log".to_string(),
            original_name: "a.log".to_string(),
            path: PathBuf::from("/tmp/a.log"),
            relative_path: "attachments/a.log".to_string(),
            content_type: Some("text/plain".to_string()),
            bytes: 1,
            checksum_sha256: "a".repeat(64),
            policy_reason_code: "allow_extension_allowlist".to_string(),
            created_unix_ms: 1,
            expires_unix_ms: None,
        },
        DownloadedGithubAttachment {
            source_url: "https://example.com/b.txt".to_string(),
            original_name: "b.txt".to_string(),
            path: PathBuf::from("/tmp/b.txt"),
            relative_path: "attachments/b.txt".to_string(),
            content_type: Some("text/plain".to_string()),
            bytes: 1,
            checksum_sha256: "b".repeat(64),
            policy_reason_code: "allow_extension_allowlist".to_string(),
            created_unix_ms: 1,
            expires_unix_ms: None,
        },
    ];
    let (_content, footer) = render_issue_comment_response_parts(&event, &report);
    assert!(footer.contains("_attachments downloaded `2`"));
    assert!(footer.contains("policy_reason_counts `allow_extension_allowlist:2`"));
}

#[tokio::test]
async fn functional_post_issue_comment_chunks_updates_and_appends() {
    let server = MockServer::start();
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/901")
            .body_includes("chunk-1");
        then.status(200).json_body(json!({
            "id": 901,
            "html_url": "https://example.test/comment/901"
        }));
    });
    let append_one = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("chunk-2");
        then.status(201).json_body(json!({
            "id": 902,
            "html_url": "https://example.test/comment/902"
        }));
    });
    let append_two = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("chunk-3");
        then.status(201).json_body(json!({
            "id": 903,
            "html_url": "https://example.test/comment/903"
        }));
    });

    let client = GithubApiClient::new(
        server.base_url(),
        "token".to_string(),
        RepoRef::parse("owner/repo").expect("repo"),
        2_000,
        1,
        1,
    )
    .expect("client");
    let chunks = vec![
        "chunk-1".to_string(),
        "chunk-2".to_string(),
        "chunk-3".to_string(),
    ];
    let outcome = post_issue_comment_chunks(&client, 7, 901, &chunks).await;
    assert!(outcome.edit_attempted);
    assert!(outcome.edit_success);
    assert_eq!(outcome.append_count, 2);
    assert_eq!(outcome.posted_comment_id, Some(903));
    update.assert_calls(1);
    append_one.assert_calls(1);
    append_two.assert_calls(1);
}

#[tokio::test]
async fn regression_post_issue_comment_chunks_falls_back_on_edit_failure() {
    let server = MockServer::start();
    let update = server.mock(|when, then| {
        when.method(PATCH)
            .path("/repos/owner/repo/issues/comments/901");
        then.status(500);
    });
    let fallback = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("warning: failed to update placeholder comment");
        then.status(201).json_body(json!({
            "id": 910,
            "html_url": "https://example.test/comment/910"
        }));
    });
    let append = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/7/comments")
            .body_includes("chunk-2");
        then.status(201).json_body(json!({
            "id": 911,
            "html_url": "https://example.test/comment/911"
        }));
    });

    let client = GithubApiClient::new(
        server.base_url(),
        "token".to_string(),
        RepoRef::parse("owner/repo").expect("repo"),
        2_000,
        1,
        1,
    )
    .expect("client");
    let chunks = vec!["chunk-1".to_string(), "chunk-2".to_string()];
    let outcome = post_issue_comment_chunks(&client, 7, 901, &chunks).await;
    assert!(outcome.edit_attempted);
    assert!(!outcome.edit_success);
    assert_eq!(outcome.append_count, 2);
    assert_eq!(outcome.posted_comment_id, Some(911));
    update.assert_calls(1);
    fallback.assert_calls(1);
    append.assert_calls(1);
}

#[tokio::test]
async fn regression_post_issue_command_comment_spills_large_output_to_overflow_artifact() {
    let server = MockServer::start();
    let post = server.mock(|when, then| {
        when.method(POST)
            .path("/repos/owner/repo/issues/77/comments")
            .body_includes("output_truncated: true")
            .body_includes("overflow_artifact: id=`artifact-");
        then.status(201).json_body(json!({
            "id": 7702,
            "html_url": "https://example.test/comment/7702"
        }));
    });

    let temp = tempdir().expect("tempdir");
    let config = test_bridge_config(&server.base_url(), temp.path());
    let runtime = GithubIssuesBridgeRuntime::new(config)
        .await
        .expect("runtime");
    let oversized = "x".repeat(80_000);
    let posted = runtime
        .post_issue_command_comment(
            77,
            "issue-comment-created:7701",
            "status",
            "reported",
            &oversized,
        )
        .await
        .expect("post");
    assert_eq!(posted.id, 7702);
    post.assert_calls(1);

    let store = ChannelStore::open(
        &temp.path().join("owner__repo").join("channel-store"),
        "github",
        "issue-77",
    )
    .expect("channel store");
    let artifacts = store
        .load_artifact_records_tolerant()
        .expect("artifact records");
    let overflow_count = artifacts
        .records
        .iter()
        .filter(|record| record.artifact_type == "github-issue-command-overflow")
        .count();
    assert_eq!(overflow_count, 1);
}

#[test]
fn unit_parse_tau_issue_command_supports_known_commands() {
    assert_eq!(
        parse_tau_issue_command("/tau run investigate failures"),
        Some(TauIssueCommand::Run {
            prompt: "investigate failures".to_string()
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau status"),
        Some(TauIssueCommand::Status)
    );
    assert_eq!(
        parse_tau_issue_command("/tau health"),
        Some(TauIssueCommand::Health)
    );
    assert_eq!(
        parse_tau_issue_command("/tau stop"),
        Some(TauIssueCommand::Stop)
    );
    assert_eq!(
        parse_tau_issue_command("/tau help"),
        Some(TauIssueCommand::Help)
    );
    assert_eq!(
        parse_tau_issue_command("/tau summarize release blockers"),
        Some(TauIssueCommand::Summarize {
            focus: Some("release blockers".to_string())
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat start"),
        Some(TauIssueCommand::ChatStart)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat resume"),
        Some(TauIssueCommand::ChatResume)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat reset"),
        Some(TauIssueCommand::ChatReset)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat export"),
        Some(TauIssueCommand::ChatExport)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat status"),
        Some(TauIssueCommand::ChatStatus)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat summary"),
        Some(TauIssueCommand::ChatSummary)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat replay"),
        Some(TauIssueCommand::ChatReplay)
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat show"),
        Some(TauIssueCommand::ChatShow {
            limit: CHAT_SHOW_DEFAULT_LIMIT
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat show 25"),
        Some(TauIssueCommand::ChatShow { limit: 25 })
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat search alpha"),
        Some(TauIssueCommand::ChatSearch {
            query: "alpha".to_string(),
            role: None,
            limit: tau_session::SESSION_SEARCH_DEFAULT_RESULTS,
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau chat search alpha --role user --limit 25"),
        Some(TauIssueCommand::ChatSearch {
            query: "alpha".to_string(),
            role: Some("user".to_string()),
            limit: 25,
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau artifacts"),
        Some(TauIssueCommand::Artifacts {
            purge: false,
            run_id: None
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau artifacts purge"),
        Some(TauIssueCommand::Artifacts {
            purge: true,
            run_id: None
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau artifacts run run-seeded"),
        Some(TauIssueCommand::Artifacts {
            purge: false,
            run_id: Some("run-seeded".to_string())
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau artifacts show artifact-123"),
        Some(TauIssueCommand::ArtifactShow {
            artifact_id: "artifact-123".to_string()
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau demo-index list"),
        Some(TauIssueCommand::DemoIndexList)
    );
    assert_eq!(
        parse_tau_issue_command("/tau demo-index report"),
        Some(TauIssueCommand::DemoIndexReport)
    );
    assert_eq!(
        parse_tau_issue_command(
            "/tau demo-index run onboarding,gateway-auth --timeout-seconds 120"
        ),
        Some(TauIssueCommand::DemoIndexRun {
            command: DemoIndexRunCommand {
                scenarios: vec!["onboarding".to_string(), "gateway-auth".to_string()],
                timeout_seconds: 120,
            },
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau demo-index run"),
        Some(TauIssueCommand::DemoIndexRun {
            command: DemoIndexRunCommand {
                scenarios: DEMO_INDEX_SCENARIOS
                    .iter()
                    .map(|scenario| scenario.to_string())
                    .collect(),
                timeout_seconds: DEMO_INDEX_DEFAULT_TIMEOUT_SECONDS,
            },
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau doctor"),
        Some(TauIssueCommand::Doctor {
            command: IssueDoctorCommand { online: false },
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau doctor --online"),
        Some(TauIssueCommand::Doctor {
            command: IssueDoctorCommand { online: true },
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau auth status"),
        Some(TauIssueCommand::Auth {
            command: TauIssueAuthCommand {
                kind: TauIssueAuthCommandKind::Status,
                args: "status".to_string(),
            },
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau auth matrix --mode-support unsupported"),
        Some(TauIssueCommand::Auth {
            command: TauIssueAuthCommand {
                kind: TauIssueAuthCommandKind::Matrix,
                args: "matrix --mode-support unsupported".to_string(),
            },
        })
    );
    assert_eq!(
        parse_tau_issue_command("/tau canvas show architecture --json"),
        Some(TauIssueCommand::Canvas {
            args: "show architecture --json".to_string()
        })
    );
    assert_eq!(parse_tau_issue_command("plain message"), None);
}

#[test]
fn regression_parse_tau_issue_command_rejects_slash_like_inputs() {
    assert_eq!(parse_tau_issue_command("/taui run nope"), None);
    let parsed = parse_tau_issue_command("/tau run").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau artifacts extra").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau artifacts run").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau artifacts run run-a run-b").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau artifacts show").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed =
        parse_tau_issue_command("/tau artifacts show artifact-a extra").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau demo-index").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau demo-index list extra").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed =
        parse_tau_issue_command("/tau demo-index run unknown-scenario").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau demo-index run onboarding --timeout-seconds 0")
        .expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau demo-index run onboarding --timeout-seconds 10000")
        .expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau doctor --json").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau doctor --online extra").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau auth").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau auth login openai").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau auth status --invalid").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau canvas").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau help extra").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau health extra").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat start now").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat export now").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat status now").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat summary now").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat replay now").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat show foo").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat show 99 100").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat search").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed =
        parse_tau_issue_command("/tau chat search alpha --role nope").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed =
        parse_tau_issue_command("/tau chat search alpha --limit 0").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed =
        parse_tau_issue_command("/tau chat search alpha --limit 99").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let parsed = parse_tau_issue_command("/tau chat unknown").expect("command parse");
    assert!(matches!(parsed, TauIssueCommand::Invalid { .. }));
    let action = event_action_from_shared_body("/tau unknown", parse_tau_issue_command);
    assert!(matches!(
        action,
        EventAction::Command(TauIssueCommand::Invalid { .. })
    ));
}
