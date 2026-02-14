//! Tests for channel-store admin status inspection and multi-channel incident reporting flows.

use super::*;

#[test]
fn functional_execute_channel_store_admin_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let store = crate::channel_store::ChannelStore::open(temp.path(), "github", "issue-1")
        .expect("open channel store");
    store
        .append_log_entry(&crate::channel_store::ChannelLogEntry {
            timestamp_unix_ms: 1,
            direction: "inbound".to_string(),
            event_key: Some("e1".to_string()),
            source: "github".to_string(),
            payload: serde_json::json!({"body":"hello"}),
        })
        .expect("append log");
    store
        .write_text_artifact(
            "run-active",
            "github-reply",
            "private",
            Some(30),
            "md",
            "artifact body",
        )
        .expect("write artifact");
    let mut artifact_index =
        std::fs::read_to_string(store.artifact_index_path()).expect("read artifact index");
    artifact_index.push_str("invalid-artifact-line\n");
    std::fs::write(store.artifact_index_path(), artifact_index).expect("seed invalid artifact");

    let mut cli = test_cli();
    cli.channel_store_root = temp.path().to_path_buf();
    cli.channel_store_inspect = Some("github/issue-1".to_string());

    execute_channel_store_admin_command(&cli).expect("inspect should succeed");
    let report = store.inspect().expect("inspect report");
    assert_eq!(report.artifact_records, 1);
    assert_eq!(report.invalid_artifact_lines, 1);
    assert_eq!(report.active_artifacts, 1);
    assert_eq!(report.expired_artifacts, 0);
}

#[test]
fn regression_execute_channel_store_admin_repair_removes_invalid_lines() {
    let temp = tempdir().expect("tempdir");
    let store = crate::channel_store::ChannelStore::open(temp.path(), "slack", "C123")
        .expect("open channel store");
    std::fs::write(store.log_path(), "{\"ok\":true}\ninvalid-json-line\n")
        .expect("seed invalid log");
    let expired = store
        .write_text_artifact(
            "run-expired",
            "slack-reply",
            "private",
            Some(0),
            "md",
            "expired artifact",
        )
        .expect("write expired artifact");
    let mut artifact_index =
        std::fs::read_to_string(store.artifact_index_path()).expect("read artifact index");
    artifact_index.push_str("invalid-artifact-line\n");
    std::fs::write(store.artifact_index_path(), artifact_index).expect("seed invalid artifact");

    let mut cli = test_cli();
    cli.channel_store_root = temp.path().to_path_buf();
    cli.channel_store_repair = Some("slack/C123".to_string());
    execute_channel_store_admin_command(&cli).expect("repair should succeed");

    let report = store.inspect().expect("inspect after repair");
    assert_eq!(report.invalid_log_lines, 0);
    assert_eq!(report.log_records, 1);
    assert_eq!(report.invalid_artifact_lines, 0);
    assert_eq!(report.expired_artifacts, 0);
    assert_eq!(report.active_artifacts, 0);
    assert!(!store.channel_dir().join(expired.relative_path).exists());
}

#[test]
fn functional_execute_channel_store_admin_github_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let github_state_dir = temp.path().join("github");
    let repo_state_dir = github_state_dir.join("owner__repo");
    std::fs::create_dir_all(&repo_state_dir).expect("create github repo dir");
    std::fs::write(
        repo_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": ["issue-comment-created:1"],
  "issue_sessions": {
    "7": {
      "session_id": "issue-7",
      "last_run_id": "run-7",
      "last_event_key": "issue-comment-created:1",
      "last_event_kind": "issue_comment_created",
      "last_actor_login": "alice",
      "last_reason_code": "command_processed",
      "total_processed_events": 1
    }
  },
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write github state");
    std::fs::write(
        repo_state_dir.join("inbound-events.jsonl"),
        r#"{"kind":"issue_comment_created","event_key":"issue-comment-created:1"}
"#,
    )
    .expect("write inbound");
    std::fs::write(
        repo_state_dir.join("outbound-events.jsonl"),
        r#"{"event_key":"issue-comment-created:1","command":"chat-status","status":"reported","reason_code":"command_processed"}
"#,
    )
    .expect("write outbound");

    let mut cli = test_cli();
    cli.github_status_inspect = Some("owner/repo".to_string());
    cli.github_status_json = true;
    cli.github_state_dir = github_state_dir;

    execute_channel_store_admin_command(&cli).expect("github status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_github_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.github_status_inspect = Some("owner/repo".to_string());
    cli.github_state_dir = temp.path().join("github");
    std::fs::create_dir_all(cli.github_state_dir.join("owner__repo")).expect("create repo dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("github status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_dashboard_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let dashboard_state_dir = temp.path().join("dashboard");
    std::fs::create_dir_all(&dashboard_state_dir).expect("create dashboard state dir");
    std::fs::write(
        dashboard_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["snapshot:s1"],
  "widget_views": [{"widget_id":"health-summary"}],
  "control_audit": [{"case_id":"c1"}],
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write dashboard state");
    std::fs::write(
        dashboard_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["widget_views_updated"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write dashboard events");

    let mut cli = test_cli();
    cli.dashboard_status_inspect = true;
    cli.dashboard_status_json = true;
    cli.dashboard_state_dir = dashboard_state_dir;
    execute_channel_store_admin_command(&cli).expect("dashboard status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_dashboard_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.dashboard_status_inspect = true;
    cli.dashboard_state_dir = temp.path().join("dashboard");
    std::fs::create_dir_all(&cli.dashboard_state_dir).expect("create dashboard dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("dashboard status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_multi_channel_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let multi_channel_state_dir = temp.path().join("multi-channel");
    std::fs::create_dir_all(&multi_channel_state_dir).expect("create multi-channel state dir");
    std::fs::write(
        multi_channel_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1", "discord:dc-1", "whatsapp:wa-1"],
  "health": {
    "updated_unix_ms": 701,
    "cycle_duration_ms": 16,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 3,
    "last_cycle_completed": 3,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-channel state");
    std::fs::write(
        multi_channel_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["healthy_cycle","events_applied"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write multi-channel events");

    let mut cli = test_cli();
    cli.multi_channel_status_inspect = true;
    cli.multi_channel_status_json = true;
    cli.multi_channel_state_dir = multi_channel_state_dir;
    execute_channel_store_admin_command(&cli).expect("multi-channel status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_multi_channel_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_channel_status_inspect = true;
    cli.multi_channel_state_dir = temp.path().join("multi-channel");
    std::fs::create_dir_all(&cli.multi_channel_state_dir).expect("create multi-channel dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("multi-channel status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_build_multi_channel_route_inspect_report_resolves_binding_and_role() {
    let temp = tempdir().expect("tempdir");
    let route_table_path = temp.path().join("route-table.json");
    let state_dir = temp.path().join("multi-channel");
    let security_dir = state_dir.join("security");
    std::fs::create_dir_all(&security_dir).expect("create security dir");
    std::fs::write(
        route_table_path.as_path(),
        r#"{
  "schema_version": 1,
  "roles": {
    "triage": {},
    "default": {}
  },
  "planner": { "role": "default" },
  "delegated": { "role": "default" },
  "delegated_categories": {
    "incident": { "role": "triage" }
  },
  "review": { "role": "default" }
}"#,
    )
    .expect("write route table");
    std::fs::write(
        security_dir.join("multi-channel-route-bindings.json"),
        r#"{
  "schema_version": 1,
  "bindings": [
    {
      "binding_id": "discord-ops",
      "transport": "discord",
      "account_id": "discord-main",
      "conversation_id": "ops-room",
      "actor_id": "*",
      "phase": "delegated_step",
      "category_hint": "incident",
      "session_key_template": "session-{role}"
    }
  ]
}"#,
    )
    .expect("write route bindings");
    let event_path = temp.path().join("event.json");
    std::fs::write(
        &event_path,
        r#"{
  "schema_version": 1,
  "transport": "discord",
  "event_kind": "message",
  "event_id": "dc-route-1",
  "conversation_id": "ops-room",
  "actor_id": "discord-user-1",
  "timestamp_ms": 1760200000000,
  "text": "please triage this incident",
  "metadata": {
    "account_id": "discord-main"
  }
}"#,
    )
    .expect("write event file");

    let mut cli = test_cli();
    cli.multi_channel_route_inspect_file = Some(event_path.clone());
    cli.multi_channel_state_dir = state_dir;
    cli.orchestrator_route_table = Some(route_table_path);
    let report = build_multi_channel_route_inspect_report(
        &tau_multi_channel::MultiChannelRouteInspectConfig {
            inspect_file: event_path.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        },
    )
    .expect("build report");
    assert_eq!(report.binding_id, "discord-ops");
    assert_eq!(report.selected_role, "triage");
    assert_eq!(report.phase, "delegated-step");
    assert_eq!(report.session_key, "session-triage");
}

#[test]
fn integration_execute_multi_channel_route_inspect_command_accepts_live_envelope_input() {
    let temp = tempdir().expect("tempdir");
    let state_dir = temp.path().join("multi-channel");
    std::fs::create_dir_all(state_dir.join("security")).expect("create security dir");
    let envelope_path = temp.path().join("telegram-envelope.json");
    std::fs::write(
        &envelope_path,
        r#"{
  "schema_version": 1,
  "transport": "telegram",
  "provider": "telegram-bot-api",
  "payload": {
    "update_id": 70001,
    "message": {
      "message_id": 501,
      "date": 1760200100,
      "text": "/status",
      "chat": { "id": "chat-100" },
      "from": { "id": "user-100", "username": "ops-user" }
    }
  }
}"#,
    )
    .expect("write envelope file");

    let mut cli = test_cli();
    cli.multi_channel_route_inspect_file = Some(envelope_path.clone());
    cli.multi_channel_route_inspect_json = true;
    cli.multi_channel_state_dir = state_dir;
    let report = build_multi_channel_route_inspect_report(
        &tau_multi_channel::MultiChannelRouteInspectConfig {
            inspect_file: envelope_path.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        },
    )
    .expect("build report");
    serde_json::to_string_pretty(&report).expect("render report");
}

#[test]
fn regression_build_multi_channel_route_inspect_report_rejects_empty_file() {
    let temp = tempdir().expect("tempdir");
    let event_path = temp.path().join("empty.json");
    std::fs::write(&event_path, "  \n").expect("write empty event file");

    let mut cli = test_cli();
    cli.multi_channel_route_inspect_file = Some(event_path.clone());
    let error = build_multi_channel_route_inspect_report(
        &tau_multi_channel::MultiChannelRouteInspectConfig {
            inspect_file: event_path.clone(),
            state_dir: cli.multi_channel_state_dir.clone(),
            orchestrator_route_table_path: cli.orchestrator_route_table.clone(),
        },
    )
    .expect_err("empty route inspect file should fail");
    assert!(error.to_string().contains("is empty"));
    assert!(error
        .to_string()
        .contains(event_path.to_string_lossy().as_ref()));
}

#[test]
fn unit_build_multi_channel_incident_timeline_report_aggregates_outcomes_and_reason_codes() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/discord/ops-room");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200000000,"direction":"inbound","event_key":"evt-allow","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200000010,"direction":"outbound","event_key":"evt-allow","source":"tau-multi-channel-runner","payload":{"event_key":"evt-allow","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
{"timestamp_unix_ms":1760200000020,"direction":"inbound","event_key":"evt-denied","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"deny_channel_policy_mention_required"}}}
{"timestamp_unix_ms":1760200000030,"direction":"outbound","event_key":"evt-denied","source":"tau-multi-channel-runner","payload":{"status":"denied","reason_code":"deny_channel_policy_mention_required"}}
{"timestamp_unix_ms":1760200000040,"direction":"inbound","event_key":"evt-retried","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200000050,"direction":"outbound","event_key":"evt-retried","source":"tau-multi-channel-runner","payload":{"status":"delivery_failed","reason_code":"delivery_provider_unavailable","retryable":true}}
{"timestamp_unix_ms":1760200000060,"direction":"outbound","event_key":"evt-retried","source":"tau-multi-channel-runner","payload":{"event_key":"evt-retried","response":"after retry","delivery":{"mode":"provider","receipts":[{"status":"sent"}]}}}
{"timestamp_unix_ms":1760200000070,"direction":"inbound","event_key":"evt-failed","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200000080,"direction":"outbound","event_key":"evt-failed","source":"tau-multi-channel-runner","payload":{"status":"delivery_failed","reason_code":"delivery_request_rejected","retryable":false}}
"#,
    )
    .expect("write channel log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 4);
    assert_eq!(report.outcomes.allowed, 1);
    assert_eq!(report.outcomes.denied, 1);
    assert_eq!(report.outcomes.retried, 1);
    assert_eq!(report.outcomes.failed, 1);
    assert_eq!(
        report
            .policy_reason_code_counts
            .get("allow_channel_policy_allow_from_any")
            .copied(),
        Some(3)
    );
    assert_eq!(
        report
            .policy_reason_code_counts
            .get("deny_channel_policy_mention_required")
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .delivery_reason_code_counts
            .get("delivery_provider_unavailable")
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .delivery_reason_code_counts
            .get("delivery_request_rejected")
            .copied(),
        Some(1)
    );
}

#[test]
fn functional_build_multi_channel_incident_timeline_report_writes_replay_export() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;
    cli.multi_channel_incident_event_limit = Some(1);
    let replay_export_path = temp.path().join("artifacts/incident-replay.json");
    cli.multi_channel_incident_replay_export = Some(replay_export_path.clone());

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/telegram/ops-chat");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200100000,"direction":"inbound","event_key":"evt-1","source":"telegram","payload":{"transport":"telegram","conversation_id":"ops-chat","route_session_key":"ops-chat","route":{"binding_id":"telegram-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200100010,"direction":"outbound","event_key":"evt-1","source":"tau-multi-channel-runner","payload":{"event_key":"evt-1","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
{"timestamp_unix_ms":1760200100020,"direction":"inbound","event_key":"evt-2","source":"telegram","payload":{"transport":"telegram","conversation_id":"ops-chat","route_session_key":"ops-chat","route":{"binding_id":"telegram-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200100030,"direction":"outbound","event_key":"evt-2","source":"tau-multi-channel-runner","payload":{"status":"denied","reason_code":"deny_channel_policy_mention_required"}}
"#,
    )
    .expect("write channel log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 1);
    assert_eq!(report.truncated_event_count, 1);
    let replay_export = report.replay_export.expect("replay export summary");
    assert_eq!(replay_export.path, replay_export_path.display().to_string());
    assert!(!replay_export.checksum_sha256.is_empty());

    let replay_raw = std::fs::read_to_string(&replay_export_path).expect("read replay artifact");
    let replay_json: serde_json::Value =
        serde_json::from_str(&replay_raw).expect("parse replay artifact");
    assert_eq!(replay_json["schema_version"].as_u64(), Some(1));
    assert_eq!(
        replay_json["events"].as_array().map(|items| items.len()),
        Some(1)
    );
}

#[test]
fn integration_build_multi_channel_incident_timeline_report_reads_sample_channel_store_corpus() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;

    let telegram_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/telegram/tg-ops");
    let discord_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/discord/dc-ops");
    let whatsapp_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/whatsapp/wa-ops");
    std::fs::create_dir_all(&telegram_dir).expect("create telegram dir");
    std::fs::create_dir_all(&discord_dir).expect("create discord dir");
    std::fs::create_dir_all(&whatsapp_dir).expect("create whatsapp dir");
    std::fs::write(
        telegram_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200400000,"direction":"inbound","event_key":"evt-tg","source":"telegram","payload":{"transport":"telegram","conversation_id":"tg-ops","route_session_key":"tg-ops","route":{"binding_id":"tg-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200400010,"direction":"outbound","event_key":"evt-tg","source":"tau-multi-channel-runner","payload":{"event_key":"evt-tg","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
"#,
    )
    .expect("write telegram log");
    std::fs::write(
        discord_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200400100,"direction":"inbound","event_key":"evt-dc","source":"discord","payload":{"transport":"discord","conversation_id":"dc-ops","route_session_key":"dc-ops","route":{"binding_id":"dc-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200400110,"direction":"outbound","event_key":"evt-dc","source":"tau-multi-channel-runner","payload":{"status":"delivery_failed","reason_code":"delivery_provider_unavailable","retryable":true}}
{"timestamp_unix_ms":1760200400120,"direction":"outbound","event_key":"evt-dc","source":"tau-multi-channel-runner","payload":{"event_key":"evt-dc","response":"ok","delivery":{"mode":"provider","receipts":[{"status":"sent"}]}}}
"#,
    )
    .expect("write discord log");
    std::fs::write(
        whatsapp_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200400200,"direction":"inbound","event_key":"evt-wa","source":"whatsapp","payload":{"transport":"whatsapp","conversation_id":"wa-ops","route_session_key":"wa-ops","route":{"binding_id":"wa-ops","binding_matched":false},"channel_policy":{"reason_code":"deny_channel_policy_mention_required"}}}
{"timestamp_unix_ms":1760200400210,"direction":"outbound","event_key":"evt-wa","source":"tau-multi-channel-runner","payload":{"status":"denied","reason_code":"deny_channel_policy_mention_required"}}
"#,
    )
    .expect("write whatsapp log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 3);
    assert_eq!(report.scanned_channel_count, 3);
    assert_eq!(report.outcomes.allowed, 1);
    assert_eq!(report.outcomes.retried, 1);
    assert_eq!(report.outcomes.denied, 1);
    assert_eq!(
        report
            .route_reason_code_counts
            .get("route_binding_matched")
            .copied(),
        Some(2)
    );
    assert_eq!(
        report
            .route_reason_code_counts
            .get("route_binding_default")
            .copied(),
        Some(1)
    );
}

#[test]
fn regression_build_multi_channel_incident_timeline_report_tolerates_malformed_log_lines() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/whatsapp/ops-room");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200200000,"direction":"inbound","event_key":"evt-1","source":"whatsapp","payload":{"transport":"whatsapp","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"wa-ops","binding_matched":false},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{bad-json-line
{"timestamp_unix_ms":1760200200010,"direction":"outbound","event_key":"evt-1","source":"tau-multi-channel-runner","payload":{"event_key":"evt-1","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
"#,
    )
    .expect("write channel log");

    let report = build_multi_channel_incident_timeline_report(
        &tau_multi_channel::MultiChannelIncidentTimelineQuery {
            state_dir: cli.multi_channel_state_dir.clone(),
            window_start_unix_ms: cli.multi_channel_incident_start_unix_ms,
            window_end_unix_ms: cli.multi_channel_incident_end_unix_ms,
            event_limit: cli.multi_channel_incident_event_limit.unwrap_or(200),
            replay_export_path: cli.multi_channel_incident_replay_export.clone(),
        },
    )
    .expect("build incident timeline report");
    assert_eq!(report.timeline.len(), 1);
    assert_eq!(report.invalid_line_count, 1);
    assert!(
        !report.diagnostics.is_empty(),
        "malformed line should surface diagnostic message"
    );
}

#[test]
fn functional_execute_channel_store_admin_multi_agent_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let multi_agent_state_dir = temp.path().join("multi-agent");
    std::fs::create_dir_all(&multi_agent_state_dir).expect("create multi-agent state dir");
    std::fs::write(
        multi_agent_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["planner:planner-success"],
  "routed_cases": [
    {
      "case_key": "planner:planner-success",
      "case_id": "planner-success",
      "phase": "planner",
      "selected_role": "planner",
      "attempted_roles": ["planner"],
      "category": "planning",
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 702,
    "cycle_duration_ms": 19,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-agent state");
    std::fs::write(
        multi_agent_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["routed_cases_updated"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write multi-agent events");

    let mut cli = test_cli();
    cli.multi_agent_status_inspect = true;
    cli.multi_agent_status_json = true;
    cli.multi_agent_state_dir = multi_agent_state_dir;
    execute_channel_store_admin_command(&cli).expect("multi-agent status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_multi_agent_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.multi_agent_status_inspect = true;
    cli.multi_agent_state_dir = temp.path().join("multi-agent");
    std::fs::create_dir_all(&cli.multi_agent_state_dir).expect("create multi-agent dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("multi-agent status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_gateway_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let gateway_state_dir = temp.path().join("gateway");
    std::fs::create_dir_all(&gateway_state_dir).expect("create gateway state dir");
    std::fs::write(
        gateway_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["POST:/v1/tasks:gateway-success"],
  "requests": [
    {
      "case_key": "POST:/v1/tasks:gateway-success",
      "case_id": "gateway-success",
      "method": "POST",
      "endpoint": "/v1/tasks",
      "actor_id": "ops-bot",
      "status_code": 201,
      "outcome": "success",
      "error_code": "",
      "response_body": {"status":"accepted"},
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 705,
    "cycle_duration_ms": 15,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write gateway state");
    std::fs::write(
        gateway_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["healthy_cycle"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write gateway events");

    let mut cli = test_cli();
    cli.gateway_status_inspect = true;
    cli.gateway_status_json = true;
    cli.gateway_state_dir = gateway_state_dir;
    execute_channel_store_admin_command(&cli).expect("gateway status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_gateway_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.gateway_status_inspect = true;
    cli.gateway_state_dir = temp.path().join("gateway");
    std::fs::create_dir_all(&cli.gateway_state_dir).expect("create gateway dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("gateway status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_custom_command_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let custom_command_state_dir = temp.path().join("custom-command");
    std::fs::create_dir_all(&custom_command_state_dir).expect("create custom-command state dir");
    std::fs::write(
        custom_command_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["CREATE:deploy_release:create-1"],
  "commands": [
    {
      "case_key": "CREATE:deploy_release:create-1",
      "case_id": "create-1",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "operation": "CREATE",
      "last_status_code": 201,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 710,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write custom-command state");
    std::fs::write(
        custom_command_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["command_registry_mutated"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write custom-command events");

    let mut cli = test_cli();
    cli.custom_command_status_inspect = true;
    cli.custom_command_status_json = true;
    cli.custom_command_state_dir = custom_command_state_dir;
    execute_channel_store_admin_command(&cli)
        .expect("custom-command status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_custom_command_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.custom_command_status_inspect = true;
    cli.custom_command_state_dir = temp.path().join("custom-command");
    std::fs::create_dir_all(&cli.custom_command_state_dir).expect("create custom-command dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("custom-command status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}

#[test]
fn functional_execute_channel_store_admin_voice_status_inspect_succeeds() {
    let temp = tempdir().expect("tempdir");
    let voice_state_dir = temp.path().join("voice");
    std::fs::create_dir_all(&voice_state_dir).expect("create voice state dir");
    std::fs::write(
        voice_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["turn:tau:ops-1:voice-success-turn"],
  "interactions": [
    {
      "case_key": "turn:tau:ops-1:voice-success-turn",
      "case_id": "voice-success-turn",
      "mode": "turn",
      "wake_word": "tau",
      "locale": "en-US",
      "speaker_id": "ops-1",
      "utterance": "open dashboard",
      "last_status_code": 202,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 1
    }
  ],
  "health": {
    "updated_unix_ms": 720,
    "cycle_duration_ms": 12,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write voice state");
    std::fs::write(
        voice_state_dir.join("runtime-events.jsonl"),
        r#"{"reason_codes":["turns_handled"],"health_reason":"no recent transport failures observed"}
"#,
    )
    .expect("write voice events");

    let mut cli = test_cli();
    cli.voice_status_inspect = true;
    cli.voice_status_json = true;
    cli.voice_state_dir = voice_state_dir;
    execute_channel_store_admin_command(&cli).expect("voice status inspect should succeed");
}

#[test]
fn regression_execute_channel_store_admin_voice_status_inspect_requires_state_file() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    cli.voice_status_inspect = true;
    cli.voice_state_dir = temp.path().join("voice");
    std::fs::create_dir_all(&cli.voice_state_dir).expect("create voice dir");

    let error = execute_channel_store_admin_command(&cli)
        .expect_err("voice status inspect should fail without state file");
    assert!(error.to_string().contains("failed to read"));
    assert!(error.to_string().contains("state.json"));
}
