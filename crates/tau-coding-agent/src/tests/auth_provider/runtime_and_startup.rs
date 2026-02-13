use super::super::{
    apply_trust_root_mutations, build_tool_policy, default_skills_lock_path,
    discover_extension_runtime_registrations, execute_rpc_capabilities_command,
    execute_rpc_dispatch_frame_command, execute_rpc_dispatch_ndjson_command,
    execute_rpc_serve_ndjson_command, execute_rpc_validate_frame_command,
    execute_startup_preflight, handle_command, handle_command_with_session_import_mode,
    initialize_session, load_multi_agent_route_table, parse_numbered_plan_steps,
    parse_sandbox_command_tokens, parse_trust_rotation_spec, parse_trusted_root_spec, pending,
    percentile_duration_ms, ready, register_extension_tools,
    register_runtime_extension_tool_hook_subscriber, render_audit_summary,
    resolve_skill_trust_roots, resolve_system_prompt, restore_env_vars, rpc_capabilities_payload,
    run_plan_first_prompt, run_plan_first_prompt_with_policy_context,
    run_plan_first_prompt_with_policy_context_and_routing, run_prompt_with_cancellation,
    set_workspace_tau_paths, skills_command_config, snapshot_env_vars, stream_text_chunks,
    summarize_audit_file, tempdir, test_auth_command_config, test_cli, test_profile_defaults,
    test_render_options, test_tool_policy_json, tool_audit_event_json, tool_policy_to_json,
    validate_rpc_frame_file, validate_session_file, Agent, AgentConfig, AgentEvent, Arc,
    AsyncMutex, BashCommandProfile, ChatResponse, ChatUsage, CliBashProfile, CliDaemonProfile,
    CliEventTemplateSchedule, CliGatewayOpenResponsesAuthMode, CliGatewayRemoteProfile,
    CliMultiChannelOutboundMode, CliMultiChannelTransport, CliOsSandboxMode, CliToolPolicyPreset,
    CommandAction, ContentBlock, Duration, HashMap, Instant, Message, MessageRole, ModelCatalog,
    MultiAgentRouteTable, NoopClient, OsSandboxMode, Path, PathBuf, PromptRunStatus,
    PromptTelemetryLogger, QueueClient, RenderOptions, RuntimeExtensionHooksConfig, SequenceClient,
    SessionImportMode, SessionRuntime, SessionStore, SlowClient, SuccessClient, TauAiError,
    ToolAuditLogger, ToolExecutionResult, ToolPolicyPreset, TrustedRootRecord, VecDeque,
    AUTH_ENV_TEST_LOCK,
};
use super::{make_script_executable, write_route_table_fixture};

#[test]
fn unit_rpc_capabilities_payload_includes_protocol_and_capabilities() {
    let payload = rpc_capabilities_payload();
    assert_eq!(payload["schema_version"].as_u64(), Some(1));
    assert_eq!(payload["protocol_version"].as_str(), Some("0.1.0"));
    let capabilities = payload["capabilities"]
        .as_array()
        .expect("capabilities should be array");
    assert!(capabilities.iter().any(|entry| entry == "run.start"));
    assert!(capabilities.iter().any(|entry| entry == "run.cancel"));
}

#[test]
fn functional_execute_rpc_capabilities_command_succeeds_when_enabled() {
    let mut cli = test_cli();
    cli.rpc_capabilities = true;
    execute_rpc_capabilities_command(&cli).expect("rpc capabilities command should succeed");
}

#[test]
fn regression_execute_rpc_capabilities_command_is_noop_when_disabled() {
    let cli = test_cli();
    execute_rpc_capabilities_command(&cli).expect("disabled rpc capabilities should be noop");
}

#[test]
fn unit_validate_rpc_frame_file_parses_supported_frame_shape() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-1",
  "kind": "run.start",
  "payload": {"prompt":"hello"}
}"#,
    )
    .expect("write frame");
    let frame = validate_rpc_frame_file(&frame_path).expect("validate frame");
    assert_eq!(frame.request_id, "req-1");
    assert_eq!(frame.payload.len(), 1);
}

#[test]
fn functional_execute_rpc_validate_frame_command_succeeds_for_valid_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-cancel",
  "kind": "run.cancel",
  "payload": {"run_id":"run-1"}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_validate_frame_file = Some(frame_path);
    execute_rpc_validate_frame_command(&cli).expect("rpc frame validate should succeed");
}

#[test]
fn regression_execute_rpc_validate_frame_command_rejects_invalid_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-invalid",
  "kind": "run.unknown",
  "payload": {}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_validate_frame_file = Some(frame_path);
    let error = execute_rpc_validate_frame_command(&cli).expect_err("invalid kind should fail");
    assert!(error.to_string().contains("unsupported rpc frame kind"));
}

#[test]
fn functional_execute_rpc_dispatch_frame_command_succeeds_for_valid_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-dispatch",
  "kind": "run.cancel",
  "payload": {"run_id":"run-1"}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_dispatch_frame_file = Some(frame_path);
    execute_rpc_dispatch_frame_command(&cli).expect("rpc frame dispatch should succeed");
}

#[test]
fn regression_execute_rpc_dispatch_frame_command_rejects_missing_prompt() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    std::fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {}
}"#,
    )
    .expect("write frame");
    let mut cli = test_cli();
    cli.rpc_dispatch_frame_file = Some(frame_path);
    let error = execute_rpc_dispatch_frame_command(&cli).expect_err("missing prompt should fail");
    assert!(error
        .to_string()
        .contains("requires non-empty payload field 'prompt'"));
}

#[test]
fn functional_execute_rpc_dispatch_ndjson_command_succeeds_for_valid_frames() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frames.ndjson");
    std::fs::write(
        &frame_path,
        r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
"#,
    )
    .expect("write frames");
    let mut cli = test_cli();
    cli.rpc_dispatch_ndjson_file = Some(frame_path);
    execute_rpc_dispatch_ndjson_command(&cli).expect("rpc ndjson dispatch should succeed");
}

#[test]
fn regression_execute_rpc_dispatch_ndjson_command_fails_with_any_error_frame() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frames.ndjson");
    std::fs::write(
        &frame_path,
        r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
not-json
"#,
    )
    .expect("write frames");
    let mut cli = test_cli();
    cli.rpc_dispatch_ndjson_file = Some(frame_path);
    let error = execute_rpc_dispatch_ndjson_command(&cli)
        .expect_err("mixed ndjson frames should return an error");
    assert!(error
        .to_string()
        .contains("rpc ndjson dispatch completed with 1 error frame(s)"));
}

#[test]
fn regression_execute_rpc_serve_ndjson_command_is_noop_when_disabled() {
    let cli = test_cli();
    execute_rpc_serve_ndjson_command(&cli).expect("disabled rpc ndjson serve should be noop");
}

#[test]
fn unit_resolve_system_prompt_uses_inline_value_when_file_is_unset() {
    let mut cli = test_cli();
    cli.system_prompt = "inline system".to_string();

    let system_prompt = resolve_system_prompt(&cli).expect("resolve system prompt");
    assert_eq!(system_prompt, "inline system");
}

#[test]
fn functional_resolve_system_prompt_reads_system_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("system.txt");
    std::fs::write(&prompt_path, "system from file").expect("write prompt");

    let mut cli = test_cli();
    cli.system_prompt_file = Some(prompt_path);

    let system_prompt = resolve_system_prompt(&cli).expect("resolve system prompt");
    assert_eq!(system_prompt, "system from file");
}

#[test]
fn regression_resolve_system_prompt_rejects_empty_system_prompt_file() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("system.txt");
    std::fs::write(&prompt_path, "\n\t  ").expect("write prompt");

    let mut cli = test_cli();
    cli.system_prompt_file = Some(prompt_path.clone());

    let error = resolve_system_prompt(&cli).expect_err("empty system prompt should fail");
    assert!(error.to_string().contains(&format!(
        "system prompt file {} is empty",
        prompt_path.display()
    )));
}

#[test]
fn pathbuf_from_cli_default_is_relative() {
    let path = PathBuf::from(".tau/sessions/default.jsonl");
    assert!(!path.is_absolute());
}

#[test]
fn unit_parse_trusted_root_spec_accepts_key_id_and_base64() {
    let parsed = parse_trusted_root_spec("root=ZmFrZS1rZXk=").expect("parse root");
    assert_eq!(parsed.id, "root");
    assert_eq!(parsed.public_key, "ZmFrZS1rZXk=");
}

#[test]
fn regression_parse_trusted_root_spec_rejects_invalid_shapes() {
    let error = parse_trusted_root_spec("missing-separator").expect_err("should fail");
    assert!(error.to_string().contains("expected key_id=base64_key"));
}

#[test]
fn unit_parse_trust_rotation_spec_accepts_old_and_new_key() {
    let (old_id, new_key) = parse_trust_rotation_spec("old:new=YQ==").expect("rotation spec parse");
    assert_eq!(old_id, "old");
    assert_eq!(new_key.id, "new");
    assert_eq!(new_key.public_key, "YQ==");
}

#[test]
fn regression_parse_trust_rotation_spec_rejects_invalid_shapes() {
    let error = parse_trust_rotation_spec("invalid-shape").expect_err("should fail");
    assert!(error
        .to_string()
        .contains("expected old_id:new_id=base64_key"));
}

#[test]
fn functional_apply_trust_root_mutations_add_revoke_and_rotate() {
    let mut records = vec![TrustedRootRecord {
        id: "old".to_string(),
        public_key: "YQ==".to_string(),
        revoked: false,
        expires_unix: None,
        rotated_from: None,
    }];
    let mut cli = test_cli();
    cli.skill_trust_add = vec!["extra=Yg==".to_string()];
    cli.skill_trust_revoke = vec!["extra".to_string()];
    cli.skill_trust_rotate = vec!["old:new=Yw==".to_string()];

    let report = apply_trust_root_mutations(&mut records, &cli).expect("mutate");
    assert_eq!(report.added, 2);
    assert_eq!(report.updated, 0);
    assert_eq!(report.revoked, 1);
    assert_eq!(report.rotated, 1);

    let old = records
        .iter()
        .find(|record| record.id == "old")
        .expect("old");
    let new = records
        .iter()
        .find(|record| record.id == "new")
        .expect("new");
    let extra = records
        .iter()
        .find(|record| record.id == "extra")
        .expect("extra");
    assert!(old.revoked);
    assert_eq!(new.rotated_from.as_deref(), Some("old"));
    assert!(extra.revoked);
}

#[test]
fn functional_resolve_skill_trust_roots_loads_inline_and_file_entries() {
    let temp = tempdir().expect("tempdir");
    let roots_file = temp.path().join("roots.json");
    std::fs::write(
        &roots_file,
        r#"{"roots":[{"id":"file-root","public_key":"YQ=="}]}"#,
    )
    .expect("write roots");

    let mut cli = test_cli();
    cli.skill_trust_root = vec!["inline-root=Yg==".to_string()];
    cli.skill_trust_root_file = Some(roots_file);

    let roots = resolve_skill_trust_roots(&cli).expect("resolve roots");
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].id, "inline-root");
    assert_eq!(roots[1].id, "file-root");
}

#[test]
fn integration_resolve_skill_trust_roots_applies_mutations_and_persists_file() {
    let temp = tempdir().expect("tempdir");
    let roots_file = temp.path().join("roots.json");
    std::fs::write(
        &roots_file,
        r#"{"roots":[{"id":"old","public_key":"YQ=="}]}"#,
    )
    .expect("write roots");

    let mut cli = test_cli();
    cli.skill_trust_root_file = Some(roots_file.clone());
    cli.skill_trust_rotate = vec!["old:new=Yg==".to_string()];

    let roots = resolve_skill_trust_roots(&cli).expect("resolve roots");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].id, "new");

    let raw = std::fs::read_to_string(&roots_file).expect("read persisted");
    assert!(raw.contains("\"id\": \"old\""));
    assert!(raw.contains("\"revoked\": true"));
    assert!(raw.contains("\"id\": \"new\""));
}

#[test]
fn regression_resolve_skill_trust_roots_requires_file_for_mutations() {
    let mut cli = test_cli();
    cli.skill_trust_add = vec!["root=YQ==".to_string()];
    let error = resolve_skill_trust_roots(&cli).expect_err("should fail");
    assert!(error
        .to_string()
        .contains("--skill-trust-root-file is required"));
}

#[test]
fn unit_stream_text_chunks_preserve_whitespace_boundaries() {
    let chunks = stream_text_chunks("hello world\nnext");
    assert_eq!(chunks, vec!["hello ", "world\n", "next"]);
}

#[test]
fn regression_stream_text_chunks_handles_empty_and_single_word() {
    assert!(stream_text_chunks("").is_empty());
    assert_eq!(stream_text_chunks("token"), vec!["token"]);
}

#[test]
fn unit_tool_audit_event_json_for_start_has_expected_shape() {
    let mut starts = HashMap::new();
    let event = AgentEvent::ToolExecutionStart {
        tool_call_id: "call-1".to_string(),
        tool_name: "bash".to_string(),
        arguments: serde_json::json!({ "command": "pwd" }),
    };
    let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

    assert_eq!(payload["event"], "tool_execution_start");
    assert_eq!(payload["tool_call_id"], "call-1");
    assert_eq!(payload["tool_name"], "bash");
    assert!(payload["arguments_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(starts.contains_key("call-1"));
}

#[test]
fn unit_tool_audit_event_json_for_end_tracks_duration_and_error_state() {
    let mut starts = HashMap::new();
    starts.insert("call-2".to_string(), Instant::now());
    let event = AgentEvent::ToolExecutionEnd {
        tool_call_id: "call-2".to_string(),
        tool_name: "read".to_string(),
        result: ToolExecutionResult::error(serde_json::json!({ "error": "denied" })),
    };
    let payload = tool_audit_event_json(&event, &mut starts).expect("expected payload");

    assert_eq!(payload["event"], "tool_execution_end");
    assert_eq!(payload["tool_call_id"], "call-2");
    assert_eq!(payload["is_error"], true);
    assert!(payload["result_bytes"].as_u64().unwrap_or(0) > 0);
    assert!(payload["duration_ms"].is_number() || payload["duration_ms"].is_null());
    assert!(!starts.contains_key("call-2"));
}

#[test]
fn integration_tool_audit_logger_persists_jsonl_records() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("tool-audit.jsonl");
    let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");

    let start = AgentEvent::ToolExecutionStart {
        tool_call_id: "call-3".to_string(),
        tool_name: "write".to_string(),
        arguments: serde_json::json!({ "path": "out.txt", "content": "x" }),
    };
    logger.log_event(&start).expect("write start event");

    let end = AgentEvent::ToolExecutionEnd {
        tool_call_id: "call-3".to_string(),
        tool_name: "write".to_string(),
        result: ToolExecutionResult::ok(serde_json::json!({ "bytes_written": 1 })),
    };
    logger.log_event(&end).expect("write end event");

    let raw = std::fs::read_to_string(log_path).expect("read audit log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("parse first");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("parse second");
    assert_eq!(first["event"], "tool_execution_start");
    assert_eq!(second["event"], "tool_execution_end");
    assert_eq!(second["is_error"], false);
}

#[test]
fn unit_percentile_duration_ms_handles_empty_and_unsorted_values() {
    assert_eq!(percentile_duration_ms(&[], 50), 0);
    assert_eq!(percentile_duration_ms(&[9], 95), 9);
    assert_eq!(percentile_duration_ms(&[50, 10, 20, 40, 30], 50), 30);
    assert_eq!(percentile_duration_ms(&[50, 10, 20, 40, 30], 95), 50);
}

#[test]
fn functional_summarize_audit_file_aggregates_tool_and_provider_metrics() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("audit.jsonl");
    let rows = [
        serde_json::json!({
            "event": "tool_execution_end",
            "tool_name": "bash",
            "duration_ms": 12,
            "is_error": false
        }),
        serde_json::json!({
            "event": "tool_execution_end",
            "tool_name": "bash",
            "duration_ms": 32,
            "is_error": true
        }),
        serde_json::json!({
            "record_type": "prompt_telemetry_v1",
            "provider": "openai",
            "status": "completed",
            "success": true,
            "duration_ms": 100,
            "token_usage": {
                "input_tokens": 4,
                "output_tokens": 2,
                "total_tokens": 6
            }
        }),
        serde_json::json!({
            "record_type": "prompt_telemetry_v1",
            "provider": "openai",
            "status": "interrupted",
            "success": false,
            "duration_ms": 180,
            "token_usage": {
                "input_tokens": 1,
                "output_tokens": 1,
                "total_tokens": 2
            }
        }),
    ]
    .iter()
    .map(serde_json::Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    std::fs::write(&log_path, format!("{rows}\n")).expect("write audit log");

    let summary = summarize_audit_file(&log_path).expect("summary");
    assert_eq!(summary.record_count, 4);
    assert_eq!(summary.tool_event_count, 2);
    assert_eq!(summary.prompt_record_count, 2);

    let tool = summary.tools.get("bash").expect("tool aggregate");
    assert_eq!(tool.count, 2);
    assert_eq!(tool.error_count, 1);
    assert_eq!(percentile_duration_ms(&tool.durations_ms, 50), 12);
    assert_eq!(percentile_duration_ms(&tool.durations_ms, 95), 32);

    let provider = summary.providers.get("openai").expect("provider aggregate");
    assert_eq!(provider.count, 2);
    assert_eq!(provider.error_count, 1);
    assert_eq!(provider.input_tokens, 5);
    assert_eq!(provider.output_tokens, 3);
    assert_eq!(provider.total_tokens, 8);
}

#[test]
fn functional_render_audit_summary_includes_expected_sections() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("audit.jsonl");
    std::fs::write(&path, "").expect("write empty log");
    let summary = summarize_audit_file(path.as_path()).expect("empty summary should parse");
    let output = render_audit_summary(&path, &summary);
    assert!(output.contains("audit summary:"));
    assert!(output.contains("tool_breakdown:"));
    assert!(output.contains("provider_breakdown:"));
}

#[test]
fn integration_prompt_telemetry_logger_persists_completed_record() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("prompt-telemetry.jsonl");
    let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
        .expect("logger open");

    logger
        .log_event(&AgentEvent::AgentStart)
        .expect("agent start");
    logger
        .log_event(&AgentEvent::TurnEnd {
            turn: 1,
            tool_results: 0,
            request_duration_ms: 44,
            usage: ChatUsage {
                input_tokens: 4,
                output_tokens: 2,
                total_tokens: 6,
            },
            finish_reason: Some("stop".to_string()),
        })
        .expect("turn end");
    logger
        .log_event(&AgentEvent::AgentEnd { new_messages: 2 })
        .expect("agent end");

    let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let record: serde_json::Value = serde_json::from_str(lines[0]).expect("parse record");
    assert_eq!(record["record_type"], "prompt_telemetry_v1");
    assert_eq!(record["provider"], "openai");
    assert_eq!(record["model"], "gpt-4o-mini");
    assert_eq!(record["status"], "completed");
    assert_eq!(record["success"], true);
    assert_eq!(record["finish_reason"], "stop");
    assert_eq!(record["token_usage"]["total_tokens"], 6);
    assert_eq!(record["redaction_policy"]["prompt_content"], "omitted");
}

#[test]
fn regression_prompt_telemetry_logger_marks_interrupted_runs() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("prompt-telemetry.jsonl");
    let logger = PromptTelemetryLogger::open(log_path.clone(), "openai", "gpt-4o-mini")
        .expect("logger open");

    logger
        .log_event(&AgentEvent::AgentStart)
        .expect("first start");
    logger
        .log_event(&AgentEvent::TurnEnd {
            turn: 1,
            tool_results: 0,
            request_duration_ms: 11,
            usage: ChatUsage {
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
            },
            finish_reason: Some("length".to_string()),
        })
        .expect("first turn");
    logger
        .log_event(&AgentEvent::AgentStart)
        .expect("second start");
    logger
        .log_event(&AgentEvent::AgentEnd { new_messages: 1 })
        .expect("finalize second run");

    let raw = std::fs::read_to_string(log_path).expect("read telemetry log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);

    let first: serde_json::Value = serde_json::from_str(lines[0]).expect("first record");
    let second: serde_json::Value = serde_json::from_str(lines[1]).expect("second record");
    assert_eq!(first["status"], "interrupted");
    assert_eq!(first["success"], false);
    assert_eq!(second["status"], "completed");
    assert_eq!(second["success"], true);
}

#[test]
fn regression_summarize_audit_file_remains_compatible_with_tool_audit_logs() {
    let temp = tempdir().expect("tempdir");
    let log_path = temp.path().join("tool-audit.jsonl");
    let logger = ToolAuditLogger::open(log_path.clone()).expect("logger should open");
    logger
        .log_event(&AgentEvent::ToolExecutionStart {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            arguments: serde_json::json!({ "path": "README.md" }),
        })
        .expect("start");
    logger
        .log_event(&AgentEvent::ToolExecutionEnd {
            tool_call_id: "call-1".to_string(),
            tool_name: "read".to_string(),
            result: ToolExecutionResult::ok(serde_json::json!({ "ok": true })),
        })
        .expect("end");

    let summary = summarize_audit_file(&log_path).expect("summarize");
    assert_eq!(summary.record_count, 2);
    assert_eq!(summary.tool_event_count, 1);
    assert_eq!(summary.prompt_record_count, 0);
    assert!(summary.providers.is_empty());
}

#[tokio::test]
async fn integration_run_prompt_with_cancellation_completes_when_not_cancelled() {
    let mut agent = Agent::new(Arc::new(SuccessClient), AgentConfig::default());
    let mut runtime = None;

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "hello",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should complete");

    assert_eq!(status, PromptRunStatus::Completed);
    assert_eq!(agent.messages().len(), 3);
    assert_eq!(agent.messages()[1].role, MessageRole::User);
    assert_eq!(agent.messages()[2].role, MessageRole::Assistant);
}

#[tokio::test]
async fn functional_run_prompt_with_cancellation_stream_fallback_avoids_blocking_delay() {
    let mut agent = Agent::new(Arc::new(SuccessClient), AgentConfig::default());
    let mut runtime = None;
    let started = Instant::now();

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "hello",
        0,
        pending::<()>(),
        RenderOptions {
            stream_output: true,
            stream_delay_ms: 300,
        },
    )
    .await
    .expect("prompt should complete");

    assert_eq!(status, PromptRunStatus::Completed);
    assert!(
        started.elapsed() < Duration::from_millis(260),
        "fallback render path should not block on configured stream delay"
    );
}

#[tokio::test]
async fn integration_tool_hook_subscriber_dispatches_pre_and_post_tool_call_hooks() {
    let temp = tempdir().expect("tempdir");
    let read_target = temp.path().join("README.md");
    std::fs::write(&read_target, "hello from test").expect("write read target");

    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("tool-observer");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let request_log = extension_dir.join("requests.ndjson");
    let hook_script = extension_dir.join("hook.sh");
    std::fs::write(
        &hook_script,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\ninput=\"$(cat)\"\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"ok\":true}}'\n",
            request_log.display()
        ),
    )
    .expect("write hook script");
    make_script_executable(&hook_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "tool-observer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["pre-tool-call", "post-tool-call"],
  "permissions": ["run-commands"],
  "timeout_ms": 5000
}"#,
    )
    .expect("write extension manifest");

    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({
                    "path": read_target.display().to_string(),
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: Message::assistant_text("tool flow complete"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig::default(),
    );
    let policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    crate::tools::register_builtin_tools(&mut agent, policy);

    let hook_config = RuntimeExtensionHooksConfig {
        enabled: true,
        root: extension_root.clone(),
    };
    register_runtime_extension_tool_hook_subscriber(&mut agent, &hook_config);

    let mut runtime = None;
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "read the file",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should succeed");
    assert_eq!(status, PromptRunStatus::Completed);

    let raw = std::fs::read_to_string(&request_log).expect("read request log");
    let rows = raw
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("json row"))
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["hook"], "pre-tool-call");
    assert_eq!(rows[1]["hook"], "post-tool-call");
    assert_eq!(rows[0]["payload"]["schema_version"], 1);
    assert_eq!(rows[1]["payload"]["schema_version"], 1);
    assert_eq!(rows[0]["payload"]["hook"], "pre-tool-call");
    assert_eq!(rows[1]["payload"]["hook"], "post-tool-call");
    assert!(rows[0]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert!(rows[1]["payload"]["emitted_at_ms"].as_u64().is_some());
    assert_eq!(rows[0]["payload"]["data"]["tool_name"], "read");
    assert_eq!(rows[1]["payload"]["data"]["tool_name"], "read");
    assert_eq!(rows[1]["payload"]["data"]["result"]["is_error"], false);
}

#[tokio::test]
async fn regression_tool_hook_subscriber_timeout_does_not_fail_prompt() {
    let temp = tempdir().expect("tempdir");
    let read_target = temp.path().join("README.md");
    std::fs::write(&read_target, "hello from timeout test").expect("write read target");

    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("slow-tool-observer");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let hook_script = extension_dir.join("hook.sh");
    std::fs::write(
        &hook_script,
        "#!/usr/bin/env bash\nsleep 1\nprintf '{\"ok\":true}'\n",
    )
    .expect("write hook script");
    make_script_executable(&hook_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "slow-tool-observer",
  "version": "0.1.0",
  "runtime": "process",
  "entrypoint": "hook.sh",
  "hooks": ["pre-tool-call", "post-tool-call"],
  "permissions": ["run-commands"],
  "timeout_ms": 20
}"#,
    )
    .expect("write extension manifest");

    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({
                    "path": read_target.display().to_string(),
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: Message::assistant_text("tool flow survived timeout"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig::default(),
    );
    let policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    crate::tools::register_builtin_tools(&mut agent, policy);

    let hook_config = RuntimeExtensionHooksConfig {
        enabled: true,
        root: extension_root,
    };
    register_runtime_extension_tool_hook_subscriber(&mut agent, &hook_config);

    let mut runtime = None;
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "read the file",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should still succeed when hook times out");
    assert_eq!(status, PromptRunStatus::Completed);
    assert_eq!(
        agent
            .messages()
            .last()
            .expect("assistant response")
            .text_content(),
        "tool flow survived timeout"
    );
}

#[tokio::test]
async fn integration_extension_registered_tool_executes_in_prompt_loop() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("tool-registry");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let request_log = extension_dir.join("tool-request.ndjson");
    let tool_script = extension_dir.join("tool.sh");
    std::fs::write(
        &tool_script,
        format!(
            "#!/bin/sh\nread -r input\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"content\":{{\"status\":\"ok\",\"source\":\"extension\"}},\"is_error\":false}}'\n",
            request_log.display()
        ),
    )
    .expect("write tool script");
    make_script_executable(&tool_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "tool-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "tool.sh",
  "permissions": ["run-commands"],
  "tools": [
    {
      "name": "issue_triage",
      "description": "triage issue labels",
      "parameters": {
        "type": "object",
        "properties": {
          "title": {"type":"string"}
        },
        "required": ["title"],
        "additionalProperties": false
      }
    }
  ]
}"#,
    )
    .expect("write extension manifest");

    let registrations =
        discover_extension_runtime_registrations(&extension_root, crate::commands::COMMAND_NAMES);
    assert_eq!(registrations.registered_tools.len(), 1);

    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "issue_triage".to_string(),
                arguments: serde_json::json!({
                    "title": "bug report",
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: Message::assistant_text("extension tool complete"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig::default(),
    );
    let policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    crate::tools::register_builtin_tools(&mut agent, policy);
    register_extension_tools(&mut agent, &registrations.registered_tools);

    let mut runtime = None;
    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "run extension tool",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should succeed");
    assert_eq!(status, PromptRunStatus::Completed);

    let raw = std::fs::read_to_string(&request_log).expect("read request log");
    let payload: serde_json::Value =
        serde_json::from_str(raw.lines().next().expect("one row")).expect("request row json");
    assert_eq!(payload["hook"], "tool-call");
    assert_eq!(payload["payload"]["kind"], "tool-call");
    assert_eq!(payload["payload"]["tool"]["name"], "issue_triage");
    assert_eq!(
        payload["payload"]["tool"]["arguments"]["title"],
        "bug report"
    );
    assert!(agent.messages().iter().any(|message| {
        message.role == MessageRole::Tool
            && message.text_content().contains("\"status\": \"ok\"")
            && message.text_content().contains("\"source\": \"extension\"")
    }));
}

#[test]
fn integration_handle_command_dispatches_extension_registered_command() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("command-registry");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let request_log = extension_dir.join("command-request.ndjson");
    let command_script = extension_dir.join("command.sh");
    std::fs::write(
        &command_script,
        format!(
            "#!/bin/sh\nread -r input\nprintf '%s\\n' \"$input\" >> \"{}\"\nprintf '{{\"output\":\"triage complete\",\"action\":\"continue\"}}'\n",
            request_log.display()
        ),
    )
    .expect("write command script");
    make_script_executable(&command_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "command-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "command.sh",
  "permissions": ["run-commands"],
  "commands": [
    {
      "name": "/triage-now",
      "description": "run triage command"
    }
  ]
}"#,
    )
    .expect("write extension manifest");
    let registrations =
        discover_extension_runtime_registrations(&extension_root, crate::commands::COMMAND_NAMES);
    assert_eq!(registrations.registered_commands.len(), 1);

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("create skills dir");
    let lock_path = default_skills_lock_path(&skills_dir);
    std::fs::write(&lock_path, "{}\n").expect("write lock path");
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/triage-now 42",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &ModelCatalog::built_in(),
        &registrations.registered_commands,
    )
    .expect("command should execute");
    assert_eq!(action, CommandAction::Continue);

    let raw = std::fs::read_to_string(&request_log).expect("read request log");
    let payload: serde_json::Value =
        serde_json::from_str(raw.lines().next().expect("one row")).expect("request row json");
    assert_eq!(payload["hook"], "command-call");
    assert_eq!(payload["payload"]["kind"], "command-call");
    assert_eq!(payload["payload"]["command"]["name"], "/triage-now");
    assert_eq!(payload["payload"]["command"]["args"], "42");
}

#[test]
fn regression_handle_command_extension_failure_is_fail_isolated() {
    let temp = tempdir().expect("tempdir");
    let extension_root = temp.path().join("extensions");
    let extension_dir = extension_root.join("command-registry");
    std::fs::create_dir_all(&extension_dir).expect("create extension dir");
    let command_script = extension_dir.join("command.sh");
    std::fs::write(
        &command_script,
        "#!/bin/sh\nread -r _input\nprintf '{\"action\":123}'\n",
    )
    .expect("write command script");
    make_script_executable(&command_script);
    std::fs::write(
        extension_dir.join("extension.json"),
        r#"{
  "schema_version": 1,
  "id": "command-registry",
  "version": "1.0.0",
  "runtime": "process",
  "entrypoint": "command.sh",
  "permissions": ["run-commands"],
  "commands": [
    {
      "name": "/triage-now",
      "description": "run triage command"
    }
  ]
}"#,
    )
    .expect("write extension manifest");
    let registrations =
        discover_extension_runtime_registrations(&extension_root, crate::commands::COMMAND_NAMES);
    assert_eq!(registrations.registered_commands.len(), 1);

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let auth_command_config = test_auth_command_config();
    let skills_dir = temp.path().join("skills");
    std::fs::create_dir_all(&skills_dir).expect("create skills dir");
    let lock_path = default_skills_lock_path(&skills_dir);
    std::fs::write(&lock_path, "{}\n").expect("write lock path");
    let skills_command_config = skills_command_config(&skills_dir, &lock_path, None);

    let action = handle_command_with_session_import_mode(
        "/triage-now 42",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Merge,
        &profile_defaults,
        &skills_command_config,
        &auth_command_config,
        &ModelCatalog::built_in(),
        &registrations.registered_commands,
    )
    .expect("errors should be fail-isolated");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn unit_parse_numbered_plan_steps_accepts_deterministic_step_format() {
    let steps = parse_numbered_plan_steps("1. Gather context\n2) Implement fix\n3. Verify");
    assert_eq!(
        steps,
        vec![
            "Gather context".to_string(),
            "Implement fix".to_string(),
            "Verify".to_string(),
        ]
    );
}

#[tokio::test]
async fn integration_run_plan_first_prompt_with_routing_uses_distinct_delegated_roles() {
    let temp = tempdir().expect("tempdir");
    let route_table_path = temp.path().join("route-table.json");
    write_route_table_fixture(
        &route_table_path,
        r#"{
  "schema_version": 1,
  "roles": {
    "planner": { "prompt_suffix": "Plan with strict ordering." },
    "executor": { "prompt_suffix": "Execute only implementation steps." },
    "reviewer": { "prompt_suffix": "Focus on verification evidence." }
  },
  "planner": { "role": "planner" },
  "delegated": { "role": "executor" },
  "delegated_categories": {
    "verify": { "role": "reviewer" }
  },
  "review": { "role": "reviewer" }
}"#,
    );
    let route_table = load_multi_agent_route_table(&route_table_path).expect("load route table");

    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(ChatResponse {
                    message: Message::assistant_text("1. Apply patch\n2. Verify behavior"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
                Ok(ChatResponse {
                    message: Message::assistant_text("patch applied"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
                Ok(ChatResponse {
                    message: Message::assistant_text("verification complete"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
                Ok(ChatResponse {
                    message: Message::assistant_text("final delegated response"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    run_plan_first_prompt_with_policy_context_and_routing(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
        Some("preset=balanced;max_command_length=4096"),
        &route_table,
        None,
    )
    .await
    .expect("delegated routed execution should succeed");

    let user_prompts = agent
        .messages()
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .map(|message| message.text_content())
        .collect::<Vec<_>>();
    assert!(user_prompts
        .iter()
        .any(|prompt| prompt.contains("phase=planner") && prompt.contains("role=planner")));
    assert!(user_prompts.iter().any(|prompt| {
        prompt.contains("phase=delegated-step") && prompt.contains("role=executor")
    }));
    assert!(user_prompts.iter().any(|prompt| {
        prompt.contains("phase=delegated-step") && prompt.contains("role=reviewer")
    }));
    assert!(user_prompts
        .iter()
        .any(|prompt| prompt.contains("phase=review") && prompt.contains("role=reviewer")));
    assert_eq!(
        agent
            .messages()
            .last()
            .expect("assistant response")
            .text_content(),
        "final delegated response"
    );
}

#[tokio::test]
async fn functional_run_plan_first_prompt_with_routing_emits_fallback_trace_records() {
    let temp = tempdir().expect("tempdir");
    let route_table_path = temp.path().join("route-table.json");
    let telemetry_log = temp.path().join("telemetry.ndjson");
    write_route_table_fixture(
        &route_table_path,
        r#"{
  "schema_version": 1,
  "roles": {
    "planner-primary": {},
    "planner-fallback": {},
    "reviewer": {}
  },
  "planner": { "role": "planner-primary", "fallback_roles": ["planner-fallback"] },
  "delegated": { "role": "planner-fallback" },
  "review": { "role": "reviewer" }
}"#,
    );
    let route_table = load_multi_agent_route_table(&route_table_path).expect("load route table");

    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Err(TauAiError::InvalidResponse(
                    "planner primary failed".to_string(),
                )),
                Ok(ChatResponse {
                    message: Message::assistant_text("1. Inspect constraints\n2. Apply fix"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
                Ok(ChatResponse {
                    message: Message::assistant_text("final execution"),
                    finish_reason: Some("stop".to_string()),
                    usage: ChatUsage::default(),
                }),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    run_plan_first_prompt_with_policy_context_and_routing(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        false,
        None,
        &route_table,
        Some(telemetry_log.as_path()),
    )
    .await
    .expect("fallback planner route should recover");

    let telemetry = std::fs::read_to_string(&telemetry_log).expect("read telemetry log");
    assert!(telemetry.contains("\"record_type\":\"orchestrator_route_trace_v1\""));
    assert!(telemetry.contains("\"event\":\"fallback\""));
    assert!(telemetry.contains("\"decision\":\"retry\""));
    assert!(telemetry.contains("\"reason\":\"prompt_execution_error\""));
    assert!(telemetry.contains("\"phase\":\"planner\""));
}

#[tokio::test]
async fn regression_routed_orchestrator_default_profile_matches_legacy_behavior() {
    let legacy_responses = VecDeque::from([
        Ok(ChatResponse {
            message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }),
        Ok(ChatResponse {
            message: Message::assistant_text("legacy final"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }),
    ]);
    let routed_responses = VecDeque::from([
        Ok(ChatResponse {
            message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }),
        Ok(ChatResponse {
            message: Message::assistant_text("legacy final"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        }),
    ]);
    let mut legacy_agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(legacy_responses),
        }),
        AgentConfig::default(),
    );
    let mut routed_agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(routed_responses),
        }),
        AgentConfig::default(),
    );
    let mut legacy_runtime = None;
    let mut routed_runtime = None;

    run_plan_first_prompt(
        &mut legacy_agent,
        &mut legacy_runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        false,
    )
    .await
    .expect("legacy run should succeed");
    run_plan_first_prompt_with_policy_context_and_routing(
        &mut routed_agent,
        &mut routed_runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        false,
        None,
        &MultiAgentRouteTable::default(),
        None,
    )
    .await
    .expect("routed default run should succeed");

    assert_eq!(
        legacy_agent
            .messages()
            .last()
            .expect("legacy final")
            .text_content(),
        routed_agent
            .messages()
            .last()
            .expect("routed final")
            .text_content()
    );
}

#[tokio::test]
async fn functional_run_plan_first_prompt_executes_planner_then_executor() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text("final implementation response"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        2_048,
        false,
    )
    .await
    .expect("plan-first prompt should succeed");

    assert_eq!(agent.messages().len(), 5);
    assert_eq!(
        agent
            .messages()
            .last()
            .expect("assistant response")
            .text_content(),
        "final implementation response"
    );
}

#[tokio::test]
async fn functional_run_plan_first_prompt_delegate_steps_executes_and_consolidates() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_one = ChatResponse {
        message: Message::assistant_text("constraints reviewed"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_two = ChatResponse {
        message: Message::assistant_text("change applied"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let consolidation_response = ChatResponse {
        message: Message::assistant_text("final delegated response"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_step_one),
                Ok(delegated_step_two),
                Ok(consolidation_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
    )
    .await
    .expect("delegated plan-first prompt should succeed");

    let messages = agent.messages();
    assert_eq!(
        messages.last().expect("assistant response").text_content(),
        "final delegated response"
    );
    assert!(messages
        .iter()
        .any(|message| message.text_content() == "constraints reviewed"));
    assert!(messages
        .iter()
        .any(|message| message.text_content() == "change applied"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_with_policy_context_fails_when_context_missing() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt_with_policy_context(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
        None,
    )
    .await
    .expect_err("missing delegated policy context should fail closed");
    assert!(error
        .to_string()
        .contains("delegated policy inheritance context is unavailable"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_on_empty_step_output() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_empty_response = ChatResponse {
        message: Message::assistant_text(""),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_empty_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        true,
    )
    .await
    .expect_err("empty delegated output should fail");
    assert!(error
        .to_string()
        .contains("delegated step 1 produced no text output"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_when_step_count_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text(
            "1. Inspect constraints\n2. Apply change\n3. Verify output",
        ),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        2,
        512,
        512,
        1_024,
        true,
    )
    .await
    .expect_err("delegated step count above budget should fail");
    assert!(error.to_string().contains("delegated step budget exceeded"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_when_step_output_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_over_budget_response = ChatResponse {
        message: Message::assistant_text("over budget delegated output"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_over_budget_response),
                Ok(delegated_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        8,
        1_024,
        true,
    )
    .await
    .expect_err("oversized delegated output should fail");
    assert!(error
        .to_string()
        .contains("delegated step 1 response exceeded budget"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_delegate_steps_fails_when_total_output_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_one = ChatResponse {
        message: Message::assistant_text("step one"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let delegated_step_two = ChatResponse {
        message: Message::assistant_text("step two"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let consolidation_unused_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(delegated_step_one),
                Ok(delegated_step_two),
                Ok(consolidation_unused_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        64,
        12,
        true,
    )
    .await
    .expect_err("oversized delegated cumulative output should fail");
    assert!(error
        .to_string()
        .contains("delegated responses exceeded cumulative budget"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_rejects_overlong_plans_before_executor_phase() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Step one\n2. Step two\n3. Step three"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text("should not execute"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        2,
        2,
        512,
        512,
        1_024,
        false,
    )
    .await
    .expect_err("overlong plan should fail");
    assert!(error.to_string().contains("planner produced 3 steps"));
    assert!(!agent
        .messages()
        .iter()
        .any(|message| message.text_content() == "should not execute"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_fails_when_executor_output_is_empty() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text(""),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        512,
        512,
        1_024,
        false,
    )
    .await
    .expect_err("empty executor output should fail");
    assert!(error
        .to_string()
        .contains("executor produced no text output"));
}

#[tokio::test]
async fn regression_run_plan_first_prompt_fails_when_executor_output_exceeds_budget() {
    let planner_response = ChatResponse {
        message: Message::assistant_text("1. Inspect constraints\n2. Apply change"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let executor_response = ChatResponse {
        message: Message::assistant_text("final implementation response"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage::default(),
    };
    let mut agent = Agent::new(
        Arc::new(SequenceClient {
            outcomes: AsyncMutex::new(VecDeque::from([
                Ok(planner_response),
                Ok(executor_response),
            ])),
        }),
        AgentConfig::default(),
    );
    let mut runtime = None;

    let error = run_plan_first_prompt(
        &mut agent,
        &mut runtime,
        "ship feature",
        0,
        test_render_options(),
        4,
        4,
        8,
        512,
        1_024,
        false,
    )
    .await
    .expect_err("oversized executor output should fail");
    assert!(error
        .to_string()
        .contains("executor response exceeded budget"));
}

#[tokio::test]
async fn regression_run_prompt_with_cancellation_restores_agent_state() {
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());
    let initial_messages = agent.messages().to_vec();
    let mut runtime = None;

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "cancel me",
        0,
        ready(()),
        test_render_options(),
    )
    .await
    .expect("cancellation branch should succeed");

    assert_eq!(status, PromptRunStatus::Cancelled);
    assert_eq!(agent.messages().len(), initial_messages.len());
    assert_eq!(agent.messages()[0].role, initial_messages[0].role);
    assert_eq!(
        agent.messages()[0].text_content(),
        initial_messages[0].text_content()
    );
}

#[tokio::test]
async fn functional_run_prompt_with_timeout_restores_agent_state() {
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());
    let initial_messages = agent.messages().to_vec();
    let mut runtime = None;

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "timeout me",
        20,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("timeout branch should succeed");

    assert_eq!(status, PromptRunStatus::TimedOut);
    assert_eq!(agent.messages().len(), initial_messages.len());
    assert_eq!(
        agent.messages()[0].text_content(),
        initial_messages[0].text_content()
    );
}

#[tokio::test]
async fn integration_regression_cancellation_does_not_persist_partial_session_entries() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("cancel-session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let active_head = store
        .ensure_initialized("You are a helpful coding assistant.")
        .expect("initialize session");

    let mut runtime = Some(SessionRuntime { store, active_head });
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "cancel me",
        0,
        ready(()),
        test_render_options(),
    )
    .await
    .expect("cancelled prompt should succeed");

    assert_eq!(status, PromptRunStatus::Cancelled);
    assert_eq!(runtime.as_ref().expect("runtime").store.entries().len(), 1);

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 1);
}

#[tokio::test]
async fn integration_regression_timeout_does_not_persist_partial_session_entries() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("timeout-session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let active_head = store
        .ensure_initialized("You are a helpful coding assistant.")
        .expect("initialize session");

    let mut runtime = Some(SessionRuntime { store, active_head });
    let mut agent = Agent::new(Arc::new(SlowClient), AgentConfig::default());

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "timeout me",
        20,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("timed-out prompt should succeed");

    assert_eq!(status, PromptRunStatus::TimedOut);
    assert_eq!(runtime.as_ref().expect("runtime").store.entries().len(), 1);

    let reloaded = SessionStore::load(&path).expect("reload");
    assert_eq!(reloaded.entries().len(), 1);
}

#[tokio::test]
async fn integration_agent_bash_policy_blocks_overlong_commands() {
    let temp = tempdir().expect("tempdir");
    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({
                    "command": "printf",
                    "cwd": temp.path().display().to_string(),
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: tau_ai::Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let client = Arc::new(QueueClient {
        responses: AsyncMutex::new(responses),
    });
    let mut agent = Agent::new(client, AgentConfig::default());

    let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_command_length = 4;
    crate::tools::register_builtin_tools(&mut agent, policy);

    let new_messages = agent
        .prompt("run command")
        .await
        .expect("prompt should succeed");
    let tool_message = new_messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result should be present");

    assert!(tool_message.is_error);
    assert!(tool_message.text_content().contains("command is too long"));
}

#[tokio::test]
async fn integration_agent_write_policy_blocks_oversized_content() {
    let temp = tempdir().expect("tempdir");
    let target = temp.path().join("target.txt");
    let responses = VecDeque::from(vec![
        ChatResponse {
            message: tau_ai::Message::assistant_blocks(vec![ContentBlock::ToolCall {
                id: "call-1".to_string(),
                name: "write".to_string(),
                arguments: serde_json::json!({
                    "path": target,
                    "content": "hello",
                }),
            }]),
            finish_reason: Some("tool_calls".to_string()),
            usage: ChatUsage::default(),
        },
        ChatResponse {
            message: tau_ai::Message::assistant_text("done"),
            finish_reason: Some("stop".to_string()),
            usage: ChatUsage::default(),
        },
    ]);

    let client = Arc::new(QueueClient {
        responses: AsyncMutex::new(responses),
    });
    let mut agent = Agent::new(client, AgentConfig::default());

    let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.max_file_write_bytes = 4;
    crate::tools::register_builtin_tools(&mut agent, policy);

    let new_messages = agent
        .prompt("write file")
        .await
        .expect("prompt should succeed");
    let tool_message = new_messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("tool result should be present");

    assert!(tool_message.is_error);
    assert!(tool_message.text_content().contains("content is too large"));
}

#[test]
fn branch_and_resume_commands_reload_agent_messages() {
    let temp = tempdir().expect("tempdir");
    let path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
                tau_ai::Message::user("q2"),
                tau_ai::Message::assistant_text("a2"),
            ],
        )
        .expect("append")
        .expect("head id");

    let branch_target = head - 2;

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
        &format!("  /branch    {branch_target}   "),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("branch command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(
        runtime.as_ref().and_then(|runtime| runtime.active_head),
        Some(branch_target)
    );
    assert_eq!(agent.messages().len(), 3);

    let action = handle_command("/resume", &mut agent, &mut runtime, &tool_policy_json)
        .expect("resume command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(
        runtime.as_ref().and_then(|runtime| runtime.active_head),
        Some(head)
    );
    assert_eq!(agent.messages().len(), 5);
}

#[test]
fn exit_commands_return_exit_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    assert_eq!(
        handle_command("/quit", &mut agent, &mut runtime, &tool_policy_json)
            .expect("quit should succeed"),
        CommandAction::Exit
    );
    assert_eq!(
        handle_command("/exit", &mut agent, &mut runtime, &tool_policy_json)
            .expect("exit should succeed"),
        CommandAction::Exit
    );
}

#[test]
fn policy_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command("/policy", &mut agent, &mut runtime, &tool_policy_json)
        .expect("policy should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn approvals_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/approvals list",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("approvals should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn canvas_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        "/canvas create architecture",
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("canvas should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn rbac_command_returns_continue_action() {
    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = None;
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command("/rbac whoami", &mut agent, &mut runtime, &tool_policy_json)
        .expect("rbac should succeed");
    assert_eq!(action, CommandAction::Continue);
}

#[test]
fn functional_session_export_command_writes_active_lineage_snapshot() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let export_path = temp.path().join("snapshot.jsonl");

    let mut store = SessionStore::load(&session_path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    let head = store
        .append_messages(
            head,
            &[
                tau_ai::Message::user("q1"),
                tau_ai::Message::assistant_text("a1"),
            ],
        )
        .expect("append")
        .expect("head");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(head),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        &format!("/session-export {}", export_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("session export should succeed");
    assert_eq!(action, CommandAction::Continue);

    let exported = SessionStore::load(&export_path).expect("load exported");
    assert_eq!(exported.entries().len(), 3);
    assert_eq!(exported.entries()[0].message.text_content(), "sys");
    assert_eq!(exported.entries()[1].message.text_content(), "q1");
    assert_eq!(exported.entries()[2].message.text_content(), "a1");
}

#[test]
fn functional_session_import_command_merges_snapshot_and_updates_active_head() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");
    let import_path = temp.path().join("import.jsonl");

    let mut target_store = SessionStore::load(&session_path).expect("load target");
    let target_head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target root")
        .expect("target head");
    target_store
        .append_messages(Some(target_head), &[tau_ai::Message::user("target-user")])
        .expect("append target user");

    let mut import_store = SessionStore::load(&import_path).expect("load import");
    let import_head = import_store
        .append_messages(None, &[tau_ai::Message::system("import-root")])
        .expect("append import root");
    import_store
        .append_messages(import_head, &[tau_ai::Message::user("import-user")])
        .expect("append import user");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = target_store
        .lineage_messages(target_store.head_id())
        .expect("target lineage");
    agent.replace_messages(target_lineage);

    let mut runtime = Some(SessionRuntime {
        store: target_store,
        active_head: Some(2),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        &format!("/session-import {}", import_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("session import should succeed");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 4);
    assert_eq!(runtime.active_head, Some(4));
    assert_eq!(runtime.store.entries()[2].id, 3);
    assert_eq!(runtime.store.entries()[2].parent_id, None);
    assert_eq!(runtime.store.entries()[3].id, 4);
    assert_eq!(runtime.store.entries()[3].parent_id, Some(3));
    assert_eq!(agent.messages().len(), 2);
    assert_eq!(agent.messages()[0].text_content(), "import-root");
    assert_eq!(agent.messages()[1].text_content(), "import-user");
}

#[test]
fn integration_session_merge_command_appends_branch_and_reloads_agent_messages() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-merge.jsonl");

    let mut store = SessionStore::load(&session_path).expect("load");
    let root = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append root")
        .expect("root");
    let target = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("target-u1"),
                tau_ai::Message::assistant_text("target-a1"),
            ],
        )
        .expect("append target")
        .expect("target");
    let source = store
        .append_messages(
            Some(root),
            &[
                tau_ai::Message::user("source-u1"),
                tau_ai::Message::assistant_text("source-a1"),
            ],
        )
        .expect("append source")
        .expect("source");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = store
        .lineage_messages(Some(target))
        .expect("target lineage should resolve");
    agent.replace_messages(target_lineage);

    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(target),
    });
    let tool_policy_json = test_tool_policy_json();

    let action = handle_command(
        &format!("/session-merge {source} {target} --strategy append"),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect("session merge should succeed");
    assert_eq!(action, CommandAction::Continue);

    let runtime = runtime.expect("runtime should remain available");
    assert!(runtime.active_head.expect("active head should exist") > target);
    assert_eq!(
        agent
            .messages()
            .last()
            .expect("merged lineage tail should exist")
            .text_content(),
        "source-a1"
    );
}

#[test]
fn integration_session_import_command_replace_mode_overwrites_runtime_state() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-replace.jsonl");
    let import_path = temp.path().join("import-replace.jsonl");

    let mut target_store = SessionStore::load(&session_path).expect("load target");
    let head = target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target root");
    target_store
        .append_messages(head, &[tau_ai::Message::user("target-user")])
        .expect("append target user");

    let import_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":10,"parent_id":null,"message":tau_ai::Message::system("import-root")}).to_string(),
            serde_json::json!({"record_type":"entry","id":11,"parent_id":10,"message":tau_ai::Message::assistant_text("import-assistant")}).to_string(),
        ]
        .join("\n");
    std::fs::write(&import_path, format!("{import_raw}\n")).expect("write import snapshot");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = target_store
        .lineage_messages(target_store.head_id())
        .expect("target lineage");
    agent.replace_messages(target_lineage);

    let mut runtime = Some(SessionRuntime {
        store: target_store,
        active_head: Some(2),
    });
    let tool_policy_json = test_tool_policy_json();
    let profile_defaults = test_profile_defaults();
    let skills_dir = PathBuf::from(".tau/skills");
    let skills_lock_path = default_skills_lock_path(&skills_dir);
    let skills_command_config = skills_command_config(&skills_dir, &skills_lock_path, None);

    let action = handle_command_with_session_import_mode(
        &format!("/session-import {}", import_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
        SessionImportMode::Replace,
        &profile_defaults,
        &skills_command_config,
        &test_auth_command_config(),
        &ModelCatalog::built_in(),
        &[],
    )
    .expect("session replace import should succeed");
    assert_eq!(action, CommandAction::Continue);

    let mut runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 2);
    assert_eq!(runtime.store.entries()[0].id, 10);
    assert_eq!(runtime.store.entries()[1].id, 11);
    assert_eq!(runtime.active_head, Some(11));
    assert_eq!(agent.messages().len(), 2);
    assert_eq!(agent.messages()[0].text_content(), "import-root");
    assert_eq!(agent.messages()[1].text_content(), "import-assistant");

    let next = runtime
        .store
        .append_messages(
            runtime.active_head,
            &[tau_ai::Message::user("after-replace")],
        )
        .expect("append after replace");
    assert_eq!(next, Some(12));
}

#[test]
fn regression_session_import_command_rejects_invalid_snapshot() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session-invalid.jsonl");
    let import_path = temp.path().join("import-invalid.jsonl");

    let mut target_store = SessionStore::load(&session_path).expect("load target");
    target_store
        .append_messages(None, &[tau_ai::Message::system("target-root")])
        .expect("append target");
    let import_raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("cycle-a")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("cycle-b")}).to_string(),
        ]
        .join("\n");
    std::fs::write(&import_path, format!("{import_raw}\n")).expect("write invalid import");

    let mut agent = Agent::new(Arc::new(NoopClient), AgentConfig::default());
    let target_lineage = target_store
        .lineage_messages(target_store.head_id())
        .expect("target lineage");
    agent.replace_messages(target_lineage.clone());

    let mut runtime = Some(SessionRuntime {
        store: target_store,
        active_head: Some(1),
    });
    let tool_policy_json = test_tool_policy_json();

    let error = handle_command(
        &format!("/session-import {}", import_path.display()),
        &mut agent,
        &mut runtime,
        &tool_policy_json,
    )
    .expect_err("invalid import should fail");
    assert!(error
        .to_string()
        .contains("import session validation failed"));

    let runtime = runtime.expect("runtime");
    assert_eq!(runtime.store.entries().len(), 1);
    assert_eq!(runtime.active_head, Some(1));
    assert_eq!(agent.messages().len(), target_lineage.len());
    assert_eq!(agent.messages()[0].text_content(), "target-root");
}

#[test]
fn functional_validate_session_file_succeeds_for_valid_session() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");

    let mut store = SessionStore::load(&session_path).expect("load");
    let head = store
        .append_messages(None, &[tau_ai::Message::system("sys")])
        .expect("append");
    store
        .append_messages(head, &[tau_ai::Message::user("hello")])
        .expect("append");

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_validate = true;

    validate_session_file(&cli.session, cli.no_session).expect("session validation should pass");
}

#[test]
fn regression_validate_session_file_fails_for_invalid_session_graph() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("session.jsonl");

    let raw = [
            serde_json::json!({"record_type":"meta","schema_version":1}).to_string(),
            serde_json::json!({"record_type":"entry","id":1,"parent_id":2,"message":tau_ai::Message::system("sys")}).to_string(),
            serde_json::json!({"record_type":"entry","id":2,"parent_id":1,"message":tau_ai::Message::user("cycle")}).to_string(),
        ]
        .join("\n");
    std::fs::write(&session_path, format!("{raw}\n")).expect("write invalid session");

    let mut cli = test_cli();
    cli.session = session_path;
    cli.session_validate = true;

    let error = validate_session_file(&cli.session, cli.no_session)
        .expect_err("session validation should fail for cycle");
    assert!(error.to_string().contains("session validation failed"));
    assert!(error.to_string().contains("cycles=2"));
}

#[test]
fn regression_validate_session_file_rejects_no_session_flag() {
    let mut cli = test_cli();
    cli.no_session = true;
    cli.session_validate = true;

    let error = validate_session_file(&cli.session, cli.no_session)
        .expect_err("validation with no-session flag should fail fast");
    assert!(error
        .to_string()
        .contains("--session-validate cannot be used together with --no-session"));
}

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
    assert!(policy.os_sandbox_command.is_empty());
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
    assert_eq!(payload["schema_version"], 2);
    assert_eq!(payload["preset"], "balanced");
    assert_eq!(payload["bash_profile"], "strict");
    assert_eq!(payload["os_sandbox_mode"], "auto");
    assert_eq!(payload["max_file_write_bytes"], 4096);
    assert_eq!(payload["enforce_regular_files"], true);
    assert_eq!(payload["bash_dry_run"], false);
    assert_eq!(payload["tool_policy_trace"], false);
    assert_eq!(
        payload["extension_policy_override_root"],
        "/tmp/policy-overrides"
    );
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
