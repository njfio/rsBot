//! Tests for event templates, scheduler behavior, and runtime execution safeguards.

use std::{path::Path, sync::Arc, time::Duration};

use anyhow::Result;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tau_ai::Message;
use tempfile::tempdir;

use super::{
    dry_run_events, due_decision, enforce_events_dry_run_gate, enforce_events_dry_run_strict_mode,
    evaluate_events_dry_run_gate, ingest_webhook_immediate_event, inspect_events,
    load_event_records, next_periodic_due_unix_ms, render_events_dry_run_gate_summary,
    render_events_dry_run_report, render_events_inspect_report, render_events_simulate_report,
    render_events_validate_report, simulate_events, validate_events_definitions,
    write_event_template, DueDecision, EventDefinition, EventRunner, EventRunnerState,
    EventSchedule, EventSchedulerConfig, EventSchedulerRuntime, EventTemplateSchedule,
    EventWebhookIngestConfig, EventsDryRunConfig, EventsDryRunGateConfig, EventsInspectConfig,
    EventsSimulateConfig, EventsTemplateConfig, EventsValidateConfig, EventsValidateReport,
    WebhookSignatureAlgorithm,
};
use tau_runtime::channel_store::ChannelLogEntry;

#[derive(Clone, Default)]
struct DummyRunner;

#[async_trait]
impl EventRunner for DummyRunner {
    async fn run_event(
        &self,
        event: &EventDefinition,
        now_unix_ms: u64,
        channel_store: &tau_runtime::channel_store::ChannelStore,
    ) -> Result<()> {
        channel_store.sync_context_from_messages(&[Message::assistant_text("scheduled reply")])?;
        channel_store.append_log_entry(&ChannelLogEntry {
            timestamp_unix_ms: now_unix_ms,
            direction: "outbound".to_string(),
            event_key: Some(event.id.clone()),
            source: "events".to_string(),
            payload: serde_json::json!({
                "event_id": event.id,
                "status": "success",
                "assistant_reply": "scheduled reply"
            }),
        })?;
        Ok(())
    }
}

fn scheduler_config(root: &Path) -> EventSchedulerConfig {
    EventSchedulerConfig {
        runner: Arc::new(DummyRunner),
        channel_store_root: root.join("channel-store"),
        events_dir: root.join("events"),
        state_path: root.join("events/state.json"),
        poll_interval: Duration::from_millis(1),
        queue_limit: 16,
        stale_immediate_max_age_seconds: 3_600,
    }
}

fn write_event(path: &Path, event: &EventDefinition) {
    let mut payload = serde_json::to_string_pretty(event).expect("serialize event");
    payload.push('\n');
    std::fs::write(path, payload).expect("write event file");
}

fn template_config(path: &Path) -> EventsTemplateConfig {
    EventsTemplateConfig {
        target_path: path.to_path_buf(),
        overwrite: false,
        schedule: EventTemplateSchedule::Immediate,
        channel: "slack/C123".to_string(),
        prompt: "".to_string(),
        event_id: None,
        at_unix_ms: None,
        cron: None,
        timezone: Some("UTC".to_string()),
    }
}

fn github_signature(secret: &str, payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("mac");
    mac.update(payload.as_bytes());
    let digest = mac.finalize().into_bytes();
    format!(
        "sha256={}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn slack_v0_signature(secret: &str, timestamp: &str, payload: &str) -> String {
    let signed = format!("v0:{timestamp}:{payload}");
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("mac");
    mac.update(signed.as_bytes());
    let digest = mac.finalize().into_bytes();
    format!(
        "v0={}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

#[test]
fn unit_due_decision_and_cron_timezone_computation_are_stable() {
    let now = 1_700_000_000_000_u64;
    let mut state = EventRunnerState::default();
    state
        .periodic_last_run_unix_ms
        .insert("periodic-1".to_string(), now.saturating_sub(120_000));

    let periodic = EventDefinition {
        id: "periodic-1".to_string(),
        channel: "slack/C1".to_string(),
        prompt: "check".to_string(),
        schedule: EventSchedule::Periodic {
            cron: "0/1 * * * * * *".to_string(),
            timezone: "UTC".to_string(),
        },
        enabled: true,
        created_unix_ms: Some(now),
    };
    let decision = due_decision(&periodic, &state, now, 3_600).expect("due decision");
    assert!(matches!(decision, DueDecision::Run | DueDecision::NotDue));

    let next = next_periodic_due_unix_ms("0/5 * * * * * *", "UTC", now).expect("next due");
    assert!(next >= now);
}

#[test]
fn functional_event_file_lifecycle_and_malformed_skip_behavior() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");

    let event = EventDefinition {
        id: "event-1".to_string(),
        channel: "github/issue-1".to_string(),
        prompt: "summarize issue".to_string(),
        schedule: EventSchedule::Immediate,
        enabled: true,
        created_unix_ms: Some(100),
    };
    write_event(&events_dir.join("event-1.json"), &event);
    std::fs::write(events_dir.join("broken.json"), "{not-json").expect("write malformed");

    let (records, malformed) = load_event_records(&events_dir).expect("load records");
    assert_eq!(records.len(), 1);
    assert_eq!(malformed, 1);
    assert_eq!(records[0].definition.id, "event-1");
}

#[test]
fn unit_events_inspect_report_counts_due_and_schedule_buckets() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");

    let now = 1_700_000_000_000_u64;
    write_event(
        &events_dir.join("immediate.json"),
        &EventDefinition {
            id: "immediate".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "run now".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );
    write_event(
        &events_dir.join("at.json"),
        &EventDefinition {
            id: "at-later".to_string(),
            channel: "github/owner/repo#1".to_string(),
            prompt: "wait".to_string(),
            schedule: EventSchedule::At {
                at_unix_ms: now.saturating_add(5_000),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );
    write_event(
        &events_dir.join("periodic-disabled.json"),
        &EventDefinition {
            id: "periodic-disabled".to_string(),
            channel: "github/owner/repo#2".to_string(),
            prompt: "periodic".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "0/1 * * * * * *".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: false,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );

    let report = inspect_events(
        &EventsInspectConfig {
            events_dir,
            state_path,
            queue_limit: 1,
            stale_immediate_max_age_seconds: 3_600,
        },
        now,
    )
    .expect("inspect report");

    assert_eq!(report.discovered_events, 3);
    assert_eq!(report.malformed_events, 0);
    assert_eq!(report.enabled_events, 2);
    assert_eq!(report.disabled_events, 1);
    assert_eq!(report.schedule_immediate_events, 1);
    assert_eq!(report.schedule_at_events, 1);
    assert_eq!(report.schedule_periodic_events, 1);
    assert_eq!(report.due_now_events, 1);
    assert_eq!(report.queued_now_events, 1);
    assert_eq!(report.not_due_events, 2);
    assert_eq!(report.stale_immediate_events, 0);
    assert_eq!(report.due_eval_failed_events, 0);
    assert_eq!(report.periodic_with_last_run_state, 0);
    assert_eq!(report.periodic_missing_last_run_state, 1);
}

#[test]
fn functional_events_inspect_render_includes_operator_fields() {
    let rendered = render_events_inspect_report(&super::EventsInspectReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/events/state.json".to_string(),
        now_unix_ms: 1_234,
        queue_limit: 8,
        stale_immediate_max_age_seconds: 600,
        discovered_events: 4,
        malformed_events: 1,
        enabled_events: 3,
        disabled_events: 1,
        schedule_immediate_events: 2,
        schedule_at_events: 1,
        schedule_periodic_events: 1,
        due_now_events: 2,
        queued_now_events: 2,
        not_due_events: 1,
        stale_immediate_events: 1,
        due_eval_failed_events: 0,
        periodic_with_last_run_state: 1,
        periodic_missing_last_run_state: 0,
    });

    assert!(rendered.contains("events inspect:"));
    assert!(rendered.contains("events_dir=/tmp/events"));
    assert!(rendered.contains("due_now_events=2"));
    assert!(rendered.contains("queued_now_events=2"));
    assert!(rendered.contains("schedule_periodic_events=1"));
    assert!(rendered.contains("queue_limit=8"));
}

#[test]
fn integration_events_inspect_reads_state_and_applies_queue_limit() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");

    let now = 1_700_000_100_000_u64;
    write_event(
        &events_dir.join("due-a.json"),
        &EventDefinition {
            id: "due-a".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "a".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );
    write_event(
        &events_dir.join("due-b.json"),
        &EventDefinition {
            id: "due-b".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "b".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );
    write_event(
        &events_dir.join("periodic.json"),
        &EventDefinition {
            id: "periodic".to_string(),
            channel: "github/owner/repo#9".to_string(),
            prompt: "periodic".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "0/1 * * * * * *".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: false,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );
    std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");

    std::fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "periodic_last_run_unix_ms": {
"periodic": 1700000000000
  },
  "debounce_last_seen_unix_ms": {},
  "signature_replay_last_seen_unix_ms": {}
}
"#,
    )
    .expect("write state");

    let report = inspect_events(
        &EventsInspectConfig {
            events_dir,
            state_path,
            queue_limit: 1,
            stale_immediate_max_age_seconds: 3_600,
        },
        now,
    )
    .expect("inspect report");

    assert_eq!(report.discovered_events, 3);
    assert_eq!(report.malformed_events, 1);
    assert_eq!(report.due_now_events, 2);
    assert_eq!(report.queued_now_events, 1);
    assert_eq!(report.periodic_with_last_run_state, 1);
    assert_eq!(report.periodic_missing_last_run_state, 0);
}

#[test]
fn regression_events_inspect_handles_invalid_periodic_and_missing_state_file() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("missing/state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");

    let now = 1_700_000_200_000_u64;
    write_event(
        &events_dir.join("invalid-periodic.json"),
        &EventDefinition {
            id: "invalid-periodic".to_string(),
            channel: "github/owner/repo#3".to_string(),
            prompt: "periodic".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "invalid-cron".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(200)),
        },
    );
    write_event(
        &events_dir.join("stale-immediate.json"),
        &EventDefinition {
            id: "stale-immediate".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "stale".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(120_000)),
        },
    );

    let report = inspect_events(
        &EventsInspectConfig {
            events_dir,
            state_path,
            queue_limit: 8,
            stale_immediate_max_age_seconds: 60,
        },
        now,
    )
    .expect("inspect report");

    assert_eq!(report.discovered_events, 2);
    assert_eq!(report.due_eval_failed_events, 1);
    assert_eq!(report.stale_immediate_events, 1);
    assert_eq!(report.queued_now_events, 0);
}

#[test]
fn unit_validate_events_definitions_classifies_channel_and_schedule_failures() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");

    write_event(
        &events_dir.join("invalid.json"),
        &EventDefinition {
            id: "invalid".to_string(),
            channel: "slack".to_string(),
            prompt: "bad".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "not-a-cron".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: true,
            created_unix_ms: Some(1_700_000_000_000),
        },
    );

    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir,
            state_path: temp.path().join("state.json"),
        },
        1_700_000_100_000,
    )
    .expect("validate report");

    assert_eq!(report.total_files, 1);
    assert_eq!(report.valid_files, 0);
    assert_eq!(report.invalid_files, 1);
    assert_eq!(report.malformed_files, 0);
    assert_eq!(report.failed_files, 1);
    assert_eq!(report.diagnostics.len(), 2);
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.reason_code == "channel_ref_invalid"));
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.reason_code == "schedule_invalid"));
}

#[test]
fn functional_render_events_validate_report_includes_summary_and_diagnostics() {
    let rendered = render_events_validate_report(&EventsValidateReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/events/state.json".to_string(),
        now_unix_ms: 1_234,
        total_files: 3,
        valid_files: 1,
        invalid_files: 1,
        malformed_files: 1,
        failed_files: 2,
        disabled_files: 1,
        diagnostics: vec![super::EventsValidateDiagnostic {
            path: "/tmp/events/bad.json".to_string(),
            event_id: Some("bad".to_string()),
            reason_code: "schedule_invalid".to_string(),
            message: "invalid cron expression".to_string(),
        }],
    });

    assert!(rendered.contains("events validate:"));
    assert!(rendered.contains("failed_files=2"));
    assert!(rendered.contains("events validate error:"));
    assert!(rendered.contains("reason_code=schedule_invalid"));
}

#[test]
fn integration_validate_events_definitions_reports_mixed_file_health() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let state_path = temp.path().join("state.json");

    write_event(
        &events_dir.join("valid.json"),
        &EventDefinition {
            id: "valid".to_string(),
            channel: "slack/C123".to_string(),
            prompt: "ok".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(1_700_000_000_000),
        },
    );
    write_event(
        &events_dir.join("invalid-periodic.json"),
        &EventDefinition {
            id: "invalid-periodic".to_string(),
            channel: "github/owner/repo#10".to_string(),
            prompt: "bad schedule".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "0/1 * * * * * *".to_string(),
                timezone: "Not/AZone".to_string(),
            },
            enabled: false,
            created_unix_ms: Some(1_700_000_000_000),
        },
    );
    std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "periodic_last_run_unix_ms": {
"invalid-periodic": 1700000000000
  },
  "debounce_last_seen_unix_ms": {},
  "signature_replay_last_seen_unix_ms": {}
}
"#,
    )
    .expect("write state");

    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir,
            state_path,
        },
        1_700_000_100_000,
    )
    .expect("validate report");

    assert_eq!(report.total_files, 3);
    assert_eq!(report.valid_files, 1);
    assert_eq!(report.invalid_files, 1);
    assert_eq!(report.malformed_files, 1);
    assert_eq!(report.failed_files, 2);
    assert_eq!(report.disabled_files, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.reason_code == "json_parse"));
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.reason_code == "schedule_invalid"));
}

#[test]
fn regression_validate_events_definitions_handles_missing_events_dir() {
    let temp = tempdir().expect("tempdir");
    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir: temp.path().join("missing-events"),
            state_path: temp.path().join("missing-state.json"),
        },
        1_700_000_200_000,
    )
    .expect("validate report");

    assert_eq!(report.total_files, 0);
    assert_eq!(report.valid_files, 0);
    assert_eq!(report.invalid_files, 0);
    assert_eq!(report.malformed_files, 0);
    assert_eq!(report.failed_files, 0);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn unit_events_template_writer_rejects_invalid_periodic_timezone() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("periodic.json");
    let mut config = template_config(&path);
    config.schedule = EventTemplateSchedule::Periodic;
    config.cron = Some("0 0/15 * * * * *".to_string());
    config.timezone = Some("".to_string());

    let error = write_event_template(&config, 1_700_000_000_000)
        .expect_err("empty periodic timezone should fail");
    assert!(error
        .to_string()
        .contains("events template requires timezone for schedule=periodic"));
}

#[test]
fn functional_events_template_writer_writes_immediate_defaults() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("immediate.json");
    let config = template_config(&path);

    write_event_template(&config, 1_700_000_000_000).expect("write template");
    let raw = std::fs::read_to_string(&path).expect("read template");
    let parsed: EventDefinition = serde_json::from_str(&raw).expect("parse template");

    assert_eq!(parsed.id, "template-immediate");
    assert_eq!(parsed.channel, "slack/C123");
    assert!(matches!(parsed.schedule, EventSchedule::Immediate));
    assert!(parsed.enabled);
    assert!(parsed.created_unix_ms.is_some());
}

#[test]
fn integration_events_template_periodic_output_passes_validation_pipeline() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let path = events_dir.join("periodic.json");

    let mut config = template_config(&path);
    config.schedule = EventTemplateSchedule::Periodic;
    config.cron = Some("0 0/10 * * * * *".to_string());
    config.timezone = Some("UTC".to_string());
    config.channel = "github/owner/repo#44".to_string();
    config.event_id = Some("deploy-check".to_string());

    write_event_template(&config, 1_700_000_000_000).expect("write periodic template");
    let report = validate_events_definitions(
        &EventsValidateConfig {
            events_dir,
            state_path: temp.path().join("state.json"),
        },
        super::current_unix_timestamp_ms(),
    )
    .expect("validate");
    assert_eq!(report.total_files, 1);
    assert_eq!(report.valid_files, 1);
    assert_eq!(report.failed_files, 0);
}

#[test]
fn regression_events_template_writer_respects_overwrite_guard() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("template.json");
    std::fs::write(&path, "{\"existing\":true}\n").expect("seed file");

    let config = template_config(&path);
    let error = write_event_template(&config, 1_700_000_000_000)
        .expect_err("existing file should fail without overwrite");
    assert!(error.to_string().contains("template path already exists"));

    let mut overwrite_config = template_config(&path);
    overwrite_config.overwrite = true;
    write_event_template(&overwrite_config, 1_700_000_000_000)
        .expect("overwrite should succeed when enabled");
    let raw = std::fs::read_to_string(path).expect("read overwritten");
    assert!(raw.contains("\"id\": \"template-immediate\""));
}

#[test]
fn unit_simulate_events_classifies_due_and_horizon_rows() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let now = 1_700_000_300_000_u64;

    write_event(
        &events_dir.join("immediate.json"),
        &EventDefinition {
            id: "immediate".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "run".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    write_event(
        &events_dir.join("at-future.json"),
        &EventDefinition {
            id: "at-future".to_string(),
            channel: "slack/C2".to_string(),
            prompt: "later".to_string(),
            schedule: EventSchedule::At {
                at_unix_ms: now.saturating_add(20_000),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );

    let report = simulate_events(
        &EventsSimulateConfig {
            events_dir,
            state_path: temp.path().join("state.json"),
            horizon_seconds: 30,
            stale_immediate_max_age_seconds: 86_400,
        },
        now,
    )
    .expect("simulate report");
    assert_eq!(report.total_files, 2);
    assert_eq!(report.simulated_rows, 2);
    assert_eq!(report.due_now_rows, 1);
    assert_eq!(report.within_horizon_rows, 2);
    assert_eq!(report.invalid_rows, 0);
    assert_eq!(report.malformed_files, 0);
}

#[test]
fn functional_render_events_simulate_report_contains_summary_and_rows() {
    let report = super::EventsSimulateReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/state.json".to_string(),
        now_unix_ms: 123,
        horizon_seconds: 60,
        total_files: 1,
        simulated_rows: 1,
        malformed_files: 0,
        invalid_rows: 0,
        due_now_rows: 1,
        within_horizon_rows: 1,
        rows: vec![super::EventsSimulateRow {
            path: "/tmp/events/a.json".to_string(),
            event_id: "evt-1".to_string(),
            channel: "slack/C1".to_string(),
            schedule: "immediate".to_string(),
            enabled: true,
            next_due_unix_ms: Some(123),
            due_now: true,
            within_horizon: true,
            last_run_unix_ms: None,
        }],
        diagnostics: Vec::new(),
    };

    let rendered = render_events_simulate_report(&report);
    assert!(rendered.contains("events simulate:"));
    assert!(rendered.contains("horizon_seconds=60"));
    assert!(rendered.contains("events simulate row:"));
    assert!(rendered.contains("event_id=evt-1"));
}

#[test]
fn integration_simulate_events_mixed_schedules_with_state_replay() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let now = 1_700_000_400_000_u64;

    write_event(
        &events_dir.join("periodic.json"),
        &EventDefinition {
            id: "periodic".to_string(),
            channel: "github/owner/repo#1".to_string(),
            prompt: "periodic".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "0/1 * * * * * *".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    write_event(
        &events_dir.join("disabled-at.json"),
        &EventDefinition {
            id: "disabled-at".to_string(),
            channel: "slack/C3".to_string(),
            prompt: "disabled".to_string(),
            schedule: EventSchedule::At {
                at_unix_ms: now.saturating_add(120_000),
            },
            enabled: false,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    std::fs::write(
        &state_path,
        r#"{
  "schema_version": 1,
  "periodic_last_run_unix_ms": {
"periodic": 1700000390000
  },
  "debounce_last_seen_unix_ms": {},
  "signature_replay_last_seen_unix_ms": {}
}
"#,
    )
    .expect("write state");

    let report = simulate_events(
        &EventsSimulateConfig {
            events_dir,
            state_path,
            horizon_seconds: 300,
            stale_immediate_max_age_seconds: 86_400,
        },
        now,
    )
    .expect("simulate report");
    assert_eq!(report.total_files, 2);
    assert_eq!(report.simulated_rows, 2);
    assert_eq!(report.malformed_files, 0);
    assert_eq!(report.invalid_rows, 0);
    assert_eq!(report.rows.iter().filter(|row| row.enabled).count(), 1);
}

#[test]
fn regression_simulate_events_reports_malformed_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let now = 1_700_000_500_000_u64;

    write_event(
        &events_dir.join("invalid-channel.json"),
        &EventDefinition {
            id: "invalid-channel".to_string(),
            channel: "slack".to_string(),
            prompt: "bad".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");

    let report = simulate_events(
        &EventsSimulateConfig {
            events_dir,
            state_path: temp.path().join("state.json"),
            horizon_seconds: 60,
            stale_immediate_max_age_seconds: 86_400,
        },
        now,
    )
    .expect("simulate report");

    assert_eq!(report.total_files, 2);
    assert_eq!(report.simulated_rows, 0);
    assert_eq!(report.malformed_files, 1);
    assert_eq!(report.invalid_rows, 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.reason_code == "channel_ref_invalid"));
    assert!(report
        .diagnostics
        .iter()
        .any(|item| item.reason_code == "json_parse"));
}

#[test]
fn unit_dry_run_events_applies_queue_limit_and_decisions() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let now = 1_700_000_600_000_u64;

    write_event(
        &events_dir.join("disabled.json"),
        &EventDefinition {
            id: "a-disabled".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "disabled".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: false,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    write_event(
        &events_dir.join("due-a.json"),
        &EventDefinition {
            id: "b-due".to_string(),
            channel: "slack/C2".to_string(),
            prompt: "due".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    write_event(
        &events_dir.join("due-b.json"),
        &EventDefinition {
            id: "c-due".to_string(),
            channel: "slack/C3".to_string(),
            prompt: "due-too".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );

    let report = dry_run_events(
        &EventsDryRunConfig {
            events_dir,
            state_path: temp.path().join("state.json"),
            queue_limit: 1,
            stale_immediate_max_age_seconds: 86_400,
        },
        now,
    )
    .expect("dry run report");

    assert_eq!(report.total_files, 3);
    assert_eq!(report.evaluated_rows, 3);
    assert_eq!(report.execute_rows, 1);
    assert_eq!(report.skipped_rows, 2);
    assert_eq!(report.error_rows, 0);
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "not_due" && row.decision == "skip"));
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "due_now" && row.decision == "execute"));
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "queue_limit_reached" && row.decision == "skip"));
    assert_eq!(
        report
            .rows
            .iter()
            .find(|row| row.reason_code == "due_now")
            .and_then(|row| row.queue_position),
        Some(1)
    );
}

#[test]
fn functional_render_events_dry_run_report_contains_summary_and_rows() {
    let report = super::EventsDryRunReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/state.json".to_string(),
        now_unix_ms: 123,
        queue_limit: 2,
        total_files: 1,
        evaluated_rows: 1,
        execute_rows: 1,
        skipped_rows: 0,
        error_rows: 0,
        malformed_files: 0,
        rows: vec![super::EventsDryRunRow {
            path: "/tmp/events/a.json".to_string(),
            event_id: Some("evt-1".to_string()),
            channel: Some("slack/C1".to_string()),
            schedule: Some("immediate".to_string()),
            enabled: Some(true),
            decision: "execute".to_string(),
            reason_code: "due_now".to_string(),
            queue_position: Some(1),
            last_run_unix_ms: None,
            message: None,
        }],
    };

    let rendered = render_events_dry_run_report(&report);
    assert!(rendered.contains("events dry run:"));
    assert!(rendered.contains("queue_limit=2"));
    assert!(rendered.contains("events dry run row:"));
    assert!(rendered.contains("decision=execute"));
}

#[test]
fn unit_enforce_events_dry_run_strict_mode_rejects_error_rows() {
    let clean = super::EventsDryRunReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/state.json".to_string(),
        now_unix_ms: 123,
        queue_limit: 2,
        total_files: 0,
        evaluated_rows: 0,
        execute_rows: 0,
        skipped_rows: 0,
        error_rows: 0,
        malformed_files: 0,
        rows: Vec::new(),
    };
    enforce_events_dry_run_strict_mode(&clean, true).expect("strict clean should pass");
    enforce_events_dry_run_strict_mode(&clean, false).expect("non-strict clean should pass");

    let mut failing = clean.clone();
    failing.error_rows = 2;
    failing.malformed_files = 1;
    let error = enforce_events_dry_run_strict_mode(&failing, true)
        .expect_err("strict failing report should error");
    assert!(error
        .to_string()
        .contains("events dry run gate: status=fail"));
    assert!(error.to_string().contains("max_error_rows_exceeded"));
}

#[test]
fn unit_evaluate_events_dry_run_gate_applies_thresholds() {
    let report = super::EventsDryRunReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/state.json".to_string(),
        now_unix_ms: 123,
        queue_limit: 2,
        total_files: 0,
        evaluated_rows: 0,
        execute_rows: 3,
        skipped_rows: 1,
        error_rows: 2,
        malformed_files: 0,
        rows: Vec::new(),
    };
    let outcome = evaluate_events_dry_run_gate(
        &report,
        &EventsDryRunGateConfig {
            max_error_rows: Some(1),
            max_execute_rows: Some(2),
        },
    );
    assert_eq!(outcome.status, "fail");
    assert_eq!(
        outcome.reason_codes,
        vec![
            "max_error_rows_exceeded".to_string(),
            "max_execute_rows_exceeded".to_string()
        ]
    );
}

#[test]
fn functional_render_events_dry_run_gate_summary_is_deterministic() {
    let summary = render_events_dry_run_gate_summary(&super::EventsDryRunGateOutcome {
        status: "pass",
        reason_codes: Vec::new(),
        execute_rows: 1,
        skipped_rows: 2,
        error_rows: 0,
        max_error_rows: Some(0),
        max_execute_rows: Some(5),
    });
    assert_eq!(
        summary,
        "events dry run gate: status=pass reason_codes=none execute_rows=1 skipped_rows=2 error_rows=0 max_error_rows=0 max_execute_rows=5"
    );
}

#[test]
fn regression_enforce_events_dry_run_gate_includes_reason_codes_in_error() {
    let report = super::EventsDryRunReport {
        events_dir: "/tmp/events".to_string(),
        state_path: "/tmp/state.json".to_string(),
        now_unix_ms: 123,
        queue_limit: 2,
        total_files: 0,
        evaluated_rows: 0,
        execute_rows: 4,
        skipped_rows: 0,
        error_rows: 0,
        malformed_files: 0,
        rows: Vec::new(),
    };
    let error = enforce_events_dry_run_gate(
        &report,
        &EventsDryRunGateConfig {
            max_error_rows: None,
            max_execute_rows: Some(2),
        },
    )
    .expect_err("gate should fail");
    let rendered = error.to_string();
    assert!(rendered.contains("events dry run gate: status=fail"));
    assert!(rendered.contains("max_execute_rows_exceeded"));
}

#[test]
fn integration_dry_run_events_is_read_only_for_event_files_and_state() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let now = 1_700_000_700_000_u64;

    let stale_path = events_dir.join("stale.json");
    write_event(
        &stale_path,
        &EventDefinition {
            id: "stale-immediate".to_string(),
            channel: "slack/C1".to_string(),
            prompt: "stale".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(3_600_000)),
        },
    );
    let stale_before = std::fs::read_to_string(&stale_path).expect("read stale before");

    let report = dry_run_events(
        &EventsDryRunConfig {
            events_dir,
            state_path: state_path.clone(),
            queue_limit: 8,
            stale_immediate_max_age_seconds: 60,
        },
        now,
    )
    .expect("dry run report");

    assert!(stale_path.exists());
    let stale_after = std::fs::read_to_string(&stale_path).expect("read stale after");
    assert_eq!(stale_before, stale_after);
    assert!(!state_path.exists());
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "stale_immediate"));
}

#[test]
fn regression_dry_run_events_reports_malformed_and_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let now = 1_700_000_800_000_u64;

    write_event(
        &events_dir.join("invalid-channel.json"),
        &EventDefinition {
            id: "invalid-channel".to_string(),
            channel: "slack".to_string(),
            prompt: "bad channel".to_string(),
            schedule: EventSchedule::Immediate,
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    write_event(
        &events_dir.join("invalid-schedule.json"),
        &EventDefinition {
            id: "invalid-schedule".to_string(),
            channel: "slack/C2".to_string(),
            prompt: "bad schedule".to_string(),
            schedule: EventSchedule::Periodic {
                cron: "not-a-cron".to_string(),
                timezone: "UTC".to_string(),
            },
            enabled: true,
            created_unix_ms: Some(now.saturating_sub(100)),
        },
    );
    std::fs::write(events_dir.join("broken.json"), "{bad-json").expect("write malformed");

    let report = dry_run_events(
        &EventsDryRunConfig {
            events_dir,
            state_path: temp.path().join("state.json"),
            queue_limit: 8,
            stale_immediate_max_age_seconds: 86_400,
        },
        now,
    )
    .expect("dry run report");

    assert_eq!(report.total_files, 3);
    assert_eq!(report.evaluated_rows, 3);
    assert_eq!(report.error_rows, 3);
    assert_eq!(report.malformed_files, 1);
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "json_parse" && row.decision == "error"));
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "channel_ref_invalid" && row.decision == "error"));
    assert!(report
        .rows
        .iter()
        .any(|row| row.reason_code == "schedule_invalid" && row.decision == "error"));
}

#[tokio::test]
async fn integration_scheduled_event_executes_into_channel_store() {
    let temp = tempdir().expect("tempdir");
    let config = scheduler_config(temp.path());
    std::fs::create_dir_all(&config.events_dir).expect("create events dir");

    let event = EventDefinition {
        id: "run-now".to_string(),
        channel: "slack/C123".to_string(),
        prompt: "say hello".to_string(),
        schedule: EventSchedule::Immediate,
        enabled: true,
        created_unix_ms: Some(super::current_unix_timestamp_ms()),
    };
    write_event(&config.events_dir.join("run-now.json"), &event);

    let now = super::current_unix_timestamp_ms();
    let mut runtime = EventSchedulerRuntime::new(config.clone()).expect("runtime");
    let report = runtime.poll_once(now).await.expect("poll once");
    assert_eq!(report.executed, 1);

    let channel_log = std::fs::read_to_string(
        config
            .channel_store_root
            .join("channels/slack/C123/log.jsonl"),
    )
    .expect("channel log");
    let channel_context = std::fs::read_to_string(
        config
            .channel_store_root
            .join("channels/slack/C123/context.jsonl"),
    )
    .expect("channel context");
    assert!(channel_log.contains("\"source\":\"events\""));
    assert!(channel_context.contains("scheduled reply"));
    assert!(!config.events_dir.join("run-now.json").exists());
}

#[tokio::test]
async fn integration_restart_recovery_runs_due_oneshot_and_keeps_periodic() {
    let temp = tempdir().expect("tempdir");
    let config = scheduler_config(temp.path());
    std::fs::create_dir_all(&config.events_dir).expect("create events dir");

    let now = super::current_unix_timestamp_ms();
    let at_event = EventDefinition {
        id: "oneshot".to_string(),
        channel: "github/issue-7".to_string(),
        prompt: "at event".to_string(),
        schedule: EventSchedule::At {
            at_unix_ms: now.saturating_sub(1_000),
        },
        enabled: true,
        created_unix_ms: Some(now.saturating_sub(2_000)),
    };
    let periodic_event = EventDefinition {
        id: "periodic".to_string(),
        channel: "github/issue-7".to_string(),
        prompt: "periodic event".to_string(),
        schedule: EventSchedule::Periodic {
            cron: "0/1 * * * * * *".to_string(),
            timezone: "UTC".to_string(),
        },
        enabled: true,
        created_unix_ms: Some(now.saturating_sub(2_000)),
    };
    write_event(&config.events_dir.join("oneshot.json"), &at_event);
    write_event(&config.events_dir.join("periodic.json"), &periodic_event);

    let mut runtime = EventSchedulerRuntime::new(config.clone()).expect("runtime");
    let first = runtime.poll_once(now).await.expect("first poll");
    assert!(first.executed >= 1);

    let mut runtime_after_restart =
        EventSchedulerRuntime::new(config.clone()).expect("restart runtime");
    let second = runtime_after_restart
        .poll_once(now.saturating_add(2_000))
        .await
        .expect("second poll");
    assert!(second.executed >= 1);
    assert!(!config.events_dir.join("oneshot.json").exists());
    assert!(config.events_dir.join("periodic.json").exists());
}

#[tokio::test]
async fn regression_stale_immediate_events_are_ignored_and_removed() {
    let temp = tempdir().expect("tempdir");
    let mut config = scheduler_config(temp.path());
    config.stale_immediate_max_age_seconds = 1;
    std::fs::create_dir_all(&config.events_dir).expect("create events dir");

    let now = super::current_unix_timestamp_ms();
    let stale = EventDefinition {
        id: "stale-immediate".to_string(),
        channel: "slack/C1".to_string(),
        prompt: "stale".to_string(),
        schedule: EventSchedule::Immediate,
        enabled: true,
        created_unix_ms: Some(now.saturating_sub(10_000)),
    };
    write_event(&config.events_dir.join("stale.json"), &stale);

    let mut runtime = EventSchedulerRuntime::new(config.clone()).expect("runtime");
    let report = runtime.poll_once(now).await.expect("poll");
    assert_eq!(report.executed, 0);
    assert_eq!(report.stale_skipped, 1);
    assert!(!config.events_dir.join("stale.json").exists());
}

#[test]
fn regression_webhook_ingest_debounces_and_writes_immediate_event() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("events/state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, "{\"signal\":\"high\"}").expect("write payload");

    let config = EventWebhookIngestConfig {
        events_dir: events_dir.clone(),
        state_path: state_path.clone(),
        channel_ref: "slack/C123".to_string(),
        payload_file: payload_path.clone(),
        prompt_prefix: "Handle incoming webhook".to_string(),
        debounce_key: Some("hook-A".to_string()),
        debounce_window_seconds: 60,
        signature: None,
        timestamp: None,
        secret: None,
        signature_algorithm: None,
        signature_max_skew_seconds: 300,
    };

    ingest_webhook_immediate_event(&config).expect("first ingest");
    let first_count = std::fs::read_dir(&events_dir)
        .expect("read dir first")
        .count();
    ingest_webhook_immediate_event(&config).expect("second ingest debounced");
    let second_count = std::fs::read_dir(&events_dir)
        .expect("read dir second")
        .count();

    assert_eq!(first_count, second_count);
    assert!(state_path.exists());
}

#[test]
fn unit_webhook_signature_github_sha256_and_slack_v0_are_verified() {
    let payload = "{\"signal\":\"ok\"}";
    let github_secret = "github-secret";
    let github_sig = github_signature(github_secret, payload);
    let github_result = super::verify_webhook_signature(
        payload,
        Some(&github_sig),
        None,
        Some(github_secret),
        Some(WebhookSignatureAlgorithm::GithubSha256),
        1_700_000_000_000,
        300,
    )
    .expect("github signature");
    assert!(github_result.is_some());

    let slack_secret = "slack-secret";
    let slack_ts = "1700000000";
    let slack_sig = slack_v0_signature(slack_secret, slack_ts, payload);
    let slack_result = super::verify_webhook_signature(
        payload,
        Some(&slack_sig),
        Some(slack_ts),
        Some(slack_secret),
        Some(WebhookSignatureAlgorithm::SlackV0),
        1_700_000_050_000,
        600,
    )
    .expect("slack signature");
    assert!(slack_result.is_some());
}

#[test]
fn functional_webhook_ingest_accepts_valid_signed_payload() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let payload_path = temp.path().join("payload.json");
    let payload = "{\"event\":\"deploy\"}";
    std::fs::write(&payload_path, payload).expect("write payload");

    let secret = "github-secret";
    let signature = github_signature(secret, payload);
    let config = EventWebhookIngestConfig {
        events_dir: events_dir.clone(),
        state_path: state_path.clone(),
        channel_ref: "github/owner/repo#10".to_string(),
        payload_file: payload_path,
        prompt_prefix: "Handle incoming webhook".to_string(),
        debounce_key: None,
        debounce_window_seconds: 60,
        signature: Some(signature),
        timestamp: None,
        secret: Some(secret.to_string()),
        signature_algorithm: Some(WebhookSignatureAlgorithm::GithubSha256),
        signature_max_skew_seconds: 300,
    };

    ingest_webhook_immediate_event(&config).expect("signed ingest");
    let count = std::fs::read_dir(&events_dir)
        .expect("read events dir")
        .count();
    assert_eq!(count, 1);
}

#[test]
fn integration_webhook_ingest_rejects_replay_signature_within_window() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let payload_path = temp.path().join("payload.json");
    let payload = "{\"event\":\"sync\"}";
    std::fs::write(&payload_path, payload).expect("write payload");

    let secret = "slack-secret";
    let timestamp = format!("{}", super::current_unix_timestamp_ms() / 1_000);
    let signature = slack_v0_signature(secret, &timestamp, payload);
    let config = EventWebhookIngestConfig {
        events_dir: events_dir.clone(),
        state_path: state_path.clone(),
        channel_ref: "slack/C123".to_string(),
        payload_file: payload_path,
        prompt_prefix: "Handle incoming webhook".to_string(),
        debounce_key: None,
        debounce_window_seconds: 60,
        signature: Some(signature),
        timestamp: Some(timestamp),
        secret: Some(secret.to_string()),
        signature_algorithm: Some(WebhookSignatureAlgorithm::SlackV0),
        signature_max_skew_seconds: 300,
    };

    ingest_webhook_immediate_event(&config).expect("first ingest");
    let error = ingest_webhook_immediate_event(&config).expect_err("replay should fail");
    assert!(error.to_string().contains("replay"));
}

#[test]
fn regression_webhook_ingest_invalid_signature_is_rejected_without_event_write() {
    let temp = tempdir().expect("tempdir");
    let events_dir = temp.path().join("events");
    let state_path = temp.path().join("state.json");
    std::fs::create_dir_all(&events_dir).expect("create events dir");
    let payload_path = temp.path().join("payload.json");
    std::fs::write(&payload_path, "{\"signal\":\"bad\"}").expect("write payload");

    let config = EventWebhookIngestConfig {
        events_dir: events_dir.clone(),
        state_path,
        channel_ref: "slack/C123".to_string(),
        payload_file: payload_path,
        prompt_prefix: "Handle incoming webhook".to_string(),
        debounce_key: None,
        debounce_window_seconds: 60,
        signature: Some("sha256=deadbeef".to_string()),
        timestamp: None,
        secret: Some("secret".to_string()),
        signature_algorithm: Some(WebhookSignatureAlgorithm::GithubSha256),
        signature_max_skew_seconds: 300,
    };

    let error = ingest_webhook_immediate_event(&config).expect_err("signature should fail");
    assert!(error.to_string().contains("verification failed"));
    let count = std::fs::read_dir(&events_dir)
        .expect("read events dir")
        .count();
    assert_eq!(count, 0);
}
