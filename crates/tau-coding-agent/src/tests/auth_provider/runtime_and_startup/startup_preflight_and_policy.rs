//! Tests for startup preflight orchestration and tool-policy/sandbox behavior.

use super::*;

#[test]
fn integration_execute_startup_preflight_runs_onboarding_and_generates_report() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.onboard = true;
    cli.onboard_non_interactive = true;
    cli.onboard_profile = "team_default".to_string();
    cli.onboard_install_daemon = true;
    cli.onboard_start_daemon = true;

    let handled = execute_startup_preflight(&cli).expect("onboarding preflight");
    assert!(handled);

    let profile_store = temp.path().join(".tau/profiles.json");
    assert!(profile_store.exists(), "profile store should be created");
    let release_channel_store = temp.path().join(".tau/release-channel.json");
    assert!(
        release_channel_store.exists(),
        "release channel store should be created"
    );

    let reports_dir = temp.path().join(".tau/reports");
    let reports = std::fs::read_dir(&reports_dir)
        .expect("reports dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("onboarding-") && name.ends_with(".json"))
        })
        .collect::<Vec<_>>();
    assert!(
        !reports.is_empty(),
        "expected at least one onboarding report in {}",
        reports_dir.display()
    );

    let latest_report = reports.last().expect("latest onboarding report");
    let report_payload =
        std::fs::read_to_string(latest_report).expect("read onboarding report payload");
    let report_json =
        serde_json::from_str::<serde_json::Value>(&report_payload).expect("parse report payload");
    assert_eq!(report_json["release_channel"], "stable");
    assert_eq!(report_json["release_channel_source"], "default");
    assert_eq!(report_json["release_channel_action"], "created");
    assert_eq!(report_json["daemon_bootstrap"]["requested_install"], true);
    assert_eq!(report_json["daemon_bootstrap"]["requested_start"], true);
    assert_eq!(
        report_json["daemon_bootstrap"]["install_action"],
        "installed"
    );
    assert_eq!(report_json["daemon_bootstrap"]["start_action"], "started");
    assert_eq!(report_json["daemon_bootstrap"]["ready"], true);
    assert_eq!(report_json["daemon_bootstrap"]["status"]["installed"], true);
    assert_eq!(report_json["daemon_bootstrap"]["status"]["running"], true);
    assert_eq!(report_json["identity_composition"]["schema_version"], 1);
    assert_eq!(report_json["identity_composition"]["loaded_count"], 0);
    assert_eq!(report_json["identity_composition"]["missing_count"], 3);
    let daemon_state_path = report_json["daemon_bootstrap"]["status"]["state_path"]
        .as_str()
        .expect("daemon state path string");
    assert!(
        PathBuf::from(daemon_state_path).exists(),
        "daemon state file should exist after onboarding preflight"
    );
}

#[test]
fn integration_execute_startup_preflight_runs_multi_channel_live_readiness_preflight() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let readiness_env_vars = [
        "TAU_TELEGRAM_BOT_TOKEN",
        "TAU_DISCORD_BOT_TOKEN",
        "TAU_WHATSAPP_ACCESS_TOKEN",
        "TAU_WHATSAPP_PHONE_NUMBER_ID",
    ];
    let snapshot = snapshot_env_vars(&readiness_env_vars);
    for key in readiness_env_vars {
        std::env::remove_var(key);
    }

    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
    std::fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    std::fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    std::fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");

    std::env::set_var("TAU_TELEGRAM_BOT_TOKEN", "telegram-token");
    std::env::set_var("TAU_DISCORD_BOT_TOKEN", "discord-token");
    std::env::set_var("TAU_WHATSAPP_ACCESS_TOKEN", "whatsapp-access-token");
    std::env::set_var("TAU_WHATSAPP_PHONE_NUMBER_ID", "15551234567");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_live_readiness_preflight = true;
    cli.multi_channel_live_readiness_json = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;

    let handled = execute_startup_preflight(&cli).expect("readiness preflight should pass");
    assert!(handled);

    restore_env_vars(snapshot);
}

#[test]
fn regression_execute_startup_preflight_multi_channel_live_readiness_preflight_fails_closed() {
    let _env_lock = AUTH_ENV_TEST_LOCK
        .lock()
        .expect("acquire auth env test lock");
    let readiness_env_vars = [
        "TAU_TELEGRAM_BOT_TOKEN",
        "TAU_DISCORD_BOT_TOKEN",
        "TAU_WHATSAPP_ACCESS_TOKEN",
        "TAU_WHATSAPP_PHONE_NUMBER_ID",
    ];
    let snapshot = snapshot_env_vars(&readiness_env_vars);
    for key in readiness_env_vars {
        std::env::remove_var(key);
    }

    let temp = tempdir().expect("tempdir");
    let ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&ingress_dir).expect("create ingress dir");
    std::fs::write(ingress_dir.join("telegram.ndjson"), "").expect("write telegram inbox");
    std::fs::write(ingress_dir.join("discord.ndjson"), "").expect("write discord inbox");
    std::fs::write(ingress_dir.join("whatsapp.ndjson"), "").expect("write whatsapp inbox");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_live_readiness_preflight = true;
    cli.multi_channel_live_ingress_dir = ingress_dir;

    let error = execute_startup_preflight(&cli).expect_err("missing secrets should fail closed");
    let error_text = error.to_string();
    assert!(error_text.contains("multi-channel live readiness gate: status=fail"));
    assert!(error_text.contains("multi_channel_live.channel.telegram:missing_prerequisites"));
    assert!(error_text.contains("multi_channel_live.channel.discord:missing_prerequisites"));
    assert!(error_text.contains("multi_channel_live.channel.whatsapp:missing_prerequisites"));

    restore_env_vars(snapshot);
}

#[test]
fn integration_execute_startup_preflight_runs_multi_channel_live_ingest_mode() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("telegram-update.json");
    std::fs::write(
        &payload_file,
        r#"{
  "update_id": 9001,
  "message": {
    "message_id": 42,
    "chat": { "id": "chat-100" },
    "from": { "id": "user-7", "username": "alice" },
    "date": 1760100000,
    "text": "hello from telegram"
  }
}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_live_ingest_provider = "telegram-bot-api".to_string();
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    let handled = execute_startup_preflight(&cli).expect("multi-channel live ingest preflight");
    assert!(handled);

    let ingress_file = cli.multi_channel_live_ingest_dir.join("telegram.ndjson");
    assert!(ingress_file.exists(), "ingress file should be created");
    let lines = std::fs::read_to_string(&ingress_file)
        .expect("read ingress file")
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let parsed: serde_json::Value =
        serde_json::from_str(&lines[0]).expect("ingress line should be valid json");
    assert_eq!(parsed["transport"].as_str(), Some("telegram"));
    assert_eq!(parsed["provider"].as_str(), Some("telegram-bot-api"));
}

#[test]
fn regression_execute_startup_preflight_multi_channel_live_ingest_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let payload_file = temp.path().join("discord-invalid.json");
    std::fs::write(
        &payload_file,
        r#"{
  "id": "discord-msg-2",
  "channel_id": "discord-channel-99",
  "timestamp": "2026-01-10T13:00:00Z",
  "content": "hello"
}"#,
    )
    .expect("write payload");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_live_ingest_file = Some(payload_file);
    cli.multi_channel_live_ingest_transport = Some(CliMultiChannelTransport::Discord);
    cli.multi_channel_live_ingest_provider = "discord-gateway".to_string();
    cli.multi_channel_live_ingest_dir = temp.path().join("live-ingress");

    let error =
        execute_startup_preflight(&cli).expect_err("invalid ingress payload should fail closed");
    let error_text = error.to_string();
    assert!(error_text.contains("multi-channel live ingest"));
    assert!(error_text.contains("reason_code=missing_field"));
}

#[test]
fn functional_execute_startup_preflight_runs_multi_channel_channel_login_and_status() {
    let temp = tempdir().expect("tempdir");
    let live_ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&live_ingress_dir).expect("create live ingress");

    let mut login_cli = test_cli();
    set_workspace_tau_paths(&mut login_cli, temp.path());
    login_cli.multi_channel_live_ingress_dir = live_ingress_dir.clone();
    login_cli.multi_channel_channel_login = Some(CliMultiChannelTransport::Telegram);
    login_cli.multi_channel_telegram_bot_token = Some("telegram-secret".to_string());

    let login_handled =
        execute_startup_preflight(&login_cli).expect("multi-channel channel login preflight");
    assert!(login_handled);

    let ingress_file = live_ingress_dir.join("telegram.ndjson");
    assert!(ingress_file.exists(), "login should create ingress file");

    let mut status_cli = test_cli();
    set_workspace_tau_paths(&mut status_cli, temp.path());
    status_cli.multi_channel_live_ingress_dir = live_ingress_dir;
    status_cli.multi_channel_channel_status = Some(CliMultiChannelTransport::Telegram);
    status_cli.multi_channel_telegram_bot_token = Some("telegram-secret".to_string());
    status_cli.multi_channel_channel_status_json = true;

    let status_handled =
        execute_startup_preflight(&status_cli).expect("multi-channel channel status preflight");
    assert!(status_handled);

    let state_raw = std::fs::read_to_string(
        status_cli
            .multi_channel_state_dir
            .join("security/channel-lifecycle.json"),
    )
    .expect("read lifecycle state");
    let parsed: serde_json::Value = serde_json::from_str(&state_raw).expect("parse lifecycle");
    assert_eq!(
        parsed["channels"]["telegram"]["lifecycle_status"].as_str(),
        Some("initialized")
    );
}

#[test]
fn integration_execute_startup_preflight_runs_multi_channel_channel_logout_and_probe() {
    let temp = tempdir().expect("tempdir");
    let live_ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&live_ingress_dir).expect("create live ingress");

    let mut login_cli = test_cli();
    set_workspace_tau_paths(&mut login_cli, temp.path());
    login_cli.multi_channel_live_ingress_dir = live_ingress_dir.clone();
    login_cli.multi_channel_channel_login = Some(CliMultiChannelTransport::Whatsapp);
    login_cli.multi_channel_whatsapp_access_token = Some("wa-token".to_string());
    login_cli.multi_channel_whatsapp_phone_number_id = Some("15551230000".to_string());
    execute_startup_preflight(&login_cli).expect("multi-channel login preflight");

    let mut logout_cli = test_cli();
    set_workspace_tau_paths(&mut logout_cli, temp.path());
    logout_cli.multi_channel_live_ingress_dir = live_ingress_dir.clone();
    logout_cli.multi_channel_channel_logout = Some(CliMultiChannelTransport::Whatsapp);
    let logout_handled =
        execute_startup_preflight(&logout_cli).expect("multi-channel logout preflight");
    assert!(logout_handled);

    let mut probe_cli = test_cli();
    set_workspace_tau_paths(&mut probe_cli, temp.path());
    probe_cli.multi_channel_live_ingress_dir = live_ingress_dir;
    probe_cli.multi_channel_channel_probe = Some(CliMultiChannelTransport::Whatsapp);
    probe_cli.multi_channel_whatsapp_access_token = Some("wa-token".to_string());
    probe_cli.multi_channel_whatsapp_phone_number_id = Some("15551230000".to_string());
    probe_cli.multi_channel_channel_probe_json = true;
    let probe_handled =
        execute_startup_preflight(&probe_cli).expect("multi-channel probe preflight");
    assert!(probe_handled);

    let state_raw = std::fs::read_to_string(
        probe_cli
            .multi_channel_state_dir
            .join("security/channel-lifecycle.json"),
    )
    .expect("read lifecycle state");
    let parsed: serde_json::Value = serde_json::from_str(&state_raw).expect("parse lifecycle");
    assert_eq!(
        parsed["channels"]["whatsapp"]["last_action"].as_str(),
        Some("probe")
    );
}

#[test]
fn integration_execute_startup_preflight_runs_multi_channel_send_command() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_send = Some(CliMultiChannelTransport::Discord);
    cli.multi_channel_send_target = Some("123456789012345678".to_string());
    cli.multi_channel_send_text = Some("hello from preflight send".to_string());
    cli.multi_channel_send_json = true;
    cli.multi_channel_outbound_mode = CliMultiChannelOutboundMode::DryRun;

    let handled = execute_startup_preflight(&cli).expect("multi-channel send preflight");
    assert!(handled);

    let store = crate::channel_store::ChannelStore::open(
        &cli.multi_channel_state_dir.join("channel-store"),
        "discord",
        "123456789012345678",
    )
    .expect("open channel-store");
    let logs = store.load_log_entries().expect("load channel logs");
    assert!(!logs.is_empty(), "send preflight should persist audit log");
    assert_eq!(logs[0].source, "multi_channel_send");
}

#[test]
fn integration_execute_startup_preflight_runs_multi_channel_incident_timeline_command() {
    let temp = tempdir().expect("tempdir");
    let replay_export_path = temp.path().join("exports/incident-replay.json");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_incident_timeline = true;
    cli.multi_channel_incident_timeline_json = true;
    cli.multi_channel_incident_event_limit = Some(10);
    cli.multi_channel_incident_replay_export = Some(replay_export_path.clone());

    let channel_dir = cli
        .multi_channel_state_dir
        .join("channel-store/channels/discord/ops-room");
    std::fs::create_dir_all(&channel_dir).expect("create channel dir");
    std::fs::write(
        channel_dir.join("log.jsonl"),
        r#"{"timestamp_unix_ms":1760200300000,"direction":"inbound","event_key":"evt-preflight","source":"discord","payload":{"transport":"discord","conversation_id":"ops-room","route_session_key":"ops-room","route":{"binding_id":"discord-ops","binding_matched":true},"channel_policy":{"reason_code":"allow_channel_policy_allow_from_any"}}}
{"timestamp_unix_ms":1760200300010,"direction":"outbound","event_key":"evt-preflight","source":"tau-multi-channel-runner","payload":{"event_key":"evt-preflight","response":"ok","delivery":{"mode":"dry_run","receipts":[{"status":"dry_run"}]}}}
"#,
    )
    .expect("write channel log");

    let handled =
        execute_startup_preflight(&cli).expect("multi-channel incident timeline preflight");
    assert!(handled);
    assert!(
        replay_export_path.exists(),
        "incident replay export should be written"
    );
    let replay_raw = std::fs::read_to_string(&replay_export_path).expect("read replay export");
    let replay_json: serde_json::Value =
        serde_json::from_str(&replay_raw).expect("parse replay export");
    assert_eq!(replay_json["schema_version"].as_u64(), Some(1));
}

#[test]
fn regression_execute_startup_preflight_multi_channel_channel_lifecycle_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let live_ingress_dir = temp.path().join("live-ingress");
    std::fs::create_dir_all(&live_ingress_dir).expect("create live ingress");

    let state_path = temp
        .path()
        .join(".tau/multi-channel/security/channel-lifecycle.json");
    std::fs::create_dir_all(state_path.parent().expect("parent")).expect("create parent");
    std::fs::write(&state_path, "{corrupted").expect("write corrupted state");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_live_ingress_dir = live_ingress_dir;
    cli.multi_channel_channel_probe = Some(CliMultiChannelTransport::Telegram);
    cli.multi_channel_telegram_bot_token = Some("telegram-secret".to_string());

    let error = execute_startup_preflight(&cli).expect_err("corrupted lifecycle should fail");
    assert!(error
        .to_string()
        .contains("failed to parse multi-channel lifecycle state"));
}

#[test]
fn functional_execute_startup_preflight_runs_deployment_wasm_package_mode() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_package_module = Some(module_path);
    cli.deployment_wasm_package_blueprint_id = "edge-wasm-preflight".to_string();
    cli.deployment_wasm_package_output_dir = temp.path().join("wasm-out");
    cli.deployment_wasm_package_json = true;

    let handled = execute_startup_preflight(&cli).expect("deployment wasm package preflight");
    assert!(handled);

    let blueprint_dir = cli
        .deployment_wasm_package_output_dir
        .join("edge-wasm-preflight");
    assert!(
        blueprint_dir.exists(),
        "blueprint output directory should exist"
    );
    let manifest_files = std::fs::read_dir(&blueprint_dir)
        .expect("read blueprint output")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.to_string_lossy().ends_with(".manifest.json"))
        .collect::<Vec<_>>();
    assert_eq!(manifest_files.len(), 1);
}

#[test]
fn integration_execute_startup_preflight_deployment_wasm_package_updates_state_metadata() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_package_module = Some(module_path);
    cli.deployment_wasm_package_blueprint_id = "edge-wasm-state".to_string();
    cli.deployment_wasm_package_output_dir = temp.path().join("wasm-out");
    let handled = execute_startup_preflight(&cli).expect("deployment wasm package state preflight");
    assert!(handled);

    let state_raw = std::fs::read_to_string(cli.deployment_state_dir.join("state.json"))
        .expect("read deployment state");
    let state_json: serde_json::Value = serde_json::from_str(&state_raw).expect("parse state");
    let deliverables = state_json
        .get("wasm_deliverables")
        .and_then(serde_json::Value::as_array)
        .expect("wasm deliverables should be an array");
    assert_eq!(deliverables.len(), 1);
    assert_eq!(
        deliverables[0]
            .get("blueprint_id")
            .and_then(serde_json::Value::as_str),
        Some("edge-wasm-state")
    );
    assert!(deliverables[0]
        .get("artifact_sha256")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value.len() == 64));
}

#[test]
fn functional_execute_startup_preflight_runs_deployment_wasm_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let module_path = temp.path().join("edge.wasm");
    std::fs::write(
        &module_path,
        [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00],
    )
    .expect("write wasm");

    let package_report = crate::deployment_wasm::package_deployment_wasm_artifact(
        &crate::deployment_wasm::DeploymentWasmPackageConfig {
            module_path,
            blueprint_id: "edge-wasm-inspect".to_string(),
            runtime_profile: "wasm_wasi".to_string(),
            output_dir: temp.path().join("wasm-out"),
            state_dir: temp.path().join(".tau/deployment"),
        },
    )
    .expect("package wasm");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_inspect_manifest = Some(PathBuf::from(package_report.manifest_path));
    cli.deployment_wasm_inspect_json = true;

    let handled = execute_startup_preflight(&cli).expect("deployment wasm inspect preflight");
    assert!(handled);
}

#[test]
fn integration_execute_startup_preflight_runs_deployment_wasm_browser_did_init_mode() {
    let temp = tempdir().expect("tempdir");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_browser_did_init = true;
    cli.deployment_wasm_browser_did_subject = "edge-browser-agent".to_string();
    cli.deployment_wasm_browser_did_entropy = "seed-browser-01".to_string();
    cli.deployment_wasm_browser_did_output = temp.path().join("browser-did.json");
    cli.deployment_wasm_browser_did_json = true;

    let handled =
        execute_startup_preflight(&cli).expect("deployment wasm browser did init preflight");
    assert!(handled);

    let raw = std::fs::read_to_string(&cli.deployment_wasm_browser_did_output)
        .expect("read browser did output");
    let payload: serde_json::Value = serde_json::from_str(&raw).expect("parse browser did output");
    assert_eq!(
        payload
            .get("identity")
            .and_then(|value| value.get("did"))
            .and_then(serde_json::Value::as_str)
            .map(|value| value.starts_with("did:key:")),
        Some(true)
    );
}

#[test]
fn regression_execute_startup_preflight_deployment_wasm_package_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let invalid_module_path = temp.path().join("invalid.bin");
    std::fs::write(&invalid_module_path, b"not-wasm").expect("write invalid");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_package_module = Some(invalid_module_path);
    cli.deployment_wasm_package_blueprint_id = "edge-invalid".to_string();
    cli.deployment_wasm_package_output_dir = temp.path().join("wasm-out");

    let error =
        execute_startup_preflight(&cli).expect_err("invalid wasm package preflight should fail");
    assert!(error.to_string().contains("invalid wasm module"));
}

#[test]
fn regression_execute_startup_preflight_deployment_wasm_inspect_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let manifest_path = temp.path().join("invalid.manifest.json");
    std::fs::write(&manifest_path, "{invalid-json").expect("write invalid manifest");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_inspect_manifest = Some(manifest_path);

    let error =
        execute_startup_preflight(&cli).expect_err("invalid wasm inspect preflight should fail");
    assert!(error
        .to_string()
        .contains("invalid deployment wasm manifest"));
}

#[test]
fn regression_execute_startup_preflight_deployment_wasm_browser_did_init_fails_closed() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_wasm_browser_did_init = true;
    cli.deployment_wasm_browser_did_subject = "invalid subject".to_string();
    cli.deployment_wasm_browser_did_output = temp.path().join("browser-did.json");

    let error = execute_startup_preflight(&cli)
        .expect_err("invalid browser did subject should fail in preflight");
    assert!(error
        .to_string()
        .contains("subject contains unsupported characters"));
}

#[test]
fn functional_execute_startup_preflight_runs_project_index_build_mode() {
    let temp = tempdir().expect("tempdir");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(workspace.join("src")).expect("create workspace");
    std::fs::write(
        workspace.join("src").join("lib.rs"),
        "pub fn project_index_ready() {}\n",
    )
    .expect("write source file");

    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.project_index_build = true;
    cli.project_index_root = workspace.clone();
    cli.project_index_state_dir = temp.path().join(".tau").join("index");

    let handled = execute_startup_preflight(&cli).expect("project index preflight");
    assert!(handled);

    let index_path = cli.project_index_state_dir.join("project-index.json");
    assert!(index_path.exists(), "project index state should be written");
    let index_raw = std::fs::read_to_string(index_path).expect("read project index");
    let index_json: serde_json::Value = serde_json::from_str(&index_raw).expect("parse index");
    let indexed_files = index_json
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map(|rows| rows.len())
        .unwrap_or_default();
    assert_eq!(indexed_files, 1);
}

#[test]
fn regression_execute_startup_preflight_project_index_json_requires_mode() {
    let mut cli = test_cli();
    cli.project_index_json = true;
    let error = execute_startup_preflight(&cli)
        .expect_err("project index json without mode should fail in preflight");
    assert!(error
        .to_string()
        .contains("--project-index-json requires one of"));
}

#[test]
fn functional_execute_startup_preflight_runs_github_status_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.github_status_inspect = Some("owner/repo".to_string());
    cli.github_status_json = true;

    let repo_state_dir = cli.github_state_dir.join("owner__repo");
    std::fs::create_dir_all(&repo_state_dir).expect("create github repo state dir");
    std::fs::write(
        repo_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "issue_sessions": {},
  "health": {
    "updated_unix_ms": 800,
    "cycle_duration_ms": 11,
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

    let handled = execute_startup_preflight(&cli).expect("github status inspect preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_operator_control_summary_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.operator_control_summary = true;

    let handled = execute_startup_preflight(&cli).expect("operator control summary preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_gateway_status_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.gateway_status_inspect = true;
    cli.gateway_status_json = true;

    std::fs::create_dir_all(&cli.gateway_state_dir).expect("create gateway state dir");
    std::fs::write(
        cli.gateway_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "requests": [],
  "health": {
    "updated_unix_ms": 800,
    "cycle_duration_ms": 11,
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

    let handled = execute_startup_preflight(&cli).expect("gateway status inspect preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_gateway_remote_profile_inspect_mode() {
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;
    cli.gateway_remote_profile_json = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::ProxyRemote;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("edge-token".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let handled =
        execute_startup_preflight(&cli).expect("gateway remote profile inspect preflight");
    assert!(handled);
}

#[test]
fn integration_execute_startup_preflight_runs_gateway_remote_profile_inspect_tailscale_funnel_mode()
{
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;
    cli.gateway_remote_profile_json = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleFunnel;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = Some("edge-password".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let handled =
        execute_startup_preflight(&cli).expect("gateway remote profile inspect preflight");
    assert!(handled);
}

#[test]
fn regression_execute_startup_preflight_gateway_remote_profile_inspect_fails_closed() {
    let mut cli = test_cli();
    cli.gateway_remote_profile_inspect = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::LocalOnly;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("edge-token".to_string());
    cli.gateway_openresponses_bind = "0.0.0.0:8787".to_string();

    let error = execute_startup_preflight(&cli)
        .expect_err("unsafe local-only remote profile should fail closed");
    assert!(error.to_string().contains("local_only_non_loopback_bind"));
}

#[test]
fn functional_execute_startup_preflight_runs_gateway_remote_plan_mode() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.gateway_remote_plan_json = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::ProxyRemote;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("edge-token".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let handled = execute_startup_preflight(&cli).expect("gateway remote plan preflight");
    assert!(handled);
}

#[test]
fn integration_execute_startup_preflight_runs_gateway_remote_plan_tailscale_serve_mode() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleServe;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::Token;
    cli.gateway_openresponses_auth_token = Some("edge-token".to_string());
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let handled = execute_startup_preflight(&cli).expect("gateway remote plan preflight");
    assert!(handled);
}

#[test]
fn regression_execute_startup_preflight_gateway_remote_plan_fails_closed() {
    let mut cli = test_cli();
    cli.gateway_remote_plan = true;
    cli.gateway_openresponses_server = true;
    cli.gateway_remote_profile = CliGatewayRemoteProfile::TailscaleFunnel;
    cli.gateway_openresponses_auth_mode = CliGatewayOpenResponsesAuthMode::PasswordSession;
    cli.gateway_openresponses_auth_password = None;
    cli.gateway_openresponses_bind = "127.0.0.1:8787".to_string();

    let error =
        execute_startup_preflight(&cli).expect_err("missing funnel password should fail closed");
    assert!(error
        .to_string()
        .contains("gateway remote plan rejected: profile=tailscale-funnel gate=hold"));
    assert!(error
        .to_string()
        .contains("tailscale_funnel_missing_password"));
}

#[test]
fn functional_execute_startup_preflight_runs_gateway_service_start_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.gateway_service_start = true;

    let handled = execute_startup_preflight(&cli).expect("gateway service start preflight");
    assert!(handled);

    let state_raw =
        std::fs::read_to_string(cli.gateway_state_dir.join("state.json")).expect("read state");
    let parsed: serde_json::Value = serde_json::from_str(&state_raw).expect("parse state");
    assert_eq!(parsed["service"]["status"].as_str(), Some("running"));
    assert!(parsed["service"]["startup_attempts"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn integration_execute_startup_preflight_runs_gateway_service_stop_and_status_modes() {
    let temp = tempdir().expect("tempdir");
    let mut start_cli = test_cli();
    set_workspace_tau_paths(&mut start_cli, temp.path());
    start_cli.gateway_service_start = true;
    execute_startup_preflight(&start_cli).expect("gateway service start preflight");

    let mut stop_cli = test_cli();
    set_workspace_tau_paths(&mut stop_cli, temp.path());
    stop_cli.gateway_service_stop = true;
    stop_cli.gateway_service_stop_reason = Some("maintenance_window".to_string());
    let stop_handled =
        execute_startup_preflight(&stop_cli).expect("gateway service stop preflight");
    assert!(stop_handled);

    let state_raw = std::fs::read_to_string(stop_cli.gateway_state_dir.join("state.json"))
        .expect("read stopped state");
    let parsed: serde_json::Value = serde_json::from_str(&state_raw).expect("parse stopped state");
    assert_eq!(parsed["service"]["status"].as_str(), Some("stopped"));
    assert_eq!(
        parsed["service"]["last_stop_reason"].as_str(),
        Some("maintenance_window")
    );

    let mut status_cli = test_cli();
    set_workspace_tau_paths(&mut status_cli, temp.path());
    status_cli.gateway_service_status = true;
    status_cli.gateway_service_status_json = true;
    let status_handled =
        execute_startup_preflight(&status_cli).expect("gateway service status preflight");
    assert!(status_handled);
}

#[test]
fn functional_execute_startup_preflight_runs_daemon_install_and_start_modes() {
    let temp = tempdir().expect("tempdir");
    let mut install_cli = test_cli();
    set_workspace_tau_paths(&mut install_cli, temp.path());
    install_cli.daemon_install = true;
    install_cli.daemon_profile = CliDaemonProfile::SystemdUser;

    let install_handled =
        execute_startup_preflight(&install_cli).expect("daemon install preflight");
    assert!(install_handled);
    let service_file = install_cli
        .daemon_state_dir
        .join("systemd")
        .join("tau-coding-agent.service");
    assert!(service_file.exists());

    let mut start_cli = test_cli();
    set_workspace_tau_paths(&mut start_cli, temp.path());
    start_cli.daemon_start = true;
    start_cli.daemon_profile = CliDaemonProfile::SystemdUser;

    let start_handled = execute_startup_preflight(&start_cli).expect("daemon start preflight");
    assert!(start_handled);
    assert!(start_cli.daemon_state_dir.join("daemon.pid").exists());
}

#[test]
fn integration_execute_startup_preflight_runs_daemon_stop_status_and_uninstall_modes() {
    let temp = tempdir().expect("tempdir");
    let mut install_cli = test_cli();
    set_workspace_tau_paths(&mut install_cli, temp.path());
    install_cli.daemon_install = true;
    install_cli.daemon_profile = CliDaemonProfile::SystemdUser;
    execute_startup_preflight(&install_cli).expect("daemon install preflight");

    let mut start_cli = test_cli();
    set_workspace_tau_paths(&mut start_cli, temp.path());
    start_cli.daemon_start = true;
    start_cli.daemon_profile = CliDaemonProfile::SystemdUser;
    execute_startup_preflight(&start_cli).expect("daemon start preflight");

    let mut stop_cli = test_cli();
    set_workspace_tau_paths(&mut stop_cli, temp.path());
    stop_cli.daemon_stop = true;
    stop_cli.daemon_profile = CliDaemonProfile::SystemdUser;
    stop_cli.daemon_stop_reason = Some("maintenance_window".to_string());
    let stop_handled = execute_startup_preflight(&stop_cli).expect("daemon stop preflight");
    assert!(stop_handled);
    assert!(!stop_cli.daemon_state_dir.join("daemon.pid").exists());

    let state_raw =
        std::fs::read_to_string(stop_cli.daemon_state_dir.join("state.json")).expect("read state");
    let parsed: serde_json::Value = serde_json::from_str(&state_raw).expect("parse state");
    assert_eq!(parsed["running"], false);
    assert_eq!(
        parsed["last_stop_reason"].as_str(),
        Some("maintenance_window")
    );

    let mut status_cli = test_cli();
    set_workspace_tau_paths(&mut status_cli, temp.path());
    status_cli.daemon_status = true;
    status_cli.daemon_status_json = true;
    status_cli.daemon_profile = CliDaemonProfile::SystemdUser;
    let status_handled = execute_startup_preflight(&status_cli).expect("daemon status preflight");
    assert!(status_handled);

    let mut uninstall_cli = test_cli();
    set_workspace_tau_paths(&mut uninstall_cli, temp.path());
    uninstall_cli.daemon_uninstall = true;
    uninstall_cli.daemon_profile = CliDaemonProfile::SystemdUser;
    let uninstall_handled =
        execute_startup_preflight(&uninstall_cli).expect("daemon uninstall preflight");
    assert!(uninstall_handled);
    let service_file = uninstall_cli
        .daemon_state_dir
        .join("systemd")
        .join("tau-coding-agent.service");
    assert!(!service_file.exists());
}

#[test]
fn functional_execute_startup_preflight_runs_multi_channel_status_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.multi_channel_status_inspect = true;
    cli.multi_channel_status_json = true;

    std::fs::create_dir_all(&cli.multi_channel_state_dir).expect("create multi-channel state dir");
    std::fs::write(
        cli.multi_channel_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_event_keys": [],
  "health": {
    "updated_unix_ms": 804,
    "cycle_duration_ms": 8,
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

    let handled = execute_startup_preflight(&cli).expect("multi-channel status inspect preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_deployment_status_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.deployment_status_inspect = true;
    cli.deployment_status_json = true;

    std::fs::create_dir_all(&cli.deployment_state_dir).expect("create deployment state dir");
    std::fs::write(
        cli.deployment_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "rollouts": [],
  "health": {
    "updated_unix_ms": 803,
    "cycle_duration_ms": 10,
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
    .expect("write deployment state");

    let handled = execute_startup_preflight(&cli).expect("deployment status inspect preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_custom_command_status_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.custom_command_status_inspect = true;
    cli.custom_command_status_json = true;

    std::fs::create_dir_all(&cli.custom_command_state_dir)
        .expect("create custom-command state dir");
    std::fs::write(
        cli.custom_command_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "commands": [],
  "health": {
    "updated_unix_ms": 801,
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
    .expect("write custom-command state");

    let handled = execute_startup_preflight(&cli).expect("custom-command status inspect preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_voice_status_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.voice_status_inspect = true;
    cli.voice_status_json = true;

    std::fs::create_dir_all(&cli.voice_state_dir).expect("create voice state dir");
    std::fs::write(
        cli.voice_state_dir.join("state.json"),
        r#"{
  "schema_version": 1,
  "processed_case_keys": [],
  "interactions": [],
  "health": {
    "updated_unix_ms": 802,
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
    .expect("write voice state");

    let handled = execute_startup_preflight(&cli).expect("voice status inspect preflight");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_events_inspect_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_inspect = true;
    cli.events_inspect_json = true;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("inspect.json"),
        r#"{
  "id": "inspect-now",
  "channel": "slack/C123",
  "prompt": "inspect me",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write inspect event");

    let handled = execute_startup_preflight(&cli).expect("events inspect preflight");
    assert!(handled);
    assert!(cli.events_dir.join("inspect.json").exists());
}

#[test]
fn integration_execute_startup_preflight_runs_events_validate_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_validate = true;
    cli.events_validate_json = true;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("validate.json"),
        r#"{
  "id": "validate-now",
  "channel": "slack/C123",
  "prompt": "validate me",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write validate event");

    let handled = execute_startup_preflight(&cli).expect("events validate preflight");
    assert!(handled);
}

#[test]
fn regression_execute_startup_preflight_events_validate_fails_on_invalid_entry() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_validate = true;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("invalid.json"),
        r#"{
  "id": "invalid",
  "channel": "slack/C123",
  "prompt": "bad",
  "schedule": {"type":"periodic","cron":"invalid-cron","timezone":"UTC"},
  "enabled": true
}
"#,
    )
    .expect("write invalid event");

    let error = execute_startup_preflight(&cli).expect_err("invalid event should fail");
    assert!(error.to_string().contains("events validate failed"));
}

#[test]
fn functional_execute_startup_preflight_runs_events_template_write_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    let target = cli.events_dir.join("template-periodic.json");
    cli.events_template_write = Some(target.clone());
    cli.events_template_schedule = CliEventTemplateSchedule::Periodic;
    cli.events_template_channel = Some("github/owner/repo#77".to_string());

    let handled = execute_startup_preflight(&cli).expect("events template preflight");
    assert!(handled);
    assert!(target.exists());
}

#[test]
fn regression_execute_startup_preflight_events_template_write_requires_overwrite() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    let target = cli.events_dir.join("template-existing.json");
    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(&target, "{\"existing\":true}\n").expect("seed existing template");

    cli.events_template_write = Some(target);
    let error = execute_startup_preflight(&cli).expect_err("overwrite should be required");
    assert!(error.to_string().contains("template path already exists"));
}

#[test]
fn functional_execute_startup_preflight_runs_events_simulate_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_simulate = true;
    cli.events_simulate_json = true;
    cli.events_simulate_horizon_seconds = 300;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("simulate.json"),
        r#"{
  "id": "simulate-now",
  "channel": "slack/C123",
  "prompt": "simulate me",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write simulate event");

    let handled = execute_startup_preflight(&cli).expect("events simulate preflight");
    assert!(handled);
}

#[test]
fn regression_execute_startup_preflight_events_simulate_reports_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_simulate = true;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("simulate-invalid.json"),
        r#"{
  "id": "simulate-invalid",
  "channel": "slack",
  "prompt": "bad",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write invalid event");

    let handled = execute_startup_preflight(&cli).expect("simulate preflight should still handle");
    assert!(handled);
}

#[test]
fn functional_execute_startup_preflight_runs_events_dry_run_mode() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_dry_run = true;
    cli.events_dry_run_json = true;
    cli.events_dry_run_strict = true;
    cli.events_queue_limit = 4;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("dry-run.json"),
        r#"{
  "id": "dry-run-now",
  "channel": "slack/C123",
  "prompt": "dry run me",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write dry-run event");

    let handled = execute_startup_preflight(&cli).expect("events dry-run preflight");
    assert!(handled);
    assert!(cli.events_dir.join("dry-run.json").exists());
    assert!(!cli.events_state_path.exists());
}

#[test]
fn regression_execute_startup_preflight_events_dry_run_reports_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_dry_run = true;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("dry-run-invalid.json"),
        r#"{
  "id": "dry-run-invalid",
  "channel": "slack",
  "prompt": "bad",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write invalid dry-run event");

    let handled = execute_startup_preflight(&cli).expect("dry-run preflight should still handle");
    assert!(handled);
}

#[test]
fn integration_execute_startup_preflight_events_dry_run_strict_fails_on_invalid_entries() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_dry_run = true;
    cli.events_dry_run_strict = true;

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("dry-run-invalid-strict.json"),
        r#"{
  "id": "dry-run-invalid-strict",
  "channel": "slack",
  "prompt": "bad",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write invalid strict dry-run event");

    let error = execute_startup_preflight(&cli).expect_err("strict dry-run should fail");
    assert!(error
        .to_string()
        .contains("events dry run gate: status=fail"));
    assert!(error.to_string().contains("max_error_rows_exceeded"));
}

#[test]
fn integration_execute_startup_preflight_events_dry_run_max_execute_rows_fails() {
    let temp = tempdir().expect("tempdir");
    let mut cli = test_cli();
    set_workspace_tau_paths(&mut cli, temp.path());
    cli.events_dry_run = true;
    cli.events_dry_run_max_execute_rows = Some(1);

    std::fs::create_dir_all(&cli.events_dir).expect("create events dir");
    std::fs::write(
        cli.events_dir.join("dry-run-a.json"),
        r#"{
  "id": "dry-run-a",
  "channel": "slack/C111",
  "prompt": "a",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write first dry-run event");
    std::fs::write(
        cli.events_dir.join("dry-run-b.json"),
        r#"{
  "id": "dry-run-b",
  "channel": "slack/C222",
  "prompt": "b",
  "schedule": {"type":"immediate"},
  "enabled": true
}
"#,
    )
    .expect("write second dry-run event");

    let error = execute_startup_preflight(&cli).expect_err("max execute threshold should fail");
    assert!(error
        .to_string()
        .contains("events dry run gate: status=fail"));
    assert!(error.to_string().contains("max_execute_rows_exceeded"));
}

#[test]
fn session_repair_command_runs_successfully() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");
    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::user("hello")])
        .expect("append");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store
        .lineage_messages(store.head_id())
        .expect("lineage should resolve");
    agent.replace_messages(lineage);

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(2),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/session-repair",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("repair command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(agent.messages().len(), 2);
}

#[test]
fn session_compact_command_prunes_inactive_branch() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session-compact.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append")
        .expect("root");
    let head = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("main-q"),
                tau_ai::Message::assistant_text("main-a"),
            ],
        )
        .expect("append")
        .expect("main head");
    store
        .append_messages(Some(root), &[tau_ai::Message::user("branch-q")])
        .expect("append branch");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let lineage = store
        .lineage_messages(Some(head))
        .expect("lineage should resolve");
    agent.replace_messages(lineage);

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/session-compact",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("compact command should succeed");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 3);
    assert_eq!(runtime.store.branch_tips().len(), 1);
    assert_eq!(runtime.store.branch_tips()[0].id, head);
    assert_eq!(agent.messages().len(), 3);
}

#[test]
fn integration_initialize_session_applies_lock_timeout_policy() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("locked-session.jsonl");
    let lock_path = session_path.with_extension("lock");
    std::fs::write(&lock_path, "locked").expect("write lock");

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_lock_wait_ms = 120;
    cli.session_lock_stale_ms = 0;
    let start = Instant::now();

    let error = initialize_session(
        &cli.session,
        cli.session_lock_wait_ms,
        cli.session_lock_stale_ms,
        cli.branch_from,
        "sys",
    )
    .expect_err("initialization should fail when lock persists");
    assert!(error.to_string().contains("timed out acquiring lock"));
    assert!(start.elapsed() < Duration::from_secs(2));

    std::fs::remove_file(lock_path).expect("cleanup lock");
}

#[test]
fn functional_initialize_session_reclaims_stale_lock_when_enabled() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("stale-lock-session.jsonl");
    let lock_path = session_path.with_extension("lock");
    std::fs::write(&lock_path, "stale").expect("write lock");
    std::thread::sleep(Duration::from_millis(30));

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_lock_wait_ms = 1_000;
    cli.session_lock_stale_ms = 10;
    let outcome = initialize_session(
        &cli.session,
        cli.session_lock_wait_ms,
        cli.session_lock_stale_ms,
        cli.branch_from,
        "sys",
    )
    .expect("initialization should reclaim stale lock");
    assert_eq!(outcome.runtime.store.entries().len(), 1);
    assert!(!lock_path.exists());
}

#[test]
fn unit_parse_sandbox_command_tokens_supports_shell_words_and_placeholders() {
    let tokens = parse_sandbox_command_tokens(&[
        "bwrap".to_string(),
        "--bind".to_string(),
        "\"{cwd}\"".to_string(),
        "{cwd}".to_string(),
        "{shell}".to_string(),
        "{command}".to_string(),
    ])
    .expect("parse should succeed");

    assert_eq!(
        tokens,
        vec![
            "bwrap".to_string(),
            "--bind".to_string(),
            "{cwd}".to_string(),
            "{cwd}".to_string(),
            "{shell}".to_string(),
            "{command}".to_string(),
        ]
    );
}

#[test]
fn regression_parse_sandbox_command_tokens_rejects_invalid_quotes() {
    let error = parse_sandbox_command_tokens(&["\"unterminated".to_string()])
        .expect_err("parse should fail");
    assert!(error
        .to_string()
        .contains("invalid --os-sandbox-command token"));
}

#[test]
fn build_tool_policy_includes_cwd_and_custom_root() {
    let mut cli = test_cli();
    cli.allow_path = vec![PathBuf::from("/tmp")];

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.allowed_roots.len() >= 2);
    assert_eq!(policy.bash_timeout_ms, 500);
    assert_eq!(policy.max_command_output_bytes, 1024);
    assert_eq!(policy.max_file_read_bytes, 2048);
    assert_eq!(policy.max_file_write_bytes, 2048);
    assert_eq!(policy.max_command_length, 4096);
    assert!(policy.allow_command_newlines);
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Off);
    assert_eq!(
        tool_policy_to_json(&policy)["os_sandbox_policy_mode"],
        "best-effort"
    );
    assert!(policy.os_sandbox_command.is_empty());
    assert_eq!(policy.http_timeout_ms, 20_000);
    assert_eq!(policy.http_max_response_bytes, 256_000);
    assert_eq!(policy.http_max_redirects, 5);
    assert!(!policy.http_allow_http);
    assert!(!policy.http_allow_private_network);
    assert!(policy.enforce_regular_files);
    assert_eq!(policy.policy_preset, ToolPolicyPreset::Balanced);
    assert!(!policy.bash_dry_run);
    assert!(!policy.tool_policy_trace);
    assert!(policy.extension_policy_override_root.is_none());
}

#[test]
fn unit_tool_policy_to_json_includes_key_limits_and_modes() {
    let mut cli = test_cli();
    cli.bash_profile = CliBashProfile::Strict;
    cli.os_sandbox_mode = CliOsSandboxMode::Auto;
    cli.max_file_write_bytes = 4096;
    cli.extension_runtime_hooks = true;
    cli.extension_runtime_root = PathBuf::from("/tmp/policy-overrides");

    let policy = build_tool_policy(&cli).expect("policy should build");
    let payload = tool_policy_to_json(&policy);
    assert_eq!(payload["schema_version"], 7);
    assert_eq!(payload["preset"], "balanced");
    assert_eq!(payload["bash_profile"], "strict");
    assert_eq!(payload["os_sandbox_mode"], "auto");
    assert_eq!(payload["os_sandbox_policy_mode"], "best-effort");
    assert_eq!(payload["memory_search_default_limit"], 5);
    assert_eq!(payload["memory_search_max_limit"], 50);
    assert_eq!(payload["memory_embedding_dimensions"], 128);
    let min_similarity = payload["memory_min_similarity"]
        .as_f64()
        .expect("memory_min_similarity as f64");
    assert!((min_similarity - 0.55).abs() < 1e-6);
    assert_eq!(payload["http_timeout_ms"], 20_000);
    assert_eq!(payload["http_max_response_bytes"], 256_000);
    assert_eq!(payload["http_max_redirects"], 5);
    assert_eq!(payload["http_allow_http"], false);
    assert_eq!(payload["http_allow_private_network"], false);
    assert_eq!(payload["max_file_write_bytes"], 4096);
    assert_eq!(payload["enforce_regular_files"], true);
    assert_eq!(payload["bash_dry_run"], false);
    assert_eq!(payload["tool_policy_trace"], false);
    assert_eq!(
        payload["extension_policy_override_root"],
        "/tmp/policy-overrides"
    );
    assert_eq!(payload["tool_rate_limit"]["max_requests"], 120);
    assert_eq!(payload["tool_rate_limit"]["window_ms"], 60000);
    assert_eq!(payload["tool_rate_limit"]["exceeded_behavior"], "reject");
}

#[test]
fn functional_build_tool_policy_hardened_preset_applies_hardened_defaults() {
    let mut cli = test_cli();
    cli.bash_timeout_ms = 120_000;
    cli.max_tool_output_bytes = 16_000;
    cli.max_file_read_bytes = 1_000_000;
    cli.max_file_write_bytes = 1_000_000;
    cli.max_command_length = 4_096;
    cli.allow_command_newlines = false;
    cli.bash_profile = CliBashProfile::Balanced;
    cli.os_sandbox_mode = CliOsSandboxMode::Off;
    cli.enforce_regular_files = true;
    cli.tool_policy_preset = CliToolPolicyPreset::Hardened;

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
    assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
    assert_eq!(policy.max_command_length, 1_024);
    assert_eq!(policy.max_command_output_bytes, 4_000);
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Force);
    assert_eq!(
        tool_policy_to_json(&policy)["os_sandbox_policy_mode"],
        "required"
    );
    assert_eq!(policy.tool_rate_limit_max_requests, 30);
    assert_eq!(policy.tool_rate_limit_window_ms, 60_000);
}

#[test]
fn regression_build_tool_policy_explicit_profile_overrides_preset_profile() {
    let mut cli = test_cli();
    cli.bash_timeout_ms = 120_000;
    cli.max_tool_output_bytes = 16_000;
    cli.max_file_read_bytes = 1_000_000;
    cli.max_file_write_bytes = 1_000_000;
    cli.max_command_length = 4_096;
    cli.allow_command_newlines = false;
    cli.os_sandbox_mode = CliOsSandboxMode::Off;
    cli.enforce_regular_files = true;
    cli.tool_policy_preset = CliToolPolicyPreset::Hardened;
    cli.bash_profile = CliBashProfile::Permissive;

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.policy_preset, ToolPolicyPreset::Hardened);
    assert_eq!(policy.bash_profile, BashCommandProfile::Permissive);
    assert!(policy.allowed_commands.is_empty());
}

#[test]
fn functional_build_tool_policy_enables_trace_when_flag_set() {
    let mut cli = test_cli();
    cli.tool_policy_trace = true;
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.tool_policy_trace);
}

#[test]
fn functional_build_tool_policy_enables_extension_policy_override_with_runtime_hooks() {
    let mut cli = test_cli();
    cli.extension_runtime_hooks = true;
    cli.extension_runtime_root = PathBuf::from("/tmp/extensions-runtime");
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(
        policy.extension_policy_override_root.as_deref(),
        Some(Path::new("/tmp/extensions-runtime"))
    );
}

#[test]
fn functional_build_tool_policy_applies_strict_profile_and_custom_allowlist() {
    let mut cli = test_cli();
    cli.bash_profile = CliBashProfile::Strict;
    cli.allow_command = vec!["python".to_string(), "cargo-nextest*".to_string()];

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.bash_profile, BashCommandProfile::Strict);
    assert!(policy.allowed_commands.contains(&"python".to_string()));
    assert!(policy
        .allowed_commands
        .contains(&"cargo-nextest*".to_string()));
    assert!(!policy.allowed_commands.contains(&"rm".to_string()));
}

#[test]
fn regression_build_tool_policy_permissive_profile_disables_allowlist() {
    let mut cli = test_cli();
    cli.bash_profile = CliBashProfile::Permissive;
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.allowed_commands.is_empty());
}

#[test]
fn regression_build_tool_policy_keeps_policy_override_disabled_without_runtime_hooks() {
    let mut cli = test_cli();
    cli.extension_runtime_root = PathBuf::from("/tmp/extensions-runtime");
    let policy = build_tool_policy(&cli).expect("policy should build");
    assert!(policy.extension_policy_override_root.is_none());
}

#[test]
fn functional_build_tool_policy_applies_sandbox_and_regular_file_settings() {
    let mut cli = test_cli();
    cli.os_sandbox_mode = CliOsSandboxMode::Auto;
    cli.os_sandbox_command = vec![
        "sandbox-run".to_string(),
        "--cwd".to_string(),
        "{cwd}".to_string(),
    ];
    cli.max_file_write_bytes = 4096;
    cli.enforce_regular_files = false;

    let policy = build_tool_policy(&cli).expect("policy should build");
    assert_eq!(policy.os_sandbox_mode, OsSandboxMode::Auto);
    assert_eq!(
        policy.os_sandbox_command,
        vec![
            "sandbox-run".to_string(),
            "--cwd".to_string(),
            "{cwd}".to_string()
        ]
    );
    assert_eq!(policy.max_file_write_bytes, 4096);
    assert!(!policy.enforce_regular_files);
}

#[test]
fn functional_build_tool_policy_applies_http_controls() {
    let mut cli = test_cli();
    cli.http_timeout_ms = 7500;
    cli.http_max_response_bytes = 12_345;
    cli.http_max_redirects = 2;
    cli.http_allow_http = true;
    cli.http_allow_private_network = true;

    let policy = build_tool_policy(&cli).expect("policy should build");
    let payload = tool_policy_to_json(&policy);
    assert_eq!(policy.http_timeout_ms, 7500);
    assert_eq!(policy.http_max_response_bytes, 12_345);
    assert_eq!(policy.http_max_redirects, 2);
    assert!(policy.http_allow_http);
    assert!(policy.http_allow_private_network);
    assert_eq!(payload["http_timeout_ms"], 7500);
    assert_eq!(payload["http_max_response_bytes"], 12_345);
    assert_eq!(payload["http_max_redirects"], 2);
    assert_eq!(payload["http_allow_http"], true);
    assert_eq!(payload["http_allow_private_network"], true);
}
