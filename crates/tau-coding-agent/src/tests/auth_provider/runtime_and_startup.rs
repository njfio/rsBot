//! Runtime/startup auth-provider tests for dispatch, policy guards, and startup preflight behavior.

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
    CommandAction, CommandExecutionContext, ContentBlock, Duration, HashMap, Instant, Message,
    MessageRole, ModelCatalog, MultiAgentRouteTable, NoopClient, OsSandboxMode, Path, PathBuf,
    PlanFirstPromptPolicyRequest, PlanFirstPromptRequest, PlanFirstPromptRoutingRequest,
    PromptRunStatus, PromptTelemetryLogger, QueueClient, RenderOptions,
    RuntimeExtensionHooksConfig, SequenceClient, SessionImportMode, SessionRuntime, SessionStore,
    SlowClient, SuccessClient, TauAiError, ToolAuditLogger, ToolExecutionResult, ToolPolicyPreset,
    TrustedRootRecord, VecDeque, AUTH_ENV_TEST_LOCK,
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
    let path = PathBuf::from(".tau/sessions/default.sqlite");
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
                cached_input_tokens: 0,
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
                cached_input_tokens: 0,
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
async fn functional_run_prompt_with_cancellation_records_session_usage_and_cost() {
    let temp = tempdir().expect("tempdir");
    let session_path = temp.path().join("usage-session.jsonl");
    let mut store = SessionStore::load(&session_path).expect("load session");
    let active_head = store
        .append_messages(None, &[Message::system("sys")])
        .expect("append system")
        .expect("system head");
    let mut runtime = Some(SessionRuntime {
        store,
        active_head: Some(active_head),
    });

    let responses = VecDeque::from(vec![ChatResponse {
        message: Message::assistant_text("usage recorded"),
        finish_reason: Some("stop".to_string()),
        usage: ChatUsage {
            input_tokens: 30,
            output_tokens: 10,
            total_tokens: 40,
            cached_input_tokens: 0,
        },
    }]);
    let mut agent = Agent::new(
        Arc::new(QueueClient {
            responses: AsyncMutex::new(responses),
        }),
        AgentConfig {
            model_input_cost_per_million: Some(10.0),
            model_cached_input_cost_per_million: None,
            model_output_cost_per_million: Some(20.0),
            ..AgentConfig::default()
        },
    );

    let status = run_prompt_with_cancellation(
        &mut agent,
        &mut runtime,
        "price this turn",
        0,
        pending::<()>(),
        test_render_options(),
    )
    .await
    .expect("prompt should complete");
    assert_eq!(status, PromptRunStatus::Completed);

    let usage = runtime
        .as_ref()
        .expect("runtime should be present")
        .store
        .usage_summary();
    assert_eq!(usage.input_tokens, 30);
    assert_eq!(usage.output_tokens, 10);
    assert_eq!(usage.total_tokens, 40);
    assert!((usage.estimated_cost_usd - 0.0005).abs() < 1e-12);
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

    let registrations = discover_extension_runtime_registrations(
        &extension_root,
        crate::tools::builtin_agent_tool_names(),
        crate::commands::COMMAND_NAMES,
    );
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
    let registrations = discover_extension_runtime_registrations(
        &extension_root,
        crate::tools::builtin_agent_tool_names(),
        crate::commands::COMMAND_NAMES,
    );
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
        CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
            model_catalog: &ModelCatalog::built_in(),
            extension_commands: &registrations.registered_commands,
        },
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
    let registrations = discover_extension_runtime_registrations(
        &extension_root,
        crate::tools::builtin_agent_tool_names(),
        crate::commands::COMMAND_NAMES,
    );
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
        CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Merge,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &auth_command_config,
            model_catalog: &ModelCatalog::built_in(),
            extension_commands: &registrations.registered_commands,
        },
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
        PlanFirstPromptRoutingRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: true,
            delegated_policy_context: Some("preset=balanced;max_command_length=4096"),
            route_table: &route_table,
            route_trace_log_path: None,
        },
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
        PlanFirstPromptRoutingRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: false,
            delegated_policy_context: None,
            route_table: &route_table,
            route_trace_log_path: Some(telemetry_log.as_path()),
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: false,
        },
    )
    .await
    .expect("legacy run should succeed");
    run_plan_first_prompt_with_policy_context_and_routing(
        &mut routed_agent,
        &mut routed_runtime,
        PlanFirstPromptRoutingRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: false,
            delegated_policy_context: None,
            route_table: &MultiAgentRouteTable::default(),
            route_trace_log_path: None,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 2_048,
            delegate_steps: false,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: true,
        },
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
        PlanFirstPromptPolicyRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: true,
            delegated_policy_context: None,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: true,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 2,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: true,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 8,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: true,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 64,
            max_delegated_total_response_chars: 12,
            delegate_steps: true,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 2,
            max_delegated_steps: 2,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: false,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 512,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: false,
        },
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
        PlanFirstPromptRequest {
            user_prompt: "ship feature",
            turn_timeout_ms: 0,
            render_options: test_render_options(),
            max_plan_steps: 4,
            max_delegated_steps: 4,
            max_executor_response_chars: 8,
            max_delegated_step_response_chars: 512,
            max_delegated_total_response_chars: 1_024,
            delegate_steps: false,
        },
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
fn unit_register_builtin_tools_includes_tool_builder_when_enabled() {
    let temp = tempdir().expect("tempdir");
    let client = Arc::new(QueueClient {
        responses: AsyncMutex::new(VecDeque::new()),
    });
    let mut agent = Agent::new(client, AgentConfig::default());
    let mut policy = crate::tools::ToolPolicy::new(vec![temp.path().to_path_buf()]);
    policy.tool_builder_enabled = true;
    policy.tool_builder_output_root = temp.path().join(".tau/generated-tools");
    policy.tool_builder_extension_root = temp.path().join(".tau/extensions/generated");
    policy.tool_builder_max_attempts = 3;
    crate::tools::register_builtin_tools(&mut agent, policy);

    assert!(agent
        .registered_tool_names()
        .iter()
        .any(|name| name == "tool_builder"));
}

#[test]
fn branch_undo_redo_and_resume_commands_reload_agent_messages() {
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

    let action = handle_command("/undo", &mut agent, &mut runtime, &tool_policy_json)
        .expect("undo command should succeed");
    assert_eq!(action, CommandAction::Continue);
    assert_eq!(
        runtime.as_ref().and_then(|runtime| runtime.active_head),
        Some(head)
    );
    assert_eq!(agent.messages().len(), 5);

    let action = handle_command("/redo", &mut agent, &mut runtime, &tool_policy_json)
        .expect("redo command should succeed");
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
        CommandExecutionContext {
            tool_policy_json: &tool_policy_json,
            session_import_mode: SessionImportMode::Replace,
            profile_defaults: &profile_defaults,
            skills_command_config: &skills_command_config,
            auth_command_config: &test_auth_command_config(),
            model_catalog: &ModelCatalog::built_in(),
            extension_commands: &[],
        },
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

mod startup_preflight_and_policy;
