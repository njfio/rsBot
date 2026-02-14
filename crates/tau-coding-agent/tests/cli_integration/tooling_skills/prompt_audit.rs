//! CLI integration coverage for prompt/system/audit telemetry flows.

use super::*;

#[test]
fn prompt_file_flag_runs_one_shot_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("prompt from file");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "file prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("prompt.txt");
    fs::write(&prompt_path, "prompt from file").expect("write prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("file prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn prompt_template_file_flag_renders_and_runs_one_shot_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("Summarize src/main.rs with focus on retries.");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "template prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    fs::write(
        &template_path,
        "Summarize {{module}} with focus on {{focus}}.",
    )
    .expect("write template");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-template-file",
        template_path.to_str().expect("utf8 path"),
        "--prompt-template-var",
        "module=src/main.rs",
        "--prompt-template-var",
        "focus=retries",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("template prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn prompt_file_dash_reads_prompt_from_stdin() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("prompt from stdin");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "stdin prompt ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        "-",
        "--no-session",
    ])
    .write_stdin("prompt from stdin");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("stdin prompt ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_prompt_file_dash_rejects_empty_stdin() {
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        "-",
        "--no-session",
    ])
    .write_stdin(" \n\t");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("stdin prompt"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn regression_empty_prompt_file_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let prompt_path = temp.path().join("empty-prompt.txt");
    fs::write(&prompt_path, " \n\t").expect("write prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-file",
        prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("prompt file"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn regression_prompt_template_file_missing_variable_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    fs::write(&template_path, "Summarize {{path}} and {{goal}}").expect("write template");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-template-file",
        template_path.to_str().expect("utf8 path"),
        "--prompt-template-var",
        "path=src/lib.rs",
        "--no-session",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "missing a --prompt-template-var value",
    ));
}

#[test]
fn regression_prompt_template_var_requires_key_value_shape() {
    let temp = tempdir().expect("tempdir");
    let template_path = temp.path().join("prompt-template.txt");
    fs::write(&template_path, "Summarize {{path}}").expect("write template");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt-template-file",
        template_path.to_str().expect("utf8 path"),
        "--prompt-template-var",
        "path",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid --prompt-template-var"));
}

#[test]
fn system_prompt_file_flag_overrides_inline_system_prompt() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("system prompt from file")
            .body_excludes(
                "You are a focused coding assistant. Prefer concrete steps and safe edits.",
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "system prompt file ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let system_prompt_path = temp.path().join("system-prompt.txt");
    fs::write(&system_prompt_path, "system prompt from file").expect("write system prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt-file",
        system_prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("system prompt file ok"));

    openai.assert_calls(1);
}

#[test]
fn regression_empty_system_prompt_file_fails_fast() {
    let temp = tempdir().expect("tempdir");
    let system_prompt_path = temp.path().join("empty-system-prompt.txt");
    fs::write(&system_prompt_path, "  \n\t").expect("write system prompt");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt-file",
        system_prompt_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("system prompt file"))
        .stderr(predicate::str::contains("is empty"));
}

#[test]
fn tool_audit_log_flag_creates_audit_log_file() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "audit log ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let audit_path = temp.path().join("tool-audit.jsonl");
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--tool-audit-log",
        audit_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit log ok"));

    assert!(audit_path.exists());
    openai.assert_calls(1);
}

#[test]
fn telemetry_log_flag_creates_prompt_telemetry_record() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "telemetry log ok"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let telemetry_path = temp.path().join("prompt-telemetry.jsonl");
    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--telemetry-log",
        telemetry_path.to_str().expect("utf8 path"),
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("telemetry log ok"));

    assert!(telemetry_path.exists());
    let raw = fs::read_to_string(&telemetry_path).expect("read telemetry log");
    let lines = raw.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let record: serde_json::Value = serde_json::from_str(lines[0]).expect("parse telemetry record");
    assert_eq!(record["record_type"], "prompt_telemetry_v1");
    assert_eq!(record["provider"], "openai");
    assert_eq!(record["model"], "gpt-4o-mini");
    assert_eq!(record["status"], "completed");
    assert_eq!(record["success"], true);
    assert_eq!(record["token_usage"]["total_tokens"], 6);
    assert_eq!(record["redaction_policy"]["prompt_content"], "omitted");
    openai.assert_calls(1);
}

#[test]
fn interactive_audit_summary_command_reports_aggregates() {
    let temp = tempdir().expect("tempdir");
    let audit_path = temp.path().join("audit.jsonl");
    let rows = [
        json!({
            "event": "tool_execution_end",
            "tool_name": "bash",
            "duration_ms": 25,
            "is_error": false
        }),
        json!({
            "record_type": "prompt_telemetry_v1",
            "provider": "openai",
            "status": "completed",
            "success": true,
            "duration_ms": 90,
            "token_usage": {
                "input_tokens": 3,
                "output_tokens": 1,
                "total_tokens": 4
            }
        }),
    ]
    .iter()
    .map(serde_json::Value::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(&audit_path, format!("{rows}\n")).expect("write audit file");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin(format!(
        "/audit-summary {}\n/quit\n",
        audit_path.to_str().expect("utf8 path")
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit summary: path="))
        .stdout(predicate::str::contains("tool_breakdown:"))
        .stdout(predicate::str::contains("provider_breakdown:"))
        .stdout(predicate::str::contains("bash count=1"))
        .stdout(predicate::str::contains("openai count=1"));
}

#[test]
fn regression_audit_summary_command_handles_missing_file_without_exiting() {
    let temp = tempdir().expect("tempdir");
    let missing_path = temp.path().join("missing.jsonl");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--no-session",
    ])
    .write_stdin(format!(
        "/audit-summary {}\n/quit\n",
        missing_path.to_str().expect("utf8 path")
    ));

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("audit summary error:"))
        .stdout(predicate::str::contains("failed to open audit file"));
}
