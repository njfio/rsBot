//! Tests for channel-store admin command, rendering, and snapshot workflows.

use std::path::PathBuf;
use std::thread;

use clap::Parser;
use tempfile::tempdir;

use super::{
    build_operator_control_summary_diff_report, collect_custom_command_status_report,
    collect_dashboard_status_report, collect_deployment_status_report,
    collect_gateway_status_report, collect_github_status_report, collect_multi_agent_status_report,
    collect_multi_channel_status_report, collect_operator_control_summary_report,
    collect_transport_health_rows, collect_voice_status_report,
    load_operator_control_summary_snapshot, operator_health_state_rank,
    parse_transport_health_inspect_target, render_custom_command_status_report,
    render_dashboard_status_report, render_deployment_status_report, render_gateway_status_report,
    render_github_status_report, render_multi_agent_status_report,
    render_multi_channel_status_report, render_operator_control_summary_diff_report,
    render_operator_control_summary_report, render_transport_health_row,
    render_transport_health_rows, render_voice_status_report,
    save_operator_control_summary_snapshot, OperatorControlComponentSummaryRow,
    OperatorControlDaemonSummary, OperatorControlPolicyPosture,
    OperatorControlReleaseChannelSummary, OperatorControlSummaryReport, TransportHealthInspectRow,
    TransportHealthInspectTarget,
};
use crate::daemon_runtime::{install_tau_daemon, start_tau_daemon, TauDaemonConfig};
use crate::transport_health::TransportHealthSnapshot;
use crate::transport_health::TransportHealthState;
use tau_cli::{Cli, CliDaemonProfile};
use tau_core::current_unix_timestamp_ms;

fn parse_cli(args: &[&str]) -> Cli {
    let owned_args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    thread::Builder::new()
        .name("tau-cli-parse".to_string())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || Cli::parse_from(owned_args))
        .expect("spawn cli parse thread")
        .join()
        .expect("join cli parse thread")
}

struct OperatorSummaryFixtureInput<'a> {
    generated_unix_ms: u64,
    health_state: &'a str,
    rollout_gate: &'a str,
    reason_code: &'a str,
    recommendation: &'a str,
    component_health_state: &'a str,
    component_rollout_gate: &'a str,
    queue_depth: usize,
    failure_streak: usize,
}

fn operator_summary_fixture(
    input: OperatorSummaryFixtureInput<'_>,
) -> OperatorControlSummaryReport {
    OperatorControlSummaryReport {
        generated_unix_ms: input.generated_unix_ms,
        health_state: input.health_state.to_string(),
        rollout_gate: input.rollout_gate.to_string(),
        reason_codes: vec![input.reason_code.to_string()],
        recommendations: vec![input.recommendation.to_string()],
        policy_posture: OperatorControlPolicyPosture {
            pairing_strict_effective: false,
            pairing_config_strict_mode: false,
            pairing_allowlist_strict: false,
            pairing_rules_configured: false,
            pairing_registry_entries: 0,
            pairing_allowlist_channel_rules: 0,
            provider_subscription_strict: false,
            gateway_auth_mode: "token".to_string(),
            gateway_remote_profile: "local-only".to_string(),
            gateway_remote_posture: "local-only".to_string(),
            gateway_remote_gate: "pass".to_string(),
            gateway_remote_risk_level: "low".to_string(),
            gateway_remote_reason_codes: vec!["local_only_profile".to_string()],
            gateway_remote_recommendations: vec!["no_immediate_action_required".to_string()],
        },
        daemon: OperatorControlDaemonSummary {
            health_state: "healthy".to_string(),
            rollout_gate: "pass".to_string(),
            reason_code: "daemon_running".to_string(),
            recommendation: "no_immediate_action_required".to_string(),
            profile: "none".to_string(),
            installed: false,
            running: false,
            start_attempts: 0,
            stop_attempts: 0,
            diagnostics: 0,
            state_path: ".tau/daemon/state.json".to_string(),
        },
        release_channel: OperatorControlReleaseChannelSummary {
            health_state: "healthy".to_string(),
            rollout_gate: "pass".to_string(),
            reason_code: "configured".to_string(),
            recommendation: "no_immediate_action_required".to_string(),
            configured: true,
            channel: "stable".to_string(),
            path: ".tau/release-channel.json".to_string(),
        },
        components: vec![OperatorControlComponentSummaryRow {
            component: "gateway".to_string(),
            state_path: ".tau/gateway/state.json".to_string(),
            health_state: input.component_health_state.to_string(),
            health_reason: "fixture".to_string(),
            rollout_gate: input.component_rollout_gate.to_string(),
            reason_code: input.reason_code.to_string(),
            recommendation: input.recommendation.to_string(),
            queue_depth: input.queue_depth,
            failure_streak: input.failure_streak,
        }],
    }
}

#[test]
fn unit_parse_transport_health_inspect_target_accepts_supported_values() {
    assert_eq!(
        parse_transport_health_inspect_target("slack").expect("slack"),
        TransportHealthInspectTarget::Slack
    );
    assert_eq!(
        parse_transport_health_inspect_target("github").expect("github"),
        TransportHealthInspectTarget::GithubAll
    );
    assert_eq!(
        parse_transport_health_inspect_target("github:owner/repo").expect("github owner/repo"),
        TransportHealthInspectTarget::GithubRepo {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
        }
    );
    assert_eq!(
        parse_transport_health_inspect_target("multi-channel").expect("multi-channel"),
        TransportHealthInspectTarget::MultiChannel
    );
    assert_eq!(
        parse_transport_health_inspect_target("multichannel").expect("multichannel"),
        TransportHealthInspectTarget::MultiChannel
    );
    assert_eq!(
        parse_transport_health_inspect_target("multi-agent").expect("multi-agent"),
        TransportHealthInspectTarget::MultiAgent
    );
    assert_eq!(
        parse_transport_health_inspect_target("multiagent").expect("multiagent"),
        TransportHealthInspectTarget::MultiAgent
    );
    assert_eq!(
        parse_transport_health_inspect_target("browser-automation").expect("browser-automation"),
        TransportHealthInspectTarget::BrowserAutomation
    );
    assert_eq!(
        parse_transport_health_inspect_target("browserautomation").expect("browserautomation"),
        TransportHealthInspectTarget::BrowserAutomation
    );
    assert_eq!(
        parse_transport_health_inspect_target("browser").expect("browser"),
        TransportHealthInspectTarget::BrowserAutomation
    );
    assert_eq!(
        parse_transport_health_inspect_target("memory").expect("memory"),
        TransportHealthInspectTarget::Memory
    );
    assert_eq!(
        parse_transport_health_inspect_target("dashboard").expect("dashboard"),
        TransportHealthInspectTarget::Dashboard
    );
    assert_eq!(
        parse_transport_health_inspect_target("gateway").expect("gateway"),
        TransportHealthInspectTarget::Gateway
    );
    assert_eq!(
        parse_transport_health_inspect_target("deployment").expect("deployment"),
        TransportHealthInspectTarget::Deployment
    );
    assert_eq!(
        parse_transport_health_inspect_target("custom-command").expect("custom-command"),
        TransportHealthInspectTarget::CustomCommand
    );
    assert_eq!(
        parse_transport_health_inspect_target("customcommand").expect("customcommand"),
        TransportHealthInspectTarget::CustomCommand
    );
    assert_eq!(
        parse_transport_health_inspect_target("voice").expect("voice"),
        TransportHealthInspectTarget::Voice
    );
}

#[test]
fn unit_render_transport_health_row_formats_expected_fields() {
    let row = TransportHealthInspectRow {
        transport: "github".to_string(),
        target: "owner/repo".to_string(),
        state_path: "/tmp/state.json".to_string(),
        health: TransportHealthSnapshot {
            updated_unix_ms: 123,
            cycle_duration_ms: 88,
            queue_depth: 3,
            active_runs: 1,
            failure_streak: 0,
            last_cycle_discovered: 4,
            last_cycle_processed: 3,
            last_cycle_completed: 2,
            last_cycle_failed: 1,
            last_cycle_duplicates: 1,
        },
    };
    let rendered = render_transport_health_row(&row);
    assert!(rendered.contains("transport=github"));
    assert!(rendered.contains("target=owner/repo"));
    assert!(rendered.contains("cycle_duration_ms=88"));
    assert!(rendered.contains("last_cycle_failed=1"));
}

#[test]
fn functional_collect_transport_health_rows_reads_github_and_slack_states() {
    let temp = tempdir().expect("tempdir");
    let github_root = temp.path().join("github");
    let slack_root = temp.path().join("slack");
    let multi_channel_root = temp.path().join("multi-channel");
    let multi_agent_root = temp.path().join("multi-agent");
    let browser_automation_root = temp.path().join("browser-automation");
    let memory_root = temp.path().join("memory");
    let dashboard_root = temp.path().join("dashboard");
    let gateway_root = temp.path().join("gateway");
    let deployment_root = temp.path().join("deployment");
    let custom_command_root = temp.path().join("custom-command");
    let voice_root = temp.path().join("voice");
    let github_repo_dir = github_root.join("owner__repo");
    std::fs::create_dir_all(&github_repo_dir).expect("create github repo dir");
    std::fs::create_dir_all(&slack_root).expect("create slack dir");
    std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
    std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
    std::fs::create_dir_all(&browser_automation_root).expect("create browser-automation dir");
    std::fs::create_dir_all(&memory_root).expect("create memory dir");
    std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
    std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
    std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
    std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
    std::fs::create_dir_all(&voice_root).expect("create voice dir");

    std::fs::write(
        github_repo_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": [],
  "issue_sessions": {},
  "health": {
    "updated_unix_ms": 100,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 1,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write github state");

    std::fs::write(
        slack_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 200,
    "cycle_duration_ms": 50,
    "queue_depth": 2,
    "active_runs": 1,
    "failure_streak": 1,
    "last_cycle_discovered": 4,
    "last_cycle_processed": 3,
    "last_cycle_completed": 1,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 1
  }
}
"#,
    )
    .expect("write slack state");

    std::fs::write(
        multi_channel_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 300,
    "cycle_duration_ms": 15,
    "queue_depth": 1,
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
        multi_agent_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "routed_cases": [],
  "health": {
    "updated_unix_ms": 350,
    "cycle_duration_ms": 19,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-agent state");

    std::fs::write(
        browser_automation_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "health": {
    "updated_unix_ms": 375,
    "cycle_duration_ms": 28,
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
    .expect("write browser-automation state");

    std::fs::write(
        memory_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "entries": [],
  "health": {
    "updated_unix_ms": 400,
    "cycle_duration_ms": 32,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 5,
    "last_cycle_processed": 5,
    "last_cycle_completed": 5,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 1
  }
}
"#,
    )
    .expect("write memory state");

    std::fs::write(
        dashboard_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "widget_views": [],
  "control_audit": [],
  "health": {
    "updated_unix_ms": 500,
    "cycle_duration_ms": 40,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write dashboard state");

    std::fs::write(
        gateway_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 550,
    "cycle_duration_ms": 21,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write gateway state");

    std::fs::write(
        deployment_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 560,
    "cycle_duration_ms": 17,
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
    .expect("write deployment state");

    std::fs::write(
        custom_command_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": [],
  "health": {
    "updated_unix_ms": 580,
    "cycle_duration_ms": 23,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 4,
    "last_cycle_processed": 4,
    "last_cycle_completed": 4,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write custom-command state");

    std::fs::write(
        voice_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": [],
  "health": {
    "updated_unix_ms": 590,
    "cycle_duration_ms": 18,
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
    .expect("write voice state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.github_state_dir = github_root;
    cli.slack_state_dir = slack_root;
    cli.multi_channel_state_dir = multi_channel_root;
    cli.multi_agent_state_dir = multi_agent_root;
    cli.browser_automation_state_dir = browser_automation_root;
    cli.memory_state_dir = memory_root;
    cli.dashboard_state_dir = dashboard_root;
    cli.gateway_state_dir = gateway_root;
    cli.deployment_state_dir = deployment_root;
    cli.custom_command_state_dir = custom_command_root;
    cli.voice_state_dir = voice_root;

    let github_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::GithubAll)
        .expect("collect github rows");
    assert_eq!(github_rows.len(), 1);
    assert_eq!(github_rows[0].transport, "github");
    assert_eq!(github_rows[0].target, "owner/repo");
    assert_eq!(github_rows[0].health.last_cycle_processed, 2);

    let slack_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Slack)
        .expect("collect slack rows");
    assert_eq!(slack_rows.len(), 1);
    assert_eq!(slack_rows[0].transport, "slack");
    assert_eq!(slack_rows[0].health.queue_depth, 2);

    let multi_channel_rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::MultiChannel)
            .expect("collect multi-channel rows");
    assert_eq!(multi_channel_rows.len(), 1);
    assert_eq!(multi_channel_rows[0].transport, "multi-channel");
    assert_eq!(multi_channel_rows[0].target, "telegram/discord/whatsapp");
    assert_eq!(multi_channel_rows[0].health.last_cycle_discovered, 3);

    let multi_agent_rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::MultiAgent)
            .expect("collect multi-agent rows");
    assert_eq!(multi_agent_rows.len(), 1);
    assert_eq!(multi_agent_rows[0].transport, "multi-agent");
    assert_eq!(multi_agent_rows[0].target, "orchestrator-router");
    assert_eq!(multi_agent_rows[0].health.last_cycle_discovered, 2);

    let browser_automation_rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::BrowserAutomation)
            .expect("collect browser-automation rows");
    assert_eq!(browser_automation_rows.len(), 1);
    assert_eq!(browser_automation_rows[0].transport, "browser-automation");
    assert_eq!(browser_automation_rows[0].target, "fixture-runtime");
    assert_eq!(browser_automation_rows[0].health.last_cycle_discovered, 1);

    let memory_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Memory)
        .expect("collect memory rows");
    assert_eq!(memory_rows.len(), 1);
    assert_eq!(memory_rows[0].transport, "memory");
    assert_eq!(memory_rows[0].target, "semantic-memory");
    assert_eq!(memory_rows[0].health.last_cycle_discovered, 5);

    let dashboard_rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Dashboard)
            .expect("collect dashboard rows");
    assert_eq!(dashboard_rows.len(), 1);
    assert_eq!(dashboard_rows[0].transport, "dashboard");
    assert_eq!(dashboard_rows[0].target, "operator-control-plane");
    assert_eq!(dashboard_rows[0].health.last_cycle_discovered, 2);

    let gateway_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Gateway)
        .expect("collect gateway rows");
    assert_eq!(gateway_rows.len(), 1);
    assert_eq!(gateway_rows[0].transport, "gateway");
    assert_eq!(gateway_rows[0].target, "gateway-service");
    assert_eq!(gateway_rows[0].health.last_cycle_discovered, 2);

    let deployment_rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Deployment)
            .expect("collect deployment rows");
    assert_eq!(deployment_rows.len(), 1);
    assert_eq!(deployment_rows[0].transport, "deployment");
    assert_eq!(deployment_rows[0].target, "cloud-and-wasm-runtime");
    assert_eq!(deployment_rows[0].health.last_cycle_discovered, 3);

    let custom_command_rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::CustomCommand)
            .expect("collect custom-command rows");
    assert_eq!(custom_command_rows.len(), 1);
    assert_eq!(custom_command_rows[0].transport, "custom-command");
    assert_eq!(custom_command_rows[0].target, "no-code-command-registry");
    assert_eq!(custom_command_rows[0].health.last_cycle_discovered, 4);

    let voice_rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Voice)
        .expect("collect voice rows");
    assert_eq!(voice_rows.len(), 1);
    assert_eq!(voice_rows[0].transport, "voice");
    assert_eq!(voice_rows[0].target, "wake-word-pipeline");
    assert_eq!(voice_rows[0].health.last_cycle_discovered, 3);

    let rendered = render_transport_health_rows(&[
        github_rows[0].clone(),
        slack_rows[0].clone(),
        multi_channel_rows[0].clone(),
        multi_agent_rows[0].clone(),
        browser_automation_rows[0].clone(),
        memory_rows[0].clone(),
        dashboard_rows[0].clone(),
        gateway_rows[0].clone(),
        deployment_rows[0].clone(),
        custom_command_rows[0].clone(),
        voice_rows[0].clone(),
    ]);
    assert!(rendered.contains("transport=github"));
    assert!(rendered.contains("transport=slack"));
    assert!(rendered.contains("transport=multi-channel"));
    assert!(rendered.contains("transport=multi-agent"));
    assert!(rendered.contains("transport=browser-automation"));
    assert!(rendered.contains("transport=memory"));
    assert!(rendered.contains("transport=dashboard"));
    assert!(rendered.contains("transport=gateway"));
    assert!(rendered.contains("transport=deployment"));
    assert!(rendered.contains("transport=custom-command"));
    assert!(rendered.contains("transport=voice"));
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields() {
    let temp = tempdir().expect("tempdir");
    let github_root = temp.path().join("github");
    let github_repo_dir = github_root.join("owner__repo");
    std::fs::create_dir_all(&github_repo_dir).expect("create github repo dir");
    std::fs::write(
        github_repo_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": null,
  "processed_event_keys": [],
  "issue_sessions": {}
}
"#,
    )
    .expect("write legacy github state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.github_state_dir = PathBuf::from(&github_root);

    let rows = collect_transport_health_rows(
        &cli,
        &TransportHealthInspectTarget::GithubRepo {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
        },
    )
    .expect("collect legacy row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_memory() {
    let temp = tempdir().expect("tempdir");
    let memory_root = temp.path().join("memory");
    std::fs::create_dir_all(&memory_root).expect("create memory dir");
    std::fs::write(
        memory_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "entries": []
}
"#,
    )
    .expect("write legacy memory state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.memory_state_dir = PathBuf::from(&memory_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Memory)
        .expect("collect memory row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_multi_agent() {
    let temp = tempdir().expect("tempdir");
    let multi_agent_root = temp.path().join("multi-agent");
    std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
    std::fs::write(
        multi_agent_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "routed_cases": []
}
"#,
    )
    .expect("write legacy multi-agent state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.multi_agent_state_dir = PathBuf::from(&multi_agent_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::MultiAgent)
        .expect("collect multi-agent row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_browser_automation()
{
    let temp = tempdir().expect("tempdir");
    let browser_automation_root = temp.path().join("browser-automation");
    std::fs::create_dir_all(&browser_automation_root).expect("create browser-automation dir");
    std::fs::write(
        browser_automation_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": []
}
"#,
    )
    .expect("write legacy browser-automation state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.browser_automation_state_dir = PathBuf::from(&browser_automation_root);

    let rows =
        collect_transport_health_rows(&cli, &TransportHealthInspectTarget::BrowserAutomation)
            .expect("collect browser-automation row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_dashboard() {
    let temp = tempdir().expect("tempdir");
    let dashboard_root = temp.path().join("dashboard");
    std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
    std::fs::write(
        dashboard_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "widget_views": [],
  "control_audit": []
}
"#,
    )
    .expect("write legacy dashboard state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.dashboard_state_dir = PathBuf::from(&dashboard_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Dashboard)
        .expect("collect dashboard row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_gateway() {
    let temp = tempdir().expect("tempdir");
    let gateway_root = temp.path().join("gateway");
    std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
    std::fs::write(
        gateway_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": []
}
"#,
    )
    .expect("write legacy gateway state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.gateway_state_dir = PathBuf::from(&gateway_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Gateway)
        .expect("collect gateway row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_custom_command() {
    let temp = tempdir().expect("tempdir");
    let custom_command_root = temp.path().join("custom-command");
    std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
    std::fs::write(
        custom_command_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": []
}
"#,
    )
    .expect("write legacy custom-command state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.custom_command_state_dir = PathBuf::from(&custom_command_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::CustomCommand)
        .expect("collect custom-command row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_deployment() {
    let temp = tempdir().expect("tempdir");
    let deployment_root = temp.path().join("deployment");
    std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
    std::fs::write(
        deployment_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": []
}
"#,
    )
    .expect("write legacy deployment state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.deployment_state_dir = PathBuf::from(&deployment_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Deployment)
        .expect("collect deployment row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn regression_collect_transport_health_rows_defaults_missing_health_fields_for_voice() {
    let temp = tempdir().expect("tempdir");
    let voice_root = temp.path().join("voice");
    std::fs::create_dir_all(&voice_root).expect("create voice dir");
    std::fs::write(
        voice_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": []
}
"#,
    )
    .expect("write legacy voice state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.voice_state_dir = PathBuf::from(&voice_root);

    let rows = collect_transport_health_rows(&cli, &TransportHealthInspectTarget::Voice)
        .expect("collect voice row");
    assert_eq!(rows[0].health, TransportHealthSnapshot::default());
}

#[test]
fn functional_collect_github_status_report_reads_state_and_logs() {
    let temp = tempdir().expect("tempdir");
    let github_root = temp.path().join("github");
    let repo_root = github_root.join("owner__repo");
    std::fs::create_dir_all(&repo_root).expect("create github repo dir");

    std::fs::write(
        repo_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "last_issue_scan_at": "2026-01-01T00:00:00Z",
  "processed_event_keys": ["issue-comment-created:100","issue-comment-created:101"],
  "issue_sessions": {
    "7": {
      "session_id": "issue-7",
      "last_comment_id": 901,
      "last_run_id": "run-7",
      "active_run_id": null,
      "last_event_key": "issue-comment-created:101",
      "last_event_kind": "issue_comment_created",
      "last_actor_login": "alice",
      "last_reason_code": "command_processed",
      "last_processed_unix_ms": 1704067200000,
      "total_processed_events": 4,
      "total_duplicate_events": 1,
      "total_failed_events": 1,
      "total_denied_events": 1,
      "total_runs_started": 3,
      "total_runs_completed": 3,
      "total_runs_failed": 1
    }
  },
  "health": {
    "updated_unix_ms": 200,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 3,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 1
  }
}
"#,
    )
    .expect("write state");
    std::fs::write(
        repo_root.join("inbound-events.jsonl"),
        r#"{"kind":"issue_comment_created","event_key":"issue-comment-created:100"}
{"kind":"issue_comment_edited","event_key":"issue-comment-edited:101"}
{"kind":"issue_comment_created"}
"#,
    )
    .expect("write inbound");
    std::fs::write(
            repo_root.join("outbound-events.jsonl"),
            r#"{"event_key":"issue-comment-created:100","command":"chat-status","status":"reported","reason_code":"command_processed"}
{"event_key":"issue-comment-created:101","command":"chat-replay","status":"reported","reason_code":"command_processed"}
{"event_key":"issue-comment-created:101","command":"authorization","status":"denied","reason_code":"pairing_denied"}
"#,
        )
        .expect("write outbound");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.github_state_dir = github_root;
    let report =
        collect_github_status_report(&cli, "owner/repo").expect("collect github status report");
    assert_eq!(report.repo, "owner/repo");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_event_count, 2);
    assert_eq!(report.issue_session_count, 1);
    assert_eq!(report.inbound_records, 3);
    assert_eq!(report.outbound_records, 3);
    assert_eq!(report.outbound_command_counts.get("chat-status"), Some(&1));
    assert_eq!(report.outbound_command_counts.get("chat-replay"), Some(&1));
    assert_eq!(report.outbound_status_counts.get("reported"), Some(&2));
    assert_eq!(
        report.outbound_reason_code_counts.get("command_processed"),
        Some(&2)
    );
    assert_eq!(
        report.outbound_reason_code_counts.get("pairing_denied"),
        Some(&1)
    );
    assert_eq!(
        report.outbound_last_event_key.as_deref(),
        Some("issue-comment-created:101")
    );
    let rendered = render_github_status_report(&report);
    assert!(rendered.contains("github status inspect: repo=owner/repo"));
    assert!(
        rendered.contains("outbound_command_counts=authorization:1,chat-replay:1,chat-status:1")
    );
    assert!(rendered.contains("outbound_reason_code_counts=command_processed:2,pairing_denied:1"));
}

#[test]
fn regression_collect_github_status_report_handles_missing_logs() {
    let temp = tempdir().expect("tempdir");
    let github_root = temp.path().join("github");
    let repo_root = github_root.join("owner__repo");
    std::fs::create_dir_all(&repo_root).expect("create github repo dir");
    std::fs::write(
        repo_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "issue_sessions": {},
  "health": {
    "updated_unix_ms": 0,
    "cycle_duration_ms": 0,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 0,
    "last_cycle_processed": 0,
    "last_cycle_completed": 0,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.github_state_dir = github_root;
    let report =
        collect_github_status_report(&cli, "owner/repo").expect("collect github status report");
    assert!(!report.inbound_log_present);
    assert!(!report.outbound_log_present);
    assert_eq!(report.inbound_records, 0);
    assert_eq!(report.outbound_records, 0);
    assert!(report.outbound_reason_code_counts.is_empty());
    assert!(report.outbound_last_reason_codes.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
}

#[test]
fn functional_collect_dashboard_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let dashboard_root = temp.path().join("dashboard");
    std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
    std::fs::write(
        dashboard_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["snapshot:s1", "control:c1"],
  "widget_views": [{"widget_id":"health-summary"}],
  "control_audit": [{"case_id":"c1"}],
  "health": {
    "updated_unix_ms": 600,
    "cycle_duration_ms": 25,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write dashboard state");
    std::fs::write(
            dashboard_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["widget_views_updated"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["widget_views_updated","control_actions_applied"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write runtime events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.dashboard_state_dir = dashboard_root;

    let report = collect_dashboard_status_report(&cli).expect("collect status report");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_case_count, 2);
    assert_eq!(report.widget_count, 1);
    assert_eq!(report.control_audit_count, 1);
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(
        report.last_reason_codes,
        vec![
            "widget_views_updated".to_string(),
            "control_actions_applied".to_string()
        ]
    );
    let rendered = render_dashboard_status_report(&report);
    assert!(rendered.contains("dashboard status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("last_reason_codes=widget_views_updated,control_actions_applied"));
}

#[test]
fn regression_collect_dashboard_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let dashboard_root = temp.path().join("dashboard");
    std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
    std::fs::write(
        dashboard_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "widget_views": [],
  "control_audit": [],
  "health": {
    "updated_unix_ms": 700,
    "cycle_duration_ms": 32,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 1,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write dashboard state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.dashboard_state_dir = dashboard_root;

    let report = collect_dashboard_status_report(&cli).expect("collect status report");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
}

#[test]
fn functional_collect_multi_channel_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let multi_channel_root = temp.path().join("multi-channel");
    std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
    std::fs::write(
        multi_channel_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1", "discord:dc-1", "whatsapp:wa-1", "telegram:tg-2"],
  "health": {
    "updated_unix_ms": 735,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 4,
    "last_cycle_processed": 4,
    "last_cycle_completed": 4,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  },
  "telemetry": {
    "typing_events_emitted": 6,
    "presence_events_emitted": 6,
    "usage_summary_records": 4,
    "usage_response_chars": 222,
    "usage_chunks": 7,
    "usage_estimated_cost_micros": 1200,
    "typing_events_by_transport": {"telegram": 4, "discord": 2},
    "presence_events_by_transport": {"telegram": 4, "discord": 2},
    "usage_summary_records_by_transport": {"telegram": 2, "discord": 1, "whatsapp": 1},
    "usage_response_chars_by_transport": {"telegram": 120, "discord": 60, "whatsapp": 42},
    "usage_chunks_by_transport": {"telegram": 3, "discord": 2, "whatsapp": 2},
    "usage_estimated_cost_micros_by_transport": {"telegram": 700, "discord": 300, "whatsapp": 200}
  },
  "telemetry_policy": {
    "typing_presence_enabled": true,
    "usage_summary_enabled": true,
    "include_identifiers": false,
    "typing_presence_min_response_chars": 90
  }
}
"#,
    )
    .expect("write multi-channel state");
    std::fs::write(
            multi_channel_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["healthy_cycle","events_applied"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_events_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write multi-channel events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.multi_channel_state_dir = multi_channel_root;

    let report = collect_multi_channel_status_report(&cli).expect("collect multi-channel status");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_event_count, 4);
    assert_eq!(report.transport_counts.get("telegram"), Some(&2));
    assert_eq!(report.transport_counts.get("discord"), Some(&1));
    assert_eq!(report.transport_counts.get("whatsapp"), Some(&1));
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(
        report.last_reason_codes,
        vec![
            "healthy_cycle".to_string(),
            "duplicate_events_skipped".to_string()
        ]
    );
    assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&2));
    assert_eq!(report.reason_code_counts.get("events_applied"), Some(&1));
    assert_eq!(
        report.reason_code_counts.get("duplicate_events_skipped"),
        Some(&1)
    );
    assert_eq!(report.telemetry.typing_events_emitted, 6);
    assert_eq!(report.telemetry.presence_events_emitted, 6);
    assert_eq!(report.telemetry.usage_summary_records, 4);
    assert_eq!(report.telemetry.usage_response_chars, 222);
    assert_eq!(report.telemetry.usage_chunks, 7);
    assert_eq!(report.telemetry.usage_estimated_cost_micros, 1200);
    assert_eq!(
        report.telemetry.typing_events_by_transport.get("telegram"),
        Some(&4)
    );
    assert_eq!(
        report
            .telemetry
            .usage_estimated_cost_micros_by_transport
            .get("discord"),
        Some(&300)
    );
    assert!(report.telemetry.policy.typing_presence_enabled);
    assert!(report.telemetry.policy.usage_summary_enabled);
    assert!(!report.telemetry.policy.include_identifiers);
    assert_eq!(
        report.telemetry.policy.typing_presence_min_response_chars,
        90
    );

    let rendered = render_multi_channel_status_report(&report);
    assert!(rendered.contains("multi-channel status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("processed_event_count=4"));
    assert!(rendered.contains("transport_counts=discord:1,telegram:2,whatsapp:1"));
    assert!(rendered.contains("typing_events=6"));
    assert!(rendered.contains("usage_records=4"));
    assert!(
        rendered.contains("usage_cost_micros_by_transport=discord:300,telegram:700,whatsapp:200")
    );
    assert!(rendered.contains("policy=typing_presence:true|usage_summary:true|include_identifiers:false|min_response_chars:90"));
    assert!(rendered.contains(
        "reason_code_counts=duplicate_events_skipped:1,events_applied:1,healthy_cycle:2"
    ));
}

#[test]
fn functional_render_multi_channel_status_report_includes_connector_breaker_guidance() {
    let temp = tempdir().expect("tempdir");
    let multi_channel_root = temp.path().join("multi-channel");
    std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
    std::fs::write(
        multi_channel_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1"],
  "health": {
    "updated_unix_ms": 910,
    "cycle_duration_ms": 9,
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
    .expect("write multi-channel state");
    std::fs::write(
            multi_channel_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["healthy_cycle"],"health_reason":"no recent transport failures observed"}"#,
        )
        .expect("write multi-channel events");

    let connectors_state_path = temp.path().join("connectors-state.json");
    let breaker_open_until_unix_ms = current_unix_timestamp_ms().saturating_add(60_000);
    let connectors_state = format!(
        r#"{{
  "schema_version": 1,
  "processed_event_keys": ["telegram:tg-1"],
  "channels": {{
    "telegram": {{
      "mode": "polling",
      "liveness": "open",
      "events_ingested": 3,
      "duplicates_skipped": 1,
      "retry_budget_remaining": 0,
      "breaker_state": "open",
      "breaker_open_until_unix_ms": {breaker_open_until_unix_ms},
      "breaker_last_open_reason": "provider_unavailable",
      "breaker_open_count": 2
    }}
  }}
}}"#
    );
    std::fs::write(&connectors_state_path, connectors_state).expect("write connector state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.multi_channel_state_dir = multi_channel_root;
    cli.multi_channel_live_connectors_state_path = connectors_state_path;

    let report = collect_multi_channel_status_report(&cli).expect("collect multi-channel status");
    let rendered = render_multi_channel_status_report(&report);
    assert!(rendered.contains("connectors=state_present=true processed_event_count=1"));
    assert!(rendered.contains("channels=telegram:polling:open:open:3:1:0:"));
    assert!(rendered.contains(":provider_unavailable:2:wait_for_breaker_recovery_until:"));
}

#[test]
fn regression_collect_multi_channel_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let multi_channel_root = temp.path().join("multi-channel");
    std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
    std::fs::write(
        multi_channel_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 736,
    "cycle_duration_ms": 19,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-channel state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.multi_channel_state_dir = multi_channel_root;

    let report = collect_multi_channel_status_report(&cli).expect("collect multi-channel status");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert!(report.reason_code_counts.is_empty());
    assert_eq!(report.telemetry.typing_events_emitted, 0);
    assert_eq!(report.telemetry.presence_events_emitted, 0);
    assert_eq!(report.telemetry.usage_summary_records, 0);
    assert!(report.telemetry.policy.typing_presence_enabled);
    assert!(report.telemetry.policy.usage_summary_enabled);
    assert!(!report.telemetry.policy.include_identifiers);
    assert_eq!(
        report.telemetry.policy.typing_presence_min_response_chars,
        120
    );
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
}

#[test]
fn functional_collect_multi_agent_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let multi_agent_root = temp.path().join("multi-agent");
    std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
    std::fs::write(
        multi_agent_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["planner:planner-success", "review:review-success"],
  "routed_cases": [
    {
      "case_key": "planner:planner-success",
      "case_id": "planner-success",
      "phase": "planner",
      "selected_role": "planner",
      "attempted_roles": ["planner"],
      "category": "planning",
      "updated_unix_ms": 1
    },
    {
      "case_key": "review:review-success",
      "case_id": "review-success",
      "phase": "review",
      "selected_role": "reviewer",
      "attempted_roles": ["reviewer"],
      "category": "review",
      "updated_unix_ms": 2
    }
  ],
  "health": {
    "updated_unix_ms": 710,
    "cycle_duration_ms": 17,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-agent state");
    std::fs::write(
            multi_agent_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["routed_cases_updated","retry_attempted"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["routed_cases_updated"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write multi-agent events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.multi_agent_state_dir = multi_agent_root;

    let report = collect_multi_agent_status_report(&cli).expect("collect status report");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_case_count, 2);
    assert_eq!(report.routed_case_count, 2);
    assert_eq!(report.phase_counts.get("planner"), Some(&1));
    assert_eq!(report.phase_counts.get("review"), Some(&1));
    assert_eq!(report.selected_role_counts.get("planner"), Some(&1));
    assert_eq!(report.selected_role_counts.get("reviewer"), Some(&1));
    assert_eq!(report.category_counts.get("planning"), Some(&1));
    assert_eq!(report.category_counts.get("review"), Some(&1));
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(
        report.last_reason_codes,
        vec!["routed_cases_updated".to_string()]
    );
    assert_eq!(
        report.reason_code_counts.get("routed_cases_updated"),
        Some(&2)
    );
    assert_eq!(report.reason_code_counts.get("retry_attempted"), Some(&1));
    let rendered = render_multi_agent_status_report(&report);
    assert!(rendered.contains("multi-agent status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("phase_counts=planner:1,review:1"));
    assert!(rendered.contains("reason_code_counts=retry_attempted:1,routed_cases_updated:2"));
}

#[test]
fn regression_collect_multi_agent_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let multi_agent_root = temp.path().join("multi-agent");
    std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
    std::fs::write(
        multi_agent_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "routed_cases": [],
  "health": {
    "updated_unix_ms": 711,
    "cycle_duration_ms": 22,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write multi-agent state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.multi_agent_state_dir = multi_agent_root;

    let report = collect_multi_agent_status_report(&cli).expect("collect status report");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert!(report.reason_code_counts.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
}

#[test]
fn functional_collect_gateway_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let gateway_root = temp.path().join("gateway");
    std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
    std::fs::write(
        gateway_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["POST:/v1/tasks:gateway-success", "GET:/v1/tasks/42:gateway-retryable"],
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
    },
    {
      "case_key": "GET:/v1/tasks/42:gateway-retryable",
      "case_id": "gateway-retryable",
      "method": "get",
      "endpoint": "/v1/tasks/42",
      "actor_id": "ops-bot",
      "status_code": 503,
      "outcome": "retryable_failure",
      "error_code": "gateway_backend_unavailable",
      "response_body": {"status":"retryable"},
      "updated_unix_ms": 2
    }
  ],
  "health": {
    "updated_unix_ms": 740,
    "cycle_duration_ms": 18,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  },
  "guardrail": {
    "gate": "pass",
    "reason_code": "guardrail_checks_passing",
    "failure_streak_threshold": 2,
    "retryable_failures_threshold": 2,
    "failure_streak": 0,
    "last_failed_cases": 0,
    "last_retryable_failures": 1,
    "updated_unix_ms": 750
  }
}
"#,
    )
    .expect("write gateway state");
    std::fs::write(
            gateway_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["healthy_cycle","retry_attempted"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_cases_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write gateway events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.gateway_state_dir = gateway_root;

    let report = collect_gateway_status_report(&cli).expect("collect gateway status report");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.rollout_reason_code, "guardrail_checks_passing");
    assert_eq!(report.service_status, "running");
    assert_eq!(report.guardrail_gate, "pass");
    assert_eq!(report.guardrail_reason_code, "guardrail_checks_passing");
    assert_eq!(report.processed_case_count, 2);
    assert_eq!(report.request_count, 2);
    assert_eq!(report.method_counts.get("GET"), Some(&1));
    assert_eq!(report.method_counts.get("POST"), Some(&1));
    assert_eq!(report.status_code_counts.get("201"), Some(&1));
    assert_eq!(report.status_code_counts.get("503"), Some(&1));
    assert_eq!(report.guardrail_failure_streak_threshold, 2);
    assert_eq!(report.guardrail_retryable_failures_threshold, 2);
    assert_eq!(report.guardrail_failure_streak, 0);
    assert_eq!(report.guardrail_last_failed_cases, 0);
    assert_eq!(report.guardrail_last_retryable_failures, 1);
    assert_eq!(
        report.error_code_counts.get("gateway_backend_unavailable"),
        Some(&1)
    );
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(
        report.last_reason_codes,
        vec![
            "healthy_cycle".to_string(),
            "duplicate_cases_skipped".to_string()
        ]
    );
    assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&2));
    assert_eq!(report.reason_code_counts.get("retry_attempted"), Some(&1));
    assert_eq!(
        report.reason_code_counts.get("duplicate_cases_skipped"),
        Some(&1)
    );

    let rendered = render_gateway_status_report(&report);
    assert!(rendered.contains("gateway status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("rollout_reason_code=guardrail_checks_passing"));
    assert!(rendered.contains("method_counts=GET:1,POST:1"));
    assert!(rendered.contains("status_code_counts=201:1,503:1"));
    assert!(rendered.contains("guardrail_failure_streak_threshold=2"));
    assert!(rendered.contains("guardrail_retryable_failures_threshold=2"));
    assert!(rendered.contains(
        "reason_code_counts=duplicate_cases_skipped:1,healthy_cycle:2,retry_attempted:1"
    ));
}

#[test]
fn regression_collect_gateway_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let gateway_root = temp.path().join("gateway");
    std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
    std::fs::write(
        gateway_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 741,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write gateway state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.gateway_state_dir = gateway_root;

    let report = collect_gateway_status_report(&cli).expect("collect gateway status report");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert!(report.reason_code_counts.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
    assert_eq!(report.rollout_reason_code, "health_state_not_healthy");
    assert_eq!(report.service_status, "running");
    assert_eq!(report.guardrail_gate, "hold");
    assert_eq!(report.guardrail_reason_code, "health_state_not_healthy");
    assert_eq!(report.guardrail_failure_streak_threshold, 0);
    assert_eq!(report.guardrail_retryable_failures_threshold, 0);
    assert_eq!(report.guardrail_failure_streak, 0);
    assert_eq!(report.guardrail_last_failed_cases, 0);
    assert_eq!(report.guardrail_last_retryable_failures, 0);
}

#[test]
fn integration_collect_gateway_status_report_service_stop_forces_rollout_hold() {
    let temp = tempdir().expect("tempdir");
    let gateway_root = temp.path().join("gateway");
    std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
    std::fs::write(
        gateway_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 901,
    "cycle_duration_ms": 11,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 1,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  },
  "guardrail": {
    "gate": "pass",
    "reason_code": "guardrail_checks_passing",
    "failure_streak_threshold": 2,
    "retryable_failures_threshold": 2,
    "failure_streak": 0,
    "last_failed_cases": 0,
    "last_retryable_failures": 0,
    "updated_unix_ms": 902
  },
  "service": {
    "status": "stopped",
    "startup_attempts": 3,
    "startup_failure_streak": 0,
    "last_startup_error": "",
    "last_started_unix_ms": 850,
    "last_stopped_unix_ms": 900,
    "last_transition_unix_ms": 900,
    "last_stop_reason": "maintenance_window"
  }
}
"#,
    )
    .expect("write gateway state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.gateway_state_dir = gateway_root;

    let report = collect_gateway_status_report(&cli).expect("collect gateway status report");
    assert_eq!(report.service_status, "stopped");
    assert_eq!(report.rollout_gate, "hold");
    assert_eq!(report.rollout_reason_code, "service_stopped");
    assert_eq!(report.guardrail_gate, "pass");
    assert_eq!(report.guardrail_reason_code, "guardrail_checks_passing");
    assert_eq!(report.service_last_stop_reason, "maintenance_window");
    assert_eq!(report.service_startup_attempts, 3);

    let rendered = render_gateway_status_report(&report);
    assert!(rendered.contains("service_status=stopped"));
    assert!(rendered.contains("rollout_reason_code=service_stopped"));
    assert!(rendered.contains("guardrail_gate=pass"));
}

#[test]
fn functional_collect_custom_command_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let custom_command_root = temp.path().join("custom-command");
    std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
    std::fs::write(
        custom_command_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["CREATE:deploy_release:create-1", "RUN:deploy_release:run-1"],
  "commands": [
    {
      "case_key": "CREATE:deploy_release:create-1",
      "case_id": "create-1",
      "command_name": "deploy_release",
      "template": "deploy {{env}}",
      "operation": "CREATE",
      "last_status_code": 201,
      "last_outcome": "success",
      "run_count": 2,
      "updated_unix_ms": 10
    },
    {
      "case_key": "UPDATE:triage_alerts:update-1",
      "case_id": "update-1",
      "command_name": "triage_alerts",
      "template": "triage {{severity}}",
      "operation": "UPDATE",
      "last_status_code": 200,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 20
    }
  ],
  "health": {
    "updated_unix_ms": 750,
    "cycle_duration_ms": 15,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write custom-command state");
    std::fs::write(
            custom_command_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["command_registry_mutated","command_runs_recorded"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_cases_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write custom-command events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.custom_command_state_dir = custom_command_root;

    let report =
        collect_custom_command_status_report(&cli).expect("collect custom-command status report");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_case_count, 2);
    assert_eq!(report.command_count, 2);
    assert_eq!(report.command_name_counts.get("deploy_release"), Some(&1));
    assert_eq!(report.command_name_counts.get("triage_alerts"), Some(&1));
    assert_eq!(report.operation_counts.get("CREATE"), Some(&1));
    assert_eq!(report.operation_counts.get("UPDATE"), Some(&1));
    assert_eq!(report.outcome_counts.get("success"), Some(&2));
    assert_eq!(report.status_code_counts.get("200"), Some(&1));
    assert_eq!(report.status_code_counts.get("201"), Some(&1));
    assert_eq!(report.total_run_count, 3);
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(
        report.last_reason_codes,
        vec![
            "healthy_cycle".to_string(),
            "duplicate_cases_skipped".to_string()
        ]
    );
    assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&1));
    assert_eq!(
        report.reason_code_counts.get("command_registry_mutated"),
        Some(&1)
    );
    assert_eq!(
        report.reason_code_counts.get("command_runs_recorded"),
        Some(&1)
    );
    assert_eq!(
        report.reason_code_counts.get("duplicate_cases_skipped"),
        Some(&1)
    );

    let rendered = render_custom_command_status_report(&report);
    assert!(rendered.contains("custom-command status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("command_name_counts=deploy_release:1,triage_alerts:1"));
    assert!(rendered.contains("operation_counts=CREATE:1,UPDATE:1"));
    assert!(rendered.contains("outcome_counts=success:2"));
    assert!(rendered.contains("status_code_counts=200:1,201:1"));
    assert!(rendered.contains("total_run_count=3"));
    assert!(rendered.contains(
            "reason_code_counts=command_registry_mutated:1,command_runs_recorded:1,duplicate_cases_skipped:1,healthy_cycle:1"
        ));
}

#[test]
fn regression_collect_custom_command_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let custom_command_root = temp.path().join("custom-command");
    std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
    std::fs::write(
        custom_command_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": [],
  "health": {
    "updated_unix_ms": 751,
    "cycle_duration_ms": 17,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write custom-command state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.custom_command_state_dir = custom_command_root;

    let report =
        collect_custom_command_status_report(&cli).expect("collect custom-command status report");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert!(report.reason_code_counts.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
}

#[test]
fn functional_collect_voice_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let voice_root = temp.path().join("voice");
    std::fs::create_dir_all(&voice_root).expect("create voice dir");
    std::fs::write(
        voice_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["turn:tau:ops-1:voice-success-turn", "wake_word:tau:ops-2:voice-wake"],
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
      "run_count": 2,
      "updated_unix_ms": 10
    },
    {
      "case_key": "wake_word:tau:ops-2:voice-wake",
      "case_id": "voice-wake",
      "mode": "wake_word",
      "wake_word": "tau",
      "locale": "en-US",
      "speaker_id": "ops-2",
      "utterance": "",
      "last_status_code": 202,
      "last_outcome": "success",
      "run_count": 1,
      "updated_unix_ms": 20
    }
  ],
  "health": {
    "updated_unix_ms": 760,
    "cycle_duration_ms": 14,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write voice state");
    std::fs::write(
            voice_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["turns_handled","wake_word_detected"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle","duplicate_cases_skipped"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write voice events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.voice_state_dir = voice_root;

    let report = collect_voice_status_report(&cli).expect("collect voice status report");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_case_count, 2);
    assert_eq!(report.interaction_count, 2);
    assert_eq!(report.mode_counts.get("turn"), Some(&1));
    assert_eq!(report.mode_counts.get("wake_word"), Some(&1));
    assert_eq!(report.speaker_counts.get("ops-1"), Some(&1));
    assert_eq!(report.speaker_counts.get("ops-2"), Some(&1));
    assert_eq!(report.outcome_counts.get("success"), Some(&2));
    assert_eq!(report.status_code_counts.get("202"), Some(&2));
    assert_eq!(report.utterance_count, 1);
    assert_eq!(report.total_run_count, 3);
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(
        report.last_reason_codes,
        vec![
            "healthy_cycle".to_string(),
            "duplicate_cases_skipped".to_string()
        ]
    );
    assert_eq!(report.reason_code_counts.get("turns_handled"), Some(&1));
    assert_eq!(
        report.reason_code_counts.get("wake_word_detected"),
        Some(&1)
    );
    assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&1));
    assert_eq!(
        report.reason_code_counts.get("duplicate_cases_skipped"),
        Some(&1)
    );

    let rendered = render_voice_status_report(&report);
    assert!(rendered.contains("voice status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("mode_counts=turn:1,wake_word:1"));
    assert!(rendered.contains("speaker_counts=ops-1:1,ops-2:1"));
    assert!(rendered.contains("outcome_counts=success:2"));
    assert!(rendered.contains("status_code_counts=202:2"));
    assert!(rendered.contains("utterance_count=1"));
    assert!(rendered.contains("total_run_count=3"));
    assert!(rendered.contains(
            "reason_code_counts=duplicate_cases_skipped:1,healthy_cycle:1,turns_handled:1,wake_word_detected:1"
        ));
}

#[test]
fn regression_collect_voice_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let voice_root = temp.path().join("voice");
    std::fs::create_dir_all(&voice_root).expect("create voice dir");
    std::fs::write(
        voice_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": [],
  "health": {
    "updated_unix_ms": 761,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write voice state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.voice_state_dir = voice_root;

    let report = collect_voice_status_report(&cli).expect("collect voice status report");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert!(report.reason_code_counts.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
}

#[test]
fn functional_collect_deployment_status_report_reads_state_and_cycle_reports() {
    let temp = tempdir().expect("tempdir");
    let deployment_root = temp.path().join("deployment");
    std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
    std::fs::write(
            deployment_root.join("state.json"),
            r#"{
  "schema_version": 1,
  "processed_case_keys": ["container:staging-container:deployment-container-staging", "wasm:edge-wasm:deployment-wasm-edge"],
  "rollouts": [
    {
      "deploy_target": "container",
      "runtime_profile": "native",
      "environment": "staging",
      "status_code": 202,
      "outcome": "success",
      "error_code": "",
      "replicas": 1
    },
    {
      "deploy_target": "wasm",
      "runtime_profile": "wasm_wasi",
      "environment": "production",
      "status_code": 201,
      "outcome": "success",
      "error_code": "",
      "replicas": 2
    }
  ],
  "health": {
    "updated_unix_ms": 780,
    "cycle_duration_ms": 16,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 0,
    "last_cycle_discovered": 2,
    "last_cycle_processed": 2,
    "last_cycle_completed": 2,
    "last_cycle_failed": 0,
    "last_cycle_duplicates": 0
  }
}
"#,
        )
        .expect("write deployment state");
    std::fs::write(
            deployment_root.join("runtime-events.jsonl"),
            r#"{"reason_codes":["cloud_rollout_applied","wasm_rollout_applied"],"health_reason":"no recent transport failures observed"}
invalid-json-line
{"reason_codes":["healthy_cycle"],"health_reason":"no recent transport failures observed"}
"#,
        )
        .expect("write deployment events");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.deployment_state_dir = deployment_root;

    let report = collect_deployment_status_report(&cli).expect("collect deployment status");
    assert_eq!(report.health_state, TransportHealthState::Healthy.as_str());
    assert_eq!(report.rollout_gate, "pass");
    assert_eq!(report.processed_case_count, 2);
    assert_eq!(report.rollout_count, 2);
    assert_eq!(report.target_counts.get("container"), Some(&1));
    assert_eq!(report.target_counts.get("wasm"), Some(&1));
    assert_eq!(report.runtime_profile_counts.get("native"), Some(&1));
    assert_eq!(report.runtime_profile_counts.get("wasm_wasi"), Some(&1));
    assert_eq!(report.environment_counts.get("staging"), Some(&1));
    assert_eq!(report.environment_counts.get("production"), Some(&1));
    assert_eq!(report.outcome_counts.get("success"), Some(&2));
    assert_eq!(report.status_code_counts.get("201"), Some(&1));
    assert_eq!(report.status_code_counts.get("202"), Some(&1));
    assert_eq!(report.total_replicas, 3);
    assert_eq!(report.wasm_rollout_count, 1);
    assert_eq!(report.cloud_rollout_count, 1);
    assert_eq!(report.cycle_reports, 2);
    assert_eq!(report.invalid_cycle_reports, 1);
    assert_eq!(report.last_reason_codes, vec!["healthy_cycle".to_string()]);
    assert_eq!(report.reason_code_counts.get("healthy_cycle"), Some(&1));
    assert_eq!(
        report.reason_code_counts.get("cloud_rollout_applied"),
        Some(&1)
    );
    assert_eq!(
        report.reason_code_counts.get("wasm_rollout_applied"),
        Some(&1)
    );

    let rendered = render_deployment_status_report(&report);
    assert!(rendered.contains("deployment status inspect:"));
    assert!(rendered.contains("rollout_gate=pass"));
    assert!(rendered.contains("target_counts=container:1,wasm:1"));
    assert!(rendered.contains("runtime_profile_counts=native:1,wasm_wasi:1"));
    assert!(rendered.contains("total_replicas=3"));
    assert!(rendered.contains("wasm_rollout_count=1"));
    assert!(rendered.contains("cloud_rollout_count=1"));
    assert!(rendered.contains(
        "reason_code_counts=cloud_rollout_applied:1,healthy_cycle:1,wasm_rollout_applied:1"
    ));
}

#[test]
fn regression_collect_deployment_status_report_handles_missing_events_log() {
    let temp = tempdir().expect("tempdir");
    let deployment_root = temp.path().join("deployment");
    std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
    std::fs::write(
        deployment_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 781,
    "cycle_duration_ms": 20,
    "queue_depth": 0,
    "active_runs": 0,
    "failure_streak": 2,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write deployment state");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.deployment_state_dir = deployment_root;

    let report = collect_deployment_status_report(&cli).expect("collect deployment status");
    assert!(!report.events_log_present);
    assert_eq!(report.cycle_reports, 0);
    assert_eq!(report.invalid_cycle_reports, 0);
    assert!(report.last_reason_codes.is_empty());
    assert!(report.reason_code_counts.is_empty());
    assert_eq!(report.health_state, TransportHealthState::Degraded.as_str());
    assert_eq!(report.rollout_gate, "hold");
}

#[test]
fn unit_operator_health_state_rank_orders_expected_states() {
    assert_eq!(operator_health_state_rank("healthy"), 0);
    assert_eq!(operator_health_state_rank("degraded"), 1);
    assert_eq!(operator_health_state_rank("failing"), 2);
    assert_eq!(operator_health_state_rank("unknown"), 2);
}

#[test]
fn unit_operator_control_summary_diff_classifies_regression() {
    let baseline = operator_summary_fixture(OperatorSummaryFixtureInput {
        generated_unix_ms: 1,
        health_state: "healthy",
        rollout_gate: "pass",
        reason_code: "all_checks_passing",
        recommendation: "no_immediate_action_required",
        component_health_state: "healthy",
        component_rollout_gate: "pass",
        queue_depth: 0,
        failure_streak: 0,
    });
    let current = operator_summary_fixture(OperatorSummaryFixtureInput {
        generated_unix_ms: 2,
        health_state: "failing",
        rollout_gate: "hold",
        reason_code: "gateway:state_unavailable",
        recommendation: "initialize gateway state",
        component_health_state: "failing",
        component_rollout_gate: "hold",
        queue_depth: 4,
        failure_streak: 2,
    });

    let diff = build_operator_control_summary_diff_report(&baseline, &current);
    assert_eq!(diff.drift_state, "regressed");
    assert_eq!(diff.risk_level, "high");
    assert_eq!(diff.health_state_before, "healthy");
    assert_eq!(diff.health_state_after, "failing");
    assert_eq!(diff.rollout_gate_before, "pass");
    assert_eq!(diff.rollout_gate_after, "hold");
    assert_eq!(diff.changed_components.len(), 1);
    let gateway = &diff.changed_components[0];
    assert_eq!(gateway.component, "gateway");
    assert_eq!(gateway.drift_state, "regressed");
    assert_eq!(gateway.severity, "high");
    assert_eq!(gateway.queue_depth_before, 0);
    assert_eq!(gateway.queue_depth_after, 4);
    assert_eq!(gateway.failure_streak_before, 0);
    assert_eq!(gateway.failure_streak_after, 2);
}

#[test]
fn functional_operator_control_summary_diff_render_includes_component_deltas() {
    let baseline = operator_summary_fixture(OperatorSummaryFixtureInput {
        generated_unix_ms: 10,
        health_state: "healthy",
        rollout_gate: "pass",
        reason_code: "all_checks_passing",
        recommendation: "no_immediate_action_required",
        component_health_state: "healthy",
        component_rollout_gate: "pass",
        queue_depth: 0,
        failure_streak: 0,
    });
    let current = operator_summary_fixture(OperatorSummaryFixtureInput {
        generated_unix_ms: 11,
        health_state: "degraded",
        rollout_gate: "hold",
        reason_code: "gateway:service_stopped",
        recommendation: "start gateway service mode",
        component_health_state: "degraded",
        component_rollout_gate: "hold",
        queue_depth: 1,
        failure_streak: 1,
    });
    let diff = build_operator_control_summary_diff_report(&baseline, &current);
    let rendered = render_operator_control_summary_diff_report(&diff);

    assert!(rendered.contains("operator control summary diff:"));
    assert!(rendered.contains("drift_state=regressed"));
    assert!(rendered.contains("operator control summary diff component: component=gateway"));
    assert!(rendered.contains("reason_code_after=gateway:service_stopped"));
}

#[test]
fn functional_operator_control_summary_render_includes_expected_sections() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli(&["tau-rs"]);
    cli.channel_store_root = temp.path().join("channel-store");
    cli.dashboard_state_dir = temp.path().join("dashboard");
    cli.multi_channel_state_dir = temp.path().join("multi-channel");
    cli.multi_agent_state_dir = temp.path().join("multi-agent");
    cli.gateway_state_dir = temp.path().join("gateway");
    cli.deployment_state_dir = temp.path().join("deployment");
    cli.custom_command_state_dir = temp.path().join("custom-command");
    cli.voice_state_dir = temp.path().join("voice");
    cli.daemon_state_dir = temp.path().join("daemon");

    let report = collect_operator_control_summary_report(&cli).expect("collect summary");
    let rendered = render_operator_control_summary_report(&report);

    assert!(rendered.contains("operator control summary:"));
    assert!(rendered.contains("operator control policy posture:"));
    assert!(rendered.contains("operator control daemon:"));
    assert!(rendered.contains("operator control release channel:"));
    assert!(rendered.contains("operator control component: component=gateway"));
}

#[test]
fn integration_operator_control_summary_reflects_persisted_runtime_state_snapshots() {
    let temp = tempdir().expect("tempdir");
    let dashboard_root = temp.path().join("dashboard");
    let multi_channel_root = temp.path().join("multi-channel");
    let multi_agent_root = temp.path().join("multi-agent");
    let gateway_root = temp.path().join("gateway");
    let deployment_root = temp.path().join("deployment");
    let custom_command_root = temp.path().join("custom-command");
    let voice_root = temp.path().join("voice");
    std::fs::create_dir_all(&dashboard_root).expect("create dashboard dir");
    std::fs::create_dir_all(&multi_channel_root).expect("create multi-channel dir");
    std::fs::create_dir_all(&multi_agent_root).expect("create multi-agent dir");
    std::fs::create_dir_all(&gateway_root).expect("create gateway dir");
    std::fs::create_dir_all(&deployment_root).expect("create deployment dir");
    std::fs::create_dir_all(&custom_command_root).expect("create custom-command dir");
    std::fs::create_dir_all(&voice_root).expect("create voice dir");

    std::fs::write(
        dashboard_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["dashboard:1"],
  "widget_views": [],
  "control_audit": [],
  "health": {
    "updated_unix_ms": 100,
    "cycle_duration_ms": 10,
    "queue_depth": 1,
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
        multi_channel_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": ["telegram:1"],
  "health": {
    "updated_unix_ms": 101,
    "cycle_duration_ms": 11,
    "queue_depth": 2,
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
    .expect("write multi-channel state");
    std::fs::write(
        multi_agent_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["multi-agent:1"],
  "routed_cases": [],
  "health": {
    "updated_unix_ms": 102,
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
    .expect("write multi-agent state");
    std::fs::write(
        gateway_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["gateway:1"],
  "requests": [],
  "health": {
    "updated_unix_ms": 103,
    "cycle_duration_ms": 13,
    "queue_depth": 3,
    "active_runs": 1,
    "failure_streak": 1,
    "last_cycle_discovered": 1,
    "last_cycle_processed": 1,
    "last_cycle_completed": 0,
    "last_cycle_failed": 1,
    "last_cycle_duplicates": 0
  }
}
"#,
    )
    .expect("write gateway state");
    std::fs::write(
        deployment_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["deployment:1"],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 104,
    "cycle_duration_ms": 14,
    "queue_depth": 4,
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
    .expect("write deployment state");
    std::fs::write(
        custom_command_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["custom-command:1"],
  "commands": [],
  "health": {
    "updated_unix_ms": 105,
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
    .expect("write custom-command state");
    std::fs::write(
        voice_root.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": ["voice:1"],
  "interactions": [],
  "health": {
    "updated_unix_ms": 106,
    "cycle_duration_ms": 16,
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

    let daemon_state_dir = temp.path().join("daemon");
    let daemon_config = TauDaemonConfig {
        state_dir: daemon_state_dir.clone(),
        profile: CliDaemonProfile::Auto,
    };
    install_tau_daemon(&daemon_config).expect("install daemon");
    start_tau_daemon(&daemon_config).expect("start daemon");

    let mut cli = parse_cli(&["tau-rs"]);
    cli.channel_store_root = temp.path().join("channel-store");
    cli.dashboard_state_dir = dashboard_root;
    cli.multi_channel_state_dir = multi_channel_root;
    cli.multi_agent_state_dir = multi_agent_root;
    cli.gateway_state_dir = gateway_root;
    cli.deployment_state_dir = deployment_root;
    cli.custom_command_state_dir = custom_command_root;
    cli.voice_state_dir = voice_root;
    cli.daemon_state_dir = daemon_state_dir;

    let report = collect_operator_control_summary_report(&cli).expect("collect summary");
    let gateway_row = report
        .components
        .iter()
        .find(|row| row.component == "gateway")
        .expect("gateway row");
    let deployment_row = report
        .components
        .iter()
        .find(|row| row.component == "deployment")
        .expect("deployment row");
    assert_eq!(gateway_row.queue_depth, 3);
    assert_eq!(gateway_row.failure_streak, 1);
    assert_eq!(deployment_row.queue_depth, 4);
    assert!(report.daemon.running);
    assert_eq!(report.daemon.rollout_gate, "pass");
}

#[test]
fn integration_operator_control_summary_snapshot_roundtrip_and_compare() {
    let temp = tempdir().expect("tempdir");
    let baseline_path = temp.path().join("operator-control-baseline.json");
    let baseline = operator_summary_fixture(OperatorSummaryFixtureInput {
        generated_unix_ms: 100,
        health_state: "healthy",
        rollout_gate: "pass",
        reason_code: "all_checks_passing",
        recommendation: "no_immediate_action_required",
        component_health_state: "healthy",
        component_rollout_gate: "pass",
        queue_depth: 0,
        failure_streak: 0,
    });
    save_operator_control_summary_snapshot(&baseline_path, &baseline).expect("save snapshot");
    let loaded = load_operator_control_summary_snapshot(&baseline_path).expect("load snapshot");
    assert_eq!(loaded, baseline);

    let current = operator_summary_fixture(OperatorSummaryFixtureInput {
        generated_unix_ms: 101,
        health_state: "degraded",
        rollout_gate: "hold",
        reason_code: "gateway:state_unavailable",
        recommendation: "initialize gateway state",
        component_health_state: "degraded",
        component_rollout_gate: "hold",
        queue_depth: 2,
        failure_streak: 1,
    });
    let diff = build_operator_control_summary_diff_report(&loaded, &current);
    assert_eq!(diff.drift_state, "regressed");
    assert_eq!(diff.changed_components.len(), 1);
}

#[test]
fn regression_operator_control_summary_handles_missing_state_files_with_explicit_defaults() {
    let temp = tempdir().expect("tempdir");
    let mut cli = parse_cli(&["tau-rs"]);
    cli.channel_store_root = temp.path().join("channel-store");
    cli.dashboard_state_dir = temp.path().join("dashboard");
    cli.multi_channel_state_dir = temp.path().join("multi-channel");
    cli.multi_agent_state_dir = temp.path().join("multi-agent");
    cli.gateway_state_dir = temp.path().join("gateway");
    cli.deployment_state_dir = temp.path().join("deployment");
    cli.custom_command_state_dir = temp.path().join("custom-command");
    cli.voice_state_dir = temp.path().join("voice");
    cli.daemon_state_dir = temp.path().join("daemon");

    let report = collect_operator_control_summary_report(&cli).expect("collect summary");
    assert_eq!(report.rollout_gate, "hold");
    assert!(report
        .components
        .iter()
        .all(|row| row.reason_code == "state_unavailable"));
    assert_eq!(report.health_state, "failing");

    let rendered = render_operator_control_summary_report(&report);
    assert!(rendered.contains("operator control summary:"));
    assert!(rendered.contains("reason_code=state_unavailable"));
}

#[test]
fn regression_operator_control_summary_snapshot_load_fails_closed_for_missing_file() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("missing-summary.json");
    let error =
        load_operator_control_summary_snapshot(&missing).expect_err("missing snapshot must fail");
    assert!(error
        .to_string()
        .contains("failed to read operator control summary snapshot"));
}
