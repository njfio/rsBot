use super::*;

#[test]
fn regression_github_issues_bridge_requires_token() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--github-issues-bridge",
        "--github-repo",
        "owner/repo",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "--github-token (or --github-token-id) is required",
    ));
}

#[test]
fn command_file_flag_executes_slash_commands_and_prints_summary() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    fs::write(&command_file, "/help session\n").expect("write command file");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
        "--command-file",
        command_file.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("command: /session"))
        .stdout(predicate::str::contains("usage: /session"))
        .stdout(predicate::str::contains("command file summary: path="))
        .stdout(predicate::str::contains("mode=fail-fast"))
        .stdout(predicate::str::contains("total=1"))
        .stdout(predicate::str::contains("succeeded=1"))
        .stdout(predicate::str::contains("failed=0"));
}

#[test]
fn integration_command_file_continue_on_error_executes_remaining_commands() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    fs::write(&command_file, "/session\nnot-command\n/help session\n").expect("write command file");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
        "--command-file",
        command_file.to_str().expect("utf8 path"),
        "--command-file-error-mode",
        "continue-on-error",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("session: disabled"))
        .stdout(predicate::str::contains("command file error: path="))
        .stdout(predicate::str::contains("command must start with '/'"))
        .stdout(predicate::str::contains("command: /session"))
        .stdout(predicate::str::contains("mode=continue-on-error"))
        .stdout(predicate::str::contains("total=3"))
        .stdout(predicate::str::contains("executed=3"))
        .stdout(predicate::str::contains("succeeded=2"))
        .stdout(predicate::str::contains("failed=1"))
        .stdout(predicate::str::contains("halted_early=false"));
}

#[test]
fn regression_command_file_fail_fast_stops_on_malformed_line_and_exits_failure() {
    let temp = tempdir().expect("tempdir");
    let command_file = temp.path().join("commands.txt");
    fs::write(&command_file, "/session\nnot-command\n/help session\n").expect("write command file");

    let mut cmd = binary_command();
    cmd.current_dir(temp.path()).args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
        "--command-file",
        command_file.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("session: disabled"))
        .stdout(predicate::str::contains("command file error: path="))
        .stdout(predicate::str::contains("command must start with '/'"))
        .stdout(predicate::str::contains("mode=fail-fast"))
        .stdout(predicate::str::contains("executed=2"))
        .stdout(predicate::str::contains("failed=1"))
        .stdout(predicate::str::contains("halted_early=true"))
        .stderr(predicate::str::contains("command file execution failed"));
}

#[test]
fn rpc_capabilities_flag_outputs_versioned_json_and_exits() {
    let mut cmd = binary_command();
    cmd.arg("--rpc-capabilities");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\": 1"))
        .stdout(predicate::str::contains("\"protocol_version\": \"0.1.0\""))
        .stdout(predicate::str::contains("\"run.cancel\""))
        .stdout(predicate::str::contains("\"run.complete\""))
        .stdout(predicate::str::contains("\"run.fail\""))
        .stdout(predicate::str::contains("\"run.status\""))
        .stdout(predicate::str::contains("\"run.timeout\""));
}

#[test]
fn regression_rpc_capabilities_flag_takes_preflight_precedence_over_prompt() {
    let mut cmd = binary_command();
    cmd.args(["--rpc-capabilities", "--prompt", "ignored prompt"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"schema_version\": 1"))
        .stderr(predicate::str::contains("OPENAI_API_KEY").not());
}

#[test]
fn rpc_validate_frame_file_flag_reports_summary_and_exits() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-run",
  "kind": "run.start",
  "payload": {"prompt":"hello"}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-validate-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("rpc frame validate:"))
        .stdout(predicate::str::contains("request_id=req-run"))
        .stdout(predicate::str::contains("kind=run.start"))
        .stdout(predicate::str::contains("payload_keys=1"));
}

#[test]
fn regression_rpc_validate_frame_file_flag_rejects_invalid_kind() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-invalid",
  "kind": "run.unknown",
  "payload": {}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-validate-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unsupported rpc frame kind"));
}

#[test]
fn rpc_dispatch_frame_file_flag_outputs_capabilities_response() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-cap",
  "kind": "capabilities.request",
  "payload": {}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"request_id\": \"req-cap\""))
        .stdout(predicate::str::contains(
            "\"kind\": \"capabilities.response\"",
        ))
        .stdout(predicate::str::contains("\"protocol_version\": \"0.1.0\""))
        .stdout(predicate::str::contains(
            "\"negotiated_request_schema_version\": 1",
        ))
        .stdout(predicate::str::contains("\"contracts\": {"))
        .stdout(predicate::str::contains("\"status_values\": ["))
        .stdout(predicate::str::contains("\"terminal_states\": ["))
        .stdout(predicate::str::contains(
            "\"terminal_state_field_present_for_terminal_status\": true",
        ))
        .stdout(predicate::str::contains("\"request_kinds\": ["))
        .stdout(predicate::str::contains("\"response_kinds\": ["))
        .stdout(predicate::str::contains("\"stream_event_kinds\": ["))
        .stdout(predicate::str::contains("\"code\": \"invalid_payload\""))
        .stdout(predicate::str::contains("\"code\": \"unsupported_schema\""));
}

#[test]
fn rpc_dispatch_frame_file_flag_negotiates_requested_capabilities_schema_version() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-cap-negotiated",
  "kind": "capabilities.request",
  "payload": {"request_schema_version":0}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "\"request_id\": \"req-cap-negotiated\"",
        ))
        .stdout(predicate::str::contains(
            "\"negotiated_request_schema_version\": 0",
        ));
}

#[test]
fn regression_rpc_dispatch_frame_file_rejects_unsupported_capabilities_schema_request() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-cap-invalid",
  "kind": "capabilities.request",
  "payload": {"request_schema_version":99}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("\"code\": \"invalid_payload\""))
        .stderr(predicate::str::contains("request_schema_version"));
}

#[test]
fn rpc_dispatch_frame_file_flag_outputs_run_accepted_response() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {"prompt":"hello"}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"run.accepted\""))
        .stdout(predicate::str::contains("\"prompt_chars\": 5"));
}

#[test]
fn rpc_dispatch_frame_file_flag_outputs_run_status_response() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-status",
  "kind": "run.status",
  "payload": {"run_id":"run-123"}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"run.status\""))
        .stdout(predicate::str::contains("\"run_id\": \"run-123\""))
        .stdout(predicate::str::contains("\"active\": false"))
        .stdout(predicate::str::contains("\"known\": false"));
}

#[test]
fn rpc_dispatch_frame_file_flag_outputs_run_completed_response() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-complete",
  "kind": "run.complete",
  "payload": {"run_id":"run-123"}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"run.completed\""))
        .stdout(predicate::str::contains("\"run_id\": \"run-123\""))
        .stdout(predicate::str::contains("\"terminal\": true"))
        .stdout(predicate::str::contains(
            "\"terminal_state\": \"completed\"",
        ));
}

#[test]
fn rpc_dispatch_frame_file_flag_outputs_run_failed_response() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-fail",
  "kind": "run.fail",
  "payload": {"run_id":"run-123","reason":"tool timeout"}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"run.failed\""))
        .stdout(predicate::str::contains("\"run_id\": \"run-123\""))
        .stdout(predicate::str::contains("\"reason\": \"tool timeout\""))
        .stdout(predicate::str::contains("\"terminal\": true"))
        .stdout(predicate::str::contains("\"terminal_state\": \"failed\""));
}

#[test]
fn rpc_dispatch_frame_file_flag_outputs_run_timed_out_response() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-timeout",
  "kind": "run.timeout",
  "payload": {"run_id":"run-123","reason":"timeout reached"}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"kind\": \"run.timed_out\""))
        .stdout(predicate::str::contains("\"run_id\": \"run-123\""))
        .stdout(predicate::str::contains("\"reason\": \"timeout reached\""))
        .stdout(predicate::str::contains("\"terminal\": true"))
        .stdout(predicate::str::contains(
            "\"terminal_state\": \"timed_out\"",
        ));
}

#[test]
fn regression_rpc_dispatch_frame_file_flag_rejects_missing_prompt() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-start",
  "kind": "run.start",
  "payload": {}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("\"kind\": \"error\""))
        .stdout(predicate::str::contains("\"code\": \"invalid_payload\""))
        .stderr(predicate::str::contains(
            "requires non-empty payload field 'prompt'",
        ));
}

#[test]
fn regression_rpc_dispatch_frame_file_maps_unsupported_kind_to_error_code() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-unknown",
  "kind": "run.unknown",
  "payload": {}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("\"kind\": \"error\""))
        .stdout(predicate::str::contains("\"code\": \"unsupported_kind\""))
        .stderr(predicate::str::contains("unsupported rpc frame kind"));
}

#[test]
fn regression_rpc_dispatch_frame_file_takes_preflight_precedence_over_prompt() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frame.json");
    fs::write(
        &frame_path,
        r#"{
  "schema_version": 1,
  "request_id": "req-cap",
  "kind": "capabilities.request",
  "payload": {}
}"#,
    )
    .expect("write frame");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-frame-file",
        frame_path.to_str().expect("utf8 path"),
        "--prompt",
        "ignored",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "\"kind\": \"capabilities.response\"",
        ))
        .stderr(predicate::str::contains("OPENAI_API_KEY").not());
}

#[test]
fn rpc_dispatch_ndjson_file_flag_outputs_ordered_response_lines() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frames.ndjson");
    fs::write(
        &frame_path,
        r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
{"schema_version":1,"request_id":"req-cancel","kind":"run.cancel","payload":{"run_id":"run-1"}}
{"schema_version":1,"request_id":"req-status","kind":"run.status","payload":{"run_id":"run-1"}}
{"schema_version":1,"request_id":"req-fail","kind":"run.fail","payload":{"run_id":"run-1","reason":"failed in dispatch"}}
{"schema_version":1,"request_id":"req-timeout","kind":"run.timeout","payload":{"run_id":"run-1","reason":"timed out in dispatch"}}
"#,
    )
    .expect("write frames");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-ndjson-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"request_id\":\"req-cap\""))
        .stdout(predicate::str::contains(
            "\"kind\":\"capabilities.response\"",
        ))
        .stdout(predicate::str::contains("\"request_id\":\"req-cancel\""))
        .stdout(predicate::str::contains("\"kind\":\"run.cancelled\""))
        .stdout(predicate::str::contains("\"request_id\":\"req-status\""))
        .stdout(predicate::str::contains("\"kind\":\"run.status\""))
        .stdout(predicate::str::contains("\"active\":false"))
        .stdout(predicate::str::contains("\"request_id\":\"req-fail\""))
        .stdout(predicate::str::contains("\"kind\":\"run.failed\""))
        .stdout(predicate::str::contains(
            "\"reason\":\"failed in dispatch\"",
        ))
        .stdout(predicate::str::contains("\"request_id\":\"req-timeout\""))
        .stdout(predicate::str::contains("\"kind\":\"run.timed_out\""))
        .stdout(predicate::str::contains(
            "\"reason\":\"timed out in dispatch\"",
        ));
}

#[test]
fn integration_rpc_dispatch_ndjson_file_replays_schema_compat_fixture() {
    let fixture = load_rpc_schema_compat_fixture("dispatch-mixed-supported.json");
    assert_eq!(fixture.name, "dispatch-mixed-supported");
    assert_eq!(fixture.mode, RpcSchemaCompatMode::DispatchNdjson);
    assert_eq!(fixture.expected_processed_lines, fixture.input_lines.len());
    assert_eq!(fixture.expected_error_count, 0);

    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("schema-compat-dispatch.ndjson");
    fs::write(&frame_path, fixture.input_lines.join("\n")).expect("write fixture input");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-ndjson-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    let assert = cmd.assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout utf8");
    let responses = parse_ndjson_values(&stdout);
    assert_eq!(responses, fixture.expected_responses);
}

#[test]
fn regression_rpc_dispatch_ndjson_file_schema_fixture_keeps_processing_after_unsupported_schema() {
    let fixture = load_rpc_schema_compat_fixture("dispatch-unsupported-continues.json");
    assert_eq!(fixture.name, "dispatch-unsupported-continues");
    assert_eq!(fixture.mode, RpcSchemaCompatMode::DispatchNdjson);
    assert_eq!(fixture.expected_processed_lines, fixture.input_lines.len());
    assert_eq!(fixture.expected_error_count, 1);

    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("schema-compat-dispatch-regression.ndjson");
    fs::write(&frame_path, fixture.input_lines.join("\n")).expect("write fixture input");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-ndjson-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    let assert = cmd.assert().failure().stderr(predicate::str::contains(
        "rpc ndjson dispatch completed with 1 error frame(s)",
    ));
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout utf8");
    let responses = parse_ndjson_values(&stdout);
    assert_eq!(responses, fixture.expected_responses);
}

#[test]
fn regression_rpc_dispatch_ndjson_file_continues_after_error_and_exits_failure() {
    let temp = tempdir().expect("tempdir");
    let frame_path = temp.path().join("frames.ndjson");
    fs::write(
        &frame_path,
        r#"{"schema_version":1,"request_id":"req-ok","kind":"run.cancel","payload":{"run_id":"run-1"}}
not-json
{"schema_version":1,"request_id":"req-ok-2","kind":"run.start","payload":{"prompt":"x"}}
"#,
    )
    .expect("write frames");

    let mut cmd = binary_command();
    cmd.args([
        "--rpc-dispatch-ndjson-file",
        frame_path.to_str().expect("utf8 path"),
    ]);

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("\"request_id\":\"req-ok\""))
        .stdout(predicate::str::contains("\"kind\":\"run.cancelled\""))
        .stdout(predicate::str::contains("\"kind\":\"error\""))
        .stdout(predicate::str::contains("\"code\":\"invalid_json\""))
        .stdout(predicate::str::contains("\"request_id\":\"req-ok-2\""))
        .stdout(predicate::str::contains("\"kind\":\"run.accepted\""))
        .stderr(predicate::str::contains(
            "rpc ndjson dispatch completed with 1 error frame(s)",
        ));
}

#[test]
fn rpc_serve_ndjson_flag_streams_ordered_response_lines() {
    let mut cmd = binary_command();
    cmd.arg("--rpc-serve-ndjson").write_stdin(
        r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}
{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-status-active","kind":"run.status","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-cancel","kind":"run.cancel","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-status-inactive","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#,
    );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"request_id\":\"req-cap\""))
        .stdout(predicate::str::contains(
            "\"kind\":\"capabilities.response\"",
        ))
        .stdout(predicate::str::contains("\"request_id\":\"req-start\""))
        .stdout(predicate::str::contains("\"kind\":\"run.accepted\""))
        .stdout(predicate::str::contains(
            "\"kind\":\"run.stream.tool_events\"",
        ))
        .stdout(predicate::str::contains(
            "\"kind\":\"run.stream.assistant_text\"",
        ))
        .stdout(predicate::str::contains(
            "\"request_id\":\"req-status-active\"",
        ))
        .stdout(predicate::str::contains("\"kind\":\"run.status\""))
        .stdout(predicate::str::contains("\"active\":true"))
        .stdout(predicate::str::contains("\"request_id\":\"req-cancel\""))
        .stdout(predicate::str::contains("\"kind\":\"run.cancelled\""))
        .stdout(predicate::str::contains("\"event\":\"run.cancelled\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"cancelled\""))
        .stdout(predicate::str::contains(
            "\"request_id\":\"req-status-inactive\"",
        ))
        .stdout(predicate::str::contains("\"active\":false"))
        .stdout(predicate::str::contains("\"known\":true"))
        .stdout(predicate::str::contains("\"status\":\"cancelled\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"cancelled\""));
}

#[test]
fn rpc_serve_ndjson_flag_supports_run_complete_lifecycle() {
    let mut cmd = binary_command();
    cmd.arg("--rpc-serve-ndjson").write_stdin(
        r#"{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-complete","kind":"run.complete","payload":{"run_id":"run-req-start"}}
{"schema_version":1,"request_id":"req-status","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#,
    );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"request_id\":\"req-complete\""))
        .stdout(predicate::str::contains("\"kind\":\"run.completed\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"completed\""))
        .stdout(predicate::str::contains("\"event\":\"run.completed\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"completed\""))
        .stdout(predicate::str::contains("\"request_id\":\"req-status\""))
        .stdout(predicate::str::contains("\"active\":false"))
        .stdout(predicate::str::contains("\"known\":true"))
        .stdout(predicate::str::contains("\"status\":\"completed\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"completed\""));
}

#[test]
fn rpc_serve_ndjson_flag_supports_run_fail_lifecycle() {
    let mut cmd = binary_command();
    cmd.arg("--rpc-serve-ndjson").write_stdin(
        r#"{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-fail","kind":"run.fail","payload":{"run_id":"run-req-start","reason":"provider timeout"}}
{"schema_version":1,"request_id":"req-status","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#,
    );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"request_id\":\"req-fail\""))
        .stdout(predicate::str::contains("\"kind\":\"run.failed\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"failed\""))
        .stdout(predicate::str::contains("\"event\":\"run.failed\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"failed\""))
        .stdout(predicate::str::contains("\"reason\":\"provider timeout\""))
        .stdout(predicate::str::contains("\"request_id\":\"req-status\""))
        .stdout(predicate::str::contains("\"active\":false"))
        .stdout(predicate::str::contains("\"known\":true"))
        .stdout(predicate::str::contains("\"status\":\"failed\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"failed\""))
        .stdout(predicate::str::contains("\"reason\":\"provider timeout\""));
}

#[test]
fn rpc_serve_ndjson_flag_supports_run_timeout_lifecycle() {
    let mut cmd = binary_command();
    cmd.arg("--rpc-serve-ndjson").write_stdin(
        r#"{"schema_version":1,"request_id":"req-start","kind":"run.start","payload":{"prompt":"hello"}}
{"schema_version":1,"request_id":"req-timeout","kind":"run.timeout","payload":{"run_id":"run-req-start","reason":"deadline exceeded"}}
{"schema_version":1,"request_id":"req-status","kind":"run.status","payload":{"run_id":"run-req-start"}}
"#,
    );

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"request_id\":\"req-timeout\""))
        .stdout(predicate::str::contains("\"kind\":\"run.timed_out\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"timed_out\""))
        .stdout(predicate::str::contains("\"event\":\"run.timed_out\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"timed_out\""))
        .stdout(predicate::str::contains("\"reason\":\"deadline exceeded\""))
        .stdout(predicate::str::contains("\"request_id\":\"req-status\""))
        .stdout(predicate::str::contains("\"active\":false"))
        .stdout(predicate::str::contains("\"known\":true"))
        .stdout(predicate::str::contains("\"status\":\"timed_out\""))
        .stdout(predicate::str::contains("\"terminal_state\":\"timed_out\""))
        .stdout(predicate::str::contains("\"reason\":\"deadline exceeded\""));
}

#[test]
fn integration_rpc_serve_ndjson_replays_schema_compat_fixture() {
    for fixture_name in ["serve-mixed-supported.json", "serve-cancel-supported.json"] {
        let fixture = load_rpc_schema_compat_fixture(fixture_name);
        assert_eq!(fixture.mode, RpcSchemaCompatMode::ServeNdjson);
        assert_eq!(fixture.expected_processed_lines, fixture.input_lines.len());
        assert_eq!(fixture.expected_error_count, 0);

        let mut cmd = binary_command();
        cmd.arg("--rpc-serve-ndjson")
            .write_stdin(fixture.input_lines.join("\n"));

        let assert = cmd.assert().success();
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout utf8");
        let responses = parse_ndjson_values(&stdout);
        assert_eq!(responses, fixture.expected_responses);
    }
}

#[test]
fn regression_rpc_serve_ndjson_schema_fixture_keeps_processing_after_unsupported_schema() {
    let fixture = load_rpc_schema_compat_fixture("serve-unsupported-continues.json");
    assert_eq!(fixture.name, "serve-unsupported-continues");
    assert_eq!(fixture.mode, RpcSchemaCompatMode::ServeNdjson);
    assert_eq!(fixture.expected_processed_lines, fixture.input_lines.len());
    assert_eq!(fixture.expected_error_count, 1);

    let mut cmd = binary_command();
    cmd.arg("--rpc-serve-ndjson")
        .write_stdin(fixture.input_lines.join("\n"));

    let assert = cmd.assert().failure().stderr(predicate::str::contains(
        "rpc ndjson serve completed with 1 error frame(s)",
    ));
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout utf8");
    let responses = parse_ndjson_values(&stdout);
    assert_eq!(responses, fixture.expected_responses);
}

#[test]
fn regression_rpc_serve_ndjson_continues_after_error_and_exits_failure() {
    let mut cmd = binary_command();
    cmd.arg("--rpc-serve-ndjson").write_stdin(
        r#"{"schema_version":1,"request_id":"req-ok","kind":"run.cancel","payload":{"run_id":"run-missing"}}
not-json
{"schema_version":1,"request_id":"req-ok-2","kind":"run.start","payload":{"prompt":"x"}}
"#,
    );

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("\"request_id\":\"req-ok\""))
        .stdout(predicate::str::contains("\"kind\":\"error\""))
        .stdout(predicate::str::contains("\"code\":\"invalid_payload\""))
        .stdout(predicate::str::contains("\"kind\":\"error\""))
        .stdout(predicate::str::contains("\"code\":\"invalid_json\""))
        .stdout(predicate::str::contains("\"request_id\":\"req-ok-2\""))
        .stdout(predicate::str::contains("\"kind\":\"run.accepted\""))
        .stderr(predicate::str::contains(
            "rpc ndjson serve completed with 2 error frame(s)",
        ));
}

#[test]
fn regression_rpc_serve_ndjson_takes_preflight_precedence_over_prompt() {
    let mut cmd = binary_command();
    cmd.args(["--rpc-serve-ndjson", "--prompt", "ignored prompt"])
        .write_stdin(r#"{"schema_version":1,"request_id":"req-cap","kind":"capabilities.request","payload":{}}"#);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "\"kind\":\"capabilities.response\"",
        ))
        .stderr(predicate::str::contains("OPENAI_API_KEY").not());
}
